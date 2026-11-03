//! One region's share of the world, detached for region streaming. A detached
//! region keeps its metadata in the world, so references into it stay
//! checkable, while its terrain, doors and outgoing links live in the slice.
//! See `docs/architecture/streaming.md#region-lifecycle-contract`.
use super::*;
use serde::{Deserialize, Serialize};

/// One region's physical payload for detachment, generation or reattachment.
///
/// Links *into* the region stay with their source regions; links *out of* it
/// travel with the slice. The world retains known metadata and published anchor
/// coordinates while the payload is absent. This value contains no simulation
/// actors, activity clocks or host persistence barrier.
///
/// Deserialization alone is not validation or trusted-source identity. See
/// [`Self::valid`] and [`World::attach_region`] for the local validation boundary.
///
/// ```
/// use std::collections::BTreeSet;
/// use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, RegionSlice, World};
/// let regions = [1, 2].map(|id| Region {
///     id: RegionId(id), name: format!("Room {id}"),
///     bounds: Extent::new(3, 3, 2).unwrap(),
/// });
/// let left = Location { region: RegionId(1), position: Position { x: 2, y: 1, z: 0 } };
/// let right = Location { region: RegionId(2), position: Position { x: 0, y: 1, z: 0 } };
/// let mut world = World::new(regions.into(), vec![
///     Passage { from: left, direction: Direction::East, to: right },
///     Passage { from: right, direction: Direction::West, to: left },
/// ]).unwrap();
/// world.place_door(right, 7, false, 1).unwrap();
/// let before = world.clone();
/// let slice = world.detach_region(RegionId(2)).unwrap();
/// assert!(slice.valid());
/// assert!(world.knows(right));
/// assert!(!world.contains(right));
/// assert!(world.region(RegionId(2)).is_none());
/// assert_eq!(world.known_region(RegionId(2)), Some(slice.region()));
/// assert_eq!(world.linked_regions(RegionId(1)), BTreeSet::from([RegionId(2)]));
/// assert!(world.linked_regions(RegionId(2)).is_empty());
/// assert_eq!(slice.linked_regions(), BTreeSet::from([RegionId(1)]));
/// assert_eq!(world.link_destination(left, Direction::East), Some(right));
/// assert_eq!(world.step(left, Direction::East), None);
///
/// let encoded = serde_json::to_vec(&slice).unwrap();
/// let decoded: RegionSlice = serde_json::from_slice(&encoded).unwrap();
/// world.attach_region(decoded).unwrap();
/// assert_eq!(world, before);
/// world.set_door(right, true);
/// assert_eq!(world.step(left, Direction::East), Some(right));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionSlice {
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        with = "crate::checkpoint_map"
    )]
    named_stairs: BTreeMap<(Location, Direction), NamedAnchor>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    named_anchors: BTreeMap<String, Position>,
    region: Region,
    chamber: Option<Extent>,
    gravity: Option<[i32; 3]>,
    #[serde(with = "crate::checkpoint_map")]
    doors: BTreeMap<Location, Door>,
    #[serde(with = "crate::checkpoint_map")]
    secret_doors: BTreeMap<Location, Concealment>,
    #[serde(with = "crate::checkpoint_map")]
    traps: BTreeMap<Location, Trap>,
    /// Outgoing links with their crossing rotations.
    #[serde(with = "crate::checkpoint_map")]
    passages: BTreeMap<(Location, Direction), (Passage, u8)>,
    physical_vertical: BTreeSet<(Location, Direction)>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    solid_boundaries: BTreeSet<(Location, Direction)>,
    #[serde(with = "crate::checkpoint_map")]
    cell_gravity: BTreeMap<Location, [i32; 3]>,
    #[serde(with = "crate::checkpoint_map")]
    terrain: BTreeMap<Location, Terrain>,
    lighting: Option<crate::lighting::RegionLight>,
    #[serde(with = "crate::checkpoint_map")]
    place_hints: BTreeMap<Location, String>,
}

impl RegionSlice {
    /// The region identity carried by this payload's metadata.
    pub fn id(&self) -> RegionId {
        self.region.id
    }
    /// Borrow the metadata that must match the receiving world's known region.
    pub fn region(&self) -> &Region {
        &self.region
    }
    /// Distinct destinations of outgoing passages/named stairs, in region-ID order.
    ///
    /// Includes unresolved named stairs. This is a structural dependency list,
    /// not a claim that their landing coordinates or terrain are available.
    pub fn linked_regions(&self) -> BTreeSet<RegionId> {
        self.passages
            .values()
            .map(|(p, _)| p.to.region)
            .chain(self.named_stairs.values().map(|to| to.region))
            .collect()
    }
    /// Door identities in base-location order, not numeric identity order.
    pub fn door_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.doors.values().map(|door| door.id)
    }
    /// Immutable feature payloads, for content-aware restore validation.
    ///
    /// Iterates in trap-location order. No actor-specific concealment filtering
    /// occurs here; a region slice is backend state.
    pub fn traps(&self) -> impl Iterator<Item = &Trap> {
        self.traps.values()
    }
    /// Trap identities in location order, not numeric identity order.
    pub fn trap_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.traps.values().map(|trap| trap.id)
    }
    /// Check the local structural constraints used by region attachment.
    ///
    /// This does not consult other regions, prove trusted-source identity or
    /// replace [`World::checkpoint_valid`]. That world-level check covers
    /// additional cross-cell and allocator invariants. The simulation/host also
    /// owns content membership, identity directories and lifecycle validation.
    pub fn valid(&self) -> bool {
        let id = self.region.id;
        let inside = |at: &Location| at.region == id && self.region.bounds.contains(at.position);
        let solid = |at: Location| {
            self.terrain.get(&at).copied().unwrap_or_else(|| {
                if self
                    .chamber
                    .is_some_and(|interior| !interior.contains(at.position))
                {
                    Terrain::Solid(crate::Material::Stone)
                } else {
                    Terrain::Empty
                }
            }) != Terrain::Empty
        };
        let supported = |at: Location| {
            if self.physical_vertical.contains(&(at, Direction::Down)) {
                return self
                    .passages
                    .get(&(at, Direction::Down))
                    .is_some_and(|(passage, _)| inside(&passage.to) && solid(passage.to));
            }
            at.position.z.checked_sub(1).is_some_and(|z| {
                let below = Location {
                    position: Position { z, ..at.position },
                    ..at
                };
                inside(&below) && solid(below)
            })
        };
        let (x, y, z) = self.region.bounds.dimensions();
        x > 0
            && self.named_anchors.iter().all(|(name, position)| {
                valid_anchor_name(name) && self.region.bounds.contains(*position)
            })
            && self.named_stairs.iter().all(|((from, direction), to)| {
                inside(from)
                    && matches!(direction, Direction::Up | Direction::Down)
                    && valid_anchor_name(&to.name)
                    && !self.passages.contains_key(&(*from, *direction))
            })
            && y > 0
            && z > 0
            && self.gravity.is_none_or(valid_gravity)
            && self.doors.iter().all(|(at, door)| {
                inside(at) && door.id > 0 && (1..=MAX_DOOR_HEIGHT).contains(&door.height)
            })
            && self
                .secret_doors
                .iter()
                .all(|(at, concealment)| self.doors.contains_key(at) && concealment.valid())
            && {
                let mut ids = BTreeSet::new();
                self.traps.iter().all(|(at, trap)| {
                    inside(at)
                        && !solid(*at)
                        && match &trap.kind {
                            TrapKind::TrapDoor {
                                destination,
                                open: true,
                            } => {
                                self.physical_vertical.contains(&(*at, Direction::Down))
                                    && self.passages.get(&(*at, Direction::Down)).is_some_and(
                                        |(passage, rotation)| {
                                            passage.to.region == destination.region
                                                && *rotation == 0
                                        },
                                    )
                            }
                            TrapKind::TrapDoor {
                                destination,
                                open: false,
                            } => {
                                destination.region != id
                                    && supported(*at)
                                    && !self.passages.contains_key(&(*at, Direction::Down))
                            }
                            _ => supported(*at),
                        }
                        && trap.valid()
                        && ids.insert(trap.id)
                })
            }
            && self
                .passages
                .iter()
                .all(|((from, direction), (passage, turns))| {
                    inside(from)
                        && passage.from == *from
                        && passage.direction == *direction
                        && *turns < 24
                })
            && self
                .physical_vertical
                .iter()
                .all(|key| self.passages.contains_key(key))
            && self.solid_boundaries.iter().all(|key| {
                !matches!(key.1, Direction::Up | Direction::Down) && self.passages.contains_key(key)
            })
            && self
                .cell_gravity
                .iter()
                .all(|(at, g)| inside(at) && valid_gravity(*g))
            && self.lighting.as_ref().is_none_or(|light| {
                light.cells.keys().all(|position| {
                    inside(&Location {
                        region: id,
                        position: *position,
                    })
                })
            })
            && self.terrain.keys().all(inside)
            && self.place_hints.keys().all(inside)
    }
}

/// A world-table key that sorts by region first, so one region's keys are a
/// contiguous range starting at [`RegionKey::first`].
trait RegionKey: Ord + Clone {
    fn region(&self) -> RegionId;
    /// The smallest key in `region`.
    fn first(region: RegionId) -> Self;
}

impl RegionKey for Location {
    fn region(&self) -> RegionId {
        self.region
    }
    fn first(region: RegionId) -> Self {
        Location {
            region,
            position: Position {
                x: i32::MIN,
                y: i32::MIN,
                z: i32::MIN,
            },
        }
    }
}

impl RegionKey for (Location, Direction) {
    fn region(&self) -> RegionId {
        self.0.region
    }
    fn first(region: RegionId) -> Self {
        // North is the first Direction variant under derived Ord. Changing
        // enum/key order requires revisiting every region-local range bound.
        (Location::first(region), Direction::North)
    }
}

/// `region`'s keys in a sorted collection's key order, found without visiting
/// other regions.
fn region_keys<'a, K: RegionKey + 'a>(
    range: impl Iterator<Item = &'a K>,
    region: RegionId,
) -> Vec<K> {
    range
        .take_while(|key| key.region() == region)
        .cloned()
        .collect()
}

/// Every key of `map` in `region`, removed and returned in order. Costs a
/// lookup per key, and copies nothing when the region has no keys.
fn take_region<K: RegionKey, V: Clone>(
    map: &mut Shared<BTreeMap<K, V>>,
    region: RegionId,
) -> BTreeMap<K, V> {
    let keys = region_keys(map.range(K::first(region)..).map(|(k, _)| k), region);
    if keys.is_empty() {
        return BTreeMap::new();
    }
    keys.into_iter()
        .map(|key| {
            let value = map.remove(&key).expect("listed key");
            (key, value)
        })
        .collect()
}

// Match take_region's empty-owner fast path: do not detach shared storage merely
// to discover that this region contributes no keys.
fn take_set<K: RegionKey>(set: &mut Shared<BTreeSet<K>>, region: RegionId) -> BTreeSet<K> {
    let keys = region_keys(set.range(K::first(region)..), region);
    if !keys.is_empty() {
        for key in &keys {
            set.remove(key);
        }
    }
    keys.into_iter().collect()
}

impl World {
    /// Whether a region is loaded or known absent, including not-yet-built regions.
    ///
    /// This is backend topology knowledge, not an actor's disclosed knowledge.
    pub fn knows_region(&self, region: RegionId) -> bool {
        self.regions.contains_key(&region) || self.absent.contains_key(&region)
    }

    /// Whether `location` lies inside a loaded, detached or unbuilt region. References
    /// held as knowledge (memory, objectives) are checked with this; anything
    /// that acts on terrain uses [`World::contains`].
    /// No terrain, occupancy or actor-specific disclosure check is performed.
    pub fn knows(&self, location: Location) -> bool {
        self.contains(location)
            || self
                .absent
                .get(&location.region)
                .is_some_and(|region| region.bounds.contains(location.position))
    }

    /// Know a region that hasn't been built: like a detached region, its
    /// metadata is here and its content arrives later as a [`RegionSlice`].
    /// A chamber's stored bounds include its stone shell, as
    /// [`World::add_chamber`] makes them.
    ///
    /// Pass the interior bounds when `chamber` is true; the helper expands them
    /// exactly once. This registers metadata only, without invoking a generator
    /// or storing a payload. The caller supplies valid metadata and the identity.
    ///
    /// # Errors
    ///
    /// Returns [`WorldError::DuplicateRegion`] for any already-known identity,
    /// or [`WorldError::InvalidEndpoint`] if shell bounds overflow. An error
    /// leaves the world unchanged.
    pub fn add_unbuilt_region(
        &mut self,
        mut region: Region,
        chamber: bool,
    ) -> Result<(), WorldError> {
        if self.knows_region(region.id) {
            return Err(WorldError::DuplicateRegion);
        }
        if chamber {
            region.bounds = region
                .bounds
                .with_shell()
                .ok_or(WorldError::InvalidEndpoint)?;
        }
        self.absent.insert(region.id, region);
        Ok(())
    }

    /// A loaded, detached or unbuilt region's metadata. Knowledge of a
    /// region (its name, say) outlives its content being loaded.
    /// Returns `None` only when neither loaded nor absent metadata exists.
    pub fn known_region(&self, id: RegionId) -> Option<&Region> {
        self.region(id).or_else(|| self.absent.get(&id))
    }

    /// Currently loaded region identities, in ascending ID order.
    ///
    /// This iterator neither builds nor activates a region.
    pub fn loaded_regions(&self) -> impl Iterator<Item = RegionId> + '_ {
        self.regions.keys().copied()
    }

    /// Known absent region identities, in ascending ID order.
    ///
    /// Includes both detached and not-yet-built regions. The world does not
    /// distinguish those host/simulation lifecycle states in this iterator.
    pub fn detached_regions(&self) -> impl Iterator<Item = RegionId> + '_ {
        self.absent.keys().copied()
    }

    /// Regions that a link out of `region` leads to, in id order. Rim
    /// projection also follows these links, so they cover every region a
    /// step from `region` can reach.
    /// Includes unresolved named stairs, unlike [`World::required_linked_regions`].
    /// Only outgoing links currently held by the world participate; links removed
    /// with an absent source live in its [`RegionSlice`] instead.
    pub fn linked_regions(&self, region: RegionId) -> BTreeSet<RegionId> {
        self.exits(region)
            .map(|p| p.to.region)
            .chain(
                self.named_stairs
                    .range((Location::first(region), Direction::North)..)
                    .take_while(|((at, _), _)| at.region == region)
                    .map(|(_, to)| to.region),
            )
            .collect()
    }

    /// Dependencies whose coordinates can already participate in movement or
    /// geometry. Unresolved named stairs are structural edges only; a loader
    /// may defer preparing them when its immutable source cannot supply them.
    ///
    /// Ordinary passage destinations and resolved named stairs are included even
    /// when their regions are absent. The result is deduplicated in region-ID order.
    pub fn required_linked_regions(&self, region: RegionId) -> BTreeSet<RegionId> {
        self.exits(region)
            .map(|passage| passage.to.region)
            .chain(
                self.named_stairs
                    .range((Location::first(region), Direction::North)..)
                    .take_while(|((at, _), _)| at.region == region)
                    .filter(|(_, anchor)| {
                        self.named_anchors
                            .get(&anchor.region)
                            .is_some_and(|names| names.contains_key(&anchor.name))
                    })
                    .map(|(_, anchor)| anchor.region),
            )
            .collect()
    }

    /// Publish a prepared record's structural anchors before any member attaches.
    ///
    /// Borrows the slice without attaching its terrain or other payload. This
    /// lets named dependencies resolve before the residency transaction attaches
    /// the complete required set.
    ///
    /// # Errors
    ///
    /// Returns [`WorldError::InvalidEndpoint`] for a locally invalid slice,
    /// metadata differing from the known region, or an anchor publication
    /// rejected by [`World::register_named_anchors`]. Rejection changes nothing.
    pub fn publish_region_anchors(&mut self, slice: &RegionSlice) -> Result<(), WorldError> {
        if !slice.valid() || self.known_region(slice.id()) != Some(slice.region()) {
            return Err(WorldError::InvalidEndpoint);
        }
        self.register_named_anchors(slice.id(), slice.named_anchors.clone())
    }

    /// Remove a loaded region's payload while retaining known metadata and anchors.
    ///
    /// Incoming links remain with their source regions. Their coordinates can
    /// still resolve, but their absent destination is not a walkable loaded cell.
    /// Outgoing links and physical payload move into the returned [`RegionSlice`].
    /// Simulation actors, clocks and storage are outside this operation's scope.
    ///
    /// Only sight scenes that list this region are invalidated. That's exact:
    /// a scene lists every region it entered and every region linked from
    /// them, and links into this region stay with their source regions.
    ///
    /// # Errors
    ///
    /// Returns [`WorldError::InvalidEndpoint`] if the region is not loaded,
    /// leaving the world unchanged. Detaching is not validation of a world
    /// previously obtained from untrusted serialized state.
    pub fn detach_region(&mut self, id: RegionId) -> Result<RegionSlice, WorldError> {
        let region = self
            .regions
            .get(&id)
            .cloned()
            .ok_or(WorldError::InvalidEndpoint)?;
        // Exhaustive, like `checkpoint::same_geometry`: new world state needs
        // an explicit decision about whether a region slice owns it.
        let World {
            doors,
            secret_doors,
            traps,
            regions,
            passages,
            rotations,
            physical_vertical,
            solid_boundaries,
            named_stairs,
            named_anchors,
            region_gravity,
            cell_gravity,
            terrain,
            lighting,
            chambers,
            place_hints,
            absent,
            sight,
        } = self;
        let links = take_region(passages, id);
        let mut turns = take_region(rotations, id);
        let slice = RegionSlice {
            named_stairs: take_region(named_stairs, id),
            named_anchors: named_anchors.get(&id).cloned().unwrap_or_default(),
            chamber: chambers.get(&id).copied(),
            gravity: region_gravity.get(&id).copied(),
            doors: take_region(doors, id),
            secret_doors: take_region(secret_doors, id),
            traps: take_region(traps, id),
            passages: links
                .into_iter()
                .map(|(key, passage)| {
                    let rotation = turns.remove(&key).unwrap_or(0);
                    (key, (passage, rotation))
                })
                .collect(),
            physical_vertical: take_set(physical_vertical, id),
            solid_boundaries: take_set(solid_boundaries, id),
            cell_gravity: take_region(cell_gravity, id),
            terrain: take_region(terrain, id),
            lighting: lighting.remove(&id),
            place_hints: take_region(place_hints, id),
            region,
        };
        if slice.chamber.is_some() {
            chambers.remove(&id);
        }
        if slice.gravity.is_some() {
            region_gravity.remove(&id);
        }
        regions.remove(&id);
        absent.insert(id, slice.region.clone());
        sight.region_presence_changed(id);
        Ok(slice)
    }

    /// Attach a payload to matching known absent metadata.
    ///
    /// Accepts a detached payload or an initially generated one. The supplied
    /// slice is consumed even on error; clone it first if a caller needs to retain
    /// it for another candidate. Successful attachment publishes its anchors and
    /// invalidates affected sight witnesses. It does not activate simulation
    /// actors, validate trusted-source identity or establish persistence.
    ///
    /// # Errors
    ///
    /// Returns [`WorldError::InvalidEndpoint`] if absent metadata is missing or
    /// differs, [`RegionSlice::valid`] fails, a door/trap ID conflicts with loaded
    /// geometry, or anchor publication conflicts. Every rejection leaves this
    /// world unchanged. The enclosing host must still validate content and
    /// complete identity/lifecycle invariants.
    ///
    /// ```
    /// use tor_world::{Extent, Location, Position, Region, RegionId, World};
    /// let regions = [1, 2].map(|id| Region {
    ///     id: RegionId(id), name: format!("Room {id}"),
    ///     bounds: Extent::new(3, 3, 2).unwrap(),
    /// });
    /// let at = |region| Location {
    ///     region: RegionId(region), position: Position { x: 1, y: 1, z: 0 },
    /// };
    /// let mut world = World::new(regions.into(), vec![]).unwrap();
    /// world.place_door(at(2), 7, false, 1).unwrap();
    /// let slice = world.detach_region(RegionId(2)).unwrap();
    ///
    /// // Deliberately introduce an ID conflict in a separate candidate.
    /// let mut conflicting = world.clone();
    /// conflicting.place_door(at(1), 7, true, 1).unwrap();
    /// let before = conflicting.clone();
    /// assert!(conflicting.attach_region(slice.clone()).is_err());
    /// assert_eq!(conflicting, before);
    /// assert!(!conflicting.contains(at(2)));
    ///
    /// // Retaining the slice permits attachment to the original, valid candidate.
    /// world.attach_region(slice).unwrap();
    /// assert!(world.contains(at(2)));
    /// assert_eq!(world.door_location(7), Some(at(2)));
    /// ```
    pub fn attach_region(&mut self, slice: RegionSlice) -> Result<(), WorldError> {
        let id = slice.id();
        if self.absent.get(&id) != Some(&slice.region) || !slice.valid() {
            return Err(WorldError::InvalidEndpoint);
        }
        if slice
            .door_ids()
            .any(|door| self.door_location(door).is_some())
            || slice
                .traps
                .values()
                .any(|trap| self.trap_location(trap.id).is_some())
        {
            return Err(WorldError::InvalidEndpoint);
        }
        self.publish_region_anchors(&slice)?;
        let RegionSlice {
            named_stairs,
            named_anchors: _,
            region,
            chamber,
            gravity,
            doors,
            secret_doors,
            traps,
            passages,
            physical_vertical,
            solid_boundaries,
            cell_gravity,
            terrain,
            lighting,
            place_hints,
        } = slice;
        self.absent.remove(&id);
        if !named_stairs.is_empty() {
            self.named_stairs.extend(named_stairs);
        }
        self.regions.insert(id, region);
        if let Some(light) = lighting {
            self.lighting.insert(id, light);
        }
        if let Some(chamber) = chamber {
            self.chambers.insert(id, chamber);
        }
        if let Some(gravity) = gravity {
            self.region_gravity.insert(id, gravity);
        }
        if !doors.is_empty() {
            self.doors.extend(doors);
        }
        if !secret_doors.is_empty() {
            self.secret_doors.extend(secret_doors);
        }
        if !traps.is_empty() {
            self.traps.extend(traps);
        }
        if !passages.is_empty() {
            for (key, (passage, turns)) in passages {
                self.passages.insert(key, passage);
                self.rotations.insert(key, turns);
            }
        }
        if !physical_vertical.is_empty() {
            self.physical_vertical.extend(physical_vertical);
        }
        if !solid_boundaries.is_empty() {
            self.solid_boundaries.extend(solid_boundaries);
        }
        if !cell_gravity.is_empty() {
            self.cell_gravity.extend(cell_gravity);
        }
        if !terrain.is_empty() {
            self.terrain.extend(terrain);
        }
        if !place_hints.is_empty() {
            self.place_hints.extend(place_hints);
        }
        self.sight.region_presence_changed(id);
        Ok(())
    }
}

#[cfg(test)]
mod loading_dependency_tests {
    use super::*;
    #[test]
    fn only_unresolved_named_stairs_are_optional_loading_dependencies() {
        let regions = (1..=3)
            .map(|id| Region {
                id: RegionId(id),
                name: "room".into(),
                bounds: crate::Extent::new(3, 3, 2).unwrap(),
            })
            .collect();
        let mut world = World::new(regions, vec![]).unwrap();
        let at = |region, x| Location {
            region: RegionId(region),
            position: crate::Position { x, y: 1, z: 0 },
        };
        world
            .connect_named_stair(
                at(1, 1),
                Direction::Down,
                NamedAnchor {
                    region: RegionId(2),
                    name: "up".into(),
                },
            )
            .unwrap();
        assert!(world.linked_regions(RegionId(1)).contains(&RegionId(2)));
        assert!(!world
            .required_linked_regions(RegionId(1))
            .contains(&RegionId(2)));
        world
            .register_named_anchors(
                RegionId(2),
                BTreeMap::from([("up".into(), crate::Position { x: 1, y: 1, z: 0 })]),
            )
            .unwrap();
        assert!(world
            .required_linked_regions(RegionId(1))
            .contains(&RegionId(2)));
        world
            .connect(
                Passage {
                    from: at(1, 2),
                    direction: Direction::East,
                    to: at(3, 0),
                },
                0,
            )
            .unwrap();
        assert!(world
            .required_linked_regions(RegionId(1))
            .contains(&RegionId(3)));
    }
}
