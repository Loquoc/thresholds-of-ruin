//! Luau host for scenario hooks.
//!
//! [`Program::compile`] validates package scripts and enumerates their
//! bindable handlers. A [`Runtime`] built from a program answers
//! [`tor_simulation::scripting::HookHost`] calls in a sandboxed VM.
//!
//! Determinism and isolation rules:
//!
//! * Only `string`, `table`, `math` (without `random`), `bit32`, `utf8` and
//!   `buffer` are available. There is no I/O, clock, `debug`, `load`, GC
//!   inspection or environment access. Globals are read-only.
//! * Every call instantiates its module afresh from cached bytecode, so a
//!   module-level local cannot carry state from one call to the next. State
//!   that must persist goes through `ctx:set_var` or a timer argument.
//! * A full collection runs before each call, so whether the memory limit is
//!   reached never depends on heap history (which differs after a restore).
//! * Work is counted at Luau interrupt points (calls and loop back-edges) plus
//!   fixed charges per query and effect. Exhaustion, the memory limit, errors
//!   and invalid requests all reject the root invocation.
//! * Identities reach scripts as strings such as `actor:12`, so they keep all
//!   64 bits and iterate deterministically as table keys.

mod context;
mod program;
mod values;

#[cfg(test)]
mod tests;

pub use program::{CompileError, Program, ScriptSource, MAX_SOURCE_BYTES};

use mlua::{Lua, LuaOptions, StdLib, VmState};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tor_simulation::scripting::{
    FailureReason, HandlerId, HookCall, HookHost, HookOutput, ScriptView, WorkBudget,
};
use tor_world::Location;

/// Runtime memory ceiling per VM.
pub const MEMORY_LIMIT: usize = 16 * 1024 * 1024;
/// Fixed charges, in interrupt-equivalent work units.
pub const QUERY_COST: u64 = 8;
pub const EFFECT_COST: u64 = 32;

/// Package facts a runtime may answer from without reading game state.
#[derive(Clone, Debug, Default)]
pub struct RuntimeConfig {
    /// Named anchors scripts may ask for.
    pub anchors: BTreeMap<String, Location>,
    /// Semantic control ids message placeholders may name.
    pub controls: BTreeSet<String>,
}

/// A sandboxed Luau VM bound to one validated program.
pub struct Runtime {
    inner: Mutex<Inner>,
    program: Arc<Program>,
    config: Arc<RuntimeConfig>,
}

struct Inner {
    lua: Lua,
    /// One loaded chunk per module, called afresh for every invocation.
    chunks: Vec<mlua::Function>,
    meter: Arc<Meter>,
}

/// Interrupt-time work meter shared with the VM callback.
#[derive(Default)]
struct Meter {
    remaining: AtomicU64,
    exhausted: AtomicBool,
}

impl Meter {
    fn charge(&self, units: u64) -> bool {
        let remaining = self.remaining.load(Ordering::Relaxed);
        if remaining < units {
            self.remaining.store(0, Ordering::Relaxed);
            self.exhausted.store(true, Ordering::Relaxed);
            false
        } else {
            self.remaining.store(remaining - units, Ordering::Relaxed);
            true
        }
    }
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime")
            .field("handlers", &self.program.handlers().len())
            .finish()
    }
}

pub(crate) fn sandbox() -> mlua::Result<Lua> {
    let lua = Lua::new_with(
        StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::BIT | StdLib::UTF8 | StdLib::BUFFER,
        LuaOptions::default(),
    )?;
    let globals = lua.globals();
    let math: mlua::Table = globals.get("math")?;
    math.set("random", mlua::Nil)?;
    math.set("randomseed", mlua::Nil)?;
    for name in [
        "print",
        "require",
        "loadstring",
        "getfenv",
        "setfenv",
        "collectgarbage",
        "gcinfo",
        "newproxy",
    ] {
        globals.set(name, mlua::Nil)?;
    }
    lua.sandbox(true)?;
    lua.set_memory_limit(MEMORY_LIMIT)?;
    Ok(lua)
}

/// A fresh read-only environment that reads the sandboxed libraries. Every
/// instantiation gets its own, so no global write can survive a call.
pub(crate) fn fresh_env(lua: &Lua) -> mlua::Result<mlua::Table> {
    let env = lua.create_table()?;
    let meta = lua.create_table()?;
    meta.set("__index", lua.globals())?;
    meta.set_readonly(true);
    env.set_metatable(Some(meta))?;
    env.set_readonly(true);
    Ok(env)
}

impl Runtime {
    /// Load a validated program into a fresh VM. Restores and replays build a
    /// new runtime the same way; nothing from an earlier VM survives.
    pub fn new(program: Arc<Program>, config: RuntimeConfig) -> Result<Self, CompileError> {
        let lua = sandbox().map_err(CompileError::runtime)?;
        let meter = Arc::new(Meter::default());
        let interrupt = meter.clone();
        lua.set_interrupt(move |_| {
            if interrupt.charge(1) {
                Ok(VmState::Continue)
            } else {
                Err(mlua::Error::runtime("work budget exhausted"))
            }
        });
        let chunks = program
            .modules()
            .iter()
            .map(|module| {
                lua.load(&module.bytecode[..])
                    .set_name(format!("={}", module.name))
                    .set_mode(mlua::chunk::ChunkMode::Binary)
                    .into_function()
            })
            .collect::<mlua::Result<Vec<_>>>()
            .map_err(CompileError::runtime)?;
        Ok(Self {
            inner: Mutex::new(Inner { lua, chunks, meter }),
            program,
            config: Arc::new(config),
        })
    }

    pub fn program(&self) -> &Arc<Program> {
        &self.program
    }
}

impl HookHost for Runtime {
    fn dispatch(
        &self,
        view: ScriptView<'_>,
        call: HookCall<'_>,
        budget: &mut WorkBudget,
    ) -> Result<HookOutput, FailureReason> {
        let (module, function) = self
            .program
            .handler(call.handler)
            .ok_or_else(|| FailureReason::Error("unknown handler".into()))?;
        let inner = self.inner.lock().unwrap_or_else(|poison| poison.into_inner());
        inner.meter.remaining.store(budget.remaining, Ordering::Relaxed);
        inner.meter.exhausted.store(false, Ordering::Relaxed);
        let result = context::invoke(
            &inner.lua,
            &inner.chunks[module],
            function,
            context::Call {
                view,
                facts: call.facts,
                program: &self.program,
                config: &self.config,
                meter: &inner.meter,
            },
        );
        budget.remaining = inner.meter.remaining.load(Ordering::Relaxed);
        let exhausted = inner.meter.exhausted.load(Ordering::Relaxed);
        drop(inner);
        match result {
            Ok(output) => Ok(output),
            Err(_) if exhausted => Err(FailureReason::Budget),
            Err(error) => Err(values::failure(error)),
        }
    }
}

/// The handler a manifest names as `module.function`.
pub fn handler_id(program: &Program, name: &str) -> Option<HandlerId> {
    program.handler_id(name)
}
