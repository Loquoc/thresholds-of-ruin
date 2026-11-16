//! Direction choice never asks geometry to predict a future ray collision.
use super::*;
use crate::abilities::{AbilityInvocation, InvocationKind, StepDirection};
use crate::rules::{
    Against, CheckActor, CheckDef, Modifier, ModifierOp, RangePreference, Scope, SkillSelector,
    StrikeSource,
};
use tor_world::{Position, RegionId, SightCell};

fn fixture() -> Game {
    let mut definition = crate::test_creatures::mana_ray();
    definition.invocation = InvocationKind::DirectionalRay;
    definition.cost = None;
    definition.ai.priority = 100;
    definition.ai.range_preference = RangePreference::Any;
    let StrikeSource::Authored { strikes } = &mut definition.strikes else {
        unreachable!()
    };
    let strike = &mut strikes[0];
    strike.check = Some(CheckDef {
        actor: CheckActor::Target,
        skill: SkillSelector::Named {
            skill: crate::attributes::Skill::Agility,
        },
        against: Against::Opposed {
            skill: SkillSelector::Named {
                skill: crate::attributes::Skill::Spellcasting,
            },
        },
    });
    strike.on_failure = std::mem::take(&mut strike.on_success);
    let mut document = crate::test_creatures::ruleset().document().clone();
    let mut foe = crate::test_creatures::species();
    foe.modifiers.push(Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::GrantAbility {
            ability: definition.id.clone(),
        },
    });
    document.abilities.push(definition);
    let mut game = super::tests::configured_game(
        crate::test_creatures::species(),
        foe,
        crate::rules::Ruleset::validate(document).unwrap(),
    );
    game.teleport(
        ActorId(1),
        Location {
            region: RegionId(1),
            position: Position { x: 4, y: 1, z: 0 },
        },
    )
    .unwrap();
    game
}

#[test]
fn hostile_ai_selects_disclosed_alignment_without_target_or_hidden_collision_preview() {
    let game = fixture();
    let mut twin = game.clone();
    twin.add_region(tor_world::Region {
        id: RegionId(3),
        name: "undisclosed".into(),
        bounds: tor_world::Extent::new(8, 8, 4).unwrap(),
    })
    .unwrap();
    let hidden = Location {
        region: RegionId(3),
        position: Position { x: 2, y: 2, z: 0 },
    };
    twin.set_wall(hidden, true).unwrap();
    twin.spawn_actor(
        Location {
            position: Position { x: 3, y: 3, z: 0 },
            ..hidden
        },
        std::num::NonZeroU64::new(100).unwrap(),
    )
    .unwrap();
    assert_eq!(
        game.observe(ActorId(2)).unwrap(),
        twin.observe(ActorId(2)).unwrap()
    );
    let before = game.clone();
    let twin_before = twin.clone();
    let selected = game.choose_ai(ActorId(2)).unwrap().action;
    assert!(matches!(
        selected,
        Action::UseAbility {
            invocation: AbilityInvocation {
                aim: crate::abilities::AbilityAim::DirectionalRay {
                    direction: StepDirection::East
                },
                ..
            },
            ..
        }
    ));
    assert_eq!(selected, twin.choose_ai(ActorId(2)).unwrap().action);
    assert_eq!(game, before);
    assert_eq!(twin, twin_before);
}

#[test]
fn disclosed_ray_alignment_rejects_undisclosed_contacts_stairs_and_inexact_vectors() {
    let game = fixture();
    let mut view = game.observe(ActorId(2)).unwrap();
    let target = ActorId(1);
    let target_location = view
        .visible_actors
        .iter()
        .find(|other| other.id == target)
        .unwrap()
        .location;
    let cell = SightCell {
        physical: true,
        location: target_location,
        offset: Position { x: 4, y: 0, z: -1 },
        rotation: 0,
        wall: false,
    };
    // Original decision cells are eye-relative. Add the known eye offset to aim
    // from the source's reference cell, without consulting hidden terrain.
    assert_eq!(
        disclosed_ray_direction(target, &[cell], &view, [0, 0, 1], 6),
        Some(StepDirection::East)
    );
    assert_eq!(
        disclosed_ray_direction(target, &[cell], &view, [0, 0, 1], 3),
        None
    );
    for direction in Direction::HORIZONTAL
        .into_iter()
        .chain([Direction::Up, Direction::Down])
    {
        let (x, y, z) = direction.delta();
        let offset = Position {
            x: 3 * x,
            y: 3 * y,
            z: 3 * z,
        };
        assert_eq!(
            disclosed_ray_direction(target, &[SightCell { offset, ..cell }], &view, [0, 0, 0], 6),
            Some(direction.try_into().unwrap())
        );
    }
    for offset in [
        Position { x: 0, y: 0, z: 0 },
        Position { x: 2, y: 1, z: 0 },
        Position { x: 1, y: 0, z: 1 },
        Position { x: 1, y: 1, z: 1 },
    ] {
        assert_eq!(
            disclosed_ray_direction(target, &[SightCell { offset, ..cell }], &view, [0, 0, 0], 6),
            None
        );
    }
    assert_eq!(
        disclosed_ray_direction(
            target,
            &[SightCell {
                physical: false,
                ..cell
            }],
            &view,
            [0, 0, 1],
            6
        ),
        None
    );
    assert_eq!(
        disclosed_ray_direction(
            target,
            &[SightCell { wall: true, ..cell }],
            &view,
            [0, 0, 1],
            6
        ),
        None
    );
    view.visible_actors
        .iter_mut()
        .find(|other| other.id == target)
        .unwrap()
        .disclosure = crate::perception::ActorDisclosure::AnonymousContact;
    assert_eq!(
        disclosed_ray_direction(target, &[cell], &view, [0, 0, 1], 6),
        None
    );
    view.visible_actors.clear();
    assert_eq!(
        disclosed_ray_direction(target, &[cell], &view, [0, 0, 1], 6),
        None
    );
}

#[test]
fn queued_ray_restores_without_target_and_starts_with_directional_work() {
    let mut game = fixture();
    // The next human decision must precede the ray's completion; a full Wait
    // lets ordinary action scheduling release it before execution returns.
    game.actors.get_mut(&ActorId(1)).unwrap().ready_at = 1;
    let action = game.choose_ai(ActorId(2)).unwrap().action;
    let id = game
        .admit_intention(
            ActorId(2),
            action.clone(),
            crate::intention::IntentionOrigin::Human,
        )
        .unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let mut restored =
        Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
    assert_eq!(restored, game);
    let execution = restored.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, id);
    assert_eq!(execution.action, Some(action));
    assert!(matches!(
        execution.outcome.unwrap().kind,
        crate::OutcomeKind::ItemStarted {
            work: crate::Work::UseAbility {
                invocation: AbilityInvocation {
                    aim: crate::abilities::AbilityAim::DirectionalRay { .. },
                    ..
                },
                ..
            }
        }
    ));
    assert_eq!(restored.tick, 1);
    assert_eq!(restored.next_actor(), Some(ActorId(1)));
    assert_eq!(restored.rules_rng, game.rules_rng);
    assert!(!restored
        .combat_events()
        .iter()
        .any(|event| matches!(event, crate::combat::CombatEvent::Resolved { .. })));
    let preparation = restored.preparation(ActorId(2)).unwrap();
    assert_eq!(preparation.intention, Some(id));
    assert!(preparation.active);
    assert!(preparation.started + preparation.remaining > restored.tick);
    assert_eq!(preparation.plan.work().target(), None);
}

#[test]
fn arbitrary_flight_aim_uses_only_disclosed_physical_target_occurrences() {
    let game = fixture();
    let mut view = game.observe(ActorId(2)).unwrap();
    let target = ActorId(1);
    let location = view
        .visible_actors
        .iter()
        .find(|other| other.id == target)
        .unwrap()
        .location;
    let cell = SightCell {
        physical: true,
        location,
        offset: Position { x: 4, y: 2, z: -1 },
        rotation: 0,
        wall: false,
    };
    assert_eq!(
        disclosed_flight_aim(target, &[cell], &view, [0, 0, 1], 6),
        Some(crate::abilities::FlightAim::Cell { offset: [4, 2, 0] })
    );
    assert_eq!(
        disclosed_flight_aim(target, &[cell], &view, [0, 0, 1], 3),
        None
    );
    assert_eq!(
        disclosed_flight_aim(
            target,
            &[SightCell {
                physical: false,
                ..cell
            }],
            &view,
            [0, 0, 1],
            6
        ),
        None
    );
    view.visible_actors.retain(|other| other.id != target);
    assert_eq!(
        disclosed_flight_aim(target, &[cell], &view, [0, 0, 1], 6),
        None
    );
}
fn disclosed_blocker_fixture(nonanchor: bool, faction: &str) -> (Game, ActorId, Location) {
    let mut game = fixture();
    let anchor = Location {
        region: RegionId(1),
        position: Position {
            x: 3,
            y: if nonanchor { 2 } else { 1 },
            z: 0,
        },
    };
    let interception = Location {
        position: Position { x: 3, y: 1, z: 0 },
        ..anchor
    };
    let build = game.creature(ActorId(2)).unwrap().build().clone();
    let blocker = game
        .spawn_actor(anchor, std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_creature(
        blocker,
        crate::CreatureIdentity {
            corpse_identity: "item.corpse.test.foe".parse().unwrap(),
            name: "disclosed bystander".into(),
            faction: faction.into(),
        },
        build,
    )
    .unwrap();
    if nonanchor {
        let mut body = (*game.actors[&blocker].body).clone();
        body.cells.push([0, -1, 0]);
        game.set_body(blocker, body).unwrap();
    }
    let view = game.observe(ActorId(2)).unwrap();
    assert!(view.visible_actors.iter().any(|row| row.id == ActorId(1)
        && row.disclosure == crate::perception::ActorDisclosure::Creature));
    assert!(
        view.visible_actors.iter().any(|row| row.id == blocker
            && row.location == interception
            && row.disclosure == crate::perception::ActorDisclosure::Creature),
        "fixture must disclose the intercepting body cell, not merely the creature's anchor"
    );
    (game, blocker, interception)
}

fn is_east_ray(action: &Action) -> bool {
    let Action::UseAbility { invocation, .. } = action else {
        return false;
    };
    match invocation.aim {
        crate::abilities::AbilityAim::DirectionalRay {
            direction: StepDirection::East,
        } => true,
        crate::abilities::AbilityAim::CellRay { offset } => {
            offset[0] > 0 && offset[1] == 0 && offset[2] == 0
        }
        _ => false,
    }
}

#[test]
fn fresh_ray_avoids_disclosed_neutral_interceptor() {
    let (game, _, _) = disclosed_blocker_fixture(false, "neutral");
    let before = game.clone();
    assert!(!is_east_ray(&game.choose_ai(ActorId(2)).unwrap().action));
    assert_eq!(game, before);
}

#[test]
fn fresh_ray_avoids_disclosed_nonanchor_ally_body_cell() {
    let (game, blocker, interception) = disclosed_blocker_fixture(true, "foe");
    assert_ne!(game.actors[&blocker].location, interception);
    let before = game.clone();
    assert!(!is_east_ray(&game.choose_ai(ActorId(2)).unwrap().action));
    assert_eq!(game, before);
}

fn interception_witness() -> (Game, Vec<SightCell>, crate::Observation) {
    let (game, _, _) = disclosed_blocker_fixture(false, "neutral");
    let view = game.observe(ActorId(2)).unwrap();
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    assert!(intercepts(&game, &sight, &view));
    (game, sight, view)
}
fn intercepts(game: &Game, sight: &[SightCell], view: &crate::Observation) -> bool {
    game.disclosed_nonhostile_interception(
        ActorId(2),
        ActorId(1),
        StepDirection::East.into(),
        sight,
        view,
        8,
    )
}

#[test]
fn interception_uses_disclosed_nonhostile_rows_and_only_the_prefix() {
    let (game, sight, mut view) = interception_witness();
    let blocker = ActorId(3);
    let before = game.clone();
    view.visible_actors.retain(|row| row.id != blocker);
    assert!(
        !intercepts(&game, &sight, &view),
        "hidden bodies are not a decision input"
    );
    let mut after = game.observe(ActorId(2)).unwrap();
    for row in after
        .visible_actors
        .iter_mut()
        .filter(|row| row.id == blocker)
    {
        row.location.position.x = 5;
    }
    assert!(
        !intercepts(&game, &sight, &after),
        "downstream safety is not certified by the prefix guard"
    );
    let (hostile, _, _) = disclosed_blocker_fixture(false, "hero");
    let view = hostile.observe(ActorId(2)).unwrap();
    let (_, sight) = hostile.scene_snapshot(ActorId(2)).unwrap();
    assert!(
        !intercepts(&hostile, &sight, &view),
        "another disclosed opponent is not protected as an ally"
    );
    assert_eq!(game, before);
}

#[test]
fn interception_ignores_undisclosed_occurrences_and_raw_wall_state() {
    let (game, mut sight, view) = interception_witness();
    let mut unknown = *sight
        .iter()
        .find(|cell| cell.location.position.x == 3 && cell.location.region == RegionId(1))
        .unwrap();
    unknown.location.position.x = 100;
    unknown.rotation = (unknown.rotation + 1) % 24;
    sight.push(unknown);
    assert!(
        intercepts(&game, &sight, &view),
        "an undisclosed folded occurrence cannot change the veto"
    );
    for cell in &mut sight {
        if cell.location.region == RegionId(1) && cell.location.position.x == 3 {
            cell.wall = !cell.wall;
        }
    }
    assert!(
        intercepts(&game, &sight, &view),
        "projected walls, not raw concealed terrain, own the decision"
    );
}

#[test]
fn interception_declines_missing_wall_folded_and_aliased_prefixes() {
    let (game, sight, view) = interception_witness();
    let blocker = view
        .visible_actors
        .iter()
        .find(|row| row.id == ActorId(3))
        .unwrap()
        .location;
    let mut missing = sight.clone();
    missing.retain(|cell| cell.location != blocker);
    assert!(!intercepts(&game, &missing, &view));
    let mut wall_view = view.clone();
    for cell in crate::observation::observation_cells_mut(&mut wall_view.visible_cells)
        .iter_mut()
        .filter(|cell| cell.location == blocker)
    {
        cell.wall = true;
    }
    assert!(!intercepts(&game, &sight, &wall_view));
    let target = view
        .visible_actors
        .iter()
        .find(|row| row.id == ActorId(1))
        .unwrap()
        .location;
    let target_cell = *sight
        .iter()
        .find(|cell| cell.location == target && cell.physical)
        .unwrap();
    let blocker_cell = *sight
        .iter()
        .find(|cell| cell.location == blocker && cell.physical)
        .unwrap();
    for reverse in [false, true] {
        let mut folded = sight.clone();
        // Both locations are disclosed, but two conflicting placements at the
        // same raster offset cannot certify an ordinary segment.
        folded.push(SightCell {
            offset: target_cell.offset,
            ..blocker_cell
        });
        if reverse {
            folded.reverse();
        }
        assert!(!intercepts(&game, &folded, &view));
        let mut alias = sight.clone();
        alias.push(SightCell {
            offset: Position { x: 1, y: 0, z: 0 },
            ..target_cell
        });
        if reverse {
            alias.reverse();
        }
        assert!(
            !intercepts(&game, &alias, &view),
            "a Direction aim does not retain which target distance won"
        );
    }
    assert!(!game.disclosed_nonhostile_interception(
        ActorId(2),
        ActorId(1),
        StepDirection::East.into(),
        &sight,
        &view,
        1
    ));
}

#[test]
fn interception_uses_body_relative_raster_with_eye_height_and_rotated_frames() {
    let (game, _, original_view) = interception_witness();
    for rotation in 0..24 {
        for eye in [[0, 0, 0], [0, 0, 1]] {
            let mut twin = game.clone();
            let mut body = (*twin.actors[&ActorId(2)].body).clone();
            if !body.cells.contains(&eye) {
                body.cells.push(eye);
            }
            body.eye = eye;
            twin.set_body(ActorId(2), body).unwrap();
            twin.actors.get_mut(&ActorId(2)).unwrap().orientation = rotation;
            let origin = twin.actors[&ActorId(2)].location;
            let mut view = original_view.clone();
            for cell in crate::observation::observation_cells_mut(&mut view.visible_cells) {
                cell.frame = rotation;
            }
            let mut sight = Vec::new();
            let mut target_offset = [0; 3];
            for id in [ActorId(3), ActorId(1)] {
                let location = view
                    .visible_actors
                    .iter()
                    .find(|row| row.id == id)
                    .unwrap()
                    .location;
                let offset = tor_world::rotate_vector(
                    tor_world::inverse_rotation(rotation),
                    [
                        i64::from(location.position.x - origin.position.x),
                        i64::from(location.position.y - origin.position.y),
                        i64::from(location.position.z - origin.position.z),
                    ],
                )
                .map(|value| i32::try_from(value).unwrap());
                if id == ActorId(1) {
                    target_offset = offset;
                }
                sight.push(SightCell {
                    physical: true,
                    wall: false,
                    location,
                    rotation,
                    offset: Position {
                        x: offset[0] - eye[0],
                        y: offset[1] - eye[1],
                        z: offset[2] - eye[2],
                    },
                });
            }
            assert!(
                twin.disclosed_nonhostile_interception(
                    ActorId(2),
                    ActorId(1),
                    crate::abilities::FlightAim::Cell {
                        offset: target_offset
                    },
                    &sight,
                    &view,
                    8
                ),
                "rotation {rotation}, eye {eye:?}"
            );
        }
    }
}

#[test]
fn interception_raster_handles_off_axis_disclosed_cells() {
    let (game, _, mut view) = interception_witness();
    let origin = game.actors[&ActorId(2)].location;
    assert_eq!(game.actors[&ActorId(2)].orientation, 0);
    let eye = game.actors[&ActorId(2)].body.eye;
    let location = |offset: [i32; 3]| Location {
        region: origin.region,
        position: Position {
            x: origin.position.x + offset[0],
            y: origin.position.y + offset[1],
            z: origin.position.z + offset[2],
        },
    };
    // Independent expected major-axis raster: [4,2,0] rounds half ties away
    // from source, so these are positions, not per-step deltas.
    let raster = [[1, 1, 0], [2, 1, 0], [3, 2, 0], [4, 2, 0]];
    let mut trajectory = tor_world::RayTrajectory::new([4, 2, 0]).unwrap();
    let mut at = [0; 3];
    for expected in raster {
        let delta = trajectory.advance();
        for axis in 0..3 {
            at[axis] += delta[axis];
        }
        assert_eq!(at, expected);
    }
    let target = location([4, 2, 0]);
    let blocker = location([2, 1, 0]);
    for row in &mut view.visible_actors {
        if row.id == ActorId(1) {
            row.location = target;
        }
        if row.id == ActorId(3) {
            row.location = blocker;
        }
    }
    let mut template = view
        .visible_cells
        .iter()
        .find(|row| !row.wall)
        .unwrap()
        .clone();
    template.frame = 0;
    template.wall = false;
    let cells = crate::observation::observation_cells_mut(&mut view.visible_cells);
    cells.clear();
    // Include an explicitly disclosed off-raster location for negative control.
    for offset in raster.into_iter().chain([[2, 2, 0]]) {
        let mut cell = template.clone();
        cell.location = location(offset);
        cells.push(cell);
    }
    let sight: Vec<_> = raster
        .into_iter()
        .chain([[2, 2, 0]])
        .map(|offset| SightCell {
            physical: true,
            wall: false,
            rotation: 0,
            location: location(offset),
            offset: Position {
                x: offset[0] - eye[0],
                y: offset[1] - eye[1],
                z: offset[2] - eye[2],
            },
        })
        .collect();
    let aim = crate::abilities::FlightAim::Cell { offset: [4, 2, 0] };
    let guard = |view: &crate::Observation| {
        game.disclosed_nonhostile_interception(ActorId(2), ActorId(1), aim, &sight, view, 8)
    };
    assert!(
        guard(&view),
        "disclosed neutral is on independently specified raster"
    );
    for row in view
        .visible_actors
        .iter_mut()
        .filter(|row| row.id == ActorId(3))
    {
        row.location = location([2, 2, 0]);
    }
    assert!(
        !guard(&view),
        "disclosed neutral off raster must not veto the bearing"
    );
}
