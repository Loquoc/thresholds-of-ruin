use super::*;
use tor_world::{Position, RegionId, SightCell};

fn at(region: u64, x: i32, y: i32, z: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z },
    }
}

#[test]
fn local_retreat_frame_uses_physical_occurrences_not_occupancy() {
    let origin = at(1, 10, 20, 0);
    let ordinary = SightCell {
        physical: true,
        location: at(1, 11, 20, 0),
        offset: Position { x: 1, y: 0, z: 0 },
        rotation: 0,
        wall: true,
    };
    let folded = SightCell {
        location: at(1, 12, 20, 0),
        ..ordinary
    };
    for ignored in [
        SightCell {
            physical: false,
            ..folded
        },
        SightCell {
            location: at(2, 12, 20, 0),
            ..folded
        },
    ] {
        let scene = [ordinary, ignored];
        let frame = LocalRetreatFrame {
            origin,
            orientation: 0,
            scene: &scene,
        };
        assert!(
            frame.is_canonical(),
            "canonical coordinates do not imply clear terrain"
        );
        assert!(frame.ordinary_step(origin, ordinary.location, 0, Direction::East));
        assert!(!frame.ordinary_step(origin, at(2, 11, 20, 0), 0, Direction::East));
        assert!(!frame.ordinary_step(origin, ordinary.location, 1, Direction::East));
        assert!(!frame.ordinary_step(origin, ordinary.location, 0, Direction::West));
    }
    for bad in [
        folded,
        SightCell {
            rotation: 1,
            ..ordinary
        },
    ] {
        let scene = [ordinary, bad];
        let frame = LocalRetreatFrame {
            origin,
            orientation: 0,
            scene: &scene,
        };
        assert!(
            !frame.is_canonical(),
            "same-region physical aliases cannot certify local distance"
        );
    }
}

#[test]
fn retreat_distance_retains_wide_signed_coordinate_arithmetic_and_units() {
    let low = at(1, i32::MIN, i32::MAX, i32::MIN);
    let high = at(1, i32::MAX, i32::MIN, i32::MAX);
    assert_eq!(
        displacement(low, high),
        [4_294_967_295, -4_294_967_295, 4_294_967_295]
    );
    assert_eq!(coordinate_distance(low, high), 12_884_901_885);
    let route = RetreatMetric::RememberedRoute { current: 7 };
    let local = RetreatMetric::LocalChart { current: 7 };
    assert_eq!(route.current(), local.current());
    assert!(route.uses_remembered_routes());
    assert!(
        !local.uses_remembered_routes(),
        "equal numbers do not make metrics interchangeable"
    );
}
