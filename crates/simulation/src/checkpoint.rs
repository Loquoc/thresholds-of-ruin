//! Backend-only deterministic checkpoint state. Equal world and navigation
//! regions, actor/AI records, actor tables, map-owner directories, intentions and
//! population states
//! are encoded once across rewind boundaries.
//! This module does not perform storage or I/O.
//!
//! Capture related states into one fresh [`SharedState`]. A [`Snapshot`] holds
//! references into that pool and is not independently restorable without it.
//! For multiple decoded snapshots, construct [`RestoreContext`], bind the complete
//! current/history root list once, then restore each snapshot. For a pool with
//! one snapshot, [`Game::restore_checkpoint`] performs that sequence for you.
//! Re-encoding a decoded pool additionally requires
//! [`SharedState::prepare_spatial_serialization`]; that is not restore authority.
//!
//! # Capture, decode and restore related states
//!
//! The host supplies the same validated content that the save pins. This example
//! exercises the in-memory codec, not disk durability, server archive validation
//! or the host's publication transaction.
//!
//! ```
//! use std::num::NonZeroU64;
//! use tor_simulation::{Action, Game};
//! use tor_simulation::checkpoint::{RestoreContext, SharedState, Snapshot};
//! use tor_simulation::creatures::{Content, ScenarioCatalog};
//! use tor_simulation::rules::{Ruleset, DEFAULT_RULESET_TOML};
//! use tor_world::{Location, Position, RegionId};
//!
//! let rules = Ruleset::validate(toml::from_str(DEFAULT_RULESET_TOML).unwrap()).unwrap();
//! let content = Content::new(rules, ScenarioCatalog::default()).unwrap();
//! let mut game = Game::two_room(42, content.clone());
//! let actor = game.spawn_actor(
//!     Location { region: RegionId(1), position: Position { x: 1, y: 1, z: 0 } },
//!     NonZeroU64::new(100).unwrap(),
//! ).unwrap();
//! let earlier = game.clone();
//! let mut shared = SharedState::default();
//! let first = game.checkpoint(&mut shared);
//! game.act(actor, Action::Wait).unwrap();
//! let second = game.checkpoint(&mut shared);
//!
//! let bytes = serde_json::to_vec(&(vec![first, second], shared)).unwrap();
//! let (snapshots, shared): (Vec<Snapshot>, SharedState) = serde_json::from_slice(&bytes).unwrap();
//! let mut context = RestoreContext::new(&shared, content);
//! context.bind_spatial_roots(&snapshots.iter().collect::<Vec<_>>()).unwrap();
//! let mut snapshots = snapshots.into_iter();
//! let restored_earlier = context.restore(snapshots.next().unwrap()).unwrap();
//! let mut restored_current = context.restore(snapshots.next().unwrap()).unwrap();
//! assert_eq!(restored_earlier, earlier);
//! assert_eq!(restored_current, game);
//! assert_eq!(
//!     restored_current.act(actor, Action::Wait).unwrap(),
//!     game.act(actor, Action::Wait).unwrap(),
//! );
//! assert_eq!(restored_current, game);
//! ```
/// Existing whole-archive raw/compressed ceiling, shared with server storage.
pub const ARCHIVE_BYTE_LIMIT: usize = 64 * 1024 * 1024;
/// Current archive envelope: 256 retained boundaries plus current snapshot.
pub const MAX_CHECKPOINT_ROOTS: usize = 257;

use crate::actor_record::ActorRecord;
use crate::creatures::Content;
use crate::explored_map::ExploredMap;
use crate::streaming::Lifecycle;
use crate::{travel::Navigation, Actor, ActorId, Game, Item, ItemId, ItemLocation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tor_world::Location;
use tor_world::{Shared, World};

/// A malformed checkpoint is rejected before any restored game is published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistError {
    MissingShared {
        table: &'static str,
        index: usize,
    },
    Navigation,
    ExploredMap,
    ChartHistory,
    /// Includes missing catalog references and invalid mutable creature state.
    CreatureRecord {
        actor: ActorId,
        error: crate::creatures::CreatureStateError,
    },
    Equipment {
        actor: ActorId,
        item: ItemId,
    },
    ActorClock {
        actor: ActorId,
    },
    ActorState {
        actor: ActorId,
    },
    ItemDefinition {
        item: ItemId,
        error: crate::rules::RulesetError,
    },
    ItemState {
        item: ItemId,
    },
    Intentions,
    Lifecycle,
    Population,
    Retrieval,
    Physics,
    Spatial,
    RunState,
    RecordPool,
    Schedule,
    World,
}
impl std::fmt::Display for PersistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid checkpoint: {self:?}")
    }
}
impl std::error::Error for PersistError {}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    combat: crate::combat::CombatWorld<usize>,
    rules_rng: u64,
    #[serde(deserialize_with = "read_events")]
    events: Vec<crate::combat::CombatEvent>,
    event_receipts: crate::event_history::EventReceipts,
    physics: crate::physics::Physics,
    world: usize,
    navigation: BTreeMap<ActorId, usize>,
    explored_maps: usize,
    chart_history: usize,
    #[serde(with = "crate::spatial_scope::retention::root_index")]
    spatial: usize,
    seed: u64,
    tick: u64,
    actors: usize,
    items: usize,
    next_actor_id: u64,
    next_item_id: u64,
    next_door_id: u64,
    next_hold_id: u64,
    intentions: crate::intention::checkpoint::Snapshot,
    /// Region streaming state in [`SharedState`], omitted while empty so
    /// games that never stream save exactly as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lifecycle: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scenario_policy: Option<crate::floor_activation::ScenarioPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    floor_activation: Option<PopulationSnapshot>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    unique_retrieval: Option<RetrievalSnapshot>,
    /// Scenario script values, omitted for unscripted games so their saves are
    /// unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    script: Option<crate::scripting::ScriptState>,
    /// Plane rules and stays, omitted for games without planes so their saves
    /// are unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    planes: Option<crate::planes::PlaneState>,
}

fn read_events<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<crate::combat::CombatEvent>, D::Error> {
    Ok(crate::bounded::Bounded::<crate::combat::CombatEvent, { crate::limits::EVENTS_PER_BATCH }>::deserialize(d)?.0)
}

/// Frequently changing active-floor clocks must not duplicate the large prepared
/// population plan and floor receipts at every retained rewind boundary.
#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct PopulationSnapshot {
    state: usize,
    wandering: Option<crate::floor_activation::WanderingPopulation>,
}

impl<'de> Deserialize<'de> for PopulationSnapshot {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            state: usize,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            wandering: Option<crate::floor_activation::WanderingPopulation>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            state: fields.state,
            wandering: fields.wandering,
        })
    }
}

/// Mutable objective evidence stays small; neither visits nor policy changes
/// cause the large once-only population snapshot to lose pooling.
#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct RetrievalSnapshot {
    policy: usize,
    initialized: bool,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    item: Option<ItemId>,
    entry_count: u64,
    placement_count: u64,
    #[serde(deserialize_with = "crate::unique_retrieval::read_entries")]
    entries: Vec<crate::unique_retrieval::RetrievalEntry>,
}

impl<'de> Deserialize<'de> for RetrievalSnapshot {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            policy: usize,
            initialized: bool,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            item: Option<ItemId>,
            entry_count: u64,
            placement_count: u64,
            #[serde(deserialize_with = "crate::unique_retrieval::read_entries")]
            entries: Vec<crate::unique_retrieval::RetrievalEntry>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            policy: fields.policy,
            initialized: fields.initialized,
            item: fields.item,
            entry_count: fields.entry_count,
            placement_count: fields.placement_count,
            entries: fields.entries,
        })
    }
}

#[path = "checkpoint_records.rs"]
mod record_pool;

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedState {
    chart_history: crate::chart_history::StoredPool,
    spatial: crate::spatial_scope::retention::PoolState,
    #[serde(with = "tor_world::checkpoint_worlds")]
    worlds: Vec<World>,
    #[serde(with = "navigation_regions")]
    navigation: Vec<Navigation>,
    #[serde(with = "explored_map_regions")]
    explored_maps: Vec<Shared<ExploredMap>>,
    /// Equal actor-to-root directories across retained boundaries are encoded once.
    #[serde(with = "explored_map_owners")]
    explored_map_owners: Vec<BTreeMap<ActorId, usize>>,
    items: Vec<BTreeMap<ItemId, Item>>,
    #[serde(deserialize_with = "read_actor_directories")]
    actors: Vec<BTreeMap<ActorId, usize>>,
    actor_records: record_pool::Pool<ActorRecord>,
    ai_records: record_pool::Pool<crate::ai::Ai>,
    // Call-local hints only: canonical record contents and indices remain the
    // authority. Weak identities prevent ABA without retaining actor payloads.
    #[serde(skip)]
    actor_capture_sources: BTreeMap<ActorId, (tor_world::SharedIdentity<Actor>, usize)>,
    #[serde(skip)]
    ai_capture_sources: BTreeMap<ActorId, usize>,
    intentions: crate::intention::checkpoint::Pool,
    /// Identical lifecycle states (with their identity directories) across
    /// rewind boundaries are encoded once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    lifecycles: Vec<Lifecycle>,
    /// Complete equal population inputs and receipts are encoded once, rather
    /// than repeating the same prepared floor cells at every ordinary boundary.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "read_population_bases"
    )]
    floor_activations: Vec<crate::floor_activation::ActivationState>,
    #[serde(deserialize_with = "read_retrieval_policies")]
    retrieval_policies: Vec<Shared<crate::unique_retrieval::UniqueRetrievalPolicy>>,
}
/// Actor identities use JSON object keys, preserving the existing directory
/// shape while replacing each complete value with its checkpoint-local index.
/// The custom reader rejects duplicates instead of silently overwriting them.
pub(crate) fn read_actor_directory<'de, T: Deserialize<'de>, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<ActorId, T>, D::Error> {
    struct Directory<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Directory<T> {
        type Value = BTreeMap<ActorId, T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("an object with unique nonzero actor identities")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            // Even the shortest nonzero JSON identity/index entry needs more
            // than four bytes; this cannot shrink the outer archive byte cap.
            let limit = ARCHIVE_BYTE_LIMIT / 4;
            if map.size_hint().is_some_and(|size| size > limit) {
                return Err(serde::de::Error::custom("actor directory exceeds limit"));
            }
            let mut directory = BTreeMap::new();
            while let Some((actor, value)) = map.next_entry::<ActorId, T>()? {
                if directory.len() >= limit {
                    return Err(serde::de::Error::custom("actor directory exceeds limit"));
                }
                if actor.0 == 0 || directory.insert(actor, value).is_some() {
                    return Err(serde::de::Error::custom(
                        "invalid or duplicate actor identity",
                    ));
                }
            }
            Ok(directory)
        }
    }
    deserializer.deserialize_map(Directory(std::marker::PhantomData))
}

fn read_actor_directories<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<BTreeMap<ActorId, usize>>, D::Error> {
    #[derive(Deserialize)]
    #[serde(transparent)]
    struct Directory(#[serde(deserialize_with = "read_actor_directory")] BTreeMap<ActorId, usize>);
    let directories =
        crate::bounded::Bounded::<Directory, MAX_CHECKPOINT_ROOTS>::deserialize(deserializer)?.0;
    let mut unique = BTreeSet::new();
    if directories
        .iter()
        .any(|directory| !unique.insert(&directory.0))
    {
        return Err(serde::de::Error::custom("duplicate actor directory"));
    }
    drop(unique);
    Ok(directories
        .into_iter()
        .map(|directory| directory.0)
        .collect())
}

fn read_population_bases<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<crate::floor_activation::ActivationState>, D::Error> {
    let states = Vec::<crate::floor_activation::ActivationState>::deserialize(d)?;
    if states.iter().any(|state| state.wandering.is_some()) {
        return Err(serde::de::Error::custom(
            "population clocks belong to each snapshot, not its shared base",
        ));
    }
    Ok(states)
}

fn read_retrieval_policies<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<Shared<crate::unique_retrieval::UniqueRetrievalPolicy>>, D::Error> {
    // One run has one immutable policy, even across rewind boundaries.
    crate::bounded::Bounded::<Shared<crate::unique_retrieval::UniqueRetrievalPolicy>, 1>::deserialize(d).map(|policies| policies.0)
}

/// Each directory is an ordered sequence of actor/root bindings. The pool may
/// contain one empty directory, but never duplicate directories or bindings.
mod explored_map_owners {
    use super::*;

    pub fn serialize<S: serde::Serializer>(
        directories: &[BTreeMap<ActorId, usize>],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(transparent)]
        struct Directory<'a>(
            #[serde(with = "tor_world::checkpoint_map")] &'a BTreeMap<ActorId, usize>,
        );
        serializer.collect_seq(directories.iter().map(Directory))
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<BTreeMap<ActorId, usize>>, D::Error> {
        #[derive(Deserialize)]
        #[serde(transparent)]
        struct Directory(#[serde(with = "tor_world::checkpoint_map")] BTreeMap<ActorId, usize>);
        let directories = Vec::<Directory>::deserialize(deserializer)?;
        let mut unique = BTreeSet::new();
        if directories.iter().any(|directory| {
            directory.0.keys().any(|actor| actor.0 == 0) || !unique.insert(&directory.0)
        }) {
            return Err(serde::de::Error::custom(
                "invalid or duplicate explored map owner directory",
            ));
        }
        drop(unique);
        Ok(directories
            .into_iter()
            .map(|directory| directory.0)
            .collect())
    }
}

impl SharedState {
    fn capture_actor(&mut self, id: ActorId, actor: &Shared<Actor>) -> usize {
        if let Some((source, index)) = self.actor_capture_sources.get(&id) {
            if source.matches(actor) {
                return *index;
            }
        }
        // Different allocations, including equal values after COW, still use
        // complete structural deduplication and the same bounded fingerprint.
        let index = self.actor_records.insert(ActorRecord::capture(actor));
        self.actor_capture_sources
            .insert(id, (actor.weak_identity(), index));
        index
    }

    fn capture_ai(&mut self, id: ActorId, ai: &crate::ai::Ai) -> usize {
        if let Some(&index) = self.ai_capture_sources.get(&id) {
            // Compare the complete value, not Ai's address or just its visits.
            // Equal shared region buckets avoid traversing unchanged entries.
            if self.ai_records.get(index) == Some(ai) {
                return index;
            }
        }
        let index = self.ai_records.insert(ai.clone());
        self.ai_capture_sources.insert(id, index);
        index
    }

    /// Whether every snapshot's expanded actor and AI record bindings fit the
    /// archive byte ceiling. Aliases within one snapshot are charged repeatedly;
    /// sharing across retained snapshots does not consume a common budget.
    /// Writers must check this before publishing a checkpoint, and readers before
    /// reconstructing actor payloads. Missing references are corruption, not size.
    ///
    /// # Errors
    /// Returns [`PersistError::RecordPool`] for malformed or unused record
    /// references; these must not be treated as capacity failures.
    pub fn record_expansion_within_limit(
        &self,
        snapshots: &[&Snapshot],
    ) -> Result<bool, PersistError> {
        self.record_expansion_with_budget(snapshots, ARCHIVE_BYTE_LIMIT)
    }

    fn record_expansion_with_budget(
        &self,
        snapshots: &[&Snapshot],
        limit: usize,
    ) -> Result<bool, PersistError> {
        // Reject malformed references before charging bytes, so corruption can
        // never masquerade as a deferrable checkpoint-capacity failure.
        if !self.record_references_valid(snapshots) {
            return Err(PersistError::RecordPool);
        }
        for snapshot in snapshots {
            let directory =
                self.actors
                    .get(snapshot.actors)
                    .ok_or(PersistError::MissingShared {
                        table: "actors",
                        index: snapshot.actors,
                    })?;
            let mut remaining = limit;
            for &index in directory.values() {
                let bytes =
                    self.actor_records
                        .encoded_bytes(index)
                        .map_err(|error| match error {
                            record_pool::SizeError::Missing => PersistError::MissingShared {
                                table: "actor_records",
                                index,
                            },
                            record_pool::SizeError::Serialization => PersistError::RecordPool,
                        })?;
                let Some(next) = remaining.checked_sub(bytes) else {
                    return Ok(false);
                };
                remaining = next;
            }
            for &index in snapshot.combat.ai.values() {
                let bytes = self
                    .ai_records
                    .encoded_bytes(index)
                    .map_err(|error| match error {
                        record_pool::SizeError::Missing => PersistError::MissingShared {
                            table: "ai_records",
                            index,
                        },
                        record_pool::SizeError::Serialization => PersistError::RecordPool,
                    })?;
                let Some(next) = remaining.checked_sub(bytes) else {
                    return Ok(false);
                };
                remaining = next;
            }
        }
        Ok(true)
    }

    /// Every actor/AI payload belongs to at least one retained snapshot, and
    /// every directory reference names a complete decoded record. This check
    /// binds the entire root set; checking only the current game is insufficient.
    fn record_references_valid(&self, snapshots: &[&Snapshot]) -> bool {
        let mut directories = BTreeSet::new();
        let mut actors = BTreeSet::new();
        let mut ai = BTreeSet::new();
        for snapshot in snapshots {
            let Some(directory) = self.actors.get(snapshot.actors) else {
                return false;
            };
            directories.insert(snapshot.actors);
            for (&id, &index) in directory {
                if id.0 == 0 || self.actor_records.encoded_bytes(index).is_err() {
                    return false;
                }
                actors.insert(index);
            }
            for (&id, &index) in &snapshot.combat.ai {
                if id.0 == 0
                    || !directory.contains_key(&id)
                    || self.ai_records.encoded_bytes(index).is_err()
                {
                    return false;
                }
                ai.insert(index);
            }
        }
        directories.len() == self.actors.len()
            && actors.len() == self.actor_records.len()
            && ai.len() == self.ai_records.len()
    }

    // Test-only reference path: clear hints without changing canonical records
    // or validation state, to compare exact serialized output against cold capture.
    #[cfg(test)]
    fn clear_capture_memos(&mut self) {
        self.actor_capture_sources.clear();
        self.ai_capture_sources.clear();
    }

    /// Prepare a decoded pool for strict canonical-shape serialization using the
    /// current snapshot and every retained history snapshot. This is not restore
    /// authority: each RestoreContext must still bind its actual roots. Callers
    /// must reprepare if the enclosing snapshot/root list changes; a failed
    /// preparation clears the prior serialization witness. Capture into decoded
    /// state remains unsupported and poisons complete serialization.
    pub fn prepare_spatial_serialization(
        &mut self,
        snapshots: &[&Snapshot],
    ) -> Result<(), PersistError> {
        // Even rejection before the codec sees roots must revoke prior proof.
        self.spatial.clear_serialization_witness();
        if snapshots.len() > MAX_CHECKPOINT_ROOTS {
            return Err(PersistError::Spatial);
        }
        let roots: Vec<_> = snapshots
            .iter()
            .map(|snapshot| snapshot.spatial)
            .chain(self.chart_history.spatial_indices())
            .collect();
        self.spatial
            .prepare_serialization(roots.into_iter())
            .map_err(|_| PersistError::Spatial)
    }

    pub fn chart_history_references_valid<'a>(
        &self,
        snapshots: impl IntoIterator<Item = &'a Snapshot>,
    ) -> bool {
        let snapshots: Vec<_> = snapshots.into_iter().collect();
        if snapshots.iter().any(|snapshot| {
            let Some(actors) = self.actors.get(snapshot.actors) else {
                return true;
            };
            self.chart_history
                .owner_ids(snapshot.chart_history)
                .is_none_or(|mut owners| owners.any(|id| !actors.contains_key(&id)))
        }) {
            return false;
        }
        self.chart_history
            .references_valid(snapshots.iter().map(|s| (s.chart_history, s.tick)))
    }
    pub fn world_references_valid<'a>(
        &self,
        snapshots: impl IntoIterator<Item = &'a Snapshot>,
    ) -> bool {
        let snapshots: Vec<_> = snapshots.into_iter().collect();
        if !self.chart_history_references_valid(snapshots.iter().copied()) {
            return false;
        }
        let used: BTreeSet<_> = snapshots
            .iter()
            .map(|s| s.world)
            .chain(self.chart_history.world_indices())
            .collect();
        used.len() == self.worlds.len()
            && used.iter().all(|index| *index < self.worlds.len())
            && !self.worlds.is_empty()
    }
    pub fn valid_world_count(&self, maximum: usize) -> bool {
        !self.worlds.is_empty() && self.worlds.len() <= maximum
    }

    /// Call with the current snapshot and every retained rewind snapshot before
    /// restoration. Each owner directory must be referenced, and every binding
    /// must name an actor present at that boundary and a referenced map root.
    pub fn explored_map_references_valid<'a>(
        &self,
        snapshots: impl IntoIterator<Item = &'a Snapshot>,
    ) -> bool {
        if !self.explored_map_owners_unique() {
            return false;
        }
        let mut used_owners = BTreeSet::new();
        let mut used_roots = BTreeSet::new();
        for snapshot in snapshots {
            let Some(owners) = self.explored_map_owners.get(snapshot.explored_maps) else {
                return false;
            };
            let Some(actors) = self.actors.get(snapshot.actors) else {
                return false;
            };
            used_owners.insert(snapshot.explored_maps);
            for (&actor, &index) in owners {
                if actor.0 == 0 || !actors.contains_key(&actor) || index >= self.explored_maps.len()
                {
                    return false;
                }
                used_roots.insert(index);
            }
        }
        used_owners.len() == self.explored_map_owners.len()
            && used_roots.len() == self.explored_maps.len()
    }

    fn explored_map_owners_unique(&self) -> bool {
        let mut unique = BTreeSet::new();
        self.explored_map_owners
            .iter()
            .all(|directory| unique.insert(directory))
    }
}

/// Ownership shared while restoring one decoded checkpoint and its rewind states.
/// This context is never serialized. Dropping it releases its temporary pool
/// references; restored games retain only the definitions they actually use.
pub struct RestoreContext<'a> {
    shared: &'a SharedState,
    spatial: Option<crate::spatial_scope::codec::ValidatedPool>,
    spatial_roots: BTreeMap<usize, Shared<crate::spatial_scope::registry::Registry>>,
    content: Arc<Content>,
    worlds: BTreeMap<usize, Shared<World>>,
    chart_history: crate::chart_history::RestoreCache,
    validated_chart_worlds: BTreeSet<usize>,
    navigation: BTreeMap<usize, Shared<Navigation>>,
    /// Validation is reusable only for these immutable decoded pool entries.
    validated_navigation: BTreeSet<(usize, usize)>,
    /// Plan validity depends only on this decoded pool and immutable Content.
    /// Snapshot-specific population validation is never cached.
    validated_population_plans: BTreeSet<usize>,
    #[cfg(test)]
    population_plan_validations: usize,
    unique_explored_map_owners: bool,
    explored_maps: Option<explored_map_regions::RestoreValidation>,
    items: BTreeMap<usize, crate::item_store::ItemStore>,
    actors: BTreeMap<(usize, usize), crate::actor_store::ActorStore>,
    // Successful ActorRecord restoration depends on its complete payload,
    // item table and this context's pinned content, not the binding ActorId.
    // Every boundary still validates actor/world/ownership consistency.
    actor_records: BTreeMap<(usize, usize), Shared<Actor>>,
    intentions: crate::intention::checkpoint::Restore,
    bodies: BTreeSet<Shared<crate::BodySpec>>,
    item_definitions: BTreeSet<Shared<crate::ItemSpec>>,
    anatomies: BTreeSet<Shared<crate::AnatomySpec>>,
}

impl<'a> RestoreContext<'a> {
    /// Validate and bind the complete current/retained snapshot root list once.
    ///
    /// Call this after [`Self::new`] and before [`Self::restore`]. Chart-history
    /// roots are included automatically. The successful binding belongs to this
    /// context and immutable decoded pool; it is not granted by preparing that
    /// pool for serialization.
    ///
    /// # Errors
    ///
    /// Rejects a second successful binding, an excessive root count, inconsistent
    /// chart-history references or a malformed spatial pool/root set. No restored
    /// game is published by this operation.
    pub fn bind_spatial_roots(&mut self, snapshots: &[&Snapshot]) -> Result<(), PersistError> {
        if self.spatial.is_some() || snapshots.len() > MAX_CHECKPOINT_ROOTS {
            return Err(PersistError::Spatial);
        }
        if !self.shared.record_expansion_within_limit(snapshots)? {
            return Err(PersistError::RecordPool);
        }
        if !self
            .shared
            .chart_history_references_valid(snapshots.iter().copied())
        {
            return Err(PersistError::ChartHistory);
        }
        let roots: Vec<_> = snapshots
            .iter()
            .map(|s| s.spatial)
            .chain(self.shared.chart_history.spatial_indices())
            .collect();
        let pool = self
            .shared
            .spatial
            .bind(roots.into_iter())
            .map_err(|_| PersistError::Spatial)?;
        self.spatial = Some(pool);
        Ok(())
    }
    fn spatial_root(
        &mut self,
        index: usize,
    ) -> Result<Shared<crate::spatial_scope::registry::Registry>, PersistError> {
        if let Some(root) = self.spatial_roots.get(&index) {
            return Ok(root.clone());
        }
        let root = self
            .spatial
            .as_ref()
            .ok_or(PersistError::Spatial)?
            .registry(index)
            .ok_or(PersistError::Spatial)?;
        self.spatial_roots.insert(index, root.clone());
        Ok(root)
    }

    /// Create an unbound restoration context for one immutable decoded pool.
    ///
    /// `content` must be the validated content pinned by the enclosing save.
    /// The host owns that identity check. Bind all snapshot roots with
    /// [`Self::bind_spatial_roots`] before restoring any game.
    pub fn new(shared: &'a SharedState, content: Arc<Content>) -> Self {
        Self {
            shared,
            spatial: None,
            spatial_roots: BTreeMap::new(),
            content,
            worlds: BTreeMap::new(),
            chart_history: crate::chart_history::RestoreCache::default(),
            validated_chart_worlds: BTreeSet::new(),
            navigation: BTreeMap::new(),
            validated_navigation: BTreeSet::new(),
            validated_population_plans: BTreeSet::new(),
            #[cfg(test)]
            population_plan_validations: 0,
            unique_explored_map_owners: shared.explored_map_owners_unique(),
            explored_maps: None,
            items: BTreeMap::new(),
            actors: BTreeMap::new(),
            actor_records: BTreeMap::new(),
            intentions: crate::intention::checkpoint::Restore::default(),
            bodies: BTreeSet::new(),
            item_definitions: BTreeSet::new(),
            anatomies: BTreeSet::new(),
        }
    }

    /// Restore and validate one snapshot using this context's already-bound pool.
    ///
    /// Shared immutable definitions may be reused across successful restores;
    /// each snapshot's mutable state and cross-references still need validation.
    ///
    /// # Errors
    ///
    /// Returns [`PersistError`] for missing/unbound shared roots or invalid state.
    /// An error returns no partially restored [`Game`].
    pub fn restore(&mut self, snapshot: Snapshot) -> Result<Game, PersistError> {
        Game::restore_with_context(snapshot, self)
    }

    #[cfg(test)]
    pub(crate) fn population_plan_validation_count(&self) -> usize {
        self.population_plan_validations
    }

    fn activation_state_valid(&mut self, game: &Game, index: Option<usize>) -> bool {
        let Some(index) = index else {
            return game.activation_state_valid();
        };
        if self.validated_population_plans.contains(&index) {
            return game.activation_boundary_valid();
        }
        #[cfg(test)]
        {
            self.population_plan_validations += 1;
        }
        if !game.activation_state_valid() {
            return false;
        }
        self.validated_population_plans.insert(index);
        true
    }

    fn floor_activation(
        &self,
        snapshot: PopulationSnapshot,
    ) -> Result<crate::floor_activation::ActivationState, PersistError> {
        let index = snapshot.state;
        // ActivationState's immutable fields already use Shared ownership.
        // Cloning the decoded pool value preserves those allocations across
        // restored games without caching boundary-dependent validation.
        let mut state = self.shared.floor_activations.get(index).cloned().ok_or(
            PersistError::MissingShared {
                table: "floor_activations",
                index,
            },
        )?;
        if state.wandering.is_some() {
            return Err(PersistError::Population);
        }
        state.wandering = snapshot.wandering;
        Ok(state)
    }

    fn world(&mut self, index: usize) -> Option<Shared<World>> {
        if let Some(world) = self.worlds.get(&index) {
            return Some(world.clone());
        }
        let world = Shared::new(self.shared.worlds.get(index)?.clone());
        self.worlds.insert(index, world.clone());
        Some(world)
    }

    fn navigation(&mut self, index: usize, world: usize) -> Option<Shared<Navigation>> {
        if !self.validated_navigation.contains(&(index, world)) {
            if !self
                .shared
                .navigation
                .get(index)?
                .checkpoint_valid(self.shared.worlds.get(world)?)
            {
                return None;
            }
            self.validated_navigation.insert((index, world));
        }
        if let Some(navigation) = self.navigation.get(&index) {
            return Some(navigation.clone());
        }
        let navigation = Shared::new(self.shared.navigation.get(index)?.clone());
        self.navigation.insert(index, navigation.clone());
        Some(navigation)
    }

    fn chart_history(
        &mut self,
        index: usize,
        actors: usize,
        tick: u64,
    ) -> Result<Shared<BTreeMap<ActorId, crate::chart_history::Owner>>, PersistError> {
        let shared = self.shared;
        let actors = shared
            .actors
            .get(actors)
            .ok_or(PersistError::ChartHistory)?;
        let mut cache = std::mem::take(&mut self.chart_history);
        let mut validated_worlds = std::mem::take(&mut self.validated_chart_worlds);
        let result = shared.chart_history.restore(
            index,
            tick,
            |id| actors.contains_key(&id),
            &mut cache,
            &mut validated_worlds,
            |world, spatial| {
                Ok(crate::chart_history::AcquisitionAuthority {
                    world: self
                        .world(world)
                        .ok_or(crate::chart_history::Error::Reference)?,
                    registry: self
                        .spatial_root(spatial)
                        .map_err(|_| crate::chart_history::Error::Reference)?,
                })
            },
        );
        self.chart_history = cache;
        self.validated_chart_worlds = validated_worlds;
        result.map_err(|_| PersistError::ChartHistory)
    }
    fn explored_maps(
        &mut self,
        index: usize,
        actors: usize,
        world: usize,
    ) -> Result<BTreeMap<ActorId, Shared<ExploredMap>>, PersistError> {
        if !self.unique_explored_map_owners {
            return Err(PersistError::ExploredMap);
        }
        let shared = self.shared;
        let owners = shared
            .explored_map_owners
            .get(index)
            .ok_or(PersistError::ExploredMap)?;
        let actors = shared
            .actors
            .get(actors)
            .ok_or(PersistError::MissingShared {
                table: "actors",
                index: actors,
            })?;
        if owners
            .keys()
            .any(|actor| actor.0 == 0 || !actors.contains_key(actor))
        {
            return Err(PersistError::ExploredMap);
        }
        owners
            .iter()
            .map(|(&actor, &root)| Some((actor, self.explored_map(root, world)?)))
            .collect::<Option<_>>()
            .ok_or(PersistError::ExploredMap)
    }

    fn explored_map(&mut self, index: usize, world: usize) -> Option<Shared<ExploredMap>> {
        if self.explored_maps.is_none() {
            self.explored_maps = Some(explored_map_regions::RestoreValidation::new_with_catalog(
                &self.shared.explored_maps,
                self.content.presentation_catalog(),
            )?);
        }
        self.explored_maps
            .as_mut()?
            .validate(index, self.shared.worlds.get(world)?)?;
        self.shared.explored_maps.get(index).cloned()
    }

    fn items(&mut self, index: usize) -> Result<crate::item_store::ItemStore, PersistError> {
        if let Some(items) = self.items.get(&index) {
            return Ok(items.clone());
        }
        let mut entries = self
            .shared
            .items
            .get(index)
            .ok_or(PersistError::MissingShared {
                table: "items",
                index,
            })?
            .clone();
        for (id, item) in &mut entries {
            item.spec
                .validate_rules(self.content.rules())
                .map_err(|error| PersistError::ItemDefinition { item: *id, error })?;
            share_definition(&mut self.item_definitions, &mut item.spec);
        }
        let items = crate::item_store::ItemStore::from_entries(entries);
        self.items.insert(index, items.clone());
        Ok(items)
    }

    fn actors(
        &mut self,
        index: usize,
        item_index: usize,
    ) -> Result<crate::actor_store::ActorStore, PersistError> {
        if let Some(actors) = self.actors.get(&(index, item_index)) {
            return Ok(actors.clone());
        }
        let records = self
            .shared
            .actors
            .get(index)
            .ok_or(PersistError::MissingShared {
                table: "actors",
                index,
            })?;
        let items = self
            .shared
            .items
            .get(item_index)
            .ok_or(PersistError::MissingShared {
                table: "items",
                index: item_index,
            })?;
        let mut entries = BTreeMap::new();
        for (&id, &record_index) in records {
            let key = (record_index, item_index);
            let actor = if let Some(actor) = self.actor_records.get(&key) {
                actor.clone()
            } else {
                let mut actor = self
                    .shared
                    .actor_records
                    .get(record_index)
                    .ok_or(PersistError::MissingShared {
                        table: "actor_records",
                        index: record_index,
                    })?
                    .restore(id, self.content.clone(), items)?;
                share_definition(&mut self.anatomies, &mut actor.anatomy);
                share_definition(&mut self.bodies, &mut actor.body);
                let actor = Shared::new(actor);
                self.actor_records.insert(key, actor.clone());
                actor
            };
            entries.insert(id, actor);
        }
        let actors = crate::actor_store::ActorStore::from_shared_entries(entries);
        self.actors.insert((index, item_index), actors.clone());
        Ok(actors)
    }
}

/// Compare the complete value, never its identity label or allocation address.
/// Borrowing the key avoids copying a definition merely to look it up.
fn share_definition<T: Ord>(pool: &mut BTreeSet<Shared<T>>, definition: &mut Shared<T>) {
    if let Some(existing) = pool.get(&**definition) {
        *definition = existing.clone();
    } else {
        pool.insert(definition.clone());
    }
}

impl Game {
    /// Capture a settled backend state, reusing equal entries in a fresh capture pool.
    ///
    /// Keep the returned [`Snapshot`] with `shared`. Reuse that pool to capture
    /// related history states, not to append to a deserialized pool. This method
    /// neither writes storage nor establishes a server durability barrier.
    ///
    /// # Panics
    ///
    /// Panics while a region transition, trap resolution, body-return finalization,
    /// host continuation or unsealed event observation remains pending. A host
    /// must settle and seal its private candidate before capture.
    pub fn checkpoint(&self, shared: &mut SharedState) -> Snapshot {
        assert!(
            !self.region_transition_pending()
                && self.trap_resolution.is_none()
                && self.body_return_finalization.settled()
                && !self.host_region_continuation
                && !self.event_observations_open(),
            "a private host candidate must settle and seal events before checkpoint capture"
        );
        let worlds = &mut shared.worlds;
        let world = worlds
            .iter()
            .position(|w| w == &*self.world)
            .unwrap_or_else(|| {
                worlds.push((*self.world).clone());
                worlds.len() - 1
            });
        let chart_history = shared.chart_history.capture(
            &self.chart_history,
            &mut shared.worlds,
            &mut shared.spatial,
        );
        let actor_records: BTreeMap<_, _> = self
            .actors
            .checkpoint_sources()
            .map(|(&id, actor)| (id, shared.capture_actor(id, actor)))
            .collect();
        let explored_map_owners: BTreeMap<_, _> = self
            .explored_maps
            .iter()
            .map(|(actor, map)| {
                let index = shared
                    .explored_maps
                    .iter()
                    .position(|existing| existing.shares_storage(map) || existing == map)
                    .unwrap_or_else(|| {
                        shared.explored_maps.push(map.clone());
                        shared.explored_maps.len() - 1
                    });
                (*actor, index)
            })
            .collect();
        Snapshot {
            spatial: shared.spatial.capture_snapshot(&self.spatial_registry),
            chart_history,
            combat: self.combat.with_ai(
                self.combat
                    .ai
                    .iter()
                    .map(|(&id, ai)| (id, shared.capture_ai(id, ai)))
                    .collect(),
            ),
            rules_rng: self.rules_rng,
            events: self.event_log.clone(),
            event_receipts: self.event_receipts.clone(),
            physics: self.physics.clone(),
            world,
            navigation: self
                .navigation
                .iter()
                .map(|(actor, navigation)| {
                    let index = shared
                        .navigation
                        .iter()
                        .position(|n| n == &**navigation)
                        .unwrap_or_else(|| {
                            shared.navigation.push((**navigation).clone());
                            shared.navigation.len() - 1
                        });
                    (*actor, index)
                })
                .collect(),
            explored_maps: shared
                .explored_map_owners
                .iter()
                .position(|owners| owners == &explored_map_owners)
                .unwrap_or_else(|| {
                    shared.explored_map_owners.push(explored_map_owners);
                    shared.explored_map_owners.len() - 1
                }),
            seed: self.seed,
            tick: self.tick,
            actors: shared
                .actors
                .iter()
                .position(|actors| actors == &actor_records)
                .unwrap_or_else(|| {
                    shared.actors.push(actor_records);
                    shared.actors.len() - 1
                }),
            items: shared
                .items
                .iter()
                .position(|items| items == &*self.items)
                .unwrap_or_else(|| {
                    shared.items.push((*self.items).clone());
                    shared.items.len() - 1
                }),
            next_actor_id: self.next_actor_id,
            next_item_id: self.next_item_id,
            next_door_id: self.next_door_id,
            next_hold_id: self.next_hold_id,
            intentions: shared.intentions.capture(&self.intentions),
            scenario_policy: (self.scenario_policy != Default::default())
                .then(|| self.scenario_policy.clone()),
            floor_activation: self.floor_activation.as_ref().map(|state| {
                let mut base = state.clone();
                let wandering = base.wandering.take();
                let states = &mut shared.floor_activations;
                let state = states
                    .iter()
                    .position(|pooled| pooled == &base)
                    .unwrap_or_else(|| {
                        states.push(base);
                        states.len() - 1
                    });
                PopulationSnapshot { state, wandering }
            }),
            unique_retrieval: self.unique_retrieval.as_ref().map(|state| {
                let policies = &mut shared.retrieval_policies;
                let policy = policies
                    .iter()
                    .position(|policy| **policy == *state.policy)
                    .unwrap_or_else(|| {
                        policies.push(state.policy.clone());
                        policies.len() - 1
                    });
                RetrievalSnapshot {
                    policy,
                    initialized: state.initialized,
                    item: state.item,
                    entry_count: state.entry_count,
                    placement_count: state.placement_count,
                    entries: state.entries.clone(),
                }
            }),
            script: self.script.clone(),
            planes: self.planes.clone(),
            lifecycle: (!self.lifecycle.is_empty()).then(|| {
                let lifecycles = &mut shared.lifecycles;
                lifecycles
                    .iter()
                    .position(|l| l == &self.lifecycle)
                    .unwrap_or_else(|| {
                        lifecycles.push(self.lifecycle.clone());
                        lifecycles.len() - 1
                    })
            }),
        }
    }

    /// Restore a single-snapshot pool with the save's validated, pinned content.
    ///
    /// For a pool shared by current and retained history snapshots, use one
    /// [`RestoreContext`] and bind the complete root list instead.
    ///
    /// # Errors
    ///
    /// Returns [`PersistError`] if root binding or snapshot validation fails.
    /// The enclosing host is responsible for verifying content/save identities.
    pub fn restore_checkpoint(
        snapshot: Snapshot,
        shared: &SharedState,
        content: Arc<Content>,
    ) -> Result<Self, PersistError> {
        let mut context = RestoreContext::new(shared, content);
        context.bind_spatial_roots(&[&snapshot])?;
        context.restore(snapshot)
    }

    fn restore_with_context(
        snapshot: Snapshot,
        context: &mut RestoreContext<'_>,
    ) -> Result<Self, PersistError> {
        let items = context.items(snapshot.items)?;
        let population_base = snapshot.floor_activation.as_ref().map(|state| state.state);
        let mut game = Self {
            content: context.content.clone(),
            combat: snapshot.combat.with_ai(
                snapshot
                    .combat
                    .ai
                    .iter()
                    .map(|(&id, &index)| {
                        context
                            .shared
                            .ai_records
                            .get(index)
                            .cloned()
                            .map(|record| (id, record))
                            .ok_or(PersistError::MissingShared {
                                table: "ai_records",
                                index,
                            })
                    })
                    .collect::<Result<_, _>>()?,
            ),
            rules_rng: snapshot.rules_rng,
            event_log: snapshot.events,
            event_receipts: snapshot.event_receipts,
            event_origins: Default::default(),
            combat_diagnostics: None,
            survival_diagnostics: None,
            physics: snapshot.physics,
            world: context
                .world(snapshot.world)
                .ok_or(PersistError::MissingShared {
                    table: "worlds",
                    index: snapshot.world,
                })?,
            navigation: snapshot
                .navigation
                .into_iter()
                .map(|(actor, index)| Some((actor, context.navigation(index, snapshot.world)?)))
                .collect::<Option<_>>()
                .ok_or(PersistError::Navigation)?,
            explored_maps: context.explored_maps(
                snapshot.explored_maps,
                snapshot.actors,
                snapshot.world,
            )?,
            chart_history: context.chart_history(
                snapshot.chart_history,
                snapshot.actors,
                snapshot.tick,
            )?,
            map_validation: Default::default(),
            map_scenes: Default::default(),
            spatial_registry: context.spatial_root(snapshot.spatial)?,
            seed: snapshot.seed,
            tick: snapshot.tick,
            actors: context.actors(snapshot.actors, snapshot.items)?,
            items,
            next_actor_id: snapshot.next_actor_id,
            next_item_id: snapshot.next_item_id,
            next_door_id: snapshot.next_door_id,
            next_hold_id: snapshot.next_hold_id,
            intentions: context
                .intentions
                .restore(snapshot.intentions, &context.shared.intentions)
                .ok_or(PersistError::Intentions)?,
            scenario_policy: snapshot.scenario_policy.unwrap_or_default(),
            authored_teleport_levels: crate::AuthoredTeleportLevels::default(),
            floor_activation: snapshot
                .floor_activation
                .map(|index| context.floor_activation(index))
                .transpose()?,
            unique_retrieval: snapshot
                .unique_retrieval
                .map(|state| {
                    let policy = context
                        .shared
                        .retrieval_policies
                        .get(state.policy)
                        .cloned()
                        .ok_or(PersistError::MissingShared {
                            table: "retrieval_policies",
                            index: state.policy,
                        })?;
                    Ok(crate::unique_retrieval::UniqueRetrievalState {
                        policy,
                        initialized: state.initialized,
                        item: state.item,
                        entry_count: state.entry_count,
                        placement_count: state.placement_count,
                        entries: state.entries,
                    })
                })
                .transpose()?,
            population_entry_boundary: None,
            preparation_region_boundary: None,
            preparation_facts: crate::preparation_facts::PreparationFacts::default(),
            trap_region_boundary: None,
            light_region_boundary: None,
            light_settled_witness: None,
            trap_resolution: None,
            body_return_finalization: Default::default(),
            host_region_continuation: false,
            hook_mask: snapshot.script.as_ref().map_or(0, |state| state.mask()),
            script: snapshot.script,
            planes: match snapshot.planes {
                Some(state) if !state.valid() => return Err(PersistError::World),
                planes => planes,
            },
            hook_port: Default::default(),
            script_report: Default::default(),
            lifecycle: match snapshot.lifecycle {
                None => Lifecycle::default(),
                // An empty state is always omitted, so encodings stay unique.
                Some(index) => context
                    .shared
                    .lifecycles
                    .get(index)
                    .filter(|l| !l.is_empty())
                    .cloned()
                    .ok_or(PersistError::Lifecycle)?,
            },
        };
        if !game.retrieval_state_valid() {
            return Err(PersistError::Retrieval);
        }
        if !context.activation_state_valid(&game, population_base) {
            return Err(PersistError::Population);
        }
        if !game.intentions_valid() {
            return Err(PersistError::Intentions);
        }
        if !game.lifecycle_state_valid() {
            return Err(PersistError::Lifecycle);
        }
        if !game.item_claims_valid(false) {
            return Err(PersistError::Intentions);
        }
        game.rebuild_timer_activity();
        if !game.physics_valid() {
            return Err(PersistError::Physics);
        }
        if game.event_log.iter().any(|event| match event {
            crate::combat::CombatEvent::EscapeResolved { actor, hold, intention, .. } => actor.0 == 0
                || actor.0 >= game.next_actor_id || hold.0 == 0 || hold.0 >= game.next_hold_id
                || intention.is_some_and(|id| !game.intention_issued(id)),
            crate::combat::CombatEvent::Resolved { outcome, strike, .. } => !outcome.valid()
                || (outcome.target_rider_participated && !strike.is_some_and(|ordinal| usize::from(ordinal) < crate::limits::STRIKES))
                || outcome.effects.iter().any(|effect| {
                matches!(effect, crate::abilities::ResolvedEffect::ObservePublicPlan { observation }
                    if observation.observed_at > game.tick || !observation.valid(&game.content.rules, &game.world))
            }),
            _ => false,
        }) {
            return Err(PersistError::RunState);
        }
        if !game.event_receipts_valid() {
            return Err(PersistError::RunState);
        }
        if !game.combat_valid() {
            return Err(PersistError::RunState);
        }
        if (game.actors.is_empty() && !game.has_detached_actors())
            || game.next_actor_id == 0
            || game.next_item_id == 0
            || game.next_door_id == 0
            || game.next_hold_id == 0
            || game
                .next_actor()
                .is_some_and(|id| game.actors[&id].ready_at != game.tick)
        {
            return Err(PersistError::Schedule);
        }
        let mut occupied = BTreeSet::new();
        for (id, actor) in game.actors.iter() {
            if actor.active_since > game.actor_clock(*id) {
                return Err(PersistError::ActorClock { actor: *id });
            }
            if !game.actor_state_valid(*id, actor) || !game.body_has_room(actor, &mut occupied) {
                return Err(PersistError::ActorState { actor: *id });
            }
        }
        for (id, item) in game.items.iter() {
            if !game.item_state_valid(*id, item) {
                return Err(PersistError::ItemState { item: *id });
            }
        }
        if game
            .navigation
            .keys()
            .any(|id| !game.actors.contains_key(id))
        {
            return Err(PersistError::Navigation);
        }
        if game
            .explored_maps
            .keys()
            .any(|id| !game.actors.contains_key(id))
        {
            return Err(PersistError::ExploredMap);
        }
        if !game.world.checkpoint_valid(game.next_door_id) || !game.loaded_traps_valid() {
            return Err(PersistError::World);
        }
        if !game.loaded_holds_valid() {
            return Err(PersistError::RunState);
        }
        if !game.recovery_contexts_valid() {
            return Err(PersistError::Schedule);
        }
        game.remember_recovery_contexts();
        for (actor, map) in &game.explored_maps {
            game.map_validation.accept_loaded(*actor, map);
        }
        Ok(game)
    }

    /// Checks restoring gives a loaded actor, apart from its body's room;
    /// attaching a stored region gives its actors the same.
    pub(crate) fn actor_state_valid(&self, id: ActorId, actor: &Actor) -> bool {
        id.0 != 0
            && actor
                .knowledge
                .iter()
                .all(|identity| !identity.is_unknown() && identity.as_str().split('.').count() >= 3)
            && crate::hallucination_clock::valid_for(actor)
            && actor.periodic_relocations.valid_for(actor, &self.items)
            && actor
                .guise
                .valid_for(&actor.perception(), actor.pending.as_ref(), actor.alive())
            && self.hold_actor_shape_valid(id, actor)
            && actor.active_since <= self.actor_clock(id)
            && actor.motion.active_fall.is_none_or(|fall| {
                actor.alive()
                    && fall.owner.0 > 0
                    && fall.owner.0 < self.next_actor_id
                    && fall.episode < self.physics.next_fall_episode
                    && fall.started_at <= self.tick
            })
            && actor.combat.as_ref().is_none_or(|combat| {
                combat
                    .creature()
                    .condition_horizon()
                    .is_none_or(|horizon| actor.active_since.checked_add(horizon).is_some())
                    && combat.creature().recovery_context_valid_at(
                        self.actor_frozen(id).then(|| self.actor_clock(id)),
                        actor.active_since,
                    )
            })
            && actor.pending.as_ref().is_none_or(|p| {
                actor.alive()
                    && p.plan.actor() == id
                    && p.plan.valid(&self.content.rules)
                    && self.plan_source_valid(&p.plan)
                    && p.progress
                        .hold_binding()
                        .is_none_or(|binding| binding.id.0 < self.next_hold_id)
                    && p.schedule_valid(self.actor_clock(id), actor.ready_at)
                    && (!p.active
                        || actor.combat.as_ref().is_none_or(|combat| {
                            !combat
                                .creature()
                                .condition_pauses_preparation(&p.plan.work().action())
                        }))
                    && p.plan.target().0 < self.next_actor_id
                    && (!(p.plan.ability_id().is_some()
                        || matches!(
                            p.plan,
                            crate::abilities::Plan::Escape(_) | crate::abilities::Plan::Respec(_)
                        ))
                        || actor.combat.as_ref().map(|c| c.creature()).is_some())
            })
            && actor.combat.as_ref().is_none_or(|c| c.valid())
            && actor
                .combat
                .as_ref()
                .map(|combat| combat.creature())
                .is_none_or(|creature| {
                    creature.derived().anatomy == *actor.anatomy
                        && creature.build().authored_body_packages_valid()
                        && creature
                            .build()
                            .active_body_package()
                            .is_none_or(|package| package.body == *actor.body)
                        && (!creature.polymorph_return_pending()
                            || (creature.cancelled()
                                && !creature.health().dead()
                                && creature
                                    .build()
                                    .base_body_package()
                                    .zip(creature.build().active_body_package())
                                    .is_some_and(|(base, active)| base.id != active.id)))
                        && match actor.pending.as_ref() {
                            Some(preparation) => {
                                let plan = &preparation.plan;
                                let linked = plan.actor() == id
                                    && plan.valid(creature.build().ruleset())
                                    && match plan {
                                        crate::abilities::Plan::Respec(plan) => {
                                            plan.matches_build(creature)
                                        }
                                        _ => true,
                                    }
                                    && plan.target().0 > 0
                                    && plan.target().0 < self.next_actor_id;
                                let owned = preparation
                                    .intention
                                    .is_none_or(|latest| self.intention_issued(latest))
                                    && preparation.origin.is_none_or(|origin| {
                                        self.intention_issued(origin)
                                            && preparation
                                                .intention
                                                .is_some_and(|latest| origin <= latest)
                                    });
                                let funded = match plan.cost().filter(|_| !preparation.has_prefix())
                                {
                                    Some(charge) => preparation.hold.is_some_and(|owner| {
                                        self.intention_issued(owner)
                                            && preparation.origin == Some(owner)
                                            && preparation
                                                .intention
                                                .is_some_and(|latest| owner <= latest)
                                            && creature.costs().reservations().len() == 1
                                            && creature.costs().reservation(owner) == Some(charge)
                                    }),
                                    None => {
                                        creature.costs().reservations().is_empty()
                                            && preparation.hold.is_none()
                                    }
                                };
                                linked && owned && funded
                            }
                            None => creature.costs().reservations().is_empty(),
                        }
                        && creature.conditions().instances().all(|entry| {
                            self.actors.contains_key(&entry.source)
                                || self.detached_actor(entry.source)
                        })
                })
            && actor.readied_ambush.toucher().is_none_or(|toucher| {
                toucher != id
                    && (self.actors.contains_key(&toucher) || self.detached_actor(toucher))
            })
            && actor.discovery.valid()
            && actor
                .discovery
                .features()
                .all(|feature| self.knows_concealed_feature(feature))
            && actor
                .discovery
                .guise_subjects()
                .all(|subject| self.actors.contains_key(&subject) || self.detached_actor(subject))
            && self.awareness_valid(id, actor)
            && self.control_valid(id, actor)
            && actor.search_clock.valid_for(actor)
            && self.spatial_condition_anchors_valid(actor)
            && actor
                .trap_contacts
                .ids()
                .all(|trap| self.knows_concealed_feature(tor_world::ConcealedFeatureId::Trap(trap)))
            && self.magic_knowledge_valid(id)
            && id.0 < self.next_actor_id
            && actor.recognized_abilities.len() <= crate::limits::CATALOG_ENTRIES
            && actor
                .recognized_abilities
                .iter()
                .all(|ability| self.content.rules.ability(ability).is_some())
            && actor.combat.as_ref().is_none_or(|combat| {
                combat
                    .creature()
                    .derived()
                    .abilities
                    .iter()
                    .all(|ability| actor.recognized_abilities.contains(ability))
            })
            && actor.public_plan.as_ref().is_none_or(|record| {
                record.observed_at <= self.tick
                    && record.valid(&self.content.rules, &self.world)
                    && match &record.state {
                        crate::public_plan::PublicPlanState::Preparing {
                            ability: Some(ability),
                            ..
                        } => actor.recognized_abilities.contains(&ability.id),
                        _ => true,
                    }
            })
            && actor.orientation < 24
            && (!actor.alive()
                || actor.ready_at >= self.actor_clock(id)
                || self
                    .condition_restricts(id, crate::rules::ConditionRestriction::VoluntaryActions))
            && self.world.contains(actor.location)
            && actor.visited.iter().all(|id| self.world.knows_region(*id))
            && actor.body.valid()
            && actor.motion.valid()
            && actor
                .motion
                .acceleration_remainder
                .iter()
                .all(|v| v.unsigned_abs() < actor.body.cells.len() as u64)
    }

    /// Whether a living actor's body fits walkable cells that no body in
    /// `occupied` holds, adding its cells. Only call this for an actor whose
    /// state is valid.
    pub(crate) fn body_has_room(&self, actor: &Actor, occupied: &mut BTreeSet<Location>) -> bool {
        !actor.alive()
            || self
                .body_cells(actor.location, actor.orientation, &actor.body)
                .is_some_and(|cells| {
                    cells
                        .iter()
                        .all(|(at, _)| self.world.walkable(*at) && occupied.insert(*at))
                })
    }

    /// Checks restoring gives a loaded item; attaching a stored region gives
    /// its items the same.
    pub(crate) fn item_state_valid(&self, id: ItemId, item: &Item) -> bool {
        if !item.condition_valid()
            || !self.relocation_item_state_valid(id, item)
            || item.charges.is_some() != item.spec.device.is_some()
            || item
                .spec
                .device
                .as_ref()
                .is_some_and(|d| item.charges.is_none_or(|c| c > d.initial_charges.maximum))
        {
            return false;
        }
        id.0 != 0
            && id.0 < self.next_item_id
            && item.quantity != 0
            && item.holds_valid()
            && (item.spec.stackable || item.quantity == 1)
            && item.spec.valid()
            && crate::portable_light::valid_item(item)
            && item.corpse.as_ref().is_none_or(|corpse| {
                item.spec.class == crate::ItemClass::Corpse
                    && !item.spec.stackable
                    && corpse.actor.0 != 0
                    && corpse.actor.0 < self.next_actor_id
            })
            && item.spec.validate_rules(self.content.rules()).is_ok()
            && item.motion.valid()
            && item.motion.active_fall.is_none()
            && item.motion.acceleration_remainder == [0; 3]
            && item.orientation < 24
            && match item.location {
                ItemLocation::Ground(location) => self.world.contains(location),
                ItemLocation::Carried(actor) => self.actors.get(&actor).is_some_and(|owner| {
                    !item.fuel.is_some_and(|fuel| fuel.lit)
                        || (owner.alive() && owner.equipment.values().any(|held| *held == id))
                }),
            }
    }

    /// Every actor a checkpoint's revisions cover: loaded, detached or not
    /// built yet.
    pub fn checkpoint_actor_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.known_actor_ids().into_iter().map(|id| id.0)
    }

    /// Whether an actor is in a loaded region.
    pub fn has_actor(&self, id: ActorId) -> bool {
        self.actors.contains_key(&id)
    }
}

#[cfg(test)]
#[path = "checkpoint_tests.rs"]
mod tests;

/// Format-6 navigation pools retain sharing when decoded, including between
/// independently equal region maps. No historical-format fallback is accepted.
mod navigation_regions {
    use super::*;
    use crate::navigation_map::RegionMap;
    use crate::travel::KnownEdge;
    use serde::{Deserializer, Serializer};
    use tor_world::{Direction, Location, RegionId};

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Instance {
        places: Vec<usize>,
        cells: Vec<usize>,
        edges: Vec<usize>,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Regions {
        places: Vec<RegionMap<Location, crate::PlaceName>>,
        cells: Vec<CellRegion>,
        edges: Vec<EdgeRegion>,
        instances: Vec<Instance>,
    }

    type Edges = RegionMap<(Location, Direction), KnownEdge>;
    type Cells = RegionMap<Location, bool>;
    /// Ordered x/y columns with explicit [z, value] entries. No implicit runs
    /// or unbounded expansion: each decoded position has an encoded entry.
    type Columns = Vec<(i32, i32, Vec<[i32; 2]>)>;

    fn columns(entries: impl IntoIterator<Item = (tor_world::Position, i32)>) -> Columns {
        let mut columns = Columns::new();
        for (p, value) in entries {
            match columns.last_mut() {
                Some((x, y, values)) if (*x, *y) == (p.x, p.y) => values.push([p.z, value]),
                _ => columns.push((p.x, p.y, vec![[p.z, value]])),
            }
        }
        columns
    }

    fn expand_columns(columns: Columns) -> Option<Vec<(tor_world::Position, i32)>> {
        let mut entries = Vec::new();
        let mut previous = None;
        for (x, y, values) in columns {
            if values.is_empty() || previous.is_some_and(|p| p >= (x, y)) {
                return None;
            }
            previous = Some((x, y));
            let mut last_z = None;
            for [z, value] in values {
                if last_z.is_some_and(|last| last >= z) {
                    return None;
                }
                last_z = Some(z);
                entries.push((tor_world::Position { x, y, z }, value));
            }
        }
        Some(entries)
    }

    /// Remembered opacity is independent of current terrain. Store the region
    /// once per shared pool entry and retain each cell's exact stale value.
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct CellRegion {
        region: RegionId,
        cells: Columns,
    }

    impl CellRegion {
        fn encode(map: &Cells) -> Self {
            Self {
                region: map
                    .iter()
                    .next()
                    .expect("pooled regions are nonempty")
                    .0
                    .region,
                cells: columns(
                    map.iter()
                        .map(|(location, opaque)| (location.position, i32::from(*opaque))),
                ),
            }
        }

        fn decode(self) -> Option<Cells> {
            if self.cells.is_empty() {
                return None;
            }
            let mut map = Cells::default();
            for (position, opaque) in expand_columns(self.cells)? {
                if !(0..=1).contains(&opaque) {
                    return None;
                }
                map.insert(
                    Location {
                        region: self.region,
                        position,
                    },
                    opaque == 1,
                );
            }
            Some(map)
        }
    }

    /// Link directions, in mask bit order.
    const DIRECTIONS: [Direction; 6] = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
        Direction::Up,
        Direction::Down,
    ];

    /// One pooled region of known links. Most links are plain: the ordinary
    /// neighbour in the same region with no rotation, fully determined by the
    /// start cell and direction. Those use coordinate columns and a mask with bit
    /// `i` for `DIRECTIONS[i]`; only the rest are stored in full. Decoding
    /// accepts exactly one encoding per map.
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct EdgeRegion {
        region: RegionId,
        plain: Columns,
        other: Vec<((Location, Direction), KnownEdge)>,
    }

    fn plain_target(from: Location, direction: Direction) -> Option<Location> {
        let (dx, dy, dz) = direction.delta();
        let p = from.position;
        Some(Location {
            position: tor_world::Position {
                x: p.x.checked_add(dx)?,
                y: p.y.checked_add(dy)?,
                z: p.z.checked_add(dz)?,
            },
            ..from
        })
    }

    fn is_plain(from: Location, direction: Direction, to: Location, turns: u8) -> bool {
        turns == 0 && plain_target(from, direction) == Some(to)
    }

    impl EdgeRegion {
        /// `map` is a pooled single-region map.
        fn encode(map: &Edges) -> Self {
            let mut plain = BTreeMap::<tor_world::Position, i32>::new();
            let mut other = Vec::new();
            let mut region = None;
            for (&(from, direction), &edge) in map.iter() {
                let (to, turns) = (edge.destination, edge.rotation);
                region = Some(from.region);
                match DIRECTIONS.iter().position(|d| *d == direction) {
                    Some(bit) if is_plain(from, direction, to, turns) => {
                        *plain.entry(from.position).or_default() |= (1 << bit)
                            | if edge.requires_flight {
                                1 << (bit + 6)
                            } else {
                                0
                            };
                    }
                    _ => other.push(((from, direction), edge)),
                }
            }
            Self {
                region: region.expect("pooled regions are nonempty"),
                plain: columns(plain),
                other,
            }
        }

        fn decode(self) -> Option<Edges> {
            let mut map = Edges::default();
            let mut count = 0usize;
            for (position, mask) in expand_columns(self.plain)? {
                if !(1..4096).contains(&mask)
                    || ((mask >> 6) & !(mask & 63)) != 0
                    || ((mask >> 6) & 15) != 0
                {
                    return None;
                }
                let from = Location {
                    region: self.region,
                    position,
                };
                for (bit, direction) in DIRECTIONS.into_iter().enumerate() {
                    if mask & (1 << bit) != 0 {
                        map.insert(
                            (from, direction),
                            KnownEdge {
                                destination: plain_target(from, direction)?,
                                rotation: 0,
                                requires_flight: mask & (1 << (bit + 6)) != 0,
                            },
                        );
                        count += 1;
                    }
                }
            }
            let mut previous = None;
            for ((from, direction), edge) in self.other {
                let (to, turns) = (edge.destination, edge.rotation);
                if from.region != self.region
                    || turns >= 24
                    || !DIRECTIONS.contains(&direction)
                    || (edge.requires_flight
                        && !matches!(direction, Direction::Up | Direction::Down))
                    || previous.is_some_and(|key| key >= (from, direction))
                    || (DIRECTIONS.contains(&direction) && is_plain(from, direction, to, turns))
                    || map.contains_key(&(from, direction))
                {
                    return None;
                }
                previous = Some((from, direction));
                map.insert((from, direction), edge);
                count += 1;
            }
            (map.iter().count() == count).then_some(map)
        }
    }

    pub(super) fn serialize<S: Serializer>(
        navigation: &[Navigation],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut saved = Regions {
            places: Vec::new(),
            cells: Vec::new(),
            edges: Vec::new(),
            instances: Vec::new(),
        };
        let mut places = BTreeMap::new();
        let mut cells = BTreeMap::new();
        let mut edges = BTreeMap::new();
        let mut edge_pool = Vec::new();
        let mut cell_pool = Vec::new();
        for map in navigation {
            saved.instances.push(Instance {
                places: map
                    .places
                    .checkpoint_regions(&mut saved.places, &mut places),
                cells: map.cells.checkpoint_regions(&mut cell_pool, &mut cells),
                edges: map.edges.checkpoint_regions(&mut edge_pool, &mut edges),
            });
        }
        saved.edges = edge_pool.iter().map(EdgeRegion::encode).collect();
        saved.cells = cell_pool.iter().map(CellRegion::encode).collect();
        saved.serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<Navigation>, D::Error> {
        let saved = Regions::deserialize(deserializer)?;
        let Some(cell_pool) = saved
            .cells
            .into_iter()
            .map(CellRegion::decode)
            .collect::<Option<Vec<_>>>()
        else {
            return Err(serde::de::Error::custom(
                "invalid checkpoint navigation cells",
            ));
        };
        let Some(edge_pool) = saved
            .edges
            .into_iter()
            .map(EdgeRegion::decode)
            .collect::<Option<Vec<_>>>()
        else {
            return Err(serde::de::Error::custom(
                "invalid checkpoint navigation links",
            ));
        };
        if saved.places.iter().any(|map| !map.is_checkpoint_region())
            || cell_pool.iter().any(|map| !map.is_checkpoint_region())
            || edge_pool.iter().any(|map| !map.is_checkpoint_region())
        {
            return Err(serde::de::Error::custom(
                "invalid checkpoint navigation region",
            ));
        }
        saved
            .instances
            .into_iter()
            .map(|instance| {
                let places = RegionMap::restore_regions(&instance.places, &saved.places);
                let cells = RegionMap::restore_regions(&instance.cells, &cell_pool);
                let edges = RegionMap::restore_regions(&instance.edges, &edge_pool);
                match (cells, edges, places) {
                    (Some(cells), Some(edges), Some(places)) => Ok(Navigation {
                        cells,
                        edges,
                        places,
                    }),
                    _ => Err(serde::de::Error::custom(
                        "invalid checkpoint navigation reference",
                    )),
                }
            })
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use tor_world::Position;

        fn at(region: u64, x: i32, y: i32, z: i32) -> Location {
            Location {
                region: RegionId(region),
                position: Position { x, y, z },
            }
        }

        #[test]
        fn remembered_opacity_round_trips_in_compact_cells() {
            let mut map = Cells::default();
            map.insert(at(7, -1, 2, 3), false);
            map.insert(at(7, 4, 5, 6), true);
            let encoded = CellRegion::encode(&map);
            assert_eq!(encoded.region, RegionId(7));
            assert_eq!(
                encoded.cells,
                vec![(-1, 2, vec![[3, 0]]), (4, 5, vec![[6, 1]])]
            );
            assert_eq!(encoded.decode(), Some(map));
        }

        #[test]
        fn coordinate_columns_compact_dense_cells_without_implicit_expansion() {
            let entries: Vec<_> = (-4..=4)
                .flat_map(|x| {
                    (-2..=2).flat_map(move |y| (-1..=8).map(move |z| (Position { x, y, z }, z & 1)))
                })
                .collect();
            let encoded = columns(entries.clone());
            assert_eq!(encoded.len(), 45);
            let flat: Vec<_> = entries
                .iter()
                .map(|(p, value)| [p.x, p.y, p.z, *value])
                .collect();
            assert!(
                serde_json::to_vec(&encoded).unwrap().len()
                    < serde_json::to_vec(&flat).unwrap().len()
            );
            assert_eq!(expand_columns(encoded), Some(entries));
            for malformed in [
                vec![(0, 0, vec![])],
                vec![(0, 0, vec![[0, 0]]), (0, 0, vec![[1, 1]])],
                vec![(0, 1, vec![[0, 0]]), (0, 0, vec![[0, 0]])],
                vec![(0, 0, vec![[1, 0], [0, 0]])],
                vec![(0, 0, vec![[0, 0], [0, 1]])],
            ] {
                assert!(expand_columns(malformed).is_none());
            }
        }

        #[test]
        fn malformed_or_noncanonical_remembered_cells_are_rejected() {
            for rows in [
                vec![],
                vec![[0, 0, 0, -1]],
                vec![[0, 0, 0, 2]],
                vec![[0, 0, 0, 0], [0, 0, 0, 1]],
                vec![[1, 0, 0, 0], [0, 0, 0, 1]],
            ] {
                assert!(CellRegion {
                    region: RegionId(1),
                    cells: columns(
                        rows.into_iter()
                            .map(|[x, y, z, value]| (Position { x, y, z }, value))
                    )
                }
                .decode()
                .is_none());
            }
        }

        #[test]
        fn links_round_trip_with_plain_masks_and_unusual_links_in_full() {
            let mut map = Edges::default();
            map.insert(
                (at(1, 0, 0, 0), Direction::East),
                KnownEdge::ordinary(at(1, 1, 0, 0), 0),
            );
            map.insert(
                (at(1, 0, 0, 0), Direction::South),
                KnownEdge::ordinary(at(1, 0, 1, 0), 0),
            );
            map.insert(
                (at(1, 0, 0, 1), Direction::Down),
                KnownEdge::ordinary(at(1, 0, 0, 0), 0),
            );
            // A portal, a rotated join, and a stair are stored in full.
            map.insert(
                (at(1, 4, 0, 0), Direction::East),
                KnownEdge::ordinary(at(2, 0, 0, 0), 0),
            );
            map.insert(
                (at(1, 0, 3, 0), Direction::West),
                KnownEdge::ordinary(at(1, -1, 3, 0), 1),
            );
            map.insert(
                (at(1, 2, 2, 0), Direction::Up),
                KnownEdge::ordinary(at(3, 2, 2, 0), 0),
            );
            let encoded = EdgeRegion::encode(&map);
            assert_eq!(
                encoded.plain,
                vec![(0, 0, vec![[0, 0b10 | 0b100], [1, 0b10_0000]])]
            );
            assert_eq!(encoded.other.len(), 3);
            assert_eq!(encoded.decode(), Some(map));
        }

        #[test]
        fn flight_masks_round_trip_and_reject_orphan_horizontal_and_unused_bits() {
            let mut map = Edges::default();
            let from = at(1, 2, 2, 0);
            map.insert(
                (from, Direction::Up),
                KnownEdge {
                    destination: at(1, 2, 2, 1),
                    rotation: 0,
                    requires_flight: true,
                },
            );
            let saved = EdgeRegion::encode(&map);
            assert_eq!(saved.plain, vec![(2, 2, vec![[0, (1 << 4) | (1 << 10)]])]);
            let encoded = serde_json::to_vec(&saved).unwrap();
            assert_eq!(
                serde_json::from_slice::<EdgeRegion>(&encoded)
                    .unwrap()
                    .decode(),
                Some(map)
            );
            for mask in [-1, 0, 64, 128, 129, (1 << 10), 4096, 8191] {
                assert!(
                    EdgeRegion {
                        region: RegionId(1),
                        plain: vec![(2, 2, vec![[0, mask]])],
                        other: vec![]
                    }
                    .decode()
                    .is_none(),
                    "{mask}"
                );
            }
            let edge = KnownEdge {
                destination: at(2, 0, 0, 0),
                rotation: 0,
                requires_flight: true,
            };
            assert!(EdgeRegion {
                region: RegionId(1),
                plain: vec![],
                other: vec![((from, Direction::East), edge)]
            }
            .decode()
            .is_none());
        }

        #[test]
        fn non_canonical_or_malformed_links_are_rejected() {
            let plain = (
                (at(1, 0, 0, 0), Direction::East),
                KnownEdge::ordinary(at(1, 1, 0, 0), 0),
            );
            let decode = |plain: Vec<[i32; 4]>, other| {
                EdgeRegion {
                    region: RegionId(1),
                    plain: columns(
                        plain
                            .into_iter()
                            .map(|[x, y, z, value]| (Position { x, y, z }, value)),
                    ),
                    other,
                }
                .decode()
            };
            // A plain link written in full has a second encoding.
            assert_eq!(decode(vec![], vec![plain]), None);
            // Duplicates, empty or out-of-range masks, and unsorted cells.
            assert_eq!(decode(vec![[0, 0, 0, 2]], vec![plain]), None);
            assert_eq!(decode(vec![[0, 0, 0, 0]], vec![]), None);
            assert_eq!(decode(vec![[0, 0, 0, 64]], vec![]), None);
            assert_eq!(decode(vec![[1, 0, 0, 2], [0, 0, 0, 2]], vec![]), None);
            // Links belong to the pooled region.
            let foreign = (
                (at(2, 4, 0, 0), Direction::East),
                KnownEdge::ordinary(at(3, 0, 0, 0), 0),
            );
            assert_eq!(decode(vec![], vec![foreign]), None);
        }
    }
}

/// Region pools retain immutable appearances and placement buckets across actors
/// and rewind boundaries. Only the current, explicit four-table shape is read.
mod explored_map_regions {
    use super::*;
    use crate::explored_map::{ChildKey, MapAppearance, Neighbor, Witness};
    use crate::navigation_map::{RegionKey, RegionMap};
    use serde::{Deserializer, Serializer};
    use std::rc::Rc;
    use tor_world::{Direction, Position, RegionId};

    #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
    #[serde(deny_unknown_fields)]
    struct Instance {
        cells: Vec<usize>,
        neighbors: Vec<usize>,
        witnesses: Vec<usize>,
        children: Vec<usize>,
    }

    impl<'de> Deserialize<'de> for Instance {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Fields {
                cells: Vec<usize>,
                neighbors: Vec<usize>,
                witnesses: Vec<usize>,
                children: Vec<usize>,
            }
            let fields: Fields = crate::bounded::named_map(deserializer)?;
            Ok(Self {
                cells: fields.cells,
                neighbors: fields.neighbors,
                witnesses: fields.witnesses,
                children: fields.children,
            })
        }
    }

    struct Regions {
        cells: Vec<RegionMap<Location, Shared<MapAppearance>>>,
        neighbors: Vec<RegionMap<(Location, Direction), Neighbor>>,
        witnesses: Vec<RegionMap<Location, Witness>>,
        children: Vec<RegionMap<ChildKey, u8>>,
        instances: Vec<Instance>,
    }

    type Cells = RegionMap<Location, Shared<MapAppearance>>;
    type Neighbors = RegionMap<(Location, Direction), Neighbor>;
    /// Each row is explicit: one cell or at most six ordinary links. There are
    /// no runs or products of dimensions controlled by checkpoint input.
    type Columns<T> = Vec<(i32, i32, Vec<(i32, T)>)>;

    fn columns<T>(entries: impl IntoIterator<Item = (Position, T)>) -> Columns<T> {
        let mut result = Columns::new();
        for (p, value) in entries {
            match result.last_mut() {
                Some((x, y, rows)) if (*x, *y) == (p.x, p.y) => rows.push((p.z, value)),
                _ => result.push((p.x, p.y, vec![(p.z, value)])),
            }
        }
        result
    }

    fn expand_columns<T>(
        columns: Columns<T>,
        mut visit: impl FnMut(Position, T) -> Option<()>,
    ) -> Option<()> {
        let mut previous = None;
        for (x, y, rows) in columns {
            if rows.is_empty() || previous.is_some_and(|p| p >= (x, y)) {
                return None;
            }
            previous = Some((x, y));
            let mut last_z = None;
            for (z, value) in rows {
                if last_z.is_some_and(|before| before >= z) {
                    return None;
                }
                last_z = Some(z);
                visit(Position { x, y, z }, value)?;
            }
        }
        Some(())
    }

    #[derive(Serialize)]
    struct CellRegion {
        region: RegionId,
        cells: Columns<usize>,
    }
    impl<'de> Deserialize<'de> for CellRegion {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Fields {
                region: RegionId,
                cells: Columns<usize>,
            }
            let fields: Fields = crate::bounded::named_map(d)?;
            Ok(Self {
                region: fields.region,
                cells: fields.cells,
            })
        }
    }

    const DIRECTIONS: [Direction; 6] = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
        Direction::Up,
        Direction::Down,
    ];
    fn plain_target(from: Location, direction: Direction) -> Option<Location> {
        let (dx, dy, dz) = direction.delta();
        let p = from.position;
        Some(Location {
            region: from.region,
            position: Position {
                x: p.x.checked_add(dx)?,
                y: p.y.checked_add(dy)?,
                z: p.z.checked_add(dz)?,
            },
        })
    }
    fn is_plain(from: Location, direction: Direction, edge: &Neighbor) -> bool {
        matches!(edge, Neighbor::Placed { target, rotation: 0 }
            if plain_target(from, direction) == Some(*target))
    }

    #[derive(Serialize)]
    struct NeighborRegion {
        region: RegionId,
        plain: Columns<u8>,
        other: Vec<((Location, Direction), Neighbor)>,
    }
    impl<'de> Deserialize<'de> for NeighborRegion {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Fields {
                region: RegionId,
                plain: Columns<u8>,
                other: Vec<((Location, Direction), Neighbor)>,
            }
            let fields: Fields = crate::bounded::named_map(d)?;
            Ok(Self {
                region: fields.region,
                plain: fields.plain,
                other: fields.other,
            })
        }
    }
    impl NeighborRegion {
        fn encode(map: &Neighbors) -> Self {
            let mut plain = BTreeMap::<Position, u8>::new();
            let mut other = Vec::new();
            for (&(from, direction), edge) in map.iter() {
                match DIRECTIONS.iter().position(|d| *d == direction) {
                    Some(bit) if is_plain(from, direction, edge) => {
                        *plain.entry(from.position).or_default() |= 1 << bit;
                    }
                    _ => other.push(((from, direction), edge.clone())),
                }
            }
            Self {
                region: map
                    .keys()
                    .next()
                    .expect("pooled regions are nonempty")
                    .0
                    .region,
                plain: columns(plain),
                other,
            }
        }
        fn decode(self) -> Option<Neighbors> {
            let mut map = Neighbors::default();
            expand_columns(self.plain, |position, mask| {
                if !(1..64).contains(&mask) {
                    return None;
                }
                let from = Location {
                    region: self.region,
                    position,
                };
                for (bit, direction) in DIRECTIONS.into_iter().enumerate() {
                    if mask & (1 << bit) != 0 {
                        map.insert(
                            (from, direction),
                            Neighbor::Placed {
                                target: plain_target(from, direction)?,
                                rotation: 0,
                            },
                        );
                    }
                }
                Some(())
            })?;
            let mut previous = None;
            for ((from, direction), edge) in self.other {
                let key = (from, direction);
                if from.region != self.region
                    || !DIRECTIONS.contains(&direction)
                    || previous.is_some_and(|before| before >= key)
                    || is_plain(from, direction, &edge)
                    || map.contains_key(&key)
                {
                    return None;
                }
                previous = Some(key);
                map.insert(key, edge);
            }
            map.is_checkpoint_region().then_some(map)
        }
    }

    /// Private wire shape. The runtime and validation tables below still use
    /// RegionMap; detached actors use this same codec, without cropping history.
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct SavedRegions {
        palette: Vec<Shared<MapAppearance>>,
        cells: Vec<CellRegion>,
        neighbors: Vec<NeighborRegion>,
        witnesses: Vec<RegionMap<Location, Witness>>,
        children: Vec<RegionMap<ChildKey, u8>>,
        instances: Vec<Instance>,
    }
    impl SavedRegions {
        fn encode(saved: Regions) -> Self {
            let palette = AppearancePalette::new(
                saved
                    .cells
                    .iter()
                    .flat_map(|bucket| bucket.iter().map(|(_, appearance)| appearance)),
            );
            let cells = saved
                .cells
                .iter()
                .map(|bucket| CellRegion {
                    region: bucket
                        .keys()
                        .next()
                        .expect("pooled regions are nonempty")
                        .region,
                    cells: columns(bucket.iter().map(|(at, appearance)| {
                        (
                            at.position,
                            palette.ranks[&std::ptr::from_ref::<MapAppearance>(appearance)],
                        )
                    })),
                })
                .collect();
            Self {
                palette: palette.values,
                cells,
                neighbors: saved.neighbors.iter().map(NeighborRegion::encode).collect(),
                witnesses: saved.witnesses,
                children: saved.children,
                instances: saved.instances,
            }
        }
        fn decode(self) -> Option<Regions> {
            // Descriptor and region-reference checks precede all row expansion.
            checked_directories(
                &self.instances,
                &self.cells,
                &self.neighbors,
                &self.witnesses,
                &self.children,
            )?;
            if self.palette.windows(2).any(|pair| pair[0] >= pair[1]) {
                return None;
            }
            let mut used = vec![false; self.palette.len()];
            let mut cells = Vec::new();
            for bucket in self.cells {
                let mut map = Cells::default();
                expand_columns(bucket.cells, |position, index| {
                    let appearance = self.palette.get(index)?;
                    used[index] = true;
                    map.insert(
                        Location {
                            region: bucket.region,
                            position,
                        },
                        appearance.clone(),
                    );
                    Some(())
                })?;
                cells.push(map);
            }
            if used.iter().any(|used| !used) {
                return None;
            }
            Some(Regions {
                cells,
                neighbors: self
                    .neighbors
                    .into_iter()
                    .map(NeighborRegion::decode)
                    .collect::<Option<Vec<_>>>()?,
                witnesses: self.witnesses,
                children: self.children,
                instances: self.instances,
            })
        }
    }

    trait PooledRegion {
        fn pooled_region(&self) -> Option<RegionId>;
    }
    impl<K: RegionKey, V: PartialEq> PooledRegion for RegionMap<K, V> {
        fn pooled_region(&self) -> Option<RegionId> {
            self.is_checkpoint_region()
                .then(|| self.keys().next().unwrap().region())
        }
    }
    impl PooledRegion for CellRegion {
        fn pooled_region(&self) -> Option<RegionId> {
            (!self.cells.is_empty()).then_some(self.region)
        }
    }
    impl PooledRegion for NeighborRegion {
        fn pooled_region(&self) -> Option<RegionId> {
            (!self.plain.is_empty() || !self.other.is_empty()).then_some(self.region)
        }
    }

    type AppearanceRanks = BTreeMap<*const MapAppearance, usize>;
    struct AppearancePalette {
        values: Vec<Shared<MapAppearance>>,
        ranks: AppearanceRanks,
    }
    impl AppearancePalette {
        fn new<'a>(appearances: impl Iterator<Item = &'a Shared<MapAppearance>>) -> Self {
            // Input roots keep every allocation alive for this entire operation.
            // Deduplicate pointers before content comparisons: a palette value
            // can contain an arbitrarily long legal ground-item vector.
            let allocations: BTreeMap<_, _> = appearances
                .map(|appearance| {
                    (
                        std::ptr::from_ref::<MapAppearance>(appearance),
                        appearance.clone(),
                    )
                })
                .collect();
            let mut distinct = BTreeMap::<Shared<MapAppearance>, Vec<*const MapAppearance>>::new();
            for (allocation, appearance) in allocations {
                distinct.entry(appearance).or_default().push(allocation);
            }
            let mut values = Vec::new();
            let mut ranks = BTreeMap::new();
            for (appearance, allocations) in distinct {
                for allocation in allocations {
                    ranks.insert(allocation, values.len());
                }
                values.push(appearance);
            }
            Self { values, ranks }
        }
    }

    trait PoolValue: Ord {
        fn pool_cmp(&self, other: &Self, _: &AppearanceRanks) -> std::cmp::Ordering {
            self.cmp(other)
        }
    }
    impl PoolValue for Neighbor {}
    impl PoolValue for Witness {}
    impl PoolValue for u8 {}
    impl PoolValue for Shared<MapAppearance> {
        fn pool_cmp(&self, other: &Self, ranks: &AppearanceRanks) -> std::cmp::Ordering {
            ranks[&std::ptr::from_ref::<MapAppearance>(self)]
                .cmp(&ranks[&std::ptr::from_ref::<MapAppearance>(other)])
        }
    }

    /// Full content equality is used only once per distinct bucket allocation.
    /// Entry comparisons are observable so near-equal historical versions cannot
    /// hide a linear candidate search behind validation-only counters.
    struct BucketContent<K, V>(Shared<BTreeMap<K, V>>, Rc<AppearanceRanks>);
    impl<K: Ord, V: PoolValue> PartialEq for BucketContent<K, V> {
        fn eq(&self, other: &Self) -> bool {
            self.cmp(other).is_eq()
        }
    }
    impl<K: Ord, V: PoolValue> Eq for BucketContent<K, V> {}
    impl<K: Ord, V: PoolValue> PartialOrd for BucketContent<K, V> {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl<K: Ord, V: PoolValue> Ord for BucketContent<K, V> {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            let length = self.0.len().cmp(&other.0.len());
            if !length.is_eq() {
                return length;
            }
            for ((key, value), (other_key, other_value)) in self.0.iter().zip(other.0.iter()) {
                crate::explored_map::count(|n| n.pool_compared_entries += 1);
                let order = key
                    .cmp(other_key)
                    .then_with(|| value.pool_cmp(other_value, &self.1));
                if !order.is_eq() {
                    return order;
                }
            }
            std::cmp::Ordering::Equal
        }
    }

    struct Buckets<K, V> {
        saved: Vec<RegionMap<K, V>>,
        // This identity never escapes collect(). All input roots remain held
        // for that call, so even a noncanonical equal allocation stays alive.
        allocations: BTreeMap<*const BTreeMap<K, V>, usize>,
        contents: BTreeMap<BucketContent<K, V>, usize>,
        appearance_ranks: Rc<AppearanceRanks>,
    }
    impl<K, V> Default for Buckets<K, V> {
        fn default() -> Self {
            Self {
                saved: Vec::new(),
                allocations: BTreeMap::new(),
                contents: BTreeMap::new(),
                appearance_ranks: Rc::default(),
            }
        }
    }
    impl<K: RegionKey, V: PoolValue> Buckets<K, V> {
        fn indices(&mut self, map: &RegionMap<K, V>) -> Vec<usize> {
            map.shared_regions()
                .map(|(&region, bucket)| {
                    let allocation = std::ptr::from_ref::<BTreeMap<K, V>>(bucket);
                    if let Some(&index) = self.allocations.get(&allocation) {
                        crate::explored_map::count(|n| n.pool_identity_hits += 1);
                        return index;
                    }
                    crate::explored_map::count(|n| n.pool_bucket_allocations += 1);
                    let index = match self
                        .contents
                        .entry(BucketContent(bucket.clone(), self.appearance_ranks.clone()))
                    {
                        std::collections::btree_map::Entry::Occupied(entry) => *entry.get(),
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            let index = self.saved.len();
                            self.saved
                                .push(RegionMap::from_shared_region(region, bucket.clone()));
                            entry.insert(index);
                            index
                        }
                    };
                    self.allocations.insert(allocation, index);
                    index
                })
                .collect()
        }
    }

    fn collect(maps: &[Shared<ExploredMap>]) -> Regions {
        // Shared historical buckets are visited once, before content pooling.
        let cell_buckets: BTreeMap<_, _> = maps
            .iter()
            .flat_map(|map| {
                map.cells.shared_regions().map(|(_, bucket)| {
                    (
                        std::ptr::from_ref::<BTreeMap<Location, Shared<MapAppearance>>>(bucket),
                        bucket,
                    )
                })
            })
            .collect();
        let palette =
            AppearancePalette::new(cell_buckets.values().flat_map(|bucket| bucket.values()));
        let mut cells = Buckets {
            appearance_ranks: Rc::new(palette.ranks),
            ..Buckets::default()
        };
        let mut neighbors = Buckets::default();
        let mut witnesses = Buckets::default();
        let mut children = Buckets::default();
        let instances = maps
            .iter()
            .map(|map| Instance {
                cells: cells.indices(&map.cells),
                neighbors: neighbors.indices(&map.neighbors),
                witnesses: witnesses.indices(&map.witnesses),
                children: children.indices(&map.children),
            })
            .collect();
        Regions {
            cells: cells.saved,
            neighbors: neighbors.saved,
            witnesses: witnesses.saved,
            children: children.saved,
            instances,
        }
    }

    pub(super) fn serialize<S: Serializer>(
        maps: &[Shared<ExploredMap>],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        SavedRegions::encode(collect(maps)).serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<Shared<ExploredMap>>, D::Error> {
        let encoded: SavedRegions = crate::bounded::named_map(deserializer)?;
        let saved = encoded.decode().ok_or_else(|| {
            <D::Error as serde::de::Error>::custom("invalid explored-map compact pool")
        })?;
        let maps = saved
            .instances
            .iter()
            .map(|instance| {
                Some(Shared::new(ExploredMap {
                    cells: RegionMap::restore_regions(&instance.cells, &saved.cells)?,
                    neighbors: RegionMap::restore_regions(&instance.neighbors, &saved.neighbors)?,
                    witnesses: RegionMap::restore_regions(&instance.witnesses, &saved.witnesses)?,
                    children: RegionMap::restore_regions(&instance.children, &saved.children)?,
                }))
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                <D::Error as serde::de::Error>::custom("invalid explored-map region reference")
            })?;
        // Re-pooling canonicalizes semantically equal buckets too, so aliases
        // using redundant bucket indices cannot evade descriptor uniqueness.
        let mut validation = RestoreValidation::new(&maps).ok_or_else(|| {
            <D::Error as serde::de::Error>::custom("invalid explored-map evidence")
        })?;
        for index in 0..maps.len() {
            validation.validate_knowledge(index).ok_or_else(|| {
                <D::Error as serde::de::Error>::custom("invalid explored-map evidence")
            })?;
        }
        Ok(maps)
    }

    #[cfg(test)]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(super) struct ValidationWork {
        pub descriptor_checks: usize,
        pub appearance_cells: usize,
        pub appearance_payloads: usize,
        pub neighbor_rows: usize,
        pub witness_rows: usize,
        pub child_rows: usize,
        pub world_cells: usize,
    }
    #[cfg(test)]
    thread_local! { static WORK: std::cell::Cell<ValidationWork> = const {
        std::cell::Cell::new(ValidationWork { descriptor_checks: 0, appearance_cells: 0, appearance_payloads: 0,
            neighbor_rows: 0, witness_rows: 0, child_rows: 0, world_cells: 0 })
    }; }
    #[cfg(test)]
    pub(super) fn work_counts() -> ValidationWork {
        WORK.with(std::cell::Cell::get)
    }
    #[cfg(test)]
    pub(super) fn reset_work_counts() {
        WORK.with(|c| c.set(ValidationWork::default()));
    }
    macro_rules! visit {
        (@local $field:ident) => {
            #[cfg(test)]
            WORK.with(|c| { let mut n = c.get(); n.$field += 1; c.set(n); });
        };
        (descriptor_checks) => { visit!(@local descriptor_checks); };
        (appearance_cells) => {
            crate::explored_map::count_map_validation(1, 0, 0);
            visit!(@local appearance_cells);
        };
        (world_cells) => {
            crate::explored_map::count_map_validation(0, 0, 1);
            visit!(@local world_cells);
        };
        ($field:ident) => {
            crate::explored_map::count_map_validation(0, 1, 0);
            visit!(@local $field);
        };
    }

    struct Directories {
        cells: BTreeMap<RegionId, usize>,
        neighbors: BTreeMap<RegionId, usize>,
        witnesses: BTreeMap<RegionId, usize>,
        children: BTreeMap<RegionId, usize>,
    }

    fn directory<T: PooledRegion>(
        indices: &[usize],
        pool: &[T],
        used: &mut BTreeSet<usize>,
    ) -> Option<BTreeMap<RegionId, usize>> {
        let mut directory = BTreeMap::new();
        let mut previous = None;
        for &index in indices {
            let region = pool.get(index)?.pooled_region()?;
            if previous.is_some_and(|before| before >= region) {
                return None;
            }
            previous = Some(region);
            directory.insert(region, index);
            used.insert(index);
        }
        Some(directory)
    }

    fn directories(saved: &Regions) -> Option<Vec<Directories>> {
        checked_directories(
            &saved.instances,
            &saved.cells,
            &saved.neighbors,
            &saved.witnesses,
            &saved.children,
        )
    }

    fn checked_directories<C: PooledRegion, N: PooledRegion>(
        instances: &[Instance],
        cell_pool: &[C],
        neighbor_pool: &[N],
        witness_pool: &[RegionMap<Location, Witness>],
        child_pool: &[RegionMap<ChildKey, u8>],
    ) -> Option<Vec<Directories>> {
        let mut distinct = BTreeSet::new();
        // Keep this ahead of both reference expansion and payload validation.
        for instance in instances {
            visit!(descriptor_checks);
            if !distinct.insert(instance) {
                return None;
            }
        }
        let (mut cells, mut neighbors, mut witnesses, mut children) = (
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeSet::new(),
        );
        let result = instances
            .iter()
            .map(|instance| {
                Some(Directories {
                    cells: directory(&instance.cells, cell_pool, &mut cells)?,
                    neighbors: directory(&instance.neighbors, neighbor_pool, &mut neighbors)?,
                    witnesses: directory(&instance.witnesses, witness_pool, &mut witnesses)?,
                    children: directory(&instance.children, child_pool, &mut children)?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        (cells.len() == cell_pool.len()
            && neighbors.len() == neighbor_pool.len()
            && witnesses.len() == witness_pool.len()
            && children.len() == child_pool.len())
        .then_some(result)
    }

    #[derive(Default)]
    struct Dependencies {
        cells: BTreeSet<RegionId>,
        reverse: BTreeSet<RegionId>,
    }
    fn dependency_indices(
        regions: &BTreeSet<RegionId>,
        directory: &BTreeMap<RegionId, usize>,
    ) -> Option<Vec<usize>> {
        regions
            .iter()
            .map(|region| directory.get(region).copied())
            .collect()
    }
    fn has_cell(saved: &Regions, directory: &Directories, at: Location) -> bool {
        directory
            .cells
            .get(&at.region)
            .is_some_and(|&index| saved.cells[index].contains_key(&at))
    }

    /// Owns only shared immutable buckets and small validation identities. A
    /// restored root never repeats validation of unchanged dependent buckets.
    pub(super) struct RestoreValidation {
        saved: Regions,
        directories: Vec<Directories>,
        neighbor_dependencies: Vec<Dependencies>,
        witness_dependencies: Vec<Dependencies>,
        child_dependencies: Vec<Dependencies>,
        valid_neighbors: BTreeSet<(usize, Vec<usize>)>,
        valid_witnesses: BTreeSet<(usize, Vec<usize>, Vec<usize>)>,
        valid_children: BTreeSet<(usize, Vec<usize>)>,
        valid_instances: BTreeSet<usize>,
        // Region names, residency and other world state cannot affect bounds.
        valid_world_cells: BTreeSet<(usize, Position, (i32, i32, i32))>,
    }

    impl RestoreValidation {
        pub(super) fn new(maps: &[Shared<ExploredMap>]) -> Option<Self> {
            Self::from_saved(collect(maps))
        }
        pub(super) fn new_with_catalog(
            maps: &[Shared<ExploredMap>],
            catalog: &crate::appearance_catalog::PresentationCatalog,
        ) -> Option<Self> {
            Self::from_saved_with_catalog(collect(maps), Some(catalog))
        }
        fn from_saved(saved: Regions) -> Option<Self> {
            Self::from_saved_with_catalog(saved, None)
        }
        fn from_saved_with_catalog(
            saved: Regions,
            catalog: Option<&crate::appearance_catalog::PresentationCatalog>,
        ) -> Option<Self> {
            let directories = directories(&saved)?;
            let mut valid_appearances = BTreeSet::new();
            for bucket in &saved.cells {
                for (_, appearance) in bucket.iter() {
                    visit!(appearance_cells);
                    if valid_appearances.insert(std::ptr::from_ref::<MapAppearance>(appearance)) {
                        visit!(@local appearance_payloads);
                        if !appearance.valid()
                            || catalog.is_some_and(|catalog| !appearance.valid_for_catalog(catalog))
                        {
                            return None;
                        }
                    }
                }
            }
            let mut neighbor_dependencies = Vec::new();
            for bucket in &saved.neighbors {
                let mut depends = Dependencies::default();
                for ((source, direction), edge) in bucket.iter() {
                    visit!(neighbor_rows);
                    if !matches!(
                        direction,
                        Direction::North
                            | Direction::East
                            | Direction::South
                            | Direction::West
                            | Direction::Up
                            | Direction::Down
                    ) {
                        return None;
                    }
                    depends.cells.insert(source.region);
                    if let Neighbor::Placed { target, rotation } = edge {
                        if *rotation >= 24 {
                            return None;
                        }
                        depends.cells.insert(target.region);
                    }
                }
                neighbor_dependencies.push(depends);
            }
            let mut witness_dependencies = Vec::new();
            for bucket in &saved.witnesses {
                let mut depends = Dependencies::default();
                for (child, witness) in bucket.iter() {
                    visit!(witness_rows);
                    depends.cells.insert(child.region);
                    if let Witness::Placed {
                        anchor,
                        delta,
                        rotation,
                    } = witness
                    {
                        // A valid cube rotation is a signed permutation. The
                        // inverse must also negate delta, excluding MIN on every axis.
                        if *rotation >= 24 || delta.iter().any(|n| n.checked_neg().is_none()) {
                            return None;
                        }
                        depends.cells.insert(anchor.region);
                        depends.reverse.insert(anchor.region);
                    }
                }
                witness_dependencies.push(depends);
            }
            let mut child_dependencies = Vec::new();
            for bucket in &saved.children {
                let mut depends = Dependencies::default();
                for ((_, _, child), rotation) in bucket.iter() {
                    visit!(child_rows);
                    if *rotation >= 24 {
                        return None;
                    }
                    depends.reverse.insert(child.region);
                }
                child_dependencies.push(depends);
            }
            Some(Self {
                saved,
                directories,
                neighbor_dependencies,
                witness_dependencies,
                child_dependencies,
                valid_neighbors: BTreeSet::new(),
                valid_witnesses: BTreeSet::new(),
                valid_children: BTreeSet::new(),
                valid_instances: BTreeSet::new(),
                valid_world_cells: BTreeSet::new(),
            })
        }

        fn validate_knowledge(&mut self, index: usize) -> Option<()> {
            if self.valid_instances.contains(&index) {
                return Some(());
            }
            let directory = self.directories.get(index)?;
            for &bucket in directory.neighbors.values() {
                let dependencies = dependency_indices(
                    &self.neighbor_dependencies[bucket].cells,
                    &directory.cells,
                )?;
                let key = (bucket, dependencies);
                if !self.valid_neighbors.contains(&key) {
                    for ((source, _), edge) in self.saved.neighbors[bucket].iter() {
                        visit!(neighbor_rows);
                        if !has_cell(&self.saved, directory, *source)
                            || matches!(edge, Neighbor::Placed { target, .. } if !has_cell(&self.saved, directory, *target))
                        {
                            return None;
                        }
                    }
                    self.valid_neighbors.insert(key);
                }
            }
            for &bucket in directory.witnesses.values() {
                let depends = &self.witness_dependencies[bucket];
                let key = (
                    bucket,
                    dependency_indices(&depends.cells, &directory.cells)?,
                    dependency_indices(&depends.reverse, &directory.children)?,
                );
                if !self.valid_witnesses.contains(&key) {
                    for (child, witness) in self.saved.witnesses[bucket].iter() {
                        visit!(witness_rows);
                        if !has_cell(&self.saved, directory, *child) {
                            return None;
                        }
                        if let Witness::Placed {
                            anchor,
                            delta,
                            rotation,
                        } = witness
                        {
                            let reverse = *directory.children.get(&anchor.region)?;
                            if !has_cell(&self.saved, directory, *anchor)
                                || self.saved.children[reverse].get(&(*anchor, *delta, *child))
                                    != Some(rotation)
                            {
                                return None;
                            }
                        }
                    }
                    self.valid_witnesses.insert(key);
                }
            }
            for &bucket in directory.children.values() {
                let dependencies = dependency_indices(
                    &self.child_dependencies[bucket].reverse,
                    &directory.witnesses,
                )?;
                let key = (bucket, dependencies);
                if !self.valid_children.contains(&key) {
                    for ((anchor, delta, child), rotation) in self.saved.children[bucket].iter() {
                        visit!(child_rows);
                        let witness = *directory.witnesses.get(&child.region)?;
                        if self.saved.witnesses[witness].get(child)
                            != Some(&Witness::Placed {
                                anchor: *anchor,
                                delta: *delta,
                                rotation: *rotation,
                            })
                        {
                            return None;
                        }
                    }
                    self.valid_children.insert(key);
                }
            }
            self.valid_instances.insert(index);
            Some(())
        }

        pub(super) fn validate(&mut self, index: usize, world: &World) -> Option<()> {
            self.validate_knowledge(index)?;
            for &bucket in &self.saved.instances.get(index)?.cells {
                let cells = &self.saved.cells[bucket];
                let region = cells.keys().next()?.region;
                let bounds = world.known_region(region)?.bounds;
                let key = (bucket, bounds.origin(), bounds.dimensions());
                if !self.valid_world_cells.contains(&key) {
                    for at in cells.keys() {
                        visit!(world_cells);
                        // Use exactly the metadata in the cache key. A forged
                        // world with conflicting loaded/absent entries cannot
                        // seed a reusable proof from the other entry's bounds.
                        if !bounds.contains(at.position) {
                            return None;
                        }
                    }
                    self.valid_world_cells.insert(key);
                }
            }
            Some(())
        }
    }
    /// Temporary owner validation for one record/arrival operation. Equal roots
    /// and shared buckets are checked once; every caller still checks ownership.
    pub(crate) struct OwnerValidation {
        validation: RestoreValidation,
        indices: BTreeMap<*const ExploredMap, usize>,
        // Keep every indexed allocation alive, including equal noncanonical roots.
        _roots: Vec<Shared<ExploredMap>>,
    }
    impl OwnerValidation {
        pub(crate) fn new<'a>(
            maps: impl IntoIterator<Item = &'a Shared<ExploredMap>>,
        ) -> Option<Self> {
            let mut indices = BTreeMap::new();
            let mut roots = Vec::new();
            for map in maps {
                let allocation = std::ptr::from_ref::<ExploredMap>(map);
                if let std::collections::btree_map::Entry::Vacant(entry) = indices.entry(allocation)
                {
                    entry.insert(roots.len());
                    roots.push(map.clone());
                }
            }
            let mut saved = collect(&roots);
            let mut distinct = BTreeMap::new();
            let mut instances = Vec::new();
            let remap = saved
                .instances
                .into_iter()
                .map(|instance| {
                    if let Some(&index) = distinct.get(&instance) {
                        return index;
                    }
                    let index = instances.len();
                    distinct.insert(instance.clone(), index);
                    instances.push(instance);
                    index
                })
                .collect::<Vec<_>>();
            saved.instances = instances;
            for index in indices.values_mut() {
                *index = remap[*index];
            }
            Some(Self {
                validation: RestoreValidation::from_saved(saved)?,
                indices,
                _roots: roots,
            })
        }
        pub(crate) fn structure(&mut self, map: &Shared<ExploredMap>) -> bool {
            self.indices
                .get(&std::ptr::from_ref::<ExploredMap>(map))
                .copied()
                .is_some_and(|index| self.validation.validate_knowledge(index).is_some())
        }
        pub(crate) fn world(&mut self, map: &Shared<ExploredMap>, world: &World) -> bool {
            self.indices
                .get(&std::ptr::from_ref::<ExploredMap>(map))
                .copied()
                .is_some_and(|index| self.validation.validate(index, world).is_some())
        }
    }
}

pub(crate) use explored_map_regions::OwnerValidation as ExploredOwnerValidation;

/// A detached actor retains all its remembered regions, with the same pooled
/// encoding as loaded checkpoints. Detachment never crops memory to its region.
pub(crate) mod explored_actor_maps {
    use super::*;
    use serde::{Deserializer, Serializer};

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Table {
        #[serde(with = "tor_world::checkpoint_map")]
        owners: BTreeMap<ActorId, usize>,
        #[serde(with = "super::explored_map_regions")]
        maps: Vec<Shared<ExploredMap>>,
    }

    pub(crate) fn serialize<S: Serializer>(
        actors: &BTreeMap<ActorId, Shared<ExploredMap>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut maps = Vec::<Shared<ExploredMap>>::new();
        let owners = actors
            .iter()
            .map(|(actor, map)| {
                let index = maps
                    .iter()
                    .position(|existing| existing.shares_storage(map) || existing == map)
                    .unwrap_or_else(|| {
                        maps.push(map.clone());
                        maps.len() - 1
                    });
                (*actor, index)
            })
            .collect();
        Table { owners, maps }.serialize(serializer)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<ActorId, Shared<ExploredMap>>, D::Error> {
        let saved: Table = crate::bounded::named_map(deserializer)?;
        let mut used = BTreeSet::new();
        let mut owners = BTreeMap::new();
        for (actor, index) in saved.owners {
            let map = saved
                .maps
                .get(index)
                .filter(|_| actor.0 != 0)
                .ok_or_else(|| serde::de::Error::custom("invalid explored-map owner reference"))?;
            used.insert(index);
            owners.insert(actor, map.clone());
        }
        if used.len() != saved.maps.len() {
            return Err(serde::de::Error::custom(
                "unreferenced explored-map instance",
            ));
        }
        Ok(owners)
    }
}

#[cfg(test)]
#[path = "checkpoint_explored_map_tests.rs"]
mod explored_map_tests;
