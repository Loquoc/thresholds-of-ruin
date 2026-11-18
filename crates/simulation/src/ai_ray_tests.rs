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

// Append inside ai::ray_tests after current source freeze. These use the
// existing authored ray fixture; real Dragon positive controls live separately.
fn alternate_target_fixture() -> (Game, ActorId) {
    let (mut game, blocker, _) = disclosed_blocker_fixture(false, "neutral");
    game.add_region(tor_world::Region {
        id: RegionId(77),
        name: "Alternate disclosed body cell".into(),
        bounds: tor_world::Extent::new(9, 3, 3).unwrap(),
    })
    .unwrap();
    game.set_region_light(RegionId(77), true).unwrap();
    for (actor, x) in [(ActorId(2), 1), (ActorId(1), 5), (blocker, 4)] {
        game.teleport(actor, alternate_at(x, 0)).unwrap();
    }
    let mut target_body = (*game.actors[&ActorId(1)].body).clone();
    target_body.cells = vec![[0, 0, 0], [0, 0, 1]];
    target_body.eye = [0, 0, 1];
    game.set_body(ActorId(1), target_body).unwrap();
    game.refresh_navigation();
    let view = game.observe(ActorId(2)).unwrap();
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    let rows: Vec<_> = view
        .visible_actors
        .iter()
        .filter(|row| row.id == ActorId(1))
        .collect();
    assert_eq!(rows[0].location, alternate_at(5, 0));
    assert!(rows.iter().any(|row| row.location == alternate_at(5, 1)));
    assert!(game.disclosed_nonhostile_interception(
        ActorId(2),
        ActorId(1),
        StepDirection::East.into(),
        &sight,
        &view,
        8
    ));
    let mut ray = tor_world::RayTrajectory::new([4, 0, 1]).unwrap();
    let mut offset = [0; 3];
    let mut path = Vec::new();
    for _ in 0..4 {
        let delta = ray.advance();
        for axis in 0..3 {
            offset[axis] += delta[axis];
        }
        path.push(offset);
    }
    assert_eq!(path, vec![[1, 0, 0], [2, 0, 1], [3, 0, 1], [4, 0, 1]]);
    (game, blocker)
}
fn alternate_at(x: i32, z: i32) -> Location {
    Location {
        region: RegionId(77),
        position: Position { x, y: 1, z },
    }
}
fn alternate_choice(
    game: &Game,
    sight: &[SightCell],
    view: &crate::Observation,
) -> Option<crate::abilities::FlightAim> {
    game.ai_disclosed_flight_aim(ActorId(2), ActorId(1), sight, view, 8)
}

#[test]
fn fresh_ray_uses_safe_disclosed_upper_target_cell_after_definite_primary_veto() {
    let (game, _) = alternate_target_fixture();
    let view = game.observe(ActorId(2)).unwrap();
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    let before = game.clone();
    assert_eq!(
        alternate_choice(&game, &sight, &view),
        Some(crate::abilities::FlightAim::Cell { offset: [4, 0, 1] })
    );
    let action = game.choose_ai(ActorId(2)).unwrap().action;
    assert!(matches!(
        action,
        Action::UseAbility {
            invocation: AbilityInvocation {
                aim: crate::abilities::AbilityAim::CellRay { offset: [4, 0, 1] },
                ..
            },
            ..
        }
    ));
    assert_eq!(game, before);
}

#[test]
fn alternate_target_cell_needs_complete_clear_disclosure_not_an_absent_veto() {
    let (mut game, blocker) = alternate_target_fixture();
    let view = game.observe(ActorId(2)).unwrap();
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    let mut missing = view.clone();
    missing
        .visible_cells
        .retain(|cell| cell.location != alternate_at(3, 1));
    assert!(game.disclosed_nonhostile_interception(
        ActorId(2),
        ActorId(1),
        StepDirection::East.into(),
        &sight,
        &missing,
        8
    ));
    assert_eq!(alternate_choice(&game, &sight, &missing), None);
    let mut aliased = sight.clone();
    let mut alias = *sight
        .iter()
        .find(|cell| cell.location == alternate_at(5, 1) && cell.physical)
        .unwrap();
    alias.offset.x += 1;
    aliased.push(alias);
    assert_eq!(alternate_choice(&game, &aliased, &view), None);
    let mut body = (*game.actors[&blocker].body).clone();
    body.cells = vec![[0, 0, 0], [0, 0, 1]];
    game.set_body(blocker, body).unwrap();
    let view = game.observe(ActorId(2)).unwrap();
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    assert_eq!(
        alternate_choice(&game, &sight, &view),
        None,
        "both disclosed rays now intercepted"
    );
}

#[test]
fn alternate_target_recovery_preserves_safe_primary_and_hidden_twin_choices() {
    let (game, blocker) = alternate_target_fixture();
    let mut twin = game.clone();
    twin.set_wall(
        Location {
            region: RegionId(2),
            position: Position { x: 3, y: 1, z: 0 },
        },
        true,
    )
    .unwrap();
    let view = game.observe(ActorId(2)).unwrap();
    let twin_view = twin.observe(ActorId(2)).unwrap();
    assert_eq!(view, twin_view);
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    let (_, twin_sight) = twin.scene_snapshot(ActorId(2)).unwrap();
    assert_eq!(
        alternate_choice(&game, &sight, &view),
        alternate_choice(&twin, &twin_sight, &twin_view)
    );
    let mut hidden_blocker = view.clone();
    hidden_blocker
        .visible_actors
        .retain(|row| row.id != blocker);
    assert_eq!(
        alternate_choice(&game, &sight, &hidden_blocker),
        Some(StepDirection::East.into()),
        "no definite disclosed veto preserves the exact legacy primary"
    );
}

#[test]
fn alternate_target_search_has_deterministic_unique_cell_cap() {
    let (game, _) = alternate_target_fixture();
    let view = game.observe(ActorId(2)).unwrap();
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    let template = view
        .visible_actors
        .iter()
        .find(|row| row.id == ActorId(1))
        .unwrap()
        .clone();
    for count in [15, 16] {
        let mut synthetic = view.clone();
        // Synthetic disclosed rows with no physical chart witnesses. They sort
        // before region77's real upper cell and must consume candidate slots,
        // not be accepted via absent collision evidence or hidden-world queries.
        for x in 0..count {
            let mut row = template.clone();
            row.location = Location {
                region: RegionId(1),
                position: Position { x, y: 0, z: 0 },
            };
            synthetic.visible_actors.push(row.clone());
            synthetic.visible_actors.push(row); // duplicates cannot consume cap
        }
        let expected =
            (count == 15).then_some(crate::abilities::FlightAim::Cell { offset: [4, 0, 1] });
        assert_eq!(alternate_choice(&game, &sight, &synthetic), expected);
        // Preserve primary row; reordering only the alternatives cannot matter.
        let primary_index = synthetic
            .visible_actors
            .iter()
            .position(|row| row.id == ActorId(1))
            .unwrap();
        synthetic.visible_actors[primary_index + 1..].reverse();
        assert_eq!(alternate_choice(&game, &sight, &synthetic), expected);
    }
}

// Append in ai::ray_tests after alternate-tests.rs. A ray preparation has no
// multi-strike prefix (PreparationProgress::Ray); this tests its elapsed work and
// frozen selected aim without pretending it has an earned strike prefix.
#[test]
fn alternate_ray_pending_cancel_and_checkpoint_keep_frozen_selected_aim() {
    let (game, blocker) = alternate_target_fixture();
    assert_retained_alternate_ray(game, &[ActorId(1), blocker]);
}

#[test]
fn known_wall_alternate_pending_and_checkpoint_keep_frozen_aim() {
    assert_retained_alternate_ray(elevated_low_wall_fixture(), &[ActorId(1)]);
}

fn assert_retained_alternate_ray(mut game: Game, peers: &[ActorId]) {
    // Ordinary peers stop the clock before the ray's 100-tick preparation
    // resolves. No damage or target disappearance is needed to hold this seam.
    for actor in peers {
        game.actors.get_mut(actor).unwrap().ready_at = 1;
    }
    let action = game.choose_ai(ActorId(2)).unwrap().action;
    assert!(matches!(
        action,
        Action::UseAbility {
            invocation: AbilityInvocation {
                aim: crate::abilities::AbilityAim::CellRay { offset: [4, 0, 1] },
                ..
            },
            ..
        }
    ));
    let intention = game.admit_ai_intention(ActorId(2)).unwrap();
    let queued_ai = game.combat.ai[&ActorId(2)].clone();
    let queued_behavior = game.actors[&ActorId(2)].behavior.clone();
    let mut cancelled = game.clone();
    cancelled.cancel_intention(ActorId(2), intention).unwrap();
    assert!(cancelled.preparation(ActorId(2)).is_none());
    assert_eq!(cancelled.combat.ai[&ActorId(2)], queued_ai);
    assert_eq!(cancelled.actors[&ActorId(2)].behavior, queued_behavior);
    assert_eq!(cancelled.choose_ai(ActorId(2)).unwrap().action, action);

    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot =
        serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    for candidate in [&mut game, &mut restored] {
        let execution = candidate.execute_next_intention().unwrap();
        assert_eq!(execution.intention.id, intention);
        assert_eq!(execution.action, Some(action.clone()));
        execution.outcome.unwrap();
        assert_eq!(candidate.tick, 1);
        let pending = candidate.preparation(ActorId(2)).unwrap();
        assert!(pending.active);
        assert!(
            !pending.has_prefix(),
            "ray work is not a multi-strike ability prefix"
        );
        let frozen = pending.plan.clone();
        let remaining = pending.remaining_at(candidate.actor_clock(ActorId(2)));
        assert!(
            remaining > 0 && remaining < pending.remaining,
            "ordinary elapsed work must be retained"
        );
        candidate.pause_preparation(ActorId(2)).unwrap();
        assert_eq!(
            candidate.preparation(ActorId(2)).unwrap().remaining,
            remaining
        );
        let view = candidate.observe(ActorId(2)).unwrap();
        let (_, sight) = candidate.scene_snapshot(ActorId(2)).unwrap();
        let legacy = disclosed_flight_aim(
            ActorId(1),
            &sight,
            &view,
            candidate.actors[&ActorId(2)].body.eye,
            8,
        );
        assert_eq!(
            legacy,
            Some(StepDirection::East.into()),
            "legacy first-row helper deliberately disagrees with retained upper-cell aim"
        );
        let before = candidate.clone();
        let resumed = candidate.choose_ai(ActorId(2)).unwrap();
        assert_eq!(resumed.action, action);
        assert_eq!(resumed.plan, Some(frozen));
        assert_eq!(
            *candidate, before,
            "retained preview cannot reroll, reset elapsed work or recapture aim"
        );
    }
    assert_eq!(game, restored);
    // A second checkpoint retains already-started and paused alternate work,
    // not merely the queued original selection.
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let paused = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(paused, game);
    assert_eq!(paused.choose_ai(ActorId(2)).unwrap().action, action);
    assert_eq!(
        paused.preparation(ActorId(2)).unwrap().plan,
        game.preparation(ActorId(2)).unwrap().plan
    );
}

// Append inside ai::ray_tests after alternate fixture helpers.
#[test]
fn alternate_target_recovery_respects_exact_effective_range_boundary() {
    let (game, _) = alternate_target_fixture();
    let view = game.observe(ActorId(2)).unwrap();
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    assert_eq!(
        game.ai_disclosed_flight_aim(ActorId(2), ActorId(1), &sight, &view, 3),
        None,
        "neither the legacy endpoint nor its alternate is in range"
    );
    assert_eq!(
        game.ai_disclosed_flight_aim(ActorId(2), ActorId(1), &sight, &view, 4),
        Some(crate::abilities::FlightAim::Cell { offset: [4, 0, 1] }),
        "the safe off-axis cell is admitted at the exact four-step boundary"
    );
}
#[test]
fn primary_flight_box_is_inclusive_overflow_safe_and_contains_rotated_prefixes() {
    use super::flight_safety::within_canonical_primary_box as inside;
    let at = |x, y, z| Location {
        region: RegionId(77),
        position: Position { x, y, z },
    };
    let low = at(i32::MIN, i32::MAX, -1);
    let high = at(i32::MAX, i32::MIN, 1);
    for point in [low, high, at(0, 0, 0)] {
        assert!(inside(low, high, point));
        assert!(inside(high, low, point));
    }
    assert!(!inside(low, high, at(0, 0, 2)));
    let elsewhere = Location {
        region: RegionId(78),
        ..low
    };
    assert!(!inside(low, high, elsewhere));
    assert!(!inside(low, elsewhere, low));

    let origin = at(13, -7, 5);
    let location = |offset: [i64; 3]| {
        at(
            i32::try_from(i64::from(origin.position.x) + offset[0]).unwrap(),
            i32::try_from(i64::from(origin.position.y) + offset[1]).unwrap(),
            i32::try_from(i64::from(origin.position.z) + offset[2]).unwrap(),
        )
    };
    for rotation in 0..24 {
        for vector in [[-7_i32, 3, 2], [4, -2, 0], [1, 1, 1], [0, 0, -5]] {
            let target = location(tor_world::rotate_vector(rotation, vector.map(i64::from)));
            let distance = vector.iter().map(|n| n.unsigned_abs()).max().unwrap();
            let mut trajectory = tor_world::RayTrajectory::new(vector).unwrap();
            let mut offset = [0_i64; 3];
            for _ in 0..distance {
                let delta = trajectory.advance();
                for axis in 0..3 {
                    offset[axis] += i64::from(delta[axis]);
                }
                let point = location(tor_world::rotate_vector(rotation, offset));
                assert!(
                    inside(origin, target, point),
                    "{rotation} {vector:?} {point:?}"
                );
            }
        }
    }
}

#[test]
fn outside_primary_box_matches_full_veto_even_with_target_aliases() {
    use super::flight_safety::within_canonical_primary_box as inside;
    let (game, blocker) = alternate_target_fixture();
    let before = game.clone();
    let view = game.observe(ActorId(2)).unwrap();
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    let origin = game.actors[&ActorId(2)].location;
    let target = view
        .visible_actors
        .iter()
        .find(|r| r.id == ActorId(1))
        .unwrap()
        .location;
    let target_cell = *sight
        .iter()
        .find(|c| c.physical && c.location == target)
        .unwrap();
    let mut aliased = sight.clone();
    aliased.push(SightCell {
        offset: Position { x: 1, y: 0, z: 0 },
        ..target_cell
    });
    let outside = [
        alternate_at(0, 0),
        alternate_at(7, 0),
        Location {
            position: Position { x: 3, y: 0, z: 0 },
            ..origin
        },
        Location {
            position: Position { x: 3, y: 2, z: 0 },
            ..origin
        },
        alternate_at(3, 2),
        Location {
            region: RegionId(2),
            ..alternate_at(3, 0)
        },
    ];
    for location in outside {
        assert!(!inside(origin, target, location));
        let mut disclosed = view.clone();
        // Keep the old relative row position deliberately: the bound must use
        // the disclosed canonical Location, not a portal presentation offset.
        for row in disclosed
            .visible_actors
            .iter_mut()
            .filter(|r| r.id == blocker)
        {
            row.location = location;
        }
        for cells in [&sight, &aliased] {
            let primary = disclosed_flight_aim(
                ActorId(1),
                cells,
                &disclosed,
                game.actors[&ActorId(2)].body.eye,
                8,
            )
            .unwrap();
            assert!(!game.disclosed_nonhostile_interception(
                ActorId(2),
                ActorId(1),
                primary,
                cells,
                &disclosed,
                8,
            ));
            assert_eq!(alternate_choice(&game, cells, &disclosed), Some(primary));
        }
    }
    assert_eq!(game, before);
}

#[test]
fn primary_box_keeps_intercepting_body_cells_when_anchor_is_outside() {
    use super::flight_safety::within_canonical_primary_box as inside;
    let (game, blocker, _) = disclosed_blocker_fixture(true, "neutral");
    let view = game.observe(ActorId(2)).unwrap();
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    let origin = game.actors[&ActorId(2)].location;
    let target = view
        .visible_actors
        .iter()
        .find(|r| r.id == ActorId(1))
        .unwrap()
        .location;
    assert!(!inside(origin, target, game.actors[&blocker].location));
    assert!(view
        .visible_actors
        .iter()
        .any(|r| r.id == blocker && inside(origin, target, r.location)));
    assert!(
        intercepts(&game, &sight, &view),
        "full-chart positive control"
    );
    assert_eq!(
        game.ai_disclosed_flight_aim(ActorId(2), ActorId(1), &sight, &view, 8),
        None
    );
}

fn disclosed_low_wall_fixture(wall_x: i32) -> Game {
    let mut game = fixture();
    game.add_region(tor_world::Region {
        id: RegionId(77),
        name: "Eye-visible foe above a body-origin obstruction".into(),
        bounds: tor_world::Extent::new(11, 7, 4).unwrap(),
    })
    .unwrap();
    game.set_region_light(RegionId(77), true).unwrap();
    for (actor, x) in [(ActorId(2), 1), (ActorId(1), 5)] {
        game.teleport(actor, low_wall_at(x, 2, 0)).unwrap();
        let mut body = (*game.actors[&actor].body).clone();
        body.cells = vec![[0, 0, 0], [0, 0, 1]];
        body.eye = [0, 0, 1];
        game.set_body(actor, body).unwrap();
    }
    game.set_wall(low_wall_at(wall_x, 2, 0), true).unwrap();
    game.refresh_navigation();
    game
}

fn low_wall_at(x: i32, y: i32, z: i32) -> Location {
    Location {
        region: RegionId(77),
        position: Position { x, y, z },
    }
}

fn assert_disclosed_low_wall(game: &Game, view: &crate::Observation, wall_x: i32) {
    let targets: Vec<_> = view
        .visible_actors
        .iter()
        .filter(|row| row.id == ActorId(1))
        .map(|row| row.location)
        .collect();
    assert_eq!(targets, vec![low_wall_at(5, 2, 0), low_wall_at(5, 2, 1)]);
    for x in 2..=5 {
        assert!(
            view.visible_cells.iter().any(|cell| {
                cell.location == low_wall_at(x, 2, 0) && cell.wall == (x == wall_x)
            }),
            "complete disclosed nominal prefix at x={x}"
        );
    }
    for z in 0..=1 {
        assert!(
            view.visible_cells
                .iter()
                .any(|cell| { cell.location == low_wall_at(2, 1, z) && !cell.wall }),
            "approach destination body cell is disclosed"
        );
    }
    assert_eq!(
        game.actor_translation(ActorId(2), Direction::NorthEast)
            .unwrap()
            .0,
        low_wall_at(2, 1, 0),
        "ordinary physical approach is available"
    );
}

#[test]
fn fresh_ray_approaches_instead_of_spending_a_shot_on_a_disclosed_low_wall() {
    let game = disclosed_low_wall_fixture(2);
    let view = game.observe(ActorId(2)).unwrap();
    assert_disclosed_low_wall(&game, &view, 2);
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    assert_eq!(
        disclosed_flight_aim(ActorId(1), &sight, &view, [0, 0, 1], 8),
        Some(StepDirection::East.into()),
        "independent original-bearing positive"
    );
    let before = game.clone();
    let decision = game.choose_ai(ActorId(2)).unwrap();
    assert_eq!(game, before, "choice is private and pure");
    let Action::Move(direction) = decision.action else {
        panic!(
            "known low wall should release ordinary approach, not spend a shot: {:?}",
            decision.action
        );
    };
    let (destination, _) = game.actor_translation(ActorId(2), direction).unwrap();
    assert_ne!(
        destination,
        low_wall_at(2, 2, 0),
        "never walk into the known wall"
    );
}

#[test]
fn fresh_ray_recovers_a_clear_upper_cell_after_a_disclosed_low_wall() {
    let game = elevated_low_wall_fixture();
    let view = game.observe(ActorId(2)).unwrap();
    assert_disclosed_low_wall(&game, &view, 3);
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    let before = game.clone();
    assert_eq!(
        game.ai_disclosed_flight_aim(ActorId(2), ActorId(1), &sight, &view, 8),
        Some(crate::abilities::FlightAim::Cell { offset: [4, 0, 1] }),
        "a known blocked primary needs the positively clear disclosed upper prefix"
    );
    assert_eq!(game, before);
}

// A separate authored three-cell caster supplies complete visibility behind the
// middle wall. The ordinary two-cell Dragon case is intentionally Unknown.
fn elevated_low_wall_fixture() -> Game {
    let mut game = disclosed_low_wall_fixture(3);
    let mut body = (*game.actors[&ActorId(2)].body).clone();
    body.cells = vec![[0, 0, 0], [0, 0, 1], [0, 0, 2]];
    body.eye = [0, 0, 2];
    game.set_body(ActorId(2), body).unwrap();
    game.refresh_navigation();
    game
}

#[test]
fn middle_low_wall_without_complete_prefix_preserves_original_aim() {
    let game = disclosed_low_wall_fixture(3);
    let view = game.observe(ActorId(2)).unwrap();
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    assert!(!view
        .visible_cells
        .iter()
        .any(|c| c.location == low_wall_at(4, 2, 0)));
    assert!(view
        .visible_cells
        .iter()
        .any(|c| c.location == low_wall_at(3, 2, 0) && c.wall));
    assert_eq!(
        alternate_choice(&game, &sight, &view),
        Some(StepDirection::East.into())
    );
}

// Staged only. Append to ai_ray_tests.rs after first-red receipt.
// These are disclosure-boundary tests; synthetic row contradictions are not
// claims about ordinary observer output.
#[test]
fn known_wall_requires_disclosure_not_raw_world_state() {
    let mut game = elevated_low_wall_fixture();
    let view = game.observe(ActorId(2)).unwrap();
    assert_disclosed_low_wall(&game, &view, 3);
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    assert_eq!(
        alternate_choice(&game, &sight, &view),
        Some(crate::abilities::FlightAim::Cell { offset: [4, 0, 1] })
    );
    let mut hidden = view.clone();
    hidden
        .visible_cells
        .retain(|c| c.location != low_wall_at(3, 2, 0));
    assert_eq!(
        alternate_choice(&game, &sight, &hidden),
        Some(StepDirection::East.into())
    );
    game.set_wall(low_wall_at(3, 2, 0), false).unwrap();
    // Identical provided disclosed evidence, different raw wall. No secret
    // geometry query is permitted, even to resolve the omitted witness.
    assert_eq!(
        alternate_choice(&game, &sight, &hidden),
        Some(StepDirection::East.into())
    );
    assert_eq!(
        alternate_choice(&game, &sight, &view),
        Some(crate::abilities::FlightAim::Cell { offset: [4, 0, 1] })
    );
}

#[test]
fn known_wall_missing_later_prefix_is_unknown_not_early_veto() {
    let game = elevated_low_wall_fixture();
    let view = game.observe(ActorId(2)).unwrap();
    assert_disclosed_low_wall(&game, &view, 3);
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    let mut missing = view.clone();
    missing
        .visible_cells
        .retain(|c| c.location != low_wall_at(4, 2, 0));
    assert_eq!(
        alternate_choice(&game, &sight, &missing),
        Some(StepDirection::East.into())
    );
    let mut missing_sight = sight.clone();
    missing_sight.retain(|c| c.location != low_wall_at(4, 2, 0));
    assert_eq!(
        alternate_choice(&game, &missing_sight, &view),
        Some(StepDirection::East.into())
    );
}

#[test]
fn known_wall_conflicting_duplicate_disclosure_is_sticky_in_both_orders() {
    let game = elevated_low_wall_fixture();
    let view = game.observe(ActorId(2)).unwrap();
    assert_disclosed_low_wall(&game, &view, 3);
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    let wall = view
        .visible_cells
        .iter()
        .find(|c| c.location == low_wall_at(3, 2, 0))
        .unwrap()
        .clone();
    let mut clear = wall.clone();
    clear.wall = false;
    for reverse in [false, true] {
        let mut conflict = view.clone();
        let cells = crate::observation::observation_cells_mut(&mut conflict.visible_cells);
        cells.retain(|c| c.location != wall.location);
        // Repeated agreeing evidence must not erase the intervening conflict.
        cells.extend([wall.clone(), clear.clone(), wall.clone()]);
        if reverse {
            cells.reverse();
        }
        assert_eq!(
            alternate_choice(&game, &sight, &conflict),
            Some(StepDirection::East.into())
        );
    }
}

#[test]
fn known_wall_with_folded_prefix_or_target_alias_remains_unknown() {
    let game = elevated_low_wall_fixture();
    let view = game.observe(ActorId(2)).unwrap();
    assert_disclosed_low_wall(&game, &view, 3);
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    let target = *sight
        .iter()
        .find(|c| c.physical && c.location == low_wall_at(5, 2, 0))
        .unwrap();
    let wall = *sight
        .iter()
        .find(|c| c.physical && c.location == low_wall_at(3, 2, 0))
        .unwrap();
    for reverse in [false, true] {
        let mut folded = sight.clone();
        folded.push(SightCell {
            offset: target.offset,
            ..wall
        });
        let mut aliased = sight.clone();
        let mut alias = target;
        alias.offset.x += 1;
        aliased.push(alias);
        if reverse {
            folded.reverse();
            aliased.reverse();
        }
        assert_eq!(
            alternate_choice(&game, &folded, &view),
            Some(StepDirection::East.into())
        );
        assert_eq!(
            alternate_choice(&game, &aliased, &view),
            Some(StepDirection::East.into())
        );
    }
}

#[test]
fn known_wall_target_cell_wall_contradiction_is_unknown() {
    let game = elevated_low_wall_fixture();
    let mut view = game.observe(ActorId(2)).unwrap();
    assert_disclosed_low_wall(&game, &view, 3);
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    for cell in crate::observation::observation_cells_mut(&mut view.visible_cells).iter_mut() {
        if cell.location == low_wall_at(5, 2, 0) {
            cell.wall = true;
        }
    }
    assert_eq!(
        alternate_choice(&game, &sight, &view),
        Some(StepDirection::East.into())
    );
}

#[test]
fn known_wall_outside_primary_box_and_off_primary_raster_keeps_east() {
    let game = elevated_low_wall_fixture();
    let view = game.observe(ActorId(2)).unwrap();
    assert_disclosed_low_wall(&game, &view, 3);
    let (_, sight) = game.scene_snapshot(ActorId(2)).unwrap();
    // Axis-aligned East has no inside-box/off-raster cells. These disclosed
    // off-raster cells cover a lateral wall, upper wall and wall past target.
    for location in [
        low_wall_at(3, 3, 0),
        low_wall_at(3, 2, 1),
        low_wall_at(6, 2, 0),
    ] {
        let mut disclosed = view.clone();
        assert!(disclosed
            .visible_cells
            .iter()
            .any(|c| c.location == location));
        for c in crate::observation::observation_cells_mut(&mut disclosed.visible_cells).iter_mut()
        {
            if c.location == low_wall_at(3, 2, 0) {
                c.wall = false;
            }
            if c.location == location {
                c.wall = true;
            }
        }
        assert_eq!(
            alternate_choice(&game, &sight, &disclosed),
            Some(StepDirection::East.into())
        );
    }
}

#[test]
fn known_wall_chart_preserves_canonical_proof_in_every_orientation() {
    let game = elevated_low_wall_fixture();
    let original_view = game.observe(ActorId(2)).unwrap();
    assert_disclosed_low_wall(&game, &original_view, 3);
    let (_, original_sight) = game.scene_snapshot(ActorId(2)).unwrap();
    let origin = game.actors[&ActorId(2)].location;
    let eye = game.actors[&ActorId(2)].body.eye;
    // Explicitly transformed chart witnesses, not a generated rotated-world
    // visibility claim. Canonical locations and the known wall stay fixed.
    for orientation in 0..24 {
        let mut twin = game.clone();
        twin.actors.get_mut(&ActorId(2)).unwrap().orientation = orientation;
        let mut view = original_view.clone();
        for cell in crate::observation::observation_cells_mut(&mut view.visible_cells) {
            cell.frame = orientation;
        }
        let mut sight = original_sight.clone();
        for cell in &mut sight {
            let offset = tor_world::rotate_vector(
                tor_world::inverse_rotation(orientation),
                [
                    i64::from(cell.location.position.x) - i64::from(origin.position.x),
                    i64::from(cell.location.position.y) - i64::from(origin.position.y),
                    i64::from(cell.location.position.z) - i64::from(origin.position.z),
                ],
            );
            cell.rotation = orientation;
            cell.offset = Position {
                x: i32::try_from(offset[0]).unwrap() - eye[0],
                y: i32::try_from(offset[1]).unwrap() - eye[1],
                z: i32::try_from(offset[2]).unwrap() - eye[2],
            };
        }
        let offset = tor_world::rotate_vector(tor_world::inverse_rotation(orientation), [4, 0, 1])
            .map(|v| i32::try_from(v).unwrap());
        assert_eq!(
            alternate_choice(&twin, &sight, &view),
            Some(crate::abilities::FlightAim::Cell { offset }),
            "orientation {orientation}"
        );
    }
}

#[test]
fn unbounded_disclosed_bearing_has_no_invented_range_and_rejects_bad_evidence() {
    let game = fixture();
    let mut view = game.observe(ActorId(2)).unwrap();
    let target = ActorId(1);
    let location = view
        .visible_actors
        .iter()
        .find(|row| row.id == target)
        .unwrap()
        .location;
    let cell = SightCell {
        physical: true,
        location,
        offset: Position { x: 9, y: 2, z: -1 },
        rotation: 0,
        wall: false,
    };
    assert_eq!(
        disclosed_flight_aim_with_range(target, &[cell], &view, [0, 0, 1], Some(6)),
        None
    );
    assert_eq!(
        disclosed_flight_aim_with_range(target, &[cell], &view, [0, 0, 1], None),
        Some(crate::abilities::FlightAim::Cell { offset: [9, 2, 0] })
    );
    for invalid in [
        SightCell {
            physical: false,
            ..cell
        },
        SightCell { wall: true, ..cell },
    ] {
        assert_eq!(
            disclosed_flight_aim_with_range(target, &[invalid], &view, [0, 0, 1], None),
            None
        );
    }
    let overflow = SightCell {
        offset: Position {
            x: i32::MAX,
            y: 0,
            z: 0,
        },
        ..cell
    };
    assert_eq!(
        disclosed_flight_aim_with_range(target, &[overflow], &view, [1, 0, 0], None),
        None
    );
    view.visible_actors.retain(|row| row.id != target);
    assert_eq!(
        disclosed_flight_aim_with_range(target, &[cell], &view, [0, 0, 1], None),
        None
    );
}
