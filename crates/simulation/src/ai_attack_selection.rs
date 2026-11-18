//! Hostile technique selection, separate from encounter priority and movement.
//! Retained work is checked before fresh candidates. Fresh granted abilities keep
//! authored priority/identity order; known finite devices remain the last fallback.
//! This owner proposes requests or frozen plans, never executes or commits state.
use super::{disclosed_flight_aim, disclosed_ray_direction, AiChoice};
use crate::{ActorId, Game};

/// Only the three supported policy combinations are constructible. A reposition
/// probe may consider an OutsideMelee ability while adjacent; it does not move or
/// execute that proposal. Cornered fallback never adds special/device work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AttackSelectionMode {
    Engage,
    CorneredBasic,
    RepositionProbe,
}

impl AttackSelectionMode {
    fn allows_specials(self) -> bool {
        self != Self::CorneredBasic
    }

    fn considers_reposition(self) -> bool {
        self == Self::RepositionProbe
    }
}

/// A mismatch may try fresh work; an unsupported retained ray shape must not.
/// Keeping these separate preserves the original selector's early-return contract.
enum RetainedAttackProbe<'a> {
    ContinueFresh,
    Frozen(&'a crate::abilities::Plan),
    StopSelection,
}

impl Game {
    /// Default combat policy uses personal capabilities and resources, never
    /// enemy defenses, immunity or condition state. Choices reserve nothing;
    /// ordinary execution rechecks funding and reach before preparation.
    pub(super) fn choose_ai_ability(
        &self,
        actor: ActorId,
        target: ActorId,
        sight: &[tor_world::SightCell],
        view: &crate::Observation,
        mode: AttackSelectionMode,
    ) -> Option<AiChoice> {
        use crate::rules::RangePreference;
        let creature = self.creature(actor)?;
        let costs = creature.costs();
        let rules = creature.build().ruleset();
        match self.probe_retained_attack(actor, target, sight, view, rules, mode) {
            RetainedAttackProbe::Frozen(plan) => return Some(plan.clone().into()),
            RetainedAttackProbe::StopSelection => return None,
            RetainedAttackProbe::ContinueFresh => {}
        }
        let in_melee = self.attack_available_in_scene(actor, target, sight);
        let mut candidates: Vec<_> = creature
            .derived()
            .abilities
            .iter()
            .filter_map(|id| {
                let definition = rules.ability(id)?;
                (definition.ai.hostile && (mode.allows_specials() || definition.basic_attack))
                    .then_some((
                        definition.ai.priority,
                        id,
                        &definition.ai,
                        definition.cost.as_ref(),
                    ))
            })
            .collect();
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        for (_, id, hints, cost) in candidates {
            if hints.range_preference == RangePreference::OutsideMelee
                && in_melee
                && !mode.considers_reposition()
            {
                continue;
            }
            if let Some(cost) = cost {
                let available = costs.available(cost.resource);
                let maximum = costs.resources().maximum(cost.resource);
                if !cost
                    .start
                    .checked_add(cost.resolution)
                    .is_some_and(|total| available >= total)
                    || u64::from(available) * 100
                        < u64::from(maximum) * u64::from(hints.minimum_resource_percent)
                {
                    continue;
                }
            }
            let definition = rules.ability(id)?;
            if definition.invocation == crate::abilities::InvocationKind::DirectionalProjectile {
                if let Some(action) =
                    self.choose_ai_projectile_request(actor, target, sight, view, id)
                {
                    return Some(AiChoice::Request(action));
                }
                continue;
            }
            let invocation = if definition.invocation.is_directional_ray() {
                let crate::rules::Reach::Sight { range } = definition.reach else {
                    continue;
                };
                let Some(direction) =
                    self.ai_disclosed_flight_aim(actor, target, sight, view, range)
                else {
                    continue;
                };
                direction.ray_invocation()
            } else {
                crate::abilities::AbilityInvocation::actor_target(target)
            };
            let Ok(plan) = self.ability_invocation_plan_in_scene(actor, id, invocation, sight)
            else {
                continue;
            };
            if let Some(plan) = self.physical_ai_plan(plan, view) {
                return Some(plan.into());
            }
        }
        if mode.allows_specials() {
            return self
                .choose_ai_known_device(actor, target, sight, view)
                .map(Into::into);
        }
        None
    }

    /// Match frozen work with the legacy disclosed-bearing predicates, never
    /// fresh obstacle/ally alternate selection. This does not recapture a plan.
    fn probe_retained_attack(
        &self,
        actor: ActorId,
        target: ActorId,
        sight: &[tor_world::SightCell],
        view: &crate::Observation,
        rules: &crate::rules::Ruleset,
        mode: AttackSelectionMode,
    ) -> RetainedAttackProbe<'_> {
        if let Some(preparation) = self
            .preparation(actor)
            .filter(|preparation| !preparation.active)
        {
            if let crate::Work::UseAbility { id, invocation, .. } = &preparation.plan.work() {
                if let Some(definition) = rules.ability(id).filter(|definition| {
                    definition.ai.hostile && (mode.allows_specials() || definition.basic_attack)
                }) {
                    let selected = match &invocation.aim {
                        crate::abilities::AbilityAim::ActorTarget {
                            target: prepared_target,
                        } => *prepared_target == target,
                        crate::abilities::AbilityAim::DirectionalRay { direction } => {
                            let crate::rules::Reach::Sight { range } = definition.reach else {
                                return RetainedAttackProbe::StopSelection;
                            };
                            let Some(owner) = self.actors.get(&actor) else {
                                return RetainedAttackProbe::StopSelection;
                            };
                            disclosed_ray_direction(target, sight, view, owner.body.eye, range)
                                == Some(*direction)
                        }
                        crate::abilities::AbilityAim::DirectionalProjectile {
                            direction, ..
                        } => preparation.plan.as_projectile().is_some_and(|plan| {
                            u16::try_from(
                                plan.range.long_feet / crate::weapon_profiles::PROJECTILE_STEP_FEET,
                            )
                            .ok()
                            .and_then(|range| {
                                disclosed_ray_direction(
                                    target,
                                    sight,
                                    view,
                                    self.actors.get(&actor)?.body.eye,
                                    range,
                                )
                            }) == Some(*direction)
                        }),
                        crate::abilities::AbilityAim::CellRay { offset } => {
                            let crate::rules::Reach::Sight { range } = definition.reach else {
                                return RetainedAttackProbe::StopSelection;
                            };
                            let Some(owner) = self.actors.get(&actor) else {
                                return RetainedAttackProbe::StopSelection;
                            };
                            disclosed_flight_aim(target, sight, view, owner.body.eye, range)
                                == Some(crate::abilities::FlightAim::Cell { offset: *offset })
                        }
                        crate::abilities::AbilityAim::CellProjectile { offset, .. } => {
                            preparation.plan.as_projectile().is_some_and(|plan| {
                                u16::try_from(
                                    plan.range.long_feet
                                        / crate::weapon_profiles::PROJECTILE_STEP_FEET,
                                )
                                .ok()
                                .and_then(|range| {
                                    disclosed_flight_aim(
                                        target,
                                        sight,
                                        view,
                                        self.actors.get(&actor)?.body.eye,
                                        range,
                                    )
                                }) == Some(crate::abilities::FlightAim::Cell { offset: *offset })
                            })
                        }
                        _ => false,
                    };
                    if selected && self.preparation_still_available(preparation, Some(sight)) {
                        return RetainedAttackProbe::Frozen(&preparation.plan);
                    }
                }
            }
        }
        RetainedAttackProbe::ContinueFresh
    }

    /// A device is a carried capability, not a granted build ability. Restrict
    /// this conservative fallback to explicitly known positive instance charges.
    /// Unknown balances cannot become a free AI-only empty-device oracle.
    fn choose_ai_known_device(
        &self,
        actor: ActorId,
        target: ActorId,
        sight: &[tor_world::SightCell],
        view: &crate::Observation,
    ) -> Option<crate::abilities::Plan> {
        let rows = &view.interactions.as_ref()?.inventory;
        let rules = self.content.rules();
        let mut candidates = Vec::new();
        for disclosed in &view.inventory {
            if !disclosed.identified
                || !rows.iter().any(|row| {
                    row.item == disclosed.id
                        && row.zappable
                        && row.charges.is_some_and(|count| count > 0)
                })
            {
                continue;
            }
            let device = self.items.get(&disclosed.id)?.spec.device.as_ref()?;
            let definition = rules.ability(&device.ability)?;
            if !definition.ai.hostile {
                continue;
            }
            let range = match definition.reach {
                crate::rules::Reach::Sight { range } => Some(range),
                crate::rules::Reach::UntilObstruction => None,
                _ => continue,
            };
            let Some(direction) =
                self.ai_disclosed_flight_aim_with_range(actor, target, sight, view, range)
            else {
                continue;
            };
            candidates.push((definition.ai.priority, disclosed.id, direction));
        }
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        for (_, item, direction) in candidates {
            let work = crate::Work::Zap {
                item,
                aim: direction.device(),
            };
            if let Some(pending) = self.preparation(actor).filter(|pending| {
                !pending.active
                    && pending.plan.work() == work
                    && self.preparation_still_available(pending, Some(sight))
            }) {
                return Some(pending.plan.clone());
            }
            if let Ok(plan) = self.plan_work(actor, work) {
                return self.physical_ai_plan(plan, view);
            }
        }
        None
    }
}

#[cfg(test)]
#[path = "ai_attack_selection_tests.rs"]
mod tests;
