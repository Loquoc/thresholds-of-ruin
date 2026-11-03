use std::collections::{BTreeMap, BTreeSet};

use crate::{Extent, Material, Position, Shared, Terrain};

#[path = "world_checkpoint.rs"]
pub mod checkpoint;
#[path = "region_slice.rs"]
mod region_slice;
pub use region_slice::RegionSlice;

/// Region identity used to scope coordinates and deterministic table ordering.
///
/// This is a numeric label, not a loaded-region handle or allocation authority.
/// Construction/deserialization does not prove that a world knows the identity.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct RegionId(#[doc = "The host-assigned numeric identity."] pub u64);

/// A cell coordinate paired with the region whose frame gives it meaning.
///
/// Ordering compares the region before its position. Region-local range scans
/// depend on that order. A value may name unloaded or nonexistent geometry;
/// distinguish [`World::knows`] from [`World::contains`].
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(deny_unknown_fields)]
pub struct Location {
    /// Owner of the coordinate frame, not proof of current residency.
    pub region: RegionId,
    /// Cell position in that owner's local frame.
    pub position: Position,
}

/// A named destination whose coordinates are published with its region.
///
/// A stair may retain this reference before its coordinates or region content
/// are available. Constructing/deserializing it does not register or validate
/// the name; [`World::connect_named_stair`] and [`World::register_named_anchors`]
/// check their respective input contracts.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct NamedAnchor {
    /// Known destination region, which need not be loaded.
    pub region: RegionId,
    /// Anchor key: 1–80 ASCII letters, digits, underscores or hyphens when registered.
    pub name: String,
}

impl<'de> serde::Deserialize<'de> for NamedAnchor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            region: RegionId,
            name: String,
        }
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = NamedAnchor;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a named anchor record")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                map: M,
            ) -> Result<Self::Value, M::Error> {
                let Fields { region, name } = serde::Deserialize::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )?;
                Ok(NamedAnchor { region, name })
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

/// Directions in the current region's coordinate system; z increases upward.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum Direction {
    /// One cell toward decreasing y.
    North,
    /// One cell toward increasing x.
    East,
    /// One cell toward increasing y.
    South,
    /// One cell toward decreasing x.
    West,
    /// Increasing x and decreasing y.
    NorthEast,
    /// Increasing x and increasing y.
    SouthEast,
    /// Decreasing x and increasing y.
    SouthWest,
    /// Decreasing x and decreasing y.
    NorthWest,
    /// One cell toward increasing z.
    Up,
    /// One cell toward decreasing z.
    Down,
    /// Increasing x and increasing z.
    EastUp,
    /// Decreasing x and increasing z.
    WestUp,
    /// Decreasing y and increasing z.
    NorthUp,
    /// Increasing y and increasing z.
    SouthUp,
    /// Increasing x and decreasing z.
    EastDown,
    /// Decreasing x and decreasing z.
    WestDown,
    /// Decreasing y and decreasing z.
    NorthDown,
    /// Increasing y and decreasing z.
    SouthDown,
}

impl Direction {
    /// The four horizontal cardinals in NESW order, then NE, SE, SW and NW.
    pub const HORIZONTAL: [Self; 8] = [
        Self::North,
        Self::East,
        Self::South,
        Self::West,
        Self::NorthEast,
        Self::SouthEast,
        Self::SouthWest,
        Self::NorthWest,
    ];

    /// Decompose a two-axis diagonal into its ordered cardinal components.
    ///
    /// Returns `None` for the six cardinals. This is a coordinate decomposition,
    /// not a traversability test; [`World::diagonal_reach`] checks the routes.
    #[inline]
    pub fn components(self) -> Option<(Self, Self)> {
        match self {
            Self::NorthEast => Some((Self::North, Self::East)),
            Self::SouthEast => Some((Self::South, Self::East)),
            Self::SouthWest => Some((Self::South, Self::West)),
            Self::NorthWest => Some((Self::North, Self::West)),
            Self::EastUp => Some((Self::East, Self::Up)),
            Self::WestUp => Some((Self::West, Self::Up)),
            Self::NorthUp => Some((Self::North, Self::Up)),
            Self::SouthUp => Some((Self::South, Self::Up)),
            Self::EastDown => Some((Self::East, Self::Down)),
            Self::WestDown => Some((Self::West, Self::Down)),
            Self::NorthDown => Some((Self::North, Self::Down)),
            Self::SouthDown => Some((Self::South, Self::Down)),
            _ => None,
        }
    }

    /// Return the unit cell delta `(x, y, z)` in the current region's frame.
    ///
    /// Diagonal components each have magnitude one; the vector is not normalized
    /// to a Euclidean unit length. Crossing a portal can change the local frame.
    #[inline]
    pub fn delta(self) -> (i32, i32, i32) {
        match self {
            Self::NorthEast => (1, -1, 0),
            Self::SouthEast => (1, 1, 0),
            Self::SouthWest => (-1, 1, 0),
            Self::NorthWest => (-1, -1, 0),
            Self::North => (0, -1, 0),
            Self::East => (1, 0, 0),
            Self::South => (0, 1, 0),
            Self::West => (-1, 0, 0),
            Self::Up => (0, 0, 1),
            Self::Down => (0, 0, -1),
            Self::EastUp => (1, 0, 1),
            Self::WestUp => (-1, 0, 1),
            Self::NorthUp => (0, -1, 1),
            Self::SouthUp => (0, 1, 1),
            Self::EastDown => (1, 0, -1),
            Self::WestDown => (-1, 0, -1),
            Self::NorthDown => (0, -1, -1),
            Self::SouthDown => (0, 1, -1),
        }
    }

    pub(crate) fn offset(self, position: Position) -> Option<Position> {
        let (x, y, z) = self.delta();
        Some(Position {
            x: position.x.checked_add(x)?,
            y: position.y.checked_add(y)?,
            z: position.z.checked_add(z)?,
        })
    }

    /// Match an exact cardinal or two-axis diagonal unit delta.
    ///
    /// Returns `None` for zero, longer vectors and three-axis corner diagonals.
    /// Components are not rounded, normalized or reduced to their signs.
    ///
    /// ```
    /// use tor_world::Direction;
    /// assert_eq!(Direction::from_delta([1, 0, 1]), Some(Direction::EastUp));
    /// assert_eq!(Direction::from_delta([2, 0, 2]), None);
    /// assert_eq!(Direction::from_delta([1, 1, 1]), None);
    /// assert_eq!(Direction::East.rotated(1), Direction::South);
    /// assert_eq!(Direction::NorthEast.components(), Some((Direction::North, Direction::East)));
    /// ```
    #[inline]
    pub fn from_delta(v: [i64; 3]) -> Option<Self> {
        Some(match v {
            [0, -1, 0] => Self::North,
            [1, 0, 0] => Self::East,
            [0, 1, 0] => Self::South,
            [-1, 0, 0] => Self::West,
            [0, 0, 1] => Self::Up,
            [0, 0, -1] => Self::Down,
            [1, -1, 0] => Self::NorthEast,
            [1, 1, 0] => Self::SouthEast,
            [-1, 1, 0] => Self::SouthWest,
            [-1, -1, 0] => Self::NorthWest,
            [1, 0, 1] => Self::EastUp,
            [-1, 0, 1] => Self::WestUp,
            [0, -1, 1] => Self::NorthUp,
            [0, 1, 1] => Self::SouthUp,
            [1, 0, -1] => Self::EastDown,
            [-1, 0, -1] => Self::WestDown,
            [0, -1, -1] => Self::NorthDown,
            [0, 1, -1] => Self::SouthDown,
            _ => return None,
        })
    }
    /// Transform this direction with a proper cube-rotation ID in `0..24`.
    ///
    /// `turns` is the historical name for the full rotation ID, not an arbitrary
    /// count of quarter turns. See [`crate::rotate_vector`].
    ///
    /// # Panics
    ///
    /// Panics for IDs outside `0..24`.
    #[inline]
    pub fn rotated(self, turns: u8) -> Self {
        if turns == 0 {
            return self;
        }
        let (x, y, z) = self.delta();
        Self::from_delta(crate::rotate_vector(
            turns,
            [i64::from(x), i64::from(y), i64::from(z)],
        ))
        .expect("cube direction")
    }
}

/// A bounded rectangular volume with region-local coordinates.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    /// Stable owner identity; adding a region does not allocate this value.
    pub id: RegionId,
    /// Authored display metadata, not a unique lookup key.
    pub name: String,
    /// Storage bounds. A chamber's registered bounds include its material shell.
    pub bounds: Extent,
}

/// A directed, single-cell connection. Horizontal apertures are at boundaries;
/// explicit vertical links can represent stairs within a room. Rotation metadata
/// is validated by [`World::connect`]. A passage need not contain a door entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Passage {
    /// Source cell in the owning region; the link is stored with this region.
    pub from: Location,
    /// Cardinal exit direction in the source frame.
    pub direction: Direction,
    /// Destination cell in its own region's frame.
    pub to: Location,
}

/// Rejection categories for world construction and topology mutation.
///
/// Individual methods describe their validation boundary; these categories do
/// not encode actor permission, simulation costs or persistence failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldError {
    /// The relevant region registry already contains this identity.
    DuplicateRegion,
    /// A location, record, geometry parameter or other structural input is invalid.
    InvalidEndpoint,
    /// A horizontal connection would not exit the source region/interior boundary.
    NotBoundaryExit,
    /// Another ordinary passage or named stair already owns the exit.
    DuplicateExit,
    /// A crossing rotation is outside the 24 supported proper cube rotations.
    InvalidRotation,
}

/// Region topology and physical payloads with stable iteration order.
///
/// Loaded geometry and known-but-absent metadata are separate. Values obtained by
/// raw deserialization still require the enclosing checkpoint/content validation;
/// serialization support is not a proof of validity or client disclosure.
/// Cloning uses copy-on-write storage, while equality ignores derived sight caches.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct World {
    #[serde(
        default,
        skip_serializing_if = "no_named_stairs",
        with = "crate::checkpoint_map::shared"
    )]
    named_stairs: Shared<BTreeMap<(Location, Direction), NamedAnchor>>,
    /// Immutable structural coordinates retained when terrain is detached.
    #[serde(default, skip_serializing_if = "no_named_anchors")]
    named_anchors: Shared<BTreeMap<RegionId, BTreeMap<String, Position>>>,
    #[serde(with = "crate::checkpoint_map::shared")]
    doors: Shared<BTreeMap<Location, Door>>,
    #[serde(with = "crate::checkpoint_map::shared")]
    secret_doors: Shared<BTreeMap<Location, Concealment>>,
    #[serde(with = "crate::checkpoint_map::shared")]
    traps: Shared<BTreeMap<Location, Trap>>,
    regions: Shared<BTreeMap<RegionId, Region>>,
    #[serde(with = "crate::checkpoint_map::shared")]
    passages: Shared<BTreeMap<(Location, Direction), Passage>>,
    #[serde(with = "crate::checkpoint_map::shared")]
    rotations: Shared<BTreeMap<(Location, Direction), u8>>,
    physical_vertical: Shared<BTreeSet<(Location, Direction)>>,
    #[serde(default, skip_serializing_if = "no_solid_boundaries")]
    solid_boundaries: Shared<BTreeSet<(Location, Direction)>>,
    region_gravity: Shared<BTreeMap<RegionId, [i32; 3]>>,
    #[serde(with = "crate::checkpoint_map::shared")]
    cell_gravity: Shared<BTreeMap<Location, [i32; 3]>>,
    #[serde(with = "crate::checkpoint_map::shared")]
    terrain: Shared<BTreeMap<Location, Terrain>>,
    #[serde(default)]
    pub(crate) lighting: Shared<BTreeMap<RegionId, crate::lighting::RegionLight>>,
    /// Carved interior extents also locate join apertures; storage includes a shell.
    chambers: Shared<BTreeMap<RegionId, Extent>>,
    /// Authored place hints, each with its authored name ("" when it has
    /// none).
    #[serde(with = "crate::checkpoint_map::shared")]
    place_hints: Shared<BTreeMap<Location, String>>,
    /// Metadata of detached regions, whose content is held elsewhere as a
    /// [`RegionSlice`]. Omitted from saves while empty.
    #[serde(default, skip_serializing_if = "no_absent_regions")]
    absent: Shared<BTreeMap<RegionId, Region>>,
    /// Derived: never saved, and ignored by equality.
    #[serde(skip)]
    pub(crate) sight: crate::sight_cache::SightCache,
}

/// A barrier entity, unrelated to portal identity. It is stored at its base
/// cell and occupies `height` cells straight up from it, filling its opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Door {
    pub id: u64,
    pub open: bool,
    pub height: u8,
}

/// Doors are at most this many cells tall.
pub const MAX_DOOR_HEIGHT: u8 = 8;

fn no_absent_regions(absent: &Shared<BTreeMap<RegionId, Region>>) -> bool {
    absent.is_empty()
}
fn no_solid_boundaries(value: &Shared<BTreeSet<(Location, Direction)>>) -> bool {
    value.is_empty()
}
fn no_named_stairs(value: &Shared<BTreeMap<(Location, Direction), NamedAnchor>>) -> bool {
    value.is_empty()
}
fn no_named_anchors(value: &Shared<BTreeMap<RegionId, BTreeMap<String, Position>>>) -> bool {
    value.is_empty()
}
fn valid_anchor_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 80
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

fn raised(location: Location, cells: i32) -> Option<Location> {
    Some(Location {
        position: Position {
            z: location.position.z.checked_add(cells)?,
            ..location.position
        },
        ..location
    })
}

impl World {
    /// Publish immutable coordinates, independently of region residency.
    ///
    /// Names are 1–80 ASCII letters, digits, underscores or hyphens; each
    /// coordinate must satisfy [`World::knows`]. Publishing the same map again
    /// is allowed. An empty map is not stored, and publication never loads terrain.
    ///
    /// # Errors
    ///
    /// Returns [`WorldError::InvalidEndpoint`] for an invalid name/coordinate or
    /// a map differing from the region's already-published map. Rejection leaves
    /// the world unchanged.
    pub fn register_named_anchors(
        &mut self,
        region: RegionId,
        anchors: BTreeMap<String, Position>,
    ) -> Result<(), WorldError> {
        if anchors.iter().any(|(name, position)| {
            !valid_anchor_name(name)
                || !self.knows(Location {
                    region,
                    position: *position,
                })
        }) || self
            .named_anchors
            .get(&region)
            .is_some_and(|existing| existing != &anchors)
        {
            return Err(WorldError::InvalidEndpoint);
        }
        if !anchors.is_empty() {
            self.named_anchors.insert(region, anchors);
            self.sight.region_changed(region);
        }
        Ok(())
    }
    /// Connect a visible logical stair without inspecting its destination's terrain.
    ///
    /// The destination region must be known, but its named coordinates may be
    /// unresolved and its content absent. This directed link creates no reverse
    /// stair. Use [`World::stair_region`] for its owner before coordinates exist,
    /// and [`World::link_destination`] after anchor publication.
    ///
    /// # Errors
    ///
    /// Returns [`WorldError::InvalidEndpoint`] unless the direction is up/down,
    /// the source is walkable, the destination region is known, its anchor name
    /// has valid syntax, and no passage/named stair owns this exit. A rejected
    /// connection changes nothing; a successful one does not authorize movement.
    ///
    /// ```
    /// use std::collections::{BTreeMap, BTreeSet};
    /// use tor_world::{Direction, Extent, Location, NamedAnchor, Position, Region, RegionId, World};
    /// let region = |id| Region {
    ///     id: RegionId(id), name: format!("Floor {id}"),
    ///     bounds: Extent::new(3, 3, 2).unwrap(),
    /// };
    /// let from = Location { region: RegionId(1), position: Position { x: 1, y: 1, z: 0 } };
    /// let to = Location { region: RegionId(2), position: Position { x: 2, y: 1, z: 0 } };
    /// let mut world = World::new(vec![region(1)], vec![]).unwrap();
    /// world.add_unbuilt_region(region(2), false).unwrap();
    /// world.connect_named_stair(from, Direction::Down, NamedAnchor {
    ///     region: RegionId(2), name: "landing".into(),
    /// }).unwrap();
    /// assert_eq!(world.stair_region(from, Direction::Down), Some(RegionId(2)));
    /// assert_eq!(world.linked_regions(RegionId(1)), BTreeSet::from([RegionId(2)]));
    /// assert!(world.required_linked_regions(RegionId(1)).is_empty());
    /// assert_eq!(world.link_destination(from, Direction::Down), None);
    ///
    /// let mut generated = World::new(vec![region(2)], vec![]).unwrap();
    /// generated.register_named_anchors(RegionId(2), BTreeMap::from([
    ///     ("landing".into(), to.position),
    /// ])).unwrap();
    /// let target = generated.detach_region(RegionId(2)).unwrap();
    /// world.publish_region_anchors(&target).unwrap();
    /// assert_eq!(world.required_linked_regions(RegionId(1)), BTreeSet::from([RegionId(2)]));
    /// assert_eq!(world.link_destination(from, Direction::Down), Some(to));
    /// assert!(!world.contains(to)); // Coordinates can resolve before terrain arrives.
    /// assert_eq!(world.step(from, Direction::Down), None);
    /// world.attach_region(target).unwrap();
    /// assert_eq!(world.step(from, Direction::Down), Some(to));
    ///
    /// let before = world.clone();
    /// assert!(world.register_named_anchors(RegionId(2), BTreeMap::from([
    ///     ("landing".into(), Position { x: 0, y: 1, z: 0 }),
    /// ])).is_err());
    /// assert_eq!(world, before); // Published anchors cannot silently move.
    /// ```
    pub fn connect_named_stair(
        &mut self,
        from: Location,
        direction: Direction,
        to: NamedAnchor,
    ) -> Result<(), WorldError> {
        if !matches!(direction, Direction::Up | Direction::Down)
            || !self.walkable(from)
            || !self.knows_region(to.region)
            || !valid_anchor_name(&to.name)
            || self.passages.contains_key(&(from, direction))
            || self.named_stairs.contains_key(&(from, direction))
        {
            return Err(WorldError::InvalidEndpoint);
        }
        self.named_stairs.insert((from, direction), to);
        self.sight.region_changed(from.region);
        Ok(())
    }
    /// Invalidates derived geometry mappings when any topology or region changes.
    /// Tokens only select cache reuse; they never affect world behavior.
    pub fn geometry_snapshot(&self) -> crate::GeometrySnapshot {
        self.sight.geometry_snapshot()
    }
    /// Invalidates derived perception when geometry, residency or light changes.
    pub fn perception_snapshot(&self) -> crate::PerceptionSnapshot {
        self.sight.perception_snapshot()
    }
    /// Authored portal endpoints must remain clear of solid terrain. Closed
    /// doors are ordinary gameplay state and do not invalidate a package.
    pub fn authored_links_clear(&self) -> bool {
        self.passages.iter().all(|(key, p)| {
            self.solid_boundaries.contains(key) || (!self.is_wall(p.from) && !self.is_wall(p.to))
        }) && self
            .named_stairs
            .keys()
            .all(|(from, _)| !self.is_wall(*from))
    }
    /// Full structural proof for an immutable historical World, which never
    /// installs or advances the current Game's door allocator. Every invariant
    /// of ordinary restoration remains, except its current next-ID comparison.
    /// Door ID zero/MAX remain impossible; MAX cannot be allocated with a next ID.
    pub fn historical_checkpoint_valid(&self) -> bool {
        self.checkpoint_valid(u64::MAX)
    }
    /// Structural validation for backend checkpoint restoration.
    pub fn checkpoint_valid(&self, next_door_id: u64) -> bool {
        let mut ids = BTreeSet::new();
        if !self
            .region_gravity
            .iter()
            .all(|(r, g)| self.region(*r).is_some() && valid_gravity(*g))
            || !self
                .cell_gravity
                .iter()
                .all(|(at, g)| self.contains(*at) && valid_gravity(*g))
        {
            return false;
        }
        if self.physical_vertical.iter().any(|(at, d)| {
            !matches!(d, Direction::Up | Direction::Down) || self.passage(*at, *d).is_none()
        }) {
            return false;
        }
        if self.solid_boundaries.iter().any(|(at, direction)| {
            matches!(direction, Direction::Up | Direction::Down)
                || !self.passages.contains_key(&(*at, *direction))
        }) {
            return false;
        }
        if !self.absent.iter().all(|(id, region)| {
            *id == region.id && !self.regions.contains_key(id) && {
                let (x, y, z) = region.bounds.dimensions();
                x > 0 && y > 0 && z > 0
            }
        }) {
            return false;
        }
        self.named_anchors.iter().all(|(region, anchors)| {
            anchors.iter().all(|(name, position)| {
                valid_anchor_name(name)
                    && self.knows(Location {
                        region: *region,
                        position: *position,
                    })
            })
        }) && self.named_stairs.iter().all(|((from, direction), to)| {
            self.contains(*from)
                && matches!(direction, Direction::Up | Direction::Down)
                && self.knows_region(to.region)
                && valid_anchor_name(&to.name)
                && !self.passages.contains_key(&(*from, *direction))
        }) && self.regions.iter().all(|(id, region)| {
            *id == region.id && {
                let (x, y, z) = region.bounds.dimensions();
                x > 0 && y > 0 && z > 0
            }
        }) && self.doors.iter().all(|(location, door)| {
            (1..=MAX_DOOR_HEIGHT).contains(&door.height)
                && door.id > 0
                && door.id < next_door_id
                && ids.insert(door.id)
                && self.door_cells(*location).all(|cell| {
                    // Every cell exists and belongs to this door alone.
                    self.contains(cell)
                        && self.door_entry(cell).map(|(base, _)| base) == Some(*location)
                })
                && self.door_cells(*location).count() == usize::from(door.height)
        }) && self
            .secret_doors
            .iter()
            .all(|(at, concealment)| self.doors.contains_key(at) && concealment.valid())
            && {
                let mut traps = BTreeSet::new();
                self.traps.iter().all(|(at, trap)| {
                    self.contains(*at)
                        && !self.is_wall(*at)
                        && self.trap_geometry_valid(*at, trap)
                        && trap.valid()
                        && traps.insert(trap.id)
                })
            }
            && self.lighting.iter().all(|(region, light)| {
                self.region(*region).is_some()
                    && light.cells.keys().all(|position| {
                        self.contains(Location {
                            region: *region,
                            position: *position,
                        })
                    })
            })
            && self.terrain.keys().all(|location| self.contains(*location))
            && self
                .place_hints
                .keys()
                .all(|location| self.contains(*location))
            && self.chambers.keys().all(|id| self.regions.contains_key(id))
            && self.passages.iter().all(|((from, direction), passage)| {
                // A link may lead into a detached region.
                *from == passage.from
                    && *direction == passage.direction
                    && self.contains(*from)
                    && self.knows(passage.to)
            })
            && self
                .rotations
                .iter()
                .all(|(key, rotation)| *rotation < 24 && self.passages.contains_key(key))
    }

    /// The base cell and door occupying `location`, if any. Door cells differ
    /// only in z, the last key component, so one short range query finds it.
    fn door_entry(&self, location: Location) -> Option<(Location, Door)> {
        let lowest = raised(location, -i32::from(MAX_DOOR_HEIGHT - 1)).unwrap_or(Location {
            position: Position {
                z: i32::MIN,
                ..location.position
            },
            ..location
        });
        self.doors
            .range(lowest..=location)
            .next_back()
            .filter(|(base, door)| {
                i64::from(location.position.z) - i64::from(base.position.z) < i64::from(door.height)
            })
            .map(|(base, door)| (*base, *door))
    }
    /// The door occupying `location`, whichever of its cells that is.
    pub fn door(&self, location: Location) -> Option<Door> {
        self.door_entry(location).map(|(_, door)| door)
    }
    /// A door's base cell.
    pub fn door_location(&self, id: u64) -> Option<Location> {
        self.doors
            .iter()
            .find_map(|(location, door)| (door.id == id).then_some(*location))
    }
    /// The cells of the door based at `base`, from the bottom up.
    pub fn door_cells(&self, base: Location) -> impl Iterator<Item = Location> + '_ {
        let height = self.doors.get(&base).map_or(0, |door| door.height);
        (0..i32::from(height)).filter_map(move |cells| raised(base, cells))
    }
    fn door_free(&self, cell: Location) -> bool {
        self.contains(cell) && !self.is_wall(cell) && self.door(cell).is_none()
    }
    /// How tall a door based at `location` could be: the run of contained,
    /// non-solid, door-free cells straight up from it, at most
    /// [`MAX_DOOR_HEIGHT`].
    pub fn door_clearance(&self, location: Location) -> u8 {
        (0..MAX_DOOR_HEIGHT)
            .take_while(|&cells| {
                raised(location, i32::from(cells)).is_some_and(|cell| self.door_free(cell))
            })
            .count() as u8
    }
    /// Whether a door based at `base`, `height` cells tall, leaves its doorway
    /// open above it: the cell above its top is open and, like its top cell,
    /// walled on both sides along the same horizontal axis. Anything can then
    /// be seen over the door. A door in a low wall under open space isn't
    /// flagged, and a doorway at a region join, which has no walls beside it
    /// in the room, can't be judged this way.
    pub fn doorway_open_above(&self, base: Location, height: u8) -> bool {
        let walled = |cell: Location, dx: i32, dy: i32| {
            [-1, 1].into_iter().all(|sign| {
                let side = Location {
                    position: Position {
                        x: cell.position.x.saturating_add(sign * dx),
                        y: cell.position.y.saturating_add(sign * dy),
                        ..cell.position
                    },
                    ..cell
                };
                !self.contains(side) || self.is_wall(side)
            })
        };
        let (Some(top), Some(above)) = (
            raised(base, i32::from(height) - 1),
            raised(base, i32::from(height)),
        ) else {
            return false;
        };
        self.door_free(above)
            && [(1, 0), (0, 1)]
                .into_iter()
                .any(|(dx, dy)| walled(top, dx, dy) && walled(above, dx, dy))
    }
    /// Place a door `height` cells tall, whose cells must all be free.
    pub fn place_door(
        &mut self,
        location: Location,
        id: u64,
        open: bool,
        height: u8,
    ) -> Result<(), WorldError> {
        if !(1..=MAX_DOOR_HEIGHT).contains(&height)
            || self.door_clearance(location) < height
            || self.door_location(id).is_some()
        {
            return Err(WorldError::InvalidEndpoint);
        }
        self.doors.insert(location, Door { id, open, height });
        self.sight.region_changed(location.region);
        Ok(())
    }
    /// Open or close the door occupying `location`.
    ///
    /// Any cell occupied by the door identifies the same base entity. This
    /// changes the barrier and invalidates its region's sight witness; it does
    /// not check an actor's reach or charge an action cost.
    ///
    /// # Panics
    ///
    /// Panics if no door occupies `location`. Check [`World::door`] first when
    /// the caller has not already established this precondition.
    pub fn set_door(&mut self, location: Location, open: bool) {
        let (base, _) = self.door_entry(location).expect("validated door");
        self.doors.get_mut(&base).expect("validated door").open = open;
        self.sight.region_changed(base.region);
    }
    pub fn opaque(&self, location: Location) -> bool {
        self.is_wall(location) || self.door(location).is_some_and(|d| !d.open)
    }
    /// The perceived adjacent cell, including a closed barrier at the destination.
    pub fn adjacent(&self, from: Location, direction: Direction) -> Option<Location> {
        if self.named_stairs.contains_key(&(from, direction)) {
            return self
                .walkable(from)
                .then(|| self.link_destination(from, direction))
                .flatten();
        }
        if direction.components().is_some() {
            return self
                .diagonal_reach(from, direction, |_| true)
                .map(|(to, _)| to);
        }
        self.sight_step(from, direction).map(|(to, _)| to)
    }

    /// Construct ordinary empty-terrain regions and directed zero-rotation passages.
    ///
    /// Region metadata is inserted as supplied; the caller must provide valid
    /// bounds and identities. Use [`World::add_chamber`] instead when an interior
    /// needs a finite stone shell. Each passage is checked by [`World::connect`].
    ///
    /// # Errors
    ///
    /// Returns [`WorldError::DuplicateRegion`] for repeated region IDs, or the
    /// first passage's connection error. An error returns no partial world.
    pub fn new(regions: Vec<Region>, passages: Vec<Passage>) -> Result<Self, WorldError> {
        let mut world = Self {
            doors: Shared::new(BTreeMap::new()),
            secret_doors: Shared::default(),
            traps: Shared::default(),
            regions: Shared::new(BTreeMap::new()),
            passages: Shared::new(BTreeMap::new()),
            rotations: Shared::new(BTreeMap::new()),
            physical_vertical: Shared::default(),
            solid_boundaries: Shared::default(),
            named_stairs: Shared::default(),
            named_anchors: Shared::default(),
            region_gravity: Shared::default(),
            cell_gravity: Shared::default(),
            terrain: Shared::new(BTreeMap::new()),
            lighting: Shared::default(),
            chambers: Shared::new(BTreeMap::new()),
            place_hints: Shared::new(BTreeMap::new()),
            absent: Shared::default(),
            sight: Default::default(),
        };
        for region in regions {
            if world.regions.insert(region.id, region).is_some() {
                return Err(WorldError::DuplicateRegion);
            }
        }
        for passage in passages {
            world.connect(passage, 0)?;
        }
        Ok(world)
    }

    /// Insert validated metadata for a new loaded, empty-terrain region.
    ///
    /// The duplicate check covers loaded IDs only. A known absent ID is accepted
    /// without removing its retained absent metadata, so this primitive is not a
    /// complete residency/replacement transaction. The simulation/host owns that
    /// reconciliation; use [`World::attach_region`] to restore a region payload.
    /// This helper does not validate deserialized bounds or allocate identities.
    /// Successful insertion invalidates derived topology witnesses.
    ///
    /// # Errors
    ///
    /// Returns [`WorldError::DuplicateRegion`] if the identity is already loaded,
    /// leaving the world unchanged.
    pub fn add_region(&mut self, region: Region) -> Result<(), WorldError> {
        if self.regions.contains_key(&region.id) {
            return Err(WorldError::DuplicateRegion);
        }
        self.regions.insert(region.id, region);
        self.sight.topology_changed();
        Ok(())
    }

    /// Allocate finite stone and carve the requested interior. Joins override
    /// adjacent shell cells, so a storage partition does not create a barrier.
    ///
    /// `region.bounds` describes the interior on input. Registered metadata has
    /// one additional storage cell on every face, without translating positions;
    /// the shell defaults to stone and the interior to empty terrain. The same
    /// metadata and residency qualifications as [`World::add_region`] apply.
    ///
    /// # Errors
    ///
    /// Returns [`WorldError::InvalidEndpoint`] if shell bounds overflow, or
    /// [`WorldError::DuplicateRegion`] if the identity is already loaded.
    /// Rejection changes nothing.
    pub fn add_chamber(&mut self, mut region: Region) -> Result<(), WorldError> {
        let interior = region.bounds;
        region.bounds = interior.with_shell().ok_or(WorldError::InvalidEndpoint)?;
        let id = region.id;
        self.add_region(region)?;
        self.chambers.insert(id, interior);
        self.sight.topology_changed();
        Ok(())
    }

    /// Convex ordinary space with no internal blocker. Only this region's
    /// overrides are inspected; chamber shells are outside the returned box.
    /// A closed door conservatively disables the proof for the entire region.
    pub(crate) fn transparent_box(&self, id: RegionId) -> Option<Extent> {
        let bounds = *self.chambers.get(&id).unwrap_or(&self.region(id)?.bounds);
        let endpoint = |value| Location {
            region: id,
            position: Position {
                x: value,
                y: value,
                z: value,
            },
        };
        let range = endpoint(i32::MIN)..=endpoint(i32::MAX);
        if self.terrain.range(range.clone()).any(|(at, terrain)| {
            bounds.contains(at.position) && matches!(terrain, Terrain::Solid(_))
        }) || self.doors.range(range).any(|(_, door)| !door.open)
        {
            return None;
        }
        Some(bounds)
    }

    pub fn terrain(&self, location: Location) -> Option<Terrain> {
        if !self.contains(location) {
            return None;
        }
        Some(self.terrain.get(&location).copied().unwrap_or_else(|| {
            if self
                .chambers
                .get(&location.region)
                .is_some_and(|bounds| !bounds.contains(location.position))
            {
                Terrain::Solid(Material::Stone)
            } else {
                Terrain::Empty
            }
        }))
    }

    pub fn is_chamber(&self, region: RegionId) -> bool {
        self.chambers.contains_key(&region)
    }

    pub(crate) fn within_aperture_bounds(&self, location: Location) -> bool {
        self.chambers
            .get(&location.region)
            .or_else(|| self.region(location.region).map(|r| &r.bounds))
            .is_some_and(|bounds| bounds.contains(location.position))
    }

    /// Connect one directed exit using a proper cube-rotation ID in `0..24`.
    ///
    /// The historical parameter name `quarter_turns` denotes a full rotation
    /// ID: only IDs `0..4` are planar quarter turns about z. The transform maps
    /// a continuing source-frame vector into the destination frame; see
    /// [`crate::rotate_vector`] and [`crate::compose_rotation`]. Callers must
    /// explicitly construct and validate reverse links.
    ///
    /// # Errors
    ///
    /// Rejects unsupported rotations, noncardinal directions, invalid or
    /// nonwalkable endpoints, horizontal exits that do not leave a boundary,
    /// and an already occupied exit. A rejected connection leaves the world
    /// unchanged.
    pub fn connect(&mut self, passage: Passage, quarter_turns: u8) -> Result<(), WorldError> {
        self.connect_checked(passage, quarter_turns, false)
    }

    fn connect_checked(
        &mut self,
        passage: Passage,
        quarter_turns: u8,
        solid_boundary: bool,
    ) -> Result<(), WorldError> {
        if passage.direction.components().is_some() {
            return Err(WorldError::InvalidEndpoint);
        }
        if quarter_turns >= 24 {
            return Err(WorldError::InvalidRotation);
        }
        if !(if solid_boundary {
            self.within_aperture_bounds(passage.from)
                && self.within_aperture_bounds(passage.to)
                && !matches!(passage.direction, Direction::Up | Direction::Down)
        } else {
            self.walkable(passage.from) && self.walkable(passage.to)
        }) {
            return Err(WorldError::InvalidEndpoint);
        }
        if !matches!(passage.direction, Direction::Up | Direction::Down)
            && passage
                .direction
                .offset(passage.from.position)
                .is_some_and(|position| {
                    self.chambers
                        .get(&passage.from.region)
                        .or_else(|| self.region(passage.from.region).map(|r| &r.bounds))
                        .is_some_and(|bounds| bounds.contains(position))
                })
        {
            return Err(WorldError::NotBoundaryExit);
        }
        let key = (passage.from, passage.direction);
        if self.passages.contains_key(&key) || self.named_stairs.contains_key(&key) {
            return Err(WorldError::DuplicateExit);
        }
        self.passages.insert(key, passage);
        if solid_boundary {
            self.solid_boundaries.insert(key);
        }
        self.rotations.insert(key, quarter_turns);
        self.sight.topology_changed();
        Ok(())
    }

    /// Atomically glue a rectangular aperture with a single affine transform.
    /// Width follows +y for east/west faces and +x otherwise; height follows
    /// +z for horizontal exits and +y for vertical exits. Destination offsets
    /// are rotated by the same transform as the continuing direction.
    pub fn connect_area(
        &mut self,
        anchor: Passage,
        turns: u8,
        width: u16,
        height: u16,
    ) -> Result<(), WorldError> {
        self.connect_area_checked(anchor, turns, width, height, false)
    }

    /// Glue horizontal region faces without changing solid terrain. Excavation
    /// can subsequently open any part of the join. Ordinary transport links
    /// continue to require traversable endpoints.
    pub fn connect_boundary_area(
        &mut self,
        anchor: Passage,
        turns: u8,
        width: u16,
        height: u16,
    ) -> Result<(), WorldError> {
        self.connect_area_checked(anchor, turns, width, height, true)
    }

    fn connect_area_checked(
        &mut self,
        anchor: Passage,
        turns: u8,
        width: u16,
        height: u16,
        solid_boundary: bool,
    ) -> Result<(), WorldError> {
        if width == 0 || height == 0 || u32::from(width) * u32::from(height) > 1024 {
            return Err(WorldError::InvalidEndpoint);
        }
        let mut candidate = self.clone();
        for u in 0..i32::from(width) {
            for v in 0..i32::from(height) {
                let (x, y, z) = match anchor.direction {
                    Direction::East | Direction::West => (0, u, v),
                    Direction::North | Direction::South => (u, 0, v),
                    Direction::Up | Direction::Down => (u, v, 0),
                    _ => return Err(WorldError::InvalidEndpoint),
                };
                if turns >= 24 {
                    return Err(WorldError::InvalidRotation);
                }
                let [rx, ry, rz] =
                    crate::rotate_vector(turns, [i64::from(x), i64::from(y), i64::from(z)])
                        .map(|v| v as i32);
                let offset =
                    |location: Location, x: i32, y: i32, z: i32| -> Result<Location, WorldError> {
                        Ok(Location {
                            position: Position {
                                x: location
                                    .position
                                    .x
                                    .checked_add(x)
                                    .ok_or(WorldError::InvalidEndpoint)?,
                                y: location
                                    .position
                                    .y
                                    .checked_add(y)
                                    .ok_or(WorldError::InvalidEndpoint)?,
                                z: location
                                    .position
                                    .z
                                    .checked_add(z)
                                    .ok_or(WorldError::InvalidEndpoint)?,
                            },
                            ..location
                        })
                    };
                candidate.connect_checked(
                    Passage {
                        from: offset(anchor.from, x, y, z)?,
                        direction: anchor.direction,
                        to: offset(anchor.to, rx, ry, rz)?,
                    },
                    turns,
                    solid_boundary,
                )?;
            }
        }
        *self = candidate;
        Ok(())
    }

    /// Physical apertures are distinct from explicit stair traversal links.
    pub fn connect_portal_area(
        &mut self,
        anchor: Passage,
        turns: u8,
        width: u16,
        height: u16,
    ) -> Result<(), WorldError> {
        if anchor
            .direction
            .offset(anchor.from.position)
            .is_some_and(|position| {
                self.within_aperture_bounds(Location {
                    position,
                    ..anchor.from
                })
            })
        {
            return Err(WorldError::NotBoundaryExit);
        }
        let mut candidate = self.clone();
        candidate.connect_area(anchor, turns, width, height)?;
        if matches!(anchor.direction, Direction::Up | Direction::Down) {
            for u in 0..i32::from(width) {
                for v in 0..i32::from(height) {
                    let mut at = anchor.from;
                    at.position.x += u;
                    at.position.y += v;
                    candidate.physical_vertical.insert((at, anchor.direction));
                    candidate.sight.topology_changed();
                }
            }
        }
        *self = candidate;
        Ok(())
    }
    pub fn physics_neighbor(&self, from: Location, direction: Direction) -> Option<(Location, u8)> {
        if !matches!(direction, Direction::Up | Direction::Down)
            || self.physical_vertical.contains(&(from, direction))
        {
            return self.movement_neighbor(from, direction);
        }
        let to = Location {
            position: direction.offset(from.position)?,
            ..from
        };
        self.contains(to).then_some((to, 0))
    }
    /// Physical geometry only, independent of source/door opacity. Transport
    /// stairs remain nonphysical: vertical fallback stays in this region.
    /// Construction-face certification uses this, not movement availability.
    pub fn physical_geometry_neighbor(
        &self,
        from: Location,
        direction: Direction,
    ) -> Option<(Location, u8)> {
        if direction.components().is_some() || !self.contains(from) {
            return None;
        }
        if !matches!(direction, Direction::Up | Direction::Down)
            || self.physical_vertical.contains(&(from, direction))
        {
            return self.geometry_step(from, direction);
        }
        let to = Location {
            position: direction.offset(from.position)?,
            ..from
        };
        self.contains(to).then_some((to, 0))
    }
    pub fn is_stair(&self, from: Location, direction: Direction) -> bool {
        matches!(direction, Direction::Up | Direction::Down)
            && (self.passage(from, direction).is_some()
                || self.named_stairs.contains_key(&(from, direction)))
            && !self.physical_vertical.contains(&(from, direction))
    }

    /// Immutable sparse topology only, without destination or navigation lookup.
    ///
    /// Yields `(source, is_logical_stair)` for every named stair, then every
    /// ordinary passage owned by `region`. Each group is in source/direction
    /// order; their concatenation is not a single globally sorted sequence.
    /// Multiple exits at one source remain separate entries. The boolean is
    /// false for horizontal passages and physical vertical joins.
    pub fn stair_candidates_in_region(
        &self,
        region: RegionId,
    ) -> impl Iterator<Item = (Location, bool)> + '_ {
        let first = Location {
            region,
            position: Position {
                x: i32::MIN,
                y: i32::MIN,
                z: i32::MIN,
            },
        };
        self.named_stairs
            .range((first, Direction::North)..)
            .take_while(move |((at, _), _)| at.region == region)
            .map(|((at, _), _)| (*at, true))
            .chain(
                self.passages
                    .range((first, Direction::North)..)
                    .take_while(move |((at, _), _)| at.region == region)
                    .map(|((at, direction), _)| (*at, self.is_stair(*at, *direction))),
            )
    }
    /// Sparse physical doorway positions, independent of concealment knowledge.
    pub fn doors_in_region(&self, region: RegionId) -> impl Iterator<Item = Location> + '_ {
        let first = Location {
            region,
            position: Position {
                x: i32::MIN,
                y: i32::MIN,
                z: i32::MIN,
            },
        };
        self.doors
            .range(first..)
            .take_while(move |(at, _)| at.region == region)
            .map(|(at, _)| *at)
    }

    /// Known owner of a stair landing, even before its coordinates exist.
    ///
    /// Returns `None` when this exit is not a logical up/down stair. A returned
    /// identity does not imply that the landing has resolved coordinates or
    /// loaded terrain. Physical vertical joins are not logical stairs.
    pub fn stair_region(&self, from: Location, direction: Direction) -> Option<RegionId> {
        if !self.is_stair(from, direction) {
            return None;
        }
        self.named_stairs
            .get(&(from, direction))
            .map(|anchor| anchor.region)
            .or_else(|| {
                self.passage(from, direction)
                    .map(|passage| passage.to.region)
            })
    }

    /// Resolve an explicit passage or a named stair with published coordinates.
    ///
    /// Returns `None` for an ordinary unlinked neighbor or unresolved anchor.
    /// Does not require the source or destination to be currently walkable, nor
    /// the destination region to be loaded. Use [`World::step`] for a walkable
    /// point destination; this lookup alone is not movement permission.
    pub fn link_destination(&self, from: Location, direction: Direction) -> Option<Location> {
        if let Some(passage) = self.passage(from, direction) {
            return Some(passage.to);
        }
        let anchor = self.named_stairs.get(&(from, direction))?;
        Some(Location {
            region: anchor.region,
            position: *self.named_anchors.get(&anchor.region)?.get(&anchor.name)?,
        })
    }

    pub fn crossing_rotation(&self, from: Location, direction: Direction) -> u8 {
        if direction.components().is_some() {
            return self
                .diagonal_reach(from, direction, |_| true)
                .map_or(0, |(_, turns)| turns);
        }
        self.rotations.get(&(from, direction)).copied().unwrap_or(0)
    }

    pub fn set_wall(&mut self, location: Location, wall: bool) -> Result<(), WorldError> {
        if !self.contains(location) || (wall && self.door(location).is_some()) {
            return Err(WorldError::InvalidEndpoint);
        }
        self.terrain.insert(
            location,
            if wall {
                Terrain::Solid(Material::Stone)
            } else {
                Terrain::Empty
            },
        );
        self.sight.region_changed(location.region);
        Ok(())
    }

    /// Unnamed spatial anchors, independent of region boundaries and topology.
    /// Terrain edits retain authored hints; solid cells suppress their disclosure.
    pub fn set_place_hint(&mut self, location: Location, present: bool) -> Result<(), WorldError> {
        if !self.contains(location) {
            return Err(WorldError::InvalidEndpoint);
        }
        if present {
            // An existing hint keeps its authored name.
            self.place_hints.entry(location).or_default();
        } else {
            self.place_hints.remove(&location);
        }
        Ok(())
    }

    /// A place hint with an authored name the character learns on seeing it.
    pub fn set_named_place_hint(
        &mut self,
        location: Location,
        name: &str,
    ) -> Result<(), WorldError> {
        if !self.contains(location) {
            return Err(WorldError::InvalidEndpoint);
        }
        self.place_hints.insert(location, name.to_owned());
        Ok(())
    }

    pub fn has_place_hint(&self, location: Location) -> bool {
        self.place_hints.contains_key(&location) && self.walkable(location)
    }

    /// The authored name of the place hint here, if it has one.
    pub fn place_name(&self, location: Location) -> Option<&str> {
        self.place_hints
            .get(&location)
            .map(String::as_str)
            .filter(|name| !name.is_empty())
    }

    pub fn is_wall(&self, location: Location) -> bool {
        matches!(self.terrain(location), Some(Terrain::Solid(_)))
    }

    pub fn walkable(&self, location: Location) -> bool {
        self.contains(location) && !self.opaque(location)
    }

    pub fn passage(&self, from: Location, direction: Direction) -> Option<&Passage> {
        self.passages.get(&(from, direction))
    }

    pub(crate) fn sight_step(
        &self,
        from: Location,
        direction: Direction,
    ) -> Option<(Location, u8)> {
        if !self.walkable(from) {
            return None;
        }
        self.geometry_step(from, direction)
    }

    /// Topology only: opaque cells still have geometric neighbors.
    pub(crate) fn geometry_step(
        &self,
        from: Location,
        direction: Direction,
    ) -> Option<(Location, u8)> {
        if let Some(passage) = self.passage(from, direction) {
            return Some((passage.to, self.rotations[&(from, direction)]));
        }
        if self.chambers.contains_key(&from.region) {
            return self.rim_step(from, direction);
        }
        let to = Location {
            position: direction.offset(from.position)?,
            ..from
        };
        self.contains(to).then_some((to, 0))
    }

    /// Resolve a consistent transform across the solid portion of a chamber
    /// face. A solid/floor rim is as important as a solid/solid rim: the former
    /// occurs at narrow doors. This never creates an unadvertised floor/floor
    /// opening. Several incompatible transforms leave the geometry unresolved.
    fn rim_step(&self, from: Location, direction: Direction) -> Option<(Location, u8)> {
        let mut projected = None;
        for passage in self.exits(from.region).filter(|p| p.direction == direction) {
            let a = passage.from.position;
            let b = from.position;
            let same_plane = match direction {
                Direction::East | Direction::West => a.x == b.x,
                Direction::North | Direction::South => a.y == b.y,
                Direction::Up | Direction::Down => a.z == b.z,
                _ => return None,
            };
            if !same_plane {
                continue;
            }
            let dx = i64::from(b.x) - i64::from(a.x);
            let dy = i64::from(b.y) - i64::from(a.y);
            let dz = i64::from(b.z) - i64::from(a.z);
            let turns = self.crossing_rotation(passage.from, direction);
            let [dx, dy, dz] = crate::rotate_vector(turns, [dx, dy, dz]);
            let to = Location {
                region: passage.to.region,
                position: Position {
                    x: i32::try_from(i64::from(passage.to.position.x) + dx).ok()?,
                    y: i32::try_from(i64::from(passage.to.position.y) + dy).ok()?,
                    z: i32::try_from(i64::from(passage.to.position.z) + dz).ok()?,
                },
            };
            let candidate = (to, turns);
            if projected.is_some_and(|old| old != candidate) {
                return None;
            }
            projected = Some(candidate);
        }
        if let Some((to, turns)) = projected {
            if self.contains(to) && (self.is_wall(from) || self.is_wall(to)) {
                return Some((to, turns));
            }
        }
        let to = Location {
            position: direction.offset(from.position)?,
            ..from
        };
        self.contains(to).then_some((to, 0))
    }

    pub fn gravity(&self, at: Location) -> Option<[i32; 3]> {
        self.cell_gravity
            .get(&at)
            .or_else(|| self.region_gravity.get(&at.region))
            .copied()
    }
    pub fn has_gravity(&self) -> bool {
        !self.region_gravity.is_empty() || !self.cell_gravity.is_empty()
    }
    pub fn set_gravity(&mut self, region: RegionId, vector: [i32; 3]) -> Result<(), WorldError> {
        if self.region(region).is_none() || !valid_gravity(vector) {
            return Err(WorldError::InvalidEndpoint);
        }
        self.region_gravity.insert(region, vector);
        Ok(())
    }
    pub fn set_cell_gravity(&mut self, at: Location, vector: [i32; 3]) -> Result<(), WorldError> {
        if !self.walkable(at) || !valid_gravity(vector) {
            return Err(WorldError::InvalidEndpoint);
        }
        self.cell_gravity.insert(at, vector);
        Ok(())
    }
    /// Return metadata only when the region's geometry is loaded.
    ///
    /// Use [`World::known_region`] when metadata must survive detachment or
    /// precede initial construction.
    pub fn region(&self, id: RegionId) -> Option<&Region> {
        self.regions.get(&id)
    }

    /// Whether a cell lies inside a currently loaded region's storage bounds.
    ///
    /// Solid terrain and closed doors still satisfy this test. It does not check
    /// movement, visibility or actor occupancy; [`World::knows`] also admits
    /// coordinates in known absent regions.
    pub fn contains(&self, location: Location) -> bool {
        self.region(location.region)
            .is_some_and(|region| region.bounds.contains(location.position))
    }

    /// Ordinary passages owned by a loaded source region, in source/direction order.
    ///
    /// Named stairs are stored separately and are not yielded. Destination
    /// regions may be absent; detaching the source moves its passages into the
    /// [`RegionSlice`]. No destination walkability or actor permission is checked.
    pub fn exits(&self, region: RegionId) -> impl Iterator<Item = &Passage> {
        let start = Location {
            region,
            position: Position {
                x: i32::MIN,
                y: i32::MIN,
                z: i32::MIN,
            },
        };
        let end = Location {
            region,
            position: Position {
                x: i32::MAX,
                y: i32::MAX,
                z: i32::MAX,
            },
        };
        self.passages
            .range((start, Direction::North)..=(end, Direction::Down))
            .map(|(_, passage)| passage)
    }

    /// Actual movement topology, excluding sight-only material rim projection.
    ///
    /// Returns the destination and crossing rotation for a cardinal direction
    /// from a walkable source, including explicit stairs. The destination still
    /// needs terrain/body/occupancy validation by the caller; a resolved link
    /// alone is not movement permission. Use [`World::diagonal_reach`] for
    /// diagonals, or [`World::step`] for a walkable point destination.
    pub fn movement_neighbor(
        &self,
        from: Location,
        direction: Direction,
    ) -> Option<(Location, u8)> {
        if direction.components().is_some() || !self.walkable(from) {
            return None;
        }
        if let Some(passage) = self.passage(from, direction) {
            return Some((passage.to, self.crossing_rotation(from, direction)));
        }
        if let Some(anchor) = self.named_stairs.get(&(from, direction)) {
            let position = *self.named_anchors.get(&anchor.region)?.get(&anchor.name)?;
            return Some((
                Location {
                    region: anchor.region,
                    position,
                },
                0,
            ));
        }
        let to = Location {
            position: direction.offset(from.position)?,
            ..from
        };
        self.contains(to).then_some((to, 0))
    }

    /// Resolve diagonal reach through at least one clear side. Destination may
    /// contain a closed door; callers validate destination occupancy separately.
    pub fn diagonal_reach(
        &self,
        from: Location,
        direction: Direction,
        clear: impl Fn(Location) -> bool,
    ) -> Option<(Location, u8)> {
        let (a, b) = direction.components()?;
        let route = |first, second: Direction| {
            let (side, r1) = self.movement_neighbor(from, first)?;
            if !self.walkable(side) || !clear(side) {
                return None;
            }
            let (to, r2) = self.movement_neighbor(side, second.rotated(r1))?;
            if self.is_wall(to) {
                return None;
            }
            Some((to, crate::compose_rotation(r1, r2)))
        };
        match (route(a, b), route(b, a)) {
            (Some(a), Some(b)) if a == b => Some(a),
            (Some(a), None) | (None, Some(a)) => Some(a),
            _ => None,
        }
    }

    /// Resolve geometry only. Occupancy and action costs belong to simulation.
    pub fn step(&self, from: Location, direction: Direction) -> Option<Location> {
        let to = self.adjacent(from, direction)?;
        self.walkable(to).then_some(to)
    }
}

#[cfg(test)]
mod sharing_tests {
    use super::*;
    #[test]
    fn named_stair_resolves_after_destination_generation_and_survives_detach() {
        let region = |id| Region {
            id: RegionId(id),
            name: format!("floor-{id}"),
            bounds: Extent::new(3, 3, 2).unwrap(),
        };
        let from = Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        };
        let to = Location {
            region: RegionId(2),
            position: Position { x: 2, y: 1, z: 0 },
        };
        let mut world = World::new(vec![region(1)], vec![]).unwrap();
        world.add_unbuilt_region(region(2), false).unwrap();
        world
            .connect_named_stair(
                from,
                Direction::Down,
                NamedAnchor {
                    region: RegionId(2),
                    name: "up".into(),
                },
            )
            .unwrap();
        assert!(world.is_stair(from, Direction::Down));
        assert_eq!(
            world.linked_regions(RegionId(1)),
            BTreeSet::from([RegionId(2)])
        );
        assert_eq!(world.movement_neighbor(from, Direction::Down), None);
        let slice = world.detach_region(RegionId(1)).unwrap();
        let mut generated = World::new(vec![region(2)], vec![]).unwrap();
        generated
            .register_named_anchors(RegionId(2), BTreeMap::from([("up".into(), to.position)]))
            .unwrap();
        let target = generated.detach_region(RegionId(2)).unwrap();
        world.publish_region_anchors(&target).unwrap();
        world.attach_region(slice).unwrap();
        assert_eq!(
            world.movement_neighbor(from, Direction::Down),
            Some((to, 0))
        );
        assert!(!world.contains(to));
        world.attach_region(target).unwrap();
        assert!(world.checkpoint_valid(1));
        world.detach_region(RegionId(2)).unwrap();
        assert_eq!(
            world.movement_neighbor(from, Direction::Down),
            Some((to, 0))
        );
    }
    #[test]
    fn solid_boundary_join_preserves_terrain_until_excavated() {
        let regions = (1..=2)
            .map(|id| Region {
                id: RegionId(id),
                name: format!("slot-{id}"),
                bounds: Extent::new(3, 3, 2).unwrap(),
            })
            .collect();
        let mut world = World::new(regions, vec![]).unwrap();
        let from = Location {
            region: RegionId(1),
            position: Position { x: 2, y: 0, z: 0 },
        };
        let to = Location {
            region: RegionId(2),
            position: Position { x: 0, y: 0, z: 0 },
        };
        world.set_wall(from, true).unwrap();
        world.set_wall(to, true).unwrap();
        world
            .connect_boundary_area(
                Passage {
                    from,
                    direction: Direction::East,
                    to,
                },
                0,
                3,
                2,
            )
            .unwrap();
        assert!(!world.walkable(from));
        assert!(!world.walkable(to));
        assert!(world.authored_links_clear());
        assert_eq!(world.physics_neighbor(from, Direction::East), None);
        world.set_wall(from, false).unwrap();
        world.set_wall(to, false).unwrap();
        assert!(world.walkable(from) && world.walkable(to));
        assert_eq!(world.physics_neighbor(from, Direction::East), Some((to, 0)));
    }
    #[test]
    fn door_edits_share_large_geometry_and_keep_the_previous_boundary_unchanged() {
        let regions = (1..=256)
            .map(|id| Region {
                id: RegionId(id),
                name: format!("room-{id}"),
                bounds: Extent::new(9, 9, 2).unwrap(),
            })
            .collect();
        let mut world = World::new(regions, vec![]).unwrap();
        let at = Location {
            region: RegionId(1),
            position: Position { x: 2, y: 2, z: 0 },
        };
        world.place_door(at, 1, false, 2).unwrap();
        let original = world.clone();
        world.set_door(at, true);
        assert!(!original.door(at).unwrap().open);
        assert!(world.door(at).unwrap().open);
        assert!(!world.doors.shares_storage(&original.doors));
        assert!(world.regions.shares_storage(&original.regions));
        assert!(world.passages.shares_storage(&original.passages));
        assert!(world.terrain.shares_storage(&original.terrain));
        assert!(world.rotations.shares_storage(&original.rotations));
    }
}

fn valid_gravity(g: [i32; 3]) -> bool {
    g.iter().filter(|v| **v != 0).count() <= 1 && g.iter().all(|v| (-1024..=1024).contains(v))
}

#[cfg(test)]
mod gravity_sharing_tests {
    use super::*;
    #[test]
    fn gravity_tables_share_geometry_and_detach_only_on_edit() {
        let region = RegionId(1);
        let mut original = World::new(
            vec![Region {
                id: region,
                name: "field".into(),
                bounds: Extent::new(2, 2, 2).unwrap(),
            }],
            vec![],
        )
        .unwrap();
        let at = Location {
            region,
            position: Position { x: 0, y: 0, z: 0 },
        };
        original.set_gravity(region, [0, 0, -1]).unwrap();
        original.set_cell_gravity(at, [0; 3]).unwrap();
        let mut edited = original.clone();
        assert!(edited
            .region_gravity
            .shares_storage(&original.region_gravity));
        assert!(edited.cell_gravity.shares_storage(&original.cell_gravity));
        edited.set_cell_gravity(at, [0, 0, 1]).unwrap();
        assert_eq!(original.gravity(at), Some([0; 3]));
        assert_eq!(edited.gravity(at), Some([0, 0, 1]));
        assert!(edited
            .region_gravity
            .shares_storage(&original.region_gravity));
    }
}

#[path = "concealed.rs"]
mod concealed;
pub use concealed::{ConcealedFeatureId, Concealment, Trap, TrapFamily, TrapKind};

#[cfg(test)]
mod historical_structure_tests {
    use super::*;
    fn fixture() -> World {
        World::new(
            vec![Region {
                id: RegionId(1),
                name: "history".into(),
                bounds: Extent::new(3, 3, 2).unwrap(),
            }],
            vec![],
        )
        .unwrap()
    }
    fn at(x: i32) -> Location {
        Location {
            region: RegionId(1),
            position: Position { x, y: 1, z: 0 },
        }
    }
    #[test]
    fn historical_structure_ignores_only_current_allocator_bound() {
        let mut world = fixture();
        world.place_door(at(1), 42, true, 1).unwrap();
        assert!(!world.checkpoint_valid(1));
        assert!(world.historical_checkpoint_valid());
        world.doors.insert(
            at(0),
            Door {
                id: 42,
                open: true,
                height: 1,
            },
        );
        assert!(!world.historical_checkpoint_valid());
    }
    #[test]
    fn lawful_terrain_and_gravity_edits_are_structural_not_initial_source_equality() {
        let mut world = fixture();
        world.set_wall(at(1), true).unwrap();
        world.set_gravity(RegionId(1), [0, 0, 1]).unwrap();
        assert!(world.historical_checkpoint_valid());
        world.region_gravity.insert(RegionId(999), [0, 0, -1]);
        assert!(!world.historical_checkpoint_valid());
    }
}
