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
