use super::*;
use crate::{behavior::Engagement, perception::ActorReferenceKind, test_creatures};
use std::{collections::BTreeSet, num::NonZeroU64};
use tor_world::{Extent, Passage, Position, Region, RegionId, World};

#[derive(Clone, Copy, Debug)]
enum Corridor {
    Unsplit,
    Split,
    Rotated,
}

impl Corridor {
    fn location(self, index: i32) -> Location {
        let (region, x, y) = match self {
            Self::Unsplit => (1, index, 0),
            Self::Split if index >= 3 => (2, index - 3, 0),
            Self::Rotated if index >= 3 => (2, 0, index - 3),
            _ => (1, index, 0),
        };
        Location {
            region: RegionId(region),
            position: Position { x, y, z: 0 },
        }
    }

    fn game(self, actor_index: i32, threat_index: i32) -> (Game, ActorId, ActorId) {
        self.game_with_reverse(actor_index, threat_index, true)
    }

    fn game_with_reverse(
        self,
        actor_index: i32,
        threat_index: i32,
        reverse: bool,
    ) -> (Game, ActorId, ActorId) {
        let mut world = World::new(vec![], vec![]).unwrap();
        let extents = match self {
            Self::Unsplit => vec![(1, 6, 1)],
            Self::Split => vec![(1, 3, 1), (2, 3, 1)],
            Self::Rotated => vec![(1, 3, 1), (2, 1, 3)],
        };
        for (id, width, depth) in extents {
            world
                .add_region(Region {
                    id: RegionId(id),
                    name: "corridor".into(),
                    bounds: Extent::new(width, depth, 1).unwrap(),
                })
                .unwrap();
        }
        if !matches!(self, Self::Unsplit) {
            let turns = if matches!(self, Self::Rotated) { 1 } else { 0 };
            world
                .connect(
                    Passage {
                        from: self.location(2),
                        direction: Direction::East,
                        to: self.location(3),
                    },
                    turns,
                )
                .unwrap();
            if reverse {
                world
                    .connect(
                        Passage {
                            from: self.location(3),
                            direction: Direction::West.rotated(turns),
                            to: self.location(2),
                        },
                        tor_world::inverse_rotation(turns),
                    )
                    .unwrap();
            }
        }
        let mut species = test_creatures::species();
        species.behavior.memory_ticks = 50;
        let content = test_creatures::content_with(species.clone(), vec![]);
        let mut game = Game::new(world, 42, content);
        let threat = game
            .spawn_actor(self.location(threat_index), NonZeroU64::new(100).unwrap())
            .unwrap();
        let actor = game
            .spawn_actor(self.location(actor_index), NonZeroU64::new(100).unwrap())
            .unwrap();
        for (id, faction) in [(threat, "hero"), (actor, "foe")] {
            test_creatures::configure(&mut game, id, faction, species.clone());
        }
        game.configure_run(
            threat,
            BTreeSet::from([threat]),
            BTreeMap::from([("foe".into(), BTreeSet::from(["hero".into()]))]),
        )
        .unwrap();
        game.configure_ai(
            actor,
            AiProfile {
                flee_percent: 100,
                ..AiProfile::default()
            },
        )
        .unwrap();
        game.actors.get_mut(&actor).unwrap().behavior.engagement = Some(Engagement {
            target: threat,
            reference: ActorReferenceKind::Creature,
            location: self.location(threat_index),
            last_seen: 0,
        });
        game.refresh_navigation();
        let view = game.observe(actor).unwrap();
        assert!(view.visible_actors.iter().any(|other| other.id == threat));
        assert_eq!(game.actors[&actor].body.cells.len(), 1);
        assert!(game.actors[&actor].locomotion().erratic.is_none());
        assert!(game.preparation(actor).is_none());
        (game, actor, threat)
    }
}

#[test]
fn cornered_flee_does_not_approach_a_threat_across_region_partitions() {
    for corridor in [Corridor::Unsplit, Corridor::Split, Corridor::Rotated] {
        let (game, actor, _) = corridor.game(0, 5);
        let route = game.travel_route(actor, corridor.location(5)).unwrap();
        assert_eq!(route.len(), 5, "{corridor:?}");
        let legal: Vec<_> = Direction::HORIZONTAL
            .into_iter()
            .chain([Direction::Up, Direction::Down])
            .filter(|direction| game.actor_translation(actor, *direction).is_some())
            .collect();
        assert_eq!(
            legal,
            [Direction::East],
            "the sole step approaches the threat"
        );
        let before = game.clone();
        let decision = game.choose_ai(actor).unwrap();
        assert_eq!(decision.ai.state, State::Flee, "{corridor:?}");
        assert_eq!(decision.action, Action::Wait, "{corridor:?}");
        assert_eq!(game, before, "a decision must not mutate actor knowledge");
    }
}

#[test]
fn fleeing_retains_a_disclosed_retreat_through_a_rotated_portal() {
    for corridor in [Corridor::Unsplit, Corridor::Split, Corridor::Rotated] {
        let (game, actor, _) = corridor.game(2, 0);
        assert_eq!(
            game.travel_route(actor, corridor.location(0))
                .unwrap()
                .len(),
            2
        );
        let (destination, orientation) = game.actor_translation(actor, Direction::East).unwrap();
        assert_eq!(destination, corridor.location(3));
        assert_eq!(
            orientation,
            if matches!(corridor, Corridor::Rotated) {
                1
            } else {
                0
            }
        );
        let return_route = game
            .route_search_from(actor, destination, orientation)
            .unwrap()
            .route(corridor.location(0))
            .unwrap();
        assert_eq!(return_route.len(), 3);
        assert_eq!(return_route[0].direction, Direction::West);
        let before = game.clone();
        let decision = game.choose_ai(actor).unwrap();
        assert_eq!(decision.ai.state, State::Flee, "{corridor:?}");
        assert_eq!(
            decision.action,
            Action::Move(Direction::East),
            "{corridor:?}"
        );
        assert_eq!(game, before);
    }
}

#[test]
fn fleeing_can_escape_through_a_disclosed_one_way_portal() {
    let corridor = Corridor::Rotated;
    let (game, actor, _) = corridor.game_with_reverse(2, 0, false);
    assert_eq!(
        game.travel_route(actor, corridor.location(0))
            .unwrap()
            .len(),
        2
    );
    let (at, orientation) = game.actor_translation(actor, Direction::East).unwrap();
    assert_eq!(
        game.route_search_from(actor, at, orientation)
            .unwrap()
            .route(corridor.location(0)),
        Err(GameError::Blocked),
        "the remembered graph has no reverse link"
    );
    assert_eq!(
        game.choose_ai(actor).unwrap().action,
        Action::Move(Direction::East)
    );
}

#[test]
fn local_flee_without_navigation_keeps_a_strict_disclosed_retreat() {
    let corridor = Corridor::Unsplit;
    let (mut game, actor, _) = corridor.game(2, 0);
    game.navigation.remove(&actor);
    assert_eq!(
        game.travel_route(actor, corridor.location(0)),
        Err(GameError::Blocked)
    );
    let before = game.clone();
    let decision = game.choose_ai(actor).unwrap();
    assert_eq!(decision.ai.state, State::Flee);
    assert_eq!(decision.action, Action::Move(Direction::East));
    assert_eq!(
        game, before,
        "current visibility must not write remembered topology"
    );
}

#[test]
fn missing_navigation_does_not_turn_an_approach_or_portal_into_a_retreat() {
    for corridor in [Corridor::Unsplit, Corridor::Split, Corridor::Rotated] {
        let (mut game, actor, _) = corridor.game(0, 5);
        game.navigation.remove(&actor);
        let before = game.clone();
        let decision = game.choose_ai(actor).unwrap();
        assert_eq!(decision.ai.state, State::Flee, "{corridor:?}");
        assert_eq!(decision.action, Action::Wait, "{corridor:?}");
        assert_eq!(game, before);
    }
    // Although both current coordinates are local, the only retreat crosses
    // a portal. Without a current route, its destination cannot be ranked by
    // coordinates from this region.
    let corridor = Corridor::Rotated;
    let (mut game, actor, _) = corridor.game(2, 0);
    game.navigation.remove(&actor);
    assert!(game.actor_translation(actor, Direction::East).is_some());
    let before = game.clone();
    assert_eq!(game.choose_ai(actor).unwrap().action, Action::Wait);
    assert_eq!(game, before);
}

#[test]
fn local_flee_without_navigation_allows_an_unwrapped_region_beside_a_portal() {
    for corridor in [Corridor::Split, Corridor::Rotated] {
        let (mut game, actor, threat) = corridor.game(1, 2);
        game.navigation.clear();
        let snapshot = game.decision_snapshot(actor).unwrap();
        assert!(snapshot
            .view
            .visible_actors
            .iter()
            .any(|other| other.id == threat));
        assert!(
            snapshot
                .scene
                .iter()
                .any(|cell| cell.physical && cell.location.region == RegionId(2)),
            "the already-disclosed scene includes the other region"
        );
        assert!(snapshot
            .scene
            .iter()
            .filter(|cell| cell.physical && cell.location.region == RegionId(1))
            .all(|cell| {
                cell.rotation == 0
                    && i64::from(cell.location.position.x) == 1 + i64::from(cell.offset.x)
                    && cell.location.position.y == cell.offset.y
                    && cell.location.position.z == cell.offset.z
            }));
        assert_eq!(
            game.actor_translation(actor, Direction::West),
            Some((corridor.location(0), 0))
        );
        assert_eq!(
            game.travel_route(actor, corridor.location(2)),
            Err(GameError::Blocked)
        );
        let before = game.clone();
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Move(Direction::West)
        );
        assert_eq!(
            game, before,
            "a disclosed remote region does not teach navigation"
        );
    }
}

#[test]
fn local_flee_without_navigation_rejects_same_region_folded_charts() {
    let corridor = Corridor::Unsplit;
    let (mut ordinary, actor, threat) = corridor.game(1, 4);
    ordinary.navigation.remove(&actor);
    let snapshot = ordinary.decision_snapshot(actor).unwrap();
    assert!(snapshot
        .scene
        .iter()
        .filter(|cell| cell.physical)
        .all(|cell| {
            cell.location.region == RegionId(1)
                && cell.rotation == 0
                && i64::from(cell.location.position.x) == 1 + i64::from(cell.offset.x)
                && cell.location.position.y == cell.offset.y
                && cell.location.position.z == cell.offset.z
        }));
    let before = ordinary.clone();
    assert_eq!(
        ordinary.choose_ai(actor).unwrap().action,
        Action::Move(Direction::West),
        "an ordinary disclosed local retreat needs no remembered map"
    );
    assert_eq!(ordinary, before);

    for turns in [0, 2] {
        let mut game = ordinary.clone();
        game.world
            .connect(
                Passage {
                    from: corridor.location(0),
                    direction: Direction::West,
                    to: corridor.location(5),
                },
                turns,
            )
            .unwrap();
        if turns == 2 {
            // Continue the same body-relative ray after its half-turn, so the
            // shortcut to the threat is physically disclosed in both cases.
            game.world
                .connect(
                    Passage {
                        from: corridor.location(5),
                        direction: Direction::East,
                        to: corridor.location(4),
                    },
                    0,
                )
                .unwrap();
        }
        let snapshot = game.decision_snapshot(actor).unwrap();
        assert!(snapshot
            .view
            .visible_actors
            .iter()
            .any(|other| other.id == threat));
        assert!(
            snapshot.scene.iter().any(|cell| {
                cell.physical
                    && cell.location == corridor.location(5)
                    && cell.offset == Position { x: -2, y: 0, z: 0 }
                    && cell.rotation == turns
            }),
            "the chart folds within one region, turns={turns}"
        );
        assert!(
            snapshot.scene.iter().any(|cell| {
                cell.physical
                    && cell.location == corridor.location(4)
                    && cell.offset == Position { x: -3, y: 0, z: 0 }
                    && cell.rotation == turns
            }),
            "the shortcut reaches the visible threat, turns={turns}"
        );
        let (destination, orientation) = game.actor_translation(actor, Direction::West).unwrap();
        assert_eq!((destination, orientation), (corridor.location(0), 0));
        assert!(!game.navigation.contains_key(&actor));
        assert_eq!(
            game.travel_route(actor, corridor.location(4)),
            Err(GameError::Blocked)
        );

        // Coordinate distance rises from 3 to 4 even though this ordinary
        // first step approaches through the disclosed same-region shortcut.
        // Learn only on an independent witness, never on the tested Game.
        let mut mapped = game.clone();
        mapped.refresh_navigation();
        assert_eq!(
            mapped
                .travel_route(actor, corridor.location(4))
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            mapped
                .route_search_from(actor, destination, orientation)
                .unwrap()
                .route(corridor.location(4))
                .unwrap()
                .len(),
            2
        );
        let before = game.clone();
        let decision = game.choose_ai(actor).unwrap();
        assert_eq!(decision.ai.state, State::Flee);
        assert_eq!(decision.action, Action::Wait, "turns={turns}");
        assert_eq!(
            game, before,
            "folded perception must not write remembered topology"
        );
    }
}

#[test]
fn fleeing_does_not_invent_distance_for_an_unknown_current_route() {
    let corridor = Corridor::Split;
    let (mut game, actor, _) = corridor.game(0, 5);
    // Knowing the cells does not license an unlearned connection between them.
    game.navigation
        .get_mut(&actor)
        .unwrap()
        .edges
        .remove(&(corridor.location(2), Direction::East));
    assert_eq!(
        game.travel_route(actor, corridor.location(5)),
        Err(GameError::Blocked)
    );
    assert!(game.actor_translation(actor, Direction::East).is_some());
    let before = game.clone();
    let decision = game.choose_ai(actor).unwrap();
    assert_eq!(decision.ai.state, State::Flee);
    assert_eq!(decision.action, Action::Wait);
    assert_eq!(
        game, before,
        "a distance query must not learn the live portal"
    );
}

#[test]
fn fleeing_does_not_select_a_blocked_portal_retreat() {
    let corridor = Corridor::Rotated;
    let (mut game, actor, _) = corridor.game(2, 0);
    game.set_wall(corridor.location(3), true).unwrap();
    assert!(game.actor_translation(actor, Direction::East).is_none());
    assert_eq!(game.choose_ai(actor).unwrap().action, Action::Wait);
}

#[test]
fn cornered_flee_intention_recomputes_the_same_wait_after_checkpoint() {
    let corridor = Corridor::Split;
    let (mut game, actor, threat) = corridor.game(0, 5);
    game.act(threat, Action::Wait).unwrap();
    assert_eq!(game.next_ai_action(), Some((actor, Action::Wait)));
    game.admit_ai_intention(actor).unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot =
        serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    for candidate in [&mut game, &mut restored] {
        let execution = candidate.execute_next_intention().unwrap();
        assert_eq!(execution.action, Some(Action::Wait));
        execution.outcome.unwrap();
        assert_eq!(
            candidate.observe(actor).unwrap().location,
            corridor.location(0)
        );
    }
    assert_eq!(game, restored);
}

/// The actor previously stood at the passage mouth. Remembered knowledge is
/// acquired from real scenes, never from an omniscient hand-built edge table.
fn plateau_game() -> (Game, ActorId, ActorId) {
    plateau_game_reflected(false)
}

fn plateau_game_reflected(reflected: bool) -> (Game, ActorId, ActorId) {
    let at = |x, y| plateau_at(if reflected { 7 - x } else { x }, y);
    let (mut game, actor, threat) = Corridor::Unsplit.game(2, 0);
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_region(Region {
            id: RegionId(1),
            name: "plateau".into(),
            bounds: Extent::new(8, 9, 1).unwrap(),
        })
        .unwrap();
    *game.world = world;
    game.actors.get_mut(&threat).unwrap().location = at(3, 1);
    game.actors.get_mut(&actor).unwrap().location = at(4, 4);
    for x in 0..8 {
        for y in 0..9 {
            if x == 0 || x == 7 || y == 0 || y == 8 || (x <= 4 && y >= 5) {
                game.set_wall(at(x, y), true).unwrap();
            }
        }
    }
    game.actors.get_mut(&threat).unwrap().location = at(3, 1);
    game.actors.get_mut(&actor).unwrap().location = at(4, 4);
    game.navigation.clear();
    game.refresh_navigation();
    game.actors.get_mut(&actor).unwrap().location = at(3, 4);
    game.actors
        .get_mut(&actor)
        .unwrap()
        .behavior
        .engagement
        .as_mut()
        .unwrap()
        .location = at(3, 1);
    game.refresh_navigation();
    (game, actor, threat)
}

fn plateau_at(x: i32, y: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y, z: 0 },
    }
}

#[test]
fn fleeing_crosses_a_known_equal_distance_step_to_a_two_step_retreat() {
    let (game, actor, _) = plateau_game();
    let threat = plateau_at(3, 1);
    assert_eq!(game.travel_route(actor, threat).unwrap().len(), 3);
    assert_eq!(
        game.actor_translation(actor, Direction::East),
        Some((plateau_at(4, 4), 0))
    );
    let mut first = game.route_search_from(actor, plateau_at(4, 4), 0).unwrap();
    assert_eq!(first.route(threat).unwrap().len(), 3);
    assert_eq!(
        first.route(plateau_at(5, 5)).unwrap().len(),
        1,
        "the second step is explicitly remembered, not merely physically possible"
    );
    assert_eq!(
        game.route_search_from(actor, plateau_at(5, 5), 0)
            .unwrap()
            .route(threat)
            .unwrap()
            .len(),
        4
    );
    let before = game.clone();
    let decision = game.choose_ai(actor).unwrap();
    assert_eq!(decision.ai.state, State::Flee);
    assert_eq!(decision.action, Action::Move(Direction::East));
    assert_eq!(game.choose_ai(actor).unwrap().action, decision.action);
    assert_eq!(game, before);
}

#[test]
fn fleeing_plateau_does_not_invent_a_missing_second_edge() {
    let (mut game, actor, _) = plateau_game();
    let knowledge = game.navigation.get_mut(&actor).unwrap();
    assert!(knowledge
        .edges
        .contains_key(&(plateau_at(4, 4), Direction::East)));
    knowledge.edges.remove(&(plateau_at(4, 4), Direction::East));
    assert_eq!(knowledge.cells.get(&plateau_at(5, 5)), Some(&false));
    let before = game.clone();
    assert_eq!(game.choose_ai(actor).unwrap().action, Action::Wait);
    assert_eq!(game, before);
}

#[test]
fn retreat_bounded_search_distinguishes_exhaustion_and_can_resume() {
    let (game, actor, _) = plateau_game();
    let mut search = game.route_search_from(actor, plateau_at(5, 5), 0).unwrap();
    let mut none = 0;
    assert_eq!(search.route_bounded(plateau_at(3, 1), &mut none), Ok(None));
    let mut one = 1;
    assert_eq!(search.route_bounded(plateau_at(3, 1), &mut one), Ok(None));
    assert_eq!(one, 0);
    let mut enough = 512;
    assert_eq!(
        search
            .route_bounded(plateau_at(3, 1), &mut enough)
            .unwrap()
            .unwrap()
            .len(),
        4
    );
    assert!(enough < 512);
    assert_eq!(
        search
            .route_bounded(plateau_at(3, 1), &mut none)
            .unwrap()
            .unwrap()
            .len(),
        4,
        "already settled destinations consume no further expansion budget"
    );
    assert_eq!(
        search.route_bounded(plateau_at(0, 0), &mut enough),
        Err(GameError::Blocked)
    );
}

#[test]
fn plateau_retreat_executes_after_checkpoint_and_replans_the_second_step() {
    let (mut game, actor, threat) = plateau_game();
    game.act(threat, Action::Wait).unwrap();
    assert_eq!(
        game.next_ai_action(),
        Some((actor, Action::Move(Direction::East)))
    );
    let rng = game.actors[&actor].behavior.rng;
    game.admit_ai_intention(actor).unwrap();
    assert_eq!(game.actors[&actor].behavior.rng, rng);
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot =
        serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    for candidate in [&mut game, &mut restored] {
        let execution = candidate.execute_next_intention().unwrap();
        assert_eq!(execution.action, Some(Action::Move(Direction::East)));
        execution.outcome.unwrap();
        assert_eq!(candidate.observe(actor).unwrap().location, plateau_at(4, 4));
        assert_eq!(
            candidate.choose_ai(actor).unwrap().action,
            Action::Move(Direction::SouthEast)
        );
    }
    assert_eq!(game, restored);
    // No two-step reservation survived the first move. Removing the witnessed
    // endpoint makes the next decision re-evaluate ordinary legal alternatives.
    game.set_wall(plateau_at(5, 5), true).unwrap();
    let action = game.choose_ai(actor).unwrap().action;
    assert_ne!(action, Action::Move(Direction::SouthEast));
}

#[test]
fn plateau_keeps_direct_retreat_priority() {
    let (mut game, actor, _) = plateau_game();
    game.set_wall(plateau_at(3, 5), false).unwrap();
    game.refresh_navigation();
    assert_eq!(
        game.choose_ai(actor).unwrap().action,
        Action::Move(Direction::South)
    );
}

#[test]
fn plateau_retreat_is_not_tied_to_east_world_coordinates() {
    let (game, actor, _) = plateau_game_reflected(true);
    assert_eq!(game.travel_route(actor, plateau_at(4, 1)).unwrap().len(), 3);
    assert_eq!(
        game.choose_ai(actor).unwrap().action,
        Action::Move(Direction::West)
    );
}

/// Absolute diagnostic cost, not a before/after benchmark or CI timing gate.
#[test]
#[ignore = "run explicitly in release for local cost diagnostics"]
fn plateau_retreat_release_decision_cost() {
    use std::{hint::black_box, time::Instant};
    let (game, actor, _) = plateau_game();
    let before = game.clone();
    for _ in 0..5 {
        black_box(game.choose_ai(actor).unwrap());
    }
    let mut samples = Vec::new();
    for _ in 0..50 {
        let start = Instant::now();
        for _ in 0..10 {
            assert_eq!(
                black_box(game.choose_ai(actor).unwrap()).action,
                Action::Move(Direction::East)
            );
        }
        samples.push(start.elapsed().as_secs_f64() * 1000. / 10.);
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "plateau decision ms n={} p50={} p95={} max={}",
        samples.len(),
        samples[25],
        samples[47],
        samples[49]
    );
    assert_eq!(game, before);
}

#[test]
fn plateau_does_not_treat_anchor_routes_as_large_body_clearance() {
    let (mut game, actor, _) = plateau_game();
    let mut body = (*game.actors[&actor].body).clone();
    body.cells.push([-1, 0, 0]);
    game.set_body(actor, body.clone()).unwrap();
    assert!(game.actor_translation(actor, Direction::East).is_some());
    assert!(!game.body_fits(actor, plateau_at(5, 5), 0, &body));
    assert_eq!(game.choose_ai(actor).unwrap().action, Action::Wait);
    game.set_wall(plateau_at(2, 5), false).unwrap();
    game.set_wall(plateau_at(3, 5), false).unwrap();
    game.refresh_navigation();
    assert_eq!(
        game.choose_ai(actor).unwrap().action,
        Action::Move(Direction::South),
        "the existing strict retreat still supports larger bodies"
    );
}
