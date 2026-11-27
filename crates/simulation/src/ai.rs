//! Decisions use disclosed observations and remembered navigation, never hidden targets.
use crate::{Action, ActorId, Game, GameError};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::collections::BTreeMap;
use tor_world::{Direction, Location};

fn low_health(current: u64, maximum: u64, percent: u32) -> bool {
    u128::from(current) * 100 <= u128::from(maximum) * u128::from(percent)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AiProfile {
    pub idle: crate::behavior::IdleBehavior,
    pub doors: bool,
    pub care: bool,
    pub equipment: bool,
    pub loot: bool,
    pub gold: bool,
    pub flee_percent: u32,
}
impl<'de> Deserialize<'de> for AiProfile {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            idle: crate::behavior::IdleBehavior,
            doors: bool,
            care: bool,
            equipment: bool,
            loot: bool,
            gold: bool,
            flee_percent: u32,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            idle: fields.idle,
            doors: fields.doors,
            care: fields.care,
            equipment: fields.equipment,
            loot: fields.loot,
            gold: fields.gold,
            flee_percent: fields.flee_percent,
        })
    }
}
impl Default for AiProfile {
    fn default() -> Self {
        Self {
            idle: crate::behavior::IdleBehavior::Explore,
            doors: false,
            care: false,
            equipment: false,
            loot: false,
            gold: false,
            flee_percent: 25,
        }
    }
}
impl AiProfile {
    /// Explicit generic adventurer policy; species still own acquisition.
    pub fn general() -> Self {
        Self {
            idle: crate::behavior::IdleBehavior::Explore,
            doors: true,
            care: true,
            equipment: true,
            loot: true,
            gold: false,
            flee_percent: 25,
        }
    }
    pub fn valid(&self) -> bool {
        self.flee_percent <= 100
    }
}

impl From<crate::behavior::BehaviorPolicy> for AiProfile {
    fn from(policy: crate::behavior::BehaviorPolicy) -> Self {
        Self {
            idle: policy.idle,
            doors: policy.doors,
            care: policy.care,
            equipment: policy.equipment,
            loot: policy.loot,
            gold: policy.gold,
            flee_percent: policy.flee_percent,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    #[default]
    Search,
    Attack,
    Flee,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ai {
    pub profile: AiProfile,
    pub state: State,
    pub visits: crate::navigation_map::RegionMap<Location, u64>,
    /// A remembered idle destination, never a stored path or hidden-world claim.
    pub exploration_destination: Option<Location>,
}

fn read_exploration_destination<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<Location>, D::Error> {
    struct Destination(Location);
    impl<'de> Deserialize<'de> for Destination {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            crate::public_plan::read_location(d).map(Self)
        }
    }
    Option::<Destination>::deserialize(d).map(|value| value.map(|value| value.0))
}

impl<'de> Deserialize<'de> for Ai {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            profile: AiProfile,
            state: State,
            visits: crate::navigation_map::RegionMap<Location, u64>,
            #[serde(deserialize_with = "read_exploration_destination")]
            exploration_destination: Option<Location>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            profile: fields.profile,
            state: fields.state,
            visits: fields.visits,
            exploration_destination: fields.exploration_destination,
        })
    }
}

/// An uninterrupted decision owns evaluated ability operands through commit.
pub(crate) struct AiDecision {
    pub action: Action,
    pub ai: Ai,
    pub behavior: crate::behavior::Behavior,
    pub plan: Option<crate::abilities::Plan>,
    pub numeric_budget_refusal: bool,
}
impl AiDecision {
    fn action(action: Action, ai: Ai, behavior: crate::behavior::Behavior) -> Self {
        Self {
            action,
            ai,
            behavior,
            plan: None,
            numeric_budget_refusal: false,
        }
    }
    fn ability(plan: crate::abilities::Plan, ai: Ai, behavior: crate::behavior::Behavior) -> Self {
        Self {
            action: plan.work().action(),
            ai,
            behavior,
            plan: Some(plan),
            numeric_budget_refusal: false,
        }
    }
}

/// A stage proposes either an ordinary request or already-evaluated operands.
/// Converting the winner attaches the single copied AI/behavior draft; neither
/// branch executes work or commits state during choice.
enum AiChoice {
    Frozen(crate::abilities::Plan),
    Request(Action),
}
impl From<crate::abilities::Plan> for AiChoice {
    fn from(plan: crate::abilities::Plan) -> Self {
        Self::Frozen(plan)
    }
}
impl AiChoice {
    fn decision(self, ai: Ai, behavior: crate::behavior::Behavior) -> AiDecision {
        match self {
            Self::Frozen(plan) => AiDecision::ability(plan, ai, behavior),
            Self::Request(action) => AiDecision::action(action, ai, behavior),
        }
    }
}
#[path = "ai_attack_selection.rs"]
mod attack_selection;
use attack_selection::AttackSelectionMode;

#[path = "ai_encounter.rs"]
mod encounter;

#[path = "ai_decision_stages.rs"]
mod decision_stages;
#[path = "ai_exploration.rs"]
mod exploration;
#[path = "ai_flight_safety.rs"]
mod flight_safety;
#[path = "ai_projectiles.rs"]
mod projectiles;
#[path = "ai_tactical_movement.rs"]
mod tactical_movement;

// Priority-ordered transition table: frightened, target known, resulting state.
const TRANSITIONS: [(bool, bool, State); 4] = [
    (true, true, State::Flee),
    (false, true, State::Attack),
    (true, false, State::Search),
    (false, false, State::Search),
];

/// Only positive healing, across all compiled active branches and riders.
pub(crate) fn safe_healing_plan(plan: &crate::abilities::Plan) -> bool {
    use crate::abilities::{Plan, PlannedAmount, PlannedEffect};
    let Plan::Ability(ability) = plan else {
        return false;
    };
    let mut positive = false;
    let harmless = ability
        .strikes
        .iter()
        .flat_map(|strike| {
            strike
                .on_success
                .iter()
                .chain(&strike.on_failure)
                .chain(strike.riders.iter().flat_map(|rider| &rider.effects))
        })
        .all(|effect| match effect {
            PlannedEffect::Heal { amount } => {
                positive |= match amount {
                    PlannedAmount::Fixed { amount } => *amount > 0,
                    PlannedAmount::Rolled { dice } => dice.maximum() > 0,
                };
                true
            }
            _ => false,
        });
    harmless && positive
}

/// Every frozen resource operand participates in a fresh AI affordability gate.
pub(crate) fn resource_budget_satisfies(
    ledger: &crate::costs::CostLedger,
    bundle: crate::costs::CostBundle,
    minimum_percent: u8,
) -> bool {
    bundle.entries().all(|cost| {
        let available = ledger.available(cost.resource);
        cost.start
            .checked_add(cost.resolution)
            .is_some_and(|total| available >= total)
            && u64::from(available) * 100
                >= u64::from(ledger.resources().maximum(cost.resource)) * u64::from(minimum_percent)
    })
}

impl Game {
    pub(crate) fn ai_plan_resources_available(&self, plan: &crate::abilities::Plan) -> bool {
        let Some(cost) = plan.cost() else {
            return true;
        };
        let ability = plan.ability_id();
        let minimum = ability
            .and_then(|id| self.content.rules().ability(id))
            .map_or(0, |definition| definition.ai.minimum_resource_percent);
        self.creature(plan.actor())
            .is_some_and(|creature| resource_budget_satisfies(creature.costs(), cost, minimum))
    }
}

impl Game {
    pub fn configure_ai(&mut self, actor: ActorId, profile: AiProfile) -> Result<(), GameError> {
        let mut candidate = self.clone();
        candidate.configure_ai_in_transaction(actor, profile)?;
        *self = candidate;
        Ok(())
    }
    fn configure_ai_in_transaction(
        &mut self,
        actor: ActorId,
        profile: AiProfile,
    ) -> Result<(), GameError> {
        if !profile.valid() {
            return Err(GameError::Rule(
                crate::abilities::RuleError::InvalidDefinition,
            ));
        }
        if self.health(actor).is_none() {
            return Err(GameError::UnknownActor);
        }
        if let Some(ai) = self.combat.ai.get_mut(&actor) {
            if profile.idle == crate::behavior::IdleBehavior::Wait {
                ai.exploration_destination = None;
            }
            ai.profile = profile;
        } else {
            self.combat.ai.insert(
                actor,
                Ai {
                    profile,
                    state: State::Search,
                    visits: Default::default(),
                    exploration_destination: None,
                },
            );
        }
        self.reconcile_awareness()
    }

    pub fn is_ai(&self, actor: ActorId) -> bool {
        self.combat.ai.contains_key(&actor)
    }

    pub fn next_ai_action(&self) -> Option<(ActorId, Action)> {
        let id = self.next_actor()?;
        self.choose_ai(id).map(|decision| (id, decision.action))
    }

    pub(crate) fn choose_ai(&self, id: ActorId) -> Option<AiDecision> {
        let mut ai = self.combat.ai.get(&id)?.clone();
        let actor = self.actors.get(&id)?;
        let policy = actor.behavior_policy()?;
        let mut behavior = actor.behavior.clone();
        behavior.engagement = actor.engagement_at(self.actor_clock(id));
        let snapshot = self.decision_snapshot(id).ok()?;
        let view = &snapshot.view;
        let retained = self.preparation(id).filter(|preparation| {
            !preparation.active
                && self.preparation_still_available(preparation, Some(&snapshot.sight))
        });
        let committed = retained.filter(|preparation| preparation.has_prefix());
        // The private fact that another observer recognizes a guise is not
        // knowledge owned by its wearer. An intact ambush guise waits for actual
        // touch or incoming hostility; mere adjacency never selects an attack.
        if actor.perception().readied_touch_ambush
            && actor.guise.appearance().is_some()
            && retained.is_none()
        {
            if !self.actor_frozen(id) && self.condition_allows_voluntary_input(id) {
                if let Some(toucher) = actor.readied_ambush.toucher() {
                    if let Some(ability) = self
                        .creature(id)
                        .and_then(|creature| crate::readied_ambush::eligible_ability(&creature))
                    {
                        if let Ok(plan) = self.compile_touch_strike_plan(id, &ability, toucher) {
                            if self.ai_plan_resources_available(&plan) {
                                // Commit installs this draft after starting the work;
                                // keep the same touch-owned continuation in both paths.
                                if let Some(engagement) =
                                    self.touch_continuation(&plan, false).ok()?
                                {
                                    behavior.engagement = Some(engagement);
                                }
                                return Some(AiDecision::ability(plan, ai, behavior));
                            }
                        }
                    }
                }
            }
            return Some(AiDecision::action(Action::Wait, ai, behavior));
        }
        let mut search = self.route_search(id).ok();
        let mut route = |destination| search.as_mut()?.route(destination).ok();
        // Greedy acquisition and Search share one positive/negative gold choice.
        let mut gold_choice = decision_stages::GoldChoiceCache::default();
        let encounter = self.refresh_ai_encounter(
            encounter::EncounterInputs {
                actor: id,
                policy,
                has_retained: retained.is_some(),
                snapshot: &snapshot,
            },
            &mut ai,
            &mut behavior,
            &mut route,
            &mut gold_choice,
        )?;
        let encounter::EncounterFacts {
            sensed_target,
            threat,
            alert,
            health: (hp, max),
        } = encounter;
        // A suspended prefix cannot be replaced by new healing, gear or attacks.
        // Only a chosen ordinary retreat can abandon it, paying the whole tail.
        if ai.state != State::Flee {
            if let Some(preparation) = retained {
                return Some(AiDecision::ability(preparation.plan.clone(), ai, behavior));
            }
        }
        if let Some(choice) =
            self.choose_fresh_care(id, &ai, retained.is_some(), (hp, max), &snapshot)
        {
            return Some(choice.decision(ai, behavior));
        }
        // Care and retained work take precedence. Random movement uses only
        // disclosed legal candidates and the copied actor-owned decision RNG.
        if retained.is_none() && !self.condition_blocks_action(id, &Action::Move(Direction::North))
        {
            if let Some(chance) = actor.locomotion().erratic.filter(|p| p.numerator > 0) {
                let candidates: Vec<_> = Direction::HORIZONTAL
                    .into_iter()
                    .chain([Direction::Up, Direction::Down])
                    .filter(|direction| {
                        self.actor_translation(id, *direction)
                            .is_some_and(|(at, _)| {
                                view.visible_cells
                                    .iter()
                                    .any(|cell| cell.location == at && !cell.wall)
                            })
                    })
                    .collect();
                if !candidates.is_empty() {
                    let draw = crate::dice::random_word(&mut behavior.rng);
                    if draw % u64::from(chance.denominator) < u64::from(chance.numerator) {
                        let choice =
                            crate::dice::random_word(&mut behavior.rng) as usize % candidates.len();
                        // A random deviation replaces an idle interest rather
                        // than forcing the next ordinary move to undo it. This
                        // is only a draft: admission/cancellation cannot clear
                        // the authoritative goal before successful execution.
                        ai.exploration_destination = None;
                        return Some(AiDecision::action(
                            Action::Move(candidates[choice]),
                            ai,
                            behavior,
                        ));
                    }
                }
            }
        }
        if ai.state == State::Flee {
            let threat = threat.expect("flee has a disclosed or remembered threat");
            if let Some(direction) = self.ai_retreat_direction(
                id,
                threat,
                route(threat).map(|route| route.len()),
                view,
                &snapshot.scene,
            ) {
                // A technique may use only this already disclosed legal retreat.
                // Paid prefixes still escape through the ordinary atomic Move.
                if committed.is_none() {
                    if let Some(plan) =
                        self.choose_ai_step_ability(id, direction, &snapshot.sight, view)
                    {
                        return Some(AiDecision::ability(plan, ai, behavior));
                    }
                }
                return Some(AiDecision::action(Action::Move(direction), ai, behavior));
            }
        }
        // No admissible retreat: finish the frozen commitment rather than
        // deadlocking on a fresh attack or silently replaying the opening hit.
        if let Some(preparation) = committed {
            return Some(AiDecision::ability(preparation.plan.clone(), ai, behavior));
        }
        if let Some(choice) = self.choose_ai_engagement(
            id,
            &mut ai,
            &mut behavior,
            &snapshot,
            &mut route,
            sensed_target,
        ) {
            return Some(choice.decision(ai, behavior));
        }
        // An anonymous cue licenses investigation only, never a target attack.
        if ai.state != State::Flee && behavior.engagement.is_none() {
            if let Some(at) = alert.filter(|at| *at != view.location) {
                if let Some(step) = route(at).and_then(|r| r.first().copied()) {
                    if self.actor_translation(id, step.direction).is_some() {
                        return Some(AiDecision::action(
                            Action::Move(step.direction),
                            ai,
                            behavior,
                        ));
                    }
                }
            }
            ai.state = State::Search;
        }
        if let Some(action) =
            self.choose_safe_acquisition(id, &ai, &snapshot, &mut route, &mut gold_choice)
        {
            return Some(AiDecision::action(action, ai, behavior));
        }
        if ai.state == State::Search
            && ai.profile.doors
            && self.condition_allows_voluntary_input(id)
        {
            if let Some(door) = view
                .visible_cells
                .iter()
                .filter(|c| c.door_reachable)
                .filter_map(|c| c.door)
                .find(|d| !d.open)
            {
                return Some(AiDecision::action(
                    Action::SetDoor {
                        door: door.id,
                        open: true,
                    },
                    ai,
                    behavior,
                ));
            }
        }
        if let Some(action) = self.choose_exploration(
            id,
            &mut ai,
            &mut behavior,
            retained.is_some(),
            view,
            search.as_mut(),
        ) {
            return Some(AiDecision::action(action, ai, behavior));
        }
        Some(AiDecision::action(Action::Wait, ai, behavior))
    }

    fn choose_ai_healing(
        &self,
        actor: ActorId,
        sight: &[tor_world::SightCell],
        view: &crate::Observation,
    ) -> Option<crate::abilities::Plan> {
        let creature = self.creature(actor)?;
        let rules = creature.build().ruleset();
        let mut candidates: Vec<_> = creature
            .derived()
            .abilities
            .iter()
            .filter_map(|id| {
                rules
                    .ability(id)
                    .filter(|definition| !definition.ai.hostile)
                    .map(|definition| (definition.ai.priority, id, definition))
            })
            .collect();
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        for (_, id, definition) in candidates {
            let plan = if let Some(pending) = self.preparation(actor).filter(|pending| {
                pending
                    .plan
                    .as_ability()
                    .is_some_and(|plan| &plan.ability == id && plan.target == actor)
                    && self.preparation_still_available(pending, Some(sight))
            }) {
                pending.plan.clone()
            } else {
                // These gates apply only to a fresh preparation. The retained
                // plan already owns its resolution cost and finite-item claims.
                if let Some(cost) = &definition.cost {
                    let available = creature.costs().available(cost.resource);
                    if cost
                        .start
                        .checked_add(cost.resolution)
                        .is_none_or(|total| total > available)
                        || u64::from(available) * 100
                            < u64::from(creature.costs().resources().maximum(cost.resource))
                                * u64::from(definition.ai.minimum_resource_percent)
                    {
                        continue;
                    }
                }
                let Ok(plan) = self.ability_plan_in_scene(actor, id, actor, sight) else {
                    continue;
                };
                plan
            };
            if self.effective_safe_healing_plan(&plan) {
                if let Some(plan) = self.physical_ai_plan(plan, view) {
                    return Some(plan);
                }
            }
        }
        None
    }

    pub(crate) fn known_healing(&self, actor: ActorId, item: crate::ItemId) -> bool {
        let Some(item) = self.items.get(&item) else {
            return false;
        };
        let Some(actor) = self.actors.get(&actor) else {
            return false;
        };
        (!item.spec.concealed || actor.knowledge.contains(&item.spec.identity))
            && item.spec.consumable.as_ref().is_some_and(|consumable| {
                !consumable.effects.is_empty()
                    && consumable
                        .effects
                        .iter()
                        .all(|effect| matches!(effect, crate::rules::Effect::Heal { amount } if match amount {
                            crate::rules::AmountExpr::Fixed { amount } => *amount > 0,
                            crate::rules::AmountExpr::Rolled { dice } => crate::dice::DicePool::new(dice.count, dice.sides, dice.bonus).is_ok_and(|pool| pool.maximum() > 0),
                        }))
            })
    }
}

impl Game {
    fn choose_ai_step_ability(
        &self,
        actor: ActorId,
        direction: Direction,
        sight: &[tor_world::SightCell],
        view: &crate::Observation,
    ) -> Option<crate::abilities::Plan> {
        use crate::abilities::{AbilityInvocation, InvocationKind};
        use crate::rules::RangePreference;
        let creature = self.creature(actor)?;
        let rules = creature.build().ruleset();
        let invocation = AbilityInvocation::self_step(direction.try_into().ok()?);
        let in_melee = view.visible_actors.iter().any(|other| {
            self.perceived_hostile(actor, other.id, other.disclosure)
                && self.attack_available_in_scene(actor, other.id, sight)
        });
        let eligible = |definition: &crate::rules::AbilityDef| {
            definition.invocation == InvocationKind::SelfStep
                && !definition.ai.hostile
                && (definition.ai.range_preference != RangePreference::OutsideMelee || !in_melee)
        };
        // An exact paused selection retains its captured clock, claims and payment.
        if let Some(preparation) = self
            .preparation(actor)
            .filter(|pending| !pending.active && !pending.has_prefix())
        {
            if let Some(plan) = preparation.plan.as_ability() {
                if plan.invocation == invocation
                    && rules.ability(&plan.ability).is_some_and(eligible)
                    && self.preparation_still_available(preparation, Some(sight))
                {
                    return Some(preparation.plan.clone());
                }
            }
        }
        let mut candidates: Vec<_> = creature
            .derived()
            .abilities
            .iter()
            .filter_map(|id| {
                let definition = rules.ability(id)?;
                eligible(definition).then_some((definition.ai.priority, id, definition))
            })
            .collect();
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        for (_, id, definition) in candidates {
            if let Some(cost) = definition.cost.as_ref() {
                let available = creature.costs().available(cost.resource);
                let maximum = creature.costs().resources().maximum(cost.resource);
                if cost
                    .start
                    .checked_add(cost.resolution)
                    .is_none_or(|total| available < total)
                    || u64::from(available) * 100
                        < u64::from(maximum) * u64::from(definition.ai.minimum_resource_percent)
                {
                    continue;
                }
            }
            let Ok(plan) = self.ability_invocation_plan_in_scene(actor, id, invocation, sight)
            else {
                continue;
            };
            if let Some(plan) = self.physical_ai_plan(plan, view) {
                return Some(plan);
            }
        }
        None
    }
}

/// Choose solely from disclosed physical occurrences. Neither first-hit bodies,
/// terrain collision, hidden defenses nor a ray trace participates in this choice.
fn disclosed_flight_aim(
    target: ActorId,
    sight: &[tor_world::SightCell],
    view: &crate::Observation,
    eye: [i32; 3],
    range: u16,
) -> Option<crate::abilities::FlightAim> {
    disclosed_flight_aim_with_range(target, sight, view, eye, Some(range))
}

fn disclosed_flight_aim_with_range(
    target: ActorId,
    sight: &[tor_world::SightCell],
    view: &crate::Observation,
    eye: [i32; 3],
    range: Option<u16>,
) -> Option<crate::abilities::FlightAim> {
    if let Some(direction) = disclosed_ray_direction_with_range(target, sight, view, eye, range) {
        return Some(direction.into());
    }
    let target = view.visible_actors.iter().find(|other| {
        other.id == target && other.disclosure == crate::perception::ActorDisclosure::Creature
    })?;
    sight
        .iter()
        .filter(|cell| cell.physical && !cell.wall && cell.location == target.location)
        .filter_map(|cell| {
            let offset = [
                cell.offset.x.checked_add(eye[0])?,
                cell.offset.y.checked_add(eye[1])?,
                cell.offset.z.checked_add(eye[2])?,
            ];
            let distance = offset.iter().map(|n| n.unsigned_abs()).max()?;
            (distance > 0 && range.is_none_or(|limit| distance <= u32::from(limit)))
                .then_some((distance, offset))
        })
        .min()
        .map(|(_, offset)| crate::abilities::FlightAim::Cell { offset })
}

fn disclosed_ray_direction(
    target: ActorId,
    sight: &[tor_world::SightCell],
    view: &crate::Observation,
    eye: [i32; 3],
    range: u16,
) -> Option<crate::abilities::StepDirection> {
    disclosed_ray_direction_with_range(target, sight, view, eye, Some(range))
}

fn disclosed_ray_direction_with_range(
    target: ActorId,
    sight: &[tor_world::SightCell],
    view: &crate::Observation,
    eye: [i32; 3],
    range: Option<u16>,
) -> Option<crate::abilities::StepDirection> {
    let target = view.visible_actors.iter().find(|other| {
        other.id == target && other.disclosure == crate::perception::ActorDisclosure::Creature
    })?;
    sight
        .iter()
        .filter(|cell| cell.physical && !cell.wall && cell.location == target.location)
        .filter_map(|cell| {
            let offset = [
                cell.offset.x.checked_add(eye[0])?,
                cell.offset.y.checked_add(eye[1])?,
                cell.offset.z.checked_add(eye[2])?,
            ];
            let distance = offset.iter().map(|value| value.unsigned_abs()).max()?;
            if distance == 0
                || range.is_some_and(|limit| distance > u32::from(limit))
                || offset
                    .iter()
                    .any(|value| *value != 0 && value.unsigned_abs() != distance)
            {
                return None;
            }
            let direction = Direction::from_delta(offset.map(|value| i64::from(value.signum())))?;
            let direction = crate::abilities::StepDirection::try_from(direction).ok()?;
            Some((distance, direction))
        })
        .min()
        .map(|(_, direction)| direction)
}

#[path = "ai_defense.rs"]
mod defense;

#[cfg(test)]
#[path = "ai_flee_tests.rs"]
mod flee_tests;

#[cfg(test)]
#[path = "ai_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "ai_defense_tests.rs"]
mod defense_tests;

#[cfg(test)]
mod wide_scalar_ratio_tests {
    use super::low_health;
    #[test]
    fn wide_scalar_ai_health_thresholds_are_exact_near_u64_maximum() {
        for maximum in [u64::from(u32::MAX) + 1, (1_u64 << 63), u64::MAX] {
            for percent in [0, 25, 50, 100] {
                let boundary =
                    u64::try_from(u128::from(maximum) * u128::from(percent) / 100).unwrap();
                assert!(low_health(boundary, maximum, percent));
                if let Some(next) = boundary.checked_add(1) {
                    assert!(!low_health(next, maximum, percent));
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "ai_idle_tests.rs"]
mod idle_tests;

#[cfg(test)]
#[path = "ai_ray_tests.rs"]
mod ray_tests;

#[cfg(test)]
#[path = "ai_item_use_tests.rs"]
mod item_use_tests;

#[cfg(test)]
#[path = "ai_exploration_tests.rs"]
mod exploration_tests;

#[cfg(test)]
#[path = "ai_stationary_tests.rs"]
mod stationary_tests;
