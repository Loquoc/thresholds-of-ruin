//! Independent witnesses for the recording memo: a moving observer's map is
//! exactly what full recording produces, and per-step work follows what
//! changed in view rather than the size of the view.
use super::*;
use std::num::NonZeroU64;

const SIZE: i32 = 40;

fn at(x: i32, y: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y, z: 0 },
    }
}

fn appearance(x: i32, y: i32) -> MapAppearance {
    MapAppearance {
        object_presentation: None,
        wall: (x + y) % 5 == 0,
        material: format!("stone {}", (x * 7 + y) % 3),
        asset: None,
        place_hint: false,
        stairs_up: false,
        stairs_down: false,
        trap: None,
        door: None,
        ground_items: Vec::new(),
    }
}

fn game() -> (Game, ActorId) {
    let mut world = tor_world::World::new(vec![], vec![]).unwrap();
    world
        .add_region(tor_world::Region {
            id: RegionId(1),
            name: "memo witness".into(),
            bounds: tor_world::Extent::new(SIZE, SIZE, 1).unwrap(),
        })
        .unwrap();
    let mut game = Game::new(world, 7, crate::test_creatures::content());
    let actor = game
        .spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    (game, actor)
}

/// A square view of `radius` around `eye`, as an unrotated observer frame.
fn view(eye: (i32, i32), radius: i32) -> Vec<SightCell> {
    let mut cells = Vec::new();
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let (x, y) = (eye.0 + dx, eye.1 + dy);
            if (0..SIZE).contains(&x) && (0..SIZE).contains(&y) {
                cells.push(SightCell {
                    physical: true,
                    location: at(x, y),
                    offset: Position { x: dx, y: dy, z: 0 },
                    rotation: 0,
                    wall: (x + y) % 5 == 0,
                });
            }
        }
    }
    cells
}

fn inputs(scene: &[SightCell]) -> Vec<MapCellInput> {
    let mut seen = BTreeSet::new();
    scene
        .iter()
        .filter(|cell| seen.insert(cell.location))
        .map(|cell| MapCellInput {
            location: cell.location,
            appearance: appearance(cell.location.position.x, cell.location.position.y),
        })
        .collect()
}

/// Record `scene` in both games: `memo` keeps its recording memo, `cold`
/// forgets it first, so it always runs full recording. Returns the work of
/// each.
fn record_both(
    memo: &mut Game,
    cold: &mut Game,
    actor: ActorId,
    scene: &[SightCell],
) -> (MapWorkCounts, MapWorkCounts) {
    let cells = inputs(scene);
    reset_map_work_counts();
    memo.record_explored_observation(actor, &cells, scene)
        .unwrap();
    let warm = map_work_counts();
    cold.map_scenes = MapSceneMemo::default();
    reset_map_work_counts();
    cold.record_explored_observation(actor, &cells, scene)
        .unwrap();
    let full = map_work_counts();
    assert_eq!(
        memo.explored_maps.get(&actor),
        cold.explored_maps.get(&actor),
        "the memo must store exactly what full recording stores"
    );
    (warm, full)
}

#[test]
fn a_moving_view_records_exactly_what_full_recording_records() {
    let (mut memo, actor) = game();
    let (mut cold, _) = game();
    let mut eye = (8, 8);
    let mut reused = 0;
    for step in 0..24 {
        // Walk a loop with diagonal and reversing steps.
        eye = match step % 8 {
            0..=2 => (eye.0 + 1, eye.1),
            3 | 4 => (eye.0 + 1, eye.1 + 1),
            5 => (eye.0, eye.1 - 1),
            _ => (eye.0 - 1, eye.1),
        };
        let (warm, full) = record_both(&mut memo, &mut cold, actor, &view(eye, 5));
        reused += warm.placement_proposals_reused;
        assert_eq!(
            full.placement_proposals_reused, 0,
            "the oracle never reuses"
        );
        assert_eq!(
            (
                warm.appearance_changes,
                warm.neighbor_changes,
                warm.witness_changes
            ),
            (
                full.appearance_changes,
                full.neighbor_changes,
                full.witness_changes
            ),
            "the same facts change at step {step}"
        );
    }
    assert!(
        reused > 0,
        "positive fixture: unchanged neighbor facts are reused"
    );
}

#[test]
fn twice_disclosed_and_turned_cells_are_proposed_again() {
    // A location seen through two offsets (a portal-like repeat) can propose
    // conflicting facts, and a turned cell changes its facts: neither may be
    // assumed unchanged, so both games keep identical ambiguity.
    let (mut memo, actor) = game();
    let (mut cold, _) = game();
    let mut scene = view((10, 10), 3);
    record_both(&mut memo, &mut cold, actor, &scene);
    let repeat = SightCell {
        offset: Position { x: 4, y: 0, z: 0 },
        ..scene[0]
    };
    scene.push(repeat);
    let (warm, _) = record_both(&mut memo, &mut cold, actor, &scene);
    assert!(warm.placement_proposals > 0);
    let turned = scene
        .iter_mut()
        .find(|cell| cell.offset == Position { x: 1, y: 1, z: 0 })
        .unwrap();
    turned.rotation = 6;
    record_both(&mut memo, &mut cold, actor, &scene);
    scene.pop();
    record_both(&mut memo, &mut cold, actor, &scene);
}

#[test]
fn a_map_replaced_by_another_path_is_not_trusted() {
    let (mut memo, actor) = game();
    let (mut cold, _) = game();
    record_both(&mut memo, &mut cold, actor, &view((10, 10), 4));
    // Checkpoint restore and region streaming replace the shared map; a fresh
    // value, even with equal content, carries no recording evidence.
    let replaced = Shared::new(ExploredMap::default());
    memo.explored_maps.insert(actor, replaced.clone());
    cold.explored_maps.insert(actor, replaced);
    let (warm, _) = record_both(&mut memo, &mut cold, actor, &view((11, 10), 4));
    assert_eq!(
        warm.placement_proposals_reused, 0,
        "a replaced map is recorded in full"
    );
}

#[test]
fn per_step_proposals_follow_the_changed_edge_not_the_view_area() {
    // One step east re-sees all but one column. Work after the first step
    // must grow with the view's edge, while full recording grows with its area.
    let step_work = |radius: i32| {
        let (mut memo, actor) = game();
        let (mut cold, _) = game();
        record_both(&mut memo, &mut cold, actor, &view((15, 15), radius));
        record_both(&mut memo, &mut cold, actor, &view((16, 15), radius))
    };
    let (small_warm, small_full) = step_work(1);
    let (large_warm, large_full) = step_work(7);
    let (small_edge, large_edge) = (3, 15);
    assert!(
        large_warm.placement_proposals <= 16 * large_edge,
        "memo work follows the edge: {} proposals for a 15-cell edge",
        large_warm.placement_proposals
    );
    // Facts are proposed along the four level directions: every ordered pair
    // of side-adjacent cells in a 15 by 15 single-level view.
    assert_eq!(
        large_full.placement_proposals,
        2 * (2 * 15 * 14),
        "the oracle re-proposes the whole area"
    );
    let warm_growth = large_warm.placement_proposals as f64 / small_warm.placement_proposals as f64;
    let full_growth = large_full.placement_proposals as f64 / small_full.placement_proposals as f64;
    assert!(
        warm_growth <= 2.0 * (large_edge as f64 / small_edge as f64),
        "memo growth {warm_growth:.1} must be about edge growth"
    );
    assert!(
        full_growth > 2.0 * warm_growth,
        "full {full_growth:.1} vs memo {warm_growth:.1}"
    );
}
