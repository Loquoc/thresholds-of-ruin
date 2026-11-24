//! Paid monster-control payloads and event-owned ground-scroll approaches.
//! Location-only fleeing never licenses an actor identity. The ordinary timed
//! condition owns expiry, immunity and active-floor time; no second timer exists.
use crate::content_ids::ConditionId;
use crate::{ActorId, Game, GameError, ItemId, ItemLocation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tor_world::{Direction, Location};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionPayload {
    pub condition: ConditionId,
    pub duration: u64,
}
impl<'de> Deserialize<'de> for ConditionPayload {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            condition: ConditionId,
            duration: u64,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            condition: f.condition,
            duration: f.duration,
        };
        if !value.valid_shape() {
            return Err(serde::de::Error::custom("invalid control duration"));
        }
        Ok(value)
    }
}
impl ConditionPayload {
    pub(crate) fn valid_shape(&self) -> bool {
        (1..=crate::limits::NUMERIC as u64).contains(&self.duration)
    }
    pub fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        self.valid_shape()
            && rules.condition(&self.condition).is_some_and(|def| {
                def.lifecycle == crate::rules::ConditionLifecycle::Timed
                    && !def.source_scoped
                    && matches!(
                        def.timing,
                        crate::rules::ConditionTiming::ActorActive { .. }
                    )
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlSpec {
    HoldNearby {
        condition: ConditionId,
        duration: u64,
        radius: u8,
    },
    ArmConfusion {
        condition: ConditionId,
        duration: u64,
    },
    ScareVisible {
        fear: ConditionPayload,
        wake_conditions: [ConditionId; 2],
    },
    Aggravate {},
    CreateMonster {},
    Nothing {},
}
impl<'de> Deserialize<'de> for ControlSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            HoldNearby {
                condition: ConditionId,
                duration: u64,
                radius: u8,
            },
            ArmConfusion {
                condition: ConditionId,
                duration: u64,
            },
            ScareVisible {
                fear: ConditionPayload,
                wake_conditions: [ConditionId; 2],
            },
            Aggravate {},
            CreateMonster {},
            Nothing {},
        }
        let value = match crate::bounded::named_map(d)? {
            Fields::HoldNearby {
                condition,
                duration,
                radius,
            } => Self::HoldNearby {
                condition,
                duration,
                radius,
            },
            Fields::ArmConfusion {
                condition,
                duration,
            } => Self::ArmConfusion {
                condition,
                duration,
            },
            Fields::ScareVisible {
                fear,
                wake_conditions,
            } => Self::ScareVisible {
                fear,
                wake_conditions,
            },
            Fields::Aggravate {} => Self::Aggravate {},
            Fields::CreateMonster {} => Self::CreateMonster {},
            Fields::Nothing {} => Self::Nothing {},
        };
        if !value.valid_shape() {
            return Err(serde::de::Error::custom("invalid control payload"));
        }
        Ok(value)
    }
}
impl ControlSpec {
    pub fn valid_shape(&self) -> bool {
        match self {
            Self::HoldNearby {
                duration, radius, ..
            } => (1..=8).contains(radius) && (1..=crate::limits::NUMERIC as u64).contains(duration),
            Self::ArmConfusion { duration, .. } => {
                (1..=crate::limits::NUMERIC as u64).contains(duration)
            }
            Self::ScareVisible {
                fear,
                wake_conditions,
            } => {
                fear.valid_shape()
                    && wake_conditions[0] != wake_conditions[1]
                    && wake_conditions.iter().all(|id| id != &fear.condition)
            }
            _ => true,
        }
    }
    pub fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        if !self.valid_shape() {
            return false;
        }
        match self {
            Self::HoldNearby {
                condition,
                duration,
                ..
            }
            | Self::ArmConfusion {
                condition,
                duration,
            } => ConditionPayload {
                condition: condition.clone(),
                duration: *duration,
            }
            .valid(rules),
            Self::ScareVisible {
                fear,
                wake_conditions,
            } => {
                fear.valid(rules)
                    && wake_conditions
                        .iter()
                        .all(|id| rules.condition(id).is_some())
            }
            _ => true,
        }
    }
}
/// A ground item's payload; intentionally has no resistance-check field.
pub type ScareSpec = ConditionPayload;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct OwnedCondition {
    condition: ConditionId,
    generation: u64,
}
impl<'de> Deserialize<'de> for OwnedCondition {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            condition: ConditionId,
            generation: u64,
        }
        let value: Fields = crate::bounded::named_map(d)?;
        // The ordinary condition allocator starts at zero. Only the exhausted
        // sentinel cannot name an allocated lifetime; restore checks ownership.
        if value.generation == u64::MAX {
            return Err(serde::de::Error::custom(
                "invalid owned condition generation",
            ));
        }
        Ok(Self {
            condition: value.condition,
            generation: value.generation,
        })
    }
}
impl OwnedCondition {
    fn valid_definition(&self, rules: &crate::rules::Ruleset) -> bool {
        ConditionPayload {
            condition: self.condition.clone(),
            duration: 1,
        }
        .valid(rules)
    }
    fn matches(&self, conditions: &crate::conditions::ConditionState) -> bool {
        conditions.instances().any(|entry| {
            entry.condition == self.condition
                && entry.generation == self.generation
                && entry.phase == crate::conditions::ConditionPhase::Active
                && entry.remaining.is_some_and(|remaining| remaining > 0)
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct FleeSource {
    owned: OwnedCondition,
    location: Location,
}
impl<'de> Deserialize<'de> for FleeSource {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            owned: OwnedCondition,
            #[serde(deserialize_with = "crate::bounded::location")]
            location: Location,
        }
        let value: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            owned: value.owned,
            location: value.location,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct ScareContact {
    item: ItemId,
    attempted: bool,
}
impl<'de> Deserialize<'de> for ScareContact {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            item: ItemId,
            attempted: bool,
        }
        let value: Fields = crate::bounded::named_map(d)?;
        if value.item.0 == 0 || value.item.0 == u64::MAX {
            return Err(serde::de::Error::custom("invalid scare item"));
        }
        Ok(Self {
            item: value.item,
            attempted: value.attempted,
        })
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ControlState {
    confusion: Option<ConditionPayload>,
    held: Option<OwnedCondition>,
    flee: Option<FleeSource>,
    scare_contacts: Vec<ScareContact>,
}
impl<'de> Deserialize<'de> for ControlState {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::required_option")]
            confusion: Option<ConditionPayload>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            held: Option<OwnedCondition>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            flee: Option<FleeSource>,
            scare_contacts: crate::bounded::Bounded<ScareContact, 64>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        if !valid_contacts(&f.scare_contacts.0) {
            return Err(serde::de::Error::custom("invalid scare contacts"));
        }
        Ok(Self {
            confusion: f.confusion,
            held: f.held,
            flee: f.flee,
            scare_contacts: f.scare_contacts.0,
        })
    }
}
fn valid_contacts(contacts: &[ScareContact]) -> bool {
    contacts.len() <= 64
        && contacts
            .iter()
            .all(|entry| entry.item.0 > 0 && entry.item.0 < u64::MAX)
        && contacts.windows(2).all(|pair| pair[0].item < pair[1].item)
}
impl ControlState {
    pub(crate) fn reconcile_conditions(
        &mut self,
        conditions: Option<&crate::conditions::ConditionState>,
    ) {
        if self
            .held
            .as_ref()
            .is_some_and(|owned| !conditions.is_some_and(|state| owned.matches(state)))
        {
            self.held = None;
        }
        if self
            .flee
            .as_ref()
            .is_some_and(|source| !conditions.is_some_and(|state| source.owned.matches(state)))
        {
            self.flee = None;
        }
    }
    fn arm_confusion(&mut self, payload: ConditionPayload) {
        self.confusion = Some(payload);
    }
    fn take_confusion(&mut self, hit: bool) -> Option<ConditionPayload> {
        hit.then(|| self.confusion.take()).flatten()
    }
    fn enter_scared(
        &mut self,
        near: &[ItemId],
        eligible: &[ItemId],
    ) -> Result<Vec<ItemId>, GameError> {
        if near.len() > 64
            || !near.windows(2).all(|p| p[0] < p[1])
            || near.iter().any(|id| id.0 == 0 || id.0 == u64::MAX)
            || eligible.iter().any(|id| near.binary_search(id).is_err())
        {
            return Err(GameError::ItemUnavailable);
        }
        self.scare_contacts
            .retain(|entry| near.binary_search(&entry.item).is_ok());
        let mut entered = Vec::new();
        for item in near {
            let index = match self
                .scare_contacts
                .binary_search_by_key(item, |entry| entry.item)
            {
                Ok(index) => index,
                Err(index) => {
                    self.scare_contacts.insert(
                        index,
                        ScareContact {
                            item: *item,
                            attempted: false,
                        },
                    );
                    index
                }
            };
            let entry = &mut self.scare_contacts[index];
            if !entry.attempted && eligible.contains(item) {
                entry.attempted = true;
                entered.push(*item);
            }
        }
        Ok(entered)
    }
}

impl Game {
    pub(crate) fn control_valid(&self, _id: ActorId, actor: &crate::Actor) -> bool {
        let control = &actor.control;
        control
            .confusion
            .as_ref()
            .is_none_or(|value| value.valid(self.content.rules()))
            && control.held.as_ref().is_none_or(|owned| {
                owned.valid_definition(self.content.rules())
                    && actor
                        .combat
                        .as_ref()
                        .is_some_and(|combat| owned.matches(combat.creature().conditions()))
            })
            && control.flee.as_ref().is_none_or(|source| {
                source.owned.valid_definition(self.content.rules())
                    && actor
                        .combat
                        .as_ref()
                        .is_some_and(|combat| source.owned.matches(combat.creature().conditions()))
                    && self.world.knows(source.location)
            })
            && valid_contacts(&control.scare_contacts)
    }
    pub(crate) fn forced_flee_location(&self, actor: ActorId) -> Option<Location> {
        let owner = self.actors.get(&actor)?;
        let source = owner.control.flee.as_ref()?;
        source
            .owned
            .matches(self.creature(actor)?.conditions())
            .then_some(source.location)
    }
    fn owned_control_condition(
        &self,
        actor: ActorId,
        condition: &ConditionId,
    ) -> Option<OwnedCondition> {
        let creature = self.creature(actor)?;
        let entry = creature.conditions().instances().find(|entry| {
            &entry.condition == condition
                && entry.phase == crate::conditions::ConditionPhase::Active
                && entry.remaining.is_none_or(|n| n > 0)
        })?;
        Some(OwnedCondition {
            condition: condition.clone(),
            generation: entry.generation,
        })
    }
    fn apply_control_condition(
        &mut self,
        source: ActorId,
        target: ActorId,
        payload: &ConditionPayload,
    ) -> Result<bool, GameError> {
        self.settle_actor_time(target)?;
        let context = self.condition_recovery_context(target, &payload.condition)?;
        let mut outcome = self.actor_outcome(target)?;
        let Some(combat) = outcome.state.combat.as_mut() else {
            return Ok(false);
        };
        let update =
            combat.apply_condition(&payload.condition, source, payload.duration, context)?;
        let accepted = !matches!(
            update,
            crate::conditions::ConditionUpdate::Immune
                | crate::conditions::ConditionUpdate::Reserved
        );
        let active = accepted
            && combat
                .creature()
                .conditions()
                .has_active(&payload.condition);
        self.apply_outcome(outcome)?;
        Ok(active)
    }
    fn control_lore_check(&mut self, source: ActorId, target: ActorId) -> Result<bool, GameError> {
        self.settle_actor_time(source)?;
        self.settle_actor_time(target)?;
        let caster = self.creature(source).ok_or(GameError::UnknownActor)?;
        let recipient = self.creature(target).ok_or(GameError::UnknownActor)?;
        let rules = self.content.rules();
        let tokens: Vec<_> = caster
            .conditions()
            .qualifying_tokens(rules, source, false)
            .into_iter()
            .chain(
                recipient
                    .conditions()
                    .qualifying_tokens(rules, target, true),
            )
            .collect();
        let edge = caster
            .derived()
            .check_edge_with_selection(
                recipient.derived(),
                rules,
                crate::creatures::CheckContext {
                    ability: None,
                    roller_role: crate::rules::CheckActor::User,
                    roller: source,
                    opponent: target,
                    defense: Some(crate::rules::DefenseKind::Spiritual),
                },
                true,
                crate::rules::OutgoingEdgePolicy::Ordinary,
                &tokens,
            )
            .map_err(|_| GameError::Rule(crate::abilities::RuleError::InvalidDefinition))?;
        let bonus = crate::attributes::SkillCheck {
            skill: crate::attributes::Skill::Lore,
            binding: caster.derived().binding(),
            modifier: 0,
            threshold: 0,
        }
        .bonus(caster.derived().attributes, caster.derived().skills);
        let check = crate::abilities::PlannedCheck {
            actor: crate::rules::CheckActor::User,
            target_participates: true,
            bonus,
            edge,
            threshold: crate::abilities::CheckThreshold::Fixed {
                value: i64::from(recipient.derived().defenses.spiritual),
            },
        };
        let mut rng = self.rules_rng;
        let success = crate::abilities::resolve_escape_check(&check, &mut rng);
        self.rules_rng = rng;
        for holder in [source, target] {
            let selected: Vec<_> = tokens
                .iter()
                .filter(|token| token.holder == holder)
                .cloned()
                .collect();
            if selected.is_empty() {
                continue;
            }
            let mut outcome = self.actor_outcome(holder)?;
            let combat = outcome
                .state
                .combat
                .as_mut()
                .ok_or(GameError::UnknownActor)?;
            combat.reserve_tokens(&selected, holder)?;
            combat.consume_tokens(&selected, holder);
            self.apply_outcome(outcome)?;
        }
        Ok(success)
    }
    fn hold_target_in_range(
        &self,
        reader: ActorId,
        target: ActorId,
        radius: u8,
    ) -> Result<bool, GameError> {
        let Some(target) = self.actors.get(&target) else {
            return Ok(false);
        };
        let [ex, ey, ez] = self
            .actors
            .get(&reader)
            .ok_or(GameError::UnknownActor)?
            .body
            .eye;
        Ok(self.scene_snapshot(reader)?.1.iter().any(|cell| {
            cell.physical
                && !cell.wall
                && cell.location == target.location
                && cell.offset.z + ez == 0
                && (cell.offset.x + ex).unsigned_abs() <= u32::from(radius)
                && (cell.offset.y + ey).unsigned_abs() <= u32::from(radius)
        }))
    }

    /// A ward can work without the occupant perceiving its victim. Name only
    /// an actually sighted creature, through the ordinary appearance projection.
    fn sighted_actor_name(&self, observer: ActorId, subject: ActorId) -> Option<String> {
        let owner = self.actors.get(&observer)?;
        if !owner.perception().sight {
            return None;
        }
        let (_, sight) = self.scene_snapshot(observer).ok()?;
        if !self.actor_visible_in_sight(observer, subject, &sight) {
            return None;
        }
        let appearance = self
            .hallucinated_actor_presentation(observer, subject, &sight)
            .ok()?;
        let target = self.actors.get(&subject)?;
        let name = appearance.as_ref().map_or_else(
            || crate::observation::projected_actor_name(owner, target),
            |shown| shown.name.as_ref(),
        );
        let name = if name.trim().is_empty() {
            "creature"
        } else {
            name
        };
        Some(name.to_owned())
    }

    /// Different from near-field contact: only bodies on physical eye-sight
    /// cells are subject to an ordinary or reversed Scare read.
    fn visible_scare_targets(&self, reader: ActorId) -> Result<BTreeSet<ActorId>, GameError> {
        let (_, sight) = self.scene_snapshot(reader)?;
        let mut targets = BTreeSet::new();
        for cell in sight.iter().filter(|cell| cell.physical && !cell.wall) {
            targets.extend(
                self.actors
                    .at(&self.world, cell.location)
                    .keys()
                    .copied()
                    .filter(|&target| {
                        target != reader && self.alive(target) && self.creature(target).is_some()
                    }),
            );
        }
        Ok(targets)
    }

    fn resolve_scare_read(
        &mut self,
        reader: ActorId,
        fear: &ConditionPayload,
        wake_conditions: &[ConditionId; 2],
        cursed: bool,
    ) -> Result<bool, GameError> {
        let actor = self.actors.get(&reader).ok_or(GameError::UnknownActor)?;
        let reversed = cursed || actor.locomotion().confused.is_some();
        let source = actor.location;
        let targets = self.visible_scare_targets(reader)?;
        // Allied followers never determine the audible distance, even though
        // eligible creatures may still be affected by the magic.
        let nearby = targets.iter().any(|&target| !self.allied(reader, target));
        let hallucinating = self.projected_hallucination(reader)?.is_some();
        let mut changed = false;
        for target in targets {
            if reversed {
                let mut conditions = BTreeSet::from([fear.condition.clone()]);
                conditions.extend(wake_conditions.iter().cloned());
                if let Some(flee) = &self.actors[&target].control.flee {
                    conditions.insert(flee.owned.condition.clone());
                }
                let effects: Vec<_> = conditions
                    .into_iter()
                    .map(|condition| crate::rules::Effect::RemoveCondition { condition })
                    .collect();
                changed |= self.apply_effects_in_transaction(target, &effects)?;
            } else if self.forced_flee_location(target).is_none()
                && self.control_lore_check(reader, target)?
                && self.apply_control_condition(reader, target, fear)?
            {
                let mut outcome = self.actor_outcome(target)?;
                outcome.state.control.flee = self
                    .owned_control_condition(target, &fear.condition)
                    .map(|owned| FleeSource {
                        owned,
                        location: source,
                    });
                self.apply_outcome(outcome)?;
                changed = true;
            }
        }
        let sound = match (reversed, hallucinating) {
            (false, false) => "a burst of laughter",
            (true, false) => "a drawn-out wail",
            (false, true) => "teapots cackling",
            (true, true) => "a choir of tea kettles wailing",
        };
        let distance = if nearby { "nearby" } else { "in the distance" };
        self.narrate_gameplay(reader, format!("You hear {sound} {distance}."));
        Ok(changed)
    }

    pub(crate) fn resolve_control_read(
        &mut self,
        reader: ActorId,
        control: &ControlSpec,
        cursed: bool,
    ) -> Result<bool, GameError> {
        match control {
            ControlSpec::ScareVisible {
                fear,
                wake_conditions,
            } => self.resolve_scare_read(reader, fear, wake_conditions, cursed),
            ControlSpec::Nothing {} => Ok(false),
            ControlSpec::Aggravate {} => {
                if self.alert_location_pulse(reader)? {
                    let cue = if self.projected_hallucination(reader)?.is_some() {
                        "You feel an itch of unease crawl up your spine."
                    } else {
                        "You feel a sudden jolt of unease."
                    };
                    self.narrate_gameplay(reader, cue.into());
                }
                Ok(true)
            }
            ControlSpec::CreateMonster {} => {
                let created = self.create_population_monster(reader)?;
                if let Some(name) = created.and_then(|actor| self.sighted_actor_name(reader, actor))
                {
                    self.narrate_gameplay(reader, format!("The {name} appears."));
                }
                Ok(created.is_some())
            }
            ControlSpec::ArmConfusion {
                condition,
                duration,
            } => {
                let mut outcome = self.actor_outcome(reader)?;
                outcome.state.control.arm_confusion(ConditionPayload {
                    condition: condition.clone(),
                    duration: *duration,
                });
                self.apply_outcome(outcome)?;
                let sight = self.actors[&reader].perception().sight;
                let hallucinating = self.projected_hallucination(reader)?.is_some();
                let cue = match (sight, hallucinating) {
                    (true, false) => "Your hands begin to glow red.",
                    (true, true) => "Your hands glow a color you can't quite name.",
                    (false, false) => "Your hands begin to tingle.",
                    (false, true) => "Your hands tingle as if they were full of tiny bells.",
                };
                self.narrate_gameplay(reader, cue.into());
                Ok(true)
            }
            ControlSpec::HoldNearby {
                condition,
                duration,
                radius,
            } => {
                let [ex, ey, ez] = self
                    .actors
                    .get(&reader)
                    .ok_or(GameError::UnknownActor)?
                    .body
                    .eye;
                let near: std::collections::BTreeSet<_> = self
                    .scene_snapshot(reader)?
                    .1
                    .iter()
                    .filter(|cell| {
                        cell.physical
                            && !cell.wall
                            && cell.offset.z + ez == 0
                            && (cell.offset.x + ex).unsigned_abs() <= u32::from(*radius)
                            && (cell.offset.y + ey).unsigned_abs() <= u32::from(*radius)
                    })
                    .map(|cell| cell.location)
                    .collect();
                let view = self.observe(reader)?;
                // Observations contain one occurrence per visible body cell.
                // The paid mental check belongs to the creature, not its size.
                let targets: std::collections::BTreeSet<_> = view
                    .visible_actors
                    .iter()
                    .filter(|other| {
                        other.id != reader
                            && self.perceived_hostile(reader, other.id, other.disclosure)
                            && near.contains(&other.location)
                            && self.actors.get(&other.id).is_some_and(|owner| {
                                owner
                                    .engagement_at(self.actor_clock(other.id))
                                    .is_some_and(|entry| entry.target == reader)
                            })
                    })
                    .map(|other| other.id)
                    .collect();
                let payload = ConditionPayload {
                    condition: condition.clone(),
                    duration: *duration,
                };
                let mut applied = false;
                for target in targets {
                    let already_held = self
                        .creature(target)
                        .is_some_and(|target| target.conditions().has_active(condition));
                    if self.hold_target_in_range(reader, target, *radius)?
                        && self.control_lore_check(reader, target)?
                        && self.hold_target_in_range(reader, target, *radius)?
                        && self.apply_control_condition(reader, target, &payload)?
                    {
                        let mut outcome = self.actor_outcome(target)?;
                        outcome.state.control.held =
                            self.owned_control_condition(target, condition);
                        self.apply_outcome(outcome)?;
                        if !already_held {
                            if let Some(name) = self.sighted_actor_name(reader, target) {
                                self.narrate_gameplay(reader, format!("The {name} stops short."));
                            }
                        }
                        applied = true;
                    }
                }
                Ok(applied)
            }
        }
    }
    pub(crate) fn release_control_hold(&mut self, target: ActorId) -> Result<(), GameError> {
        let Some(held) = self
            .actors
            .get(&target)
            .and_then(|actor| actor.control.held.clone())
        else {
            return Ok(());
        };
        if self
            .creature(target)
            .is_some_and(|creature| held.matches(creature.conditions()))
        {
            self.apply_effects_in_transaction(
                target,
                &[crate::rules::Effect::RemoveCondition {
                    condition: held.condition,
                }],
            )?;
        }
        self.actors
            .get_mut(&target)
            .ok_or(GameError::UnknownActor)?
            .control
            .held = None;
        Ok(())
    }
    /// Called once for each committed hostile primary, never for its riders.
    pub(crate) fn control_on_attack(
        &mut self,
        outcome: &crate::abilities::Outcome,
    ) -> Result<(), GameError> {
        let Some(ability) = outcome
            .ability
            .as_ref()
            .and_then(|id| self.content.rules().ability(id))
        else {
            return Ok(());
        };
        if outcome.actor == outcome.target
            || ability.interaction != crate::rules::Interaction::HostileToTarget
        {
            return Ok(());
        }
        let physical = matches!(
            ability.strikes,
            crate::rules::StrikeSource::SelectedMelee
                | crate::rules::StrikeSource::NaturalMelee { .. }
                | crate::rules::StrikeSource::Projectile { .. }
        );
        // A committed hostile attempt releases Hold even if the prepared
        // target became unreachable; selection/preview never calls this hook.
        self.release_control_hold(outcome.target)?;
        let hit = physical && outcome.check.is_some_and(|check| check.success);
        let payload = self
            .actors
            .get_mut(&outcome.actor)
            .and_then(|mut actor| actor.control.take_confusion(hit));
        if let Some(payload) = payload {
            if self.alive(outcome.target)
                && self.control_lore_check(outcome.actor, outcome.target)?
            {
                let already_confused = self
                    .creature(outcome.target)
                    .is_some_and(|target| target.conditions().has_active(&payload.condition));
                if self.apply_control_condition(outcome.actor, outcome.target, &payload)?
                    && !already_confused
                {
                    if let Some(name) = self.sighted_actor_name(outcome.actor, outcome.target) {
                        self.narrate_gameplay(
                            outcome.actor,
                            format!("The {name} reels in confusion."),
                        );
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) fn reconcile_control_contacts(&mut self) -> Result<(), GameError> {
        let _diagnostic_span = crate::diagnostics::spans::span("control_contacts");
        // Loaded ground items are the only possible sources of these contacts.
        // A carried scare scroll is not a ground effect. This exact predicate
        // avoids a new index/invalidation contract and is stable on the absent
        // path, which only clears obsolete actor-owned contact receipts.
        let has_ground_scare = self.items.values().any(|item| {
            matches!(item.location, ItemLocation::Ground(_)) && item.spec.ground_scare.is_some()
        });
        let recipients: Vec<_> = self.actors.keys().copied().collect();
        for recipient in recipients {
            if !self.alive(recipient)
                || self.actor_frozen(recipient)
                || !self.combat.ai.contains_key(&recipient)
            {
                continue;
            }
            if self.actors[&recipient].perception().readied_touch_ambush
                && self.actors[&recipient].guise.appearance().is_some()
            {
                continue;
            }
            let actor = &self.actors[&recipient];
            let cells = self
                .body_cells(actor.location, actor.orientation, &actor.body)
                .ok_or(GameError::InvalidLocation)?;
            // Retain body validation even on this fast path: absence of a
            // feature must not mask the existing InvalidLocation error.
            if !has_ground_scare {
                if !actor.control.scare_contacts.is_empty() {
                    self.actors
                        .get_mut(&recipient)
                        .ok_or(GameError::UnknownActor)?
                        .control
                        .scare_contacts
                        .clear();
                }
                // Earned flee/hold/confusion ownership is independent of these
                // approach receipts and must survive removal of the scroll.
                continue;
            }
            let mut contacts = BTreeMap::new();
            for (cell, _) in cells {
                for direction in Direction::HORIZONTAL {
                    crate::diagnostics::control_contact_neighbor_probe();
                    let next = if direction.components().is_some() {
                        self.world.diagonal_reach(cell, direction, |_| true)
                    } else {
                        self.world.movement_neighbor(cell, direction)
                    };
                    let Some((at, _)) = next else {
                        continue;
                    };
                    if !self.world.walkable(at) {
                        continue;
                    }
                    let beneficiary =
                        self.actors
                            .at(&self.world, at)
                            .keys()
                            .copied()
                            .find(|other| {
                                *other != recipient
                                    && self.alive(*other)
                                    && self.hostile(recipient, *other)
                            });
                    for item in self.items.at(ItemLocation::Ground(at)) {
                        if let Some(spec) = &self.items[&item].spec.ground_scare {
                            contacts.insert(item, (beneficiary, at, spec.clone()));
                        }
                    }
                }
            }
            let ids: Vec<_> = contacts.keys().copied().collect();
            let eligible: Vec<_> = contacts
                .iter()
                .filter(|(_, (source, _, _))| source.is_some())
                .map(|(id, _)| *id)
                .collect();
            // The bounded contact vector often remains identical. Compute the
            // same ordered transition without dirtying actor indexes or taking
            // a COW write guard unless authoritative control state changes.
            let mut control = self.actors[&recipient].control.clone();
            let entered = control.enter_scared(&ids, &eligible)?;
            if control != self.actors[&recipient].control {
                self.actors
                    .get_mut(&recipient)
                    .ok_or(GameError::UnknownActor)?
                    .control = control;
            }
            if self.forced_flee_location(recipient).is_some() {
                continue;
            }
            for item in entered {
                let (source, location, payload) = &contacts[&item];
                let Some(source) = source else {
                    continue;
                };
                if self.apply_control_condition(*source, recipient, payload)? {
                    let mut outcome = self.actor_outcome(recipient)?;
                    outcome.state.control.flee = self
                        .owned_control_condition(recipient, &payload.condition)
                        .map(|owned| FleeSource {
                            owned,
                            location: *location,
                        });
                    self.apply_outcome(outcome)?;
                    if self.forced_flee_location(recipient).is_some() {
                        if let Some(name) = self.sighted_actor_name(*source, recipient) {
                            self.narrate_gameplay(
                                *source,
                                format!("The {name} recoils and flees."),
                            );
                        }
                    }
                    break;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "control_scroll_tests.rs"]
mod tests;
