//! Bounded physical ray steps, independent of actors and effect resolution.

use crate::{compose_rotation, Direction, Location, RegionId, World};
use std::collections::BTreeSet;

/// The physical boundary encountered by one ray step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RayStep {
    /// Loaded, empty terrain or an open door, with the crossing frame change.
    Clear { to: Location, rotation: u8 },
    /// A wall, closed door, or diagonal with both component sides blocked.
    Solid,
    /// The source or required geometry is absent, detached, or out of bounds.
    Unavailable,
    /// Two clear-side routes disagree on the destination or crossing frame.
    Ambiguous,
}

/// A vector probe retains the source-frame face axes needed for reflection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RayVectorStep {
    Clear { to: Location, rotation: u8 },
    Solid { normal_axes: [bool; 3] },
    Unavailable,
    Ambiguous,
}

impl World {
    /// Probe a single raster step with one, two or three active axes.
    /// All cardinal route permutations are examined, so a clear route cannot
    /// silently choose between contradictory portal destinations. Unknown
    /// geometry supplies dependencies, never an invented reflecting surface.
    pub fn ray_vector_step(&self, from: Location, delta: [i32; 3]) -> RayVectorStep {
        self.ray_vector_step_dependencies(from, delta).0
    }

    pub fn ray_vector_step_dependencies(
        &self,
        from: Location,
        delta: [i32; 3],
    ) -> (RayVectorStep, BTreeSet<RegionId>) {
        let mut dependencies = BTreeSet::new();
        if self.known_region(from.region).is_some() {
            dependencies.insert(from.region);
        }
        if !self.contains(from)
            || self.opaque(from)
            || delta == [0; 3]
            || delta.iter().any(|n| !(-1..=1).contains(n))
        {
            return (RayVectorStep::Unavailable, dependencies);
        }
        let axes: Vec<usize> = (0..3).filter(|&axis| delta[axis] != 0).collect();
        let orders: &[&[usize]] = match axes.len() {
            1 => &[&[0]],
            2 => &[&[0, 1], &[1, 0]],
            _ => &[
                &[0, 1, 2],
                &[0, 2, 1],
                &[1, 0, 2],
                &[1, 2, 0],
                &[2, 0, 1],
                &[2, 1, 0],
            ],
        };
        let mut endpoint = None;
        let mut normals = [false; 3];
        let mut unavailable = false;
        let mut clear = false;
        for order in orders {
            let mut at = from;
            let mut rotation = 0;
            for (index, &slot) in order.iter().enumerate() {
                let axis = axes[slot];
                let mut unit = [0i64; 3];
                unit[axis] = i64::from(delta[axis]);
                let direction = Direction::from_delta(crate::rotate_vector(rotation, unit))
                    .expect("rotated cardinal remains cardinal");
                let Some((to, turn)) =
                    self.ray_neighbor(at, direction, &mut Some(&mut dependencies))
                else {
                    unavailable = true;
                    break;
                };
                let next_rotation = compose_rotation(rotation, turn);
                let final_cell = index + 1 == order.len();
                if final_cell {
                    let candidate = (to, next_rotation);
                    if endpoint.is_some_and(|previous| previous != candidate) {
                        return (RayVectorStep::Ambiguous, dependencies);
                    }
                    endpoint = Some(candidate);
                }
                if self.opaque(to) {
                    normals[axis] = true;
                    break;
                }
                if final_cell {
                    clear = true;
                }
                at = to;
                rotation = next_rotation;
            }
        }
        let result = if clear {
            let (to, rotation) = endpoint.expect("clear route has endpoint");
            RayVectorStep::Clear { to, rotation }
        } else if unavailable {
            RayVectorStep::Unavailable
        } else {
            RayVectorStep::Solid {
                normal_axes: normals,
            }
        };
        (result, dependencies)
    }

    /// Resolve one physical step in any supported direction without loading or
    /// generating regions. Abstract stairs and sight-only rim projection never
    /// redirect a ray; ordinary geometry at those locations still applies.
    ///
    /// A diagonal needs at least one clear component side. If both routes reach
    /// loaded destinations, their locations and rotations must agree, including
    /// when a destination is solid. With no resolved route, dependencies geometry
    /// takes precedence over an obstructed side. At most four cardinal edges
    /// are examined. Actors and the response to each boundary belong to callers.
    pub fn ray_step(&self, from: Location, direction: Direction) -> RayStep {
        self.ray_step_recording_dependencies(from, direction, &mut None)
    }

    /// The same physical probe, plus exact known region dependencies examined
    /// by its cardinal routes. A caller requiring complete geometry must load
    /// any absent dependencies before accepting even a Clear result: the other diagonal route may
    /// otherwise become contradictory. Retain loaded side regions too, because they
    /// are real occlusion dependencies. This neither loads nor grants adjacency.
    pub fn ray_step_dependencies(
        &self,
        from: Location,
        direction: Direction,
    ) -> (RayStep, BTreeSet<RegionId>) {
        let mut dependencies = BTreeSet::new();
        let step =
            self.ray_step_recording_dependencies(from, direction, &mut Some(&mut dependencies));
        (step, dependencies)
    }

    fn ray_step_recording_dependencies(
        &self,
        from: Location,
        direction: Direction,
        dependencies: &mut Option<&mut BTreeSet<RegionId>>,
    ) -> RayStep {
        if self.known_region(from.region).is_some() {
            if let Some(dependencies) = dependencies.as_deref_mut() {
                dependencies.insert(from.region);
            }
        }
        if !self.contains(from) {
            return RayStep::Unavailable;
        }
        if self.opaque(from) {
            return RayStep::Solid;
        }
        let Some((a, b)) = direction.components() else {
            return self
                .ray_neighbor(from, direction, dependencies)
                .map_or(RayStep::Unavailable, |(to, rotation)| {
                    self.ray_destination(to, rotation)
                });
        };
        let mut route = |first, second: Direction| {
            let (side, first_rotation) = self
                .ray_neighbor(from, first, dependencies)
                .ok_or(RayStep::Unavailable)?;
            if self.opaque(side) {
                return Err(RayStep::Solid);
            }
            let (to, second_rotation) = self
                .ray_neighbor(side, second.rotated(first_rotation), dependencies)
                .ok_or(RayStep::Unavailable)?;
            Ok((to, compose_rotation(first_rotation, second_rotation)))
        };
        match (route(a, b), route(b, a)) {
            (Ok(a), Ok(b)) if a != b => RayStep::Ambiguous,
            (Ok((to, rotation)), _) | (_, Ok((to, rotation))) => self.ray_destination(to, rotation),
            (Err(RayStep::Solid), Err(RayStep::Solid)) => RayStep::Solid,
            _ => RayStep::Unavailable,
        }
    }

    fn ray_neighbor(
        &self,
        from: Location,
        direction: Direction,
        dependencies: &mut Option<&mut BTreeSet<RegionId>>,
    ) -> Option<(Location, u8)> {
        let (to, rotation) = self.physics_neighbor(from, direction)?;
        if self.known_region(to.region).is_some() {
            if let Some(dependencies) = dependencies.as_deref_mut() {
                dependencies.insert(to.region);
            }
        }
        self.contains(to).then_some((to, rotation))
    }

    fn ray_destination(&self, to: Location, rotation: u8) -> RayStep {
        if self.opaque(to) {
            RayStep::Solid
        } else {
            RayStep::Clear { to, rotation }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Extent, NamedAnchor, Passage, Position, Region, RegionId};

    fn cell(region: u64, x: i32, y: i32, z: i32) -> Location {
        Location {
            region: RegionId(region),
            position: Position { x, y, z },
        }
    }

    fn region(id: u64) -> Region {
        Region {
            id: RegionId(id),
            name: format!("ray-{id}"),
            bounds: Extent::new(3, 3, 3).unwrap(),
        }
    }

    fn world() -> World {
        World::new((1..=3).map(region).collect(), vec![]).unwrap()
    }

    fn directions() -> impl Iterator<Item = Direction> {
        Direction::HORIZONTAL.into_iter().chain([
            Direction::Up,
            Direction::Down,
            Direction::EastUp,
            Direction::WestUp,
            Direction::NorthUp,
            Direction::SouthUp,
            Direction::EastDown,
            Direction::WestDown,
            Direction::NorthDown,
            Direction::SouthDown,
        ])
    }

    #[test]
    fn arbitrary_vector_steps_reach_all_twenty_six_neighbors() {
        let world = world();
        let from = cell(1, 1, 1, 1);
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    if [x, y, z] == [0; 3] {
                        continue;
                    }
                    assert_eq!(
                        world.ray_vector_step(from, [x, y, z]),
                        RayVectorStep::Clear {
                            to: cell(1, 1 + x, 1 + y, 1 + z),
                            rotation: 0
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn reflecting_faces_distinguish_wall_ceiling_and_corner() {
        let from = cell(1, 1, 1, 1);
        for axis in 0..3 {
            let mut world = world();
            for a in 0..3 {
                for b in 0..3 {
                    let mut p = [0; 3];
                    p[axis] = 2;
                    p[(axis + 1) % 3] = a;
                    p[(axis + 2) % 3] = b;
                    world.set_wall(cell(1, p[0], p[1], p[2]), true).unwrap();
                }
            }
            let mut normal_axes = [false; 3];
            normal_axes[axis] = true;
            assert_eq!(
                world.ray_vector_step(from, [1, 1, 1]),
                RayVectorStep::Solid { normal_axes }
            );
        }
        let mut world = world();
        world.set_wall(cell(1, 2, 2, 2), true).unwrap();
        assert_eq!(
            world.ray_vector_step(from, [1, 1, 1]),
            RayVectorStep::Solid {
                normal_axes: [true; 3]
            }
        );
    }

    #[test]
    fn missing_geometry_and_invalid_steps_do_not_create_bounce_faces() {
        let world = world();
        for delta in [[0, 0, 0], [2, 0, 0], [i32::MIN, 0, 0]] {
            assert_eq!(
                world.ray_vector_step(cell(1, 1, 1, 1), delta),
                RayVectorStep::Unavailable
            );
        }
        assert_eq!(
            world.ray_vector_step(cell(1, 2, 2, 2), [1, 1, 1]),
            RayVectorStep::Unavailable
        );
    }

    #[test]
    fn every_supported_direction_reaches_clear_terrain_and_stops_at_a_wall() {
        let from = cell(1, 1, 1, 1);
        for direction in directions() {
            let mut world = world();
            let (dx, dy, dz) = direction.delta();
            let to = cell(1, 1 + dx, 1 + dy, 1 + dz);
            assert_eq!(
                world.ray_step(from, direction),
                RayStep::Clear { to, rotation: 0 },
                "{direction:?}"
            );
            world.set_wall(to, true).unwrap();
            assert_eq!(world.ray_step(from, direction), RayStep::Solid);
        }
    }

    #[test]
    fn closed_doors_block_their_whole_height_and_open_doors_pass() {
        let mut world = world();
        world.place_door(cell(1, 2, 1, 0), 1, false, 2).unwrap();
        for open in [false, true] {
            world.set_door(cell(1, 2, 1, 0), open);
            for z in 0..3 {
                let to = cell(1, 2, 1, z);
                let expected = if open || z == 2 {
                    RayStep::Clear { to, rotation: 0 }
                } else {
                    RayStep::Solid
                };
                assert_eq!(world.ray_step(cell(1, 1, 1, z), Direction::East), expected);
            }
        }
    }

    #[test]
    fn every_diagonal_requires_one_clear_side_and_a_clear_destination() {
        let from = cell(1, 1, 1, 1);
        for direction in directions().filter(|direction| direction.components().is_some()) {
            let (a, b) = direction.components().unwrap();
            for doors in [false, true] {
                for mask in 0..8 {
                    let mut world = world();
                    let side_a = world.physics_neighbor(from, a).unwrap().0;
                    let side_b = world.physics_neighbor(from, b).unwrap().0;
                    let to = world.physics_neighbor(side_a, b).unwrap().0;
                    for (bit, at) in [(1, side_a), (2, side_b), (4, to)] {
                        if mask & bit != 0 {
                            if doors {
                                world.place_door(at, bit, false, 1).unwrap();
                            } else {
                                world.set_wall(at, true).unwrap();
                            }
                        }
                    }
                    let expected = if mask & 4 == 0 && mask & 3 != 3 {
                        RayStep::Clear { to, rotation: 0 }
                    } else {
                        RayStep::Solid
                    };
                    assert_eq!(
                        world.ray_step(from, direction),
                        expected,
                        "{direction:?}, doors={doors}, mask={mask}"
                    );
                }
            }
        }
    }

    #[test]
    fn physical_apertures_preserve_every_supported_cube_rotation() {
        let center = cell(1, 1, 1, 1);
        let to = cell(2, 1, 1, 1);
        for direction in directions().filter(|direction| direction.components().is_none()) {
            let from = Location {
                position: direction.offset(center.position).unwrap(),
                ..center
            };
            for rotation in 0..24 {
                let mut world = world();
                world
                    .connect_portal_area(
                        Passage {
                            from,
                            direction,
                            to,
                        },
                        rotation,
                        1,
                        1,
                    )
                    .unwrap();
                assert_eq!(
                    world.ray_step(from, direction),
                    RayStep::Clear { to, rotation },
                    "{direction:?}, rotation={rotation}"
                );
                let (x, y, z) = direction.delta();
                assert_eq!(
                    world.ray_vector_step(from, [x, y, z]),
                    RayVectorStep::Clear { to, rotation }
                );
                world.place_door(to, 1, false, 1).unwrap();
                assert_eq!(world.ray_step(from, direction), RayStep::Solid);
                world.set_door(to, true);
                assert_eq!(
                    world.ray_step(from, direction),
                    RayStep::Clear { to, rotation }
                );
            }
        }
    }

    #[test]
    fn diagonals_follow_a_physical_aperture_under_every_cube_rotation() {
        let from = cell(1, 2, 1, 1);
        let to = cell(2, 1, 1, 1);
        for rotation in 0..24 {
            let mut world = world();
            world
                .connect_area(
                    Passage {
                        from: cell(1, 2, 0, 1),
                        direction: Direction::East,
                        to,
                    },
                    rotation,
                    2,
                    1,
                )
                .unwrap();
            assert_eq!(
                world.ray_step(from, Direction::NorthEast),
                RayStep::Clear { to, rotation },
                "rotation={rotation}"
            );
            assert_eq!(
                world.ray_vector_step(from, [1, -1, 0]),
                RayVectorStep::Clear { to, rotation }
            );
            world.set_wall(cell(1, 2, 0, 1), true).unwrap();
            assert_eq!(
                world.ray_step(from, Direction::NorthEast),
                RayStep::Clear { to, rotation }
            );
        }
    }

    #[test]
    fn a_vertical_diagonal_crosses_physical_portals_without_using_stairs() {
        let mut world = world();
        let from = cell(1, 1, 1, 2);
        world
            .connect_portal_area(
                Passage {
                    from,
                    direction: Direction::Up,
                    to: cell(2, 1, 1, 1),
                },
                23,
                2,
                1,
            )
            .unwrap();
        assert!(!world.is_stair(from, Direction::Up));
        assert_eq!(
            world.ray_step(from, Direction::EastUp),
            RayStep::Clear {
                to: cell(2, 1, 1, 0),
                rotation: 23,
            }
        );
    }

    #[test]
    fn a_diagonal_composes_two_physical_crossing_frames() {
        let mut world = world();
        let from = cell(1, 2, 1, 1);
        let side = cell(2, 2, 1, 1);
        let to = cell(3, 1, 1, 1);
        world.set_wall(cell(1, 2, 0, 1), true).unwrap();
        for (from, to, rotation) in [(from, side, 1), (side, to, 23)] {
            world
                .connect(
                    Passage {
                        from,
                        direction: Direction::East,
                        to,
                    },
                    rotation,
                )
                .unwrap();
        }
        assert_eq!(
            world.ray_step(from, Direction::NorthEast),
            RayStep::Clear { to, rotation: 19 }
        );
    }

    #[test]
    fn conflicting_diagonal_destinations_are_ambiguous_even_if_one_is_solid() {
        let mut world = world();
        let from = cell(1, 2, 0, 1);
        for (direction, to) in [
            (Direction::North, cell(2, 1, 1, 1)),
            (Direction::East, cell(3, 1, 1, 1)),
        ] {
            world
                .connect(
                    Passage {
                        from,
                        direction,
                        to,
                    },
                    0,
                )
                .unwrap();
        }
        assert_eq!(
            world.ray_step(from, Direction::NorthEast),
            RayStep::Ambiguous
        );
        world.set_wall(cell(2, 2, 1, 1), true).unwrap();
        assert_eq!(
            world.ray_step(from, Direction::NorthEast),
            RayStep::Ambiguous
        );
        world.set_wall(cell(2, 1, 1, 1), true).unwrap();
        assert_eq!(
            world.ray_step(from, Direction::NorthEast),
            RayStep::Clear {
                to: cell(3, 1, 0, 1),
                rotation: 0,
            }
        );
    }

    #[test]
    fn equal_diagonal_destinations_with_conflicting_frames_are_ambiguous() {
        let mut world = world();
        let from = cell(1, 2, 0, 1);
        let to = cell(2, 0, 1, 1);
        for (direction, rotation) in [(Direction::North, 0), (Direction::East, 1)] {
            world
                .connect(
                    Passage {
                        from,
                        direction,
                        to,
                    },
                    rotation,
                )
                .unwrap();
        }
        assert_eq!(
            world.ray_step(from, Direction::NorthEast),
            RayStep::Ambiguous
        );
    }

    #[test]
    fn abstract_stairs_do_not_redirect_cardinal_or_diagonal_rays() {
        let from = cell(1, 1, 1, 1);
        for (vertical, diagonal, z) in [
            (Direction::Up, Direction::EastUp, 2),
            (Direction::Down, Direction::EastDown, 0),
        ] {
            let mut world = world();
            for from in [from, cell(1, 2, 1, 1)] {
                world
                    .connect(
                        Passage {
                            from,
                            direction: vertical,
                            to: cell(2, 1, 1, 1),
                        },
                        23,
                    )
                    .unwrap();
            }
            assert!(world.is_stair(from, vertical));
            assert_eq!(
                world.movement_neighbor(from, vertical),
                Some((cell(2, 1, 1, 1), 23))
            );
            assert_eq!(
                world.ray_step(from, vertical),
                RayStep::Clear {
                    to: cell(1, 1, 1, z),
                    rotation: 0,
                }
            );
            assert_eq!(
                world.ray_step(from, diagonal),
                RayStep::Clear {
                    to: cell(1, 2, 1, z),
                    rotation: 0,
                }
            );
        }
    }

    #[test]
    fn an_unbuilt_named_stair_never_generates_or_loads_a_ray_destination() {
        let mut world = World::new(vec![region(1)], vec![]).unwrap();
        world.add_unbuilt_region(region(2), false).unwrap();
        let from = cell(1, 1, 1, 0);
        world
            .connect_named_stair(
                from,
                Direction::Down,
                NamedAnchor {
                    region: RegionId(2),
                    name: "landing".into(),
                },
            )
            .unwrap();
        let before = world.clone();
        assert!(world.is_stair(from, Direction::Down));
        assert_eq!(world.ray_step(from, Direction::Down), RayStep::Unavailable);
        assert_eq!(
            world.ray_step(from, Direction::EastDown),
            RayStep::Unavailable
        );
        assert_eq!(world, before);
        assert_eq!(
            world.loaded_regions().collect::<Vec<_>>(),
            vec![RegionId(1)]
        );
    }

    #[test]
    fn a_physical_join_to_detached_geometry_is_unavailable_until_attached() {
        let mut world = world();
        let from = cell(1, 2, 1, 1);
        let to = cell(2, 1, 1, 1);
        world
            .connect(
                Passage {
                    from,
                    direction: Direction::East,
                    to,
                },
                1,
            )
            .unwrap();
        let target = world.detach_region(RegionId(2)).unwrap();
        let before = world.clone();
        assert_eq!(world.physics_neighbor(from, Direction::East), Some((to, 1)));
        assert_eq!(world.ray_step(from, Direction::East), RayStep::Unavailable);
        assert_eq!(world.ray_step(to, Direction::North), RayStep::Unavailable);
        assert_eq!(world, before);
        world.attach_region(target).unwrap();
        assert_eq!(
            world.ray_step(from, Direction::East),
            RayStep::Clear { to, rotation: 1 }
        );
    }

    #[test]
    fn dependencies_diagonal_geometry_is_not_a_solid_boundary() {
        let mut world = world();
        let from = cell(1, 2, 1, 1);
        assert_eq!(world.ray_step(from, Direction::East), RayStep::Unavailable);
        assert_eq!(
            world.ray_step(from, Direction::NorthEast),
            RayStep::Unavailable
        );
        world.set_wall(cell(1, 2, 0, 1), true).unwrap();
        assert_eq!(
            world.ray_step(from, Direction::NorthEast),
            RayStep::Unavailable
        );
    }

    #[test]
    fn one_resolved_diagonal_route_suffices_when_the_alternate_is_dependencies() {
        let mut world = world();
        let from = cell(1, 2, 1, 1);
        let to = cell(2, 1, 1, 1);
        world
            .connect(
                Passage {
                    from: cell(1, 2, 0, 1),
                    direction: Direction::East,
                    to,
                },
                1,
            )
            .unwrap();
        assert_eq!(world.physics_neighbor(from, Direction::East), None);
        assert_eq!(
            world.ray_step(from, Direction::NorthEast),
            RayStep::Clear { to, rotation: 1 }
        );
        world.set_wall(to, true).unwrap();
        assert_eq!(world.ray_step(from, Direction::NorthEast), RayStep::Solid);
    }

    #[test]
    fn a_self_join_consumes_one_step_without_traversing_the_cycle() {
        let mut world = world();
        let from = cell(1, 2, 1, 1);
        world
            .connect(
                Passage {
                    from,
                    direction: Direction::East,
                    to: from,
                },
                1,
            )
            .unwrap();
        let before = world.clone();
        assert_eq!(
            world.ray_step(from, Direction::East),
            RayStep::Clear {
                to: from,
                rotation: 1,
            }
        );
        assert_eq!(world, before);
    }

    #[test]
    fn sight_only_rim_projection_does_not_redirect_a_physical_ray() {
        let mut world = World::new(vec![], vec![]).unwrap();
        world.add_chamber(region(1)).unwrap();
        world.add_chamber(region(2)).unwrap();
        world
            .connect(
                Passage {
                    from: cell(1, 2, 1, 1),
                    direction: Direction::East,
                    to: cell(2, 0, 1, 1),
                },
                0,
            )
            .unwrap();
        let from = cell(1, 2, 0, 1);
        let projected = cell(2, 0, 0, 1);
        world.set_wall(projected, true).unwrap();
        assert_eq!(world.adjacent(from, Direction::East), Some(projected));
        assert_eq!(world.ray_step(from, Direction::East), RayStep::Solid);
        world.set_wall(cell(1, 3, 0, 1), false).unwrap();
        assert_eq!(
            world.ray_step(from, Direction::East),
            RayStep::Clear {
                to: cell(1, 3, 0, 1),
                rotation: 0,
            }
        );
        assert_eq!(
            world.ray_step(cell(1, 3, 0, 1), Direction::East),
            RayStep::Unavailable
        );
    }
    #[test]
    fn complete_probe_reports_missing_alternative_before_clear_can_be_final() {
        let mut world = world();
        let from = cell(1, 2, 0, 1);
        for (direction, to) in [
            (Direction::North, cell(2, 1, 1, 1)),
            (Direction::East, cell(3, 1, 1, 1)),
        ] {
            world
                .connect(
                    Passage {
                        from,
                        direction,
                        to,
                    },
                    0,
                )
                .unwrap();
        }
        assert_eq!(
            world.ray_step(from, Direction::NorthEast),
            RayStep::Ambiguous
        );
        let detached = world.detach_region(RegionId(3)).unwrap();
        let before = world.clone();
        let (step, dependencies) = world.ray_step_dependencies(from, Direction::NorthEast);
        assert!(
            matches!(step, RayStep::Clear { .. }),
            "legacy local probe remains unchanged"
        );
        assert_eq!(step, world.ray_step(from, Direction::NorthEast));
        assert_eq!(dependencies, [RegionId(1), RegionId(2), RegionId(3)].into());
        assert!(world.region(RegionId(3)).is_none());
        assert_eq!(world, before, "dependency discovery never loads");
        world.attach_region(detached).unwrap();
        assert_eq!(
            world.ray_step_dependencies(from, Direction::NorthEast).0,
            RayStep::Ambiguous
        );
    }

    #[test]
    fn complete_probe_retains_loaded_intermediate_regions_in_composed_routes() {
        let mut world = world();
        let from = cell(1, 2, 1, 1);
        let side = cell(2, 2, 1, 1);
        let to = cell(3, 1, 1, 1);
        world.set_wall(cell(1, 2, 0, 1), true).unwrap();
        for (from, to, rotation) in [(from, side, 1), (side, to, 23)] {
            world
                .connect(
                    Passage {
                        from,
                        direction: Direction::East,
                        to,
                    },
                    rotation,
                )
                .unwrap();
        }
        let (step, dependencies) = world.ray_step_dependencies(from, Direction::NorthEast);
        assert_eq!(step, RayStep::Clear { to, rotation: 19 });
        assert_eq!(dependencies, [RegionId(1), RegionId(2), RegionId(3)].into());
        assert_ne!(side.region, from.region);
        assert_ne!(side.region, to.region);
    }
}
