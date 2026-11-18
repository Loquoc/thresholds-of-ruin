//! Script validation and handler enumeration.

use mlua::chunk::{ChunkMode, Compiler};
use mlua::VmState;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tor_simulation::scripting::HandlerId;

/// Bytes per script source.
pub const MAX_SOURCE_BYTES: usize = 256 * 1024;
pub const MAX_MODULES: usize = 64;
pub const MAX_HANDLERS: usize = 1024;
/// Work units a module's top level may use while it is instantiated.
pub const MODULE_BUDGET: u64 = 100_000;

/// One package script: `name` is the module name handlers are qualified by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptSource {
    pub name: String,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileError {
    pub module: Option<String>,
    pub message: String,
}

impl CompileError {
    fn new(module: &str, message: impl Into<String>) -> Self {
        Self {
            module: Some(module.into()),
            message: message.into(),
        }
    }
    pub(crate) fn runtime(error: mlua::Error) -> Self {
        Self {
            module: None,
            message: error.to_string(),
        }
    }
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.module {
            Some(module) => write!(f, "script {module}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}
impl std::error::Error for CompileError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Module {
    pub name: String,
    pub bytecode: Vec<u8>,
}

/// Validated scripts and their bindable handlers. Handler identities are the
/// sorted order of `module.function` names, so they are stable for identical
/// sources on every platform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    modules: Vec<Module>,
    /// Sorted qualified names with their module index and function name.
    handlers: Vec<(String, usize, String)>,
}

fn module_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_lowercase())
        && name.len() <= 32
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

impl Program {
    /// Compile and instantiate every module once. Each must return a table
    /// whose entries are all functions with lowercase identifier names.
    pub fn compile(sources: &[ScriptSource]) -> Result<Arc<Program>, CompileError> {
        if sources.len() > MAX_MODULES {
            return Err(CompileError {
                module: None,
                message: format!("at most {MAX_MODULES} scripts"),
            });
        }
        let compiler = Compiler::new()
            .set_optimization_level(1)
            .set_debug_level(1);
        let lua = crate::sandbox().map_err(CompileError::runtime)?;
        let remaining = Arc::new(AtomicU64::new(0));
        let meter = remaining.clone();
        lua.set_interrupt(move |_| {
            let left = meter.load(Ordering::Relaxed);
            if left == 0 {
                return Err(mlua::Error::runtime("module top level exceeds its work budget"));
            }
            meter.store(left - 1, Ordering::Relaxed);
            Ok(VmState::Continue)
        });
        let mut modules = Vec::new();
        let mut handlers = Vec::new();
        for (index, source) in sources.iter().enumerate() {
            let name = &source.name;
            if !module_name(name) {
                return Err(CompileError::new(name, "invalid module name"));
            }
            if modules.iter().any(|m: &Module| &m.name == name) {
                return Err(CompileError::new(name, "duplicate module name"));
            }
            if source.source.len() > MAX_SOURCE_BYTES {
                return Err(CompileError::new(name, "script exceeds 256 KiB"));
            }
            let bytecode = compiler
                .compile(&source.source)
                .map_err(|error| CompileError::new(name, error.to_string()))?;
            remaining.store(MODULE_BUDGET, Ordering::Relaxed);
            let table: mlua::Table = lua
                .load(&bytecode[..])
                .set_name(format!("={name}"))
                .set_mode(ChunkMode::Binary)
                .set_environment(crate::fresh_env(&lua).map_err(CompileError::runtime)?)
                .call(())
                .map_err(|error| CompileError::new(name, error.to_string()))?;
            let mut functions = Vec::new();
            for pair in table.pairs::<mlua::Value, mlua::Value>() {
                let (key, value) =
                    pair.map_err(|error| CompileError::new(name, error.to_string()))?;
                let key = match key {
                    mlua::Value::String(key) => key.to_str().map(|s| s.to_owned()).ok(),
                    _ => None,
                }
                .filter(|key| module_name(key))
                .ok_or_else(|| CompileError::new(name, "module keys must be lowercase names"))?;
                if !value.is_function() {
                    return Err(CompileError::new(
                        name,
                        format!("module entry {key} is not a function; keep state in variables"),
                    ));
                }
                functions.push(key);
            }
            functions.sort();
            for function in functions {
                handlers.push((format!("{name}.{function}"), index, function));
            }
            modules.push(Module {
                name: name.clone(),
                bytecode,
            });
        }
        if handlers.len() > MAX_HANDLERS {
            return Err(CompileError {
                module: None,
                message: format!("at most {MAX_HANDLERS} handlers"),
            });
        }
        handlers.sort();
        Ok(Arc::new(Program { modules, handlers }))
    }

    pub(crate) fn modules(&self) -> &[Module] {
        &self.modules
    }

    /// Qualified handler names in identity order.
    pub fn handlers(&self) -> impl ExactSizeIterator<Item = &str> {
        self.handlers.iter().map(|(name, _, _)| name.as_str())
    }

    pub fn handler_count(&self) -> u32 {
        self.handlers.len() as u32
    }

    pub fn handler_id(&self, name: &str) -> Option<HandlerId> {
        self.handlers
            .binary_search_by(|(qualified, _, _)| qualified.as_str().cmp(name))
            .ok()
            .map(|index| HandlerId(index as u32))
    }

    pub fn handler_name(&self, id: HandlerId) -> Option<&str> {
        self.handlers.get(id.0 as usize).map(|(name, _, _)| name.as_str())
    }

    pub(crate) fn handler(&self, id: HandlerId) -> Option<(usize, &str)> {
        self.handlers
            .get(id.0 as usize)
            .map(|(_, module, function)| (*module, function.as_str()))
    }
}
