//! Last-disclosed appearance and witnessed placement, never movement authority.
//! Projection receives saved knowledge and a disclosed scene, not a World.
mod projection_cache;
use crate::navigation_map::{RegionKey, RegionMap};
use crate::{ActorId, Game, GameError, ItemClass};
pub(crate) use projection_cache::ProjectionCache;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use tor_world::{Direction, Location, Position, RegionId, Shared, SightCell};

const MAX_STATES: usize = 32_768;
const MAX_RELATIONS: usize = 262_144;
const MAX_SEEKS: usize = 131_072;
const MAX_OUTPUT: usize = 12_474;
const DIRECTIONS: [Direction; 6] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
    Direction::Up,
    Direction::Down,
];

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapDoorAppearance {
    pub name: String,
    pub description: String,
    pub open: bool,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub asset: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapTrapAppearance {
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub family: Option<tor_world::TrapFamily>,
}
impl<'de> Deserialize<'de> for MapTrapAppearance {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::required_option")]
            family: Option<tor_world::TrapFamily>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            family: fields.family,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapItemAppearance {
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub presentation: Option<crate::appearance_catalog::ObjectPresentation>,
    pub class: ItemClass,
    pub description: String,
    pub name: String,
    pub appearance: String,
    pub identified: bool,
    pub quantity: u64,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub asset: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapAppearance {
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub object_presentation: Option<crate::appearance_catalog::ObjectPresentation>,
    pub wall: bool,
    pub material: String,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub asset: Option<String>,
    pub place_hint: bool,
    pub stairs_up: bool,
    pub stairs_down: bool,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub door: Option<MapDoorAppearance>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub trap: Option<MapTrapAppearance>,
    #[serde(deserialize_with = "read_items")]
    pub ground_items: Vec<MapItemAppearance>,
}

fn read_items<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<MapItemAppearance>, D::Error> {
    // Existing ground stacks have no independent count ceiling. The enclosing
    // save frame bounds decoding; presentation budgets must not reject a turn.
    Vec::<MapItemAppearance>::deserialize(d)
}

fn read_map_text<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let text = String::deserialize(d)?;
    if text.len() > 4096 || text.chars().any(char::is_control) {
        return Err(serde::de::Error::custom("invalid explored-map text"));
    }
    Ok(text)
}

fn read_map_asset<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    // Preserve the ordinary source value, including values too large to page.
    Option::<String>::deserialize(d)
}

fn source_item_label(value: &str) -> bool {
    !value.is_empty() && value.len() <= 80 && !value.chars().any(char::is_control)
}

fn read_item_label<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if !source_item_label(&value) {
        return Err(serde::de::Error::custom("invalid explored-map item label"));
    }
    Ok(value)
}

impl<'de> Deserialize<'de> for MapDoorAppearance {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "read_map_text")]
            name: String,
            #[serde(deserialize_with = "read_map_text")]
            description: String,
            open: bool,
            #[serde(deserialize_with = "read_map_asset")]
            asset: Option<String>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            name: fields.name,
            description: fields.description,
            open: fields.open,
            asset: fields.asset,
        })
    }
}

impl<'de> Deserialize<'de> for MapItemAppearance {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::required_option")]
            presentation: Option<crate::appearance_catalog::ObjectPresentation>,
            class: ItemClass,
            #[serde(deserialize_with = "read_map_text")]
            description: String,
            #[serde(deserialize_with = "read_item_label")]
            name: String,
            #[serde(deserialize_with = "read_item_label")]
            appearance: String,
            identified: bool,
            quantity: u64,
            #[serde(deserialize_with = "read_map_asset")]
            asset: Option<String>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        if fields.quantity == 0
            || (fields.presentation.is_some()
                && (fields.name != "object"
                    || fields.appearance != "object"
                    || !fields.description.is_empty()
                    || fields.asset.is_some()))
        {
            return Err(serde::de::Error::custom("invalid explored-map item"));
        }
        Ok(Self {
            presentation: fields.presentation,
            class: fields.class,
            description: fields.description,
            name: fields.name,
            appearance: fields.appearance,
            identified: fields.identified,
            quantity: fields.quantity,
            asset: fields.asset,
        })
    }
}

impl<'de> Deserialize<'de> for MapAppearance {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::required_option")]
            object_presentation: Option<crate::appearance_catalog::ObjectPresentation>,
            wall: bool,
            #[serde(deserialize_with = "read_map_text")]
            material: String,
            #[serde(deserialize_with = "read_map_asset")]
            asset: Option<String>,
            place_hint: bool,
            stairs_up: bool,
            stairs_down: bool,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            door: Option<MapDoorAppearance>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            trap: Option<MapTrapAppearance>,
            #[serde(deserialize_with = "read_items")]
            ground_items: Vec<MapItemAppearance>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            object_presentation: fields.object_presentation,
            wall: fields.wall,
            material: fields.material,
            asset: fields.asset,
            place_hint: fields.place_hint,
            stairs_up: fields.stairs_up,
            stairs_down: fields.stairs_down,
            door: fields.door,
            trap: fields.trap,
            ground_items: fields.ground_items,
        })
    }
}

impl<'de> Deserialize<'de> for ExploredMap {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            cells: RegionMap<Location, Shared<MapAppearance>>,
            neighbors: RegionMap<(Location, Direction), Neighbor>,
            witnesses: RegionMap<Location, Witness>,
            children: RegionMap<ChildKey, u8>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let map = Self {
            cells: fields.cells,
            neighbors: fields.neighbors,
            witnesses: fields.witnesses,
            children: fields.children,
        };
        if !map.structurally_valid() {
            return Err(serde::de::Error::custom("invalid explored-map evidence"));
        }
        Ok(map)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapCellInput {
    pub location: Location,
    pub appearance: MapAppearance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectedMapCell {
    pub location: Location,
    pub position: Position,
    pub appearance: Shared<MapAppearance>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapWindow {
    pub min: Position,
    pub max: Position,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapProjection {
    pub cells: Vec<ProjectedMapCell>,
    pub complete: bool,
    pub ambiguous: bool,
    pub limited: bool,
    /// Wire materialization would exceed its metadata budget, rather than a
    /// geometry-work cap. Hosts may report a correlated ResourceLimit for this.
    pub appearance_limited: bool,
    pub work: MapWorkCounts,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapWorkCounts {
    pub observed_cells: usize,
    pub placement_proposals: usize,
    /// Neighbor facts unchanged since the actor's last recording, not proposed again.
    pub placement_proposals_reused: usize,
    pub appearance_changes: usize,
    pub neighbor_changes: usize,
    pub witness_changes: usize,
    pub owner_copies: usize,
    pub directory_entries_copied: usize,
    pub bucket_entries_copied: usize,
    pub appearance_payload_copies: usize,
    pub projection_states: usize,
    pub candidate_relations: usize,
    pub inverse_checks: usize,
    pub range_seeks: usize,
    pub reachability_edges: usize,
    pub validation_cells: usize,
    pub validation_relations: usize,
    pub world_location_checks: usize,
    pub validation_directory_entries_copied: usize,
    pub pool_bucket_allocations: usize,
    pub pool_identity_hits: usize,
    pub pool_compared_entries: usize,
}

thread_local! { static COUNTS: std::cell::Cell<MapWorkCounts> = const {
    std::cell::Cell::new(MapWorkCounts { observed_cells:0, placement_proposals:0,
        placement_proposals_reused:0,
        appearance_changes:0, neighbor_changes:0, witness_changes:0, owner_copies:0,
        directory_entries_copied:0, bucket_entries_copied:0, appearance_payload_copies:0,
        projection_states:0, candidate_relations:0, inverse_checks:0, range_seeks:0,
        reachability_edges:0, validation_cells:0, validation_relations:0,
        world_location_checks:0, validation_directory_entries_copied:0,
        pool_bucket_allocations:0, pool_identity_hits:0, pool_compared_entries:0 })
}; }

pub fn map_work_counts() -> MapWorkCounts {
    COUNTS.with(std::cell::Cell::get)
}
pub fn reset_map_work_counts() {
    COUNTS.with(|c| c.set(MapWorkCounts::default()));
}
pub(crate) fn count(f: impl FnOnce(&mut MapWorkCounts)) {
    COUNTS.with(|c| {
        let mut n = c.get();
        f(&mut n);
        c.set(n);
    });
}

pub(crate) fn count_map_validation(cells: usize, relations: usize, world_locations: usize) {
    count(|n| {
        n.validation_cells += cells;
        n.validation_relations += relations;
        n.world_location_checks += world_locations;
    });
}

type RootProofs = BTreeMap<ActorId, tor_world::SharedIdentity<ExploredMap>>;

/// Derived recording evidence: each actor's last recorded physical scene and
/// the stored map that recording left. Never serialized or compared as
/// gameplay state.
///
/// A moving observer re-sees almost the same cells each step. A neighbor fact
/// between two cells whose locations, rotations and relative placement are
/// unchanged since the last recording, each disclosed once in both scenes,
/// was proposed then without competition and stored; while the stored map is
/// still that recording's result, proposing it again only to discard it as
/// unchanged is skipped. Recording is the only in-place writer of a stored
/// map; every other path replaces the shared value, so its identity differs.
#[derive(Clone, Debug, Default)]
pub(crate) struct MapSceneMemo {
    scenes: Shared<BTreeMap<ActorId, Shared<SceneMemo>>>,
}
impl PartialEq for MapSceneMemo {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for MapSceneMemo {}

#[derive(Debug)]
struct SceneMemo {
    map: tor_world::SharedIdentity<ExploredMap>,
    /// Each physical location: how often it was disclosed, and the rotation
    /// and offset of its (last) occurrence.
    placed: Placements,
}

type Placements = std::collections::HashMap<Location, (u32, u8, Position)>;

impl SceneMemo {
    /// Where a location disclosed exactly once sat: rotation and offset.
    fn once(&self, location: &Location) -> Option<(u8, Position)> {
        self.placed
            .get(location)
            .filter(|(count, _, _)| *count == 1)
            .map(|&(_, rotation, offset)| (rotation, offset))
    }
}

impl MapSceneMemo {
    /// The actor's last recorded scene, if its stored map is still what that
    /// recording left.
    fn current(&self, actor: ActorId, map: Option<&Shared<ExploredMap>>) -> Option<&SceneMemo> {
        let memo = self.scenes.get(&actor)?;
        map.is_some_and(|map| memo.map.matches(map))
            .then_some(&**memo)
    }
    fn remember(&mut self, actor: ActorId, memo: SceneMemo) {
        self.scenes.insert(actor, Shared::new(memo));
    }
}

/// Derived Game-owned validation evidence, never serialized or compared as
/// gameplay state. Weak roots do not keep detached appearance payloads loaded.
#[derive(Clone, Debug, Default)]
pub(crate) struct MapValidationCache {
    loaded: Shared<RootProofs>,
    detached: Shared<BTreeMap<crate::RecordId, Shared<RootProofs>>>,
}
impl PartialEq for MapValidationCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for MapValidationCache {}
impl MapValidationCache {
    fn loaded_mut(&mut self) -> &mut RootProofs {
        if self.loaded.is_shared() {
            count(|n| n.validation_directory_entries_copied += self.loaded.len());
        }
        &mut self.loaded
    }
    fn detached_mut(&mut self) -> &mut BTreeMap<crate::RecordId, Shared<RootProofs>> {
        if self.detached.is_shared() {
            count(|n| n.validation_directory_entries_copied += self.detached.len());
        }
        &mut self.detached
    }
    pub(crate) fn loaded_matches(&self, actor: ActorId, map: &Shared<ExploredMap>) -> bool {
        self.loaded
            .get(&actor)
            .is_some_and(|proof| proof.matches(map))
    }
    pub(crate) fn accept_loaded(&mut self, actor: ActorId, map: &Shared<ExploredMap>) {
        if !self.loaded_matches(actor, map) {
            self.loaded_mut().insert(actor, map.weak_identity());
        }
    }
    pub(crate) fn forget_loaded(&mut self, actor: ActorId) {
        if self.loaded.contains_key(&actor) {
            self.loaded_mut().remove(&actor);
        }
    }
    pub(crate) fn detach(
        &mut self,
        id: crate::RecordId,
        maps: &BTreeMap<ActorId, Shared<ExploredMap>>,
    ) {
        let valid = maps
            .iter()
            .all(|(actor, map)| self.loaded_matches(*actor, map));
        let roots = valid.then(|| {
            maps.iter()
                .map(|(actor, map)| (*actor, map.weak_identity()))
                .collect()
        });
        for actor in maps.keys() {
            self.forget_loaded(*actor);
        }
        if let Some(roots) = roots {
            if !maps.is_empty() {
                self.detached_mut().insert(id, Shared::new(roots));
            }
        }
    }
    pub(crate) fn detached_matches(
        &self,
        id: crate::RecordId,
        maps: &BTreeMap<ActorId, Shared<ExploredMap>>,
    ) -> bool {
        maps.is_empty()
            || self.detached.get(&id).is_some_and(|proofs| {
                proofs.len() == maps.len()
                    && maps.iter().all(|(actor, map)| {
                        proofs.get(actor).is_some_and(|proof| proof.matches(map))
                    })
            })
    }
    pub(crate) fn attach(
        &mut self,
        id: crate::RecordId,
        maps: &BTreeMap<ActorId, Shared<ExploredMap>>,
        trusted: bool,
    ) {
        if self.detached.contains_key(&id) {
            self.detached_mut().remove(&id);
        }
        if trusted {
            for (actor, map) in maps {
                self.accept_loaded(*actor, map);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Neighbor {
    Placed { target: Location, rotation: u8 },
    Ambiguous,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Witness {
    Placed {
        anchor: Location,
        delta: [i64; 3],
        rotation: u8,
    },
    Ambiguous,
}

pub(crate) type ChildKey = (Location, [i64; 3], Location);
impl RegionKey for ChildKey {
    fn region(&self) -> RegionId {
        self.0.region
    }
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExploredMap {
    pub(crate) cells: RegionMap<Location, Shared<MapAppearance>>,
    pub(crate) neighbors: RegionMap<(Location, Direction), Neighbor>,
    pub(crate) witnesses: RegionMap<Location, Witness>,
    pub(crate) children: RegionMap<ChildKey, u8>,
}

impl MapAppearance {
    pub(crate) fn valid_for_catalog(
        &self,
        catalog: &crate::appearance_catalog::PresentationCatalog,
    ) -> bool {
        self.object_presentation
            .as_ref()
            .is_none_or(|p| catalog.contains_object(p))
            && self.ground_items.iter().all(|i| {
                i.presentation
                    .as_ref()
                    .is_none_or(|p| catalog.contains_object(p))
            })
    }
    pub(crate) fn valid(&self) -> bool {
        let text = |s: &str| s.len() <= 4096 && !s.chars().any(char::is_control);
        text(&self.material)
            && self
                .object_presentation
                .as_ref()
                .is_none_or(|p| p.valid() && !self.wall && (self.stairs_up || self.stairs_down))
            && self
                .door
                .as_ref()
                .is_none_or(|d| text(&d.name) && text(&d.description))
            && self.ground_items.iter().all(|i| {
                i.quantity > 0
                    && i.presentation.as_ref().is_none_or(|p| {
                        p.valid()
                            && i.name == "object"
                            && i.appearance == "object"
                            && i.description.is_empty()
                            && i.asset.is_none()
                    })
                    && source_item_label(&i.name)
                    && text(&i.description)
                    && source_item_label(&i.appearance)
            })
    }
}

impl ExploredMap {
    pub(crate) fn checkpoint_valid(&self, world: &tor_world::World) -> bool {
        self.structurally_valid() && self.world_locations_valid(world)
    }
    pub(crate) fn world_locations_valid(&self, world: &tor_world::World) -> bool {
        self.cells.keys().all(|at| {
            count(|n| n.world_location_checks += 1);
            world
                .known_region(at.region)
                .is_some_and(|region| region.bounds.contains(at.position))
        })
    }
    pub(crate) fn structurally_valid(&self) -> bool {
        self.cells.iter().all(|(_, appearance)| {
            count(|n| n.validation_cells += 1);
            appearance.valid()
        }) && self.neighbors.iter().all(|((source, direction), edge)| {
            count(|n| n.validation_relations += 1);
            DIRECTIONS.contains(direction)
                && self.cells.contains_key(source)
                && match edge {
                    Neighbor::Ambiguous => true,
                    Neighbor::Placed { target, rotation } => {
                        *rotation < 24 && self.cells.contains_key(target)
                    }
                }
        }) && self.witnesses.iter().all(|(child, witness)| {
            count(|n| n.validation_relations += 1);
            self.cells.contains_key(child)
                && match witness {
                    Witness::Ambiguous => true,
                    Witness::Placed {
                        anchor,
                        delta,
                        rotation,
                    } => {
                        *rotation < 24
                            && self.cells.contains_key(anchor)
                            && checked_rotate(*rotation, *delta).is_some()
                            && delta.iter().all(|n| n.checked_neg().is_some())
                            && self.children.get(&(*anchor, *delta, *child)) == Some(rotation)
                    }
                }
        }) && self
            .children
            .iter()
            .all(|((anchor, delta, child), rotation)| {
                count(|n| n.validation_relations += 1);
                self.witnesses.get(child)
                    == Some(&Witness::Placed {
                        anchor: *anchor,
                        delta: *delta,
                        rotation: *rotation,
                    })
            })
    }
}

/// Cube rotations are signed permutations. Validate signs without ever negating MIN.
fn checked_rotate(rotation: u8, value: [i64; 3]) -> Option<[i64; 3]> {
    if rotation >= 24 {
        return None;
    }
    let mut out = [0; 3];
    for axis in 0..3 {
        let mut basis = [0; 3];
        basis[axis] = 1;
        let rotated = tor_world::rotate_vector(rotation, basis);
        for target in 0..3 {
            if rotated[target] != 0 {
                out[target] = if rotated[target] == 1 {
                    value[axis]
                } else {
                    value[axis].checked_neg()?
                };
            }
        }
    }
    Some(out)
}

impl Clone for ExploredMap {
    fn clone(&self) -> Self {
        count(|n| {
            n.owner_copies += 1;
            n.directory_entries_copied += self.cells.region_count()
                + self.neighbors.region_count()
                + self.witnesses.region_count()
                + self.children.region_count();
        });
        Self {
            cells: self.cells.clone(),
            neighbors: self.neighbors.clone(),
            witnesses: self.witnesses.clone(),
            children: self.children.clone(),
        }
    }
}

/// Opaque immutable ownership proof for a bounded transfer. Cloning is O(1).
#[derive(Clone, Debug)]
pub struct MapKnowledgeRoot(Option<Shared<ExploredMap>>);

fn vector(p: Position) -> [i64; 3] {
    [i64::from(p.x), i64::from(p.y), i64::from(p.z)]
}
fn offset(p: Position, delta: [i64; 3]) -> Option<Position> {
    let p = vector(p);
    Some(Position {
        x: i32::try_from(p[0].checked_add(delta[0])?).ok()?,
        y: i32::try_from(p[1].checked_add(delta[1])?).ok()?,
        z: i32::try_from(p[2].checked_add(delta[2])?).ok()?,
    })
}
fn difference(a: Position, b: Position) -> [i64; 3] {
    let a = vector(a);
    let b = vector(b);
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn direction_delta(d: Direction) -> [i64; 3] {
    let (x, y, z) = d.delta();
    [i64::from(x), i64::from(y), i64::from(z)]
}
fn opposite(d: Direction) -> Direction {
    Direction::from_delta(direction_delta(d).map(|v| -v)).expect("cardinal direction")
}
fn propose<K: Ord, V: PartialEq>(map: &mut BTreeMap<K, V>, key: K, value: V, ambiguous: V) {
    match map.entry(key) {
        std::collections::btree_map::Entry::Vacant(e) => {
            e.insert(value);
        }
        std::collections::btree_map::Entry::Occupied(mut e) if *e.get() != value => {
            e.insert(ambiguous);
        }
        _ => {}
    }
}
fn root(parents: &mut [usize], mut at: usize) -> usize {
    while parents[at] != at {
        parents[at] = parents[parents[at]];
        at = parents[at];
    }
    at
}

impl Game {
    /// Dependency identities only skip work. A changed identity conservatively
    /// rebuilds ordinary disclosure; identities never enter saved knowledge.
    pub fn map_disclosure_changed_since(&self, before: &Game, id: ActorId) -> bool {
        !self
            .map_disclosure_changed_actors_since(before, [id])
            .is_empty()
    }

    /// Compute shared optical dependencies once, preserving caller actor order.
    /// This is not caller-supplied authority: all skip proofs use the two Games.
    pub fn map_disclosure_changed_actors_since(
        &self,
        before: &Game,
        actors: impl IntoIterator<Item = ActorId>,
    ) -> Vec<ActorId> {
        let now = self.emitter_signature();
        let prior = before.emitter_signature();
        let shared_changed = !self.world.shares_storage(&before.world)
            || now != prior
            || (!self.items.shares_storage(&before.items)
                && (now.0.is_empty() || !self.map_items_equal_except_burning_fuel(before)));
        actors
            .into_iter()
            .filter(|id| self.map_actor_changed_since(before, *id, shared_changed))
            .collect()
    }

    fn map_actor_changed_since(&self, before: &Game, id: ActorId, shared_changed: bool) -> bool {
        if !self.explored_maps.contains_key(&id) || shared_changed {
            return true;
        }
        let (Some(a), Some(b)) = (self.actors.get(&id), before.actors.get(&id)) else {
            return true;
        };
        self.projected_hallucination(id)
            .map(|c| c.map(|c| (c.generation(), c.presentation_tick())))
            != before
                .projected_hallucination(id)
                .map(|c| c.map(|c| (c.generation(), c.presentation_tick())))
            || self.actor_scene_witness(id) != before.actor_scene_witness(id)
            || a.location != b.location
            || a.orientation != b.orientation
            || !a.body.shares_storage(&b.body)
            || a.alive() != b.alive()
            || !a.knowledge.shares_storage(&b.knowledge)
            || (self.combat.selected == Some(id)) != (before.combat.selected == Some(id))
    }

    fn map_items_equal_except_burning_fuel(&self, before: &Game) -> bool {
        if self.items.len() != before.items.len() {
            return false;
        }
        self.items
            .iter()
            .zip(before.items.iter())
            .all(|((id, item), (prior_id, old))| {
                if id != prior_id {
                    return false;
                }
                // Exhaustive field inventory: a new item field must be classified,
                // rather than silently inheriting this narrowly optical exception.
                let crate::Item {
                    corpse,
                    condition,
                    charges,
                    relocation_elapsed,
                    fuel,
                    motion,
                    orientation,
                    spec,
                    quantity,
                    holds,
                    location,
                } = item;
                corpse == &old.corpse
                    && condition == &old.condition
                    && charges == &old.charges
                    && relocation_elapsed == &old.relocation_elapsed
                    && motion == &old.motion
                    && orientation == &old.orientation
                    && spec == &old.spec
                    && quantity == &old.quantity
                    && holds == &old.holds
                    && location == &old.location
                    && (fuel == &old.fuel
                        || matches!((fuel, old.fuel),
                    (Some(crate::portable_light::FuelState { remaining, lit: true }),
                     Some(crate::portable_light::FuelState { remaining: previous, lit: true }))
                        if *remaining > 0 && *remaining <= previous))
            })
    }

    pub fn map_knowledge_root(&self, actor: ActorId) -> MapKnowledgeRoot {
        MapKnowledgeRoot(self.explored_maps.get(&actor).cloned())
    }
    pub fn map_knowledge_is_current(&self, actor: ActorId, root: &MapKnowledgeRoot) -> bool {
        match (self.explored_maps.get(&actor), &root.0) {
            (Some(a), Some(b)) => a.shares_storage(b),
            (None, None) => true,
            _ => false,
        }
    }

    pub fn record_explored_observation(
        &mut self,
        actor: ActorId,
        cells: &[MapCellInput],
        scene: &[SightCell],
    ) -> Result<(), GameError> {
        let origin = self
            .actors
            .get(&actor)
            .ok_or(GameError::UnknownActor)?
            .location;
        if let Some(previous) = self.explored_maps.get(&actor) {
            if !self.map_validation.loaded_matches(actor, previous)
                && !previous.checkpoint_valid(&self.world)
            {
                return Err(GameError::InvalidLocation);
            }
        }
        let mut appearances = BTreeMap::new();
        for cell in cells {
            count_map_validation(0, 0, 1);
            if !self.world.knows(cell.location)
                || !self
                    .world
                    .known_region(cell.location.region)
                    .is_some_and(|region| region.bounds.contains(cell.location.position))
                || !cell.appearance.valid()
                || !cell
                    .appearance
                    .valid_for_catalog(self.content.presentation_catalog())
            {
                return Err(GameError::InvalidLocation);
            }
            if appearances
                .insert(cell.location, &cell.appearance)
                .is_some_and(|previous| previous != &cell.appearance)
            {
                return Err(GameError::InvalidLocation);
            }
        }
        let physical: Vec<_> = scene.iter().filter(|c| c.physical).collect();
        let mut seen_offsets = std::collections::HashSet::with_capacity(physical.len());
        let mut placed = Placements::with_capacity(physical.len());
        for cell in &physical {
            if cell.rotation >= 24
                || !appearances.contains_key(&cell.location)
                || !seen_offsets.insert(cell.offset)
            {
                return Err(GameError::InvalidLocation);
            }
            let entry = placed
                .entry(cell.location)
                .or_insert((0, cell.rotation, cell.offset));
            *entry = (entry.0 + 1, cell.rotation, cell.offset);
        }
        drop(seen_offsets);
        let index = crate::travel::OffsetIndex::new(
            &physical
                .iter()
                .map(|cell| (cell.offset.x, cell.offset.y, cell.offset.z))
                .collect::<Vec<_>>(),
        );
        let memo = self
            .map_scenes
            .current(actor, self.explored_maps.get(&actor));
        // Each cell is resolved against the last recorded scene once: whether
        // its location is disclosed once now, and where it sat then if it was
        // disclosed once there too. Neighbor checks then compare indexes.
        let once_now: Vec<bool> = physical
            .iter()
            .map(|cell| {
                placed
                    .get(&cell.location)
                    .is_some_and(|(count, _, _)| *count == 1)
            })
            .collect();
        let before: Vec<Option<(u8, Position)>> = physical
            .iter()
            .zip(&once_now)
            .map(|(cell, &once)| {
                memo.filter(|_| once)
                    .and_then(|memo| memo.once(&cell.location))
            })
            .collect();
        let mut parents: Vec<_> = (0..physical.len()).collect();
        let mut neighbors = BTreeMap::new();
        let mut reused = 0;
        for (i, a) in physical.iter().enumerate() {
            // Where this cell sat in the last recorded scene, if it is
            // disclosed once in both and keeps its rotation.
            let was = before[i]
                .filter(|(rotation, _)| *rotation == a.rotation)
                .map(|(_, was)| was);
            for direction in DIRECTIONS {
                let delta = direction_delta(direction);
                let Some(next) = offset(a.offset, delta) else {
                    continue;
                };
                let Some(j) = index.get(&(next.x, next.y, next.z)) else {
                    continue;
                };
                let b = physical[j];
                let ar = root(&mut parents, i);
                let br = root(&mut parents, j);
                parents[ar] = br;
                // The neighbor, disclosed once in both scenes, keeps its
                // rotation and sits at the same step from this cell as then.
                // Locations are unique there, so this is the very same fact.
                let unchanged = was.is_some_and(|was| {
                    once_now[j]
                        && before[j].is_some_and(|(rotation, then)| {
                            rotation == b.rotation && offset(was, delta) == Some(then)
                        })
                });
                if unchanged {
                    reused += 1;
                    continue;
                }
                propose(
                    &mut neighbors,
                    (a.location, direction.rotated(a.rotation)),
                    Neighbor::Placed {
                        target: b.location,
                        rotation: tor_world::compose_rotation(
                            tor_world::inverse_rotation(a.rotation),
                            b.rotation,
                        ),
                    },
                    Neighbor::Ambiguous,
                );
            }
        }
        let mut representatives = BTreeMap::new();
        let mut primary = None;
        for (i, cell) in physical.iter().enumerate() {
            let component = root(&mut parents, i);
            let key = (cell.location, cell.rotation, cell.offset);
            representatives
                .entry(component)
                .and_modify(|rep: &mut usize| {
                    let old = physical[*rep];
                    if key < (old.location, old.rotation, old.offset) {
                        *rep = i;
                    }
                })
                .or_insert(i);
            if cell.location == origin && cell.offset == (Position { x: 0, y: 0, z: 0 }) {
                primary = Some(component);
            }
        }
        let primary = primary.or_else(|| {
            representatives
                .iter()
                .min_by_key(|(_, i)| {
                    let c = physical[**i];
                    (c.location, c.rotation, c.offset)
                })
                .map(|(c, _)| *c)
        });
        let mut witnesses = BTreeMap::new();
        if let Some(component) = primary {
            let a = physical[representatives[&component]];
            for (&other, &i) in &representatives {
                if other == component {
                    continue;
                }
                let b = physical[i];
                let delta = checked_rotate(a.rotation, difference(b.offset, a.offset))
                    .ok_or(GameError::InvalidLocation)?;
                propose(
                    &mut witnesses,
                    b.location,
                    Witness::Placed {
                        anchor: a.location,
                        delta,
                        rotation: tor_world::compose_rotation(
                            tor_world::inverse_rotation(a.rotation),
                            b.rotation,
                        ),
                    },
                    Witness::Ambiguous,
                );
            }
        }
        count(|n| {
            n.observed_cells += cells.len();
            n.placement_proposals += neighbors.len() + witnesses.len();
            n.placement_proposals_reused += reused;
        });
        let recorded = placed;
        let previous = self.explored_maps.get(&actor);
        appearances.retain(|at, appearance| {
            previous
                .and_then(|m| m.cells.get(at))
                .is_none_or(|old| &**old != *appearance)
        });
        neighbors.retain(|key, value| previous.and_then(|m| m.neighbors.get(key)) != Some(value));
        witnesses.retain(|key, value| previous.and_then(|m| m.witnesses.get(key)) != Some(value));
        if appearances.is_empty() && neighbors.is_empty() && witnesses.is_empty() {
            if let Some(map) = self.explored_maps.get(&actor) {
                self.map_validation.accept_loaded(actor, map);
                let map = map.weak_identity();
                self.map_scenes.remember(
                    actor,
                    SceneMemo {
                        map,
                        placed: recorded,
                    },
                );
            }
            return Ok(());
        }
        self.map_validation.forget_loaded(actor);
        let map = &mut **self.explored_maps.entry(actor).or_default();
        for (at, appearance) in appearances {
            count(|n| {
                n.appearance_changes += 1;
                n.appearance_payload_copies += 1;
                n.bucket_entries_copied += map.cells.copy_on_write_entries(at.region);
            });
            map.cells.insert(at, Shared::new(appearance.clone()));
        }
        for (key, value) in neighbors {
            count(|n| {
                n.neighbor_changes += 1;
                n.bucket_entries_copied += map.neighbors.copy_on_write_entries(key.0.region);
            });
            map.neighbors.insert(key, value);
        }
        for (child, value) in witnesses {
            if let Some(Witness::Placed { anchor, delta, .. }) = map.witnesses.get(&child) {
                let key = (*anchor, *delta, child);
                count(|n| {
                    n.bucket_entries_copied += map.children.copy_on_write_entries(anchor.region)
                });
                map.children.remove(&key);
            }
            if let Witness::Placed {
                anchor,
                delta,
                rotation,
            } = value
            {
                count(|n| {
                    n.bucket_entries_copied += map.children.copy_on_write_entries(anchor.region)
                });
                map.children.insert((anchor, delta, child), rotation);
            }
            count(|n| {
                n.witness_changes += 1;
                n.bucket_entries_copied += map.witnesses.copy_on_write_entries(child.region);
            });
            map.witnesses.insert(child, value);
        }
        self.map_validation
            .accept_loaded(actor, &self.explored_maps[&actor]);
        let map = self.explored_maps[&actor].weak_identity();
        self.map_scenes.remember(
            actor,
            SceneMemo {
                map,
                placed: recorded,
            },
        );
        Ok(())
    }
}

impl MapWindow {
    fn contains(self, p: Position) -> bool {
        p.x >= self.min.x
            && p.x <= self.max.x
            && p.y >= self.min.y
            && p.y <= self.max.y
            && p.z >= self.min.z
            && p.z <= self.max.z
    }
    fn halo(self) -> Option<Self> {
        for (a, b) in vector(self.min).into_iter().zip(vector(self.max)) {
            if b < a || b - a >= 129 {
                return None;
            }
        }
        Some(Self {
            min: offset(self.min, [-1; 3])?,
            max: offset(self.max, [1; 3])?,
        })
    }
    fn projection_halo(self, domain: Self) -> Option<Self> {
        self.halo()?;
        if !domain.contains(self.min) || !domain.contains(self.max) {
            return None;
        }
        domain.halo()
    }
    fn local_bounds(self, p: Position, rotation: u8) -> Option<([i64; 3], [i64; 3])> {
        let a = checked_rotate(rotation, difference(self.min, p))?;
        let b = checked_rotate(rotation, difference(self.max, p))?;
        Some((
            std::array::from_fn(|i| a[i].min(b[i])),
            std::array::from_fn(|i| a[i].max(b[i])),
        ))
    }
}

impl MapAppearance {
    /// Conservative JSON/wire bound; inspect lengths without serializing or
    /// cloning payloads. Valid appearance strings exclude controls, so two
    /// bytes per UTF-8 byte covers the remaining quote/backslash JSON escapes.
    fn encoded_size_bound(&self) -> usize {
        let text = |s: &str| s.len().saturating_mul(2).saturating_add(2);
        let asset = |s: &Option<String>| {
            s.as_deref().map_or(0, |s| {
                if s.len() > 120 * 1024 {
                    return usize::MAX;
                }
                s.bytes().fold(2usize, |n, b| {
                    n.saturating_add(match b {
                        b'"' | b'\\' => 2,
                        0..=31 => 6,
                        _ => 1,
                    })
                })
            })
        };
        let mut bytes = 400usize
            .saturating_add(text(&self.material))
            .saturating_add(asset(&self.asset));
        if let Some(p) = &self.object_presentation {
            bytes = bytes.saturating_add(p.encoded_size_bound());
        }
        if let Some(d) = &self.door {
            bytes = bytes
                .saturating_add(256)
                .saturating_add(text(&d.name))
                .saturating_add(text(&d.description))
                .saturating_add(asset(&d.asset));
        }
        if self.trap.is_some() {
            bytes = bytes.saturating_add(64);
        }
        for i in &self.ground_items {
            if let Some(p) = &i.presentation {
                bytes = bytes.saturating_add(p.encoded_size_bound());
            }
            bytes = bytes
                .saturating_add(256)
                .saturating_add(text(&i.name))
                .saturating_add(text(&i.description))
                .saturating_add(text(&i.appearance))
                .saturating_add(asset(&i.asset));
        }
        bytes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Placement {
    at: Location,
    offset: Position,
    frame: u8,
}
type Priority = (u64, Position, Location, u8, usize);
fn priority(p: Placement, id: usize) -> Priority {
    let v = vector(p.offset);
    (
        v.into_iter().map(i64::unsigned_abs).sum(),
        p.offset,
        p.at,
        p.frame,
        id,
    )
}
struct Projection<'a> {
    map: &'a ExploredMap,
    window: MapWindow,
    halo: MapWindow,
    current: BTreeMap<Position, (Location, u8)>,
    states: Vec<Placement>,
    ids: BTreeMap<Placement, usize>,
    occupied: BTreeMap<Position, usize>,
    blocked: Vec<bool>,
    seeds: Vec<usize>,
    queue: BTreeSet<Priority>,
    edges: Vec<(usize, usize)>,
    work: MapWorkCounts,
    ambiguous: bool,
    limited: bool,
    appearance_limited: bool,
}
impl Projection<'_> {
    fn relation(&mut self) -> bool {
        if self.work.candidate_relations >= MAX_RELATIONS {
            self.limited = true;
            return false;
        }
        self.work.candidate_relations += 1;
        true
    }
    fn inverse(&mut self) -> bool {
        if self.work.inverse_checks >= MAX_RELATIONS {
            self.limited = true;
            return false;
        }
        self.work.inverse_checks += 1;
        true
    }
    fn seek(&mut self) -> bool {
        if self.work.range_seeks >= MAX_SEEKS {
            self.limited = true;
            return false;
        }
        self.work.range_seeks += 1;
        true
    }
    fn add(&mut self, p: Placement, seed: bool) -> Option<usize> {
        if !self.halo.contains(p.offset) || !self.map.cells.contains_key(&p.at) {
            return None;
        }
        if let Some(&id) = self.ids.get(&p) {
            return Some(id);
        }
        if self.states.len() >= MAX_STATES {
            self.limited = true;
            return None;
        }
        let id = self.states.len();
        let mut blocked = false;
        if let Some(&(at, frame)) = self.current.get(&p.offset) {
            if (p.at, p.frame) != (at, frame) {
                blocked = true;
                self.ambiguous = true;
            }
        } else if let Some(&other) = self.occupied.get(&p.offset) {
            if self.states[other] != p {
                blocked = true;
                self.blocked[other] = true;
                self.ambiguous = true;
            }
        }
        self.occupied.entry(p.offset).or_insert(id);
        self.ids.insert(p, id);
        self.states.push(p);
        self.blocked.push(blocked);
        self.work.projection_states += 1;
        if seed {
            self.seeds.push(id);
        }
        self.queue.insert(priority(p, id));
        Some(id)
    }
    fn edge(&mut self, source: usize, target: Location, delta: [i64; 3], rotation: u8) {
        let p = self.states[source];
        let Some(delta) = checked_rotate(tor_world::inverse_rotation(p.frame), delta) else {
            self.ambiguous = true;
            return;
        };
        let Some(position) = offset(p.offset, delta) else {
            self.ambiguous = true;
            return;
        };
        if rotation >= 24 {
            self.ambiguous = true;
            return;
        }
        let target = Placement {
            at: target,
            offset: position,
            frame: tor_world::compose_rotation(p.frame, rotation),
        };
        if let Some(id) = self.add(target, false) {
            if self.edges.len() >= MAX_RELATIONS {
                self.limited = true;
                return;
            }
            self.edges.push((source, id));
        }
    }
    fn expand(&mut self, id: usize) {
        let p = self.states[id];
        for direction in DIRECTIONS {
            if !self.relation() {
                return;
            }
            match self.map.neighbors.get(&(p.at, direction)).cloned() {
                Some(Neighbor::Placed { target, rotation }) if rotation < 24 => {
                    if !self.inverse() {
                        return;
                    }
                    let reverse = opposite(direction).rotated(rotation);
                    if self.map.neighbors.get(&(target, reverse))
                        == Some(&Neighbor::Placed {
                            target: p.at,
                            rotation: tor_world::inverse_rotation(rotation),
                        })
                    {
                        self.edge(id, target, direction_delta(direction), rotation);
                    } else {
                        self.ambiguous = true;
                    }
                }
                Some(_) => self.ambiguous = true,
                None => {}
            }
            if self.limited {
                return;
            }
        }
        if !self.relation() {
            return;
        }
        match self.map.witnesses.get(&p.at).cloned() {
            Some(Witness::Placed {
                anchor,
                delta,
                rotation,
            }) if rotation < 24 => {
                if !self.inverse() {
                    return;
                }
                if self.map.children.get(&(anchor, delta, p.at)) == Some(&rotation) {
                    let neg = delta[0]
                        .checked_neg()
                        .zip(delta[1].checked_neg())
                        .zip(delta[2].checked_neg())
                        .and_then(|((x, y), z)| checked_rotate(rotation, [x, y, z]));
                    if let Some(neg) = neg {
                        self.edge(id, anchor, neg, tor_world::inverse_rotation(rotation));
                    } else {
                        self.ambiguous = true;
                    }
                } else {
                    self.ambiguous = true;
                }
            }
            Some(_) => self.ambiguous = true,
            None => {}
        }
        if self.limited {
            return;
        }
        let Some((min, max)) = self.halo.local_bounds(p.offset, p.frame) else {
            self.ambiguous = true;
            return;
        };
        let lowest = Location {
            region: RegionId(0),
            position: Position {
                x: i32::MIN,
                y: i32::MIN,
                z: i32::MIN,
            },
        };
        use std::ops::Bound::{Excluded, Included, Unbounded};
        let mut lower = Included((p.at, min, lowest));
        loop {
            if !self.seek() {
                return;
            }
            // Charge before touching even the first row; an empty seek may
            // conservatively consume a candidate slot as well.
            if !self.relation() {
                return;
            }
            let entry = self
                .map
                .children
                .region(p.at.region)
                .and_then(|bucket| bucket.range((lower, Unbounded)).next())
                .map(|(k, r)| (*k, *r));
            let Some(((anchor, delta, child), rotation)) = entry else {
                break;
            };
            if anchor != p.at || delta[0] > max[0] {
                break;
            }
            // Seek over a whole excluded run. Historical distant children do
            // not consume one visit each or starve a nearby in-window child.
            let jump = if delta[1] < min[1] {
                Some([delta[0], min[1], min[2]])
            } else if delta[1] > max[1] {
                delta[0].checked_add(1).map(|x| [x, min[1], min[2]])
            } else if delta[2] < min[2] {
                Some([delta[0], delta[1], min[2]])
            } else if delta[2] > max[2] {
                delta[1].checked_add(1).map(|y| [delta[0], y, min[2]])
            } else {
                None
            };
            if let Some(jump) = jump {
                lower = Included((p.at, jump, lowest));
                continue;
            }
            lower = Excluded((anchor, delta, child));
            if !self.inverse() {
                return;
            }
            if self.map.witnesses.get(&child)
                == Some(&Witness::Placed {
                    anchor,
                    delta,
                    rotation,
                })
            {
                self.edge(id, child, delta, rotation);
            } else {
                self.ambiguous = true;
            }
            if self.limited {
                return;
            }
        }
    }
    fn finish(mut self) -> MapProjection {
        while !self.limited {
            let Some((_, _, _, _, id)) = self.queue.pop_first() else {
                break;
            };
            // Discovery is independent of queue order and absolute offsets.
            // Even a blocked parent can reveal a competing placement which
            // must be considered before the second reachability pass.
            self.expand(id);
        }
        let mut cells = Vec::new();
        if !self.limited {
            self.edges.sort_unstable();
            self.edges.dedup();
            let mut reachable = vec![false; self.states.len()];
            let mut queue = VecDeque::new();
            for id in self.seeds {
                if !self.blocked[id] && !reachable[id] {
                    reachable[id] = true;
                    queue.push_back(id);
                }
            }
            while let Some(id) = queue.pop_front() {
                let first = self.edges.partition_point(|&(from, _)| from < id);
                for &(_, to) in self.edges[first..]
                    .iter()
                    .take_while(|&&(from, _)| from == id)
                {
                    self.work.reachability_edges += 1;
                    if !self.blocked[to] && !reachable[to] {
                        reachable[to] = true;
                        queue.push_back(to);
                    }
                }
            }
            let mut bytes = 0usize;
            for (id, p) in self.states.iter().enumerate() {
                if !reachable[id] || !self.window.contains(p.offset) {
                    continue;
                }
                let appearance = self.map.cells.get(&p.at).expect("admitted known cell");
                let size = appearance.encoded_size_bound();
                bytes = bytes.saturating_add(size);
                if cells.len() >= MAX_OUTPUT || size > 120 * 1024 || bytes > 7 * 1024 * 1024 {
                    self.appearance_limited = size > 120 * 1024 || bytes > 7 * 1024 * 1024;
                    self.limited = true;
                    cells.clear();
                    break;
                }
                cells.push(ProjectedMapCell {
                    location: p.at,
                    position: p.offset,
                    appearance: appearance.clone(),
                });
            }
            cells.sort_by_key(|c| (c.position, c.location));
        }
        MapProjection {
            cells,
            complete: !self.limited,
            ambiguous: self.ambiguous,
            limited: self.limited,
            appearance_limited: self.appearance_limited,
            work: self.work,
        }
    }
}

#[cfg(test)]
thread_local! { static PROJECTION_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
#[cfg(test)]
fn projection_calls() -> usize {
    PROJECTION_CALLS.with(std::cell::Cell::get)
}
#[cfg(test)]
fn reset_projection_calls() {
    PROJECTION_CALLS.with(|n| n.set(0));
}
#[cfg(test)]
mod projection_cache_tests;
#[cfg(test)]
mod scene_memo_tests;

impl ExploredMap {
    #[cfg(test)]
    fn project(&self, scene: &[SightCell], window: MapWindow) -> Result<MapProjection, GameError> {
        self.project_region(scene, window, window)
    }

    fn project_region(
        &self,
        scene: &[SightCell],
        window: MapWindow,
        domain: MapWindow,
    ) -> Result<MapProjection, GameError> {
        #[cfg(test)]
        PROJECTION_CALLS.with(|n| n.set(n.get() + 1));
        let halo = window
            .projection_halo(domain)
            .ok_or(GameError::InvalidLocation)?;
        let mut current = BTreeMap::new();
        let mut seeds = BTreeSet::new();
        let mut omitted = false;
        for cell in scene
            .iter()
            .filter(|c| c.physical && halo.contains(c.offset))
        {
            if cell.rotation >= 24 {
                return Err(GameError::InvalidLocation);
            }
            if current.contains_key(&cell.offset) {
                return Err(GameError::InvalidLocation);
            }
            if current.len() < MAX_STATES {
                current.insert(cell.offset, (cell.location, cell.rotation));
            } else {
                omitted = true;
            }
            let p = Placement {
                at: cell.location,
                offset: cell.offset,
                frame: cell.rotation,
            };
            seeds.insert(priority(p, 0));
            if seeds.len() > MAX_STATES {
                seeds.pop_last();
                omitted = true;
            }
        }
        let mut projection = Projection {
            map: self,
            window,
            halo,
            current,
            states: Vec::new(),
            ids: BTreeMap::new(),
            occupied: BTreeMap::new(),
            blocked: Vec::new(),
            seeds: Vec::new(),
            queue: BTreeSet::new(),
            edges: Vec::new(),
            work: MapWorkCounts::default(),
            ambiguous: false,
            limited: omitted,
            appearance_limited: false,
        };
        for (_, offset, at, frame, _) in seeds {
            projection.add(Placement { at, offset, frame }, true);
        }
        Ok(projection.finish())
    }
}
impl Game {
    pub fn project_explored_map(
        &self,
        actor: ActorId,
        scene: &[SightCell],
        window: MapWindow,
    ) -> Result<MapProjection, GameError> {
        if !self.actors.contains_key(&actor) {
            return Err(GameError::UnknownActor);
        }
        self.lifecycle.projections.project(
            actor,
            self.explored_maps.get(&actor),
            scene,
            window,
            window,
        )
    }

    /// Project a subwindow using the containing domain's current scene anchors
    /// and one-cell traversal halo. Output clipping never narrows the evidence
    /// used to resolve placements, aliases or ambiguity. Both windows retain
    /// the ordinary bounds and all projection work limits remain in force.
    pub fn project_explored_map_region(
        &self,
        actor: ActorId,
        scene: &[SightCell],
        window: MapWindow,
        domain: MapWindow,
    ) -> Result<MapProjection, GameError> {
        // Reject malformed caller metadata before consulting actor-owned state.
        window
            .projection_halo(domain)
            .ok_or(GameError::InvalidLocation)?;
        if !self.actors.contains_key(&actor) {
            return Err(GameError::UnknownActor);
        }
        self.lifecycle.projections.project(
            actor,
            self.explored_maps.get(&actor),
            scene,
            window,
            domain,
        )
    }
}

#[cfg(test)]
#[path = "explored_map_tests.rs"]
mod tests;
