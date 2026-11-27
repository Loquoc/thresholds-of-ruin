//! Actor-local lifecycle outcomes and authoritative state application.
//!
//! Planners edit owned actor snapshots. This boundary alone publishes their
//! creature state, reconciles preparations, performs death cleanup, and emits
//! combat events. It runs inside the caller's existing Game transaction.
use crate::abilities::{Outcome, ResolvedEffect, RuleError};
use crate::combat::CombatEvent;
use crate::{Actor, ActorId, Game, GameError, Item, ItemId, ItemLocation, ItemSpec, Work};

#[cfg(test)]
thread_local! {
    static TOKEN_PLAN_COMPARISONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Who caused a resolved batch, for damage activity and fatal attribution.
struct EffectAttribution<'a> {
    activity: &'a mut DamageActivityGuard,
    origin: Option<&'a crate::earned_growth::FatalOrigin>,
    strike: Option<u16>,
    disclosure_origin: &'a crate::event_history::EventOrigin,
}

/// A recipient's staged state after applying one resolved batch.
struct EffectApplication {
    transition: ActorOutcome,
    placement: crate::abilities::Placement,
    /// A forced translation moved the recipient.
    forced: bool,
    /// Some effect caused actual damage or health loss.
    damaged: bool,
    created_hold: Option<crate::linked_hold::HoldId>,
}

/// Transient once-per-publication damage activity, including area recipients.
/// Entries are actual owned actors, never retained in a frozen or saved plan.
#[derive(Default)]
pub(crate) struct DamageActivityGuard {
    actors: [Option<ActorId>; 2],
    overflow: std::collections::BTreeSet<ActorId>,
}
impl DamageActivityGuard {
    fn first(&mut self, actor: ActorId) -> bool {
        if self.actors.contains(&Some(actor)) {
            return false;
        }
        if let Some(slot) = self.actors.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(actor);
            return true;
        }
        self.overflow.insert(actor)
    }
}

/// A receipt is valid only within the caller's existing strike transaction. Its
/// fields are private so only successful lifecycle publication can create it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PublishedResolution {
    chart_completion: Option<crate::detection_lifecycle::CompletionReceipt>,
    created_hold: Option<crate::linked_hold::HoldId>,
    event: usize,
    role: PublishedRole,
}
impl PublishedResolution {
    pub(crate) fn chart_completion(self) -> Option<crate::detection_lifecycle::CompletionReceipt> {
        self.chart_completion
    }
    pub(crate) fn created_hold(self) -> Option<crate::linked_hold::HoldId> {
        self.created_hold
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PublishedRole {
    Standalone,
    Primary {
        strike: u16,
    },
    Rider {
        primary: usize,
        strike: u16,
        ordinal: u16,
    },
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum ResolutionPublication {
    Standalone,
    Primary {
        strike: u16,
    },
    Rider {
        primary: PublishedResolution,
        strike: u16,
        ordinal: u16,
    },
}

/// Static lifecycle dispatch without a large temporary wrapper enum or heap
/// allocation. Only lifecycle-owned outcome implementations publish state.
pub(crate) trait ApplicationOutcome {
    fn apply(self, game: &mut Game) -> Result<(), GameError>;
}
impl ApplicationOutcome for Outcome {
    fn apply(self, game: &mut Game) -> Result<(), GameError> {
        game.apply_resolved_outcome(
            self,
            ResolutionPublication::Standalone,
            &mut DamageActivityGuard::default(),
        )?;
        Ok(())
    }
}
impl ApplicationOutcome for ActorOutcome {
    fn apply(self, game: &mut Game) -> Result<(), GameError> {
        game.commit_actor_outcome(self)
    }
}
impl ApplicationOutcome for ItemOutcome {
    fn apply(self, game: &mut Game) -> Result<(), GameError> {
        game.apply_item_outcome(self)
    }
}

pub(crate) struct ActorOutcome {
    pub id: ActorId,
    pub state: Actor,
    pub(crate) entry_cause: crate::floor_activation::EntryCause,
    pub(crate) spent_items: Option<crate::supplies::PreparationLineage>,
    pub(crate) nutrition_empty_depletion: bool,
    /// Active time exhausted the reserve and killed the actor.
    starved: bool,
    /// Item-owned changes staged with an atomic body/equipment candidate.
    pub(crate) extinguish_lights: std::collections::BTreeSet<ItemId>,
    pub(crate) survival: crate::survival_diagnostics::SurvivalFlows,
    events: Vec<CombatEvent>,
    disclosure_origin: Option<crate::event_history::EventOrigin>,
    input: InputBoundary,
    fatal_cause: Option<crate::earned_growth::FatalCause>,
}
#[derive(Default)]
enum InputBoundary {
    #[default]
    Preserve,
    Human,
    Require,
}
pub(crate) enum Interruption {
    Suspend,
    Discard,
}
impl ActorOutcome {
    pub(crate) fn interrupt(&mut self, mode: Interruption, clock: u64) -> Result<(), GameError> {
        let Some(preparation) = self.state.pending.as_ref() else {
            return Ok(());
        };
        let mode = if matches!(preparation.plan, crate::abilities::Plan::Respec(_)) {
            Interruption::Discard
        } else {
            mode
        };
        if matches!(mode, Interruption::Suspend) && !preparation.active {
            return Ok(());
        }
        let intention = preparation.intention;
        match mode {
            Interruption::Suspend => {
                self.pause(clock);
            }
            Interruption::Discard => {
                self.discard(clock)?;
            }
        }
        if !self.events.iter().any(|event| matches!(event,
            CombatEvent::Interrupted { actor, intention: prior } if *actor == self.id && *prior == intention)) {
            self.events.push(CombatEvent::Interrupted { actor: self.id, intention });
        }
        self.input = InputBoundary::Human;
        Ok(())
    }
    /// Abandon only unresolved work. A resolved prefix has an irrevocable tail.
    pub(crate) fn discard(&mut self, clock: u64) -> Result<bool, GameError> {
        let Some(preparation) = self.state.pending.as_ref() else {
            return Ok(false);
        };
        let debt = preparation.abandon_delay(clock)?;
        let ready_at = if preparation.has_prefix() {
            clock
                .checked_add(debt)
                .and_then(|ready| {
                    // Ordinary waiting while suspended must not erase either debt.
                    ready.checked_add(if preparation.active {
                        0
                    } else {
                        self.state.ready_at.saturating_sub(clock)
                    })
                })
                .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?
        } else if preparation.active {
            clock
        } else {
            self.state.ready_at
        };
        let ended =
            preparation
                .plan
                .ability_id()
                .map(|_| crate::combat::CombatEvent::SequenceEnded {
                    actor: self.id,
                    intention: preparation.intention,
                    reason: crate::combat::SequenceEnd::Abandoned,
                });
        self.state.cancel_preparation();
        self.state.ready_at = ready_at;
        if let Some(event) = ended {
            self.events.push(event);
        }
        self.input = InputBoundary::Human;
        Ok(true)
    }
    pub(crate) fn pause(&mut self, clock: u64) -> Option<Work> {
        let preparation = self.state.pending.as_mut().filter(|p| p.active)?;
        preparation.remaining = preparation
            .remaining
            .saturating_sub(clock.saturating_sub(preparation.started));
        preparation.active = false;
        self.state.ready_at = clock;
        self.input = InputBoundary::Require;
        Some(preparation.plan.work())
    }
}

pub(crate) struct ItemOutcome {
    pub actor: ActorId,
    pub work: Work,
    pub intention: Option<crate::IntentionId>,
    pub change: ItemChange,
}
pub(crate) enum ItemChange {
    PortableLight {
        item: crate::ItemId,
        lit: bool,
        actor: ActorOutcome,
    },
    CurseBlocked(ActorOutcome),
    Equipment(ActorOutcome),
    Consumed {
        actor: ActorOutcome,
    },
}

/// Valid only inside the caller's uninterrupted transaction or discarded preview.
/// Fallible actor/death preparation is finished; publication never repeats it.
/// This is not Clone/Serialize and cannot become persisted or client authority.
pub(crate) struct PreparedActorOutcome {
    outcome: ActorOutcome,
    death: Option<DeathOutcome>,
    rebuild: crate::creatures::RebuildOutcome,
}
impl PreparedActorOutcome {
    /// Preparation can add interruptions or death cleanup after a caller has
    /// finished changing its owned actor. Only this finalized value can prove
    /// that publication will not append lifecycle events.
    pub(crate) fn emits_events(&self) -> bool {
        !self.outcome.events.is_empty() || self.death.is_some()
    }

    pub(crate) fn rebuild_outcome(&self) -> &crate::creatures::RebuildOutcome {
        &self.rebuild
    }
    pub(crate) fn creature(&self) -> Option<&crate::creatures::CreatureState> {
        self.outcome
            .state
            .combat
            .as_ref()
            .map(|combat| combat.creature())
    }
}

struct DeathOutcome {
    corpse: ItemId,
    next_item_id: u64,
    body: Item,
    inventory: Vec<ItemId>,
    award: Option<DeathAward>,
}
struct DeathAward {
    owner: ActorId,
    amount: u32,
}

impl Game {
    pub(crate) fn actor_outcome(&self, id: ActorId) -> Result<ActorOutcome, GameError> {
        let state = self.actors.get(&id).ok_or(GameError::UnknownActor)?.clone();
        Ok(ActorOutcome {
            id,
            state,
            spent_items: None,
            nutrition_empty_depletion: false,
            starved: false,
            extinguish_lights: Default::default(),
            survival: Default::default(),
            events: vec![],
            disclosure_origin: None,
            input: InputBoundary::Preserve,
            fatal_cause: None,
            entry_cause: crate::floor_activation::EntryCause::Movement,
        })
    }

    /// Materialize an actor-local candidate without committing on a later error.
    pub(crate) fn settled_actor_outcome(&self, id: ActorId) -> Result<ActorOutcome, GameError> {
        let outcome = self.actor_outcome_at_clock(id, self.survival_diagnostics.is_some())?;
        #[cfg(test)]
        if outcome.state.active_since != self.actors[&id].active_since {
            crate::creature_time::count_settlement();
        }
        Ok(outcome)
    }

    /// Inspection shares exact boundary projection without materializing an
    /// authoritative candidate or affecting timer-settlement diagnostics.
    pub(crate) fn projected_actor_outcome(&self, id: ActorId) -> Result<ActorOutcome, GameError> {
        self.actor_outcome_at_clock(id, false)
    }

    fn actor_outcome_at_clock(
        &self,
        id: ActorId,
        collect: bool,
    ) -> Result<ActorOutcome, GameError> {
        let mut outcome = self.actor_outcome(id)?;
        let state = &mut outcome.state;
        let clock = self.actor_clock(id);
        let elapsed = clock.saturating_sub(state.active_since);
        if elapsed > 0 {
            // Only the old generation owns this elapsed interval. Conditions
            // advance below; final candidate reconciliation happens afterwards.
            if let Some((_, remaining)) = crate::hallucination_clock::hallucination_instance(state)
            {
                crate::hallucination_clock::settle_before_expiry(
                    &mut state.hallucination,
                    remaining,
                    elapsed,
                )?;
            } else {
                state.hallucination = None;
            }
            if state.alive() {
                state.periodic_relocations.advance(elapsed)?;
            }
            if state.awareness.next_in(state).is_some() {
                let denominator = state
                    .perception()
                    .aggravation
                    .expect("active aggravation")
                    .chance_denominator;
                state.awareness.advance(elapsed, denominator)?;
            }
            if state.search_clock.next_in(state).is_some() {
                let creature = state
                    .combat
                    .as_ref()
                    .expect("search source creature")
                    .creature();
                let check = crate::discovery::FrozenDiscoveryCheck::capture_for(creature, id)?;
                state.search_clock.advance(elapsed, check)?;
            }
            if let Some(combat) = state.combat.as_mut() {
                let denied = combat
                    .creature()
                    .condition_restricts(crate::rules::ConditionRestriction::VoluntaryActions);
                let expires = state
                    .behavior
                    .engagement
                    .and_then(|entry| {
                        entry
                            .last_seen
                            .checked_add(combat.creature().derived().behavior.memory_ticks)
                    })
                    .filter(|deadline| *deadline <= clock);
                let prefix = expires.map_or(elapsed, |deadline| {
                    deadline.saturating_sub(state.active_since).min(elapsed)
                });
                let endpoint_context = if expires.is_some()
                    && combat.creature().derived().contextual_health_recovery()
                {
                    Some(self.senses_faction_hostile(id)?)
                } else {
                    None
                };
                let settled = combat.advance_active_with_endpoint_context(prefix, endpoint_context);
                outcome.nutrition_empty_depletion = settled.empty_time_depletion;
                outcome.starved |= settled.starved;
                if collect {
                    outcome.survival.active(&settled);
                }
                if expires.is_some() {
                    state.behavior.engagement = None;
                    // Only the final positive interval licenses a depletion
                    // trigger. A zero-length suffix preserves the prefix event.
                    if elapsed > prefix {
                        let settled = combat.advance_active(elapsed - prefix);
                        outcome.nutrition_empty_depletion = settled.empty_time_depletion;
                        outcome.starved |= settled.starved;
                        if collect {
                            outcome.survival.active(&settled);
                        }
                    }
                }
                if denied && state.pending.as_ref().is_none_or(|pending| !pending.active) {
                    // Input-denied actors can outlive their ordinary readiness.
                    // Normalize only elapsed readiness, never future recovery or
                    // the paused work's frozen remaining commitment.
                    state.ready_at = state.ready_at.max(clock);
                }
            }
            state.holds.advance(elapsed);
            state.active_since = clock;
        }
        Ok(outcome)
    }

    /// External effect application owns one transaction; internal lifecycle callers
    /// use the already existing action/physics transaction and actor-local snapshots.
    pub fn apply(&mut self, outcome: Outcome) -> Result<(), GameError> {
        self.with_effect_publication(|candidate| {
            // Standalone resolved effects can arrive between timer sweeps. Earned
            // automatic work belongs to the old policy, before this effect can
            // change senses, placement or eligibility.
            let mut owners = [outcome.actor, outcome.target];
            owners.sort();
            candidate.settle_actor_time(owners[0])?;
            if owners[1] != owners[0] {
                candidate.settle_actor_time(owners[1])?;
            }
            candidate.apply_outcome(outcome)
        })
    }

    /// Publish one complete public effect operation. Internal effect/rider work
    /// stays on apply_outcome until its owning transaction finishes participation.
    /// A host candidate retains its observer extension for the outer publication.
    pub(crate) fn with_effect_publication<R>(
        &mut self,
        change: impl FnOnce(&mut Self) -> Result<R, GameError>,
    ) -> Result<R, GameError> {
        let mut candidate = self.clone();
        candidate.begin_effect_publication()?;
        let result = change(&mut candidate)?;
        candidate.finish_effect_publication()?;
        *self = candidate;
        Ok(result)
    }

    /// Existing private candidates share the same publication boundary without
    /// nesting another whole-Game transaction.
    pub(crate) fn begin_effect_publication(&mut self) -> Result<(), GameError> {
        if !self.host_region_continuation {
            self.seal_event_observations()?;
        }
        Ok(())
    }

    pub(crate) fn finish_effect_publication(&mut self) -> Result<(), GameError> {
        self.reconcile_recovery_contexts()?;
        self.reconcile_passive_discovery()?;
        self.reconcile_awareness()?;
        self.reconcile_control_contacts()?;
        self.settle_aggravation()?;
        self.settle_periodic_searches()?;
        self.settle_periodic_relocations()?;
        self.seal_participant_events()
    }

    /// Public actor mutations keep the cheap eventless setup path. Any finalized
    /// lifecycle event, new contextual owner or caller-owned rollback need uses
    /// the same atomic publication as a complete resolved effect operation.
    pub(crate) fn publish_actor_effect(
        &mut self,
        prepared: PreparedActorOutcome,
        requires_candidate: bool,
    ) -> Result<(), GameError> {
        let requires_candidate = requires_candidate
            || prepared.outcome.state.awareness.pending()
            || prepared.outcome.state.search_clock.pending()
            || prepared.outcome.state.periodic_relocations.pending()
            || prepared.emits_events()
            || prepared
                .creature()
                .is_some_and(|creature| creature.derived().contextual_health_recovery());
        self.with_recovery_transition(requires_candidate, |candidate| {
            candidate.publish_actor_outcome_after_search(prepared)
        })
    }

    /// Trusted source/placement changes settle an earned scan at the old place,
    /// before installing the new policy. Pure previews never call this boundary.
    pub(crate) fn publish_actor_outcome_after_search(
        &mut self,
        mut prepared: PreparedActorOutcome,
    ) -> Result<(), GameError> {
        if let Some(denominator) = prepared.outcome.state.awareness.take_pending() {
            self.run_aggravation_trial(prepared.outcome.id, denominator)?;
        }
        if let Some(check) = prepared.outcome.state.search_clock.take_pending() {
            let id = prepared.outcome.id;
            self.run_active_discovery(id, &check)?;
            prepared.outcome.state.discovery = self.actors[&id].discovery.clone();
        }
        self.publish_actor_outcome(prepared)
    }

    pub(crate) fn apply_outcome(
        &mut self,
        outcome: impl ApplicationOutcome,
    ) -> Result<(), GameError> {
        outcome.apply(self)
    }

    fn apply_item_outcome(&mut self, outcome: ItemOutcome) -> Result<(), GameError> {
        self.push_event(
            if matches!(&outcome.change, ItemChange::CurseBlocked(_)) {
                CombatEvent::ItemCurseBlocked {
                    actor: outcome.actor,
                    item: outcome.work.item().ok_or(GameError::ItemUnavailable)?,
                    intention: outcome.intention,
                }
            } else {
                CombatEvent::ItemCompleted {
                    actor: outcome.actor,
                    work: outcome.work,
                    intention: outcome.intention,
                }
            },
            None,
        )?;
        match outcome.change {
            ItemChange::CurseBlocked(actor) | ItemChange::Equipment(actor) => {
                self.commit_actor_outcome(actor)?
            }
            ItemChange::Consumed { actor } => self.commit_actor_outcome(actor)?,
            ItemChange::PortableLight { item, lit, actor } => {
                self.set_item_light(item, lit)?;
                self.commit_actor_outcome(actor)?;
            }
        }
        Ok(())
    }

    /// Shared authoritative effect publication for a standalone outcome or a
    /// sequence of primary/rider batches sharing one first-damage policy guard.
    pub(crate) fn apply_resolved_outcome(
        &mut self,
        outcome: Outcome,
        site: ResolutionPublication,
        activity: &mut DamageActivityGuard,
    ) -> Result<PublishedResolution, GameError> {
        self.apply_resolved_outcome_with_origin(outcome, site, activity, None)
    }

    /// Future damaging-condition/displacement adapters use this same boundary.
    /// They must materialize the causal owner without activating its floor before
    /// execution; an unloaded owner fails atomically instead of losing XP.
    pub(crate) fn apply_resolved_outcome_with_origin(
        &mut self,
        outcome: Outcome,
        site: ResolutionPublication,
        activity: &mut DamageActivityGuard,
        origin: Option<crate::earned_growth::FatalOrigin>,
    ) -> Result<PublishedResolution, GameError> {
        self.apply_resolved_outcome_with_transfer(outcome, site, activity, origin, None, None)
    }

    pub(crate) fn apply_resolved_with_disclosure(
        &mut self,
        outcome: Outcome,
        site: ResolutionPublication,
        activity: &mut DamageActivityGuard,
        disclosure_origin: crate::event_history::EventOrigin,
    ) -> Result<PublishedResolution, GameError> {
        self.apply_resolved_outcome_with_transfer(
            outcome,
            site,
            activity,
            None,
            None,
            Some(disclosure_origin),
        )
    }

    pub(crate) fn apply_resolved_theft(
        &mut self,
        outcome: Outcome,
        site: ResolutionPublication,
        activity: &mut DamageActivityGuard,
        receipt: crate::theft::TransferReceipt,
        disclosure_origin: crate::event_history::EventOrigin,
    ) -> Result<PublishedResolution, GameError> {
        self.apply_resolved_outcome_with_transfer(
            outcome,
            site,
            activity,
            None,
            Some(receipt),
            Some(disclosure_origin),
        )
    }

    fn apply_resolved_outcome_with_transfer(
        &mut self,
        mut outcome: Outcome,
        site: ResolutionPublication,
        activity: &mut DamageActivityGuard,
        origin: Option<crate::earned_growth::FatalOrigin>,
        transfer: Option<crate::theft::TransferReceipt>,
        disclosure_origin: Option<crate::event_history::EventOrigin>,
    ) -> Result<PublishedResolution, GameError> {
        let mut disclosure_origin = disclosure_origin
            .unwrap_or_else(|| self.capture_event_origin(outcome.actor, outcome.target));
        if outcome
            .ability
            .as_ref()
            .is_some_and(|id| id.as_str() == "rattling_blow")
            && self
                .actors
                .get(&outcome.target)
                .is_some_and(|target| !target.perception().sight)
        {
            // A blind recipient can feel this contact, but not identify the
            // attacker's technique from the body's near-field presence.
            disclosure_origin.cap_contact_source();
        }
        let recorded_transfer = outcome.effects.iter().find_map(|effect| match effect {
            ResolvedEffect::Theft { quantity, .. } => Some(*quantity),
            _ => None,
        });
        if transfer
            .as_ref()
            .is_some_and(|receipt| !receipt.matches(outcome.actor, outcome.target))
            || recorded_transfer
                != transfer
                    .as_ref()
                    .map(crate::theft::TransferReceipt::quantity)
            || (recorded_transfer.is_some()
                && (outcome.effects.len() != 1
                    || outcome.actor == outcome.target
                    || !matches!(site, ResolutionPublication::Rider { .. })))
        {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        }
        if outcome.result != crate::abilities::OutcomeStatus::Miss {
            outcome.result = outcome.canonical_result();
        }
        if outcome.target_rider_participated || !outcome.valid() {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        }
        let role = self.publication_role(&outcome, site)?;
        self.publish_target_effect(&outcome, disclosure_origin.ability_disclosure())?;
        if outcome.actor != outcome.target
            && outcome.target_participated()
            && outcome
                .ability
                .as_ref()
                .and_then(|ability| self.content.rules.ability(ability))
                .is_some_and(|ability| {
                    ability.interaction == crate::rules::Interaction::HostileToTarget
                })
            && self
                .actors
                .get(&outcome.target)
                .is_some_and(|actor| actor.guise.appearance().is_some())
        {
            // The attack breaks a newly acquired guise, but its historical
            // reference ceiling was captured before this reveal.
            self.reveal_guise(outcome.target)?;
        }

        // Detection is a sole self operation. Commit only its real actor
        // mutation on this outer unpublished transaction candidate before the
        // producer performs its one complete census/projection/delivery pass.
        let mut chart_completion = None;
        if let Some(kind) = outcome
            .effects
            .iter()
            .find_map(crate::detection_lifecycle::resolved_kind)
        {
            if !crate::detection_lifecycle::resolved_shape_valid(&outcome)
                || matches!(role, PublishedRole::Rider { .. })
            {
                return Err(GameError::Rule(RuleError::InvalidPlan));
            }
            let mut condition_changed = false;
            if let ResolvedEffect::DetectMonsters { id, duration, .. } = &outcome.effects[0] {
                if !crate::detection_lifecycle::valid_resolved_duration(*duration)
                    || !self
                        .content
                        .rules()
                        .condition(id)
                        .is_some_and(|definition| definition.detecting_monsters)
                {
                    return Err(GameError::Rule(RuleError::InvalidPlan));
                }
                let mut activation = self.settled_actor_outcome(outcome.actor)?;
                if !activation.state.alive() {
                    return Err(GameError::UnknownActor);
                }
                let combat = activation
                    .state
                    .combat
                    .as_mut()
                    .ok_or(GameError::UnknownActor)?;
                let before = combat.creature().conditions().clone();
                combat.apply_condition(id, outcome.actor, *duration, None)?;
                condition_changed = &before != combat.creature().conditions();
                self.commit_actor_outcome(activation)?;
            }
            let completed = self.complete_detection_chart(outcome.actor, kind)?;
            let receipt = crate::detection_lifecycle::CompletionReceipt::from_admitted(
                kind,
                &completed.candidate,
            )?;
            debug_assert_eq!(receipt.identifies(), completed.admitted_nonempty());
            let changed = condition_changed || completed.history_changed;
            match &mut outcome.effects[0] {
                ResolvedEffect::DetectMonsters { changed: value, .. }
                | ResolvedEffect::DetectFood { changed: value }
                | ResolvedEffect::MapTopology { changed: value }
                | ResolvedEffect::DetectMagic { changed: value } => *value = changed,
                _ => unreachable!("closed detection kind"),
            }
            chart_completion = Some(receipt);
        }

        if outcome
            .effects
            .iter()
            .any(|effect| matches!(effect, ResolvedEffect::DrainLife { .. }))
        {
            if outcome.actor != outcome.target || outcome.effects.len() != 1 {
                return Err(GameError::Rule(RuleError::InvalidPlan));
            }
            // Snapshot/distribute before taking the outer actor candidate: nested
            // ordinary damage/death/XP publication must never be overwritten.
            let changed = self.apply_drain_life(&outcome, activity)?;
            outcome.effects[0] = ResolvedEffect::DrainLife { changed };
        }

        let strike = match role {
            PublishedRole::Primary { strike } => Some(strike),
            PublishedRole::Standalone | PublishedRole::Rider { .. } => None,
        };
        let recipient = outcome.effects.first().map_or(
            crate::abilities::EffectRecipient::Target,
            ResolvedEffect::recipient,
        );
        let recipient_id = match recipient {
            crate::abilities::EffectRecipient::Target => outcome.target,
            crate::abilities::EffectRecipient::Source => outcome.actor,
        };
        self.validate_equipment_alterations(recipient_id, &outcome.effects)?;
        let mut transition = self.settled_actor_outcome(recipient_id)?;
        transition.disclosure_origin = Some(disclosure_origin.clone());
        let placement = crate::abilities::Placement {
            location: transition.state.location,
            orientation: transition.state.orientation,
        };
        let original_placement = placement;
        let EffectApplication {
            mut transition,
            placement,
            forced,
            damaged,
            created_hold,
        } = self.apply_outcome_effects(
            &mut outcome,
            recipient_id,
            transition,
            placement,
            EffectAttribution {
                activity,
                origin: origin.as_ref(),
                strike,
                disclosure_origin: &disclosure_origin,
            },
        )?;
        // Damage components, self-damage and source/target overlap must not
        // apply percentage quiet resets more than once per strike.
        let source_id = outcome.actor;
        let target_id = recipient_id;
        transition.state.location = placement.location;
        transition.state.orientation = placement.orientation;
        if placement != original_placement {
            transition.state.visited.insert(placement.location.region);
            if transition.state.motion.active_fall.is_some()
                && !self.fall_continues(recipient_id, &transition.state)
            {
                transition.state.motion.active_fall = None;
            }
        }
        // A resolution event precedes its interruption/death consequences.
        transition.events.insert(
            0,
            CombatEvent::Resolved {
                outcome,
                strike,
                source_disclosure: crate::combat::SourceAbilityDisclosure::Public,
            },
        );
        let receipt = PublishedResolution {
            chart_completion,
            created_hold,
            event: self.event_log.len(),
            role,
        };
        self.commit_actor_outcome(transition)?;
        if forced {
            self.physics.displaced.insert(target_id);
        }
        if placement != original_placement {
            self.sync_actor_lifecycle(target_id)?;
        }
        // Target death/cancellation can remove source-held token claims.
        // Capture only after that publication, never overwrite the source
        // with an older snapshot. Full/dead Health has no banked work.
        let source_needs_reset = damaged
            && source_id != target_id
            && activity.first(source_id)
            && self
                .actors
                .get(&source_id)
                .and_then(|actor| actor.combat.as_ref())
                .is_some_and(|combat| {
                    let creature = combat.creature();
                    !creature.health().dead()
                        && creature.health().injury() > 0
                        && creature.derived().health_recovery_reset_percent > 0
                });
        if source_needs_reset {
            let mut source = self.settled_actor_outcome(source_id)?;
            if let Some(combat) = source.state.combat.as_mut() {
                combat.damage_activity();
            }
            self.commit_actor_outcome(source)?;
        }
        self.published_resolution(receipt)?;
        Ok(receipt)
    }

    /// Apply a resolved batch's effects to its recipient in order, staging them
    /// on `transition`. Effects that did not apply are dropped from
    /// `outcome.effects`, so event accounting holds only applied amounts.
    fn apply_outcome_effects(
        &mut self,
        outcome: &mut Outcome,
        recipient_id: ActorId,
        mut transition: ActorOutcome,
        mut placement: crate::abilities::Placement,
        attribution: EffectAttribution<'_>,
    ) -> Result<EffectApplication, GameError> {
        // Relocation witnesses are live execution facts, not preparation-time
        // destinations. Recheck immediately before publication; a blocked exact
        // endpoint remains a paid failed effect and cannot fall back elsewhere.
        for effect in &mut outcome.effects {
            if let ResolvedEffect::Relocate { result, .. } = effect {
                if let crate::abilities::MovementResult::Moved { from, to } = *result {
                    if from != placement {
                        *result = crate::abilities::MovementResult::Stale;
                    } else if !self.relocation_destination_valid(recipient_id, to) {
                        *result = crate::abilities::MovementResult::Blocked;
                    }
                }
            }
        }
        let mut forced = false;
        let mut damaged = false;
        let mut applied_count = 0;
        let mut created_hold = None;
        for index in 0..outcome.effects.len() {
            let effect = &outcome.effects[index];
            if matches!(effect, ResolvedEffect::DrainLife { .. })
                || crate::detection_lifecycle::resolved_kind(effect).is_some()
            {
                outcome.effects[applied_count] = effect.clone();
                applied_count += 1;
                continue;
            }
            if matches!(effect, ResolvedEffect::RoomLight { .. }) {
                if recipient_id != outcome.actor || outcome.actor != outcome.target {
                    return Err(GameError::Rule(RuleError::InvalidPlan));
                }
                let changed = self.apply_room_light(placement.location)?;
                outcome.effects[applied_count] = ResolvedEffect::RoomLight { changed };
                applied_count += 1;
                continue;
            }
            if let ResolvedEffect::SearchAdjacentGuises { roll } = effect {
                if recipient_id != outcome.actor || outcome.actor != outcome.target {
                    return Err(GameError::Rule(RuleError::InvalidPlan));
                }
                self.apply_discovery_roll(outcome.actor, *roll)?;
                transition.state.discovery = self.actors[&outcome.actor].discovery.clone();
                outcome.effects[applied_count] = effect.clone();
                applied_count += 1;
                continue;
            }
            if matches!(effect, ResolvedEffect::Theft { .. }) {
                outcome.effects[applied_count] = effect.clone();
                applied_count += 1;
                continue;
            }
            if let ResolvedEffect::AlterEquipment { alteration } = effect {
                if self.alter_equipment_in_outcome(&mut transition, alteration)? {
                    outcome.effects[applied_count] = effect.clone();
                    applied_count += 1;
                }
                continue;
            }
            if let ResolvedEffect::CorrodeArmor { slot, amount } = effect {
                let applied = self.corrode_armor_in_outcome(&mut transition, slot, *amount)?;
                if applied != 0 {
                    outcome.effects[applied_count] = ResolvedEffect::CorrodeArmor {
                        slot: slot.clone(),
                        amount: applied,
                    };
                    applied_count += 1;
                }
                continue;
            }
            if let ResolvedEffect::ApplyLinkedHold { policy } = effect {
                // Earlier effects become authoritative before testing contact/capability.
                transition.state.location = placement.location;
                transition.state.orientation = placement.orientation;
                let mut events = std::mem::take(&mut transition.events);
                let event_start = self.event_log.len();
                self.commit_actor_outcome(transition)?;
                events.extend(self.take_event_suffix(event_start)?.0);
                let highwater = self.next_hold_id;
                let applied = self.apply_linked_hold_in_transaction(
                    outcome.actor,
                    recipient_id,
                    policy.clone(),
                )?;
                if self.next_hold_id != highwater {
                    created_hold = created_hold.or(applied);
                }
                transition = self.actor_outcome(recipient_id)?;
                transition.disclosure_origin = Some(attribution.disclosure_origin.clone());
                transition.events.extend(events);
                if applied.is_some() {
                    outcome.effects[applied_count] = effect.clone();
                    applied_count += 1;
                }
                continue;
            }
            let alive_before = transition.state.alive();
            if matches!(
                effect,
                ResolvedEffect::ApplyCondition { .. }
                    | ResolvedEffect::ApplyPersistentCondition { .. }
                    | ResolvedEffect::HasteDose { .. }
            ) && !self.actors.contains_key(&outcome.actor)
                && !self.detached_actor(outcome.actor)
            {
                return Err(GameError::UnknownActor);
            }
            if let ResolvedEffect::ApplyQualifiedConditionToSource { qualification, .. } = effect {
                if qualification.opponent != outcome.target
                    || (!self.actors.contains_key(&qualification.opponent)
                        && !self.detached_actor(qualification.opponent))
                {
                    return Err(GameError::Rule(RuleError::InvalidPlan));
                }
            }
            if let ResolvedEffect::ObservePublicPlan { observation } = effect {
                if !observation.valid(&self.content.rules, &self.world)
                    || observation.observed_at != self.tick
                {
                    return Err(GameError::Rule(RuleError::InvalidPlan));
                }
            }
            let context = match effect {
                ResolvedEffect::ApplyCondition { id, .. }
                | ResolvedEffect::ApplyPersistentCondition { id }
                | ResolvedEffect::RemoveCondition { id }
                | ResolvedEffect::ApplyQualifiedConditionToSource { id, .. } => {
                    if self.condition_uses_recovery_context(id)? {
                        Some(self.recovery_context_at_placement(recipient_id, placement)?)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            let nutrition_before = transition
                .state
                .combat
                .as_ref()
                .map(|combat| combat.creature().nutrition());
            let cancellation_return = if matches!(effect, ResolvedEffect::CancelSpecials { .. }) {
                transition.state.location = placement.location;
                transition.state.orientation = placement.orientation;
                self.stage_cancellation_return(&mut transition)?
            } else {
                None
            };
            let applied = if let Some(changed) = cancellation_return {
                Some(ResolvedEffect::CancelSpecials { changed })
            } else {
                transition
                    .state
                    .combat
                    .as_mut()
                    .ok_or(GameError::Rule(RuleError::InvalidDefinition))?
                    .apply_effect(
                        outcome.actor,
                        effect,
                        &mut transition.state.public_plan,
                        &mut placement,
                        context,
                    )?
            };
            let Some(mut applied) = applied else {
                continue;
            };
            if let ResolvedEffect::BalefulPolymorph { form, changed } = &mut applied {
                *changed = false;
                if let Some(form) = form {
                    let package = self.content.polymorph_body_package(form).cloned().ok_or(
                        GameError::Build(crate::creatures::BuildError::InvalidDefinition),
                    )?;
                    transition.state.location = placement.location;
                    transition.state.orientation = placement.orientation;
                    *changed = self.stage_body_transition(&mut transition, package)?
                        == crate::BodyTransitionResult::Changed;
                }
            }
            if let ResolvedEffect::CancelSpecials { changed } = &mut applied {
                let before = transition.state.guise.clone();
                transition.state.guise.reveal();
                *changed |= transition.state.guise != before;
                *changed |= self.stage_polymorph_return(&mut transition)?;
                // Normal commit reconciles exactly those reciprocal linked
                // holds whose explicitly tagged sustaining flag disappeared.
            }
            if self.survival_diagnostics.is_some() {
                transition.survival.effect(&applied, nutrition_before);
            }
            if matches!(applied, ResolvedEffect::ArrestDescent) {
                self.arrest_descent_in_outcome(&mut transition)?;
            }
            if let ResolvedEffect::Relocate { mode, result } = &applied {
                if result.changed() {
                    transition.entry_cause = crate::floor_activation::EntryCause::Teleport;
                    transition.state.motion = crate::MotionState::default();
                    if *mode == crate::abilities::MovementMode::Forced {
                        transition
                            .interrupt(Interruption::Discard, self.actor_clock(recipient_id))?;
                    }
                }
            }
            if matches!(applied, ResolvedEffect::Translate { mode: crate::abilities::MovementMode::Forced, result } if result.changed())
            {
                forced = true;
                transition.entry_cause = crate::floor_activation::EntryCause::Physics;
                let episode = self.physics.next_fall_episode;
                self.physics.next_fall_episode =
                    episode.checked_add(1).ok_or(GameError::IdentityExhausted)?;
                transition.state.motion.active_fall = Some(crate::physics::ActiveFall {
                    owner: outcome.actor,
                    episode,
                    started_at: self.tick,
                });
                // Retire pending work now, including actors already scanned by
                // this preparation scheduler boundary. Do not depend on ID order.
                transition.interrupt(Interruption::Discard, self.actor_clock(recipient_id))?;
            }
            if matches!(applied, ResolvedEffect::Translate {
                mode: crate::abilities::MovementMode::Voluntary, result,
            } if result.changed())
            {
                transition.state.motion.active_fall = None;
            }
            if alive_before && !transition.state.alive() {
                let effect = u16::try_from(index)
                    .map_err(|_| GameError::Rule(RuleError::InvalidDefinition))?;
                transition.fatal_cause = Some(crate::earned_growth::FatalCause {
                    owner: outcome.actor,
                    origin: attribution.origin.cloned().unwrap_or_else(|| {
                        crate::earned_growth::FatalOrigin::Direct {
                            ability: outcome.ability.clone(),
                            intention: outcome.intention,
                            strike: attribution.strike,
                            effect,
                        }
                    }),
                });
            }
            if matches!(applied, ResolvedEffect::Damage { amount } if amount > 0)
                || matches!(applied, ResolvedEffect::DirectHealthLoss { amount } if amount > 0)
            {
                // The policy at the first actual damage wins; a later
                // effect cannot retroactively preserve earlier progress.
                if !damaged && attribution.activity.first(recipient_id) {
                    transition
                        .state
                        .combat
                        .as_mut()
                        .expect("resolved creature")
                        .damage_activity();
                }
                damaged = true;
                transition.interrupt(Interruption::Suspend, self.actor_clock(recipient_id))?;
            }
            if matches!(&applied, ResolvedEffect::Translate { result, .. } | ResolvedEffect::Relocate { result, .. } if result.changed())
                || self.hold_capability_lost(recipient_id, &transition.state)
            {
                // The accepted spatial prefix is authoritative before later
                // effects, including a fatal effect that allocates a corpse.
                // Use ordinary lifecycle publication so claims/clocks retire
                // once, then continue from that candidate's current actor.
                transition.state.location = placement.location;
                transition.state.orientation = placement.orientation;
                transition.state.visited.insert(placement.location.region);
                if transition.state.motion.active_fall.is_some()
                    && !self.fall_continues(recipient_id, &transition.state)
                {
                    transition.state.motion.active_fall = None;
                }
                let entry_cause = transition.entry_cause;
                let event_start = self.event_log.len();
                self.commit_actor_outcome(transition)?;
                let events = self.take_event_suffix(event_start)?.0;
                transition = self.actor_outcome(recipient_id)?;
                transition.disclosure_origin = Some(attribution.disclosure_origin.clone());
                transition.entry_cause = entry_cause;
                // Publication can emit interruption/sequence consequences.
                // Keep them behind this batch's one original Resolved event,
                // without applying any resolved effect a second time.
                transition.events = events;
            }
            // Compact in place so event accounting contains only actual
            // applied amounts, without a second resolution or allocation.
            outcome.effects[applied_count] = applied;
            applied_count += 1;
        }
        outcome.effects.truncate(applied_count);
        outcome.result = outcome.canonical_result();
        Ok(EffectApplication {
            transition,
            placement,
            forced,
            damaged,
            created_hold,
        })
    }

    /// A paid primary whose frozen target is no longer reachable still owns an
    /// offensive event, but never fabricates a target application or check.
    pub(crate) fn publish_unavailable_primary(
        &mut self,
        outcome: Outcome,
        strike: u16,
        origin: crate::event_history::EventOrigin,
    ) -> Result<PublishedResolution, GameError> {
        if !outcome.valid()
            || outcome.result != crate::abilities::OutcomeStatus::Miss
            || outcome.check.is_some()
            || outcome.resistance.is_some()
            || outcome.target_rider_participated
            || !outcome.effects.is_empty()
            || outcome.participation
                != crate::abilities::AttemptedParticipation::blocked(
                    None,
                    crate::abilities::UnselectedReason::UnavailableTarget,
                )
        {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        }
        let role = self.publication_role(&outcome, ResolutionPublication::Primary { strike })?;
        let receipt = PublishedResolution {
            chart_completion: None,
            created_hold: None,
            event: self.event_log.len(),
            role,
        };
        self.push_event(
            CombatEvent::Resolved {
                outcome,
                strike: Some(strike),
                source_disclosure: crate::combat::SourceAbilityDisclosure::Public,
            },
            Some(origin),
        )?;
        self.published_resolution(receipt)?;
        Ok(receipt)
    }

    /// The private publication receipt is the causal selector. Validate the
    /// exact final primary outcome, including rider participation, before any
    /// consumer uses its event index; ordinals alone are not event identities.
    pub(crate) fn published_primary_event(
        &self,
        receipt: PublishedResolution,
        outcome: &Outcome,
    ) -> Result<usize, GameError> {
        if !matches!(receipt.role, PublishedRole::Primary { .. })
            || self.published_resolution(receipt)? != outcome
        {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        }
        Ok(receipt.event)
    }

    pub(crate) fn published_resolution(
        &self,
        receipt: PublishedResolution,
    ) -> Result<&Outcome, GameError> {
        let Some(CombatEvent::Resolved {
            outcome, strike, ..
        }) = self.event_log.get(receipt.event)
        else {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        };
        let valid_role = match receipt.role {
            PublishedRole::Standalone => strike.is_none() && !outcome.target_rider_participated,
            PublishedRole::Primary { strike: ordinal } => {
                *strike == Some(ordinal) && outcome.ability.is_some()
            }
            PublishedRole::Rider { .. } => {
                strike.is_none() && outcome.ability.is_none() && !outcome.target_rider_participated
            }
        };
        if valid_role
            && outcome.valid()
            && receipt
                .chart_completion
                .is_none_or(|chart| chart.matches(outcome))
        {
            Ok(outcome)
        } else {
            Err(GameError::Rule(RuleError::InvalidPlan))
        }
    }

    fn publication_role(
        &self,
        outcome: &Outcome,
        site: ResolutionPublication,
    ) -> Result<PublishedRole, GameError> {
        let invalid = || GameError::Rule(RuleError::InvalidPlan);
        match site {
            ResolutionPublication::Standalone => Ok(PublishedRole::Standalone),
            ResolutionPublication::Primary { strike }
                if usize::from(strike) < crate::limits::STRIKES && outcome.ability.is_some() =>
            {
                Ok(PublishedRole::Primary { strike })
            }
            ResolutionPublication::Rider {
                primary,
                strike,
                ordinal,
            } => {
                if primary.role != (PublishedRole::Primary { strike })
                    || usize::from(ordinal) >= crate::limits::RIDERS_PER_STRIKE
                    || outcome.ability.is_some()
                {
                    return Err(invalid());
                }
                let parent = self.published_resolution(primary)?;
                if (parent.actor, parent.target, parent.intention)
                    != (outcome.actor, outcome.target, outcome.intention)
                    || matches!(
                        parent.participation.payload,
                        crate::abilities::PayloadParticipation::Unselected { .. }
                    )
                {
                    return Err(invalid());
                }
                Ok(PublishedRole::Rider {
                    primary: primary.event,
                    strike,
                    ordinal,
                })
            }
            ResolutionPublication::Primary { .. } => Err(invalid()),
        }
    }

    /// Only a committed rider can license its own strike's existing public event.
    /// This never changes the primary check, result, effects, or event ordering.
    pub(crate) fn merge_committed_rider_participation(
        &mut self,
        primary: PublishedResolution,
        rider: PublishedResolution,
    ) -> Result<bool, GameError> {
        if self.event_is_sealed(primary.event) {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        }
        let PublishedRole::Primary { strike } = primary.role else {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        };
        if !matches!(rider.role, PublishedRole::Rider { primary: parent, strike: rider_strike, .. } if parent == primary.event && rider_strike == strike)
            || rider.event <= primary.event
        {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        }
        let parent = self.published_resolution(primary)?;
        let child = self.published_resolution(rider)?;
        if (parent.actor, parent.target, parent.intention)
            != (child.actor, child.target, child.intention)
            || matches!(
                parent.participation.payload,
                crate::abilities::PayloadParticipation::Unselected { .. }
            )
        {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        }
        let participated =
            parent.target_rider_participated || child.participation.target_participated();
        let Some(CombatEvent::Resolved { outcome, .. }) = self.event_log.get_mut(primary.event)
        else {
            unreachable!("validated primary receipt")
        };
        outcome.target_rider_participated = participated;
        self.bind_event_participation(primary.event)?;
        Ok(participated)
    }

    fn commit_actor_outcome(&mut self, outcome: ActorOutcome) -> Result<(), GameError> {
        let prepared = self.prepare_actor_outcome(outcome)?;
        self.publish_actor_outcome(prepared)
    }

    /// Finalize the owned actor and validate death allocation without mutation.
    /// Preview and execution see the same post-equipment-removal creature state.
    pub(crate) fn prepare_actor_outcome(
        &self,
        mut outcome: ActorOutcome,
    ) -> Result<PreparedActorOutcome, GameError> {
        let before = self
            .actors
            .get(&outcome.id)
            .ok_or(GameError::UnknownActor)?;
        let mut rebuild = crate::creatures::RebuildOutcome::default();
        if let (Some(after), Some(prior)) = (outcome.state.combat.as_mut(), before.combat.as_ref())
        {
            rebuild = self.retry_growth_with_recovery_context(
                outcome.id,
                crate::abilities::Placement {
                    location: outcome.state.location,
                    orientation: outcome.state.orientation,
                },
                after,
                prior,
            )?;
        }
        // A supported species rebuild settles old active time first. Its new
        // explicit memory policy can shorten an engagement, never resurrect one
        // already removed by the old policy or reset the actor's random stream.
        outcome
            .state
            .behavior
            .validate_deadline(outcome.state.behavior_policy())?;
        if before.behavior_policy() != outcome.state.behavior_policy() {
            outcome.state.behavior.engagement =
                outcome.state.engagement_at(self.actor_clock(outcome.id));
        }
        if !outcome.state.alive() {
            self.set_return_pending(&mut outcome, false)?;
        }
        let died = before.alive() && !outcome.state.alive();
        if let Some(horizon) = outcome
            .state
            .combat
            .as_ref()
            .and_then(|combat| combat.creature().condition_horizon())
        {
            outcome
                .state
                .active_since
                .checked_add(horizon)
                .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
        }
        rebuild.died |= died;
        let invalid_preparation = outcome.state.pending.as_ref().is_some_and(|pending| {
            outcome.state.combat.as_ref().is_some_and(|combat| {
                pending
                    .hold
                    .is_some_and(|owner| combat.creature().costs().reservation(owner).is_none())
                    || matches!(&pending.plan,crate::abilities::Plan::Respec(plan) if plan.basis.build_epoch!=combat.creature().build_epoch())
                    || pending.plan.ability_id().is_some_and(|ability| {
                        !combat
                            .creature()
                            .derived()
                            .abilities
                            .contains(ability)
                    })
            })
        });
        if invalid_preparation {
            if let Some(owner) = outcome
                .state
                .pending
                .as_ref()
                .and_then(|pending| pending.hold)
            {
                if !rebuild.canceled.contains(&owner) {
                    rebuild.canceled.push(owner);
                }
            }
            outcome.interrupt(Interruption::Discard, self.actor_clock(outcome.id))?;
        }
        // An authored status pauses whatever valid work still exists. Earlier
        // damage, grant loss or quiet-gate invalidation can already have discarded
        // it; this never recreates work or adopts that cause's discard policy.
        if outcome.state.pending.as_ref().is_some_and(|pending| {
            outcome.state.combat.as_ref().is_some_and(|combat| {
                combat
                    .creature()
                    .condition_pauses_preparation(&pending.plan.work().action())
            })
        }) {
            let intention = outcome
                .state
                .pending
                .as_ref()
                .and_then(|pending| pending.intention);
            if outcome.pause(self.actor_clock(outcome.id)).is_some()
                && !outcome.events.iter().any(|event| {
                    matches!(event,
                    CombatEvent::Interrupted { actor, intention: prior }
                    if *actor == outcome.id && *prior == intention)
                })
            {
                outcome.events.push(CombatEvent::Interrupted {
                    actor: outcome.id,
                    intention,
                });
            }
        }
        // Validate every fallible death operation before publishing the actor.
        let death = if died {
            Some(self.death_outcome(&mut outcome)?)
        } else {
            None
        };
        if let Some(combat) = outcome.state.combat.as_ref() {
            let learned: Vec<_> = combat
                .creature()
                .derived()
                .abilities
                .iter()
                .filter(|ability| !outcome.state.recognized_abilities.contains(*ability))
                .cloned()
                .collect();
            if !learned.is_empty() {
                outcome.state.recognized_abilities.extend(learned);
            }
        }
        outcome
            .state
            .search_clock
            .rephase(outcome.state.perception().search_interval);
        outcome.state.control.reconcile_conditions(
            outcome
                .state
                .combat
                .as_ref()
                .map(|combat| combat.creature().conditions()),
        );
        outcome.state.awareness.reconcile_alert(
            outcome.state.behavior_policy().map(|p| p.memory_ticks),
            self.actor_clock(outcome.id),
            before.location,
            outcome.state.location,
        )?;
        outcome
            .state
            .awareness
            .rephase(outcome.state.perception().aggravation);
        self.reconcile_relocation_sources(&mut outcome)?;
        let generation = crate::hallucination_clock::hallucination_instance(&outcome.state)
            .map(|(generation, _)| generation);
        crate::hallucination_clock::reconcile(
            &mut outcome.state.hallucination,
            generation,
            outcome.state.turn_ticks,
        );
        if let Some(interval) = crate::hallucination_clock::next_in(&outcome.state) {
            outcome
                .state
                .active_since
                .checked_add(interval)
                .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
        }
        Ok(PreparedActorOutcome {
            outcome,
            death,
            rebuild,
        })
    }

    /// Publish exactly one already finalized actor under the existing transaction.
    pub(crate) fn publish_actor_outcome(
        &mut self,
        prepared: PreparedActorOutcome,
    ) -> Result<(), GameError> {
        let PreparedActorOutcome {
            mut outcome, death, ..
        } = prepared;
        let id = outcome.id;
        let from = self.actors[&id].location;
        let from_orientation = self.actors[&id].orientation;
        let to = outcome.state.location;
        let ground_contact_changed = from != to
            || self.actors[&id].orientation != outcome.state.orientation
            || self.actors[&id].body != outcome.state.body
            || self.actors[&id].locomotion().hovers() != outcome.state.locomotion().hovers();
        let entry_cause = outcome.entry_cause;
        // Capture private prior knowledge before publishing the destination:
        // discovery caused by this entry cannot retroactively avoid its trap.
        let avoidable_traps = if entry_cause == crate::floor_activation::EntryCause::Movement
            && (from != to || self.actors[&id].orientation != outcome.state.orientation)
        {
            self.known_traps_at_placement(id, to, outcome.state.orientation)?
        } else {
            std::collections::BTreeSet::new()
        };
        let disclosure_origin = outcome
            .disclosure_origin
            .take()
            .or_else(|| (!outcome.events.is_empty()).then(|| self.capture_event_origin(id, id)));
        let perception = outcome.state.perception();
        outcome
            .state
            .guise
            .reconcile(&perception, from != to || !outcome.state.alive());
        if outcome.state.guise.needs_concealment_roll() {
            let source = perception
                .object_guise_check
                .ok_or(GameError::Rule(RuleError::InvalidDefinition))?;
            let creature = outcome
                .state
                .combat
                .as_ref()
                .ok_or(GameError::UnknownActor)?
                .creature();
            let check = crate::discovery::FrozenDiscoveryCheck::capture_skill_for(
                creature,
                id,
                source.skill,
                source.edge,
            )?;
            let total = check.roll(&mut self.rules_rng, false);
            outcome.state.guise.capture_concealment(total);
        }
        outcome
            .state
            .guise
            .retire_claim(outcome.state.pending.as_ref());
        self.reconcile_readied_ambush(&mut outcome)?;
        let previous_items = self.actors[&id]
            .pending
            .as_ref()
            .filter(|previous| !previous.item_payment.refs().is_empty())
            .filter(|previous| {
                outcome.state.pending.as_ref().is_none_or(|next| {
                    previous.item_lineage() != next.item_lineage()
                        || previous.item_payment != next.item_payment
                })
            })
            .cloned();
        if let Some(previous) = previous_items {
            if outcome.spent_items == previous.item_lineage() {
                if outcome.state.pending.as_ref().is_some_and(|next| {
                    next.item_lineage() != previous.item_lineage()
                        || !matches!(next.item_payment, crate::supplies::ItemPayment::Settled)
                }) {
                    return Err(GameError::ItemUnavailable);
                }
            } else {
                self.release_item_payment(&previous)?;
            }
        } else if outcome.spent_items.is_some() {
            return Err(GameError::ItemUnavailable);
        }
        let consumed = self.actors[&id]
            .pending
            .as_ref()
            // No claims means no retirement work, regardless of plan changes.
            .filter(|previous| previous.plan.tokens().len() > 0)
            .filter(|previous| {
                outcome.state.pending.as_ref().is_none_or(|next| {
                    #[cfg(test)]
                    TOKEN_PLAN_COMPARISONS.with(|count| count.set(count.get() + 1));
                    previous.plan != next.plan
                })
            })
            .map(|previous| previous.plan.tokens().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        if self.survival_diagnostics.is_some() {
            outcome.survival.boundary(&self.actors[&id], &outcome.state);
            self.record_survival(id, &outcome.survival);
        }
        if self.actors[&id].locomotion().hovers() != outcome.state.locomotion().hovers() {
            outcome.state.motion.acceleration_remainder = [0; 3];
        }
        if outcome.state.motion.active_fall.is_some() && !self.fall_continues(id, &outcome.state) {
            outcome.state.motion.active_fall = None;
        }
        self.publish_relocation_sources(&outcome)?;
        self.actors.insert(id, outcome.state);
        self.relocate_body_return_authority(id, from, from_orientation);
        for item in outcome.extinguish_lights {
            self.set_item_light(item, false)?;
        }
        self.reconcile_holds_for_actor(id)?;
        self.reconcile_spatial_condition_anchors(id)?;
        self.suspend_condition_denied_continuation(id);
        // Completion, replacement, discard and death all retire the original
        // reservation. Suspended or resumed plans keep it. Tokens are already
        // spent, so this never refunds an edge or recreates a condition.
        for holder_id in consumed
            .iter()
            .map(|token| token.holder)
            .collect::<std::collections::BTreeSet<_>>()
        {
            let tokens: Vec<_> = consumed
                .iter()
                .filter(|token| token.holder == holder_id)
                .cloned()
                .collect();
            if let Some(mut holder) = self.actors.get_mut(&holder_id) {
                if let Some(combat) = holder.combat.as_mut() {
                    combat.consume_tokens(&tokens, id);
                }
            }
        }
        match outcome.input {
            InputBoundary::Preserve => {}
            InputBoundary::Human => {
                if !self.is_ai(id) {
                    self.combat
                        .input_boundaries
                        .insert(id, self.actor_input_eligibility(id));
                }
            }
            InputBoundary::Require => {
                self.combat
                    .input_boundaries
                    .insert(id, self.actor_input_eligibility(id));
            }
        }
        // Preserve membership and its historical cause even during denial.
        // Detached actors retain this bounded value until record admission can
        // validate the live condition book again.
        if self.combat.input_boundaries.contains_key(&id) {
            self.combat
                .input_boundaries
                .insert(id, self.actor_input_eligibility(id));
        }
        for event in outcome.events {
            self.push_event(event, disclosure_origin.clone())?;
        }
        if let Some(death) = death {
            self.combat.input_boundaries.remove(&id);
            for item in death.inventory {
                self.items
                    .edit(item, |item| {
                        item.location = death.body.location;
                        item.motion = death.body.motion.clone();
                        item.orientation = death.body.orientation;
                    })
                    .expect("carried item from current inventory index");
            }
            // Without corpses the body item is discarded unallocated; dropped
            // inventory keeps the body's location and motion either way.
            if self.scenario_capabilities().corpses {
                self.next_item_id = death.next_item_id;
                self.items.insert(death.corpse, death.body);
            }
            self.on_actor_death(id)?;
            if let Some(award) = death.award {
                // Capture after death/claim retirement: never overwrite the
                // owner's current conditions or its paid frozen strike prefix.
                let mut owner = self.settled_actor_outcome(award.owner)?;
                owner
                    .state
                    .combat
                    .as_mut()
                    .ok_or(GameError::UnknownActor)?
                    .stage_xp_award(award.amount)
                    .map_err(GameError::Creature)?;
                self.commit_actor_outcome(owner)?;
            }
        }
        // Publish the entry that actually happened before a destination trap
        // can relocate the actor again. Nested entries own their own cause and
        // must not be followed by a stale outer population/retrieval witness.
        if self.floor_activation.is_some() && from.region != to.region {
            self.sync_actor_lifecycle(id)?;
            self.accepted_region_entry(id, from, to, entry_cause)?;
        }
        if ground_contact_changed {
            if self.actor_frozen(id) && !self.body_return_continuation_authorized(id) {
                // Trusted relocation can move a frozen actor. Old contact
                // identity must not follow it, and an active destination must
                // thaw before its new contact is evaluated.
                self.actors
                    .get_mut(&id)
                    .expect("published actor")
                    .trap_contacts = tor_world::Shared::new(crate::traps::TrapContacts::default());
                self.sync_actor_lifecycle(id)?;
            }
            self.reconcile_trap_contacts_after_entry(id, None, avoidable_traps)?;
        }
        self.finish_pending_body_return(id)?;
        self.refresh_light_boundary();
        Ok(())
    }

    fn death_outcome(&self, outcome: &mut ActorOutcome) -> Result<DeathOutcome, GameError> {
        let award = self.prepare_death_award(outcome)?;
        let actor = &mut outcome.state;
        actor.identification = None;
        let combat = actor.combat.as_mut().ok_or(GameError::UnknownActor)?;
        combat
            .set_equipment(Vec::new())
            .map_err(GameError::Creature)?;
        let next_item_id = self
            .next_item_id
            .checked_add(1)
            .ok_or(GameError::IdentityExhausted)?;
        let mut spec = ItemSpec::ordinary(
            combat.identity.corpse_identity.clone(),
            format!("{} corpse", combat.identity.name),
        );
        spec.class = crate::ItemClass::Corpse;
        spec.properties
            .insert("actor".into(), outcome.id.0.to_string());
        spec.properties
            .insert("death_tick".into(), self.tick.to_string());
        if let Some(preparation) = actor.cancel_preparation() {
            if preparation.plan.ability_id().is_some() {
                outcome.events.push(CombatEvent::SequenceEnded {
                    actor: outcome.id,
                    intention: preparation.intention,
                    reason: crate::combat::SequenceEnd::Abandoned,
                });
            }
        }
        actor.equipment.clear();
        actor.motion.active_fall = None;
        let body = Item {
            relocation_elapsed: None,
            fuel: None,
            condition: crate::armor::ItemCondition::default(),
            charges: None,
            corpse: Some(crate::CorpseAppearance {
                actor: outcome.id,
                asset: actor.current_body_asset(),
            }),
            holds: vec![],
            spec: tor_world::Shared::new(spec),
            quantity: 1,
            location: ItemLocation::Ground(actor.location),
            motion: std::mem::take(&mut actor.motion),
            orientation: actor.orientation,
        };
        let cause = if outcome.starved {
            crate::combat::DeathCause::Starvation
        } else {
            crate::combat::DeathCause::Injury
        };
        outcome.events.push(CombatEvent::Died {
            actor: outcome.id,
            cause,
        });
        Ok(DeathOutcome {
            corpse: ItemId(self.next_item_id),
            next_item_id,
            body,
            inventory: self.items.at(ItemLocation::Carried(outcome.id)).collect(),
            award,
        })
    }

    fn prepare_death_award(&self, outcome: &ActorOutcome) -> Result<Option<DeathAward>, GameError> {
        let Some(cause) = &outcome.fatal_cause else {
            return Ok(None);
        };
        if cause.owner == outcome.id {
            return Ok(None);
        }
        let owner = self
            .actors
            .get(&cause.owner)
            .and_then(|actor| actor.combat.as_ref())
            .ok_or(GameError::UnknownActor)?
            .creature();
        let victim = self
            .actors
            .get(&outcome.id)
            .and_then(|actor| actor.combat.as_ref())
            .ok_or(GameError::UnknownActor)?
            .creature();
        let policy = &owner.build().ruleset().document().growth;
        let amount = policy.reward(
            victim.earned_progress().peak_threat(),
            owner.earned_progress().highest_earned_tier(),
        );
        let mut checked = owner.earned_progress().clone();
        checked
            .award(amount, owner.build().personal_hd_count(), policy)
            .map_err(|error| {
                GameError::Creature(crate::creatures::CreatureStateError::Growth(error))
            })?;
        Ok(Some(DeathAward {
            owner: cause.owner,
            amount,
        }))
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::CombatEvent;
    use crate::content_ids::{AbilityId, TemplateId};
    use crate::rules::{Modifier, ModifierOp, Scope};
    use crate::{Action, ActorId, CreatureIdentity, Game};

    fn game() -> Game {
        game_with_configuration(|_| {})
    }
    pub(super) fn game_with_configuration(
        configure: impl FnOnce(&mut crate::rules::RulesetDocument),
    ) -> Game {
        let mut document = crate::test_support::test_ruleset().document().clone();
        document.abilities.push(crate::test_creatures::paid_melee());
        configure(&mut document);
        let mut ability = document
            .abilities
            .iter()
            .find(|ability| ability.basic_attack)
            .unwrap()
            .clone();
        ability.id = AbilityId::new("borrowed_strike").unwrap();
        ability.name = "Borrowed strike".into();
        ability.basic_attack = false;
        ability.cost = None;
        document.abilities.push(ability.clone());
        document.conditions.push(crate::rules::ConditionDef {
            detecting_monsters: false,
            hallucinating: false,
            id: crate::content_ids::ConditionId::new("borrowed_grant").unwrap(),
            family: crate::content_ids::ConditionFamilyId::new("borrowed_grant").unwrap(),
            lifecycle: crate::rules::ConditionLifecycle::Timed,
            timing: tor_simulation::rules::ConditionTiming::expiring(),
            restrictions: vec![],
            preparation: tor_simulation::rules::ConditionPreparation::Preserve,
            name: "Borrowed grant".into(),
            modifiers: vec![Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: ability.id.clone(),
                },
            }],
            stacking: crate::rules::Stacking::Longest,
            source_scoped: true,
            descriptors: vec![],
            capacity: crate::rules::Eviction::Oldest,
        });
        let mut template =
            crate::creatures::Template::new(TemplateId::new("borrowed_grant").unwrap(), 0);
        template.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: ability.id,
            },
        });
        template.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: AbilityId::new("fixture_paid_melee").unwrap(),
            },
        });
        let species = crate::test_creatures::species();
        let species_id = species.id.clone();
        let content = crate::creatures::Content::new(
            crate::rules::Ruleset::validate(document).unwrap(),
            crate::creatures::ScenarioCatalog {
                species: [(species_id.clone(), species)].into(),
                templates: [(template.id.clone(), template)].into(),
            },
        )
        .unwrap();
        let mut build = crate::creatures::CreatureBuild::new(
            content.clone(),
            species_id,
            crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial], 7)
                .unwrap(),
            crate::attributes::ManaBinding::Intellect,
            [
                tor_simulation::attributes::Skill::Deception,
                tor_simulation::attributes::Skill::Persuasion,
            ],
        )
        .unwrap();
        build
            .set_templates(vec![TemplateId::new("borrowed_grant").unwrap()])
            .unwrap();
        let mut game = Game::two_room_in_stone(42, content);
        for x in [1, 2] {
            let id = game
                .spawn_actor(
                    tor_world::Location {
                        region: tor_world::RegionId(1),
                        position: tor_world::Position { x, y: 1, z: 0 },
                    },
                    std::num::NonZeroU64::new(100).unwrap(),
                )
                .unwrap();
            game.configure_creature(
                id,
                CreatureIdentity {
                    corpse_identity: format!("item.corpse.test.subject-{x}").parse().unwrap(),
                    name: format!("subject {x}"),
                    faction: "neutral".into(),
                },
                build.clone(),
            )
            .unwrap();
        }
        game
    }

    #[test]
    fn equipment_refresh_validates_candidate_anatomy_without_publishing_it() {
        use crate::{AnatomySpec, EquipmentSlotId, EquipmentSpec, ItemSpec};
        use std::collections::BTreeMap;
        use tor_world::Shared;

        let content = game().content().clone();
        let mut game = Game::two_room_in_stone(42, content);
        let at = tor_world::Location {
            region: tor_world::RegionId(1),
            position: tor_world::Position { x: 1, y: 1, z: 0 },
        };
        let actor = game
            .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        let anatomy = AnatomySpec::from_rules(game.content().rules());
        game.configure_anatomy(actor, anatomy.clone()).unwrap();
        let ring = crate::EquipmentSlot::new("ring").unwrap();
        let existing_socket =
            EquipmentSlotId(anatomy.slots.iter().position(|slot| *slot == ring).unwrap() as u16);
        let mut spec = ItemSpec::ordinary(
            "item.test.candidate-ring".parse().unwrap(),
            "candidate ring".into(),
        );
        spec.class = crate::ItemClass::Ring;
        spec.equipment = Some(EquipmentSpec {
            periodic_relocation: None,
            slot: ring.clone(),
            armor: None,
            attack: None,
            weapon_profile: None,
            modifiers: vec![],
            potency_modifiers: vec![],
        });
        let item = game
            .place_item_stack(game.next_item_id, at, Some(actor), 1, spec)
            .unwrap();
        let before = game.clone();
        let mut unchanged = game.settled_actor_outcome(actor).unwrap();
        game.refresh_equipment_outcome(&mut unchanged, BTreeMap::from([(existing_socket, item)]))
            .unwrap();
        let mut removed = game.settled_actor_outcome(actor).unwrap();
        removed.state.anatomy = Shared::new(AnatomySpec::default());
        assert!(
            game.refresh_equipment_outcome(&mut removed, BTreeMap::from([(existing_socket, item)]))
                .is_err(),
            "an old socket must not validate after its removal"
        );
        assert!(removed.state.equipment.is_empty());
        let mut added = game.settled_actor_outcome(actor).unwrap();
        let mut expanded = anatomy;
        let new_socket = EquipmentSlotId(expanded.slots.len() as u16);
        expanded.slots.push(ring);
        assert!(expanded.valid(game.content().rules()));
        added.state.anatomy = Shared::new(expanded);
        game.refresh_equipment_outcome(&mut added, BTreeMap::from([(new_socket, item)]))
            .unwrap();
        assert_eq!(added.state.equipment.get(&new_socket), Some(&item));
        assert_eq!(
            game, before,
            "candidate validation does not mutate the live actor"
        );
    }

    #[test]
    fn same_anchor_body_changes_reconcile_new_and_removed_trap_contacts() {
        let mut game = game();
        let actor = ActorId(1);
        let at = game.actors[&actor].location;
        let extra = tor_world::Location {
            region: at.region,
            position: tor_world::Position {
                x: at.position.x,
                y: at.position.y + 1,
                z: at.position.z,
            },
        };
        game.place_trap(
            extra,
            tor_world::Trap {
                id: 1,
                dc: 99,
                avoidance_dc: None,
                kind: tor_world::TrapKind::Mysterious {},
            },
        )
        .unwrap();
        assert!(game.actors[&actor].trap_contacts.ids().next().is_none());
        let original_body = game.actors[&actor].body.clone();
        let mut expanded = (*original_body).clone();
        expanded.cells.push([0, 1, 0]);
        assert!(game.body_fits(actor, at, game.actors[&actor].orientation, &expanded));
        let mut outcome = game.settled_actor_outcome(actor).unwrap();
        outcome.state.body = tor_world::Shared::new(expanded);
        game.commit_actor_outcome(outcome).unwrap();
        assert_eq!(game.actors[&actor].location, at);
        assert_eq!(
            game.actors[&actor].trap_contacts.ids().collect::<Vec<_>>(),
            vec![1]
        );
        assert!(game.feature_family_known(actor, tor_world::ConcealedFeatureId::Trap(1)));
        let mut outcome = game.settled_actor_outcome(actor).unwrap();
        outcome.state.body = original_body;
        game.commit_actor_outcome(outcome).unwrap();
        assert!(game.actors[&actor].trap_contacts.ids().next().is_none());
        assert_eq!(game.actors[&actor].location, at);
    }

    fn prepared_game_with_token(token: bool) -> Game {
        let mut game = game();
        if token {
            game.apply_condition(
                ActorId(1),
                &crate::content_ids::ConditionId::new("readied").unwrap(),
                ActorId(2),
                1000,
            )
            .unwrap();
        }
        let plan = game
            .ability_plan(
                ActorId(1),
                &AbilityId::new("borrowed_strike").unwrap(),
                ActorId(2),
            )
            .unwrap();
        assert_eq!(plan.tokens().len(), usize::from(token));
        game.start_work(plan, None).unwrap();
        game
    }

    #[test]
    fn token_free_outcomes_skip_retirement_plan_comparisons() {
        let prepared = prepared_game_with_token(false);
        let actor = ActorId(1);
        let mut comparisons = Vec::new();
        for change in ["unchanged", "suspended", "replaced", "completed"] {
            let mut game = prepared.clone();
            let mut outcome = game.actor_outcome(actor).unwrap();
            match change {
                "suspended" => {
                    outcome
                        .interrupt(super::Interruption::Suspend, game.actor_clock(actor))
                        .unwrap();
                }
                "replaced" => {
                    let crate::abilities::Plan::Ability(plan) =
                        &mut outcome.state.pending.as_mut().unwrap().plan
                    else {
                        panic!("expected an ability plan");
                    };
                    // A different frozen operand, with the same kind of work.
                    plan.strikes[0].check.as_mut().unwrap().edge += 1;
                    assert!(plan.valid(&game.content.rules));
                }
                "completed" => outcome.state.pending = None,
                _ => {}
            }
            let before = super::TOKEN_PLAN_COMPARISONS.with(std::cell::Cell::get);
            game.apply_outcome(outcome).unwrap();
            let count = super::TOKEN_PLAN_COMPARISONS.with(std::cell::Cell::get) - before;
            comparisons.push((change, count));
            assert!(game.token_reservations_valid(actor, false));
        }
        assert_eq!(
            comparisons,
            [
                ("unchanged", 0),
                ("suspended", 0),
                ("replaced", 0),
                ("completed", 0)
            ]
        );
    }

    #[test]
    fn reserved_token_outcomes_compare_plans_and_retire_only_replacements() {
        let mut game = prepared_game_with_token(true);
        let actor = ActorId(1);
        let original = game.preparation(actor).unwrap().plan.clone();
        let mut suspended = game.actor_outcome(actor).unwrap();
        suspended
            .interrupt(super::Interruption::Suspend, game.actor_clock(actor))
            .unwrap();
        let before = super::TOKEN_PLAN_COMPARISONS.with(std::cell::Cell::get);
        game.apply_outcome(suspended).unwrap();
        assert_eq!(
            super::TOKEN_PLAN_COMPARISONS.with(std::cell::Cell::get) - before,
            1
        );
        assert_eq!(game.preparation(actor).unwrap().plan, original);
        assert!(!game.preparation(actor).unwrap().active);
        assert_eq!(
            game.creature(actor)
                .unwrap()
                .conditions()
                .instances()
                .next()
                .unwrap()
                .reserved_by,
            Some(actor)
        );
        assert!(game.token_reservations_valid(actor, false));

        let replacement = game
            .ability_plan(
                actor,
                &AbilityId::new("borrowed_strike").unwrap(),
                ActorId(2),
            )
            .unwrap();
        assert_eq!(replacement.work(), original.work());
        assert_ne!(replacement, original);
        assert!(replacement.tokens().len() == 0);
        let mut replaced = game.actor_outcome(actor).unwrap();
        replaced.state.pending.as_mut().unwrap().plan = replacement.clone();
        let before = super::TOKEN_PLAN_COMPARISONS.with(std::cell::Cell::get);
        game.apply_outcome(replaced).unwrap();
        assert_eq!(
            super::TOKEN_PLAN_COMPARISONS.with(std::cell::Cell::get) - before,
            1
        );
        assert_eq!(game.preparation(actor).unwrap().plan, replacement);
        assert!(game.creature(actor).unwrap().conditions().is_empty());
        assert!(game.token_reservations_valid(actor, false));
    }

    #[test]
    fn rebuild_losing_a_free_ability_cancels_its_captured_preparation() {
        let mut game = game();
        let actor = ActorId(1);
        game.act(
            actor,
            Action::UseAbility {
                id: AbilityId::new("borrowed_strike").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(2)),
                supplies: Default::default(),
            },
        )
        .unwrap();
        assert!(game.preparation(actor).unwrap().hold.is_none());
        let mut build = game.creature(actor).unwrap().build().clone();
        build.set_templates(vec![]).unwrap();
        game.rebuild_creature(actor, build).unwrap();
        assert!(game.preparation(actor).is_none());
        assert!(matches!(
            game.combat_events().last(),
            Some(crate::combat::CombatEvent::Interrupted {
                actor: ActorId(1),
                ..
            })
        ));
    }

    #[test]
    fn expired_free_grant_interrupts_before_preparation_completion() {
        let mut game = game();
        let actor = ActorId(1);
        let mut build = game.creature(actor).unwrap().build().clone();
        build.set_templates(vec![]).unwrap();
        game.rebuild_creature(actor, build).unwrap();
        game.apply_condition(
            actor,
            &crate::content_ids::ConditionId::new("borrowed_grant").unwrap(),
            ActorId(2),
            10,
        )
        .unwrap();
        game.act(
            actor,
            Action::UseAbility {
                id: AbilityId::new("borrowed_strike").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(2)),
                supplies: Default::default(),
            },
        )
        .unwrap();
        assert!(game.preparation(actor).is_some());
        game.tick = 10;
        game.settle_creature_time().unwrap();
        assert!(game.preparation(actor).is_none());
        assert_eq!(
            game.combat_events()
                .iter()
                .filter(|event| matches!(
                    event,
                    CombatEvent::Interrupted {
                        actor: ActorId(1),
                        ..
                    }
                ))
                .count(),
            1
        );
        assert!(!game
            .combat_events()
            .iter()
            .any(|event| matches!(event, CombatEvent::Resolved { .. })));
    }

    #[test]
    fn lethal_paid_work_emits_one_interruption_and_one_death() {
        let mut game = game();
        let actor = ActorId(1);
        let intention = game
            .admit_intention(
                actor,
                Action::UseAbility {
                    id: AbilityId::new("fixture_paid_melee").unwrap(),
                    invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(2)),
                    supplies: Default::default(),
                },
                crate::IntentionOrigin::Human,
            )
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        assert_eq!(game.preparation(actor).unwrap().hold, Some(intention));
        crate::test_creatures::injure(&mut game, actor, 100).unwrap();
        assert!(!game.alive(actor));
        assert!(game.preparation(actor).is_none());
        assert!(game
            .creature(actor)
            .unwrap()
            .costs()
            .reservations()
            .is_empty());
        assert_eq!(
            game.combat_events()
                .iter()
                .filter(|event| matches!(
                    event,
                    CombatEvent::Interrupted {
                        actor: ActorId(1),
                        ..
                    }
                ))
                .count(),
            1
        );
        assert_eq!(
            game.combat_events()
                .iter()
                .filter(|event| matches!(
                    event,
                    CombatEvent::Died {
                        actor: ActorId(1),
                        cause: crate::combat::DeathCause::Injury,
                    }
                ))
                .count(),
            1
        );
    }

    #[test]
    fn rejected_condition_does_not_commit_elapsed_timers_or_events() {
        let mut game = game();
        let actor = ActorId(1);
        let fear = crate::content_ids::ConditionId::new("fear").unwrap();
        game.apply_condition(actor, &fear, ActorId(2), 100).unwrap();
        game.tick = 10;
        let before = game.clone();
        assert_eq!(
            game.apply_condition(actor, &fear, ActorId(999), 20),
            Err(crate::GameError::UnknownActor)
        );
        assert!(
            game == before,
            "rejected edit must preserve the complete game"
        );
    }

    fn participation_outcome(ability: bool) -> crate::abilities::Outcome {
        crate::abilities::Outcome {
            resistance: None,
            actor: ActorId(1),
            target: ActorId(2),
            ability: ability.then(|| AbilityId::new("basic_melee").unwrap()),
            intention: None,
            result: crate::abilities::OutcomeStatus::Unaffected,
            check: None,
            participation: crate::abilities::AttemptedParticipation::target(),
            target_rider_participated: false,
            effects: vec![crate::abilities::ResolvedEffect::Heal { amount: 0 }],
        }
    }
    #[test]
    fn public_plan_rider_review_receipts_bind_primary_strike_and_participants() {
        use super::{DamageActivityGuard, ResolutionPublication};
        let mut game = game();
        let mut activity = DamageActivityGuard::default();
        let mut source = participation_outcome(true);
        source.participation = crate::abilities::AttemptedParticipation::source();
        source.effects.clear();
        let first = game
            .apply_resolved_outcome(
                source.clone(),
                ResolutionPublication::Primary { strike: 0 },
                &mut activity,
            )
            .unwrap();
        let second = game
            .apply_resolved_outcome(
                source.clone(),
                ResolutionPublication::Primary { strike: 1 },
                &mut activity,
            )
            .unwrap();
        let repeated_ordinal = game
            .apply_resolved_outcome(
                source,
                ResolutionPublication::Primary { strike: 0 },
                &mut activity,
            )
            .unwrap();
        let rider = game
            .apply_resolved_outcome(
                participation_outcome(false),
                ResolutionPublication::Rider {
                    primary: first,
                    strike: 0,
                    ordinal: 0,
                },
                &mut activity,
            )
            .unwrap();
        for parent in [second, repeated_ordinal, rider] {
            let before = game.clone();
            assert!(game
                .merge_committed_rider_participation(parent, rider)
                .is_err());
            assert_eq!(game, before, "a crossed receipt cannot change any event");
        }
        for variant in 0..4 {
            let mut input = participation_outcome(false);
            let strike = if variant == 0 { 1 } else { 0 };
            match variant {
                1 => input.actor = ActorId(2),
                2 => input.target = ActorId(1),
                3 => input.intention = Some(crate::IntentionId(999)),
                _ => {}
            }
            let before = game.clone();
            assert!(game
                .apply_resolved_outcome(
                    input,
                    ResolutionPublication::Rider {
                        primary: first,
                        strike,
                        ordinal: 0
                    },
                    &mut activity
                )
                .is_err());
            assert_eq!(
                game, before,
                "invalid rider context rejects before publication"
            );
        }
        let before = game.published_resolution(first).unwrap().clone();
        assert!(game
            .merge_committed_rider_participation(first, rider)
            .unwrap());
        let mut expected = before;
        expected.target_rider_participated = true;
        assert_eq!(
            game.published_resolution(first).unwrap(),
            &expected,
            "only the summary scalar changes"
        );
        assert!(
            !game
                .published_resolution(second)
                .unwrap()
                .target_rider_participated
        );
        assert!(
            !game
                .published_resolution(repeated_ordinal)
                .unwrap()
                .target_rider_participated
        );
        assert!(game.published_resolution(rider).unwrap().ability.is_none());
    }
    #[test]
    fn public_plan_rider_review_blocked_primary_and_prefilled_summary_reject() {
        use super::{DamageActivityGuard, ResolutionPublication};
        use crate::abilities::{
            AttemptedParticipation, CheckParticipation, OutcomeStatus, PayloadParticipation,
            UnselectedReason,
        };
        let mut game = game();
        let mut activity = DamageActivityGuard::default();
        let mut prefilled = participation_outcome(true);
        prefilled.target_rider_participated = true;
        let before = game.clone();
        assert!(game.apply(prefilled).is_err());
        assert_eq!(game, before);
        for reason in [
            UnselectedReason::Dead,
            UnselectedReason::UnavailableTarget,
            UnselectedReason::Immune,
        ] {
            let mut blocked = participation_outcome(true);
            blocked.effects.clear();
            blocked.result = if reason == UnselectedReason::UnavailableTarget {
                OutcomeStatus::Miss
            } else {
                OutcomeStatus::Unaffected
            };
            blocked.participation = AttemptedParticipation {
                check: if reason == UnselectedReason::Immune {
                    CheckParticipation::TargetInvolved
                } else {
                    CheckParticipation::None
                },
                payload: PayloadParticipation::Unselected { reason },
            };
            let primary = game
                .apply_resolved_outcome(
                    blocked,
                    ResolutionPublication::Primary { strike: 0 },
                    &mut activity,
                )
                .unwrap();
            let before = game.clone();
            assert!(game
                .apply_resolved_outcome(
                    participation_outcome(false),
                    ResolutionPublication::Rider {
                        primary,
                        strike: 0,
                        ordinal: 0
                    },
                    &mut activity
                )
                .is_err());
            assert_eq!(
                game, before,
                "an early-blocked primary cannot publish a rider"
            );
        }
    }
}

#[cfg(test)]
#[path = "lifecycle_growth_tests.rs"]
mod growth_tests;
