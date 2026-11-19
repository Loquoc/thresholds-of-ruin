//! Simulation-level stand-ins for scenario outcome scripts: the run is lost
//! when the selected character dies, and an arena ends when at most one
//! faction still stands. Packages do this in Luau; the engine never decides
//! an outcome by itself. Hosts are never saved, so tests reattach them after
//! a checkpoint restore exactly as the server does.
use std::sync::Arc;
use tor_simulation::scripting::{
    ActorClass, Binding, FailureReason, FilterKey, HandlerId, HookCall, HookHost, HookKind,
    HookOutput, HookPort, RunEnd, ScriptEffect, ScriptSetup, ScriptView, WorkBudget,
};
use tor_simulation::{ActorId, Game};

struct Defeat;

impl HookHost for Defeat {
    fn dispatch(
        &self,
        view: ScriptView<'_>,
        call: HookCall<'_>,
        _budget: &mut WorkBudget,
    ) -> Result<HookOutput, FailureReason> {
        Ok(HookOutput {
            effects: vec![ScriptEffect::EndRun {
                end: RunEnd::Defeat,
                actor: call.facts[0].actor,
                terminal: true,
            }],
            rng: view.state().rng(),
        })
    }
}

/// Lose the run when `selected` dies.
pub fn lose_on_death(game: &mut Game, selected: ActorId) {
    game.install_scripts(ScriptSetup {
        handlers: 1,
        schema: Default::default(),
        bindings: vec![Binding {
            kind: HookKind::Died,
            key: FilterKey::Actor(selected),
            actors: ActorClass::Any,
            handler: HandlerId(0),
            owner: None,
            once: false,
        }],
        defaults: vec![],
    })
    .unwrap();
    reattach(game);
}

/// Reinstall the defeat host after a checkpoint restore.
pub fn reattach(game: &mut Game) {
    game.set_hook_host(HookPort::new(Arc::new(Defeat)));
}

/// Ends an arena the way the elimination script does.
struct Eliminate;

impl HookHost for Eliminate {
    fn dispatch(
        &self,
        view: ScriptView<'_>,
        _call: HookCall<'_>,
        _budget: &mut WorkBudget,
    ) -> Result<HookOutput, FailureReason> {
        let mut factions = std::collections::BTreeSet::new();
        let mut standing = std::collections::BTreeSet::new();
        for actor in view.characters() {
            if let Some(faction) = view.faction(actor) {
                factions.insert(faction);
                if view.alive(actor) {
                    standing.insert(faction);
                }
            }
        }
        let effects = if standing.is_empty() {
            vec![ScriptEffect::StopArena { winner: None }]
        } else if factions.len() > 1 && standing.len() == 1 {
            vec![ScriptEffect::StopArena {
                winner: standing.first().map(|faction| (*faction).to_owned()),
            }]
        } else {
            vec![]
        };
        Ok(HookOutput {
            effects,
            rng: view.state().rng(),
        })
    }
}

/// End the arena by elimination when any participant dies. A game restored
/// with that binding already in its script state only gets the host back.
pub fn eliminate_on_death(game: &mut Game) {
    if game.script_state().is_none() {
        game.install_scripts(ScriptSetup {
            handlers: 1,
            schema: Default::default(),
            bindings: vec![Binding {
                kind: HookKind::Died,
                key: FilterKey::Any,
                actors: ActorClass::Character,
                handler: HandlerId(0),
                owner: None,
                once: false,
            }],
            defaults: vec![],
        })
        .unwrap();
    }
    game.set_hook_host(HookPort::new(Arc::new(Eliminate)));
}
