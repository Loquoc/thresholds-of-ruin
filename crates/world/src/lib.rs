//! Region-local geometry, independent of rendering and transport.
//!
//! [`World`] owns physical cells, directed connections and barriers. Locations
//! are meaningful within their [`RegionId`], not a global coordinate system.
//! Connections can change the local frame using the 24 proper cube rotations;
//! [`compose_rotation`] applies its first transform before its second.
//! Geometry queries do not authorize an actor's movement, pay action costs or
//! load absent regions. Those responsibilities belong to the simulation/host.
//!
//! # Connecting regions and checking a barrier
//!
//! Reverse connections are explicit. [`World::adjacent`] can disclose a closed
//! door, while [`World::step`] additionally requires a walkable destination.
//!
//! ```
//! use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};
//!
//! let regions = [1, 2].map(|id| Region {
//!     id: RegionId(id),
//!     name: format!("Room {id}"),
//!     bounds: Extent::new(3, 3, 2).unwrap(),
//! });
//! let mut world = World::new(regions.into(), vec![]).unwrap();
//! let left = Location {
//!     region: RegionId(1),
//!     position: Position { x: 2, y: 1, z: 0 },
//! };
//! let right = Location {
//!     region: RegionId(2),
//!     position: Position { x: 0, y: 1, z: 0 },
//! };
//! world.connect(Passage { from: left, direction: Direction::East, to: right }, 0).unwrap();
//! assert_eq!(world.step(left, Direction::East), Some(right));
//! assert_eq!(world.step(right, Direction::West), None);
//! world.connect(Passage { from: right, direction: Direction::West, to: left }, 0).unwrap();
//!
//! world.place_door(right, 1, false, 1).unwrap();
//! assert_eq!(world.adjacent(left, Direction::East), Some(right));
//! assert_eq!(world.step(left, Direction::East), None);
//! world.set_door(right, true);
//! assert_eq!(world.step(left, Direction::East), Some(right));
//! assert_eq!(world.step(right, Direction::West), Some(left));
//! ```

pub mod checkpoint_map;
mod geometry;
pub use geometry::Adjacency;
mod ray;
pub use ray::RayStep;
mod lighting;
mod scene;
mod shadowcasting;
mod topology;
pub use scene::SightCell;
pub use sight_cache::{GeometrySnapshot, PerceptionSnapshot};
mod material;
pub use material::{Material, Terrain};

/// Physical scale shared by horizontal and vertical world cells.
pub const CELL_SIZE_FEET: u32 = 5;

pub use topology::checkpoint as checkpoint_worlds;
pub use topology::{
    ConcealedFeatureId, Concealment, Direction, Door, Location, NamedAnchor, Passage, Region,
    RegionId, RegionSlice, Trap, TrapFamily, TrapKind, World, WorldError, MAX_DOOR_HEIGHT,
};

/// Integer cell position in a region's local coordinate system.
///
/// This value does not identify a region or establish that a cell exists. Use
/// [`Location`] to name the region and [`World::contains`] to test loaded bounds.
/// Ordering is lexicographic by x, then y, then z; region table lookups rely on it.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct Position {
    /// Eastward cell coordinate; negative values can name chamber shell cells.
    pub x: i32,
    /// Southward cell coordinate; north decreases this value.
    pub y: i32,
    /// Upward cell coordinate, using the same [`CELL_SIZE_FEET`] scale as x and y.
    pub z: i32,
}

/// Positive dimensions and an origin; authored interiors start at zero, while
/// their material shells include negative coordinates.
///
/// Bounds include the origin and exclude origin plus dimensions. [`Self::new`]
/// validates positive dimensions; deserializing the representation alone does
/// not perform that constructor check. Untrusted metadata needs its enclosing
/// world/content validation before use.
///
/// ```
/// use tor_world::{Extent, Position};
/// let interior = Extent::new(3, 4, 2).unwrap();
/// let shell = interior.with_shell().unwrap();
/// assert_eq!(shell.origin(), Position { x: -1, y: -1, z: -1 });
/// assert_eq!(shell.dimensions(), (5, 6, 4));
/// let corner = Position { x: 3, y: 4, z: 2 };
/// assert!(!interior.contains(corner));
/// assert!(shell.contains(corner));
/// assert!(!shell.contains(Position { x: 4, ..corner }));
/// assert!(Extent::new(0, 4, 2).is_none());
/// assert!(Extent::new(i32::MAX, 4, 2).unwrap().with_shell().is_none());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extent {
    origin: Position,
    width: i32,
    depth: i32,
    height: i32,
}

impl Extent {
    /// Returns width (x), depth (y), and height (z).
    pub fn dimensions(self) -> (i32, i32, i32) {
        (self.width, self.depth, self.height)
    }
    /// The included minimum corner, without any coordinate translation.
    pub fn origin(self) -> Position {
        self.origin
    }

    /// Construct zero-origin bounds with positive x/y/z cell dimensions.
    ///
    /// Returns `None` if any dimension is zero or negative. No world cells or
    /// terrain are allocated by constructing bounds.
    pub fn new(width: i32, depth: i32, height: i32) -> Option<Self> {
        (width > 0 && depth > 0 && height > 0).then_some(Self {
            origin: Position { x: 0, y: 0, z: 0 },
            width,
            depth,
            height,
        })
    }

    /// Test all three half-open coordinate intervals, independent of terrain.
    ///
    /// Differences are computed in a wider integer domain, so extreme positions
    /// do not overflow the bounds test. This does not establish world residency.
    pub fn contains(self, position: Position) -> bool {
        let inside = |value: i32, start: i32, size: i32| {
            (0..i64::from(size)).contains(&(i64::from(value) - i64::from(start)))
        };
        inside(position.x, self.origin.x, self.width)
            && inside(position.y, self.origin.y, self.depth)
            && inside(position.z, self.origin.z, self.height)
    }

    /// Add one cell of finite storage on every face, without shifting coordinates.
    ///
    /// Returns `None` if subtracting one from an origin or adding two to a
    /// dimension would exceed the `i32` representation. This changes only bounds;
    /// [`World::add_chamber`] supplies the shell's default stone terrain.
    pub fn with_shell(self) -> Option<Self> {
        Some(Self {
            origin: Position {
                x: self.origin.x.checked_sub(1)?,
                y: self.origin.y.checked_sub(1)?,
                z: self.origin.z.checked_sub(1)?,
            },
            width: self.width.checked_add(2)?,
            depth: self.depth.checked_add(2)?,
            height: self.height.checked_add(2)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Extent;

    #[test]
    fn regions_require_positive_dimensions_on_every_axis() {
        for invalid in [i32::MIN, -1, 0] {
            assert!(Extent::new(invalid, 1, 1).is_none());
            assert!(Extent::new(1, invalid, 1).is_none());
            assert!(Extent::new(1, 1, invalid).is_none());
        }
        assert!(Extent::new(1, 1, 1).is_some());
    }
}

mod shared;
pub use shared::{Shared, SharedIdentity};

mod rotation;
pub use rotation::{compose_rotation, inverse_rotation, rotate_vector};

mod reach;
mod sight3d;
mod sight3d_fast;
mod sight_cache;
mod volume_scene;
