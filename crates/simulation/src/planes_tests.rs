//! Plane shifting, stays, returns and the plane movement mode, through the
//! public action and setup boundaries.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

use tor_world::{Direction, Extent, Location, Overlap, PlaneId, Position, Region, RegionId, World};

use super::*;
use crate::checkpoint::SharedState;
use crate::{Action, Game, GameError, OutcomeKind};

const ETHEREAL: PlaneId = PlaneId(1);

fn cell(region: u64, x: i32, y: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z: 0 },
    }
}

fn spec(glide: Option<i32>) -> PlaneSpec {
    PlaneSpec {
        name: "ethereal".into(),
        mysterious: "a grey mist".into(),
        revealed: "the Ethereal Plane".into(),
        channels: vec![ChannelSpec {
            to: PlaneId::MATERIAL,
            range: 12,
        }],
        glide: glide.map(|dc| GlideRule {
            skill: crate::attributes::Skill::Discipline,
            dc,
        }),
    }
}

/// Material room 1 and Ethereal room 2, 6x4x2 each, the same place over
/// x = 0..=3 only. The Ethereal room has explicit zero gravity.
fn game(glide: Option<i32>) -> (Game, ActorId) {
    let region = |id| Region {
        id: RegionId(id),
        name: format!("room {id}"),
        bounds: Extent::new(6, 4, 2).unwrap(),
    };
    let world = World::new(vec![region(1), region(2)], vec![]).unwrap();
    let mut game = Game::new(world, 42, crate::test_creatures::content());
    game.install_planes(BTreeMap::from([(ETHEREAL, spec(glide))]))
        .unwrap();
    game.set_region_plane(RegionId(2), ETHEREAL).unwrap();
    game.world.set_gravity(RegionId(2), [0, 0, 0]).unwrap();
    game.add_overlap(Overlap {
        from: RegionId(1),
        min: Position { x: 0, y: 0, z: 0 },
        max: Position { x: 3, y: 3, z: 1 },
        to: RegionId(2),
        origin: Position { x: 0, y: 0, z: 0 },
        rotation: 0,
        scale: 1,
    })
    .unwrap();
    let actor = game
        .spawn_actor(cell(1, 1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    (game, actor)
}

fn place(game: &Game, actor: ActorId) -> Option<Location> {
    game.actors.get(&actor).map(|a| a.location)
}

fn creature(game: &mut Game, actor: ActorId) {
    crate::test_creatures::configure(game, actor, "party", crate::test_creatures::species());
}

#[test]
fn a_shift_moves_to_the_counterpart_records_home_and_returns_by_action() {
    let (mut game, actor) = game(Some(0));
    let shifted = game.shift_actor(actor, ShiftRequest::to(ETHEREAL)).unwrap();
    assert_eq!(
        shifted,
        ShiftResult::Shifted {
            from: cell(1, 1, 1),
            to: cell(2, 1, 1)
        }
    );
    assert_eq!(game.actor_plane(actor), Some(ETHEREAL));
    assert_eq!(
        game.plane_stay(actor),
        Some(PlaneStay {
            home: PlaneId::MATERIAL,
            returns: ReturnPolicy::Action,
            expires_at: None,
        })
    );
    let outcome = game.act(actor, Action::ReturnPlane).unwrap();
    assert_eq!(
        outcome.kind,
        OutcomeKind::PlaneReturned {
            from: cell(2, 1, 1),
            to: cell(1, 1, 1)
        }
    );
    // A return is an ordinary timed action.
    assert_eq!(outcome.next_tick, 100);
    assert_eq!(place(&game, actor), Some(cell(1, 1, 1)));
    assert_eq!(game.plane_stay(actor), None);
    // Home again: there is nothing to return from.
    assert_eq!(
        game.act(actor, Action::ReturnPlane).unwrap_err(),
        GameError::NoPlaneReturn
    );
}

#[test]
fn shifts_without_a_counterpart_or_room_are_rejected_and_move_nothing() {
    let (mut game, actor) = game(Some(0));
    // Outside the overlap's volume there is no counterpart.
    game.teleport(actor, cell(1, 5, 1)).unwrap();
    assert_eq!(
        game.shift_actor(actor, ShiftRequest::to(ETHEREAL)).unwrap(),
        ShiftResult::Blocked
    );
    assert_eq!(place(&game, actor), Some(cell(1, 5, 1)));
    assert_eq!(game.plane_stay(actor), None);
    // An occupied counterpart blocks the shift too.
    game.teleport(actor, cell(1, 2, 2)).unwrap();
    let other = game
        .spawn_actor(cell(2, 2, 2), NonZeroU64::new(100).unwrap())
        .unwrap();
    assert_eq!(
        game.shift_actor(actor, ShiftRequest::to(ETHEREAL)).unwrap(),
        ShiftResult::Blocked
    );
    assert_eq!(place(&game, actor), Some(cell(1, 2, 2)));
    assert_eq!(place(&game, other), Some(cell(2, 2, 2)));
    // Shifting to the plane one is already on is not a shift.
    assert_eq!(
        game.shift_actor(other, ShiftRequest::to(ETHEREAL)).unwrap(),
        ShiftResult::Blocked
    );
}

#[test]
fn a_return_into_solid_material_is_rejected_without_spending_time() {
    let (mut game, actor) = game(Some(0));
    game.shift_actor(actor, ShiftRequest::to(ETHEREAL)).unwrap();
    game.world.set_wall(cell(1, 1, 1), true).unwrap();
    assert_eq!(
        game.act(actor, Action::ReturnPlane).unwrap_err(),
        GameError::PlaneShiftBlocked
    );
    assert_eq!(place(&game, actor), Some(cell(2, 1, 1)));
    assert_eq!(game.tick(), 0);
    assert!(game.plane_stay(actor).is_some());
}

#[test]
fn expiry_only_stays_end_at_the_next_boundary_after_their_duration() {
    let (mut game, actor) = game(Some(0));
    game.shift_actor(
        actor,
        ShiftRequest {
            plane: ETHEREAL,
            returns: ReturnPolicy::ExpiryOnly,
            duration: NonZeroU64::new(150),
            reveals: false,
        },
    )
    .unwrap();
    assert_eq!(
        game.act(actor, Action::ReturnPlane).unwrap_err(),
        GameError::NoPlaneReturn
    );
    game.act(actor, Action::Wait).unwrap();
    // Tick 0: the first wait's boundary is before expiry.
    assert_eq!(game.actor_plane(actor), Some(ETHEREAL));
    game.act(actor, Action::Wait).unwrap();
    // Tick 100: still before expiry at 150.
    assert_eq!(game.actor_plane(actor), Some(ETHEREAL));
    game.act(actor, Action::Wait).unwrap();
    // Tick 200: past expiry, so the forced return happened.
    assert_eq!(game.actor_plane(actor), Some(PlaneId::MATERIAL));
    assert_eq!(game.plane_stay(actor), None);
}

#[test]
fn a_blocked_forced_return_keeps_the_stay_and_retries() {
    let (mut game, actor) = game(Some(0));
    game.shift_actor(
        actor,
        ShiftRequest {
            plane: ETHEREAL,
            returns: ReturnPolicy::None,
            duration: NonZeroU64::new(1),
            reveals: false,
        },
    )
    .unwrap();
    game.world.set_wall(cell(1, 1, 1), true).unwrap();
    game.act(actor, Action::Wait).unwrap();
    game.act(actor, Action::Wait).unwrap();
    assert_eq!(game.actor_plane(actor), Some(ETHEREAL));
    assert!(game.plane_stay(actor).is_some());
    game.world.set_wall(cell(1, 1, 1), false).unwrap();
    game.act(actor, Action::Wait).unwrap();
    assert_eq!(game.actor_plane(actor), Some(PlaneId::MATERIAL));
}

#[test]
fn the_plane_movement_mode_lets_walkers_move_unsupported_after_a_check() {
    // Without a movement mode, zero gravity strands a walker.
    let (mut stranded, actor) = game(None);
    creature(&mut stranded, actor);
    stranded
        .shift_actor(actor, ShiftRequest::to(ETHEREAL))
        .unwrap();
    assert_eq!(
        stranded
            .act(actor, Action::Move(Direction::East))
            .unwrap_err(),
        GameError::StepNeedsSupport
    );

    // An easy check always passes.
    let (mut easy, actor) = game(Some(0));
    creature(&mut easy, actor);
    easy.shift_actor(actor, ShiftRequest::to(ETHEREAL)).unwrap();
    let before = crate::diagnostics::work_counts().plane_glide_checks;
    let moved = easy.act(actor, Action::Move(Direction::East)).unwrap();
    assert!(matches!(moved.kind, OutcomeKind::Moved { .. }));
    assert_eq!(place(&easy, actor), Some(cell(2, 2, 1)));
    assert_eq!(
        crate::diagnostics::work_counts().plane_glide_checks,
        before + 1
    );

    // An impossible check always fails: time passes, the actor stays put.
    let (mut hard, actor) = game(Some(crate::planes::MAX_PLANE_DC));
    creature(&mut hard, actor);
    hard.shift_actor(actor, ShiftRequest::to(ETHEREAL)).unwrap();
    let failed = hard.act(actor, Action::Move(Direction::East)).unwrap();
    assert_eq!(failed.kind, OutcomeKind::Waited);
    assert_eq!(place(&hard, actor), Some(cell(2, 1, 1)));
    assert!(failed.next_tick > 0);
}

#[test]
fn overlap_groups_are_active_and_loaded_together() {
    let (game, _) = game(Some(0));
    let settled = game.settle_region_transition(&crate::streaming::RegionTransition {
        active: BTreeSet::from([RegionId(1)]),
        loaded: BTreeSet::from([RegionId(1)]),
    });
    assert!(settled.active.contains(&RegionId(2)));
    assert!(settled.loaded.contains(&RegionId(2)));
}

#[test]
fn plane_state_round_trips_and_single_plane_games_save_none() {
    let (mut game, actor) = game(Some(15));
    game.shift_actor(actor, ShiftRequest::to(ETHEREAL)).unwrap();
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let json = serde_json::to_value(&snapshot).unwrap();
    assert!(json.get("planes").is_some());
    let restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(restored.plane_stay(actor), game.plane_stay(actor));
    assert_eq!(restored.actor_plane(actor), Some(ETHEREAL));
    assert_eq!(restored.world.overlaps(), game.world.overlaps());

    let mut plain = Game::two_room(42, crate::test_creatures::content());
    plain
        .spawn_actor(cell(1, 1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let json = serde_json::to_value(plain.checkpoint(&mut SharedState::default())).unwrap();
    assert!(json.get("planes").is_none());
}

#[test]
fn plane_rules_reject_malformed_declarations() {
    let (mut game, _) = game(Some(0));
    for bad in [
        BTreeMap::from([(PlaneId::MATERIAL, spec(None))]),
        BTreeMap::from([(
            ETHEREAL,
            PlaneSpec {
                name: "material".into(),
                mysterious: String::new(),
                revealed: String::new(),
                glide: None,
                channels: Vec::new(),
            },
        )]),
        BTreeMap::from([(ETHEREAL, spec(Some(MAX_PLANE_DC + 1)))]),
        BTreeMap::from([(PlaneId(1), spec(None)), (PlaneId(2), spec(None))]),
    ] {
        assert_eq!(game.install_planes(bad), Err(GameError::InvalidLocation));
    }
    assert_eq!(game.plane_by_name("ethereal"), Some(ETHEREAL));
    assert_eq!(game.plane_by_name("material"), Some(PlaneId::MATERIAL));
    assert_eq!(game.plane_name(ETHEREAL), Some("ethereal"));
}

#[test]
fn two_cell_bodies_glide_in_zero_gravity() {
    let (mut game, actor) = game(Some(0));
    game.world.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    game.set_body(
        actor,
        crate::BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 1],
            mass: 80,
        },
    )
    .unwrap();
    creature(&mut game, actor);
    game.shift_actor(actor, ShiftRequest::to(ETHEREAL)).unwrap();
    let moved = game.act(actor, Action::Move(Direction::West));
    assert!(moved.is_ok(), "{moved:?}");
}

#[test]
fn the_channel_shows_the_material_plane_from_the_counterpart_eye() {
    let (mut game, actor) = game(Some(0));
    let walker = game
        .spawn_actor(cell(1, 3, 2), NonZeroU64::new(100).unwrap())
        .unwrap();
    creature(&mut game, walker);
    game.shift_actor(actor, ShiftRequest::to(ETHEREAL)).unwrap();
    let views = game.plane_channel_views(actor);
    assert_eq!(views.len(), 1);
    let view = &views[0];
    assert_eq!(view.plane, PlaneId::MATERIAL);
    // Every echo cell is a material cell, relative to the observer's feet.
    assert!(view.cells.iter().all(|c| c.location.region == RegionId(1)));
    assert!(view
        .cells
        .iter()
        .any(|c| c.location == cell(1, 1, 1) && c.offset == Position { x: 0, y: 0, z: 0 }));
    // The material walker is seen across the planes, two east and one north.
    assert!(
        view.actors
            .iter()
            .any(|a| a.actor == walker && a.offset == Position { x: 2, y: 1, z: 0 }),
        "{:?}",
        view.actors
    );
    // A material observer has no channel in this fixture.
    assert!(game.plane_channel_views(walker).is_empty());
}

#[test]
fn an_eye_inside_solid_material_sees_nothing_of_it() {
    let (mut game, actor) = game(Some(0));
    game.shift_actor(actor, ShiftRequest::to(ETHEREAL)).unwrap();
    game.world.set_wall(cell(1, 1, 1), true).unwrap();
    let views = game.plane_channel_views(actor);
    assert!(views[0].cells.is_empty());
}

#[test]
fn plane_knowledge_changes_the_description_and_survives_saves() {
    let game_fixture = game;
    let (mut game, actor) = game_fixture(Some(0));
    game.shift_actor(actor, ShiftRequest::to(ETHEREAL)).unwrap();
    let seen = game.plane_observation(actor).unwrap();
    assert!(!seen.known);
    assert_eq!(seen.description, "a grey mist");
    assert_eq!(seen.echoes[0].2, "the material world");
    assert!(game.reveal_plane(actor, ETHEREAL));
    assert!(!game.reveal_plane(actor, ETHEREAL), "already known");
    let known = game.plane_observation(actor).unwrap();
    assert!(known.known);
    assert_eq!(known.description, "the Ethereal Plane");
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert!(restored.knows_plane(actor, ETHEREAL));

    // A revealing shift teaches the plane on arrival.
    let (mut taught, other) = game_fixture(Some(0));
    taught
        .shift_actor(
            other,
            ShiftRequest {
                reveals: true,
                ..ShiftRequest::to(ETHEREAL)
            },
        )
        .unwrap();
    assert!(taught.knows_plane(other, ETHEREAL));
}
