//! Coexisting planes: which plane each region is on, and the overlaps that
//! assert two volumes are the same place on different planes.
//!
//! An overlap is a similarity map between a box in one region and a volume in
//! another: `to = origin + rotate(rotation, from - min)`, scaled by `scale`.
//! Only `scale == 1` is supported; the field exists so scaled overlaps need no
//! format change later. Overlaps are structural: they are kept whatever the
//! residency of either region, like named anchors.
//!
//! Correspondence is symmetric. A location has at most one counterpart on each
//! other plane, so overlaps whose volumes would give a cell two counterparts on
//! the same plane are rejected. Overlaps never compose: a shift follows exactly
//! one declared overlap.

use std::collections::{BTreeMap, BTreeSet};

use crate::{inverse_rotation, rotate_vector, Location, Position, RegionId, World, WorldError};

/// A plane's identity. Zero is the material plane, which every region is on
/// unless declared otherwise.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct PlaneId(pub u16);

impl PlaneId {
    pub const MATERIAL: Self = Self(0);
}

/// The largest supported overlap scale. Scaled overlaps are reserved.
pub const MAX_OVERLAP_SCALE: u8 = 1;

/// Two volumes that are the same place on different planes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overlap {
    /// The region holding the box.
    pub from: RegionId,
    /// Inclusive box corners in `from`'s frame; `min <= max` on every axis.
    pub min: Position,
    pub max: Position,
    /// The region the box corresponds to.
    pub to: RegionId,
    /// Where `min` lands in `to`'s frame.
    pub origin: Position,
    /// Proper cube rotation from `from`'s frame to `to`'s, in `0..24`.
    pub rotation: u8,
    /// Reserved: must be 1.
    pub scale: u8,
}

fn offset(a: Position, b: Position) -> Option<[i64; 3]> {
    Some([
        i64::from(a.x) - i64::from(b.x),
        i64::from(a.y) - i64::from(b.y),
        i64::from(a.z) - i64::from(b.z),
    ])
}

fn translate(base: Position, v: [i64; 3]) -> Option<Position> {
    Some(Position {
        x: i32::try_from(i64::from(base.x).checked_add(v[0])?).ok()?,
        y: i32::try_from(i64::from(base.y).checked_add(v[1])?).ok()?,
        z: i32::try_from(i64::from(base.z).checked_add(v[2])?).ok()?,
    })
}

fn inside(min: Position, max: Position, at: Position) -> bool {
    (min.x..=max.x).contains(&at.x)
        && (min.y..=max.y).contains(&at.y)
        && (min.z..=max.z).contains(&at.z)
}

fn boxes_meet(a: (Position, Position), b: (Position, Position)) -> bool {
    a.0.x <= b.1.x
        && b.0.x <= a.1.x
        && a.0.y <= b.1.y
        && b.0.y <= a.1.y
        && a.0.z <= b.1.z
        && b.0.z <= a.1.z
}

impl Overlap {
    /// Map a position in `from` to its counterpart in `to`.
    pub fn forward(&self, at: Position) -> Option<Position> {
        if !inside(self.min, self.max, at) {
            return None;
        }
        translate(
            self.origin,
            rotate_vector(self.rotation, offset(at, self.min)?),
        )
    }

    /// Map a position in `to` back to its counterpart in `from`.
    pub fn backward(&self, at: Position) -> Option<Position> {
        let back = translate(
            self.min,
            rotate_vector(inverse_rotation(self.rotation), offset(at, self.origin)?),
        )?;
        inside(self.min, self.max, back).then_some(back)
    }

    /// The image of the box in `to`'s frame, as inclusive corners.
    pub fn image(&self) -> Option<(Position, Position)> {
        let a = self.forward(self.min)?;
        let b = self.forward(self.max)?;
        Some((
            Position {
                x: a.x.min(b.x),
                y: a.y.min(b.y),
                z: a.z.min(b.z),
            },
            Position {
                x: a.x.max(b.x),
                y: a.y.max(b.y),
                z: a.z.max(b.z),
            },
        ))
    }

    fn structurally_valid(&self) -> bool {
        self.rotation < 24
            && self.scale == MAX_OVERLAP_SCALE
            && self.from != self.to
            && self.min.x <= self.max.x
            && self.min.y <= self.max.y
            && self.min.z <= self.max.z
            && self.image().is_some()
    }
}

/// A run of region pairs that are the same place, for generated families:
/// region `from + i` (material) and region `to + i` (on `plane`), for every
/// `i < count`, over the whole box `0..=max` with the identity transform. A
/// run describes any number of pairs without listing them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverlapRange {
    pub from: RegionId,
    pub to: RegionId,
    pub count: u64,
    pub max: Position,
    pub plane: PlaneId,
}

impl OverlapRange {
    fn offset_in(start: RegionId, count: u64, region: RegionId) -> Option<u64> {
        let offset = region.0.checked_sub(start.0)?;
        (offset < count).then_some(offset)
    }
    /// The derived partner of a material region in the run.
    pub fn partner_of_source(&self, region: RegionId) -> Option<RegionId> {
        Self::offset_in(self.from, self.count, region).map(|i| RegionId(self.to.0 + i))
    }
    /// The material partner of a derived region in the run.
    pub fn partner_of_derived(&self, region: RegionId) -> Option<RegionId> {
        Self::offset_in(self.to, self.count, region).map(|i| RegionId(self.from.0 + i))
    }
    fn valid(&self) -> bool {
        self.count > 0
            && self.plane != PlaneId::MATERIAL
            && self.from.0.checked_add(self.count).is_some()
            && self.to.0.checked_add(self.count).is_some()
            && self.max.x >= 0
            && self.max.y >= 0
            && self.max.z >= 0
    }
    fn runs(&self) -> [(u64, u64); 2] {
        [
            (self.from.0, self.from.0 + self.count),
            (self.to.0, self.to.0 + self.count),
        ]
    }
}

/// One side of an overlap: a region, its box there, and the plane the box
/// corresponds to.
type Side = (RegionId, (Position, Position), PlaneId);

/// Plane membership and overlaps, stored on [`World`].
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlaneTopology {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) planes: BTreeMap<RegionId, PlaneId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) overlaps: Vec<Overlap>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) ranges: Vec<OverlapRange>,
}

impl PlaneTopology {
    pub(crate) fn is_empty(&self) -> bool {
        self.planes.is_empty() && self.overlaps.is_empty() && self.ranges.is_empty()
    }

    fn plane_of(&self, region: RegionId) -> PlaneId {
        if let Some(plane) = self.planes.get(&region) {
            return *plane;
        }
        self.ranges
            .iter()
            .find(|range| range.partner_of_derived(region).is_some())
            .map_or(PlaneId::MATERIAL, |range| range.plane)
    }

    /// Each side of an overlap: the region, its box there, and the plane the
    /// box corresponds to.
    fn sides(&self, overlap: &Overlap) -> Option<[Side; 2]> {
        Some([
            (
                overlap.from,
                (overlap.min, overlap.max),
                self.plane_of(overlap.to),
            ),
            (overlap.to, overlap.image()?, self.plane_of(overlap.from)),
        ])
    }

    /// Whether every overlap is well formed, joins regions on different
    /// planes, and gives each cell at most one counterpart per plane.
    pub(crate) fn valid(&self) -> bool {
        let mut sides = Vec::with_capacity(self.overlaps.len() * 2);
        for overlap in &self.overlaps {
            if !overlap.structurally_valid()
                || self.plane_of(overlap.from) == self.plane_of(overlap.to)
            {
                return false;
            }
            let Some(pair) = self.sides(overlap) else {
                return false;
            };
            for side in pair {
                if sides.iter().any(|(region, cells, plane)| {
                    *region == side.0 && *plane == side.2 && boxes_meet(*cells, side.1)
                }) {
                    return false;
                }
                sides.push(side);
            }
        }
        // Runs never share regions with each other or with listed planes or
        // overlaps, so each region keeps one plane and one counterpart rule.
        let runs: Vec<(u64, u64)> = self.ranges.iter().flat_map(OverlapRange::runs).collect();
        let in_runs = |region: RegionId| runs.iter().any(|(a, b)| (*a..*b).contains(&region.0));
        self.ranges.iter().all(OverlapRange::valid)
            && runs
                .iter()
                .enumerate()
                .all(|(i, (a, b))| runs.iter().skip(i + 1).all(|(c, d)| b <= c || d <= a))
            && !self.planes.keys().copied().any(in_runs)
            && !self
                .overlaps
                .iter()
                .any(|overlap| in_runs(overlap.from) || in_runs(overlap.to))
            && self
                .planes
                .values()
                .all(|plane| *plane != PlaneId::MATERIAL)
    }
}

impl World {
    /// The plane `region` is on. Undeclared regions are material.
    pub fn plane_of(&self, region: RegionId) -> PlaneId {
        self.planes.plane_of(region)
    }

    /// Whether any region is on a plane other than the material one.
    pub fn has_planes(&self) -> bool {
        !self.planes.is_empty()
    }

    /// Put `region` on `plane`. The region need not be loaded or built.
    ///
    /// # Errors
    ///
    /// [`WorldError::InvalidEndpoint`] if the change would make an existing
    /// overlap join two regions on one plane or give a cell two counterparts.
    pub fn set_region_plane(&mut self, region: RegionId, plane: PlaneId) -> Result<(), WorldError> {
        let mut next = (*self.planes).clone();
        if plane == PlaneId::MATERIAL {
            next.planes.remove(&region);
        } else {
            next.planes.insert(region, plane);
        }
        if !next.valid() {
            return Err(WorldError::InvalidEndpoint);
        }
        *self.planes = next;
        Ok(())
    }

    /// Declare that a box of one region is the same place as a volume of
    /// another region on a different plane.
    ///
    /// # Errors
    ///
    /// [`WorldError::InvalidRotation`] for a rotation outside `0..24`, and
    /// [`WorldError::InvalidEndpoint`] for an unsupported scale, an empty or
    /// unrepresentable box, two regions on the same plane, or a volume that
    /// would give a cell a second counterpart on one plane. Rejection changes
    /// nothing.
    pub fn add_overlap(&mut self, overlap: Overlap) -> Result<(), WorldError> {
        if overlap.rotation >= 24 {
            return Err(WorldError::InvalidRotation);
        }
        let mut next = (*self.planes).clone();
        next.overlaps.push(overlap);
        if !next.valid() {
            return Err(WorldError::InvalidEndpoint);
        }
        *self.planes = next;
        Ok(())
    }

    /// Declare a run of region pairs that are the same place.
    ///
    /// # Errors
    ///
    /// [`WorldError::InvalidEndpoint`] for an empty, overflowing or
    /// material-plane run, or one sharing regions with other planes.
    pub fn add_overlap_range(&mut self, range: OverlapRange) -> Result<(), WorldError> {
        let mut next = (*self.planes).clone();
        next.ranges.push(range);
        if !next.valid() {
            return Err(WorldError::InvalidEndpoint);
        }
        *self.planes = next;
        Ok(())
    }

    /// Every declared overlap range.
    pub fn overlap_ranges(&self) -> &[OverlapRange] {
        &self.planes.ranges
    }

    /// Every declared overlap, in declaration order.
    pub fn overlaps(&self) -> &[Overlap] {
        &self.planes.overlaps
    }

    /// The counterpart of `at` on `plane`, and the rotation from `at`'s frame
    /// to the counterpart's. `None` when no declared overlap covers `at` and
    /// leads to that plane.
    pub fn counterpart(&self, at: Location, plane: PlaneId) -> Option<(Location, u8)> {
        if self.plane_of(at.region) == plane {
            return None;
        }
        let inside = |max: Position| {
            (0..=max.x).contains(&at.position.x)
                && (0..=max.y).contains(&at.position.y)
                && (0..=max.z).contains(&at.position.z)
        };
        for range in &self.planes.ranges {
            if let Some(to) = range.partner_of_source(at.region) {
                if range.plane == plane && inside(range.max) {
                    return Some((Location { region: to, ..at }, 0));
                }
            } else if let Some(from) = range.partner_of_derived(at.region) {
                if plane == PlaneId::MATERIAL && inside(range.max) {
                    return Some((Location { region: from, ..at }, 0));
                }
            }
        }
        self.planes.overlaps.iter().find_map(|overlap| {
            if overlap.from == at.region && self.plane_of(overlap.to) == plane {
                let position = overlap.forward(at.position)?;
                Some((
                    Location {
                        region: overlap.to,
                        position,
                    },
                    overlap.rotation,
                ))
            } else if overlap.to == at.region && self.plane_of(overlap.from) == plane {
                let position = overlap.backward(at.position)?;
                Some((
                    Location {
                        region: overlap.from,
                        position,
                    },
                    inverse_rotation(overlap.rotation),
                ))
            } else {
                None
            }
        })
    }

    /// Regions sharing an overlap with `region`, in region order.
    pub fn overlap_partners(&self, region: RegionId) -> BTreeSet<RegionId> {
        self.planes
            .overlaps
            .iter()
            .filter_map(|overlap| {
                if overlap.from == region {
                    Some(overlap.to)
                } else if overlap.to == region {
                    Some(overlap.from)
                } else {
                    None
                }
            })
            .chain(self.planes.ranges.iter().filter_map(|range| {
                range
                    .partner_of_source(region)
                    .or_else(|| range.partner_of_derived(region))
            }))
            .collect()
    }

    /// The overlap group of `region`: every region connected to it through
    /// overlaps and overlap ranges, including itself, in region order.
    pub fn overlap_group(&self, region: RegionId) -> BTreeSet<RegionId> {
        let mut group = BTreeSet::from([region]);
        let mut queue = vec![region];
        while let Some(next) = queue.pop() {
            for partner in self.overlap_partners(next) {
                if group.insert(partner) {
                    queue.push(partner);
                }
            }
        }
        group
    }

    /// Whether any overlap or overlap range is declared.
    pub fn has_overlaps(&self) -> bool {
        !self.planes.overlaps.is_empty() || !self.planes.ranges.is_empty()
    }
}

/// The overlap groups of `overlaps`: connected components of regions linked by
/// overlaps, each in region order, ordered by their least region.
pub fn overlap_groups(overlaps: &[Overlap]) -> Vec<BTreeSet<RegionId>> {
    let mut links: BTreeMap<RegionId, BTreeSet<RegionId>> = BTreeMap::new();
    for overlap in overlaps {
        links.entry(overlap.from).or_default().insert(overlap.to);
        links.entry(overlap.to).or_default().insert(overlap.from);
    }
    let mut seen = BTreeSet::new();
    let mut groups = Vec::new();
    for start in links.keys().copied() {
        if !seen.insert(start) {
            continue;
        }
        let mut group = BTreeSet::from([start]);
        let mut queue = vec![start];
        while let Some(region) = queue.pop() {
            for next in links.get(&region).into_iter().flatten().copied() {
                if seen.insert(next) {
                    group.insert(next);
                    queue.push(next);
                }
            }
        }
        groups.push(group);
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Extent, Region};

    fn at(x: i32, y: i32, z: i32) -> Position {
        Position { x, y, z }
    }

    fn world() -> World {
        let region = |id, width| Region {
            id: RegionId(id),
            name: format!("r{id}"),
            bounds: Extent::new(width, 4, 2).unwrap(),
        };
        let mut world = World::new(vec![region(1, 6), region(2, 6), region(3, 6)], vec![]).unwrap();
        world.set_region_plane(RegionId(2), PlaneId(1)).unwrap();
        world.set_region_plane(RegionId(3), PlaneId(1)).unwrap();
        world
    }

    fn overlap(from: u64, to: u64, rotation: u8) -> Overlap {
        Overlap {
            from: RegionId(from),
            min: at(0, 0, 0),
            max: at(2, 3, 1),
            to: RegionId(to),
            origin: at(1, 0, 0),
            rotation,
            scale: 1,
        }
    }

    #[test]
    fn counterparts_map_both_ways_and_only_inside_the_volume() {
        let mut world = world();
        world.add_overlap(overlap(1, 2, 0)).unwrap();
        let material = Location {
            region: RegionId(1),
            position: at(2, 1, 0),
        };
        let (ethereal, rotation) = world.counterpart(material, PlaneId(1)).unwrap();
        assert_eq!(ethereal.region, RegionId(2));
        assert_eq!(ethereal.position, at(3, 1, 0));
        assert_eq!(rotation, 0);
        assert_eq!(
            world.counterpart(ethereal, PlaneId::MATERIAL),
            Some((material, 0))
        );
        // Outside the box: no counterpart on either side.
        let outside = Location {
            region: RegionId(1),
            position: at(3, 1, 0),
        };
        assert_eq!(world.counterpart(outside, PlaneId(1)), None);
        // A region is never its own plane's counterpart.
        assert_eq!(world.counterpart(material, PlaneId::MATERIAL), None);
    }

    #[test]
    fn rotated_overlaps_invert_exactly() {
        for rotation in 0..24 {
            let mut world = world();
            let mut o = overlap(1, 2, rotation);
            o.origin = at(10, 10, 10);
            world.add_overlap(o).unwrap();
            for x in 0..=2 {
                for y in 0..=3 {
                    for z in 0..=1 {
                        let from = Location {
                            region: RegionId(1),
                            position: at(x, y, z),
                        };
                        let (to, r) = world.counterpart(from, PlaneId(1)).unwrap();
                        assert_eq!(r, rotation);
                        let (back, inverse) = world.counterpart(to, PlaneId::MATERIAL).unwrap();
                        assert_eq!(back, from);
                        assert_eq!(crate::compose_rotation(r, inverse), 0);
                    }
                }
            }
        }
    }

    #[test]
    fn invalid_overlaps_are_rejected_without_change() {
        let mut world = world();
        let mut scaled = overlap(1, 2, 0);
        scaled.scale = 2;
        assert_eq!(world.add_overlap(scaled), Err(WorldError::InvalidEndpoint));
        assert_eq!(
            world.add_overlap(overlap(1, 2, 24)),
            Err(WorldError::InvalidRotation)
        );
        // Same plane on both sides.
        assert_eq!(
            world.add_overlap(overlap(2, 3, 0)),
            Err(WorldError::InvalidEndpoint)
        );
        let mut empty = overlap(1, 2, 0);
        empty.max = at(-1, 0, 0);
        assert_eq!(world.add_overlap(empty), Err(WorldError::InvalidEndpoint));
        assert!(world.overlaps().is_empty());
    }

    #[test]
    fn a_cell_has_at_most_one_counterpart_per_plane() {
        let mut world = world();
        world.add_overlap(overlap(1, 2, 0)).unwrap();
        // The same material cells cannot also coexist with region 3.
        assert_eq!(
            world.add_overlap(overlap(1, 3, 0)),
            Err(WorldError::InvalidEndpoint)
        );
        // Disjoint material cells can.
        let mut other = overlap(1, 3, 0);
        other.min = at(3, 0, 0);
        other.max = at(5, 3, 1);
        world.add_overlap(other).unwrap();
        // Moving a region onto a plane that makes an overlap same-plane fails.
        assert_eq!(
            world.set_region_plane(RegionId(1), PlaneId(1)),
            Err(WorldError::InvalidEndpoint)
        );
        assert_eq!(world.plane_of(RegionId(1)), PlaneId::MATERIAL);
    }

    #[test]
    fn overlap_ranges_pair_whole_runs_of_regions() {
        let mut world = World::new(vec![], vec![]).unwrap();
        let run = OverlapRange {
            from: RegionId(1),
            to: RegionId(101),
            count: 50,
            max: at(9, 9, 1),
            plane: PlaneId(1),
        };
        world.add_overlap_range(run).unwrap();
        assert_eq!(world.plane_of(RegionId(120)), PlaneId(1));
        assert_eq!(world.plane_of(RegionId(20)), PlaneId::MATERIAL);
        assert_eq!(world.plane_of(RegionId(151)), PlaneId::MATERIAL);
        let cell = Location {
            region: RegionId(20),
            position: at(3, 4, 1),
        };
        let (shadow, rotation) = world.counterpart(cell, PlaneId(1)).unwrap();
        assert_eq!(
            (shadow.region, shadow.position, rotation),
            (RegionId(120), at(3, 4, 1), 0)
        );
        assert_eq!(
            world.counterpart(shadow, PlaneId::MATERIAL),
            Some((cell, 0))
        );
        // Outside the box there is no counterpart.
        let outside = Location {
            region: RegionId(20),
            position: at(10, 4, 1),
        };
        assert_eq!(world.counterpart(outside, PlaneId(1)), None);
        assert_eq!(
            world.overlap_group(RegionId(20)),
            BTreeSet::from([RegionId(20), RegionId(120)])
        );
        // Runs may not share regions.
        let mut clash = run;
        clash.to = RegionId(140);
        assert_eq!(
            world.add_overlap_range(clash),
            Err(WorldError::InvalidEndpoint)
        );
        assert_eq!(world.overlap_ranges().len(), 1);
    }

    #[test]
    fn groups_follow_overlaps_only() {
        let mut world = world();
        world.add_overlap(overlap(1, 2, 0)).unwrap();
        assert_eq!(
            world.overlap_group(RegionId(2)),
            BTreeSet::from([RegionId(1), RegionId(2)])
        );
        assert_eq!(
            world.overlap_group(RegionId(3)),
            BTreeSet::from([RegionId(3)])
        );
        assert_eq!(
            world.overlap_partners(RegionId(1)),
            BTreeSet::from([RegionId(2)])
        );
    }
}
