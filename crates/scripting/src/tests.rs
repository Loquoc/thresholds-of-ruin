//! Luau sandbox, budget and end-to-end hook contracts.

use super::*;
use std::collections::BTreeSet;
use std::num::NonZeroU64;
use tor_simulation::creatures::{Content, ScenarioCatalog};
use tor_simulation::rules::{Ruleset, DEFAULT_RULESET_TOML};
use tor_simulation::scripting::{
    ActorClass, Binding, FailureReason, FilterKey, HookKind, HookPort, MessageSegment,
    ScopeKind, ScriptSetup, Value, ValueType, VarScope, VariableSchema,
};
use tor_simulation::{Action, ActorId, Game};
use tor_world::{Direction, Position, RegionId};

fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    }
}

fn source(name: &str, text: &str) -> ScriptSource {
    ScriptSource {
        name: name.into(),
        source: text.into(),
    }
}

fn content() -> Arc<Content> {
    let rules = Ruleset::validate(toml::from_str(DEFAULT_RULESET_TOML).unwrap()).unwrap();
    Content::new(rules, ScenarioCatalog::default()).unwrap()
}

fn config() -> RuntimeConfig {
    RuntimeConfig {
        anchors: BTreeMap::from([("goal".into(), at(3))]),
        controls: BTreeSet::from(["pick_up".into(), "wait".into()]),
    }
}

/// A character at x = 1 in the two-room fixture, scripted with `script`
/// and bound by `bindings` (handler names, kinds and keys).
fn game(script: &str, bindings: &[(&str, HookKind, FilterKey)]) -> (Game, ActorId, Arc<Program>) {
    let program = Program::compile(&[source("lesson", script)]).unwrap();
    let mut game = Game::two_room(42, content());
    let actor = game
        .spawn_actor(at(1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_run(
        actor,
        BTreeSet::from([actor]),
        None,
        BTreeMap::new(),
    )
    .unwrap();
    game.install_scripts(ScriptSetup {
        handlers: program.handler_count(),
        schema: VariableSchema {
            variables: BTreeMap::from([
                ("steps".into(), (ScopeKind::Scenario, ValueType::Int)),
                ("note".into(), (ScopeKind::Actor, ValueType::Text)),
            ]),
        },
        bindings: bindings
            .iter()
            .map(|(name, kind, key)| Binding {
                kind: *kind,
                key: key.clone(),
                actors: ActorClass::Character,
                handler: program.handler_id(name).unwrap(),
                owner: None,
                once: false,
            })
            .collect(),
        defaults: vec![("steps".into(), Value::Int(0))],
    })
    .unwrap();
    let runtime = Runtime::new(program.clone(), config()).unwrap();
    game.set_hook_host(HookPort::new(Arc::new(runtime)));
    (game, actor, program)
}

fn steps(game: &Game) -> Option<&Value> {
    game.script_state().unwrap().var(VarScope::Scenario, "steps")
}

#[test]
fn compile_enumerates_sorted_handlers_and_rejects_bad_modules() {
    let program = Program::compile(&[
        source("doors", "return { pull = function() end, close = function() end }"),
        source("eggs", "local m = {} function m.hatch() end return m"),
    ])
    .unwrap();
    assert_eq!(
        program.handlers().collect::<Vec<_>>(),
        ["doors.close", "doors.pull", "eggs.hatch"]
    );
    assert_eq!(program.handler_id("eggs.hatch").unwrap().0, 2);
    for (name, text) in [
        ("broken", "return {"),
        ("noreturn", "local x = 1"),
        ("state", "return { count = 0 }"),
        ("Upper", "return {}"),
        ("loop", "while true do end"),
        ("global", "x = 1 return {}"),
    ] {
        assert!(
            Program::compile(&[source(name, text)]).is_err(),
            "{name} must be rejected"
        );
    }
    assert!(Program::compile(&[source("a", "return {}"), source("a", "return {}")]).is_err());
}

#[test]
fn the_sandbox_exposes_no_io_clock_reflection_or_unseeded_randomness() {
    let lua = sandbox().unwrap();
    for global in [
        "os", "io", "debug", "require", "load", "loadstring", "dofile", "getfenv", "setfenv",
        "collectgarbage", "gcinfo", "print", "coroutine",
    ] {
        let value: mlua::Value = lua.globals().get(global).unwrap();
        assert!(value.is_nil(), "{global} must be absent");
    }
    let random: mlua::Value = lua.load("return math.random").eval().unwrap();
    assert!(random.is_nil());
    assert!(lua.load("math.pi = 3").exec().is_err(), "libraries are read-only");
    assert!(lua.load("string.rep = nil").exec().is_err());
}

#[test]
fn a_handler_reads_facts_and_its_effects_apply() {
    let (mut game, actor, _) = game(
        r#"
        local lesson = {}
        function lesson.arrive(ctx, facts)
            local fact = facts[1]
            assert(fact.kind == "moved")
            assert(fact.to.x == ctx:anchor("goal").x)
            ctx:set_var("steps", #facts + 41)
            ctx:set_var("note", "arrived", fact.actor)
            ctx:message("Press {control:pick_up} to take it.", { style = "hint", to = fact.actor })
        end
        return lesson
        "#,
        &[("lesson.arrive", HookKind::Moved, FilterKey::Cell(at(3)))],
    );
    game.act(actor, Action::Move(Direction::East)).unwrap();
    game.act(actor, Action::Move(Direction::East)).unwrap();
    assert_eq!(steps(&game), Some(&Value::Int(42)));
    assert_eq!(
        game.script_state().unwrap().var(VarScope::Actor(actor), "note"),
        Some(&Value::Text("arrived".into()))
    );
    let report = game.take_script_report();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(
        report.messages[0].segments,
        vec![
            MessageSegment::Text("Press ".into()),
            MessageSegment::Control("pick_up".into()),
            MessageSegment::Text(" to take it.".into()),
        ]
    );
}

#[test]
fn module_locals_never_carry_state_between_calls() {
    let (mut game, actor, _) = game(
        r#"
        local calls = 0
        return {
            count = function(ctx)
                calls += 1
                ctx:set_var("steps", calls)
            end,
        }
        "#,
        &[("lesson.count", HookKind::Moved, FilterKey::Any)],
    );
    game.act(actor, Action::Move(Direction::East)).unwrap();
    game.act(actor, Action::Move(Direction::East)).unwrap();
    assert_eq!(steps(&game), Some(&Value::Int(1)), "each call starts fresh");
}

#[test]
fn persistent_counting_goes_through_variables() {
    let (mut game, actor, _) = game(
        r#"
        return {
            count = function(ctx)
                ctx:set_var("steps", ctx:var("steps") + 1)
            end,
        }
        "#,
        &[("lesson.count", HookKind::Moved, FilterKey::Any)],
    );
    for _ in 0..3 {
        game.act(actor, Action::Move(Direction::East)).unwrap();
    }
    assert_eq!(steps(&game), Some(&Value::Int(3)));
}

#[test]
fn an_infinite_loop_exhausts_the_budget_and_changes_nothing() {
    let (mut game, actor, _) = game(
        r#"return { spin = function(ctx) ctx:set_var("steps", 9) while true do end end }"#,
        &[("lesson.spin", HookKind::Moved, FilterKey::Any)],
    );
    game.act(actor, Action::Move(Direction::East)).unwrap();
    assert_eq!(steps(&game), Some(&Value::Int(0)));
    assert_eq!(
        game.take_script_report().failures[0].reason,
        FailureReason::Budget
    );
    // The VM stays usable for later calls.
    game.act(actor, Action::Move(Direction::East)).unwrap();
}

#[test]
fn runaway_allocation_hits_the_memory_limit_deterministically() {
    let (mut game, actor, _) = game(
        r#"return { grow = function(ctx)
            local t = {}
            for i = 1, 1e9 do t[i] = string.rep("x", 4096) end
        end }"#,
        &[("lesson.grow", HookKind::Moved, FilterKey::Any)],
    );
    game.act(actor, Action::Move(Direction::East)).unwrap();
    let reason = game.take_script_report().failures[0].reason.clone();
    assert!(
        matches!(reason, FailureReason::Memory | FailureReason::Budget),
        "{reason:?}"
    );
}

#[test]
fn errors_and_invalid_requests_reject_with_a_bounded_message() {
    for script in [
        r#"return { f = function(ctx) error("lesson broke") end }"#,
        r#"return { f = function(ctx) ctx:message("{control:fly}") end }"#,
        r#"return { f = function(ctx) ctx:set_var("steps", 0.5) end }"#,
        r#"return { f = function(ctx) ctx:teleport("item:1", ctx:anchor("goal")) end }"#,
        r#"return { f = function(ctx) ctx:bind({ on = "moved", handler = "lesson.missing" }) end }"#,
    ] {
        let (mut game, actor, _) = game(script, &[("lesson.f", HookKind::Moved, FilterKey::Any)]);
        game.act(actor, Action::Move(Direction::East)).unwrap();
        let failures = game.take_script_report().failures;
        assert_eq!(failures.len(), 1, "{script}");
        assert!(matches!(&failures[0].reason, FailureReason::Error(m) if m.len() <= 512));
    }
}

#[test]
fn dynamic_bindings_and_timers_round_trip_through_identities() {
    let (mut game, actor, program) = game(
        r#"
        local lesson = {}
        function lesson.start(ctx, facts)
            ctx:bind({ on = "waited", handler = "lesson.waited", once = true })
            ctx:start_timer("lesson.ring", 300, { arg = 7 })
        end
        function lesson.waited(ctx) ctx:set_var("steps", 100) end
        function lesson.ring(ctx, facts) ctx:set_var("steps", facts[1].arg) end
        return lesson
        "#,
        &[("lesson.start", HookKind::RunStarted, FilterKey::Any)],
    );
    game.start_scripts().unwrap();
    assert!(game.take_script_report().failures.is_empty());
    game.act(actor, Action::Wait).unwrap();
    assert_eq!(steps(&game), Some(&Value::Int(100)));
    while game.tick() < 300 {
        game.act(actor, Action::Wait).unwrap();
    }
    assert_eq!(steps(&game), Some(&Value::Int(7)));
    let report = game.take_script_report();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let handlers: Vec<_> = report
        .dispatches
        .iter()
        .map(|d| program.handler_name(d.handler).unwrap())
        .collect();
    assert_eq!(handlers.last(), Some(&"lesson.ring"));
}

#[test]
fn script_randomness_is_seeded_replayable_and_rejected_roots_keep_no_draws() {
    let script = r#"return { roll = function(ctx)
        ctx:set_var("steps", ctx:random(1000000))
    end }"#;
    let bindings = [("lesson.roll", HookKind::Moved, FilterKey::Any)];
    let (mut a, actor, _) = game(script, &bindings);
    let (mut b, _, _) = game(script, &bindings);
    for _ in 0..2 {
        a.act(actor, Action::Move(Direction::East)).unwrap();
        b.act(actor, Action::Move(Direction::East)).unwrap();
    }
    assert_eq!(steps(&a), steps(&b));
    assert_eq!(a.script_state().unwrap().rng(), b.script_state().unwrap().rng());
    assert_eq!(a.take_script_report(), b.take_script_report());

    let (mut c, actor, _) = game(
        r#"return { roll = function(ctx) ctx:random(6) error("no") end }"#,
        &bindings,
    );
    let before = c.script_state().unwrap().rng();
    c.act(actor, Action::Move(Direction::East)).unwrap();
    assert_eq!(c.script_state().unwrap().rng(), before);
}

#[test]
fn identical_runs_charge_identical_work() {
    let script = r#"return { work = function(ctx, facts)
        local s = 0
        for i = 1, 5000 do s += i % 7 end
        ctx:set_var("steps", s)
    end }"#;
    let bindings = [("lesson.work", HookKind::Moved, FilterKey::Any)];
    let (mut a, actor, _) = game(script, &bindings);
    let (mut b, _, _) = game(script, &bindings);
    a.act(actor, Action::Move(Direction::East)).unwrap();
    b.act(actor, Action::Move(Direction::East)).unwrap();
    let (ra, rb) = (a.take_script_report(), b.take_script_report());
    assert!(ra.dispatches[0].work > 0);
    assert_eq!(ra.dispatches, rb.dispatches);
}

#[test]
fn full_width_identities_survive_the_boundary() {
    let lua = sandbox().unwrap();
    let raw = u64::MAX - 1;
    let value = mlua::Value::String(lua.create_string(values::id("actor", raw)).unwrap());
    assert_eq!(values::actor(&value).unwrap(), ActorId(raw));
    let bad = mlua::Value::String(lua.create_string("actor:18446744073709551616").unwrap());
    assert!(values::actor(&bad).is_err());
    assert!(values::exact(&lua, 1 << 53).is_err());
}
