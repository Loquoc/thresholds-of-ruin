//! Scenario scripts through the engine: hooks fire at real boundaries, their
//! effects and messages are journaled, replay reproduces them exactly, and
//! certificates pin the script text.
use crate::support::{self, act, CHARACTER};
use serde_json::Value;
use tempfile::tempdir;
use tor_server::journal::{Action, Direction};
use tor_server::{scenario_package, Engine, Scenario};

fn step(engine: &mut Engine, direction: Direction, times: usize) {
    for _ in 0..times {
        act(engine, Action::Move { direction }).unwrap();
    }
}

/// Every journaled script message, in order, as plain text with controls
/// shown as `<id>`.
fn messages(archive: &Value) -> Vec<String> {
    let mut reports: Vec<&Value> = archive.get("initial_hooks").into_iter().collect();
    for record in archive["records"].as_array().unwrap() {
        reports.extend(record.get("hooks"));
    }
    reports
        .into_iter()
        .flat_map(|report| report["messages"].as_array().unwrap())
        .map(|message| {
            message["segments"]
                .as_array()
                .unwrap()
                .iter()
                .map(
                    |segment| match (segment.get("text"), segment.get("control")) {
                        (Some(text), _) => text.as_str().unwrap().to_owned(),
                        (_, Some(control)) => format!("<{}>", control.as_str().unwrap()),
                        _ => panic!("unknown segment {segment}"),
                    },
                )
                .collect::<String>()
        })
        .collect()
}

fn failures(archive: &Value) -> usize {
    archive["records"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|record| record.get("hooks"))
        .chain(archive.get("initial_hooks"))
        .map(|report| report["failures"].as_array().map_or(0, Vec::len))
        .sum()
}

fn open_door(engine: &mut Engine) {
    let view = engine.observation(CHARACTER).unwrap();
    let door = view
        .visible_cells
        .iter()
        .find_map(|cell| cell.door.as_ref())
        .expect("the hall's door is visible")
        .id;
    support::act_disclosed(
        engine,
        CHARACTER,
        tor_protocol::Action::SetDoor { door, open: true },
    )
    .unwrap();
}

fn victory(engine: &Engine) -> bool {
    engine
        .state(CHARACTER)
        .unwrap()
        .observation
        .combat
        .as_ref()
        .is_some_and(|combat| combat.victory)
}

#[test]
fn tutorial_lessons_follow_play_and_replay_exactly() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("tutorial.json");
    let mut engine = Engine::open(&path, support::load("tutorial", 7)).unwrap();
    step(&mut engine, Direction::East, 2);
    act(
        &mut engine,
        Action::Take {
            item: 1,
            quantity: None,
        },
    )
    .unwrap();
    step(&mut engine, Direction::East, 2);
    assert!(!victory(&engine));
    open_door(&mut engine);
    step(&mut engine, Direction::East, 6);
    let won = victory(&engine);
    engine.flush().unwrap();
    drop(engine);

    let archive = support::read(&path);
    assert_eq!(
        messages(&archive),
        [
            "Welcome to the practice hall. Walk east with <move>.",
            "Whenever you are unsure what to do, <help> lists your commands.",
            "Something small lies here. Pick it up with <pick_up>.",
            "Your <inventory> shows everything you carry.",
            "A closed door blocks the way. Open it with <open_door>.",
            "Doors stay as you leave them. The way out is at the far end of the hall.",
            "You found the token and the way out. That is the whole lesson.",
        ]
    );
    assert_eq!(failures(&archive), 0);
    assert!(won, "the exit lesson ends the run");

    // Reopening replays every record and requires identical hook calls,
    // work and effects; the restored game keeps its script state.
    let reopened = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert!(victory(&reopened));
}

#[test]
fn the_exit_waits_for_the_token_and_the_timer_nudges_idle_players() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("idle.json");
    let mut engine = Engine::open(&path, support::load("tutorial", 7)).unwrap();
    // Ten waits reach the nudge timer before any lesson has advanced.
    for _ in 0..10 {
        act(&mut engine, Action::Wait).unwrap();
    }
    engine.flush().unwrap();
    let archive = support::read(&path);
    let said = messages(&archive);
    assert_eq!(
        said.last().map(String::as_str),
        Some("Try <move> toward the east. You can also <wait> to let time pass.")
    );
    assert_eq!(said.len(), 3, "the nudge fires once: {said:?}");
}

#[test]
fn unscripted_saves_carry_no_script_records() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("plain.json");
    let mut engine = Engine::open(&path, support::load("doors", 3)).unwrap();
    act(&mut engine, Action::Wait).unwrap();
    engine.flush().unwrap();
    let archive = support::read(&path);
    assert!(archive.get("initial_hooks").is_none());
    assert!(archive["records"]
        .as_array()
        .unwrap()
        .iter()
        .all(|record| record.get("hooks").is_none()));
}

#[test]
fn certificates_pin_script_text_and_validation_rejects_broken_scripts() {
    let dir = tempdir().unwrap();
    let package = dir.path().join("tutorial");
    support::copy_package(&support::package("tutorial"), &package);
    let script = package.join("scripts/lessons.luau");
    let text = std::fs::read_to_string(&script).unwrap();
    std::fs::write(&script, text.replace("Welcome", "Hello")).unwrap();
    let stale = scenario_package::load(&package, 0, None, false);
    assert!(
        stale.is_err(),
        "an edited script makes the certificate stale"
    );
    // Revalidating the edited package succeeds; a broken one is refused.
    scenario_package::validate(&package).unwrap();
    std::fs::write(&script, "return {").unwrap();
    let error = scenario_package::validate(&package)
        .unwrap_err()
        .to_string();
    assert!(error.contains("lessons"), "{error}");
    std::fs::write(&script, &text).unwrap();
    let manifest = package.join("scenario.toml");
    let manifest_text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        manifest_text.replace(
            "handler = \"lessons.exit\"",
            "handler = \"lessons.missing\"",
        ),
    )
    .unwrap();
    let error = scenario_package::validate(&package)
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown handler"), "{error}");
}
