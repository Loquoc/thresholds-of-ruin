//! Independent witnesses for derived projection reuse; gameplay APIs stay unchanged.
use super::*;
use std::num::NonZeroU64;

fn pos(x: i32, y: i32, z: i32) -> Position {
    Position { x, y, z }
}
fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: pos(x, 1, 0),
    }
}
fn appearance(label: &str) -> MapAppearance {
    MapAppearance {
        object_presentation: None,
        wall: true,
        material: label.into(),
        asset: None,
        place_hint: false,
        stairs_up: false,
        stairs_down: false,
        trap: None,
        door: None,
        ground_items: Vec::new(),
    }
}
fn seen(x: i32, offset: i32) -> SightCell {
    SightCell {
        location: at(x),
        offset: pos(offset, 0, 0),
        rotation: 0,
        wall: true,
        physical: true,
    }
}
fn window() -> MapWindow {
    MapWindow {
        min: pos(-3, -1, -1),
        max: pos(3, 1, 1),
    }
}
fn fixture() -> (Game, ActorId, [SightCell; 2]) {
    let mut world = tor_world::World::new(vec![], vec![]).unwrap();
    world
        .add_region(tor_world::Region {
            id: RegionId(1),
            name: "cache witness".into(),
            bounds: tor_world::Extent::new(12, 4, 2).unwrap(),
        })
        .unwrap();
    let mut game = Game::new(world, 42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(at(1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let scene = [seen(1, 0), seen(2, 1)];
    record(&mut game, actor, &scene, "old");
    (game, actor, scene)
}
fn record(game: &mut Game, actor: ActorId, scene: &[SightCell], label: &str) {
    let cells: Vec<_> = scene
        .iter()
        .map(|c| MapCellInput {
            location: c.location,
            appearance: appearance(label),
        })
        .collect();
    game.record_explored_observation(actor, &cells, scene)
        .unwrap();
}
fn assert_same_output(a: &MapProjection, b: &MapProjection) {
    assert_eq!(a.cells, b.cells);
    assert_eq!(
        (a.complete, a.ambiguous, a.limited, a.appearance_limited),
        (b.complete, b.ambiguous, b.limited, b.appearance_limited)
    );
}

#[test]
fn projection_cache_reuses_real_work_across_clone_and_unchanged_learning() {
    let (game, actor, scene) = fixture();
    let mut branch = game.clone();
    reset_projection_calls();
    let first = game.project_explored_map(actor, &scene, window()).unwrap();
    assert_eq!(projection_calls(), 1);
    assert!(first.work.projection_states > 0);
    assert_eq!(first.cells.len(), 2);
    assert_eq!(first.cells[0].location, at(1));
    assert_eq!(first.cells[1].location, at(2));
    record(&mut branch, actor, &scene, "old");
    let repeated = branch
        .project_explored_map(actor, &scene, window())
        .unwrap();
    assert_same_output(&first, &repeated);
    assert_eq!(
        projection_calls(),
        1,
        "a clone must reuse the real projector, not replay counted work"
    );
    assert_eq!(repeated.work, MapWorkCounts::default());
}

#[test]
fn projection_cache_does_not_cross_mutated_roots_or_actor_knowledge() {
    let (game, actor, scene) = fixture();
    let mut branch = game.clone();
    let old = game.project_explored_map(actor, &scene, window()).unwrap();
    record(&mut branch, actor, &scene, "new");
    let fresh = branch
        .project_explored_map(actor, &scene, window())
        .unwrap();
    assert!(fresh.cells.iter().all(|c| c.appearance.material == "new"));
    let original = game.project_explored_map(actor, &scene, window()).unwrap();
    assert_same_output(&old, &original);
    assert!(original
        .cells
        .iter()
        .all(|c| c.appearance.material == "old"));
    let other = branch
        .spawn_actor(at(3), NonZeroU64::new(100).unwrap())
        .unwrap();
    let private = branch
        .project_explored_map(other, &scene, window())
        .unwrap();
    assert!(private.complete && private.cells.is_empty());
    record(&mut branch, other, &scene[..1], "other");
    let personal = branch
        .project_explored_map(other, &scene, window())
        .unwrap();
    assert_eq!(personal.cells.len(), 1);
    assert_eq!(personal.cells[0].appearance.material, "other");
}

#[test]
fn projection_cache_keeps_api_error_precedence_and_scene_validation() {
    let (game, actor, scene) = fixture();
    game.project_explored_map(actor, &scene, window()).unwrap();
    let unknown = ActorId(actor.0 + 1000);
    let invalid = MapWindow {
        min: pos(2, 0, 0),
        max: pos(1, 0, 0),
    };
    assert_eq!(
        game.project_explored_map(unknown, &scene, invalid),
        Err(GameError::UnknownActor)
    );
    assert_eq!(
        game.project_explored_map_region(unknown, &scene, invalid, window()),
        Err(GameError::InvalidLocation)
    );
    assert_eq!(
        game.project_explored_map_region(unknown, &scene, window(), window()),
        Err(GameError::UnknownActor)
    );
    let mut bad = scene;
    bad[0].rotation = 24;
    assert_eq!(
        game.project_explored_map(actor, &bad, window()),
        Err(GameError::InvalidLocation)
    );
    assert_eq!(
        game.project_explored_map(actor, &[scene[0], scene[0]], window()),
        Err(GameError::InvalidLocation)
    );
    let extreme = MapWindow {
        min: pos(i32::MAX, 0, 0),
        max: pos(i32::MAX, 0, 0),
    };
    assert_eq!(
        game.project_explored_map_region(actor, &scene, extreme, extreme),
        Err(GameError::InvalidLocation)
    );
    let good = game.project_explored_map(actor, &scene, window()).unwrap();
    assert_eq!(
        good.cells.len(),
        2,
        "rejections must not poison later valid reads"
    );
}

#[test]
fn projection_cache_keys_exact_order_physical_halo_rotation_and_location() {
    let (game, actor, scene) = fixture();
    reset_projection_calls();
    let original = game.project_explored_map(actor, &scene, window()).unwrap();
    let ignored = SightCell {
        physical: false,
        rotation: 255,
        ..scene[0]
    };
    let outside = SightCell {
        offset: pos(100, 0, 0),
        rotation: 255,
        ..scene[0]
    };
    let equivalent = [scene[0], ignored, scene[1], outside];
    let same = game
        .project_explored_map(actor, &equivalent, window())
        .unwrap();
    assert_same_output(&original, &same);
    assert_eq!(
        projection_calls(),
        1,
        "ignored cells do not participate in the projection key"
    );
    let reordered = [scene[1], scene[0]];
    let reordered_projection = game
        .project_explored_map(actor, &reordered, window())
        .unwrap();
    assert_same_output(&original, &reordered_projection);
    assert_eq!(
        projection_calls(),
        2,
        "preserve input order even when this small example commutes"
    );
    let changed = [
        SightCell {
            rotation: 1,
            ..scene[0]
        },
        scene[1],
    ];
    game.project_explored_map(actor, &changed, window())
        .unwrap();
    assert_eq!(projection_calls(), 3);
    let changed = [
        SightCell {
            location: at(3),
            ..scene[0]
        },
        scene[1],
    ];
    game.project_explored_map(actor, &changed, window())
        .unwrap();
    assert_eq!(projection_calls(), 4);
    let changed = [
        scene[0],
        SightCell {
            offset: pos(2, 0, 0),
            ..scene[1]
        },
    ];
    game.project_explored_map(actor, &changed, window())
        .unwrap();
    assert_eq!(projection_calls(), 5);
}

#[test]
fn projection_cache_does_not_substitute_other_windows_or_domains() {
    let (game, actor, scene) = fixture();
    let point = MapWindow {
        min: pos(1, 0, 0),
        max: pos(1, 0, 0),
    };
    let distant = MapWindow {
        min: pos(3, 0, 0),
        max: pos(3, 0, 0),
    };
    reset_projection_calls();
    let full = game
        .project_explored_map_region(actor, &scene, window(), window())
        .unwrap();
    assert_eq!(full.cells.len(), 2);
    let clipped = game
        .project_explored_map_region(actor, &scene, point, window())
        .unwrap();
    assert_eq!(clipped.cells.len(), 1);
    assert_eq!(clipped.cells[0].position, point.min);
    game.project_explored_map_region(actor, &scene, point, point)
        .unwrap();
    assert_eq!(
        projection_calls(),
        3,
        "output and domain both distinguish the key"
    );
    let unanchored = game
        .project_explored_map_region(actor, &scene, distant, distant)
        .unwrap();
    assert!(unanchored.complete && unanchored.cells.is_empty());
}

#[test]
fn projection_cache_retains_no_map_root_or_directory_copy_obligation() {
    let (mut game, actor, scene) = fixture();
    let branch = game.clone();
    drop(
        branch
            .project_explored_map(actor, &scene, window())
            .unwrap(),
    );
    drop(branch);
    assert!(
        !game.explored_maps[&actor].is_shared(),
        "cached projection must not own map root"
    );
    reset_map_work_counts();
    record(&mut game, actor, &scene, "changed");
    assert_eq!(map_work_counts().owner_copies, 0);
    assert_eq!(map_work_counts().directory_entries_copied, 0);
    let projected = game.project_explored_map(actor, &scene, window()).unwrap();
    assert!(projected
        .cells
        .iter()
        .all(|c| c.appearance.material == "changed"));
}

#[test]
fn projection_cache_single_entry_eviction_and_concurrent_branches_preserve_outputs() {
    let (game, actor, scene) = fixture();
    let mut other = game.clone();
    record(&mut other, actor, &scene, "branch");
    reset_projection_calls();
    game.project_explored_map(actor, &scene, window()).unwrap();
    other.project_explored_map(actor, &scene, window()).unwrap();
    game.project_explored_map(actor, &scene, window()).unwrap();
    assert_eq!(projection_calls(), 3, "only the last entry is retained");
    std::thread::scope(|scope| {
        for (branch, label) in [(&game, "old"), (&other, "branch")] {
            scope.spawn(move || {
                for _ in 0..20 {
                    let projected = branch
                        .project_explored_map(actor, &scene, window())
                        .unwrap();
                    assert_eq!(projected.cells.len(), 2);
                    assert!(projected.complete && !projected.ambiguous && !projected.limited);
                    assert!(projected
                        .cells
                        .iter()
                        .all(|c| c.appearance.material == label));
                }
            });
        }
    });
}

#[test]
fn projection_cache_keeps_limited_flags_and_falls_back_for_oversized_keys() {
    let (mut game, actor, scene) = fixture();
    let mut huge = appearance("huge");
    huge.asset = Some("x".repeat(130 * 1024));
    game.record_explored_observation(
        actor,
        &[MapCellInput {
            location: scene[0].location,
            appearance: huge,
        }],
        &scene[..1],
    )
    .unwrap();
    reset_projection_calls();
    let limited = game
        .project_explored_map(actor, &scene[..1], window())
        .unwrap();
    assert!(
        limited.appearance_limited
            && limited.limited
            && !limited.complete
            && limited.cells.is_empty()
    );
    let repeated = game
        .project_explored_map(actor, &scene[..1], window())
        .unwrap();
    assert_same_output(&limited, &repeated);
    assert_eq!(projection_calls(), 1);
    assert_eq!(repeated.work, MapWorkCounts::default());
    let wide = MapWindow {
        min: pos(0, 0, 0),
        max: pos(32, 32, 32),
    };
    let mut aliases = Vec::new();
    for x in 0..33 {
        for y in 0..33 {
            for z in 0..33 {
                aliases.push(SightCell {
                    offset: pos(x, y, z),
                    ..scene[0]
                });
            }
        }
    }
    assert!(aliases.len() > MAX_STATES);
    reset_projection_calls();
    let first = game.project_explored_map(actor, &aliases, wide).unwrap();
    let second = game.project_explored_map(actor, &aliases, wide).unwrap();
    assert_eq!(first, second);
    assert!(first.limited && !first.complete && first.cells.is_empty());
    assert_eq!(projection_calls(), 2, "oversized keys are never admitted");
}

#[test]
fn projection_cache_returned_value_mutation_cannot_poison_warm_result() {
    let (game, actor, scene) = fixture();
    reset_projection_calls();
    let mut returned = game.project_explored_map(actor, &scene, window()).unwrap();
    assert_eq!(returned.cells.len(), 2);
    returned.cells[0].appearance.material = "caller changed shared appearance".into();
    returned.cells[0].position = pos(99, 99, 99);
    returned.cells.pop();
    returned.complete = false;
    returned.ambiguous = true;
    returned.limited = true;
    returned.appearance_limited = true;
    returned.work.projection_states = usize::MAX;
    let warm = game.project_explored_map(actor, &scene, window()).unwrap();
    assert_eq!(projection_calls(), 1);
    assert_eq!(warm.cells.len(), 2);
    assert_eq!(warm.cells[0].location, at(1));
    assert_eq!(warm.cells[0].position, pos(0, 0, 0));
    assert_eq!(warm.cells[1].location, at(2));
    assert_eq!(warm.cells[1].position, pos(1, 0, 0));
    assert!(warm
        .cells
        .iter()
        .all(|cell| *cell.appearance == appearance("old")));
    assert!(warm.complete && !warm.ambiguous && !warm.limited && !warm.appearance_limited);
    assert_eq!(warm.work, MapWorkCounts::default());
}

#[test]
fn projection_cache_absent_root_hit_invalidates_on_first_knowledge() {
    let (mut game, _, scene) = fixture();
    let actor = game
        .spawn_actor(at(3), NonZeroU64::new(100).unwrap())
        .unwrap();
    assert!(!game.explored_maps.contains_key(&actor));
    reset_projection_calls();
    for _ in 0..2 {
        let blank = game.project_explored_map(actor, &scene, window()).unwrap();
        assert!(blank.complete && blank.cells.is_empty());
        assert!(!blank.ambiguous && !blank.limited && !blank.appearance_limited);
    }
    assert_eq!(projection_calls(), 1);
    record(&mut game, actor, &scene[..1], "first knowledge");
    let learned = game.project_explored_map(actor, &scene, window()).unwrap();
    assert_eq!(projection_calls(), 2);
    assert_eq!(learned.cells.len(), 1);
    assert_eq!(learned.cells[0].location, at(1));
    assert_eq!(learned.cells[0].position, pos(0, 0, 0));
    assert_eq!(*learned.cells[0].appearance, appearance("first knowledge"));
    assert!(learned.complete && !learned.ambiguous && !learned.limited);
}

#[test]
fn projection_cache_preserves_warm_ambiguity_without_replaying_work() {
    let (mut game, actor, _) = fixture();
    let mut map = ExploredMap::default();
    for x in 1..=5 {
        map.cells
            .insert(at(x), Shared::new(appearance("remembered")));
    }
    // Two independently witnessed paths compete for (1,-1,0). Neither
    // competing terminal survives; the anchor and two first steps remain.
    for (from, direction, to) in [
        (1, Direction::East, 2),
        (1, Direction::North, 4),
        (2, Direction::North, 3),
        (4, Direction::East, 5),
    ] {
        map.neighbors.insert(
            (at(from), direction),
            Neighbor::Placed {
                target: at(to),
                rotation: 0,
            },
        );
        map.neighbors.insert(
            (at(to), opposite(direction)),
            Neighbor::Placed {
                target: at(from),
                rotation: 0,
            },
        );
    }
    game.explored_maps.insert(actor, Shared::new(map));
    reset_projection_calls();
    for pass in 0..2 {
        let result = game
            .project_explored_map(actor, &[seen(1, 0)], window())
            .unwrap();
        assert!(
            result.complete && result.ambiguous && !result.limited && !result.appearance_limited
        );
        assert_eq!(
            result
                .cells
                .iter()
                .map(|c| c.location)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([at(1), at(2), at(4)])
        );
        if pass == 1 {
            assert_eq!(result.work, MapWorkCounts::default());
        }
    }
    assert_eq!(projection_calls(), 1);
}

#[test]
fn projection_cache_preserves_warm_traversal_limit_without_replaying_work() {
    let (mut game, actor, _) = fixture();
    let mut map = ExploredMap::default();
    map.cells.insert(at(1), Shared::new(appearance("cycle")));
    for direction in DIRECTIONS {
        map.neighbors.insert(
            (at(1), direction),
            Neighbor::Placed {
                target: at(1),
                rotation: 0,
            },
        );
    }
    game.explored_maps.insert(actor, Shared::new(map));
    let domain = MapWindow {
        min: pos(-64, -64, -64),
        max: pos(64, 64, 64),
    };
    let output = MapWindow {
        min: pos(64, 64, 64),
        max: pos(64, 64, 64),
    };
    reset_projection_calls();
    for pass in 0..2 {
        let result = game
            .project_explored_map_region(actor, &[seen(1, 0)], output, domain)
            .unwrap();
        assert!(!result.complete && result.limited && !result.appearance_limited);
        assert!(result.cells.is_empty());
        if pass == 0 {
            assert!(result.work.projection_states > 0);
            assert!(result.work.projection_states <= MAX_STATES);
            assert!(result.work.candidate_relations <= MAX_RELATIONS);
            assert!(result.work.inverse_checks <= MAX_RELATIONS);
            assert!(result.work.range_seeks <= MAX_SEEKS);
        } else {
            assert_eq!(result.work, MapWorkCounts::default());
        }
    }
    assert_eq!(projection_calls(), 1);
}
