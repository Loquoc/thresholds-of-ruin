//! Coexisting planes through the engine: a script shifts the character onto
//! a derived Ethereal region, the plane's movement mode carries it across
//! zero gravity, the return action brings it home, and replay reproduces it.
use crate::support::{self, act, CHARACTER};
use tempfile::tempdir;
use tor_server::journal::{Action, Direction};
use tor_server::Engine;

const NAVE: u64 = 1;
const GREY_NAVE: u64 = 1001;

fn region(engine: &Engine) -> Option<u64> {
    engine.palette_region(CHARACTER)
}

#[test]
fn waiting_at_the_altar_shifts_to_the_derived_region_and_returning_comes_home() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("planes.json");
    let mut engine = Engine::open(&path, support::load("planes", 7)).unwrap();
    // Both members of the nave's overlap group are active from the start.
    let counts = engine.region_counts().unwrap();
    assert!(counts.active >= 2, "{counts:?}");
    assert_eq!(region(&engine), Some(NAVE));

    act(
        &mut engine,
        Action::Move {
            direction: Direction::East,
        },
    )
    .unwrap();
    let home = engine.observation(CHARACTER).unwrap().plane.unwrap();
    assert!(home.plane.known);
    assert_eq!(home.plane.description, "the material world");
    assert!(home.echoes.is_empty(), "the material plane has no channel");
    act(&mut engine, Action::Wait).unwrap();
    assert_eq!(region(&engine), Some(GREY_NAVE), "the altar script shifted");
    // On the Ethereal Plane the observer is told the plane's description,
    // mysterious or revealed by the arrival Lore check, and sees the material
    // nave as an echo with its own cell keys.
    let away = engine.observation(CHARACTER).unwrap().plane.unwrap();
    assert_ne!(away.plane.key, home.plane.key);
    assert_eq!(
        away.plane.description,
        if away.plane.known {
            "the Border Ethereal"
        } else {
            "a grey, sourceless mist in which walls are only suggestions"
        }
    );
    let echo = &away.echoes[0];
    assert_eq!(echo.plane.key, home.plane.key);
    assert!(!echo.cells.is_empty());
    assert!(echo
        .cells
        .iter()
        .any(|cell| cell.position == tor_protocol::Position { x: 0, y: 0, z: 0 }));

    // Zero gravity would strand a walker; ether-gliding lets every move be
    // attempted. A failed check spends the move's time without moving. The
    // derived room has no shell, so two steps west reach its edge at most.
    for _ in 0..2 {
        act(
            &mut engine,
            Action::Move {
                direction: Direction::West,
            },
        )
        .unwrap();
    }
    assert_eq!(region(&engine), Some(GREY_NAVE));

    act(&mut engine, Action::ReturnPlane).unwrap();
    assert_eq!(region(&engine), Some(NAVE));
    // Home again: no stay to return from.
    assert!(act(&mut engine, Action::ReturnPlane).is_err());
    engine.flush().unwrap();
    drop(engine);

    // Replay reproduces the shift, the checks and the return exactly.
    let reopened = Engine::open(&path, support::load("planes", 7)).unwrap();
    assert_eq!(region(&reopened), Some(NAVE));
}

#[test]
fn derived_regions_are_rejected_when_they_collide_with_authored_regions() {
    use tor_server::scenario_package::planes::{DeriveDecl, PlaneDecl};
    let package = support::package("planes");
    let mut manifest: toml::Value =
        toml::from_str(&std::fs::read_to_string(package.join("scenario.toml")).unwrap()).unwrap();
    let derive = manifest["planes"]["ethereal"]["derive"]
        .as_array_mut()
        .unwrap();
    // Deriving onto an authored region's identity collides.
    derive.push(
        toml::Value::try_from(DeriveDecl {
            from: 2,
            id: 1,
            volume: None,
        })
        .unwrap(),
    );
    let plane: PlaneDecl = manifest["planes"]["ethereal"].clone().try_into().unwrap();
    assert_eq!(plane.derive.len(), 3);
    let dir = tempdir().unwrap();
    support::copy_package(&package, dir.path());
    std::fs::write(
        dir.path().join("scenario.toml"),
        toml::to_string(&manifest).unwrap(),
    )
    .unwrap();
    let error = tor_server::scenario_package::validate(dir.path())
        .err()
        .expect("a colliding derived region is rejected");
    assert!(error.to_string().contains("collides"), "{error}");
}

#[test]
fn drinking_veil_incense_shifts_through_the_plane_shift_effect() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("incense.json");
    let mut engine = Engine::open(&path, support::load("planes", 7)).unwrap();
    // Walk east through the nave into the vestry and pick up the incense.
    for _ in 0..6 {
        act(
            &mut engine,
            Action::Move {
                direction: Direction::East,
            },
        )
        .unwrap();
    }
    assert_eq!(region(&engine), Some(2));
    act(
        &mut engine,
        Action::Take {
            item: 1,
            quantity: None,
        },
    )
    .unwrap();
    // Item work is admitted as an intention and executes when due.
    let incense = engine.observation(CHARACTER).unwrap().inventory[0]
        .id
        .clone();
    let action = engine
        .decode_action(CHARACTER, &tor_protocol::Action::Drink { item: incense })
        .unwrap();
    let expected_boundary = engine.causal_boundary(CHARACTER).unwrap();
    support::submit(
        &mut engine,
        CHARACTER,
        tor_server::journal::Command::AdmitIntention {
            expected_boundary,
            action,
        },
    )
    .unwrap();
    for _ in 0..20 {
        if region(&engine) == Some(1002) {
            break;
        }
        if engine.execute_next_intention().unwrap().is_none() {
            act(&mut engine, Action::Wait).unwrap();
        }
    }
    assert_eq!(
        region(&engine),
        Some(1002),
        "the incense shifted the drinker"
    );
    act(&mut engine, Action::ReturnPlane).unwrap();
    assert_eq!(region(&engine), Some(2));
}

/// Every journaled script message, in order.
fn messages(archive: &serde_json::Value) -> Vec<String> {
    let mut reports: Vec<&serde_json::Value> = archive.get("initial_hooks").into_iter().collect();
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
                .filter_map(|segment| segment.get("text").and_then(|t| t.as_str()))
                .collect::<String>()
        })
        .collect()
}

/// Let AI actors take their turns, then act for the character.
fn play(
    engine: &mut Engine,
    action: Action,
) -> Result<tor_server::CommandResult, tor_server::Failure> {
    for _ in 0..64 {
        match engine.next_actor() {
            Some(next) if next != CHARACTER && engine.is_ai(next) => {
                engine.advance_ai(next).unwrap();
            }
            _ => break,
        }
    }
    act(engine, action)
}

#[test]
fn chapel_ghosts_strike_across_the_planes_and_glimpses_allow_a_strike_back() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("chapel.json");
    let mut engine = Engine::open(&path, support::load("haunted-chapel", 7)).unwrap();
    for _ in 0..3 {
        play(
            &mut engine,
            Action::Move {
                direction: Direction::East,
            },
        )
        .unwrap();
    }
    assert_eq!(region(&engine), Some(2), "the pilgrim is in the nave");
    let full = engine
        .state(CHARACTER)
        .unwrap()
        .observation
        .combat
        .unwrap()
        .max_hp;
    let mut struck_back = false;
    for _ in 0..80 {
        if !engine.alive(CHARACTER) {
            break;
        }
        let state = engine.state(CHARACTER).unwrap();
        let ghost = state
            .observation
            .visible_actors
            .iter()
            .find(|seen| seen.id != state.observation.self_target)
            .map(|seen| seen.id.clone());
        if let Some(ghost) = ghost.filter(|_| !struck_back) {
            // A glimpse shows the ghost as an ordinary target; a plain strike
            // at it is admitted, though it cannot harm the ghost.
            let action = engine
                .decode_action(
                    CHARACTER,
                    &tor_protocol::Action::UseAbility {
                        id: "basic_melee".into(),
                        invocation: tor_protocol::AbilityInvocation {
                            aim: tor_protocol::AbilityAim::ActorTarget { target: ghost },
                            weapon: tor_protocol::WeaponSelection::Default,
                        },
                        supplies: Default::default(),
                    },
                )
                .unwrap();
            match play(&mut engine, action) {
                Ok(_) => {
                    struck_back = true;
                    continue;
                }
                Err(_) => {} // Out of reach this turn: wait for it.
            }
        }
        if play(&mut engine, Action::Wait).is_err() {
            break;
        }
    }
    let hurt = engine
        .state(CHARACTER)
        .unwrap()
        .observation
        .combat
        .map_or(0, |combat| combat.hp)
        < full;
    engine.flush().unwrap();
    drop(engine);
    let said = messages(&support::read(&path));
    let touches = said
        .iter()
        .filter(|text| {
            text.starts_with("For a heartbeat you see it") || text.starts_with("Something unseen")
        })
        .count();
    assert!(touches > 0, "the ghost struck across the planes: {said:?}");
    assert!(hurt || touches > 0);
    assert!(said.contains(&"The chapel is colder than the night outside.".to_owned()));
    if said.iter().any(|text| text.starts_with("For a heartbeat")) {
        assert!(struck_back, "a glimpse made the ghost a target");
    }
    // Replay reproduces the haunting exactly.
    Engine::open(&path, support::load("haunted-chapel", 7)).unwrap();
}

#[test]
fn generated_family_regions_have_shadows_in_the_family_block() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("generated.json");
    let mut engine = Engine::open(&path, support::load("planes-generated", 7)).unwrap();
    // No authored regions: the family's block starts at 1. One slot per depth
    // over two depths is regions 1-2; the Ethereal shadow span is 3-4.
    let shadow = region(&engine).unwrap();
    assert!(
        (3..=4).contains(&shadow),
        "the run started on the plane: {shadow}"
    );
    let view = engine.observation(CHARACTER).unwrap().plane.unwrap();
    assert!(!view.echoes.is_empty() && !view.echoes[0].cells.is_empty());
    play(&mut engine, Action::ReturnPlane).unwrap();
    assert_eq!(
        region(&engine),
        Some(shadow - 2),
        "home to the generated room"
    );
    engine.flush().unwrap();
    drop(engine);
    let reopened = Engine::open(&path, support::load("planes-generated", 7)).unwrap();
    assert_eq!(region(&reopened), Some(shadow - 2));
}
