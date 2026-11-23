//! Invariants that every scenario package keeps. Each package is played for a
//! few turns, its own AI included, and then:
//!
//! - a deliberate stale command preserves disclosed states/history and accepted sequence;
//! - a retried request gets the original answer and changes nothing;
//! - a restart replays every actor's state exactly;
//! - rewinding to the start restores the initial observation. A busy package
//!   whose play outgrew the retained rewind window instead refuses that rewind
//!   without acting, and a one-turn game of it rewinds exactly.
//!
//! Feature tests don't need to repeat these checks for their own packages:
//! adding a package to `scenarios/` or `scenarios/tests/` covers it here.
use crate::support::{self, run_ai_turns};
use tor_protocol::{ActorId, StateView};
use tor_server::journal::Command;
use tor_server::journal::{Action, Direction};
use tor_server::{scenario_package, CommandResult, Engine, Scenario};

const TURNS: usize = 12;
/// The engine retains this many latest selectable states for rewinding
/// (`REWIND_BOUNDARIES`). Every actor's action is one, so a busy package can
/// play past its initial state within a few turns.
const REWIND_WINDOW: u64 = 128;
const DIRECTIONS: [Direction; 8] = [
    Direction::East,
    Direction::North,
    Direction::West,
    Direction::South,
    Direction::NorthEast,
    Direction::SouthWest,
    Direction::NorthWest,
    Direction::SouthEast,
];

fn states(engine: &Engine) -> Vec<(ActorId, StateView)> {
    engine
        .actors()
        .into_iter()
        .map(|actor| (actor, engine.state(actor).unwrap()))
        .collect()
}

fn disclosed(engine: &Engine) -> serde_json::Value {
    serde_json::to_value(
        engine
            .actors()
            .into_iter()
            .map(|actor| {
                (
                    actor,
                    engine.state(actor).unwrap(),
                    engine.history(actor, "player", None, 100).unwrap(),
                )
            })
            .collect::<Vec<_>>(),
    )
    .unwrap()
}

fn finished(engine: &Engine, character: ActorId) -> bool {
    let observation = engine.observation(character).unwrap();
    !engine.alive(character) || observation.combat.as_ref().is_some_and(|c| c.terminal)
}

/// Play the package's character for up to `turns` turns. Returns the last
/// accepted request so it can be retried.
fn play(
    engine: &mut Engine,
    character: ActorId,
    name: &str,
    turns: usize,
) -> Option<(String, Command, CommandResult)> {
    let mut last = None;
    for turn in 0..turns {
        run_ai_turns(engine);
        if finished(engine, character) {
            break;
        }
        let mut accepted = false;
        for action in [
            Action::Move {
                direction: DIRECTIONS[turn % DIRECTIONS.len()],
            },
            Action::Wait,
        ] {
            let before = states(engine);
            let request = format!("turn-{turn}-{action:?}");
            let command = Command::Act {
                expected_boundary: engine.causal_boundary(character).unwrap(),
                action: action.clone(),
            };
            let branch = engine.branch().clone();
            match engine.command(
                "player",
                "test",
                character,
                &request,
                &branch,
                command.clone(),
            ) {
                Ok(result) => {
                    last = Some((request, command, result));
                    accepted = true;
                    break;
                }
                Err(error) => assert_eq!(
                    states(engine),
                    before,
                    "{name}: rejected {action:?} ({error}) changed the game"
                ),
            }
        }
        if !accepted {
            break;
        }
    }
    last
}

#[test]
fn every_package_rejects_retries_replays_and_rewinds_exactly() {
    let packages = support::all_packages();
    assert!(
        packages.len() >= 30,
        "found only {} packages",
        packages.len()
    );
    for path in packages {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let scenario =
            scenario_package::load(&path, 7, None, false).unwrap_or_else(|e| panic!("{name}: {e}"));
        let character = ActorId(scenario.package.as_ref().unwrap().selected);
        let directory = tempfile::tempdir().unwrap();
        let save = directory.path().join("game.db");
        let mut engine = Engine::open(&save, scenario).unwrap();
        let initial = observations(&engine);
        let before_play = states(&engine);

        // Non-vacuous rejection, including a repeated failure under the same ID.
        let rejected_before = disclosed(&engine);
        let sequence = engine.save_status().accepted_sequence;
        let stale = Command::Act {
            expected_boundary: engine.causal_boundary(character).unwrap() + 1,
            action: Action::Wait,
        };
        for _ in 0..2 {
            let error = engine
                .command(
                    "player",
                    "test",
                    character,
                    "fresh-stale",
                    &engine.branch().clone(),
                    stale.clone(),
                )
                .unwrap_err();
            assert_eq!(error.code, tor_protocol::ErrorCode::StaleRevision, "{name}");
            assert_eq!(
                disclosed(&engine),
                rejected_before,
                "{name}: rejection changed disclosure/history"
            );
            assert_eq!(
                engine.save_status().accepted_sequence,
                sequence,
                "{name}: rejection consumed a sequence"
            );
        }

        // The public wire command resolves to a durable admission. Restart
        // before execution, recover its original receipt, then cancel it.
        let wire = tor_protocol::Command::Act {
            action: tor_protocol::Action::Wait,
        };
        let command = support::decode_command(&engine, character, &wire).unwrap();
        assert!(matches!(command, Command::AdmitIntention { .. }));
        let branch = engine.branch().clone();
        let admitted = engine
            .command(
                "player",
                "test",
                character,
                "queued",
                &branch,
                command.clone(),
            )
            .unwrap();
        assert_eq!(
            states(&engine),
            before_play,
            "{name}: admission executed early"
        );
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open(&save, Scenario::two_room(0)).unwrap();
        let retry = engine
            .command(
                "player",
                "reconnected",
                character,
                "queued",
                &branch,
                command,
            )
            .unwrap();
        assert!(retry.duplicate, "{name}: queued restart lost its receipt");
        assert_eq!(
            retry.entry, admitted.entry,
            "{name}: queued restart changed receipt"
        );
        assert_eq!(
            states(&engine),
            before_play,
            "{name}: queued restart executed early"
        );
        let cancel = support::decode_command(
            &engine,
            character,
            &tor_protocol::Command::CancelIntention {
                step: None,
                intention: tor_protocol::IntentionId(admitted.entry.id.0),
            },
        )
        .unwrap();
        engine
            .command(
                "player",
                "test",
                character,
                "cancel-queued",
                &branch,
                cancel,
            )
            .unwrap();
        assert_eq!(states(&engine), before_play, "{name}: cancellation acted");

        let (request, command, original) = play(&mut engine, character, &name, TURNS)
            .unwrap_or_else(|| panic!("{name}: the character could neither move nor wait"));
        let played = states(&engine);
        assert_ne!(played, before_play, "{name}: playing changed nothing");
        let branch = engine.branch().clone();
        let sequence = engine.save_status().accepted_sequence;
        let retry = engine
            .command(
                "player",
                "test",
                character,
                &request,
                &branch,
                command.clone(),
            )
            .unwrap();
        assert!(
            retry.duplicate,
            "{name}: retry must identify the original result"
        );
        assert_eq!(
            retry.entry, original.entry,
            "{name}: retry changed receipt identity/content"
        );
        assert_eq!(
            engine.save_status().accepted_sequence,
            sequence,
            "{name}: retry appended a record"
        );
        assert_eq!(states(&engine), played, "{name}: a retry acted again");

        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open(&save, Scenario::two_room(0)).unwrap();
        assert_eq!(
            states(&engine),
            played,
            "{name}: the restart replayed differently"
        );

        let retry = engine
            .command(
                "player",
                "reconnected",
                character,
                &request,
                &branch,
                command,
            )
            .unwrap();
        assert!(
            retry.duplicate,
            "{name}: persisted retry was not recognized"
        );
        assert_eq!(
            retry.entry, original.entry,
            "{name}: persisted receipt changed"
        );

        engine.enable_wizard().unwrap();
        let sequence = engine.save_status().accepted_sequence;
        let current = states(&engine);
        match support::wizard(&mut engine, character, "rewind initial") {
            Ok(_) => assert_eq!(
                observations(&engine),
                initial,
                "{name}: rewinding didn't restore the start"
            ),
            Err(error) => {
                // Only history longer than the retained window may drop the
                // start, and refusing to rewind to it changes nothing.
                assert!(
                    sequence > REWIND_WINDOW,
                    "{name}: rewind failed within the retained window: {error}"
                );
                assert_eq!(error.code, tor_protocol::ErrorCode::InvalidAction, "{name}");
                assert_eq!(states(&engine), current, "{name}: a refused rewind acted");
                assert_eq!(
                    engine.save_status().accepted_sequence,
                    sequence,
                    "{name}: a refused rewind consumed a sequence"
                );
                // A shorter game stays within the window and rewinds exactly.
                let scenario = scenario_package::load(&path, 7, None, false).unwrap();
                let directory = tempfile::tempdir().unwrap();
                let mut short = Engine::open(directory.path().join("short.db"), scenario).unwrap();
                let initial = observations(&short);
                play(&mut short, character, &name, 1)
                    .unwrap_or_else(|| panic!("{name}: the short game could not act"));
                assert!(
                    short.save_status().accepted_sequence <= REWIND_WINDOW,
                    "{name}: one turn outgrew the rewind window"
                );
                short.enable_wizard().unwrap();
                support::wizard(&mut short, character, "rewind initial")
                    .unwrap_or_else(|e| panic!("{name}: short rewind failed: {e}"));
                assert_eq!(
                    observations(&short),
                    initial,
                    "{name}: rewinding the short game didn't restore the start"
                );
            }
        }
    }
}

fn observations(engine: &Engine) -> Vec<(ActorId, tor_protocol::Observation)> {
    states(engine)
        .into_iter()
        .map(|(id, state)| (id, state.observation))
        .collect()
}
