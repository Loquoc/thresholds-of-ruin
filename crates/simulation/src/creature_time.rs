//! Relative creature timers follow active simulation ticks, never wall time or
//! the elapsed global time while an actor is frozen or detached.
use crate::{ActorId, Game, GameError};

#[cfg(test)]
thread_local! {
    static TIMER_SETTLEMENTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn count_settlement() {
    TIMER_SETTLEMENTS.with(|count| count.set(count.get() + 1));
}

impl Game {
    pub(crate) fn actor_input_eligibility(
        &self,
        actor: ActorId,
    ) -> crate::combat::InputEligibility {
        if !self.actors.contains_key(&actor) {
            crate::combat::InputEligibility::Pending
        } else if self.condition_allows_voluntary_input(actor) {
            crate::combat::InputEligibility::Permitted
        } else {
            crate::combat::InputEligibility::Denied
        }
    }
    pub(crate) fn condition_allows_voluntary_input(&self, id: ActorId) -> bool {
        !self.condition_restricts(id, crate::rules::ConditionRestriction::VoluntaryActions)
    }
    pub(crate) fn condition_blocks_action(&self, id: ActorId, action: &crate::Action) -> bool {
        use crate::rules::ConditionRestriction;
        if self.locomotion_blocks_action(id, action) {
            return true;
        }
        [
            ConditionRestriction::VoluntaryActions,
            ConditionRestriction::OrdinaryMovement,
            ConditionRestriction::MovementAbilities,
        ]
        .into_iter()
        .any(|operation| operation.blocks(action) && self.condition_restricts(id, operation))
    }
    /// Read current operation eligibility without settling or changing clocks.
    pub(crate) fn condition_restricts(
        &self,
        id: ActorId,
        operation: crate::rules::ConditionRestriction,
    ) -> bool {
        self.actors.get(&id).is_some_and(|actor| {
            (operation == crate::rules::ConditionRestriction::OrdinaryMovement
                && actor.holds.links().next().is_some())
                || actor.combat.as_ref().is_some_and(|combat| {
                    combat.creature().condition_restricts_after(
                        operation,
                        self.actor_clock(id).saturating_sub(actor.active_since),
                    )
                })
        })
    }
    /// Absolute deadlines are updated only when their actor changes. Frozen
    /// actors keep their remaining interval and cannot wake the simulation.
    pub fn next_creature_event_tick(&self) -> Option<u64> {
        self.actors
            .scheduled_timers()
            .next()
            .map(|(deadline, _)| deadline)
            .into_iter()
            .chain(self.next_wandering_event_tick())
            .min()
    }

    /// Settle only expired deadlines. Fractional progress on all other actors
    /// remains lazy until they are touched or projected for an observation.
    pub(crate) fn settle_creature_time(&mut self) -> Result<(), GameError> {
        let due: Vec<_> = self
            .actors
            .scheduled_timers()
            .take_while(|(deadline, _)| *deadline <= self.tick)
            .map(|(_, id)| id)
            .collect();
        for id in due {
            self.settle_actor_time(id)?;
        }
        self.reconcile_recovery_contexts()?;
        self.reconcile_passive_discovery()?;
        self.settle_periodic_searches()?;
        self.settle_periodic_relocations()?;
        self.settle_aggravation()?;
        self.reconcile_awareness()?;
        self.reconcile_control_contacts()?;
        self.settle_wandering_population()
    }

    pub(crate) fn settle_actor_time(&mut self, id: ActorId) -> Result<(), GameError> {
        let clock = self.actor_clock(id);
        if self
            .actors
            .get(&id)
            .is_none_or(|actor| clock <= actor.active_since)
        {
            return Ok(());
        }
        let outcome = self.settled_actor_outcome(id)?;
        let trigger = &self.content.rules().document().nutrition_condition_trigger;
        let eligible = trigger.enabled()
            && outcome.nutrition_empty_depletion
            && outcome.state.alive()
            && outcome.state.combat.as_ref().is_some_and(|combat| {
                let creature = combat.creature();
                !creature.condition_restricts(crate::rules::ConditionRestriction::VoluntaryActions)
            });
        if !eligible {
            self.apply_outcome(outcome)?;
            self.settle_periodic_search_for(id)?;
            self.settle_periodic_relocations_for(id)?;
            return self.settle_aggravation_for(id);
        }
        let mut rng = self.rules_rng;
        let effect = trigger.sample(&mut rng);
        // The time settlement, random stream, ordinary condition resolution,
        // paid-work pause and events share one candidate publication boundary.
        let mut candidate = self.clone();
        candidate.apply_outcome(outcome)?;
        // A scan earned under the preceding active policy belongs before a
        // condition introduced at this same deadline. Publish the sampled RNG
        // first, then carry the scan's draws into ordinary effect resolution.
        candidate.rules_rng = rng;
        candidate.settle_periodic_search_for(id)?;
        candidate.settle_periodic_relocations_for(id)?;
        candidate.settle_aggravation_for(id)?;
        let mut rng = candidate.rules_rng;
        if let Some(effect) = effect {
            let crate::abilities::PlannedEffect::ApplyCondition {
                id: ref condition, ..
            } = effect
            else {
                unreachable!("nutrition triggers only apply a condition");
            };
            let context = candidate.condition_recovery_context(id, condition)?;
            let creature = candidate.actors[&id]
                .combat
                .as_ref()
                .expect("enrolled actor")
                .creature();
            let snapshot = crate::abilities::Snapshot::capture(creature).with_context(
                crate::abilities::ResolutionContext {
                    intangible: false,
                    target_recovery_context: context,
                    ..Default::default()
                },
            );
            let effects = [effect];
            let resolved = crate::abilities::resolve_checked(
                crate::abilities::ResolutionPlan::Effects {
                    actor: id,
                    target: id,
                    effects: &effects,
                },
                &snapshot,
                &mut rng,
                &mut (),
            )
            .map_err(GameError::from)?;
            candidate.apply_outcome(resolved)?;
        }
        candidate.rules_rng = rng;
        *self = candidate;
        Ok(())
    }

    /// Authored construction explicitly enrolls a creature. It cannot reset an
    /// existing required reserve or quietly enroll every actor in the ruleset.
    pub fn configure_nutrition(
        &mut self,
        id: ActorId,
        requirement: crate::nutrition::NutritionRequirement,
    ) -> Result<(), GameError> {
        let outcome = self.prepare_nutrition_configuration(id, requirement)?;
        let prepared = self.prepare_actor_outcome(outcome)?;
        self.publish_actor_effect(prepared, false)
    }

    /// Birth construction is already inside its enclosing publication candidate.
    pub(crate) fn configure_nutrition_in_transaction(
        &mut self,
        id: ActorId,
        requirement: crate::nutrition::NutritionRequirement,
    ) -> Result<(), GameError> {
        let outcome = self.prepare_nutrition_configuration(id, requirement)?;
        self.apply_outcome(outcome)
    }

    fn prepare_nutrition_configuration(
        &self,
        id: ActorId,
        requirement: crate::nutrition::NutritionRequirement,
    ) -> Result<crate::lifecycle::ActorOutcome, GameError> {
        let mut outcome = self.settled_actor_outcome(id)?;
        let combat = outcome.state.combat.as_mut().ok_or(GameError::Rule(
            crate::abilities::RuleError::InvalidDefinition,
        ))?;
        combat
            .configure_nutrition(requirement)
            .map_err(GameError::Creature)?;
        if let Some(interval) = combat.creature().next_change_in() {
            self.actor_clock(id)
                .checked_add(interval)
                .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
        }
        Ok(outcome)
    }

    /// Settle earned active work under the old nutrition policy, then rephase
    /// its fraction. Neither stopping nor resuming gives catch-up healing.
    pub fn set_recovery_nutrition(
        &mut self,
        id: ActorId,
        nutrition: crate::health_recovery::RecoveryNutrition,
    ) -> Result<(), GameError> {
        let mut outcome = self.settled_actor_outcome(id)?;
        let combat = outcome.state.combat.as_mut().ok_or(GameError::Rule(
            crate::abilities::RuleError::InvalidDefinition,
        ))?;
        combat
            .set_recovery_nutrition(nutrition)
            .map_err(|_| GameError::Rule(crate::abilities::RuleError::InvalidDefinition))?;
        let prepared = self.prepare_actor_outcome(outcome)?;
        self.publish_actor_effect(prepared, false)
    }

    pub fn restore_resource(
        &mut self,
        id: ActorId,
        resource: crate::resources::Resource,
        amount: u32,
    ) -> Result<u32, GameError> {
        let mut outcome = self.settled_actor_outcome(id)?;
        if !outcome.state.alive() {
            return Err(GameError::UnknownActor);
        }
        let combat = outcome.state.combat.as_mut().ok_or(GameError::Rule(
            crate::abilities::RuleError::InvalidDefinition,
        ))?;
        let restored = combat.restore_resource(resource, amount);
        let prepared = self.prepare_actor_outcome(outcome)?;
        self.publish_actor_effect(prepared, false)?;
        Ok(restored)
    }

    /// Cure the named condition through the same active-time and recovery
    /// boundary as application. Production removal preserves protection and
    /// the reservations already owned by paid intentions.
    pub fn remove_condition(
        &mut self,
        actor: ActorId,
        condition: &crate::content_ids::ConditionId,
    ) -> Result<bool, GameError> {
        let mut outcome = self.settled_actor_outcome(actor)?;
        if !outcome.state.alive() {
            return Err(GameError::UnknownActor);
        }
        let combat = outcome.state.combat.as_mut().ok_or(GameError::Rule(
            crate::abilities::RuleError::InvalidDefinition,
        ))?;
        let context = self.condition_recovery_context(actor, condition)?;
        let changed = combat
            .with_recovery_context(context, |creature| creature.remove_condition(condition))
            .map_err(|_| GameError::Rule(crate::abilities::RuleError::InvalidDefinition))?;
        let prepared = self.prepare_actor_outcome(outcome)?;
        self.publish_actor_effect(prepared, false)?;
        Ok(changed)
    }

    /// Trusted effect application. A causer can be dead or detached; an existing
    /// condition needs neither current reach nor sight of its causer.
    pub fn apply_condition(
        &mut self,
        actor: ActorId,
        condition: &crate::content_ids::ConditionId,
        source: ActorId,
        duration: u64,
    ) -> Result<crate::conditions::ConditionUpdate, GameError> {
        self.apply_condition_lifetime(actor, condition, source, Some(duration))
    }
    pub fn apply_persistent_condition(
        &mut self,
        actor: ActorId,
        condition: &crate::content_ids::ConditionId,
        source: ActorId,
    ) -> Result<crate::conditions::ConditionUpdate, GameError> {
        self.apply_condition_lifetime(actor, condition, source, None)
    }
    fn apply_condition_lifetime(
        &mut self,
        actor: ActorId,
        condition: &crate::content_ids::ConditionId,
        source: ActorId,
        duration: Option<u64>,
    ) -> Result<crate::conditions::ConditionUpdate, GameError> {
        if !self.actors.contains_key(&source) && !self.detached_actor(source) {
            return Err(GameError::UnknownActor);
        }
        let mut outcome = self.settled_actor_outcome(actor)?;
        if !outcome.state.alive() {
            return Err(GameError::UnknownActor);
        }
        let combat = outcome.state.combat.as_mut().ok_or(GameError::Rule(
            crate::abilities::RuleError::InvalidDefinition,
        ))?;
        let context = self.condition_recovery_context(actor, condition)?;
        let update = match duration {
            Some(duration) => combat.apply_condition(condition, source, duration, context)?,
            None => combat.apply_persistent_condition(condition, source, context)?,
        };
        let prepared = self.prepare_actor_outcome(outcome)?;
        self.publish_actor_effect(prepared, false)?;
        Ok(update)
    }
}

#[cfg(test)]
#[path = "creature_time_tests.rs"]
mod tests;
