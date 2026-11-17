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
mod tests {
    use super::*;
    use crate::{combat::DamageType, test_creatures};
    use std::{collections::BTreeSet, num::NonZeroU64};
    use tor_world::{Position, RegionId};
    pub(super) fn at(region: u64, x: i32, y: i32) -> Location {
        Location {
            region: RegionId(region),
            position: Position { x, y, z: 0 },
        }
    }
    #[test]
    fn explore_equal_neighbors_use_actor_rng_without_mutating_preview() {
        let mut game = fixture();
        game.configure_run(
            ActorId(1),
            BTreeSet::from([ActorId(1)]),
            None,
            BTreeMap::new(),
        )
        .unwrap();
        let mut choices = BTreeSet::new();
        for seed in 0..26 {
            game.actors.get_mut(&ActorId(2)).unwrap().behavior.rng = seed;
            let before = game.clone();
            let decision = game.choose_ai(ActorId(2)).unwrap();
            assert!(matches!(decision.action, Action::Move(_)));
            assert_eq!(decision.action, game.choose_ai(ActorId(2)).unwrap().action);
            assert_eq!(game, before, "preview must not commit RNG or visit history");
            assert_ne!(
                decision.behavior.rng,
                before.actors[&ActorId(2)].behavior.rng
            );
            choices.insert(format!("{:?}", decision.action));
        }
        assert!(
            choices.len() >= 3,
            "equally good neighbors must not all resolve by coordinate: {choices:?}"
        );
    }

    #[test]
    fn explore_local_fallback_unique_neighbor_and_idle_wait_do_not_draw() {
        let mut game = fixture();
        game.configure_run(
            ActorId(1),
            BTreeSet::from([ActorId(1)]),
            None,
            BTreeMap::new(),
        )
        .unwrap();
        let candidates: Vec<_> = Direction::HORIZONTAL
            .into_iter()
            .filter_map(|direction| {
                game.actor_translation(ActorId(2), direction)
                    .map(|(at, _)| (at, direction))
            })
            .collect();
        assert!(candidates.len() > 2);
        let preferred = candidates[1];
        for (at, _) in candidates {
            game.combat
                .ai
                .get_mut(&ActorId(2))
                .unwrap()
                .visits
                .insert(at, if at == preferred.0 { 0 } else { 10 });
        }
        let rng = game.actors[&ActorId(2)].behavior.rng;
        let before = game.clone();
        let view = game.observe(ActorId(2)).unwrap();
        let mut ai = game.combat.ai[&ActorId(2)].clone();
        let mut behavior = game.actors[&ActorId(2)].behavior.clone();
        // This is the local fallback contract, now below the destination stage.
        // Missing remembered routing cannot make the unique local choice random.
        let action =
            game.choose_exploration(ActorId(2), &mut ai, &mut behavior, false, &view, None);
        assert_eq!(action, Some(Action::Move(preferred.1)));
        assert_eq!(behavior.rng, rng);
        assert_eq!(game, before);
        game.combat.ai.get_mut(&ActorId(2)).unwrap().profile.idle =
            crate::behavior::IdleBehavior::Wait;
        let decision = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(decision.action, Action::Wait);
        assert_eq!(decision.behavior.rng, rng);
    }

    #[test]
    fn explore_rng_commits_with_execution_and_survives_checkpoint() {
        let mut game = fixture();
        game.configure_run(
            ActorId(1),
            BTreeSet::from([ActorId(1)]),
            None,
            BTreeMap::new(),
        )
        .unwrap();
        game.act(ActorId(1), Action::Wait).unwrap();
        let chosen = game.choose_ai(ActorId(2)).unwrap();
        let rng = game.actors[&ActorId(2)].behavior.rng;
        game.admit_ai_intention(ActorId(2)).unwrap();
        assert_eq!(game.actors[&ActorId(2)].behavior.rng, rng);
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot =
            serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
                .unwrap();
        let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
        let mut restored =
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        for candidate in [&mut game, &mut restored] {
            let execution = candidate.execute_next_intention().unwrap();
            assert_eq!(execution.action, Some(chosen.action.clone()));
            execution.outcome.unwrap();
            assert_eq!(
                candidate.actors[&ActorId(2)].behavior.rng,
                chosen.behavior.rng
            );
            assert_eq!(candidate.combat.ai[&ActorId(2)].visits, chosen.ai.visits);
        }
        assert_eq!(game, restored);
    }

    #[test]
    fn invalid_ai_profile_and_missing_creature_have_distinct_errors() {
        let mut game = fixture();
        let before = game.clone();
        assert_eq!(
            game.configure_ai(
                ActorId(2),
                AiProfile {
                    flee_percent: 101,
                    ..AiProfile::general()
                }
            ),
            Err(GameError::Rule(
                crate::abilities::RuleError::InvalidDefinition
            ))
        );
        assert_eq!(
            game.configure_ai(ActorId(999), AiProfile::general()),
            Err(GameError::UnknownActor)
        );
        assert_eq!(game, before);
    }

    fn fixture() -> Game {
        fixture_with_anatomy(crate::AnatomySpec::from_rules(
            &crate::test_creatures::ruleset(),
        ))
    }
    fn fixture_with_anatomy(anatomy: crate::AnatomySpec) -> Game {
        let hero = test_creatures::species();
        let mut foe = test_creatures::species();
        foe.anatomy = anatomy;
        configured_game(hero, foe, paid_probe_rules())
    }
    pub(super) fn configured_game(
        mut hero: crate::creatures::Species,
        mut foe: crate::creatures::Species,
        rules: std::sync::Arc<crate::rules::Ruleset>,
    ) -> Game {
        use crate::{
            attributes::ManaBinding,
            content_ids::SpeciesId,
            creatures::{Content, CreatureBuild, ScenarioCatalog},
            progression::{HdLedger, HdSource, HitDie},
        };
        hero.id = SpeciesId::new("hero").unwrap();
        foe.id = SpeciesId::new("foe").unwrap();
        foe.behavior.memory_ticks = 50;
        let content = Content::new(
            rules,
            ScenarioCatalog {
                species: BTreeMap::from([(hero.id.clone(), hero), (foe.id.clone(), foe)]),
                templates: BTreeMap::new(),
            },
        )
        .unwrap();
        let mut game = Game::two_room_in_stone(42, content.clone());
        for (x, name, faction, corpse_identity) in [
            (1, "hero", "hero", "item.corpse.test.hero"),
            (2, "foe", "foe", "item.corpse.test.foe"),
        ] {
            let actor = game
                .spawn_actor(at(1, x, 1), NonZeroU64::new(100).unwrap())
                .unwrap();
            let build = CreatureBuild::new(
                content.clone(),
                SpeciesId::new(name).unwrap(),
                HdLedger::new(vec![HitDie::new(HdSource::Racial, 42)]).unwrap(),
                ManaBinding::Intellect,
                [
                    tor_simulation::attributes::Skill::Deception,
                    tor_simulation::attributes::Skill::Persuasion,
                ],
            )
            .unwrap();
            game.configure_creature(
                actor,
                crate::CreatureIdentity {
                    corpse_identity: corpse_identity.parse().unwrap(),
                    name: name.into(),
                    faction: faction.into(),
                },
                build,
            )
            .unwrap();
        }
        game.configure_run(
            ActorId(1),
            BTreeSet::from([ActorId(1)]),
            None,
            BTreeMap::from([("foe".into(), BTreeSet::from(["hero".into()]))]),
        )
        .unwrap();
        game.configure_ai(ActorId(2), AiProfile::general()).unwrap();
        game.refresh_navigation();
        game
    }
    fn paid_probe_rules() -> std::sync::Arc<crate::rules::Ruleset> {
        let mut document = test_creatures::ruleset().document().clone();
        document.abilities.extend([
            test_creatures::paid_melee(),
            test_creatures::mana_ray(),
            test_creatures::focus_fear(),
        ]);
        crate::rules::Ruleset::validate(document).unwrap()
    }
    pub(super) fn paid_fixture(magical: bool, fear: bool) -> Game {
        use crate::{
            attributes::{Attributes, ByAttribute},
            content_ids::{AbilityId, DescriptorId, FlagId},
            rules::{Modifier, ModifierOp, Scope, Selector},
        };
        let mut hero = test_creatures::species();
        hero.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Immunity {
                selector: Selector::Descriptor {
                    descriptor: DescriptorId::new("fear").unwrap(),
                },
            },
        });
        let mut foe = test_creatures::species();
        foe.default_attributes = Attributes::new(ByAttribute {
            strength: 2,
            speed: 1,
            intellect: 2,
            willpower: 2,
            awareness: 1,
            presence: 1,
        })
        .unwrap();
        for name in ["fixture_paid_melee", "fixture_mana_ray"]
            .into_iter()
            .chain(fear.then_some("fixture_focus_fear"))
        {
            foe.modifiers.push(Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: AbilityId::new(name).unwrap(),
                },
            });
        }
        if magical {
            foe.modifiers.push(Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Flag {
                    flag: FlagId::new("magical").unwrap(),
                },
            });
        }
        configured_game(hero, foe, paid_probe_rules())
    }
    pub(super) fn basic(target: ActorId) -> Action {
        Action::UseAbility {
            id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
            invocation: crate::abilities::AbilityInvocation::actor_target(target),
            supplies: Default::default(),
        }
    }
    fn heal(amount: u32) -> crate::rules::Effect {
        crate::rules::Effect::Heal {
            amount: crate::rules::AmountExpr::Fixed { amount },
        }
    }
    fn injury(game: &mut Game, actor: ActorId, amount: u32) {
        let damage = crate::damage::DamageSpec::new(
            vec![crate::damage::DamageComponent::fixed(
                DamageType::Vital,
                None,
                amount,
            )],
            None,
        )
        .unwrap();
        game.apply_damage(actor, &damage).unwrap();
    }

    #[test]
    fn paid_choices_do_not_read_hidden_fear_immunity() {
        let game = paid_fixture(false, true);
        assert!(game
            .creature(ActorId(1))
            .unwrap()
            .derived()
            .protection
            .has_immunity(&crate::rules::Selector::Descriptor {
                descriptor: crate::content_ids::DescriptorId::new("fear").unwrap()
            }));
        let before = game.clone();
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().action,
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("fixture_focus_fear").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(1)),
                supplies: Default::default(),
            }
        );
        assert_eq!(game, before);
    }

    #[test]
    fn paid_choices_use_granted_power_strike_without_mutating_state() {
        let game = paid_fixture(false, false);
        let before = game.clone();
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().action,
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("fixture_paid_melee").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(1)),
                supplies: Default::default(),
            }
        );
        assert_eq!(game, before);
    }

    fn ranged_specialist_fixture(magical: bool) -> Game {
        ranged_specialist_fixture_with_modifiers(magical, Vec::new())
    }
    fn ranged_specialist_fixture_with_modifiers(
        magical: bool,
        modifiers: Vec<crate::rules::Modifier>,
    ) -> Game {
        use crate::{
            content_ids::{AbilityId, FlagId},
            rules::{Modifier, ModifierOp, Scope},
        };
        let mut foe = test_creatures::species();
        for op in [
            ModifierOp::GrantAbility {
                ability: AbilityId::new("fixture_mana_ray").unwrap(),
            },
            ModifierOp::Flag {
                flag: FlagId::new("magical").unwrap(),
            },
        ] {
            if magical || matches!(op, ModifierOp::GrantAbility { .. }) {
                foe.modifiers.push(Modifier {
                    cancellable: false,
                    scope: Scope::Always,
                    op,
                });
            }
        }
        foe.modifiers.extend(modifiers);
        configured_game(test_creatures::species(), foe, paid_probe_rules())
    }

    #[test]
    fn funded_ranged_specialist_repositions_before_basic_melee() {
        let game = ranged_specialist_fixture(true);
        let before = game.clone();
        let decision = game.choose_ai(ActorId(2)).unwrap();
        assert!(
            matches!(decision.action, Action::Move(_)),
            "funded ranged specialist should seek shooting space, got {:?}",
            decision.action
        );
        assert_eq!(game, before, "preview remains pure");
    }

    #[test]
    fn ranged_positioning_supports_upright_eye_height() {
        let mut game = ranged_specialist_fixture(true);
        game.set_body(
            ActorId(2),
            crate::BodySpec {
                cells: vec![[0, 0, 0], [0, 0, 1]],
                eye: [0, 0, 1],
                mass: 80,
            },
        )
        .unwrap();
        assert!(matches!(
            game.choose_ai(ActorId(2)).unwrap().action,
            Action::Move(_)
        ));
    }

    #[test]
    fn ranged_positioning_rejects_negative_eye_without_disclosure() {
        let mut game = ranged_specialist_fixture(true);
        for actor in [ActorId(1), ActorId(2)] {
            let mut location = game.actors[&actor].location;
            location.position.z = 1;
            game.teleport(actor, location).unwrap();
        }
        game.set_body(
            ActorId(2),
            crate::BodySpec {
                cells: vec![[0, 0, 0], [0, 0, -1]],
                eye: [0, 0, -1],
                mass: 80,
            },
        )
        .unwrap();
        let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
        let mut view = snapshot.view.clone();
        assert!(view
            .visible_actors
            .iter()
            .any(|other| other.id == ActorId(1)));
        view.visible_cells = tor_world::Shared::new(Vec::new());
        assert_eq!(
            game.ai_ranged_reposition(
                ActorId(2),
                ActorId(1),
                &snapshot.sight,
                &snapshot.scene,
                &view
            ),
            None
        );
    }

    #[test]
    fn ranged_positioning_rejects_wide_or_pitched_own_bodies() {
        let mut wide = ranged_specialist_fixture(true);
        wide.set_body(
            ActorId(2),
            crate::BodySpec {
                cells: vec![[0, 0, 0], [0, 1, 0]],
                eye: [0, 0, 0],
                mass: 80,
            },
        )
        .unwrap();
        assert!(!matches!(
            wide.choose_ai(ActorId(2)).unwrap().action,
            Action::Move(_)
        ));
        let mut pitched = ranged_specialist_fixture(true);
        pitched
            .set_body(
                ActorId(2),
                crate::BodySpec {
                    cells: vec![[0, 0, 0], [0, 0, 1]],
                    eye: [0, 0, 1],
                    mass: 80,
                },
            )
            .unwrap();
        let rotation = (0..24)
            .find(|r| tor_world::rotate_vector(*r, [0, 0, 1]) == [1, 0, 0])
            .unwrap();
        pitched.actors.get_mut(&ActorId(2)).unwrap().orientation = rotation;
        pitched.refresh_navigation();
        assert!(!matches!(
            pitched.choose_ai(ActorId(2)).unwrap().action,
            Action::Move(_)
        ));
    }

    #[test]
    fn ranged_positioning_requires_disclosed_destination_and_corridor() {
        let game = ranged_specialist_fixture(true);
        let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
        assert!(game
            .ai_ranged_reposition(
                ActorId(2),
                ActorId(1),
                &snapshot.sight,
                &snapshot.scene,
                &snapshot.view
            )
            .is_some());
        let mut view = snapshot.view.clone();
        view.visible_cells = tor_world::Shared::new(Vec::new());
        assert_eq!(
            game.ai_ranged_reposition(
                ActorId(2),
                ActorId(1),
                &snapshot.sight,
                &snapshot.scene,
                &view
            ),
            None
        );
    }

    #[test]
    fn ranged_positioning_requires_upper_body_disclosure() {
        let mut game = ranged_specialist_fixture(true);
        game.set_body(
            ActorId(2),
            crate::BodySpec {
                cells: vec![[0, 0, 0], [0, 0, 1]],
                eye: [0, 0, 0],
                mass: 80,
            },
        )
        .unwrap();
        let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
        assert!(game
            .ai_ranged_reposition(
                ActorId(2),
                ActorId(1),
                &snapshot.sight,
                &snapshot.scene,
                &snapshot.view
            )
            .is_some());
        let mut view = snapshot.view.clone();
        // Keep the entire ground-level corridor and support, omit only upper
        // destination body cells. Eye height zero keeps them outside the corridor.
        view.visible_cells = tor_world::Shared::new(
            view.visible_cells
                .iter()
                .filter(|cell| !(cell.location.position.x == 3 && cell.location.position.z == 1))
                .cloned()
                .collect(),
        );
        assert_eq!(
            game.ai_ranged_reposition(
                ActorId(2),
                ActorId(1),
                &snapshot.sight,
                &snapshot.scene,
                &view
            ),
            None
        );
    }

    #[test]
    fn ranged_positioning_requires_intermediate_corridor_disclosure() {
        let game = ranged_specialist_fixture(true);
        let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
        assert!(game
            .ai_ranged_reposition(
                ActorId(2),
                ActorId(1),
                &snapshot.sight,
                &snapshot.scene,
                &snapshot.view
            )
            .is_some());
        let mut view = snapshot.view.clone();
        // Preserve every destination cell and ground support. Each candidate's
        // rectangle crosses x=2, between the target x=1 and destination x=3.
        view.visible_cells = tor_world::Shared::new(
            view.visible_cells
                .iter()
                .filter(|cell| !(cell.location.position.x == 2 && cell.location.position.z == 0))
                .cloned()
                .collect(),
        );
        assert_eq!(
            game.ai_ranged_reposition(
                ActorId(2),
                ActorId(1),
                &snapshot.sight,
                &snapshot.scene,
                &view
            ),
            None
        );
    }

    #[test]
    fn ranged_positioning_requires_support_occurrence_not_location_alias() {
        let game = ranged_specialist_fixture(true);
        let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
        assert!(game
            .ai_ranged_reposition(
                ActorId(2),
                ActorId(1),
                &snapshot.sight,
                &snapshot.scene,
                &snapshot.view
            )
            .is_some());
        // Projection-boundary witness: location-only public rows remain, but
        // candidate footing is no longer disclosed at its physical offset.
        // This is not claimed to construct a real-world portal failure.
        let scene: Vec<_> = snapshot
            .scene
            .iter()
            .filter(|cell| {
                !(cell.location.region == RegionId(1)
                    && cell.location.position.x == 3
                    && cell.location.position.z == -1)
            })
            .copied()
            .collect();
        assert_eq!(
            game.ai_ranged_reposition(
                ActorId(2),
                ActorId(1),
                &snapshot.sight,
                &scene,
                &snapshot.view
            ),
            None
        );
    }

    #[test]
    fn ranged_positioning_footing_is_yaw_equivariant() {
        for orientation in (0..24).filter(|r| tor_world::rotate_vector(*r, [0, 0, 1]) == [0, 0, 1])
        {
            let mut game = ranged_specialist_fixture(true);
            game.actors.get_mut(&ActorId(2)).unwrap().orientation = orientation;
            game.refresh_navigation();
            assert!(
                matches!(game.choose_ai(ActorId(2)).unwrap().action, Action::Move(_)),
                "upright yaw {orientation} must retain disclosed footing"
            );
        }
    }

    #[test]
    fn ranged_positioning_rejects_non_downward_current_gravity() {
        for gravity in [[0, 0, 1], [1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0]] {
            let mut game = ranged_specialist_fixture(true);
            game.world.set_gravity(RegionId(1), gravity).unwrap();
            let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
            assert_eq!(
                game.ai_ranged_reposition(
                    ActorId(2),
                    ActorId(1),
                    &snapshot.sight,
                    &snapshot.scene,
                    &snapshot.view
                ),
                None,
                "unsupported current gravity {gravity:?}"
            );
        }
    }

    #[test]
    fn ranged_positioning_physical_downward_portal_boundary() {
        for rotation in (0..24).filter(|r| tor_world::rotate_vector(*r, [0, 0, 1]) == [0, 0, 1]) {
            let mut game = ranged_specialist_fixture(true);
            game.world.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
            let source = at(1, 3, 1);
            let destination = at(2, 1, 1);
            game.connect_portal_area(
                tor_world::Passage {
                    from: source,
                    direction: Direction::Down,
                    to: destination,
                },
                rotation,
                1,
                1,
            )
            .unwrap();
            let below = Location {
                position: Position {
                    z: -1,
                    ..source.position
                },
                ..source
            };
            assert!(game.world.is_wall(below));
            assert_eq!(
                game.world.physics_neighbor(source, Direction::Down),
                Some((destination, rotation))
            );
            let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
            let cross_region = snapshot.scene.iter().any(|cell| {
                cell.physical && cell.offset.z < 0 && cell.location.region != source.region
            });
            let disclosed_coordinate_support = snapshot
                .view
                .visible_cells
                .iter()
                .any(|cell| cell.location == below && cell.wall);
            assert!(
                cross_region || !disclosed_coordinate_support,
                "real snapshot must disclose the crossing or withhold coordinate-only support"
            );
            assert_ne!(
                game.ai_ranged_reposition(
                    ActorId(2),
                    ActorId(1),
                    &snapshot.sight,
                    &snapshot.scene,
                    &snapshot.view
                ),
                Some(Direction::East)
            );
        }
    }

    #[test]
    fn ranged_positioning_avoids_every_disclosed_hostile_body_cell() {
        let mut game = ranged_specialist_fixture(true);
        game.set_body(
            ActorId(1),
            crate::BodySpec {
                cells: vec![[0, 0, 0], [1, -1, 0]],
                eye: [0, 0, 0],
                mass: 80,
            },
        )
        .unwrap();
        let view = game.observe(ActorId(2)).unwrap();
        assert_eq!(
            view.visible_actors
                .iter()
                .filter(|row| row.id == ActorId(1))
                .count(),
            2
        );
        let Action::Move(direction) = game.choose_ai(ActorId(2)).unwrap().action else {
            panic!("expected disclosed safe alternative");
        };
        let (at, _) = game.actor_translation(ActorId(2), direction).unwrap();
        assert!(view
            .visible_actors
            .iter()
            .filter(|row| row.id == ActorId(1))
            .all(|row| at
                .position
                .x
                .abs_diff(row.location.position.x)
                .max(at.position.y.abs_diff(row.location.position.y))
                > 1));
    }

    #[test]
    fn ranged_positioning_effective_range_guard() {
        use crate::rules::{AbilityStat, Modifier, ModifierOp, Scope, Stat};
        let game = ranged_specialist_fixture_with_modifiers(
            true,
            vec![Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Ability {
                        ability: crate::content_ids::AbilityId::new("fixture_mana_ray").unwrap(),
                        stat: AbilityStat::Range,
                    },
                    value: -5,
                },
            }],
        );
        assert_eq!(
            game.creature(ActorId(2))
                .unwrap()
                .derived()
                .ability_reach(&crate::content_ids::AbilityId::new("fixture_mana_ray").unwrap())
                .unwrap(),
            crate::rules::Reach::Sight { range: 1 }
        );
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().action,
            basic(ActorId(1))
        );
    }

    #[test]
    fn ranged_positioning_ledge_guard() {
        let mut game = ranged_specialist_fixture(true);
        game.world.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
        for y in 0..3 {
            game.set_wall(
                Location {
                    region: RegionId(1),
                    position: Position { x: 3, y, z: -1 },
                },
                false,
            )
            .unwrap();
        }
        let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
        assert!(
            game.actor_translation(ActorId(2), Direction::East)
                .is_some(),
            "a legal step is not proof of support"
        );
        assert_eq!(
            game.ai_ranged_reposition(
                ActorId(2),
                ActorId(1),
                &snapshot.sight,
                &snapshot.scene,
                &snapshot.view
            ),
            None
        );
    }

    #[test]
    fn unfunded_ranged_specialist_does_not_reposition() {
        let game = ranged_specialist_fixture(false);
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().action,
            basic(ActorId(1))
        );
    }

    #[test]
    fn ranged_specialist_uses_created_space_for_a_shot() {
        let mut game = ranged_specialist_fixture(true);
        let Action::Move(direction) = game.choose_ai(ActorId(2)).unwrap().action else {
            panic!("expected reposition")
        };
        let (at, _) = game.actor_translation(ActorId(2), direction).unwrap();
        game.act(ActorId(1), Action::Wait).unwrap();
        game.act(ActorId(2), Action::Move(direction)).unwrap();
        assert_eq!(game.actors[&ActorId(2)].location, at);
        assert!(
            matches!(game.choose_ai(ActorId(2)).unwrap().action, Action::UseAbility { id, .. } if id.as_str() == "fixture_mana_ray")
        );
    }

    #[test]
    fn paid_choices_preserve_mana_for_ranged_combat() {
        let mut unfunded = paid_fixture(false, false);
        unfunded.teleport(ActorId(1), at(1, 4, 1)).unwrap();
        assert_eq!(
            unfunded.choose_ai(ActorId(2)).unwrap().action,
            Action::Move(Direction::East)
        );
        let mut game = paid_fixture(true, false);
        game.teleport(ActorId(1), at(1, 4, 1)).unwrap();
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().action,
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("fixture_mana_ray").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(1)),
                supplies: Default::default(),
            }
        );
    }
    #[test]
    fn one_decision_shares_its_search_across_visible_targets() {
        let mut game = fixture();
        for actor in [ActorId(1), ActorId(2)] {
            assert!(
                game.creature(actor).is_some(),
                "AI subjects own their builds"
            );
        }
        let other = game
            .spawn_actor(at(1, 3, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let build = game.creature(ActorId(1)).unwrap().build().clone();
        game.configure_creature(
            other,
            crate::CreatureIdentity {
                corpse_identity: "item.corpse.test.other".parse().unwrap(),
                name: "other".into(),
                faction: "hero".into(),
            },
            build,
        )
        .unwrap();
        game.refresh_navigation();
        assert_eq!(game.observe(ActorId(2)).unwrap().visible_actors.len(), 2);
        let before = crate::diagnostics::work_counts();
        let AiDecision { action, .. } = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(action, basic(ActorId(1)));
        let after = crate::diagnostics::work_counts();
        assert_eq!(after.route_searches - before.route_searches, 1);
        assert_eq!(
            after.scenes - before.scenes,
            0,
            "the decision reuses the exact geometry warmed by observation"
        );
    }

    #[test]
    fn hidden_movement_does_not_update_target_memory_and_memory_expires() {
        let mut game = fixture();
        let AiDecision { ai, behavior, .. } = game.choose_ai(ActorId(2)).unwrap();
        let expected = Some(crate::behavior::Engagement {
            reference: crate::perception::ActorReferenceKind::Creature,
            target: ActorId(1),
            location: at(1, 1, 1),
            last_seen: 0,
        });
        assert_eq!(behavior.engagement, expected);
        game.actors.get_mut(&ActorId(2)).unwrap().behavior = behavior;
        game.combat.ai.insert(ActorId(2), ai);
        game.set_region_light(RegionId(2), false).unwrap();
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        let AiDecision {
            behavior: remembered,
            ..
        } = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(remembered.engagement, expected);
        game.tick = 50;
        let AiDecision {
            ai,
            behavior: expired,
            ..
        } = game.choose_ai(ActorId(2)).unwrap();
        assert!(expired.engagement.is_none());
        assert_eq!(ai.state, State::Search);
    }
    #[test]
    fn wounded_ai_flees_and_a_cornered_ai_fights() {
        let mut game = fixture();
        injury(&mut game, ActorId(2), 25);
        let AiDecision { action, ai, .. } = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(ai.state, State::Flee);
        assert!(matches!(action, Action::Move(_)));
        for (x, y) in [(1, 0), (1, 2), (2, 0), (2, 2), (3, 0), (3, 1), (3, 2)] {
            game.set_wall(at(1, x, y), true).unwrap();
        }
        let AiDecision { action, ai, .. } = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(ai.state, State::Flee);
        assert_eq!(action, basic(ActorId(1)));
    }
    #[test]
    fn a_scenario_without_low_health_flight_keeps_wounded_ai_fighting() {
        let mut game = fixture();
        game.scenario_capabilities_mut().low_health_flight = false;
        injury(&mut game, ActorId(2), 25);
        let AiDecision { action, ai, .. } = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(ai.state, State::Attack);
        assert_eq!(action, basic(ActorId(1)));
    }

    fn ring(game: &mut Game, item: u64, defense: i32, concealed: bool) {
        let mut spec = crate::ItemSpec::ordinary(
            format!("item.test.ring-{item}").parse().unwrap(),
            format!("ring-{item}"),
        );
        spec.class = crate::ItemClass::Ring;
        spec.concealed = concealed;
        spec.appearance = "silver ring".into();
        spec.equipment = Some(crate::EquipmentSpec {
            periodic_relocation: None,
            potency_modifiers: Vec::new(),
            weapon_profile: None,
            armor: None,
            slot: crate::EquipmentSlot::new("ring").unwrap(),
            attack: None,
            modifiers: vec![crate::rules::Modifier {
                cancellable: false,
                scope: crate::rules::Scope::Always,
                op: crate::rules::ModifierOp::Add {
                    stat: crate::rules::Stat::Defense {
                        defense: crate::rules::DefenseKind::Physical,
                    },
                    value: i64::from(defense),
                },
            }],
        });
        game.place_item_stack(item, at(1, 2, 1), Some(ActorId(2)), 1, spec)
            .unwrap();
    }

    #[test]
    fn loot_pursues_distant_remembered_routes_but_preserves_safe_first_step() {
        use tor_world::{Extent, Region, World};
        let mut world = World::new(vec![], vec![]).unwrap();
        world
            .add_region(Region {
                id: RegionId(1),
                name: "arena".into(),
                bounds: Extent::new(10, 5, 1).unwrap(),
            })
            .unwrap();
        let mut game = Game::new(world, 42, test_creatures::content());
        let human = game
            .spawn_actor(at(1, 3, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let actor = game
            .spawn_actor(at(1, 1, 2), NonZeroU64::new(100).unwrap())
            .unwrap();
        for (id, faction) in [(human, "hero"), (actor, "foe")] {
            test_creatures::configure(&mut game, id, faction, test_creatures::species());
        }
        game.configure_run(
            human,
            BTreeSet::from([human]),
            None,
            BTreeMap::from([("foe".into(), BTreeSet::from(["hero".into()]))]),
        )
        .unwrap();
        let mut potion =
            crate::ItemSpec::ordinary("item.test.healing".parse().unwrap(), "healing".into());
        potion.class = crate::ItemClass::Potion;
        potion.consumable = Some(crate::ConsumableSpec {
            descriptors: vec![],
            discovery: crate::ItemDiscovery::SelfEffect,
            effects: vec![heal(10)],
        });
        game.place_item_stack(20, at(1, 4, 2), None, 1, potion.clone())
            .unwrap();
        game.place_item_stack(21, at(1, 1, 0), None, 1, potion.clone())
            .unwrap();
        game.refresh_navigation();
        let view = game.observe(actor).unwrap();
        let mut search = game.route_search(actor).unwrap();
        let mut route = |at| search.route(at).ok();
        // The eastward first step is adjacent to the hostile; the northward
        // alternative remains safe and must still be considered.
        assert_eq!(
            game.choose_nearby_loot(actor, &view, &AiProfile::general(), &[], &mut route),
            Some(Action::Move(Direction::North))
        );
        game.teleport(human, at(1, 8, 2)).unwrap();
        game.place_item_stack(22, at(1, 5, 2), None, 1, potion)
            .unwrap();
        game.refresh_navigation();
        let mut view = game.observe(actor).unwrap();
        view.ground_items
            .retain(|item| item.id == crate::ItemId(22));
        let mut search = game.route_search(actor).unwrap();
        assert_eq!(
            game.choose_nearby_loot(actor, &view, &AiProfile::general(), &[], &mut |at| search
                .route(at)
                .ok()),
            Some(Action::Move(Direction::East))
        );
    }

    #[test]
    fn nearby_loot_uses_own_knowledge_and_takes_only_one_unit() {
        let mut game = fixture();
        let actor = ActorId(2);
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        game.set_region_light(RegionId(2), false).unwrap();
        let mut potion =
            crate::ItemSpec::ordinary("item.test.healing".parse().unwrap(), "healing".into());
        potion.class = crate::ItemClass::Potion;
        potion.stackable = true;
        potion.concealed = true;
        potion.appearance = "red potion".into();
        potion.consumable = Some(crate::ConsumableSpec {
            descriptors: vec![],
            discovery: crate::ItemDiscovery::SelfEffect,
            effects: vec![heal(10)],
        });
        game.place_item_stack(20, at(1, 2, 1), None, 3, potion)
            .unwrap();
        game.identify_item(ActorId(1), crate::ItemId(20)).unwrap();
        assert!(!matches!(
            game.choose_ai(actor).unwrap().action,
            Action::Take { .. }
        ));
        game.identify_item(actor, crate::ItemId(20)).unwrap();
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Take {
                item: crate::ItemId(20),
                quantity: Some(1)
            }
        );
    }

    #[test]
    fn nearby_loot_routes_share_one_search_and_do_not_step_beside_hostiles() {
        let mut game = fixture();
        let actor = ActorId(2);
        game.teleport(ActorId(1), at(1, 5, 1)).unwrap();
        for (id, x) in [(20, 3), (21, 4)] {
            let mut potion = crate::ItemSpec::ordinary(
                format!("item.test.healing-{id}").parse().unwrap(),
                format!("healing {id}"),
            );
            potion.class = crate::ItemClass::Potion;
            potion.consumable = Some(crate::ConsumableSpec {
                descriptors: vec![],
                discovery: crate::ItemDiscovery::SelfEffect,
                effects: vec![heal(10)],
            });
            game.place_item_stack(id, at(1, x, 1), None, 1, potion)
                .unwrap();
        }
        game.refresh_navigation();
        let before = crate::diagnostics::work_counts().route_searches;
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Move(Direction::East)
        );
        assert_eq!(crate::diagnostics::work_counts().route_searches - before, 1);
        game.teleport(actor, at(1, 3, 1)).unwrap();
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Move(Direction::East),
            "engagement takes priority over even an underfoot healing reserve"
        );
        let mut unengaged = game.clone();
        unengaged.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        unengaged.set_region_light(RegionId(2), false).unwrap();
        assert_eq!(
            unengaged.choose_ai(actor).unwrap().action,
            Action::Take {
                item: crate::ItemId(20),
                quantity: Some(1)
            }
        );
        game.teleport(ActorId(1), at(1, 4, 1)).unwrap();
        assert_eq!(game.choose_ai(actor).unwrap().action, basic(ActorId(1)));
    }

    #[test]
    fn equipment_uses_duplicate_anatomy_sockets_and_only_the_ais_known_stats() {
        let mut game = fixture_with_anatomy(crate::AnatomySpec {
            capacities: Default::default(),
            primary_attack_socket: crate::equipment_capacity::PrimaryAttackSocket::None,
            slots: vec![crate::EquipmentSlot::new("ring").unwrap(); 2],
        });
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        game.set_region_light(RegionId(2), false).unwrap();
        ring(&mut game, 10, 1, false);
        ring(&mut game, 11, 2, false);
        ring(&mut game, 12, 3, true);
        game.equip_authored(ActorId(2), crate::ItemId(10), crate::EquipmentSlotId(0))
            .unwrap();
        let expected = Action::Equip {
            item: crate::ItemId(11),
            slot: crate::EquipmentSlotId(1),
        };
        assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, expected);
        game.identify_item(ActorId(1), crate::ItemId(12)).unwrap();
        assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, expected);
        game.identify_item(ActorId(2), crate::ItemId(12)).unwrap();
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().action,
            Action::Equip {
                item: crate::ItemId(12),
                slot: crate::EquipmentSlotId(1)
            }
        );
    }

    #[test]
    fn gear_decisions_do_not_read_hidden_curses_and_stop_after_observed_failure() {
        let mut game = fixture_with_anatomy(crate::AnatomySpec {
            capacities: Default::default(),
            primary_attack_socket: crate::equipment_capacity::PrimaryAttackSocket::None,
            slots: vec![crate::EquipmentSlot::new("ring").unwrap()],
        });
        let actor = ActorId(2);
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        game.set_region_light(RegionId(2), false).unwrap();
        ring(&mut game, 10, 1, false);
        ring(&mut game, 11, 2, false);
        game.equip_authored(actor, crate::ItemId(10), crate::EquipmentSlotId(0))
            .unwrap();
        let action = game.choose_ai(actor).unwrap().action;
        assert_eq!(
            action,
            Action::Unequip {
                item: crate::ItemId(10)
            }
        );
        game.items
            .edit(crate::ItemId(10), |item| item.condition.cursed = true)
            .unwrap();
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            action,
            "Hidden curse changes cannot alter the decision"
        );
        assert!(game
            .known_equipment_view(actor, crate::ItemId(10))
            .is_none());
        assert_eq!(
            game.actors[&actor].known_equipment.curse(crate::ItemId(10)),
            None
        );
        game.act(ActorId(1), Action::Wait).unwrap();
        game.act(actor, action).unwrap();
        while game.preparation(actor).is_some() {
            let next = game.next_actor().unwrap();
            game.act(next, Action::Wait).unwrap();
        }
        assert_eq!(
            game.actors[&actor].known_equipment.curse(crate::ItemId(10)),
            Some(true)
        );
        assert_eq!(
            game.equipment(actor)
                .unwrap()
                .get(&crate::EquipmentSlotId(0)),
            Some(&crate::ItemId(10))
        );
        assert!(game
            .known_equipment_view(actor, crate::ItemId(10))
            .is_none());
        assert!(!matches!(
            game.choose_ai(actor).unwrap().action,
            Action::Unequip { .. }
        ));
    }

    #[test]
    fn ordinary_gear_estimates_ignore_hidden_bonuses_without_disclosing_numbers() {
        let slot = crate::EquipmentSlot::new("body_armor").unwrap();
        let mut game = fixture_with_anatomy(crate::AnatomySpec {
            capacities: Default::default(),
            primary_attack_socket: crate::equipment_capacity::PrimaryAttackSocket::None,
            slots: vec![slot.clone()],
        });
        let actor = ActorId(2);
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        game.set_region_light(RegionId(2), false).unwrap();
        for (id, reduction) in [(10, 1), (11, 2)] {
            let mut spec = crate::ItemSpec::ordinary(
                format!("item.test.armor-{id}").parse().unwrap(),
                format!("armor-{id}"),
            );
            spec.class = crate::ItemClass::Armor;
            spec.equipment = Some(crate::EquipmentSpec {
                periodic_relocation: None,
                potency_modifiers: Vec::new(),
                weapon_profile: None,
                slot: slot.clone(),
                armor: Some(crate::ArmorSpec {
                    physical_reduction: reduction,
                    material: crate::ArmorMaterial::Metal,
                    rustproof: false,
                }),
                attack: None,
                modifiers: vec![],
            });
            game.place_item_stack(id, at(1, 2, 1), Some(actor), 1, spec)
                .unwrap();
        }
        game.equip_authored(actor, crate::ItemId(10), crate::EquipmentSlotId(0))
            .unwrap();
        // Authored placement supplies no inspection witness for either instance.
        assert!(game
            .known_equipment_view(actor, crate::ItemId(10))
            .is_none());
        assert!(game
            .known_equipment_view(actor, crate::ItemId(11))
            .is_none());
        let expected = Action::Unequip {
            item: crate::ItemId(10),
        };
        assert_eq!(game.choose_ai(actor).unwrap().action, expected);
        let mut hidden = game.clone();
        hidden
            .items
            .edit(crate::ItemId(10), |item| {
                item.condition.cursed = true;
                item.condition.armor_enchantment = 10;
            })
            .unwrap();
        hidden
            .items
            .edit(crate::ItemId(11), |item| {
                item.condition.armor_penalty = 2;
            })
            .unwrap();
        assert_eq!(hidden.choose_ai(actor).unwrap().action, expected);
        assert!(hidden
            .known_equipment_view(actor, crate::ItemId(10))
            .is_none());
        assert!(hidden
            .known_equipment_view(actor, crate::ItemId(11))
            .is_none());
        // Once observed, actual instance facts replace the ordinary estimate.
        hidden.identify_item(actor, crate::ItemId(11)).unwrap();
        assert!(!matches!(
            hidden.choose_ai(actor).unwrap().action,
            Action::Unequip { .. }
        ));
    }

    #[test]
    fn gear_removal_chooses_an_upgrade_and_does_not_reequip_the_dominated_old_item() {
        let mut game = fixture_with_anatomy(crate::AnatomySpec {
            capacities: Default::default(),
            primary_attack_socket: crate::equipment_capacity::PrimaryAttackSocket::None,
            slots: vec![crate::EquipmentSlot::new("ring").unwrap(); 1],
        });
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        game.set_region_light(RegionId(2), false).unwrap();
        ring(&mut game, 10, 1, false);
        ring(&mut game, 11, 2, false);
        game.equip_authored(ActorId(2), crate::ItemId(10), crate::EquipmentSlotId(0))
            .unwrap();
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().action,
            Action::Unequip {
                item: crate::ItemId(10)
            }
        );
        game.act(ActorId(1), Action::Wait).unwrap();
        game.act(
            ActorId(2),
            Action::Unequip {
                item: crate::ItemId(10),
            },
        )
        .unwrap();
        while game.preparation(ActorId(2)).is_some() {
            let next = game.next_actor().unwrap();
            game.act(next, Action::Wait).unwrap();
        }
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().action,
            Action::Equip {
                item: crate::ItemId(11),
                slot: crate::EquipmentSlotId(0)
            }
        );
    }

    #[test]
    fn gear_work_does_not_replace_unknown_equipment_or_start_beside_a_visible_hostile() {
        let mut game = fixture_with_anatomy(crate::AnatomySpec {
            capacities: Default::default(),
            primary_attack_socket: crate::equipment_capacity::PrimaryAttackSocket::None,
            slots: vec![crate::EquipmentSlot::new("ring").unwrap(); 1],
        });
        ring(&mut game, 10, 1, true);
        ring(&mut game, 11, 2, false);
        game.equip_authored(ActorId(2), crate::ItemId(10), crate::EquipmentSlotId(0))
            .unwrap();
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().action,
            basic(ActorId(1))
        );
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        assert!(!matches!(
            game.choose_ai(ActorId(2)).unwrap().action,
            Action::Unequip { .. } | Action::Equip { .. }
        ));
    }

    #[test]
    fn healing_decisions_require_the_ai_actors_own_identity_knowledge() {
        let mut game = fixture();
        let actor = ActorId(2);
        let mut potion =
            crate::ItemSpec::ordinary("item.test.healing".parse().unwrap(), "healing".into());
        potion.class = crate::ItemClass::Potion;
        potion.concealed = true;
        potion.appearance = "red potion".into();
        potion.consumable = Some(crate::ConsumableSpec {
            descriptors: vec![],
            discovery: crate::ItemDiscovery::SelfEffect,
            effects: vec![heal(10)],
        });
        game.place_item_stack(20, at(1, 2, 1), Some(actor), 1, potion)
            .unwrap();
        injury(&mut game, actor, 15);
        assert_eq!(game.choose_ai(actor).unwrap().action, basic(ActorId(1)));
        game.identify_item(ActorId(1), crate::ItemId(20)).unwrap();
        assert_eq!(game.choose_ai(actor).unwrap().action, basic(ActorId(1)));
        game.identify_item(actor, crate::ItemId(20)).unwrap();
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Drink {
                item: crate::ItemId(20)
            }
        );
    }

    #[test]
    fn healing_priority_does_not_drink_known_mixed_harmful_effects_or_waste_full_health() {
        let mut game = fixture();
        let actor = ActorId(2);
        let mut potion =
            crate::ItemSpec::ordinary("item.test.mixed".parse().unwrap(), "mixed".into());
        potion.class = crate::ItemClass::Potion;
        potion.consumable = Some(crate::ConsumableSpec {
            descriptors: vec![],
            discovery: crate::ItemDiscovery::SelfEffect,
            effects: vec![
                heal(10),
                crate::rules::Effect::Damage {
                    damage: crate::rules::DamageExpr::Components {
                        components: vec![crate::rules::DamageComponentExpr {
                            category: DamageType::Vital,
                            descriptor: None,
                            amount: crate::rules::AmountExpr::Fixed { amount: 1 },
                        }],
                    },
                },
            ],
        });
        game.place_item_stack(20, at(1, 2, 1), Some(actor), 1, potion)
            .unwrap();
        injury(&mut game, actor, 15);
        assert_eq!(game.choose_ai(actor).unwrap().action, basic(ActorId(1)));
        let mut healing =
            crate::ItemSpec::ordinary("item.test.healing".parse().unwrap(), "healing".into());
        healing.class = crate::ItemClass::Potion;
        healing.consumable = Some(crate::ConsumableSpec {
            descriptors: vec![],
            discovery: crate::ItemDiscovery::SelfEffect,
            effects: vec![heal(20)],
        });
        game.place_item_stack(21, at(1, 2, 1), Some(actor), 1, healing)
            .unwrap();
        game.apply_effects(actor, &[heal(30)]).unwrap();
        assert_eq!(game.choose_ai(actor).unwrap().action, basic(ActorId(1)));
        injury(&mut game, actor, 25);
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Drink {
                item: crate::ItemId(21)
            }
        );
    }
    fn data_ability_game(reverse: bool) -> Game {
        use crate::{
            content_ids::{AbilityId, FlagId},
            rules::{Modifier, ModifierOp, RangePreference, Ruleset, Scope},
        };
        let rules = paid_probe_rules();
        let mut document = rules.document().clone();
        let base = rules
            .ability(&AbilityId::new("fixture_mana_ray").unwrap())
            .unwrap();
        let mut foe = test_creatures::species();
        foe.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Flag {
                flag: FlagId::new("magical").unwrap(),
            },
        });
        for name in ["custom_arc", "custom_zap"] {
            let mut definition = base.clone();
            definition.id = AbilityId::new(name).unwrap();
            definition.name = name.into();
            definition.ai.priority = 100;
            definition.ai.range_preference = RangePreference::Any;
            definition.cost = None;
            foe.modifiers.push(Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: definition.id.clone(),
                },
            });
            document.abilities.push(definition);
        }
        for index in 0..8 {
            let mut definition = base.clone();
            definition.id = AbilityId::new(format!("unfunded_{index}")).unwrap();
            definition.name = format!("Unfunded {index}");
            definition.ai.priority = 200;
            definition.ai.range_preference = RangePreference::Any;
            definition.cost = Some(crate::rules::Cost {
                resource: crate::resources::Resource::Mana,
                start: 100,
                resolution: 100,
            });
            foe.modifiers.push(Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: definition.id.clone(),
                },
            });
            document.abilities.push(definition);
        }
        if reverse {
            document.abilities.reverse();
        }
        configured_game(
            test_creatures::species(),
            foe,
            Ruleset::validate(document).unwrap(),
        )
    }

    #[test]
    fn authored_abilities_use_priority_and_stable_ids_with_one_sight_scene_per_decision() {
        for reverse in [false, true] {
            let mut game = data_ability_game(reverse);
            game.teleport(ActorId(1), at(1, 4, 1)).unwrap();
            // Measure one cold construction, independently of fixture reads.
            game.lifecycle.scenes = Default::default();
            let before = crate::diagnostics::work_counts();
            let AiDecision { action, .. } = game.choose_ai(ActorId(2)).unwrap();
            let after = crate::diagnostics::work_counts();
            assert_eq!(
                action,
                Action::UseAbility {
                    id: crate::content_ids::AbilityId::new("custom_arc").unwrap(),
                    invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(1)),
                    supplies: Default::default(),
                }
            );
            assert_eq!(after.scenes - before.scenes, 1);
            assert_eq!(after.route_searches - before.route_searches, 1);
        }
    }

    #[test]
    fn admitted_ai_executes_one_captured_plan_from_one_decision_scene() {
        let mut game = data_ability_game(false);
        game.teleport(ActorId(1), at(1, 4, 1)).unwrap();
        game.act(ActorId(1), Action::Wait).unwrap();
        assert_eq!(game.next_actor(), Some(ActorId(2)));
        // Keep this a cold-build witness; admission itself must not build.
        game.lifecycle.scenes = Default::default();
        let before_admission = crate::diagnostics::work_counts();
        game.admit_ai_intention(ActorId(2)).unwrap();
        let before = crate::diagnostics::work_counts();
        assert_eq!(before.scenes, before_admission.scenes);
        assert_eq!(before.ability_plans, before_admission.ability_plans);
        let execution = game.execute_next_intention().unwrap();
        let after = crate::diagnostics::work_counts();
        assert_eq!(
            execution.action,
            Some(Action::UseAbility {
                id: crate::content_ids::AbilityId::new("custom_arc").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(1)),
                supplies: Default::default(),
            })
        );
        execution.outcome.unwrap();
        assert_eq!(
            after.ability_plans - before.ability_plans,
            1,
            "unfunded candidates are filtered before planning and selected operands are reused"
        );
        assert_eq!(after.route_searches - before.route_searches, 1);
        assert_eq!(after.decision_scenes - before.decision_scenes, 1);
        assert_eq!(
            after.ai_precommit_ordinary_scenes - before.ai_precommit_ordinary_scenes,
            0,
            "no ordinary scene may be built before consuming the chosen plan"
        );
        // Stationary sight/reach and source-origin queries reuse the caster's
        // decision scene. Only the teleported target's participant disclosure
        // builds an ordinary scene, after commitment. Count actual builds,
        // including cold snapshot misses, rather than cached scene requests.
        assert_eq!(after.ordinary_scenes - before.ordinary_scenes, 1);
        assert_eq!(after.scenes - before.scenes, 2);
        assert_eq!(
            after.scenes - before.scenes,
            (after.decision_scenes - before.decision_scenes)
                + (after.ordinary_scenes - before.ordinary_scenes)
        );
        assert!(
            game.disclosed_event_history(ActorId(1))
                .iter()
                .any(|event| {
                    matches!(event, crate::combat::DisclosedCombatEvent::Ability {
                caster: Some(ActorId(2)),
                caster_reference: Some(crate::perception::ActorReferenceKind::Creature),
                target: Some(ActorId(1)),
                id,
                ..
            } if id.as_str() == "custom_arc")
                }),
            "the shared scenes retain the target's identified participant feedback"
        );
    }

    #[test]
    fn tactile_awareness_does_not_count_as_illuminated_sight_for_ranged_abilities() {
        let mut game = data_ability_game(false);
        game.set_region_light(RegionId(1), false).unwrap();
        // Keep the cold-build assertion independent of setup observations.
        game.lifecycle.scenes = Default::default();
        let before = crate::diagnostics::work_counts();
        let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
        assert!(snapshot
            .view
            .visible_actors
            .iter()
            .any(|actor| actor.id == ActorId(1)));
        assert!(!snapshot
            .sight
            .iter()
            .any(|cell| cell.location == at(1, 1, 1)));
        assert_eq!(crate::diagnostics::work_counts().scenes - before.scenes, 1);
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().action,
            basic(ActorId(1))
        );
    }
    /// Exercise the shipped paid W05 definition rather than a colliding clone.
    fn public_plan_ai_rules() -> std::sync::Arc<crate::rules::Ruleset> {
        test_creatures::ruleset()
    }
    #[test]
    fn public_plan_ai_uses_authored_policy_then_spends_readied_on_a_real_attack() {
        use crate::{
            content_ids::AbilityId,
            resources::Resource,
            rules::{Modifier, ModifierOp, Scope, Stat},
        };
        let rules = public_plan_ai_rules();
        assert_eq!(
            rules
                .document()
                .resources
                .iter()
                .find(|definition| definition.resource == Resource::Focus)
                .unwrap()
                .recovery_period,
            300
        );
        let observation = AbilityId::new("read_the_rhythm").unwrap();
        let definition = rules
            .ability(&observation)
            .expect("ordinary authored W05 definition");
        assert_eq!(definition.cost.as_ref().unwrap().resource, Resource::Focus);
        assert_eq!(
            definition.cost.as_ref().unwrap().start + definition.cost.as_ref().unwrap().resolution,
            2
        );
        assert!(definition.ai.hostile);
        assert_eq!(definition.ai.minimum_resource_percent, 100);
        let mut foe = test_creatures::species();
        foe.modifiers.extend([
            Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: observation.clone(),
                },
            },
            Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Set {
                    stat: Stat::ResourceMax {
                        resource: Resource::Focus,
                    },
                    value: 2,
                },
            },
        ]);
        let mut game = configured_game(test_creatures::species(), foe, rules);
        game.act(ActorId(1), Action::Wait).unwrap();
        let decision = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(
            decision.action,
            Action::UseAbility {
                id: observation,
                invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(1)),
                supplies: Default::default()
            }
        );
        let plan = decision.plan.unwrap();
        assert_eq!(
            (plan.preparation(), plan.as_ability().unwrap().recovery),
            (100, 100)
        );
        game.admit_ai_intention(ActorId(2)).unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        assert_eq!(game.tick(), 100);
        let remembered = game.public_plan_observation(ActorId(2)).unwrap().clone();
        assert_eq!(remembered.observed_at, 100);
        assert_eq!(
            game.creature(ActorId(2))
                .unwrap()
                .costs()
                .resources()
                .balance(Resource::Focus),
            0
        );
        assert_eq!(
            game.creature(ActorId(2))
                .unwrap()
                .conditions()
                .instances()
                .filter(|entry| entry.condition.as_str() == "readied")
                .count(),
            1
        );
        for _ in 0..3 {
            if game.next_actor() == Some(ActorId(2)) {
                break;
            }
            game.act(ActorId(1), Action::Wait).unwrap();
        }
        assert_eq!(game.tick(), 200);
        assert_eq!(game.next_actor(), Some(ActorId(2)));
        let attack = game.choose_ai(ActorId(2)).unwrap();
        let plan = attack.plan.as_ref().unwrap().as_ability().unwrap();
        assert!(game
            .content()
            .rules()
            .ability(&plan.ability)
            .unwrap()
            .tags
            .iter()
            .any(|tag| tag.as_str() == "attack"));
        assert_eq!(plan.tokens.len(), 1);
        assert_eq!(
            plan.tokens[0]
                .reservation
                .qualification
                .as_ref()
                .unwrap()
                .opponent,
            ActorId(1)
        );
        game.admit_ai_intention(ActorId(2)).unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        for _ in 0..4 {
            if game.preparation(ActorId(2)).is_none() {
                break;
            }
            game.act(ActorId(1), Action::Wait).unwrap();
        }
        assert!(game.preparation(ActorId(2)).is_none());
        assert!(game
            .creature(ActorId(2))
            .unwrap()
            .conditions()
            .instances()
            .all(|entry| entry.condition.as_str() != "readied"));
        assert_eq!(game.tick(), 300);
        assert_eq!(
            game.creature(ActorId(2))
                .unwrap()
                .costs()
                .resources()
                .balance(Resource::Focus),
            1
        );
        assert_eq!(game.public_plan_observation(ActorId(2)), Some(&remembered));
    }
    #[test]
    fn public_plan_ai_choice_is_equal_for_publicly_equal_idle_and_private_work() {
        use crate::{
            content_ids::AbilityId,
            rules::{Modifier, ModifierOp, Scope},
        };
        let mut foe = test_creatures::species();
        foe.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: AbilityId::new("read_the_rhythm").unwrap(),
            },
        });
        let mut idle = configured_game(test_creatures::species(), foe, public_plan_ai_rules());
        let hero = ActorId(1);
        let observer = ActorId(2);
        let slot = idle
            .actors
            .get(&hero)
            .unwrap()
            .anatomy
            .slots
            .iter()
            .position(|slot| slot.as_str() == "body_armor")
            .unwrap();
        let mut item = crate::ItemSpec::ordinary(
            "item.test.private-armor".parse().unwrap(),
            "private armor".into(),
        );
        item.class = crate::ItemClass::Armor;
        item.equipment = Some(crate::EquipmentSpec {
            periodic_relocation: None,
            potency_modifiers: Vec::new(),
            weapon_profile: None,
            armor: None,
            slot: crate::EquipmentSlot::new("body_armor").unwrap(),
            attack: None,
            modifiers: vec![],
        });
        let item = idle
            .place_item_stack(20, at(1, 1, 1), Some(hero), 1, item)
            .unwrap();
        let mut private = idle.clone();
        idle.act(hero, Action::Wait).unwrap();
        private
            .admit_intention(
                hero,
                Action::Equip {
                    item,
                    slot: crate::EquipmentSlotId(slot as u16),
                },
                crate::IntentionOrigin::Human,
            )
            .unwrap();
        private.execute_next_intention().unwrap().outcome.unwrap();
        assert_eq!(idle.tick(), private.tick());
        assert_eq!(
            idle.observe(observer).unwrap(),
            private.observe(observer).unwrap(),
            "same authorized facts precede the decision"
        );
        let idle_choice = idle.choose_ai(observer).unwrap();
        let private_choice = private.choose_ai(observer).unwrap();
        assert_eq!(idle_choice.action, private_choice.action);
        assert_eq!(idle_choice.plan, private_choice.plan);
        assert!(
            matches!(idle_choice.action, Action::UseAbility { ref id, invocation: crate::abilities::AbilityInvocation { aim: crate::abilities::AbilityAim::ActorTarget { target }, .. }, .. } if id.as_str() == "read_the_rhythm" && target == hero)
        );
    }
    #[test]
    fn declined_erratic_draw_reaches_engagement_and_commits_once_after_restore() {
        use crate::locomotion::{LocomotionValue, MovementChance};
        use crate::rules::{Modifier, ModifierOp, Scope};
        let mut foe = test_creatures::species();
        foe.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Locomotion {
                value: LocomotionValue::Erratic {
                    chance: Some(MovementChance {
                        numerator: 1,
                        denominator: 2,
                    }),
                },
            },
        });
        let mut game = configured_game(test_creatures::species(), foe, paid_probe_rules());
        let actor = ActorId(2);
        game.configure_ai(
            actor,
            AiProfile {
                idle: crate::behavior::IdleBehavior::Wait,
                flee_percent: 0,
                ..Default::default()
            },
        )
        .unwrap();
        game.configure_run(
            ActorId(1),
            BTreeSet::from([ActorId(1)]),
            None,
            BTreeMap::new(),
        )
        .unwrap();
        game.act(ActorId(1), Action::Wait).unwrap();
        let clock = game.actor_clock(actor);
        {
            let behavior = &mut game.actors.get_mut(&actor).unwrap().behavior;
            behavior.rng = 0;
            behavior.engagement = Some(crate::behavior::Engagement {
                reference: crate::perception::ActorReferenceKind::Creature,
                target: ActorId(1),
                location: at(1, 1, 1),
                last_seen: clock,
            });
        }
        let mut expected_rng = 0;
        assert_eq!(
            crate::dice::random_word(&mut expected_rng) % 2,
            1,
            "pinned seed declines erratic movement"
        );
        let original = game.clone();
        let chosen = game.choose_ai(actor).unwrap();
        assert!(matches!(chosen.action, Action::UseAbility { .. }));
        assert!(chosen.plan.is_some());
        assert_eq!(
            chosen.behavior.rng, expected_rng,
            "declining consumes exactly the chance draw, not a direction draw"
        );
        assert_eq!(game.choose_ai(actor).unwrap().behavior.rng, expected_rng);
        assert_eq!(game, original, "preview keeps private RNG unchanged");
        game.admit_ai_intention(actor).unwrap();
        assert_eq!(game.actors[&actor].behavior.rng, 0);
        let mut shared = crate::checkpoint::SharedState::default();
        let checkpoint =
            serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
                .unwrap();
        let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
        let mut restored =
            Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
        for candidate in [&mut game, &mut restored] {
            let execution = candidate.execute_next_intention().unwrap();
            assert_eq!(execution.action, Some(chosen.action.clone()));
            execution.outcome.unwrap();
            assert_eq!(candidate.actors[&actor].behavior.rng, expected_rng);
        }
        assert_eq!(game, restored);
    }

    #[test]
    fn erratic_move_can_replace_a_fresh_adjacent_attack_but_preview_does_not_draw() {
        use crate::locomotion::{LocomotionValue, MovementChance};
        use crate::rules::{Modifier, ModifierOp, Scope};
        let mut foe = test_creatures::species();
        let ordinary = foe.clone();
        foe.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Locomotion {
                value: LocomotionValue::Erratic {
                    chance: Some(MovementChance {
                        numerator: 1,
                        denominator: 1,
                    }),
                },
            },
        });
        let mut game = configured_game(test_creatures::species(), foe, paid_probe_rules());
        game.configure_ai(
            ActorId(2),
            AiProfile {
                idle: crate::behavior::IdleBehavior::Wait,
                flee_percent: 0,
                ..Default::default()
            },
        )
        .unwrap();
        let engagement = Some(crate::behavior::Engagement {
            reference: crate::perception::ActorReferenceKind::Creature,
            target: ActorId(1),
            location: at(1, 1, 1),
            last_seen: game.actor_clock(ActorId(2)),
        });
        game.actors
            .get_mut(&ActorId(2))
            .unwrap()
            .behavior
            .engagement = engagement;
        let before = game.clone();
        let chosen = game.choose_ai(ActorId(2)).unwrap();
        assert!(matches!(chosen.action, Action::Move(_)));
        assert!(chosen.plan.is_none());
        assert_ne!(chosen.behavior.rng, before.actors[&ActorId(2)].behavior.rng);
        assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, chosen.action);
        assert_eq!(game, before);
        // The same fixture without the modifier chooses its ordinary adjacent attack.
        let mut ordinary = configured_game(test_creatures::species(), ordinary, paid_probe_rules());
        ordinary
            .configure_ai(
                ActorId(2),
                AiProfile {
                    idle: crate::behavior::IdleBehavior::Wait,
                    flee_percent: 0,
                    ..Default::default()
                },
            )
            .unwrap();
        ordinary
            .actors
            .get_mut(&ActorId(2))
            .unwrap()
            .behavior
            .engagement = engagement;
        assert!(matches!(
            ordinary.choose_ai(ActorId(2)).unwrap().action,
            Action::UseAbility { .. }
        ));
    }
}

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
