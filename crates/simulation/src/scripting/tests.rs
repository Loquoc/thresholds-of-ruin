//! Scenario hook contracts with a fake host. Luau-specific behavior is owned
//! by `tor-scripting`; these tests pin the language-independent boundary.

use super::*;
use crate::checkpoint::{SharedState, Snapshot};
use crate::{Action, Game};
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};
use tor_world::{Direction, Location, Position, RegionId};

fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    }
}

type Reply = dyn Fn(ScriptView<'_>, &HookCall<'_>) -> Result<Vec<ScriptEffect>, FailureReason>
    + Send
    + Sync;

/// Records every call; answers with a closure.
struct FakeHost {
    reply: Box<Reply>,
    calls: Mutex<Vec<(HandlerId, u64, Vec<HookFact>, u8)>>,
}

impl HookHost for FakeHost {
    fn dispatch(
        &self,
        view: ScriptView<'_>,
        call: HookCall<'_>,
        budget: &mut WorkBudget,
    ) -> Result<HookOutput, FailureReason> {
        self.calls.lock().unwrap().push((
            call.handler,
            view.tick(),
            call.facts.to_vec(),
            call.depth,
        ));
        if !budget.charge(10) {
            return Err(FailureReason::Budget);
        }
        let effects = (self.reply)(view, &call)?;
        Ok(HookOutput {
            effects,
            rng: view.state().rng(),
        })
    }
}

fn host(reply: impl Fn(ScriptView<'_>, &HookCall<'_>) -> Result<Vec<ScriptEffect>, FailureReason> + Send + Sync + 'static) -> Arc<FakeHost> {
    Arc::new(FakeHost {
        reply: Box::new(reply),
        calls: Mutex::new(Vec::new()),
    })
}

fn schema() -> VariableSchema {
    VariableSchema {
        variables: BTreeMap::from([
            ("seen".into(), (ScopeKind::Scenario, ValueType::Bool)),
            ("count".into(), (ScopeKind::Scenario, ValueType::Int)),
        ]),
    }
}

fn bind(kind: HookKind, key: FilterKey, handler: u32) -> Binding {
    Binding {
        kind,
        key,
        actors: ActorClass::Character,
        handler: HandlerId(handler),
        owner: None,
        once: false,
    }
}

/// A character in the two-room fixture at x = 1, scripted with `bindings`.
fn scripted(bindings: Vec<Binding>, host: Option<Arc<FakeHost>>) -> (Game, ActorId) {
    let mut game = Game::two_room(42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(at(1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_run(actor, BTreeSet::from([actor]), None, BTreeMap::new())
        .unwrap();
    game.install_scripts(ScriptSetup {
        handlers: 4,
        schema: schema(),
        bindings,
        defaults: vec![("count".into(), Value::Int(0))],
    })
    .unwrap();
    if let Some(host) = host {
        game.set_hook_host(HookPort::new(host));
    }
    (game, actor)
}

fn message(text: &str) -> ScriptEffect {
    ScriptEffect::Message {
        audience: Audience::Characters,
        style: MessageStyle::Tutorial,
        segments: vec![MessageSegment::Text(text.into())],
    }
}

fn set_count(value: i64) -> ScriptEffect {
    ScriptEffect::SetVar {
        scope: VarScope::Scenario,
        name: "count".into(),
        value: Value::Int(value),
    }
}

#[test]
fn unscripted_games_never_build_facts_or_call_a_host() {
    let mut game = Game::two_room(42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(at(1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let before = crate::diagnostics::work_counts();
    game.act(actor, Action::Move(Direction::East)).unwrap();
    game.act(actor, Action::Wait).unwrap();
    let after = crate::diagnostics::work_counts();
    // An unscripted site is one mask test: no lookup, fact or call.
    assert_eq!(after.hook_lookups, before.hook_lookups);
    assert_eq!(after.hook_facts_noted, before.hook_facts_noted);
    assert_eq!(after.hook_handler_calls, before.hook_handler_calls);
    assert_eq!(game.hook_mask, 0);
    let snapshot = game.checkpoint(&mut SharedState::default());
    let json = serde_json::to_value(&snapshot).unwrap();
    assert!(json.get("script").is_none(), "unscripted saves are unchanged");
}

#[test]
fn a_cell_binding_hears_only_its_cell_at_the_exact_tick() {
    let fake = host(|_, _| Ok(vec![message("You found the spot.")]));
    let (mut game, actor) = scripted(
        vec![bind(HookKind::Moved, FilterKey::Cell(at(3)), 1)],
        Some(fake.clone()),
    );
    game.act(actor, Action::Move(Direction::East)).unwrap();
    assert!(fake.calls.lock().unwrap().is_empty(), "x = 2 is not bound");
    assert!(game.take_script_report().is_empty());
    let tick = game.tick();
    game.act(actor, Action::Move(Direction::East)).unwrap();
    let calls = fake.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    let (handler, call_tick, facts, depth) = &calls[0];
    assert_eq!((*handler, *depth), (HandlerId(1), 0));
    assert_eq!(*call_tick, tick, "handlers run at the action's boundary");
    assert_eq!(
        facts[0].subject,
        FactSubject::Moved {
            from: at(2),
            to: at(3)
        }
    );
    let report = game.take_script_report();
    assert_eq!(report.messages.len(), 1);
    assert_eq!(report.dispatches.len(), 1);
}

#[test]
fn unsubscribed_kinds_and_unmatched_keys_cost_no_fact_and_no_call() {
    let fake = host(|_, _| Ok(vec![]));
    let (mut game, actor) = scripted(
        vec![bind(HookKind::Moved, FilterKey::Cell(at(4)), 0)],
        Some(fake.clone()),
    );
    // Waiting is not subscribed: the mask test alone rejects it.
    let before = crate::diagnostics::work_counts();
    game.act(actor, Action::Wait).unwrap();
    let after = crate::diagnostics::work_counts();
    assert_eq!(after.hook_lookups, before.hook_lookups);
    // Moving is subscribed, but only the bound cell's class is looked up and
    // nothing matches x = 2: one lookup, no fact, no call.
    game.act(actor, Action::Move(Direction::East)).unwrap();
    let moved = crate::diagnostics::work_counts();
    assert_eq!(moved.hook_lookups, after.hook_lookups + 1);
    assert_eq!(moved.hook_facts_noted, after.hook_facts_noted);
    assert_eq!(moved.hook_handler_calls, after.hook_handler_calls);
    assert!(fake.calls.lock().unwrap().is_empty());
}

#[test]
fn roots_without_world_effects_never_clone_the_game() {
    let fake = host(|_, _| Ok(vec![message("noted"), set_count(1)]));
    let (mut game, actor) = scripted(
        vec![bind(HookKind::Moved, FilterKey::Any, 0)],
        Some(fake.clone()),
    );
    let before = crate::diagnostics::work_counts();
    game.note_moved(actor, at(1), at(2));
    game.settle_script_hooks().unwrap();
    let after = crate::diagnostics::work_counts();
    assert_eq!(fake.calls.lock().unwrap().len(), 1);
    assert_eq!(after.game_clones, before.game_clones, "messages and variables need no checkpoint");
    assert_eq!(
        game.script_state().unwrap().var(VarScope::Scenario, "count"),
        Some(&Value::Int(1))
    );
}

#[test]
fn a_world_effect_checkpoints_once_and_a_rejected_root_restores_it() {
    let mut game = Game::two_room_with_doorway(42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(at(1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.install_scripts(ScriptSetup {
        handlers: 1,
        schema: schema(),
        bindings: vec![Binding {
            actors: ActorClass::Any,
            ..bind(HookKind::Moved, FilterKey::Any, 0)
        }],
        defaults: vec![],
    })
    .unwrap();
    let door = at(5);
    // Two world effects, then an invariant violation: one checkpoint, and the
    // door and variable are both restored.
    let fake = host(move |_, _| {
        Ok(vec![
            set_count(3),
            ScriptEffect::SetDoor { at: door, open: false },
            ScriptEffect::SetDoor { at: door, open: true },
            ScriptEffect::Teleport { actor: ActorId(999), to: at(1) },
        ])
    });
    game.set_hook_host(HookPort::new(fake));
    let open = game.world.door(door).unwrap().open;
    let before = crate::diagnostics::work_counts();
    game.note_moved(actor, at(1), at(2));
    game.settle_script_hooks().unwrap();
    let after = crate::diagnostics::work_counts();
    assert_eq!(after.game_clones - before.game_clones, 1);
    assert_eq!(game.world.door(door).unwrap().open, open);
    assert_eq!(game.script_state().unwrap().var(VarScope::Scenario, "count"), None);
    let report = game.take_script_report();
    assert_eq!(report.failures.len(), 1);
    assert!(report.messages.is_empty() && report.dispatches.is_empty());
}

#[test]
fn actor_class_filters_characters_from_creatures() {
    let fake = host(|_, _| Ok(vec![]));
    let mut creatures = bind(HookKind::Moved, FilterKey::Any, 0);
    creatures.actors = ActorClass::Creature;
    let (mut game, actor) = scripted(vec![creatures], Some(fake.clone()));
    game.act(actor, Action::Move(Direction::East)).unwrap();
    assert!(fake.calls.lock().unwrap().is_empty());
}

#[test]
fn a_failed_root_changes_nothing_and_its_siblings_still_apply() {
    let fake = host(|_, call| match call.handler.0 {
        0 => Ok(vec![set_count(7), message("never shown")]),
        _ => Err(FailureReason::Error("boom".into())),
    });
    let mut failing = bind(HookKind::Moved, FilterKey::Any, 1);
    failing.once = true;
    let (mut game, actor) = scripted(
        vec![bind(HookKind::Moved, FilterKey::Any, 0), failing],
        Some(fake),
    );
    game.act(actor, Action::Move(Direction::East)).unwrap();
    let state = game.script_state().unwrap();
    assert_eq!(state.var(VarScope::Scenario, "count"), Some(&Value::Int(7)));
    assert!(
        state.binding(BindingId(2)).is_some(),
        "a once binding is retired only by a successful root"
    );
    let report = game.take_script_report();
    assert_eq!(report.messages.len(), 1);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].handler, HandlerId(1));
    assert_eq!(report.failures[0].reason, FailureReason::Error("boom".into()));
}

#[test]
fn an_invariant_violation_rejects_every_effect_of_its_root() {
    let fake = host(|_, _| {
        Ok(vec![
            set_count(3),
            ScriptEffect::Teleport {
                actor: ActorId(1),
                to: Location {
                    region: RegionId(1),
                    position: Position { x: 99, y: 99, z: 0 },
                },
            },
        ])
    });
    let (mut game, actor) = scripted(
        vec![bind(HookKind::Moved, FilterKey::Any, 0)],
        Some(fake),
    );
    game.act(actor, Action::Move(Direction::East)).unwrap();
    assert_eq!(
        game.script_state().unwrap().var(VarScope::Scenario, "count"),
        Some(&Value::Int(0))
    );
    let report = game.take_script_report();
    assert!(matches!(
        report.failures[0].reason,
        FailureReason::Invariant { effect: 1, .. }
    ));
    assert!(report.dispatches.is_empty());
}

#[test]
fn undeclared_or_mistyped_variables_are_invariant_violations() {
    for effect in [
        ScriptEffect::SetVar {
            scope: VarScope::Scenario,
            name: "missing".into(),
            value: Value::Int(1),
        },
        ScriptEffect::SetVar {
            scope: VarScope::Scenario,
            name: "seen".into(),
            value: Value::Int(1),
        },
    ] {
        let fake = host(move |_, _| Ok(vec![effect.clone()]));
        let (mut game, actor) = scripted(
            vec![bind(HookKind::Moved, FilterKey::Any, 0)],
            Some(fake),
        );
        game.act(actor, Action::Move(Direction::East)).unwrap();
        assert_eq!(game.take_script_report().failures.len(), 1);
    }
}

#[test]
fn scripts_bind_at_runtime_and_once_bindings_retire_themselves() {
    // Handler 0 binds handler 1 to the next cell, once.
    let fake = host(|view, call| match call.handler.0 {
        0 => {
            let mut next = bind(HookKind::Moved, FilterKey::Cell(at(3)), 1);
            next.once = true;
            Ok(vec![ScriptEffect::Bind {
                id: view.state().next_binding(),
                binding: next,
            }])
        }
        _ => Ok(vec![message("next lesson")]),
    });
    let mut first = bind(HookKind::Moved, FilterKey::Cell(at(2)), 0);
    first.once = true;
    let (mut game, actor) = scripted(vec![first], Some(fake.clone()));
    game.act(actor, Action::Move(Direction::East)).unwrap();
    assert!(game.script_state().unwrap().binding(BindingId(1)).is_none());
    assert!(game.script_state().unwrap().binding(BindingId(2)).is_some());
    game.act(actor, Action::Move(Direction::East)).unwrap();
    assert!(game.script_state().unwrap().binding(BindingId(2)).is_none());
    game.act(actor, Action::Move(Direction::West)).unwrap();
    game.act(actor, Action::Move(Direction::East)).unwrap();
    let handlers: Vec<u32> = fake.calls.lock().unwrap().iter().map(|c| c.0 .0).collect();
    assert_eq!(handlers, vec![0, 1], "retired bindings never fire again");
    assert_eq!(game.hook_mask, 0, "no live bindings leave every site one branch");
}

#[test]
fn a_mispredicted_identity_is_rejected() {
    let fake = host(|_, _| {
        Ok(vec![ScriptEffect::Bind {
            id: BindingId(99),
            binding: bind(HookKind::Waited, FilterKey::Any, 0),
        }])
    });
    let (mut game, actor) = scripted(
        vec![bind(HookKind::Moved, FilterKey::Any, 0)],
        Some(fake),
    );
    game.act(actor, Action::Move(Direction::East)).unwrap();
    assert_eq!(game.take_script_report().failures.len(), 1);
    assert!(game.script_state().unwrap().binding(BindingId(99)).is_none());
}

#[test]
fn cascades_are_bounded_and_a_runaway_cascade_changes_nothing() {
    // Every move teleports back and forth, which is itself a move.
    let fake = host(|view, _| {
        let actor = ActorId(1);
        let here = view.actor_location(actor).unwrap();
        let to = if here == at(2) { at(3) } else { at(2) };
        Ok(vec![ScriptEffect::Teleport { actor, to }, set_count(1)])
    });
    let (mut game, actor) = scripted(
        vec![bind(HookKind::Moved, FilterKey::Any, 0)],
        Some(fake.clone()),
    );
    game.act(actor, Action::Move(Direction::East)).unwrap();
    let report = game.take_script_report();
    assert_eq!(report.failures[0].reason, FailureReason::Cascade);
    assert_eq!(
        fake.calls.lock().unwrap().len(),
        usize::from(MAX_CASCADE_DEPTH) + 1
    );
    assert_eq!(game.actors[&actor].location, at(2), "no teleport survives");
    assert_eq!(
        game.script_state().unwrap().var(VarScope::Scenario, "count"),
        Some(&Value::Int(0))
    );
}

#[test]
fn a_bounded_cascade_applies_with_its_root() {
    // A move to x = 2 teleports to x = 3; that move is heard once more.
    let fake = host(|view, call| {
        let here = view.actor_location(ActorId(1)).unwrap();
        if call.depth == 0 && here == at(2) {
            Ok(vec![ScriptEffect::Teleport {
                actor: ActorId(1),
                to: at(3),
            }])
        } else {
            Ok(vec![set_count(i64::from(call.depth))])
        }
    });
    let (mut game, actor) = scripted(
        vec![bind(HookKind::Moved, FilterKey::Any, 0)],
        Some(fake),
    );
    game.act(actor, Action::Move(Direction::East)).unwrap();
    assert_eq!(game.actors[&actor].location, at(3));
    assert_eq!(
        game.script_state().unwrap().var(VarScope::Scenario, "count"),
        Some(&Value::Int(1))
    );
    let depths: Vec<u8> = game
        .take_script_report()
        .dispatches
        .iter()
        .map(|d| d.depth)
        .collect();
    assert_eq!(depths, vec![0, 1]);
}

#[test]
fn exhausting_the_budget_rejects_the_root() {
    let fake = host(|_, _| Err(FailureReason::Budget));
    let (mut game, actor) = scripted(
        vec![bind(HookKind::Moved, FilterKey::Any, 0)],
        Some(fake),
    );
    game.act(actor, Action::Move(Direction::East)).unwrap();
    assert_eq!(
        game.take_script_report().failures[0].reason,
        FailureReason::Budget
    );
}

#[test]
fn a_bound_game_without_a_host_reports_instead_of_guessing() {
    let (mut game, actor) = scripted(vec![bind(HookKind::Moved, FilterKey::Any, 0)], None);
    game.act(actor, Action::Move(Direction::East)).unwrap();
    assert_eq!(
        game.take_script_report().failures[0].reason,
        FailureReason::NoHost
    );
}

#[test]
fn timers_fire_at_their_due_tick_and_wake_the_scheduler() {
    let fake = host(|view, call| match call.facts[0].subject {
        FactSubject::None => Ok(vec![ScriptEffect::StartTimer {
            id: view.state().next_timer(),
            handler: HandlerId(2),
            owner: None,
            after: 250,
            arg: 5,
        }]),
        FactSubject::Timer { arg, .. } => Ok(vec![set_count(arg)]),
        _ => Ok(vec![]),
    });
    let (mut game, actor) = scripted(
        vec![bind(HookKind::RunStarted, FilterKey::Any, 0)],
        Some(fake.clone()),
    );
    game.start_scripts().unwrap();
    assert_eq!(game.next_script_timer_tick(), Some(250));
    while game.tick() < 250 {
        game.act(actor, Action::Wait).unwrap();
    }
    let calls = fake.calls.lock().unwrap().clone();
    let timer = calls.iter().find(|c| c.0 == HandlerId(2)).expect("fired");
    assert_eq!(timer.1, 250, "a timer fires at exactly its due tick");
    assert_eq!(
        game.script_state().unwrap().var(VarScope::Scenario, "count"),
        Some(&Value::Int(5))
    );
    assert_eq!(game.next_script_timer_tick(), None);
}

#[test]
fn timers_freeze_with_their_owner_region_and_resume_without_catch_up() {
    let (mut game, actor) = scripted(vec![], None);
    let state = game.script.as_mut().unwrap();
    state
        .start_timer(
            TimerId(1),
            state::Timer {
                handler: HandlerId(0),
                owner: Some(Owner::Region(RegionId(1))),
                arg: 0,
                clock: state::TimerClock::Due(300),
            },
        )
        .unwrap();
    game.tick = 100;
    game.shift_script_timers(RegionId(1), true);
    assert_eq!(game.next_script_timer_tick(), None);
    assert_eq!(game.script_state().unwrap().timer(TimerId(1)).unwrap().clock, state::TimerClock::Frozen { remaining: 200 });
    game.tick = 1_000;
    game.shift_script_timers(RegionId(1), false);
    assert_eq!(game.next_script_timer_tick(), Some(1_200));
    let _ = actor;
}

#[test]
fn scenario_authority_opens_doors_no_actor_can_reach() {
    let mut game = Game::two_room_with_doorway(42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(at(1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_run(actor, BTreeSet::from([actor]), None, BTreeMap::new())
        .unwrap();
    game.install_scripts(ScriptSetup {
        handlers: 2,
        schema: schema(),
        bindings: vec![
            bind(HookKind::Waited, FilterKey::Any, 0),
            bind(HookKind::DoorChanged, FilterKey::Any, 1),
        ],
        defaults: vec![],
    })
    .unwrap();
    let door = at(5);
    let fake = host(move |view, call| match call.handler.0 {
        0 => Ok(vec![ScriptEffect::SetDoor {
            at: door,
            open: !view.door_open(door).unwrap(),
        }]),
        _ => Ok(vec![]),
    });
    game.set_hook_host(HookPort::new(fake.clone()));
    let before = game.world.door(door).unwrap().open;
    game.act(actor, Action::Wait).unwrap();
    assert_ne!(game.world.door(door).unwrap().open, before);
    let calls = fake.calls.lock().unwrap();
    assert_eq!(calls.len(), 2, "the script's own door change is a fact too");
    assert_eq!(calls[1].3, 1);
    assert_eq!(calls[1].2[0].actor, None);
}

#[test]
fn end_run_sets_a_terminal_victory() {
    let fake = host(|_, _| {
        Ok(vec![ScriptEffect::EndRun {
            end: RunEnd::Victory,
            actor: Some(ActorId(1)),
        }])
    });
    let (mut game, actor) = scripted(
        vec![bind(HookKind::Moved, FilterKey::Cell(at(2)), 0)],
        Some(fake),
    );
    game.act(actor, Action::Move(Direction::East)).unwrap();
    assert!(game.run_outcome().terminal);
    assert_eq!(game.run_outcome().victor, Some(actor));
}

#[test]
fn script_state_survives_checkpoint_and_rebuilds_its_index() {
    let fake = host(|view, _| {
        Ok(vec![
            set_count(9),
            ScriptEffect::Bind {
                id: view.state().next_binding(),
                binding: bind(HookKind::Waited, FilterKey::Any, 3),
            },
            ScriptEffect::StartTimer {
                id: view.state().next_timer(),
                handler: HandlerId(2),
                owner: None,
                after: 10_000,
                arg: 0,
            },
        ])
    });
    let (mut game, actor) = scripted(
        vec![bind(HookKind::Moved, FilterKey::Any, 0)],
        Some(fake.clone()),
    );
    game.act(actor, Action::Move(Direction::East)).unwrap();
    game.take_script_report();
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let bytes = serde_json::to_vec(&(snapshot, &shared)).unwrap();
    let (snapshot, shared): (Snapshot, SharedState) = serde_json::from_slice(&bytes).unwrap();
    let mut restored =
        Game::restore_checkpoint(snapshot, &shared, crate::test_creatures::content()).unwrap();
    assert_eq!(restored, game);
    assert_eq!(restored.hook_mask, game.hook_mask);
    assert_eq!(restored.next_script_timer_tick(), game.next_script_timer_tick());
    // The restored game needs its host again; then it behaves identically.
    restored.set_hook_host(HookPort::new(fake.clone()));
    game.act(actor, Action::Wait).unwrap();
    restored.act(actor, Action::Wait).unwrap();
    assert_eq!(restored, game);
    assert_eq!(restored.take_script_report(), game.take_script_report());
}

#[test]
fn script_state_rejects_tampered_saves() {
    let (game, _) = scripted(vec![bind(HookKind::Moved, FilterKey::Any, 0)], None);
    let mut json = serde_json::to_value(game.script_state().unwrap()).unwrap();
    json["bindings"][0][1]["handler"] = serde_json::json!(99);
    assert!(serde_json::from_value::<ScriptState>(json).is_err());
    let mut json = serde_json::to_value(game.script_state().unwrap()).unwrap();
    json["vars"][0][1] = serde_json::json!({"text": "zero"});
    assert!(serde_json::from_value::<ScriptState>(json).is_err());
}

#[test]
fn script_random_is_exact_and_rejects_a_zero_bound() {
    let mut a = 7;
    let mut b = 7;
    let draws: Vec<u64> = (0..32).map(|_| script_random(&mut a, 6).unwrap()).collect();
    assert!(draws.iter().all(|&d| d < 6));
    assert_eq!(
        draws,
        (0..32).map(|_| script_random(&mut b, 6).unwrap()).collect::<Vec<_>>()
    );
    assert_eq!(script_random(&mut a, 0), None);
}
