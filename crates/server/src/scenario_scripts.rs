//! Package scripts: the `[scripting]` manifest table, its validation, and
//! attaching a Luau runtime to games built from the package.
//!
//! Script files are pinned into the package (and so into saves and compiled
//! artifacts) as authored text. Their digests join the certificate's files,
//! so they participate in the content hash, and a scripted package's model
//! hash also covers them. A game restored from a save gets a fresh runtime
//! built from the pinned text; the runtime holds no game state.
use super::{fail, label, require, source_digest, Manifest, Package};
use crate::Failure;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};
use std::sync::{Arc, Mutex};
use tor_scripting::{Program, Runtime, RuntimeConfig, ScriptSource};
use tor_simulation::scripting::{
    ActorClass, Binding, FilterKey, HookKind, HookPort, ScopeKind, ScriptSetup, Value, ValueType,
    VariableSchema, MAX_BINDINGS, MAX_NAME_BYTES, MAX_TEXT_BYTES, MAX_VARIABLES,
};
use tor_simulation::{ActorId, Game, ItemId, ItemIdentity};
use tor_world::RegionId;

/// All of a package's scripts together.
pub(crate) const MAX_TOTAL_SCRIPT_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptingManifest {
    /// Package-relative `.luau` files. Each file's stem names its module, and
    /// handlers are `module.function`.
    pub scripts: Vec<String>,
    /// Every variable a script may read or write, with its scope and type.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, VariableDecl>,
    /// Initial bindings. Scripts can add and remove bindings while playing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<HookDecl>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariableDecl {
    pub scope: ScopeKind,
    #[serde(rename = "type")]
    pub value_type: ValueType,
    /// Only scenario-scoped variables have defaults; others start unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<DefaultValue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DefaultValue {
    Bool(bool),
    Int(i64),
    Text(String),
}

impl DefaultValue {
    fn value(&self) -> Value {
        match self {
            Self::Bool(value) => Value::Bool(*value),
            Self::Int(value) => Value::Int(*value),
            Self::Text(value) => Value::Text(value.clone()),
        }
    }
}

/// One initial binding. At most one filter (`anchor`, `region`, `actor`,
/// `item` or `identity`) narrows it; none hears every fact of its kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HookDecl {
    pub on: HookKind,
    pub handler: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<u64>,
    /// An actor id, or `"selected"` for the run's selected character, which
    /// is resolved when the scripts are installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<ActorFilter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actors: Option<ActorClass>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub once: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ActorFilter {
    Id(u64),
    Named(NamedActor),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamedActor {
    Selected,
}

/// The validated program and, per seed, the runtime games share. A runtime
/// keeps no game state, so every game of one package and seed can use it.
#[derive(Default)]
pub(crate) struct ScriptCache {
    program: Option<Arc<Program>>,
    runtime: Option<(u64, Arc<Runtime>)>,
}

impl std::fmt::Debug for ScriptCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptCache")
            .field("compiled", &self.program.is_some())
            .finish()
    }
}

pub(crate) type SharedScriptCache = Arc<Mutex<ScriptCache>>;

pub(crate) fn valid_script_path(relative: &str) -> bool {
    !relative.is_empty()
        && relative.ends_with(".luau")
        && !relative.contains(['\\', ':'])
        && !relative.starts_with("regions/")
        && Path::new(relative)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

fn module_of(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.strip_suffix(".luau").unwrap_or(file)
}

fn variable_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    name.len() <= MAX_NAME_BYTES
        && bytes.next().is_some_and(|b| b.is_ascii_lowercase())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Read the manifest's script files from a package directory.
pub(crate) fn read_scripts(
    root: &Path,
    manifest: &Manifest,
) -> Result<BTreeMap<String, String>, Failure> {
    let Some(scripting) = &manifest.scripting else {
        return Ok(BTreeMap::new());
    };
    let mut scripts = BTreeMap::new();
    let mut total = 0usize;
    for path in &scripting.scripts {
        require(
            valid_script_path(path),
            format!("Script {path} must be a relative .luau file in the package"),
        )?;
        let text = super::read_limited(root, path, tor_scripting::MAX_SOURCE_BYTES as u64)?;
        total = total.saturating_add(text.len());
        require(
            total <= MAX_TOTAL_SCRIPT_BYTES,
            "Scripts exceed 512 KiB in total",
        )?;
        require(
            scripts.insert(path.clone(), text).is_none(),
            format!("Duplicate script {path}"),
        )?;
    }
    Ok(scripts)
}

/// Package-relative script digests, for the certificate's files.
pub(crate) fn digests(scripts: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    scripts
        .iter()
        .map(|(path, text)| (path.clone(), source_digest(text)))
        .collect()
}

/// Compile the package's scripts and check every declaration that does not
/// depend on a seed. Anchors are resolved when a game is built.
pub(crate) fn compile(
    manifest: &Manifest,
    scripts: &BTreeMap<String, String>,
) -> Result<Option<Arc<Program>>, Failure> {
    let Some(scripting) = &manifest.scripting else {
        require(scripts.is_empty(), "Scripts without a [scripting] table")?;
        return Ok(None);
    };
    require(
        !scripting.scripts.is_empty(),
        "[scripting] must list at least one script",
    )?;
    require(
        scripting.scripts.iter().collect::<BTreeSet<_>>() == scripts.keys().collect(),
        "Pinned scripts differ from the manifest's script list",
    )?;
    let sources: Vec<_> = scripting
        .scripts
        .iter()
        .map(|path| ScriptSource {
            name: module_of(path).to_owned(),
            source: scripts[path].clone(),
        })
        .collect();
    let program = Program::compile(&sources).map_err(|error| fail(error.to_string()))?;
    require(
        scripting.variables.len() <= MAX_VARIABLES,
        "Too many script variables",
    )?;
    for (name, decl) in &scripting.variables {
        require(
            variable_name(name),
            format!("Script variable {name} must be a lowercase name"),
        )?;
        if let Some(default) = &decl.default {
            require(
                decl.scope == ScopeKind::Scenario,
                format!("Script variable {name}: only scenario variables have defaults"),
            )?;
            let value = default.value();
            require(
                value.value_type() == decl.value_type,
                format!("Script variable {name}: default does not match its type"),
            )?;
            if let Value::Text(text) = &value {
                require(
                    text.len() <= MAX_TEXT_BYTES,
                    format!("Script variable {name}: default text is too long"),
                )?;
            }
        }
    }
    require(
        scripting.hooks.len() <= MAX_BINDINGS,
        "Too many script hooks",
    )?;
    for (index, hook) in scripting.hooks.iter().enumerate() {
        let place = || format!("Script hook {} ({})", index + 1, hook.handler);
        require(
            program.handler_id(&hook.handler).is_some(),
            format!("{}: unknown handler", place()),
        )?;
        let filters = [
            hook.anchor.is_some(),
            hook.region.is_some(),
            hook.actor.is_some(),
            hook.item.is_some(),
            hook.identity.is_some(),
        ];
        require(
            filters.iter().filter(|set| **set).count() <= 1,
            format!("{}: at most one filter", place()),
        )?;
        if let Some(anchor) = &hook.anchor {
            require(label(anchor), format!("{}: invalid anchor", place()))?;
        }
        if let Some(identity) = &hook.identity {
            ItemIdentity::try_from(identity.as_str())
                .map_err(|_| fail(format!("{}: invalid item identity", place())))?;
        }
        // The key's shape is enough to check what the kind accepts.
        let shape = key_shape(hook, 0);
        require(
            hook.on.accepts(&shape),
            format!("{}: {:?} hooks cannot use this filter", place(), hook.on),
        )?;
    }
    Ok(Some(program))
}

fn key_shape(hook: &HookDecl, selected: u64) -> FilterKey {
    if hook.anchor.is_some() {
        FilterKey::Cell(tor_world::Location {
            region: RegionId(0),
            position: tor_world::Position { x: 0, y: 0, z: 0 },
        })
    } else if let Some(region) = hook.region {
        FilterKey::Region(RegionId(region))
    } else if let Some(actor) = hook.actor {
        FilterKey::Actor(ActorId(match actor {
            ActorFilter::Id(id) => id,
            ActorFilter::Named(NamedActor::Selected) => selected,
        }))
    } else if let Some(item) = hook.item {
        FilterKey::Item(ItemId(item))
    } else if let Some(identity) = &hook.identity {
        ItemIdentity::try_from(identity.as_str()).map_or(FilterKey::Any, FilterKey::ItemIdentity)
    } else {
        FilterKey::Any
    }
}

fn schema(scripting: &ScriptingManifest) -> VariableSchema {
    VariableSchema {
        variables: scripting
            .variables
            .iter()
            .map(|(name, decl)| (name.clone(), (decl.scope, decl.value_type)))
            .collect(),
    }
}

impl Package {
    /// The validated program, compiled once per loaded package.
    pub(crate) fn script_program(&self) -> Result<Option<Arc<Program>>, Failure> {
        if self.manifest.scripting.is_none() && self.scripts.is_empty() {
            return Ok(None);
        }
        let mut cache = self
            .script_cache
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(program) = &cache.program {
            return Ok(Some(program.clone()));
        }
        let program = compile(&self.manifest, &self.scripts)?;
        cache.program = program.clone();
        Ok(program)
    }

    /// Certificate integrity for pinned scripts: each script's digest is one
    /// of the certified files, and the program compiles.
    pub(crate) fn check_scripts(&self) -> Result<(), Failure> {
        for (path, text) in &self.scripts {
            require(
                valid_script_path(path)
                    && self.certificate.files.get(path) == Some(&source_digest(text)),
                "Pinned script integrity mismatch",
            )?;
        }
        let declared = self
            .manifest
            .scripting
            .as_ref()
            .map_or(0, |scripting| scripting.scripts.len());
        require(
            declared == self.scripts.len(),
            "Pinned scripts differ from the manifest's script list",
        )?;
        self.script_program().map(|_| ())
    }

    /// Named places scripts can use: every package anchor and, for a unique
    /// retrieval, its exit endpoint.
    fn script_anchors(&self, seed: u64) -> Result<BTreeMap<String, tor_world::Location>, Failure> {
        let mut anchors = self.anchors(seed)?;
        if let Some(policy) = &self.manifest.unique_retrieval {
            let mut resolved = anchors.clone();
            let exit =
                self.character_anchor(self.selected, seed, &policy.exit_anchor, &mut resolved)?;
            anchors.insert(policy.exit_anchor.clone(), exit);
        }
        Ok(anchors)
    }

    /// Anchors that every hook names must exist for this seed.
    pub(crate) fn check_script_anchors(&self, seed: u64) -> Result<(), Failure> {
        let Some(scripting) = &self.manifest.scripting else {
            return Ok(());
        };
        let anchors = self.script_anchors(seed)?;
        for hook in &scripting.hooks {
            if let Some(anchor) = &hook.anchor {
                require(
                    anchors.contains_key(anchor),
                    format!("Script hook {}: unknown anchor {anchor}", hook.handler),
                )?;
            }
        }
        Ok(())
    }

    fn script_runtime(&self, program: &Arc<Program>, seed: u64) -> Result<Arc<Runtime>, Failure> {
        let cache = self
            .script_cache
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some((cached, runtime)) = &cache.runtime {
            if *cached == seed {
                return Ok(runtime.clone());
            }
        }
        drop(cache);
        let config = RuntimeConfig {
            anchors: self.script_anchors(seed)?,
            // The protocol's closed control vocabulary; clients name their
            // own bindings.
            controls: tor_protocol::ControlHint::ALL
                .iter()
                .map(|control| control.id().to_owned())
                .collect(),
        };
        let runtime = Arc::new(
            Runtime::new(program.clone(), config).map_err(|error| fail(error.to_string()))?,
        );
        let mut cache = self
            .script_cache
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        cache.runtime = Some((seed, runtime.clone()));
        Ok(runtime)
    }

    /// Give a newly built game its initial script state and runtime.
    pub(crate) fn install_scripts(&self, game: &mut Game, seed: u64) -> Result<(), Failure> {
        let Some(program) = self.script_program()? else {
            return Ok(());
        };
        let scripting = self
            .manifest
            .scripting
            .as_ref()
            .ok_or_else(|| fail("Scripts without a [scripting] table"))?;
        let anchors = self.script_anchors(seed)?;
        let mut bindings = Vec::with_capacity(scripting.hooks.len());
        for hook in &scripting.hooks {
            let key = match &hook.anchor {
                Some(anchor) => FilterKey::Cell(*anchors.get(anchor).ok_or_else(|| {
                    fail(format!(
                        "Script hook {}: unknown anchor {anchor}",
                        hook.handler
                    ))
                })?),
                None => key_shape(hook, self.selected),
            };
            bindings.push(Binding {
                kind: hook.on,
                key,
                actors: hook.actors.unwrap_or_default(),
                handler: program
                    .handler_id(&hook.handler)
                    .ok_or_else(|| fail("Unknown script handler"))?,
                owner: None,
                once: hook.once,
            });
        }
        let setup = ScriptSetup {
            handlers: program.handler_count(),
            schema: schema(scripting),
            bindings,
            defaults: scripting
                .variables
                .iter()
                .filter_map(|(name, decl)| Some((name.clone(), decl.default.as_ref()?.value())))
                .collect(),
        };
        game.install_scripts(setup)
            .map_err(|_| fail("Invalid script setup"))?;
        let runtime = self.script_runtime(&program, seed)?;
        game.set_hook_host(HookPort::new(runtime));
        Ok(())
    }

    /// Reattach the runtime to a restored game, after checking its script
    /// state was made by this package's scripts.
    pub(crate) fn attach_scripts(&self, game: &mut Game, seed: u64) -> Result<(), Failure> {
        let program = self.script_program()?;
        match (&program, game.script_state(), &self.manifest.scripting) {
            (None, None, _) => Ok(()),
            (Some(program), Some(state), Some(scripting)) => {
                require(
                    state.handlers() == program.handler_count()
                        && *state.schema() == schema(scripting),
                    "Saved script state does not match the package's scripts",
                )?;
                let runtime = self.script_runtime(program, seed)?;
                game.set_hook_host(HookPort::new(runtime));
                Ok(())
            }
            _ => Err(fail("Saved script state does not match the package")),
        }
    }
}
