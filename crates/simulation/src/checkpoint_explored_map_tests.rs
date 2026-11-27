use super::*;
use crate::explored_map::{MapAppearance, MapDoorAppearance, MapItemAppearance, Neighbor, Witness};
use std::num::NonZeroU64;
use tor_world::{Direction, Extent, Position, Region, RegionId};

#[derive(Serialize, Deserialize)]
struct Maps {
    #[serde(with = "super::explored_map_regions")]
    maps: Vec<Shared<ExploredMap>>,
}

fn at(region: u64, x: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y: 1, z: 0 },
    }
}

fn appearance(material: &str) -> Shared<MapAppearance> {
    Shared::new(MapAppearance {
        object_presentation: None,
        wall: true,
        material: material.into(),
        asset: Some("wall/remembered".into()),
        place_hint: true,
        stairs_up: true,
        stairs_down: false,
        trap: None,
        door: Some(MapDoorAppearance {
            name: "old gate".into(),
            description: "last seen closed".into(),
            open: false,
            asset: Some("door/closed".into()),
        }),
        ground_items: vec![MapItemAppearance {
            presentation: None,
            class: crate::ItemClass::Potion,
            description: "a blue bottle".into(),
            name: "blue potion".into(),
            appearance: "blue".into(),
            identified: false,
            quantity: 3,
            asset: Some("item/blue".into()),
        }],
    })
}

fn map() -> ExploredMap {
    let mut map = ExploredMap::default();
    for location in [at(1, 1), at(1, 2), at(2, 1)] {
        map.cells.insert(location, appearance("remembered stone"));
    }
    map.neighbors.insert(
        (at(1, 1), Direction::East),
        Neighbor::Placed {
            target: at(1, 2),
            rotation: 0,
        },
    );
    map.neighbors.insert(
        (at(1, 2), Direction::West),
        Neighbor::Placed {
            target: at(1, 1),
            rotation: 0,
        },
    );
    map.witnesses.insert(
        at(2, 1),
        Witness::Placed {
            anchor: at(1, 1),
            delta: [10, 0, 0],
            rotation: 0,
        },
    );
    map.children.insert((at(1, 1), [10, 0, 0], at(2, 1)), 0);
    map
}

fn large_map(cells: i32) -> ExploredMap {
    let mut map = ExploredMap::default();
    let visual = appearance("shared history");
    for x in 0..cells {
        map.cells.insert(at(1, x), visual.clone());
    }
    map.neighbors.insert(
        (at(1, 0), Direction::East),
        Neighbor::Placed {
            target: at(1, 1),
            rotation: 0,
        },
    );
    map.neighbors.insert(
        (at(1, 1), Direction::West),
        Neighbor::Placed {
            target: at(1, 0),
            rotation: 0,
        },
    );
    let delta = [i64::from(cells - 1), 0, 0];
    map.witnesses.insert(
        at(1, cells - 1),
        Witness::Placed {
            anchor: at(1, 0),
            delta,
            rotation: 0,
        },
    );
    map.children.insert((at(1, 0), delta, at(1, cells - 1)), 0);
    map
}

fn roots_with_unrelated_changes(cells: i32, roots: usize) -> Vec<Shared<ExploredMap>> {
    let base = large_map(cells);
    (0..roots)
        .map(|root| {
            let mut map = base.clone();
            map.cells
                .insert(at(2, 1), appearance(&format!("unrelated change {root}")));
            Shared::new(map)
        })
        .collect()
}

#[test]
fn duplicate_root_descriptors_fail_before_history_sized_validation() {
    use super::explored_map_regions::{reset_work_counts, work_counts};
    let map = Shared::new(large_map(4096));
    for roots in [2, 128] {
        let bytes = serde_json::to_vec(&Maps {
            maps: vec![map.clone(); roots],
        })
        .unwrap();
        reset_work_counts();
        crate::reset_map_work_counts();
        assert!(serde_json::from_slice::<Maps>(&bytes).is_err());
        let work = work_counts();
        assert_eq!(work.descriptor_checks, 2);
        assert_eq!(work.appearance_cells, 0);
        assert_eq!(work.neighbor_rows + work.witness_rows + work.child_rows, 0);
        assert_eq!(crate::map_work_counts().validation_cells, 0);
        assert_eq!(crate::map_work_counts().validation_relations, 0);
    }
}

#[test]
fn redundant_bucket_indices_cannot_hide_duplicate_map_roots() {
    use super::explored_map_regions::{reset_work_counts, work_counts};
    let mut value = serde_json::to_value(Maps {
        maps: vec![Shared::new(large_map(1024))],
    })
    .unwrap();
    let cells = value["maps"]["cells"][0].clone();
    value["maps"]["cells"].as_array_mut().unwrap().push(cells);
    let mut alias = value["maps"]["instances"][0].clone();
    alias["cells"] = serde_json::json!([1]);
    value["maps"]["instances"]
        .as_array_mut()
        .unwrap()
        .push(alias);
    reset_work_counts();
    assert!(serde_json::from_value::<Maps>(value).is_err());
    assert_eq!(work_counts().appearance_cells, 0);
    assert_eq!(
        work_counts().neighbor_rows + work_counts().witness_rows + work_counts().child_rows,
        0
    );
}

#[test]
fn distinct_roots_validate_shared_payloads_and_actual_relation_dependencies_once() {
    use super::explored_map_regions::{reset_work_counts, work_counts};
    for roots in [1, 32] {
        let maps = roots_with_unrelated_changes(4096, roots);
        let bytes = serde_json::to_vec(&Maps { maps: maps.clone() }).unwrap();
        reset_work_counts();
        crate::reset_map_work_counts();
        let restored: Maps = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.maps, maps);
        let work = work_counts();
        assert_eq!(work.appearance_cells, 4096 + roots);
        assert_eq!(
            work.neighbor_rows, 4,
            "unrelated region changes cannot invalidate links"
        );
        assert_eq!(work.witness_rows, 2);
        assert_eq!(work.child_rows, 2);
        assert_eq!(crate::map_work_counts().validation_cells, 4096 + roots);
        assert_eq!(crate::map_work_counts().validation_relations, 8);
    }
}

#[test]
fn later_shared_bucket_versions_are_canonicalized_once_before_content_comparison() {
    const CELLS: i32 = 4096;
    for roots in [1, 32, 64] {
        let original = Shared::new(large_map(CELLS));
        let mut later = (*original).clone();
        later
            .cells
            .insert(at(1, CELLS - 1), appearance("later version"));
        let mut maps = vec![original];
        for root in 0..roots {
            let mut map = later.clone();
            map.cells
                .insert(at(2, 1), appearance(&format!("tiny change {root:03}")));
            maps.push(Shared::new(map));
        }
        crate::reset_map_work_counts();
        let _validation = super::explored_map_regions::RestoreValidation::new(&maps).unwrap();
        let work = crate::map_work_counts();
        assert_eq!(work.pool_bucket_allocations, roots + 5);
        assert_eq!(work.validation_cells, 2 * CELLS as usize + roots);
        assert!(work.pool_identity_hits >= roots * 3);
        // The second large bucket is compared once with the first; extra
        // roots compare only their one-cell unrelated buckets.
        assert!(work.pool_compared_entries >= CELLS as usize);
        assert!(work.pool_compared_entries <= CELLS as usize + roots * 32);
    }
}

#[test]
fn near_equal_distinct_bucket_versions_use_ordered_not_linear_canonicalization() {
    const CELLS: i32 = 256;
    for versions in [8, 128] {
        let original = large_map(CELLS);
        let maps = (0..versions)
            .map(|version| {
                let mut map = original.clone();
                map.cells.insert(
                    at(1, CELLS - 1),
                    appearance(&format!("version {version:03}")),
                );
                Shared::new(map)
            })
            .collect::<Vec<_>>();
        crate::reset_map_work_counts();
        let _validation = super::explored_map_regions::RestoreValidation::new(&maps).unwrap();
        let work = crate::map_work_counts();
        assert_eq!(work.pool_bucket_allocations, versions + 3);
        assert_eq!(work.validation_cells, CELLS as usize * versions);
        assert!(work.pool_compared_entries > 0);
        // An ordered B-tree search is logarithmic in versions; this loose
        // ceiling allows its small-node comparisons but excludes V*(V-1)/2
        // full scans for the 128-version witness.
        assert!(work.pool_compared_entries <= CELLS as usize * versions * 36);
    }
}

#[test]
fn owner_batches_share_bucket_validation_across_distinct_and_equal_roots() {
    let mut roots = roots_with_unrelated_changes(4096, 32);
    // Equal content in a distinct root is valid for a different owner, and
    // repeated ownership of that exact root must also stay constant-work.
    roots.push(Shared::new((*roots[0]).clone()));
    roots.extend(vec![roots[0].clone(); 32]);
    let world = validation_world(4096, 0);
    crate::reset_map_work_counts();
    let mut validation = super::ExploredOwnerValidation::new(roots.iter()).unwrap();
    for root in &roots {
        assert!(validation.structure(root));
        assert!(validation.world(root, &world));
    }
    let work = crate::map_work_counts();
    assert_eq!(work.validation_cells, 4096 + 32);
    assert_eq!(work.validation_relations, 8);
    assert_eq!(work.world_location_checks, 4096 + 32);
}

#[test]
fn pooled_relation_cache_never_crosses_changed_endpoints_or_reverse_indices() {
    let original = Shared::new(large_map(64));
    for damage_reverse in [false, true] {
        let mut damaged = original.clone();
        if damage_reverse {
            damaged
                .children
                .insert((at(1, 0), [63, 0, 0], at(1, 63)), 1);
        } else {
            damaged.cells.remove(&at(1, 1));
        }
        let maps = Maps {
            maps: vec![original.clone(), damaged],
        };
        let bytes = serde_json::to_vec(&maps).unwrap();
        assert!(serde_json::from_slice::<Maps>(&bytes).is_err());
        // Also exercise forged in-memory state, which bypasses serde.
        let mut validator =
            super::explored_map_regions::RestoreValidation::new(&maps.maps).unwrap();
        let mut world = World::new(vec![], vec![]).unwrap();
        world
            .add_region(Region {
                id: RegionId(1),
                name: "memory".into(),
                bounds: Extent::new(64, 3, 1).unwrap(),
            })
            .unwrap();
        assert!(validator.validate(0, &world).is_some());
        assert!(validator.validate(1, &world).is_none());
    }
}

fn validation_world(cells: i32, label: usize) -> World {
    let mut world = World::new(vec![], vec![]).unwrap();
    for (id, width) in [(1, cells), (2, 3)] {
        world
            .add_region(Region {
                id: RegionId(id),
                name: format!("world {label} region {id}"),
                bounds: Extent::new(width, 3, 1).unwrap(),
            })
            .unwrap();
    }
    world
}

#[test]
fn pooled_world_cache_tracks_bounds_and_reuses_loaded_or_detached_metadata() {
    use super::explored_map_regions::{reset_work_counts, work_counts, RestoreValidation};
    let mut validation = RestoreValidation::new(&[Shared::new(large_map(64))]).unwrap();
    let loaded = validation_world(64, 0);
    reset_work_counts();
    assert!(validation.validate(0, &loaded).is_some());
    assert_eq!(work_counts().world_cells, 64);
    let mut detached = World::new(vec![], vec![]).unwrap();
    detached
        .add_unbuilt_region(loaded.known_region(RegionId(1)).unwrap().clone(), false)
        .unwrap();
    assert!(validation.validate(0, &detached).is_some());
    assert_eq!(
        work_counts().world_cells,
        64,
        "residency does not change membership"
    );
    assert!(validation.validate(0, &validation_world(63, 1)).is_none());
    assert!(validation
        .validate(0, &World::new(vec![], vec![]).unwrap())
        .is_none());
    let mut shifted = World::new(vec![], vec![]).unwrap();
    let mut bounds = serde_json::to_value(Extent::new(64, 3, 1).unwrap()).unwrap();
    bounds["origin"]["x"] = serde_json::json!(1);
    shifted
        .add_region(Region {
            id: RegionId(1),
            name: "shifted origin".into(),
            bounds: serde_json::from_value(bounds).unwrap(),
        })
        .unwrap();
    assert!(validation.validate(0, &shifted).is_none());
    assert!(validation.validate(0, &loaded).is_some());
}

#[test]
fn restore_context_reuses_shared_buckets_across_distinct_roots_and_worlds() {
    use super::explored_map_regions::{reset_work_counts, work_counts};
    let roots = roots_with_unrelated_changes(4096, 32);
    let mut game = Game::new(
        validation_world(4096, 0),
        42,
        crate::test_creatures::content(),
    );
    let actor = game
        .spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let mut shared = SharedState::default();
    let mut snapshots = Vec::new();
    for (label, map) in roots.into_iter().enumerate() {
        game.explored_maps.insert(actor, map);
        let mut snapshot = game.checkpoint(&mut shared);
        // Distinct immutable worlds with equal relevant bounds are safe to
        // reuse. Names and residency are not knowledge membership facts.
        shared.worlds.push(validation_world(4096, label + 1));
        snapshot.world = shared.worlds.len() - 1;
        snapshots.push(snapshot);
    }
    assert!(shared.explored_map_references_valid(snapshots.iter()));
    let bytes = serde_json::to_vec(&(snapshots, shared)).unwrap();
    let (snapshots, decoded): (Vec<Snapshot>, SharedState) =
        serde_json::from_slice(&bytes).unwrap();
    reset_work_counts();
    crate::reset_map_work_counts();
    let mut context = RestoreContext::new(&decoded, game.content().clone());
    context
        .bind_spatial_roots(&snapshots.iter().collect::<Vec<_>>())
        .unwrap();
    for snapshot in snapshots {
        let restored = context.restore(snapshot).unwrap();
        assert!(restored
            .map_validation
            .loaded_matches(actor, &restored.explored_maps[&actor]));
    }
    let work = work_counts();
    assert_eq!(work.appearance_cells, 4096 + 32);
    assert_eq!(work.neighbor_rows, 4);
    assert_eq!(work.witness_rows, 2);
    assert_eq!(work.child_rows, 2);
    assert_eq!(work.world_cells, 4096 + 32);
    assert_eq!(crate::map_work_counts().validation_cells, 4096 + 32);
    assert_eq!(crate::map_work_counts().validation_relations, 8);
    assert_eq!(crate::map_work_counts().world_location_checks, 4096 + 32);
}

#[test]
fn complete_checkpoint_reference_validation_rejects_unused_and_dangling_roots() {
    let mut game = Game::new(
        validation_world(64, 0),
        42,
        crate::test_creatures::content(),
    );
    let actor = game
        .spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let roots = roots_with_unrelated_changes(64, 2);
    let mut shared = SharedState::default();
    game.explored_maps.insert(actor, roots[0].clone());
    let before = game.checkpoint(&mut shared);
    game.explored_maps.insert(actor, roots[1].clone());
    let after = game.checkpoint(&mut shared);
    assert!(shared.explored_map_references_valid([&before, &after]));
    assert!(!shared.explored_map_references_valid([&before]));
    let mut dangling = game.checkpoint(&mut shared);
    dangling.explored_maps = usize::MAX;
    assert!(!shared.explored_map_references_valid([&before, &after, &dangling]));
    dangling.explored_maps = shared.explored_map_owners.len();
    shared
        .explored_map_owners
        .push(BTreeMap::from([(actor, usize::MAX)]));
    assert!(!shared.explored_map_references_valid([&before, &after, &dangling]));
    shared.explored_map_owners.pop();
    // Every directory must be used, even when it adds no new map roots.
    shared.explored_map_owners.push(BTreeMap::new());
    assert!(!shared.explored_map_references_valid([&before, &after]));
    shared.explored_map_owners.pop();
    // Duplicate equal roots forged directly also fail before payload scans.
    shared.explored_maps.push(shared.explored_maps[0].clone());
    assert!(!shared.explored_map_references_valid([&before, &after]));
    let mut forged = game.checkpoint(&mut shared);
    forged.explored_maps = shared.explored_map_owners.len();
    shared
        .explored_map_owners
        .push(BTreeMap::from([(actor, shared.explored_maps.len() - 1)]));
    super::explored_map_regions::reset_work_counts();
    let mut context = RestoreContext::new(&shared, game.content().clone());
    context
        .bind_spatial_roots(&[&before, &after, &forged])
        .unwrap();
    assert!(context.restore(forged).is_err());
    assert_eq!(
        super::explored_map_regions::work_counts().appearance_cells,
        0
    );
}

#[test]
fn explored_map_region_pool_restores_exact_stale_values_and_sharing() {
    let before = Shared::new(map());
    let mut after = before.clone();
    after.cells.insert(at(1, 1), appearance("newly seen wood"));
    let original = Maps {
        maps: vec![before.clone(), after.clone()],
    };
    let bytes = serde_json::to_vec(&original).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["maps"]["cells"].as_array().unwrap().len(), 3);
    assert_eq!(value["maps"]["neighbors"].as_array().unwrap().len(), 1);
    assert_eq!(value["maps"]["witnesses"].as_array().unwrap().len(), 1);
    assert_eq!(value["maps"]["children"].as_array().unwrap().len(), 1);
    let mut decoded: Maps = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded.maps, original.maps);
    assert_eq!(serde_json::to_vec(&decoded).unwrap(), bytes);
    assert!(std::ptr::eq(
        decoded.maps[0].cells.region(RegionId(2)).unwrap(),
        decoded.maps[1].cells.region(RegionId(2)).unwrap(),
    ));
    assert!(std::ptr::eq(
        decoded.maps[0].neighbors.region(RegionId(1)).unwrap(),
        decoded.maps[1].neighbors.region(RegionId(1)).unwrap(),
    ));
    assert!(std::ptr::eq(
        decoded.maps[0].witnesses.region(RegionId(2)).unwrap(),
        decoded.maps[1].witnesses.region(RegionId(2)).unwrap(),
    ));
    assert!(std::ptr::eq(
        decoded.maps[0].children.region(RegionId(1)).unwrap(),
        decoded.maps[1].children.region(RegionId(1)).unwrap(),
    ));
    decoded.maps[0]
        .cells
        .insert(at(2, 1), appearance("changed after restore"));
    assert_eq!(decoded.maps[1], after);
    assert_eq!(
        decoded.maps[1].cells.get(&at(2, 1)).unwrap().material,
        "remembered stone"
    );
}

#[test]
fn compact_palette_aliases_validate_large_appearance_payloads_once() {
    use super::explored_map_regions::{reset_work_counts, work_counts};
    const CELLS: i32 = 2048;
    let mut visual = appearance("large stale item stack");
    visual.ground_items = vec![visual.ground_items[0].clone(); 4097];
    let mut before = ExploredMap::default();
    for x in 0..CELLS {
        before.cells.insert(at(1, x), visual.clone());
    }
    let mut after = before.clone();
    let mut changed = visual.clone();
    changed.material = "later stale item stack".into();
    after.cells.insert(at(1, CELLS - 1), changed);
    let expected = vec![Shared::new(before), Shared::new(after)];
    let bytes = serde_json::to_vec(&Maps {
        maps: expected.clone(),
    })
    .unwrap();
    reset_work_counts();
    let restored: Maps = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(work_counts().appearance_cells, 2 * CELLS as usize);
    assert_eq!(
        work_counts().appearance_payloads,
        2,
        "cell aliases must not multiply scans of unbounded legal item vectors"
    );
    for (restored, expected) in restored.maps.iter().zip(&expected) {
        assert_eq!(
            restored.cells.keys().collect::<Vec<_>>(),
            expected.cells.keys().collect::<Vec<_>>()
        );
        for at in [at(1, 0), at(1, CELLS - 1)] {
            assert_eq!(restored.cells.get(&at), expected.cells.get(&at));
        }
        let first = restored.cells.get(&at(1, 0)).unwrap();
        assert!(restored
            .cells
            .iter()
            .take((CELLS - 1) as usize)
            .all(|(_, appearance)| appearance.shares_storage(first)));
    }
    assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
}

#[test]
fn compact_map_palette_columns_and_neighbor_masks_round_trip_exactly() {
    let mut original = map();
    let center = Location {
        region: RegionId(1),
        position: Position { x: 5, y: 5, z: 5 },
    };
    original
        .cells
        .insert(center, appearance("stale painted stone"));
    for direction in [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
        Direction::Up,
        Direction::Down,
    ] {
        let (x, y, z) = direction.delta();
        let target = Location {
            region: center.region,
            position: Position {
                x: center.position.x + x,
                y: center.position.y + y,
                z: center.position.z + z,
            },
        };
        original
            .cells
            .insert(target, appearance("stale painted stone"));
        original.neighbors.insert(
            (center, direction),
            Neighbor::Placed {
                target,
                rotation: 0,
            },
        );
    }
    original.neighbors.insert(
        (at(1, 2), Direction::North),
        Neighbor::Placed {
            target: at(2, 1),
            rotation: 0,
        },
    );
    original.neighbors.insert(
        (at(1, 2), Direction::East),
        Neighbor::Placed {
            target: at(1, 1),
            rotation: 7,
        },
    );
    original
        .neighbors
        .insert((at(1, 2), Direction::Up), Neighbor::Ambiguous);
    let expected = vec![Shared::new(original)];
    let bytes = serde_json::to_vec(&Maps {
        maps: expected.clone(),
    })
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["maps"]["palette"].as_array().unwrap().len(), 2);
    let plain = value["maps"]["neighbors"][0]["plain"].as_array().unwrap();
    assert!(plain.contains(&serde_json::json!([5, 5, [[5, 63]]])));
    assert_eq!(
        value["maps"]["neighbors"][0]["other"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let restored: Maps = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored.maps, expected);
    assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
    assert!(restored.maps[0].cells.get(&center).unwrap().shares_storage(
        restored.maps[0]
            .cells
            .get(&Location {
                region: center.region,
                position: Position {
                    z: 6,
                    ..center.position
                },
            })
            .unwrap()
    ));
    // Existing witnesses/children remain explicit, with their exact stale values.
    assert_eq!(restored.maps[0].witnesses, expected[0].witnesses);
    assert_eq!(restored.maps[0].children, expected[0].children);
}

#[test]
fn compact_map_codec_rejects_noncanonical_palette_columns_and_masks() {
    let good = serde_json::to_value(Maps {
        maps: vec![Shared::new(map())],
    })
    .unwrap();
    let reject = |value| assert!(serde_json::from_value::<Maps>(value).is_err());
    let mut bad = good.clone();
    let duplicate = bad["maps"]["palette"][0].clone();
    bad["maps"]["palette"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    reject(bad);
    let mut bad = good.clone();
    bad["maps"]["palette"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::to_value(appearance("zz unused")).unwrap());
    reject(bad);
    let mut two = map();
    two.cells
        .insert(at(1, 1), appearance("another stale value"));
    let mut bad = serde_json::to_value(Maps {
        maps: vec![Shared::new(two)],
    })
    .unwrap();
    bad["maps"]["palette"].as_array_mut().unwrap().reverse();
    reject(bad);
    let mut bad = good.clone();
    bad["maps"]["cells"][0]["cells"][0][2][0][1] = serde_json::json!(999);
    reject(bad);
    for columns in [
        serde_json::json!([]),
        serde_json::json!([[1, 1, []]]),
        serde_json::json!([[1, 1, [[0, 0], [0, 0]]]]),
        serde_json::json!([[1, 1, [[1, 0], [0, 0]]]]),
        serde_json::json!([[1, 1, [[0, 0]]], [1, 1, [[1, 0]]]]),
        serde_json::json!([[2, 1, [[0, 0]]], [1, 1, [[0, 0]]]]),
    ] {
        let mut bad = good.clone();
        bad["maps"]["cells"][0]["cells"] = columns;
        reject(bad);
    }
    for columns in [
        serde_json::json!([[1, 1, []]]),
        serde_json::json!([[1, 1, [[0, 0]]]]),
        serde_json::json!([[1, 1, [[0, 64]]]]),
        serde_json::json!([[1, 1, [[0, -1]]]]),
        serde_json::json!([[1, 1, [[0, 2], [0, 2]]]]),
        serde_json::json!([[1, 1, [[1, 2], [0, 2]]]]),
        serde_json::json!([[2, 1, [[0, 2]]], [1, 1, [[0, 2]]]]),
        serde_json::json!([[i32::MAX, 1, [[0, 2]]]]),
        serde_json::json!([[i32::MIN, 1, [[0, 8]]]]),
        serde_json::json!([[1, i32::MAX, [[0, 4]]]]),
        serde_json::json!([[1, i32::MIN, [[0, 1]]]]),
        serde_json::json!([[1, 1, [[i32::MAX, 16]]]]),
        serde_json::json!([[1, 1, [[i32::MIN, 32]]]]),
    ] {
        let mut bad = good.clone();
        bad["maps"]["neighbors"][0]["plain"] = columns;
        reject(bad);
    }
}

#[test]
fn compact_map_codec_rejects_noncanonical_exceptions_and_unnamed_records() {
    let good = serde_json::to_value(Maps {
        maps: vec![Shared::new(map())],
    })
    .unwrap();
    let reject = |value| assert!(serde_json::from_value::<Maps>(value).is_err());
    let placed = serde_json::json!([[at(1, 1), "East"],
            {"kind":"placed", "target":at(1, 2), "rotation":0}]);
    let ambiguous = serde_json::json!([[at(1, 1), "East"], {"kind":"ambiguous"}]);
    let later = serde_json::json!([[at(1, 2), "East"], {"kind":"ambiguous"}]);
    for (plain, other) in [
        (serde_json::json!([]), serde_json::json!([placed])),
        (
            good["maps"]["neighbors"][0]["plain"].clone(),
            serde_json::json!([ambiguous.clone()]),
        ),
        (
            serde_json::json!([]),
            serde_json::json!([ambiguous.clone(), ambiguous.clone()]),
        ),
        (serde_json::json!([]), serde_json::json!([later, ambiguous])),
        (
            serde_json::json!([]),
            serde_json::json!([[[at(2, 1), "East"], {"kind":"ambiguous"}]]),
        ),
        (
            serde_json::json!([]),
            serde_json::json!([[[at(1, 1), "NorthEast"], {"kind":"ambiguous"}]]),
        ),
        (serde_json::json!([]), serde_json::json!([])),
    ] {
        let mut bad = good.clone();
        bad["maps"]["neighbors"][0]["plain"] = plain;
        bad["maps"]["neighbors"][0]["other"] = other;
        reject(bad);
    }
    for (path, fields) in [
        (
            "/maps",
            vec![
                "palette",
                "cells",
                "neighbors",
                "witnesses",
                "children",
                "instances",
            ],
        ),
        ("/maps/cells/0", vec!["region", "cells"]),
        ("/maps/neighbors/0", vec!["region", "plain", "other"]),
    ] {
        let mut bad = good.clone();
        *bad.pointer_mut(path).unwrap() = serde_json::json!([]);
        reject(bad);
        let mut bad = good.clone();
        bad.pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), serde_json::json!(0));
        reject(bad);
        for field in fields {
            let mut bad = good.clone();
            bad.pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            reject(bad);
        }
    }
    let mut legacy = good;
    legacy["maps"]["cells"] = serde_json::to_value(vec![map().cells]).unwrap();
    reject(legacy);
}

#[test]
fn explored_map_pool_rejects_invalid_order_regions_references_and_missing_fields() {
    let good = serde_json::to_value(Maps {
        maps: vec![Shared::new(map())],
    })
    .unwrap();
    let reject = |value: serde_json::Value| {
        assert!(serde_json::from_value::<Maps>(value).is_err());
    };
    let mut bad = good.clone();
    bad["maps"]["cells"][0]["cells"]
        .as_array_mut()
        .unwrap()
        .reverse();
    reject(bad);
    let mut bad = good.clone();
    let duplicate = bad["maps"]["cells"][0]["cells"][0].clone();
    bad["maps"]["cells"][0]["cells"]
        .as_array_mut()
        .unwrap()
        .insert(0, duplicate);
    reject(bad);
    let mut bad = good.clone();
    // A region-once bucket cannot smuggle foreign locations in its rows;
    // changing its region must still reject duplicate region ownership.
    bad["maps"]["cells"][0]["region"] = serde_json::json!(2);
    reject(bad);
    let mut bad = good.clone();
    bad["maps"]["instances"][0]["cells"] = serde_json::json!([1, 0]);
    reject(bad);
    let mut bad = good.clone();
    bad["maps"]["instances"][0]["cells"] = serde_json::json!([0, 0]);
    reject(bad);
    let mut bad = good.clone();
    bad["maps"]["instances"][0]["children"] = serde_json::json!([999]);
    reject(bad);
    let mut bad = good.clone();
    bad["maps"]["instances"][0] = serde_json::json!([[0, 1], [0], [0], [0]]);
    reject(bad);
    let mut bad = good.clone();
    bad["maps"]["instances"][0]
        .as_object_mut()
        .unwrap()
        .remove("witnesses");
    reject(bad);
    let mut bad = good.clone();
    bad["maps"].as_object_mut().unwrap().remove("children");
    reject(bad);
    let mut bad = good;
    let unused = bad["maps"]["cells"][0].clone();
    bad["maps"]["cells"].as_array_mut().unwrap().push(unused);
    reject(bad);
}

#[test]
fn explored_map_pool_rejects_invalid_appearance_frames_and_reverse_evidence() {
    let reject = |map: ExploredMap| {
        let bytes = serde_json::to_vec(&Maps {
            maps: vec![Shared::new(map)],
        })
        .unwrap();
        assert!(serde_json::from_slice::<Maps>(&bytes).is_err());
    };
    let mut bad = map();
    bad.cells.get_mut(&at(1, 1)).unwrap().ground_items[0].quantity = 0;
    reject(bad);
    let mut bad = map();
    bad.cells.get_mut(&at(1, 1)).unwrap().material = "x".repeat(4097);
    reject(bad);
    let mut bad = map();
    bad.neighbors.insert(
        (at(1, 1), Direction::East),
        Neighbor::Placed {
            target: at(1, 2),
            rotation: 24,
        },
    );
    reject(bad);
    let mut bad = map();
    bad.neighbors.insert(
        (at(1, 1), Direction::East),
        Neighbor::Placed {
            target: at(9, 1),
            rotation: 0,
        },
    );
    reject(bad);
    let mut bad = map();
    bad.witnesses.insert(
        at(2, 1),
        Witness::Placed {
            anchor: at(1, 1),
            delta: [10, 0, 0],
            rotation: 24,
        },
    );
    bad.children.insert((at(1, 1), [10, 0, 0], at(2, 1)), 24);
    reject(bad);
    let mut bad = map();
    bad.children.remove(&(at(1, 1), [10, 0, 0], at(2, 1)));
    reject(bad);
    let mut bad = map();
    bad.children.insert((at(1, 1), [11, 0, 0], at(2, 1)), 0);
    reject(bad);
    let mut bad = map();
    bad.witnesses.insert(at(2, 1), Witness::Ambiguous);
    reject(bad);
    let mut bad = map();
    bad.children.remove(&(at(1, 1), [10, 0, 0], at(2, 1)));
    bad.witnesses.insert(
        at(2, 1),
        Witness::Placed {
            anchor: at(1, 1),
            delta: [i64::MIN, 0, 0],
            rotation: 0,
        },
    );
    bad.children
        .insert((at(1, 1), [i64::MIN, 0, 0], at(2, 1)), 0);
    reject(bad);
}

#[test]
fn explored_appearance_decoding_requires_named_complete_bounded_values() {
    let good = serde_json::to_value(appearance("remembered")).unwrap();
    assert!(serde_json::from_value::<MapAppearance>(good.clone()).is_ok());
    let reject = |value: serde_json::Value| {
        assert!(serde_json::from_value::<MapAppearance>(value).is_err());
    };
    reject(serde_json::json!([
        true,
        "stone",
        null,
        true,
        true,
        false,
        null,
        null,
        []
    ]));
    for field in ["asset", "door", "trap", "ground_items"] {
        let mut bad = good.clone();
        bad.as_object_mut().unwrap().remove(field);
        reject(bad);
    }
    for path in ["door", "ground_items"] {
        let mut bad = good.clone();
        let nested = if path == "door" {
            &mut bad["door"]
        } else {
            &mut bad["ground_items"][0]
        };
        nested.as_object_mut().unwrap().remove("asset");
        reject(bad);
    }
    let mut bad = good.clone();
    bad["door"] = serde_json::json!(["gate", "closed", false, null]);
    reject(bad);
    let mut bad = good.clone();
    bad["ground_items"][0] =
        serde_json::json!(["potion", "blue", "potion", "blue", false, 3, null]);
    reject(bad);
    let mut bad = good.clone();
    bad["door"]["action_id"] = serde_json::json!(99);
    reject(bad);
    let mut bad = good.clone();
    bad["ground_items"][0]["item_id"] = serde_json::json!(99);
    reject(bad);
    let mut bad = good.clone();
    bad["material"] = serde_json::json!("tab\tcharacter");
    reject(bad);
    let mut bad = good.clone();
    bad["ground_items"] = serde_json::Value::Array(vec![good["ground_items"][0].clone(); 4097]);
    assert!(
        serde_json::from_value::<MapAppearance>(bad).is_ok(),
        "presentation limits cannot impose a new ground-stack gameplay limit"
    );
    let mut maximum = good;
    maximum["material"] = serde_json::json!("x".repeat(4096));
    assert!(serde_json::from_value::<MapAppearance>(maximum).is_ok());
}

#[test]
fn direct_explored_map_decode_checks_named_fields_and_reverse_evidence() {
    let good = serde_json::to_value(map()).unwrap();
    assert_eq!(
        serde_json::from_value::<ExploredMap>(good.clone()).unwrap(),
        map()
    );
    assert!(serde_json::from_value::<ExploredMap>(serde_json::json!([[], [], [], []])).is_err());
    let mut missing = good.clone();
    missing.as_object_mut().unwrap().remove("children");
    assert!(serde_json::from_value::<ExploredMap>(missing).is_err());
    let mut mismatch = good;
    mismatch["children"] = serde_json::json!([]);
    assert!(serde_json::from_value::<ExploredMap>(mismatch).is_err());
}

#[test]
fn explored_map_restore_validation_is_scoped_to_world_and_actor_membership() {
    let world = |width| {
        let mut world = World::new(vec![], vec![]).unwrap();
        world
            .add_region(Region {
                id: RegionId(1),
                name: "memory".into(),
                bounds: Extent::new(width, 3, 1).unwrap(),
            })
            .unwrap();
        world
    };
    let mut game = Game::new(world(12), 42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let mut map = ExploredMap::default();
    map.cells.insert(at(1, 9), appearance("stale wall"));
    game.explored_maps.insert(actor, Shared::new(map));
    let mut shared = SharedState::default();
    let valid = game.checkpoint(&mut shared);
    let mut wrong_membership = game.checkpoint(&mut shared);
    shared.actors.push(BTreeMap::new());
    wrong_membership.actors = shared.actors.len() - 1;
    assert!(!shared.explored_map_references_valid([&wrong_membership]));
    let mut zero_actor = game.checkpoint(&mut shared);
    // Keep the actor directory valid so this independently tests the
    // explored-map owner's zero identity, not the earlier record-pool gate.
    zero_actor.explored_maps = shared.explored_map_owners.len();
    shared
        .explored_map_owners
        .push(BTreeMap::from([(ActorId(0), 0)]));
    assert!(!shared.explored_map_references_valid([&zero_actor]));
    let mut wrong_world = game.checkpoint(&mut shared);
    shared.worlds.push(world(3));
    wrong_world.world = shared.worlds.len() - 1;
    let mut wrong_actor = game.checkpoint(&mut shared);
    let index = shared.explored_map_owners[wrong_actor.explored_maps][&actor];
    wrong_actor.explored_maps = shared.explored_map_owners.len();
    shared
        .explored_map_owners
        .push(BTreeMap::from([(ActorId(999), index)]));
    assert!(!shared.explored_map_references_valid([&valid, &wrong_actor]));
    let mut wrong_index = game.checkpoint(&mut shared);
    wrong_index.explored_maps = shared.explored_map_owners.len();
    shared
        .explored_map_owners
        .push(BTreeMap::from([(actor, usize::MAX)]));
    let mut missing_directory = game.checkpoint(&mut shared);
    missing_directory.explored_maps = usize::MAX;
    assert!(!shared.explored_map_references_valid([&wrong_actor]));
    assert!(!shared.explored_map_references_valid([&wrong_index]));
    assert!(!shared.explored_map_references_valid([&missing_directory]));
    let mut context = RestoreContext::new(&shared, game.content().clone());
    context
        .bind_spatial_roots(&[
            &valid,
            &zero_actor,
            &wrong_membership,
            &wrong_actor,
            &wrong_index,
            &missing_directory,
        ])
        .unwrap();
    super::explored_map_regions::reset_work_counts();
    assert_eq!(context.restore(zero_actor), Err(PersistError::ExploredMap));
    assert_eq!(
        super::explored_map_regions::work_counts().appearance_cells,
        0
    );
    let restored = context.restore(valid).unwrap();
    assert_eq!(restored, game);
    assert!(restored.explored_maps[&actor].shares_storage(&game.explored_maps[&actor]));
    assert_eq!(
        context.restore(wrong_membership),
        Err(PersistError::ExploredMap)
    );
    assert_eq!(context.restore(wrong_world), Err(PersistError::ExploredMap));
    assert_eq!(context.restore(wrong_actor), Err(PersistError::ExploredMap));
    assert_eq!(context.restore(wrong_index), Err(PersistError::ExploredMap));
    assert_eq!(
        context.restore(missing_directory),
        Err(PersistError::ExploredMap)
    );
}

#[test]
fn checkpoints_require_explored_maps_and_preserve_more_than_one_viewport() {
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_region(Region {
            id: RegionId(1),
            name: "large history".into(),
            bounds: Extent::new(5002, 3, 1).unwrap(),
        })
        .unwrap();
    let mut game = Game::new(world, 42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let mut map = ExploredMap::default();
    for x in 0..5000 {
        map.cells.insert(at(1, x), appearance("last seen"));
    }
    game.explored_maps.insert(actor, Shared::new(map));
    let original = game.clone();
    assert!(game.explored_maps[&actor].shares_storage(&original.explored_maps[&actor]));
    let mut shared = SharedState::default();
    let before = game.checkpoint(&mut shared);
    let unchanged = game.checkpoint(&mut shared);
    assert_eq!(shared.explored_maps.len(), 1);
    assert_eq!(shared.explored_map_owners.len(), 1);
    assert_eq!(before.explored_maps, unchanged.explored_maps);
    let bytes = serde_json::to_vec(&(vec![before, unchanged], &shared)).unwrap();
    let (snapshots, decoded): (Vec<Snapshot>, SharedState) =
        serde_json::from_slice(&bytes).unwrap();
    let mut context = RestoreContext::new(&decoded, game.content().clone());
    context
        .bind_spatial_roots(&snapshots.iter().collect::<Vec<_>>())
        .unwrap();
    let restored: Vec<_> = snapshots
        .into_iter()
        .map(|s| context.restore(s).unwrap())
        .collect();
    assert_eq!(restored[0], original);
    assert_eq!(restored[0].explored_maps[&actor].cells.iter().count(), 5000);
    assert!(restored[0].explored_maps[&actor].shares_storage(&restored[1].explored_maps[&actor]));
    let snapshot = game.checkpoint(&mut shared);
    let mut value = serde_json::to_value(snapshot).unwrap();
    assert!(value["explored_maps"].is_u64());
    for invalid in [
        serde_json::json!([]),
        serde_json::json!([[actor, 0]]),
        serde_json::Value::Null,
    ] {
        let mut legacy = value.clone();
        legacy["explored_maps"] = invalid;
        assert!(serde_json::from_value::<Snapshot>(legacy).is_err());
    }
    value.as_object_mut().unwrap().remove("explored_maps");
    assert!(serde_json::from_value::<Snapshot>(value).is_err());
    let value = serde_json::to_value(shared).unwrap();
    for field in ["explored_maps", "explored_map_owners"] {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<SharedState>(missing).is_err());
    }
    for invalid in [
        serde_json::Value::Null,
        serde_json::json!([[[0, 0]]]),
        serde_json::json!([[[1, 0], [1, 0]]]),
        serde_json::json!([[[1, 0], [1, 1]]]),
        serde_json::json!([[[2, 0], [1, 0]]]),
        serde_json::json!([[[1, 0]], [[1, 0]]]),
        serde_json::json!([[], []]),
    ] {
        let mut malformed = value.clone();
        malformed["explored_map_owners"] = invalid;
        assert!(serde_json::from_value::<SharedState>(malformed).is_err());
    }
}

#[test]
fn checkpoints_pool_complete_owner_directories_without_merging_actor_knowledge() {
    let mut game = Game::new(
        validation_world(64, 0),
        42,
        crate::test_creatures::content(),
    );
    let first = game
        .spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let second = game
        .spawn_actor(at(1, 2), NonZeroU64::new(100).unwrap())
        .unwrap();
    let roots = roots_with_unrelated_changes(64, 2);
    game.explored_maps.insert(first, roots[0].clone());
    game.explored_maps.insert(second, roots[1].clone());
    let original = game.clone();
    let mut shared = SharedState::default();
    let before = game.checkpoint(&mut shared);
    let unchanged = game.checkpoint(&mut shared);
    assert_eq!(before.explored_maps, unchanged.explored_maps);
    assert_eq!(shared.explored_map_owners.len(), 1);
    assert_eq!(shared.explored_map_owners[0].len(), 2);
    game.explored_maps.insert(second, roots[0].clone());
    let changed = game.checkpoint(&mut shared);
    assert_ne!(before.explored_maps, changed.explored_maps);
    assert_eq!(shared.explored_map_owners.len(), 2);
    assert_eq!(shared.explored_maps.len(), 2);
    assert!(shared.explored_map_references_valid([&before, &unchanged, &changed]));
    // The changed state still uses root zero, but cannot hide either the
    // unreferenced earlier directory or its distinct second actor's root.
    assert!(!shared.explored_map_references_valid([&changed]));
    let bytes = serde_json::to_vec(&(vec![before, unchanged, changed], &shared)).unwrap();
    let (snapshots, decoded): (Vec<Snapshot>, SharedState) =
        serde_json::from_slice(&bytes).unwrap();
    let mut context = RestoreContext::new(&decoded, game.content().clone());
    context
        .bind_spatial_roots(&snapshots.iter().collect::<Vec<_>>())
        .unwrap();
    let restored: Vec<_> = snapshots
        .into_iter()
        .map(|snapshot| context.restore(snapshot).unwrap())
        .collect();
    assert_eq!(restored[0], original);
    assert_eq!(restored[1], original);
    assert_eq!(restored[2], game);
    assert!(restored[0].explored_maps[&first].shares_storage(&restored[1].explored_maps[&first]));
    assert!(restored[0].explored_maps[&first].shares_storage(&restored[2].explored_maps[&second]));
    assert_ne!(
        restored[0].explored_maps[&first],
        restored[0].explored_maps[&second]
    );
}

#[test]
fn checkpoints_require_unique_referenced_owner_directories_even_when_empty() {
    let mut game = Game::new(
        validation_world(64, 0),
        42,
        crate::test_creatures::content(),
    );
    game.spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    assert_eq!(snapshot.explored_maps, 0);
    assert_eq!(shared.explored_map_owners, vec![BTreeMap::new()]);
    assert!(shared.explored_maps.is_empty());
    assert!(shared.explored_map_references_valid([&snapshot]));
    let bytes = serde_json::to_vec(&(&snapshot, &shared)).unwrap();
    let (decoded_snapshot, decoded_shared): (Snapshot, SharedState) =
        serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        Game::restore_checkpoint(decoded_snapshot, &decoded_shared, game.content().clone())
            .unwrap(),
        game
    );
    shared.explored_map_owners.push(BTreeMap::new());
    assert!(!shared.explored_map_references_valid([&snapshot]));
    assert_eq!(
        Game::restore_checkpoint(snapshot, &shared, game.content().clone()),
        Err(PersistError::ExploredMap)
    );
}
