//! Captured public intent and observer-owned, one-time plan observations.
//!
//! These are backend facts. Wire adapters project only the already-disclosed
//! historical result, never current actor identity or location.
use crate::content_ids::AbilityId;
use crate::{ActorId, Game};
use serde::{Deserialize, Serialize};
use tor_world::{Location, Position, RegionId, SightCell};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedAbility {
    pub id: AbilityId,
    pub name: String,
}
impl<'de> Deserialize<'de> for ObservedAbility {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: AbilityId,
            #[serde(deserialize_with = "read_name")]
            name: String,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            id: fields.id,
            name: fields.name,
        })
    }
}

/// Observer-relative coordinates and opaque-cell input frozen at observation.
/// The backend location is never a wire coordinate or selectable actor handle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ObservedAim {
    Unavailable,
    Direction {
        direction: Position,
    },
    Disclosed {
        location: Location,
        occurrences: Vec<Position>,
    },
}
impl<'de> Deserialize<'de> for ObservedAim {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Unavailable {},
            Direction {
                #[serde(deserialize_with = "read_position")]
                direction: Position,
            },
            Disclosed {
                #[serde(deserialize_with = "read_location")]
                location: Location,
                #[serde(deserialize_with = "read_occurrences")]
                occurrences: Vec<Position>,
            },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::Unavailable {} => Self::Unavailable,
            Fields::Direction { direction } => {
                if !ray_direction_valid(direction) {
                    return Err(serde::de::Error::custom("invalid ray direction"));
                }
                Self::Direction { direction }
            }
            Fields::Disclosed {
                location,
                occurrences,
            } => Self::Disclosed {
                location,
                occurrences,
            },
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PublicPlanState {
    /// Uniform for idle, recovery and private/non-ability work. Public recovery
    /// needs separate licensed provenance; private ready_at is not that model.
    NoPublicPlan,
    Preparing {
        ability: Option<ObservedAbility>,
        aim: ObservedAim,
        remaining_work: u64,
    },
}
impl<'de> Deserialize<'de> for PublicPlanState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            NoPublicPlan {},
            Preparing {
                #[serde(deserialize_with = "crate::bounded::required_option")]
                ability: Option<ObservedAbility>,
                aim: ObservedAim,
                remaining_work: u64,
            },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::NoPublicPlan {} => Self::NoPublicPlan,
            Fields::Preparing {
                ability,
                aim,
                remaining_work,
            } => Self::Preparing {
                ability,
                aim,
                remaining_work,
            },
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicPlanObservation {
    pub observed_at: u64,
    pub subject_name: String,
    pub state: PublicPlanState,
}
impl<'de> Deserialize<'de> for PublicPlanObservation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            observed_at: u64,
            #[serde(deserialize_with = "read_name")]
            subject_name: String,
            state: PublicPlanState,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            observed_at: fields.observed_at,
            subject_name: fields.subject_name,
            state: fields.state,
        })
    }
}
impl PublicPlanObservation {
    pub(crate) fn valid(&self, rules: &crate::rules::Ruleset, world: &tor_world::World) -> bool {
        valid_name(&self.subject_name)
            && match &self.state {
                PublicPlanState::NoPublicPlan => true,
                PublicPlanState::Preparing {
                    ability,
                    aim,
                    remaining_work,
                } => {
                    *remaining_work <= crate::limits::NUMERIC as u64
                        && ability.as_ref().is_none_or(|known| {
                            rules
                                .ability(&known.id)
                                .is_some_and(|definition| definition.name == known.name)
                        })
                        && match aim {
                            ObservedAim::Unavailable => true,
                            ObservedAim::Direction { direction } => ray_direction_valid(*direction),
                            ObservedAim::Disclosed {
                                location,
                                occurrences,
                            } => {
                                world.knows(*location)
                                    && !occurrences.is_empty()
                                    && occurrences.len() <= crate::limits::PUBLIC_AIM_OCCURRENCES
                                    && occurrences.windows(2).all(|pair| pair[0] < pair[1])
                            }
                        }
                }
            }
    }
}
fn ray_direction_valid(direction: Position) -> bool {
    let parts = [direction.x, direction.y, direction.z];
    parts.iter().all(|v| (-1..=1).contains(v))
        && matches!(parts.iter().filter(|v| **v != 0).count(), 1 | 2)
}
fn valid_name(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= crate::limits::DISPLAY_NAME_BYTES
        && !value.chars().any(char::is_control)
}
fn read_name<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    struct Name;
    impl serde::de::Visitor<'_> for Name {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a bounded display name")
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<String, E> {
            if valid_name(value) {
                Ok(value.to_owned())
            } else {
                Err(E::custom("invalid display name"))
            }
        }
    }
    deserializer.deserialize_str(Name)
}
fn read_occurrences<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Position>, D::Error> {
    struct NamedPosition(Position);
    impl<'de> Deserialize<'de> for NamedPosition {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            read_position(deserializer).map(Self)
        }
    }
    crate::bounded::Bounded::<NamedPosition, { crate::limits::PUBLIC_AIM_OCCURRENCES }>::deserialize(deserializer)
        .map(|rows| rows.0.into_iter().map(|row| row.0).collect())
}

/// The declared aim captured with preparation, independently of a moving target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PublicAim {
    Unavailable,
    Cell { location: Location },
}

impl<'de> Deserialize<'de> for PublicAim {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Unavailable {},
            Cell {
                #[serde(deserialize_with = "read_location")]
                location: Location,
            },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::Unavailable {} => Self::Unavailable,
            Fields::Cell { location } => Self::Cell { location },
        })
    }
}

fn read_position<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Position, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        x: i32,
        y: i32,
        z: i32,
    }
    let fields: Fields = crate::bounded::named_map(deserializer)?;
    Ok(Position {
        x: fields.x,
        y: fields.y,
        z: fields.z,
    })
}

pub(crate) fn read_location<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Location, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        region: RegionId,
        #[serde(deserialize_with = "read_position")]
        position: Position,
    }
    let fields: Fields = crate::bounded::named_map(deserializer)?;
    Ok(Location {
        region: fields.region,
        position: fields.position,
    })
}

/// Opaque, bounded work prefilter for one committed observation boundary.
/// These backend identities grant no disclosure authority. Only an ordinary
/// observer view can license additions to remembered ability knowledge.
pub struct ObservationKnowledgeCandidates(std::collections::BTreeSet<AbilityId>);
impl ObservationKnowledgeCandidates {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The visible stance of a perceived public ability preparation. It is a
/// category only, never the ability identity, aim, remaining work or timing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicPosture {
    Melee,
    Ranged,
    Casting,
    Other,
}

impl Game {
    /// The posture category of a living actor's public ability preparation.
    /// Idle, recovering and item work all yield `None`, so they remain
    /// indistinguishable. This classifies only: the caller must already hold
    /// the observer's creature-classified sighting, the same gate as paid plan
    /// observation, and must not recompute perception for it.
    pub fn public_posture(&self, subject: ActorId) -> Option<PublicPosture> {
        let actor = self.actors.get(&subject)?;
        if !actor.alive() {
            return None;
        }
        let pending = actor.pending.as_ref()?;
        let ability = pending.plan.ability_id()?;
        Some(if pending.plan.as_projectile().is_some() {
            PublicPosture::Ranged
        } else if pending.plan.as_ray().is_some() {
            PublicPosture::Casting
        } else {
            match self.content.rules.ability(ability).map(|d| &d.reach) {
                Some(crate::rules::Reach::Melee) => PublicPosture::Melee,
                Some(crate::rules::Reach::Projectile) => PublicPosture::Ranged,
                Some(
                    crate::rules::Reach::Sight { .. }
                    | crate::rules::Reach::MutualSight { .. }
                    | crate::rules::Reach::UntilObstruction,
                ) => PublicPosture::Casting,
                Some(crate::rules::Reach::SelfOnly | crate::rules::Reach::SelfOrAlly { .. })
                | None => PublicPosture::Other,
            }
        })
    }

    /// Scan the event log once, rather than once for every potential observer.
    /// Valid games contain at most the bounded ability catalog's distinct IDs.
    pub fn observation_knowledge_candidates(
        &self,
    ) -> Result<ObservationKnowledgeCandidates, crate::GameError> {
        let mut ids = std::collections::BTreeSet::new();
        for event in &self.event_log {
            if let crate::combat::CombatEvent::Resolved { outcome, .. } = event {
                if let Some(id) = &outcome.ability {
                    ids.insert(id.clone());
                    if ids.len() > crate::limits::CATALOG_ENTRIES {
                        return Err(crate::GameError::Rule(
                            crate::abilities::RuleError::InvalidPlan,
                        ));
                    }
                }
            }
        }
        Ok(ObservationKnowledgeCandidates(ids))
    }

    /// Conservative only: an unknown backend event may require a final view,
    /// but is never itself evidence that this observer saw the ability.
    pub fn may_add_observation_knowledge(
        &self,
        observer: ActorId,
        candidates: &ObservationKnowledgeCandidates,
    ) -> bool {
        self.actors.get(&observer).is_some_and(|actor| {
            candidates
                .0
                .iter()
                .any(|id| !actor.recognized_abilities.contains(id))
        })
    }

    /// Persist only ability identities already licensed by this committed
    /// observer view. The host calls this at the authoritative perception
    /// boundary, never in response to a read or connection-cache fill.
    pub fn record_observation_knowledge(
        &mut self,
        observation: &crate::Observation,
    ) -> Result<(), crate::GameError> {
        self.record_disclosed_event_knowledge(
            observation.actor,
            observation
                .combat
                .as_ref()
                .into_iter()
                .flat_map(|combat| combat.events.iter()),
        )
    }

    /// Host-only knowledge commit after event receipts have been persisted for
    /// this boundary. Free reads/capture must never call this mutation.
    /// Uses historical disclosure, not a fresh visibility check or raw event IDs.
    pub fn record_committed_event_knowledge(
        &mut self,
        observer: ActorId,
    ) -> Result<(), crate::GameError> {
        let actor = self
            .actors
            .get(&observer)
            .ok_or(crate::GameError::UnknownActor)?;
        // Legacy ordinary views contain no combat event rows for plain actors.
        if actor.combat.is_none() {
            return Ok(());
        }
        let events = self.disclosed_event_history(observer);
        self.record_disclosed_event_knowledge(observer, events.iter())
    }

    fn record_disclosed_event_knowledge<'a>(
        &mut self,
        observer: ActorId,
        events: impl IntoIterator<Item = &'a crate::combat::DisclosedCombatEvent>,
    ) -> Result<(), crate::GameError> {
        let known = &self
            .actors
            .get(&observer)
            .ok_or(crate::GameError::UnknownActor)?
            .recognized_abilities;
        let mut learned = std::collections::BTreeSet::new();
        for event in events {
            if let crate::combat::DisclosedCombatEvent::Ability { id, name, .. } = event {
                if self
                    .content
                    .rules
                    .ability(id)
                    .is_none_or(|definition| definition.name != *name)
                {
                    return Err(crate::GameError::Rule(
                        crate::abilities::RuleError::InvalidPlan,
                    ));
                }
                if !known.contains(id) {
                    learned.insert(id.clone());
                }
            }
        }
        if !learned.is_empty() {
            self.actors
                .get_mut(&observer)
                .ok_or(crate::GameError::UnknownActor)?
                .recognized_abilities
                .extend(learned);
        }
        Ok(())
    }

    /// A past paid result, never a fresh inspection or selectable target handle.
    pub fn public_plan_observation(&self, observer: ActorId) -> Option<&PublicPlanObservation> {
        self.actors.get(&observer)?.public_plan.as_deref()
    }

    pub(crate) fn capture_public_plan_observation(
        &self,
        observer: ActorId,
        subject: ActorId,
    ) -> Option<PublicPlanObservation> {
        let actor = self.actors.get(&subject)?;
        let owner = self.actors.get(&observer)?;
        if observer == subject || !actor.alive() || !owner.alive() {
            return None;
        }
        let (scene, sight) = self.scene_snapshot(observer).ok()?;
        if self
            .perception_projection(observer, &scene, &sight)
            .ok()?
            .get(&subject)?
            .disclosure
            != crate::perception::ActorDisclosure::Creature
        {
            return None;
        }
        // Classification grants disclosure; a timing helper alone never does.
        let state = if let Some((pending, ability_id)) = actor
            .pending
            .as_ref()
            .and_then(|pending| pending.plan.ability_id().map(|id| (pending, id)))
        {
            let ability = owner
                .recognized_abilities
                .contains(ability_id)
                .then(|| self.content.rules.ability(ability_id))
                .flatten()
                .map(|definition| ObservedAbility {
                    id: definition.id.clone(),
                    name: definition.name.clone(),
                });
            let directional = pending
                .plan
                .as_ray()
                .and_then(|plan| {
                    plan.aim
                        .direction()
                        .map(|direction| (plan.origin, direction))
                })
                .or_else(|| {
                    pending.plan.as_projectile().and_then(|plan| {
                        plan.aim
                            .direction()
                            .map(|direction| (plan.origin, direction))
                    })
                });
            let aim = if let Some((origin, launch_direction)) = directional {
                let mut directions = scene
                    .iter()
                    .filter(|cell| cell.physical && cell.location == origin.location)
                    .map(|cell| {
                        let direction = tor_world::Direction::from(launch_direction)
                            .rotated(origin.orientation)
                            .rotated(tor_world::inverse_rotation(cell.rotation));
                        let (x, y, z) = direction.delta();
                        Position { x, y, z }
                    })
                    .collect::<std::collections::BTreeSet<_>>();
                if directions.len() == 1 {
                    ObservedAim::Direction {
                        direction: directions.pop_first().expect("single disclosed direction"),
                    }
                } else {
                    ObservedAim::Unavailable
                }
            } else {
                match pending.plan.as_ability().map(|plan| plan.public_aim) {
                    Some(PublicAim::Cell { location }) => {
                        let occurrences: Vec<_> = scene
                            .iter()
                            .filter(|cell| cell.location == location)
                            .map(|cell| cell.offset)
                            .collect::<std::collections::BTreeSet<_>>()
                            .into_iter()
                            .collect();
                        if occurrences.is_empty() {
                            ObservedAim::Unavailable
                        } else {
                            ObservedAim::Disclosed {
                                location,
                                occurrences,
                            }
                        }
                    }
                    _ => ObservedAim::Unavailable,
                }
            };
            PublicPlanState::Preparing {
                ability,
                aim,
                remaining_work: self.preparation_timing(subject)?.preparation_remaining?,
            }
        } else {
            PublicPlanState::NoPublicPlan
        };
        let presentation = self
            .hallucinated_actor_presentation(observer, subject, &sight)
            .ok()?;
        let name = presentation.as_ref().map_or_else(
            || crate::observation::projected_actor_name(owner, actor),
            |value| value.name.as_ref(),
        );
        Some(PublicPlanObservation {
            observed_at: self.tick,
            subject_name: if name.is_empty() {
                "figure".into()
            } else {
                name.to_owned()
            },
            state,
        })
    }

    /// Selection can see only part of a body. Never substitute a hidden anchor
    /// or an arbitrary contact for the explicitly captured public target cell.
    pub(crate) fn capture_public_aim(
        &self,
        actor: ActorId,
        target: ActorId,
        sight: Option<&[SightCell]>,
    ) -> PublicAim {
        if !self
            .actor_disclosure(actor, target)
            .ok()
            .is_some_and(|class| class == crate::perception::ActorDisclosure::Creature)
        {
            return PublicAim::Unavailable;
        }
        let Some(target) = self.actors.get(&target) else {
            return PublicAim::Unavailable;
        };
        let visible = sight.map_or_else(
            || self.scene_contains(actor, target.location).unwrap_or(false),
            |scene| scene.iter().any(|cell| cell.location == target.location),
        );
        if visible {
            PublicAim::Cell {
                location: target.location,
            }
        } else {
            PublicAim::Unavailable
        }
    }
}

pub(crate) fn read_recognized_abilities<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<tor_world::Shared<std::collections::BTreeSet<AbilityId>>, D::Error> {
    let values =
        crate::bounded::Bounded::<AbilityId, { crate::limits::CATALOG_ENTRIES }>::deserialize(
            deserializer,
        )?
        .0;
    let count = values.len();
    let known: std::collections::BTreeSet<_> = values.into_iter().collect();
    if known.len() != count {
        return Err(serde::de::Error::custom("duplicate recognized ability"));
    }
    Ok(tor_world::Shared::new(known))
}

#[cfg(test)]
#[path = "public_plan_tests.rs"]
mod tests;
