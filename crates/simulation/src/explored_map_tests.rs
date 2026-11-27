use super::*;
use std::num::NonZeroU64;
#[test]
fn trap_memory_is_strict_nullable_appearance_without_authority_or_private_operands() {
    for family in [
        None,
        Some(tor_world::TrapFamily::Rust),
        Some(tor_world::TrapFamily::SleepingGas),
    ] {
        let value = MapTrapAppearance { family };
        let encoded = serde_json::to_value(&value).unwrap();
        assert_eq!(encoded.as_object().unwrap().len(), 1);
        assert_eq!(
            serde_json::from_value::<MapTrapAppearance>(encoded).unwrap(),
            value
        );
    }
    for invalid in [
        serde_json::json!({}),
        serde_json::json!([null]),
        serde_json::json!({"family":null,"id":1}),
        serde_json::json!({"family":null,"dc":12}),
        serde_json::json!({"family":{"rust":null}}),
        serde_json::json!({"family":{"sleeping_gas":null}}),
    ] {
        assert!(serde_json::from_value::<MapTrapAppearance>(invalid).is_err());
    }
}
fn pos(x: i32, y: i32, z: i32) -> Position {
    Position { x, y, z }
}
fn at(region: u64, x: i32) -> Location {
    Location {
        region: RegionId(region),
        position: pos(x, 1, 0),
    }
}
fn appearance(label: &str) -> MapAppearance {
    MapAppearance {
        object_presentation: None,
        wall: true,
        material: label.into(),
        asset: Some("old-stone".into()),
        place_hint: false,
        stairs_up: false,
        stairs_down: false,
        trap: None,
        door: None,
        ground_items: Vec::new(),
    }
}
fn seen(location: Location, offset: Position, rotation: u8) -> SightCell {
    SightCell {
        location,
        offset,
        rotation,
        wall: true,
        physical: true,
    }
}
fn window() -> MapWindow {
    MapWindow {
        min: pos(-31, -16, -2),
        max: pos(31, 16, 3),
    }
}
fn game() -> (Game, ActorId) {
    let mut world = tor_world::World::new(vec![], vec![]).unwrap();
    world
        .add_region(tor_world::Region {
            id: RegionId(1),
            name: "memory".into(),
            bounds: tor_world::Extent::new(6000, 4, 2).unwrap(),
        })
        .unwrap();
    let mut game = Game::new(world, 42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    (game, actor)
}
fn record(game: &mut Game, actor: ActorId, scene: &[SightCell]) {
    let inputs: Vec<_> = scene
        .iter()
        .map(|c| MapCellInput {
            location: c.location,
            appearance: appearance("old"),
        })
        .collect();
    game.record_explored_observation(actor, &inputs, scene)
        .unwrap();
}
fn assert_clipped_projection(
    map: &ExploredMap,
    scene: &[SightCell],
    output: MapWindow,
    domain: MapWindow,
    full: &MapProjection,
) {
    let mut expected = full.clone();
    expected.cells.retain(|cell| output.contains(cell.position));
    assert_eq!(map.project_region(scene, output, domain).unwrap(), expected);
}

#[test]
fn projection_regions_require_valid_containing_bounded_domains() {
    let (game, actor) = game();
    let point = MapWindow {
        min: pos(0, 0, 0),
        max: pos(0, 0, 0),
    };
    let reversed = MapWindow {
        min: pos(1, 0, 0),
        ..point
    };
    let outside = MapWindow {
        min: pos(32, 0, 0),
        max: pos(32, 0, 0),
    };
    let too_wide = MapWindow {
        min: pos(-64, 0, 0),
        max: pos(65, 0, 0),
    };
    let lowest = MapWindow {
        min: pos(i32::MIN, 0, 0),
        max: pos(i32::MIN, 0, 0),
    };
    let highest = MapWindow {
        min: pos(i32::MAX, 0, 0),
        max: pos(i32::MAX, 0, 0),
    };
    for (output, domain) in [
        (outside, window()),
        (reversed, window()),
        (point, reversed),
        (point, too_wide),
        (lowest, lowest),
        (highest, highest),
        (
            MapWindow {
                min: pos(i32::MIN + 1, 0, 0),
                max: pos(i32::MIN + 1, 0, 0),
            },
            MapWindow {
                max: pos(i32::MIN + 1, 0, 0),
                ..lowest
            },
        ),
        (
            MapWindow {
                min: pos(i32::MAX - 1, 0, 0),
                max: pos(i32::MAX - 1, 0, 0),
            },
            MapWindow {
                min: pos(i32::MAX - 1, 0, 0),
                ..highest
            },
        ),
        (window(), point),
    ] {
        assert_eq!(
            game.project_explored_map_region(actor, &[], output, domain),
            Err(GameError::InvalidLocation),
            "output={output:?}, domain={domain:?}"
        );
    }
    for output in [point, window()] {
        let projection = game
            .project_explored_map_region(actor, &[], output, window())
            .unwrap();
        assert!(projection.complete && projection.cells.is_empty());
    }
    assert_eq!(
        game.project_explored_map_region(ActorId(actor.0 + 1), &[], point, window()),
        Err(GameError::UnknownActor)
    );
    assert_eq!(
        game.project_explored_map_region(ActorId(actor.0 + 1), &[], point, too_wide),
        Err(GameError::InvalidLocation)
    );
}

#[test]
fn off_sight_region_uses_only_its_actors_saved_appearances_and_bounded_anchors() {
    let (mut game, actor) = game();
    let other = game
        .spawn_actor(at(1, 2), NonZeroU64::new(100).unwrap())
        .unwrap();
    let anchor = seen(at(1, 1), pos(0, 0, 0), 0);
    let edge = seen(at(1, 32), pos(31, 0, 0), 0);
    record(&mut game, actor, &[anchor, edge]);
    record(&mut game, other, &[anchor]);
    let slab = MapWindow {
        min: pos(31, -16, -2),
        max: window().max,
    };
    // The saved wall deliberately differs from the live floor. Projection
    // must not refresh it or borrow another actor's knowledge.
    assert!(!game.world.is_wall(edge.location));
    let before = game.clone();
    reset_map_work_counts();
    let full = game
        .project_explored_map(actor, &[anchor], window())
        .unwrap();
    let partial = game
        .project_explored_map_region(actor, &[anchor], slab, window())
        .unwrap();
    assert_eq!(partial.cells.len(), 1);
    assert_eq!(partial.cells[0].location, edge.location);
    assert_eq!(&*partial.cells[0].appearance, &appearance("old"));
    assert_clipped_projection(
        &game.explored_maps[&actor],
        &[anchor],
        slab,
        window(),
        &full,
    );
    let unseen = game
        .project_explored_map_region(other, &[anchor], slab, window())
        .unwrap();
    assert!(unseen.complete && unseen.cells.is_empty());
    // A domain without a current anchor stays empty; no historical scan or
    // world fallback expands it. The legacy API keeps its local semantics.
    let unanchored = game
        .project_explored_map_region(actor, &[anchor], slab, slab)
        .unwrap();
    assert!(unanchored.complete && unanchored.cells.is_empty());
    assert_eq!(
        game.project_explored_map(actor, &[anchor], slab).unwrap(),
        unanchored
    );
    assert_eq!(game, before);
    assert_eq!(map_work_counts(), MapWorkCounts::default());
}

#[test]
fn small_output_does_not_bypass_projection_domain_work_caps() {
    let mut map = ExploredMap::default();
    let cell = at(1, 1);
    map.cells.insert(cell, Shared::new(appearance("wall")));
    for direction in DIRECTIONS {
        map.neighbors.insert(
            (cell, direction),
            Neighbor::Placed {
                target: cell,
                rotation: 0,
            },
        );
    }
    let domain = MapWindow {
        min: pos(-64, -64, -64),
        max: pos(64, 64, 64),
    };
    let output = MapWindow {
        min: pos(64, 64, 64),
        max: pos(64, 64, 64),
    };
    let scene = [seen(cell, pos(0, 0, 0), 0)];
    let full = map.project(&scene, domain).unwrap();
    assert!(full.limited && !full.complete && full.cells.is_empty());
    assert!(!full.appearance_limited);
    assert!(full.work.projection_states <= MAX_STATES);
    assert!(full.work.candidate_relations <= MAX_RELATIONS);
    assert!(full.work.inverse_checks <= MAX_RELATIONS);
    assert!(full.work.range_seeks <= MAX_SEEKS);
    assert_clipped_projection(&map, &scene, output, domain, &full);
}

#[test]
fn acquisition_checks_new_world_references_and_does_not_trust_forged_roots() {
    let (mut game, actor) = game();
    record(&mut game, actor, &[seen(at(1, 1), pos(0, 0, 0), 0)]);
    let before = game.clone();
    assert_eq!(
        game.record_explored_observation(
            actor,
            &[MapCellInput {
                location: at(99, 1),
                appearance: appearance("forged"),
            }],
            &[]
        ),
        Err(GameError::InvalidLocation)
    );
    assert_eq!(game, before);
    game.explored_maps
        .get_mut(&actor)
        .unwrap()
        .cells
        .insert(at(99, 1), Shared::new(appearance("forged")));
    assert!(!game
        .map_validation
        .loaded_matches(actor, &game.explored_maps[&actor]));
    assert_eq!(
        game.record_explored_observation(actor, &[], &[]),
        Err(GameError::InvalidLocation)
    );
}

#[test]
fn validation_cache_is_derived_and_game_clones_do_not_copy_its_directories() {
    let (mut game, actor) = game();
    record(&mut game, actor, &[seen(at(1, 1), pos(0, 0, 0), 0)]);
    reset_map_work_counts();
    let mut clone = game.clone();
    assert_eq!(map_work_counts(), MapWorkCounts::default());
    clone.map_validation = MapValidationCache::default();
    assert_eq!(clone, game);
    let checkpoint = |game: &Game| {
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        serde_json::to_vec(&(snapshot, shared)).unwrap()
    };
    assert_eq!(checkpoint(&clone), checkpoint(&game));
}

#[test]
fn unchanged_observations_share_owners_and_translation_does_not_grow_evidence() {
    let (mut game, actor) = game();
    let a = seen(at(1, 1), pos(0, 0, 0), 0);
    let b = seen(at(1, 2), pos(1, 0, 0), 0);
    let c = seen(at(1, 9), pos(9, 0, 0), 0);
    record(&mut game, actor, &[a, b, c]);
    let before = game.clone();
    reset_map_work_counts();
    let translated = [a, b, c].map(|mut c| {
        c.offset.x += 1;
        c
    });
    record(&mut game, actor, &translated);
    assert!(game.explored_maps[&actor].shares_storage(&before.explored_maps[&actor]));
    let work = map_work_counts();
    assert_eq!(
        (
            work.owner_copies,
            work.appearance_changes,
            work.neighbor_changes,
            work.witness_changes
        ),
        (0, 0, 0, 0)
    );
    assert_eq!(game.explored_maps[&actor].children.iter().count(), 1);
}
#[test]
fn all_cube_frames_reproject_nonwalkable_surfaces_and_disconnected_islands() {
    for frame in 0..24 {
        let (mut game, actor) = game();
        let scene = [
            seen(at(1, 1), pos(0, 0, 0), frame),
            seen(at(1, 2), pos(1, 0, 0), frame),
            seen(at(1, 9), pos(7, 3, 0), frame),
        ];
        record(&mut game, actor, &scene);
        for new_frame in 0..24 {
            let seed = seen(at(1, 1), pos(0, 0, 0), new_frame);
            let result = game
                .project_explored_map(
                    actor,
                    &[seed],
                    MapWindow {
                        min: pos(-10, -10, -10),
                        max: pos(10, 10, 10),
                    },
                )
                .unwrap();
            assert!(
                result.complete && !result.ambiguous,
                "{frame}/{new_frame}: {result:?}"
            );
            for (cell, original) in result
                .cells
                .iter()
                .map(|c| (c, scene.iter().find(|s| s.location == c.location).unwrap()))
            {
                let expected = checked_rotate(
                    tor_world::inverse_rotation(new_frame),
                    checked_rotate(frame, vector(original.offset)).unwrap(),
                )
                .unwrap();
                assert_eq!(vector(cell.position), expected);
                assert!(cell.appearance.wall);
            }
            assert_eq!(result.cells.len(), 3);
            for cell in &result.cells {
                assert_clipped_projection(
                    &game.explored_maps[&actor],
                    &[seed],
                    MapWindow {
                        min: cell.position,
                        max: cell.position,
                    },
                    MapWindow {
                        min: pos(-10, -10, -10),
                        max: pos(10, 10, 10),
                    },
                    &result,
                );
            }
        }
    }
}
#[test]
fn synthetic_stair_landing_is_not_geometry_and_current_seed_beats_stale_rival() {
    let (mut game, actor) = game();
    let a = seen(at(1, 1), pos(0, 0, 0), 0);
    let b = seen(at(1, 2), pos(1, 0, 0), 0);
    let mut landing = seen(at(1, 30), pos(0, 0, 1), 0);
    landing.physical = false;
    record(&mut game, actor, &[a, b, landing]);
    let result = game.project_explored_map(actor, &[a], window()).unwrap();
    assert_eq!(result.cells.len(), 2);
    let rival = seen(at(1, 3), pos(1, 0, 0), 0);
    record(&mut game, actor, &[rival]);
    let result = game
        .project_explored_map(actor, &[a, rival], window())
        .unwrap();
    assert!(result.complete && result.ambiguous);
    assert!(result.cells.iter().any(|c| c.location == rival.location));
    assert!(!result.cells.iter().any(|c| c.location == b.location));
    assert_clipped_projection(
        &game.explored_maps[&actor],
        &[a, rival],
        MapWindow {
            min: rival.offset,
            max: rival.offset,
        },
        window(),
        &result,
    );
}
#[test]
fn inverse_witness_index_seeks_past_distant_history_without_candidate_scan() {
    let anchor = at(1, 1);
    let near = at(2, 1);
    let mut map = ExploredMap::default();
    map.cells.insert(anchor, Shared::new(appearance("anchor")));
    map.cells.insert(near, Shared::new(appearance("near")));
    map.witnesses.insert(
        near,
        Witness::Placed {
            anchor,
            delta: [0, 2, 0],
            rotation: 0,
        },
    );
    map.children.insert((anchor, [0, 2, 0], near), 0);
    let seed = seen(anchor, pos(0, 0, 0), 0);
    let before = map.project(&[seed], window()).unwrap();
    assert_eq!(
        before
            .cells
            .iter()
            .map(|cell| cell.location)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([anchor, near])
    );
    let mut one_distant = None;
    for i in 0..40_000 {
        let child = at(3, i);
        map.cells.insert(child, Shared::new(appearance("far")));
        map.witnesses.insert(
            child,
            Witness::Placed {
                anchor,
                delta: [0, -100_000 + i64::from(i), 0],
                rotation: 0,
            },
        );
        map.children
            .insert((anchor, [0, -100_000 + i64::from(i), 0], child), 0);
        if i == 0 {
            one_distant = Some(map.project(&[seed], window()).unwrap());
        }
    }
    let after = map.project(&[seed], window()).unwrap();
    let one_distant = one_distant.unwrap();
    assert_eq!(before.cells, after.cells);
    assert_eq!(one_distant.cells, after.cells);
    // Encountering an excluded run costs one seek to jump past it. Adding
    // the other 39,999 rows must not add per-row visits or hide nearby cells.
    assert!(one_distant.work.candidate_relations <= before.work.candidate_relations + 1);
    assert!(one_distant.work.range_seeks <= before.work.range_seeks + 1);
    assert_eq!(
        one_distant.work.candidate_relations,
        after.work.candidate_relations
    );
    assert_eq!(one_distant.work.range_seeks, after.work.range_seeks);
    assert!(after.complete && !after.ambiguous);
}
#[test]
fn more_than_4096_sequential_sightings_survive_return_without_history_projection() {
    let (mut game, actor) = game();
    for x in 1..5000 {
        record(
            &mut game,
            actor,
            &[
                seen(at(1, x), pos(0, 0, 0), 0),
                seen(at(1, x + 1), pos(1, 0, 0), 0),
            ],
        );
    }
    assert_eq!(game.explored_maps[&actor].cells.iter().count(), 5000);
    let result = game
        .project_explored_map(actor, &[seen(at(1, 1), pos(0, 0, 0), 0)], window())
        .unwrap();
    assert!(result.complete);
    assert_eq!(result.cells.len(), 32);
    assert!(result.work.projection_states <= 33);
    assert!(result.work.candidate_relations < 400);
}
#[test]
fn source_bucket_copy_growth_is_distinct_from_region_directory_growth() {
    let (mut game, actor) = game();
    for region in 2..=512 {
        game.add_region(tor_world::Region {
            id: RegionId(region),
            name: "memory".into(),
            bounds: tor_world::Extent::new(4, 3, 1).unwrap(),
        })
        .unwrap();
    }
    for (size, regions) in [(16, 1), (512, 1), (5000, 1), (3, 1), (3, 32), (3, 512)] {
        let mut map = ExploredMap::default();
        for x in 1..=size {
            map.cells.insert(at(1, x), Shared::new(appearance("old")));
            if x > 1 {
                map.neighbors.insert(
                    (at(1, x - 1), Direction::East),
                    Neighbor::Placed {
                        target: at(1, x),
                        rotation: 0,
                    },
                );
                map.neighbors.insert(
                    (at(1, x), Direction::West),
                    Neighbor::Placed {
                        target: at(1, x - 1),
                        rotation: 0,
                    },
                );
            }
        }
        for region in 2..=regions {
            for x in 1..=3 {
                map.cells
                    .insert(at(region, x), Shared::new(appearance("old")));
            }
        }
        game.explored_maps.insert(actor, Shared::new(map));
        let old = [MapCellInput {
            location: at(1, 1),
            appearance: appearance("old"),
        }];
        let new = [MapCellInput {
            location: at(1, 1),
            appearance: appearance("new"),
        }];
        // Fixture validation, map population and snapshot clones are outside
        // the bounded timing witness. Timings are diagnostic, never gates.
        game.record_explored_observation(actor, &old, &[]).unwrap();
        let mut samples = Vec::new();
        let mut receipt = None;
        for _ in 0..3 {
            let mut changed = game.clone();
            reset_map_work_counts();
            let started = std::time::Instant::now();
            game.record_explored_observation(actor, &old, &[]).unwrap();
            let unchanged_ns = started.elapsed().as_nanos();
            let unchanged = map_work_counts();
            assert_eq!(
                (
                    unchanged.owner_copies,
                    unchanged.validation_cells,
                    unchanged.appearance_changes
                ),
                (0, 0, 0)
            );
            assert_eq!(unchanged.world_location_checks, 1);
            reset_map_work_counts();
            let started = std::time::Instant::now();
            changed
                .record_explored_observation(actor, &new, &[])
                .unwrap();
            let changed_ns = started.elapsed().as_nanos();
            let work = map_work_counts();
            assert_eq!(work.owner_copies, 1);
            assert_eq!(work.directory_entries_copied, regions as usize + 1);
            assert_eq!(work.bucket_entries_copied, size as usize);
            assert_eq!(work.appearance_payload_copies, 1);
            assert_eq!(work.validation_cells, 0);
            assert_eq!(work.world_location_checks, 1);
            assert_eq!(
                game.explored_maps[&actor]
                    .cells
                    .get(&at(1, 1))
                    .unwrap()
                    .material,
                "old"
            );
            reset_map_work_counts();
            let started = std::time::Instant::now();
            let projection = game
                .project_explored_map(actor, &[seen(at(1, 1), pos(0, 0, 0), 0)], window())
                .unwrap();
            let reconnect_ns = started.elapsed().as_nanos();
            assert!(projection.complete);
            assert_eq!(projection.cells.len(), (size as usize).min(32));
            assert!(projection.work.projection_states <= 33);
            assert!(projection.work.candidate_relations < 400);
            assert_eq!(map_work_counts().validation_cells, 0);
            samples.push((unchanged_ns, changed_ns, reconnect_ns));
            receipt = Some((
                work.directory_entries_copied,
                work.bucket_entries_copied,
                work.validation_directory_entries_copied,
                projection.work.projection_states,
                projection.work.candidate_relations,
                projection.work.range_seeks,
            ));
        }
        println!("map_history_witness cells_in_source={size} regions={regions} samples_ns_unchanged_changed_reconnect={samples:?} counts_directory_bucket_validation_directory_states_candidates_seeks={:?}", receipt.unwrap());
    }
}
#[test]
fn ordinary_full_six_plane_viewport_fits_and_oversize_appearance_is_limited_before_clone() {
    let mut map = ExploredMap::default();
    let mut scene = Vec::new();
    let mut n = 0;
    for z in -2..=3 {
        for y in -16..=16 {
            for x in -31..=31 {
                let at = at(1, n);
                n += 1;
                map.cells.insert(at, Shared::new(appearance("stone")));
                scene.push(seen(at, pos(x, y, z), 0));
            }
        }
    }
    let result = map.project(&scene, window()).unwrap();
    assert!(result.complete && !result.limited);
    assert_eq!(result.cells.len(), 12_474);
    assert_eq!(result.work.projection_states, 12_474);
    let mut huge = appearance("stone");
    huge.ground_items = (0..4096)
        .map(|_| MapItemAppearance {
            presentation: None,
            class: ItemClass::Potion,
            description: "x".repeat(80),
            name: "bottle".into(),
            appearance: "bottle".into(),
            identified: false,
            quantity: 1,
            asset: None,
        })
        .collect();
    map.cells.insert(scene[0].location, Shared::new(huge));
    let limited = map.project(&scene, window()).unwrap();
    assert!(limited.limited && !limited.complete && limited.cells.is_empty());
    let output = MapWindow {
        min: pos(31, 16, 3),
        max: pos(31, 16, 3),
    };
    let small = map.project_region(&scene, output, window()).unwrap();
    assert!(small.complete && !small.appearance_limited);
    assert_eq!(small.cells.len(), 1);
    assert_eq!(small.cells[0].position, output.min);
    assert_eq!(small.cells[0].appearance.material, "stone");
    let oversized = MapWindow {
        min: scene[0].offset,
        max: scene[0].offset,
    };
    let limited = map.project_region(&scene, oversized, window()).unwrap();
    assert!(limited.appearance_limited && !limited.complete && limited.cells.is_empty());
}
#[test]
fn checked_rotation_rejects_sign_overflow_and_accepts_every_finite_cube_frame() {
    for rotation in 0..24 {
        assert_eq!(
            checked_rotate(rotation, [19, -7, 3]),
            Some(tor_world::rotate_vector(rotation, [19, -7, 3]))
        );
        for axis in 0..3 {
            let mut v = [0; 3];
            v[axis] = i64::MIN;
            let mut basis = [0; 3];
            basis[axis] = 1;
            let positive = tor_world::rotate_vector(rotation, basis)
                .iter()
                .all(|v| *v >= 0);
            assert_eq!(checked_rotate(rotation, v).is_some(), positive);
        }
    }
    assert!(checked_rotate(24, [0; 3]).is_none());
}
#[test]
fn repeated_occurrences_of_a_cyclic_cell_remain_distinct_and_never_add_navigation() {
    let (mut game, actor) = game();
    let before = game.navigation.clone();
    let a = seen(at(1, 1), pos(0, 0, 0), 0);
    let repeated = seen(a.location, pos(1, 0, 0), 0);
    record(&mut game, actor, &[a, repeated]);
    assert_eq!(game.navigation, before);
    let result = game.project_explored_map(actor, &[a], window()).unwrap();
    assert!(result.complete && !result.ambiguous);
    assert_eq!(result.cells.len(), 63);
    assert!(result.cells.iter().all(|c| c.location == a.location));
    assert_eq!(
        result
            .cells
            .iter()
            .map(|c| c.position)
            .collect::<BTreeSet<_>>()
            .len(),
        63
    );
    for (min, max) in [(-31, -31), (-1, 1), (31, 31)] {
        assert_clipped_projection(
            &game.explored_maps[&actor],
            &[a],
            MapWindow {
                min: pos(min, 0, 0),
                max: pos(max, 0, 0),
            },
            window(),
            &result,
        );
    }
}
#[test]
fn conflicting_cycle_suppresses_only_ambiguous_placements() {
    let mut map = ExploredMap::default();
    let [a, b, c, d, e] = [1, 2, 3, 4, 5].map(|x| at(1, x));
    for cell in [a, b, c, d, e] {
        map.cells.insert(cell, Shared::new(appearance("wall")));
    }
    let mut link = |from, direction, to| {
        map.neighbors.insert(
            (from, direction),
            Neighbor::Placed {
                target: to,
                rotation: 0,
            },
        );
        map.neighbors.insert(
            (to, opposite(direction)),
            Neighbor::Placed {
                target: from,
                rotation: 0,
            },
        );
    };
    link(a, Direction::East, b);
    link(a, Direction::North, d);
    link(b, Direction::North, c);
    link(d, Direction::East, e);
    let result = map.project(&[seen(a, pos(0, 0, 0), 0)], window()).unwrap();
    assert!(result.complete && result.ambiguous);
    assert_eq!(
        result
            .cells
            .iter()
            .map(|c| c.location)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([a, b, d])
    );
    for output in [
        MapWindow {
            min: pos(1, -1, 0),
            max: pos(1, 0, 0),
        },
        MapWindow {
            min: pos(31, 0, 0),
            max: pos(31, 0, 0),
        },
    ] {
        assert_clipped_projection(&map, &[seen(a, pos(0, 0, 0), 0)], output, window(), &result);
    }
}
#[test]
fn observers_keep_independent_last_seen_values_and_observed_emptiness_clears_items() {
    let (mut game, first) = game();
    let second = game
        .spawn_actor(at(1, 2), NonZeroU64::new(100).unwrap())
        .unwrap();
    let cell = seen(at(1, 1), pos(0, 0, 0), 0);
    let mut old = appearance("closed wall");
    old.door = Some(MapDoorAppearance {
        name: "old gate".into(),
        description: "old gate".into(),
        open: false,
        asset: Some("old door".into()),
    });
    old.ground_items.push(MapItemAppearance {
        presentation: None,
        class: ItemClass::Potion,
        description: "unknown bottle".into(),
        name: "blue bottle".into(),
        appearance: "blue bottle".into(),
        identified: false,
        quantity: 2,
        asset: Some("old bottle".into()),
    });
    for observer in [first, second] {
        game.record_explored_observation(
            observer,
            &[MapCellInput {
                location: cell.location,
                appearance: old.clone(),
            }],
            &[cell],
        )
        .unwrap();
    }
    let root = game.map_knowledge_root(second);
    game.record_explored_observation(
        first,
        &[MapCellInput {
            location: cell.location,
            appearance: appearance("open floor"),
        }],
        &[cell],
    )
    .unwrap();
    assert!(game.map_knowledge_is_current(second, &root));
    let stale = game
        .project_explored_map(second, &[cell], window())
        .unwrap();
    assert_eq!(&*stale.cells[0].appearance, &old);
    let fresh = game.project_explored_map(first, &[cell], window()).unwrap();
    assert!(fresh.cells[0].appearance.ground_items.is_empty());
    assert!(fresh.cells[0].appearance.door.is_none());
    assert!(
        game.map_knowledge_is_current(second, &root),
        "read-only projection must not learn"
    );
}
#[test]
fn legitimate_oversized_appearance_is_acquired_and_persisted_but_not_materialized_for_wire() {
    let (mut game, actor) = game();
    let cell = seen(at(1, 1), pos(0, 0, 0), 0);
    let mut large = appearance("stone");
    large.asset = Some("a".repeat(8192));
    large.ground_items = (0..4097)
        .map(|_| MapItemAppearance {
            presentation: None,
            class: ItemClass::Potion,
            description: String::new(),
            name: "bottle".into(),
            appearance: "bottle".into(),
            identified: false,
            quantity: 1,
            asset: None,
        })
        .collect();
    game.record_explored_observation(
        actor,
        &[MapCellInput {
            location: cell.location,
            appearance: large.clone(),
        }],
        &[cell],
    )
    .unwrap();
    let saved = serde_json::to_vec(&*game.explored_maps[&actor]).unwrap();
    let restored: ExploredMap = serde_json::from_slice(&saved).unwrap();
    assert!(restored.checkpoint_valid(&game.world));
    assert_eq!(&**restored.cells.get(&cell.location).unwrap(), &large);
    reset_map_work_counts();
    let result = restored.project(&[cell], window()).unwrap();
    assert!(result.limited && result.cells.is_empty());
    assert_eq!(map_work_counts().appearance_payload_copies, 0);
}

#[test]
fn blocked_state_discovery_is_translation_invariant() {
    let mut map = ExploredMap::default();
    let [a, x, b, c, d, f] = [1, 2, 3, 4, 5, 6].map(|n| at(1, n));
    for cell in [a, x, b, c, d, f] {
        map.cells.insert(cell, Shared::new(appearance("wall")));
    }
    for (anchor, delta, child) in [(a, [2, 0, 0], b), (a, [3, 0, 0], f), (x, [-8, 0, 0], c)] {
        map.witnesses.insert(
            child,
            Witness::Placed {
                anchor,
                delta,
                rotation: 0,
            },
        );
        map.children.insert((anchor, delta, child), 0);
    }
    map.neighbors.insert(
        (b, Direction::East),
        Neighbor::Placed {
            target: d,
            rotation: 0,
        },
    );
    map.neighbors.insert(
        (d, Direction::West),
        Neighbor::Placed {
            target: b,
            rotation: 0,
        },
    );
    let projected = |shift| {
        map.project(
            &[
                seen(a, pos(shift, 0, 0), 0),
                seen(x, pos(shift + 10, 0, 0), 0),
            ],
            window(),
        )
        .unwrap()
    };
    let first = projected(0);
    let translated = projected(-10);
    for result in [&first, &translated] {
        assert!(result.complete && result.ambiguous && !result.limited);
        assert_eq!(
            result
                .cells
                .iter()
                .map(|c| c.location)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([a, x])
        );
    }
    assert_eq!(
        first
            .cells
            .iter()
            .map(|c| (c.location, c.position))
            .collect::<Vec<_>>(),
        translated
            .cells
            .iter()
            .map(|c| (c.location, offset(c.position, [10, 0, 0]).unwrap()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn saved_item_labels_obey_the_ordinary_source_bounds() {
    let mut item = MapItemAppearance {
        presentation: None,
        class: ItemClass::Potion,
        description: String::new(),
        name: "bottle".into(),
        appearance: "blue bottle".into(),
        identified: false,
        quantity: 1,
        asset: Some("a".repeat(8192)),
    };
    for invalid in [String::new(), "x".repeat(81)] {
        let mut bad = item.clone();
        bad.name = invalid.clone();
        assert!(
            serde_json::from_value::<MapItemAppearance>(serde_json::to_value(bad).unwrap())
                .is_err()
        );
        let mut bad = item.clone();
        bad.appearance = invalid;
        assert!(
            serde_json::from_value::<MapItemAppearance>(serde_json::to_value(bad).unwrap())
                .is_err()
        );
    }
    item.name = "x".repeat(80);
    item.appearance = "y".repeat(80);
    assert_eq!(
        serde_json::from_value::<MapItemAppearance>(serde_json::to_value(&item).unwrap()).unwrap(),
        item
    );
    let mut value = appearance("stone");
    value.ground_items.push(item);
    assert!(value.valid());
    value.ground_items[0].name.clear();
    assert!(!value.valid());
    value.ground_items[0].name = "bottle".into();
    value.ground_items[0].appearance = "x".repeat(81);
    assert!(!value.valid());
}
