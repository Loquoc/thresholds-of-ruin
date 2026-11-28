//! Simulation-owned admitted work. Admission is distinct from execution, and
//! execution rebuilds transient preparation against the current authoritative state.
use crate::{Action, ActionOutcome, ActorId, Game, GameError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_QUEUED_INTENTIONS: usize = 4096;

pub(crate) mod checkpoint;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IntentionId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentionOrigin {
    Human,
    Autonomous,
    Travel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentionState {
    Queued,
    Suspended,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "action",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum IntentionWork {
    Action(Action),
    AiDecision,
    ResumePreparation { work: crate::Work },
}

/// Region-local movement meaning at admission, including portal frame changes.
/// This is a guard, not a prepared action or cached permission to move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MovementContext {
    from: tor_world::Location,
    orientation: u8,
    to: tor_world::Location,
    next_orientation: u8,
}
impl<'de> Deserialize<'de> for MovementContext {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::location")]
            from: tor_world::Location,
            orientation: u8,
            #[serde(deserialize_with = "crate::bounded::location")]
            to: tor_world::Location,
            next_orientation: u8,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        let value = Self {
            from: fields.from,
            orientation: fields.orientation,
            to: fields.to,
            next_orientation: fields.next_orientation,
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid movement context"));
        }
        Ok(value)
    }
}

impl MovementContext {
    pub(crate) fn valid(&self) -> bool {
        self.from.region.0 != 0
            && self.to.region.0 != 0
            && self.orientation < 24
            && self.next_orientation < 24
    }
    /// Backend audit checks compare the saved region-local destination.
    pub fn destination(&self) -> tor_world::Location {
        self.to
    }
    pub(crate) fn destination_orientation(&self) -> u8 {
        self.next_orientation
    }

    pub(crate) fn matches(
        &self,
        from: tor_world::Location,
        orientation: u8,
        to: tor_world::Location,
        next_orientation: u8,
    ) -> bool {
        (self.from, self.orientation, self.to, self.next_orientation)
            == (from, orientation, to, next_orientation)
    }
}

/// An intentional terminal action has an endpoint, never a fabricated
/// movement destination. It is rechecked at execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalExitContext {
    at: tor_world::Location,
    direction: tor_world::Direction,
}
impl<'de> Deserialize<'de> for TerminalExitContext {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::location")]
            at: tor_world::Location,
            direction: tor_world::Direction,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        if fields.at.region.0 == 0 || fields.direction != tor_world::Direction::Up {
            return Err(serde::de::Error::custom("invalid terminal exit context"));
        }
        Ok(Self {
            at: fields.at,
            direction: fields.direction,
        })
    }
}
impl TerminalExitContext {
    fn policy_matches(self, game: &Game, actor: ActorId, action: &Action) -> bool {
        matches!(action, Action::Move(direction) if *direction == self.direction)
            && game.is_terminal_exit(actor, self.at, self.direction)
    }
    fn live_matches(self, game: &Game, actor: ActorId, action: &Action) -> bool {
        self.policy_matches(game, actor, action)
            && game
                .actors
                .get(&actor)
                .is_some_and(|state| state.location == self.at)
    }
}

fn deserialize_movement_context<'de, D>(
    deserializer: D,
) -> Result<Option<MovementContext>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<MovementContext>::deserialize(deserializer)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedIntention {
    pub id: IntentionId,
    pub actor: ActorId,
    pub work: IntentionWork,
    pub origin: IntentionOrigin,
    pub state: IntentionState,
    #[serde(deserialize_with = "deserialize_movement_context")]
    pub movement_context: Option<MovementContext>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub terminal_exit_context: Option<TerminalExitContext>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub human_operands: Option<crate::HumanOperandBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntentionExecution {
    pub intention: QueuedIntention,
    /// A concrete action is selected only at execution for deferred AI work.
    pub action: Option<Action>,
    pub outcome: Result<ActionOutcome, GameError>,
}

/// Derived queue capacity and existing-work controls. Backend-only identities
/// and availability are never authority to act: attachment, actor validity,
/// targets and timing must still be checked at the appropriate boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentionInput {
    pub slot_available: bool,
    controls: [Option<IntentionControl>; 2],
}

impl IntentionInput {
    /// Queue first, then independent preparation. No intermediate list allocates.
    pub fn controls(&self) -> impl Iterator<Item = IntentionControl> + '_ {
        self.controls.iter().flatten().copied()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentionControlState {
    Queued,
    Suspended,
    Started,
    Paused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentionControl {
    pub intention: IntentionId,
    pub state: IntentionControlState,
    pub can_resume: bool,
    pub can_cancel: bool,
}

/// The validated resume operation determines its mutation without rechecking
/// a second policy. Existing queue work never allocates another identity.
enum ResumeIntention {
    Queue,
    Preparation { work: crate::Work },
}

enum CancelIntention {
    Queue,
    Preparation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IntentionQueue {
    next_id: u64,
    entries: BTreeMap<ActorId, QueuedIntention>,
}

impl Default for IntentionQueue {
    fn default() -> Self {
        Self {
            next_id: 1,
            entries: BTreeMap::new(),
        }
    }
}

impl Game {
    /// Validate currently available work without moving time or applying effects.
    /// A later execution can still fail if the actor, target or topology changes.
    pub fn admit_intention(
        &mut self,
        actor: ActorId,
        action: Action,
        origin: IntentionOrigin,
    ) -> Result<IntentionId, GameError> {
        self.admit_action_intention(actor, action, origin, None)
    }

    /// Admit one planned region-local step without moving time or applying effects.
    /// The ordinary movement context also protects its portal frame at execution.
    pub fn admit_travel_intention(
        &mut self,
        actor: ActorId,
        step: crate::TravelStep,
    ) -> Result<IntentionId, GameError> {
        self.admit_action_intention(
            actor,
            Action::Move(step.direction),
            IntentionOrigin::Travel,
            Some(step.destination),
        )
    }

    fn admit_action_intention(
        &mut self,
        actor: ActorId,
        action: Action,
        origin: IntentionOrigin,
        destination: Option<tor_world::Location>,
    ) -> Result<IntentionId, GameError> {
        let next = self.check_intention_slot(actor)?;
        if origin == IntentionOrigin::Travel && !matches!(action, Action::Move(_)) {
            return Err(GameError::Schedule(crate::ScheduleError::InvalidIntention));
        }
        let human_operands = if origin == IntentionOrigin::Human {
            Some(self.prepare_human_intention(actor, action.clone())?)
        } else {
            self.validate_intention_action(actor, action.clone())?;
            None
        };
        let movement_context = if self.terminal_exit_context(actor, &action).is_some() {
            None
        } else {
            action.selected_step().and_then(|direction| {
                if let Some((to, next_orientation)) = matches!(action, Action::Move(_))
                    .then(|| self.abstract_stair_destination(actor, direction))
                    .flatten()
                {
                    let state = &self.actors[&actor];
                    return Some(MovementContext {
                        from: state.location,
                        orientation: state.orientation,
                        to,
                        next_orientation,
                    });
                }
                self.movement_context(actor, direction)
            })
        };
        if destination.is_some_and(|destination| {
            movement_context.is_none_or(|context| context.to != destination)
        }) {
            return Err(GameError::InvalidLocation);
        }
        Ok(self.insert_intention(
            actor,
            IntentionWork::Action(action),
            origin,
            next,
            movement_context,
            human_operands,
        ))
    }

    /// Queue the decision itself; no observation, search or RNG work runs here.
    pub fn admit_ai_intention(&mut self, actor: ActorId) -> Result<IntentionId, GameError> {
        let next = self.check_intention_slot(actor)?;
        if self.run_outcome().terminal
            || !self.is_ai(actor)
            || self.health(actor).is_none_or(|(hp, _)| hp == 0)
        {
            return Err(GameError::UnknownActor);
        }
        Ok(self.insert_intention(
            actor,
            IntentionWork::AiDecision,
            IntentionOrigin::Autonomous,
            next,
            None,
            None,
        ))
    }

    /// Observe input-related queue state without preparing an action, observing
    /// the world, allocating an identity, changing time or consuming randomness.
    /// Slot availability is the shared capacity/identity check, not a claim that
    /// any particular actor, action or target is valid.
    pub fn intention_input(&self, actor: ActorId) -> IntentionInput {
        let queued = self.pending_intention(actor);
        let queue_control = queued.map(|queued| {
            self.intention_control(
                actor,
                queued.id,
                match queued.state {
                    IntentionState::Queued => IntentionControlState::Queued,
                    IntentionState::Suspended => IntentionControlState::Suspended,
                },
            )
        });
        let preparation_control = self.preparation(actor).and_then(|preparation| {
            let id = preparation
                .intention
                .filter(|id| queued.is_none_or(|queued| queued.id != *id))?;
            Some(self.intention_control(
                actor,
                id,
                if preparation.active {
                    IntentionControlState::Started
                } else {
                    IntentionControlState::Paused
                },
            ))
        });
        IntentionInput {
            slot_available: self.check_intention_slot(actor).is_ok(),
            controls: [queue_control, preparation_control],
        }
    }

    fn intention_control(
        &self,
        actor: ActorId,
        intention: IntentionId,
        state: IntentionControlState,
    ) -> IntentionControl {
        IntentionControl {
            intention,
            state,
            can_resume: self.validate_resume_intention(actor, intention).is_ok(),
            can_cancel: self.validate_cancel_intention(actor, intention).is_ok(),
        }
    }

    fn check_intention_slot(&self, actor: ActorId) -> Result<u64, GameError> {
        if self.intentions.entries.contains_key(&actor) {
            return Err(GameError::ActorBusy);
        }
        if self.intentions.entries.len() >= MAX_QUEUED_INTENTIONS {
            return Err(GameError::Schedule(crate::ScheduleError::QueueFull));
        }
        self.intentions
            .next_id
            .checked_add(1)
            .ok_or(GameError::IdentityExhausted)
    }

    fn insert_intention(
        &mut self,
        actor: ActorId,
        work: IntentionWork,
        origin: IntentionOrigin,
        next: u64,
        movement_context: Option<MovementContext>,
        human_operands: Option<crate::HumanOperandBinding>,
    ) -> IntentionId {
        let id = IntentionId(self.intentions.next_id);
        let terminal_exit_context = match &work {
            IntentionWork::Action(action) => self.terminal_exit_context(actor, action),
            _ => None,
        };
        self.intentions.entries.insert(
            actor,
            QueuedIntention {
                id,
                actor,
                work,
                origin,
                state: IntentionState::Queued,
                movement_context,
                terminal_exit_context,
                human_operands,
            },
        );
        self.intentions.next_id = next;
        id
    }

    pub fn pending_intention(&self, actor: ActorId) -> Option<&QueuedIntention> {
        self.intentions.entries.get(&actor)
    }

    fn terminal_exit_context(
        &self,
        actor: ActorId,
        action: &Action,
    ) -> Option<TerminalExitContext> {
        let Action::Move(direction) = action else {
            return None;
        };
        if self.bear_trap_struggle(actor) {
            return None;
        }
        self.prepare_terminal_escape(actor, *direction).ok()?;
        Some(TerminalExitContext {
            at: self.actors.get(&actor)?.location,
            direction: *direction,
        })
    }

    pub(crate) fn movement_context(
        &self,
        actor: ActorId,
        direction: tor_world::Direction,
    ) -> Option<MovementContext> {
        let state = self.actors.get(&actor)?;
        let (to, next_orientation) = if self.bear_trap_struggle(actor) {
            (state.location, state.orientation)
        } else {
            self.actor_translation(actor, direction).or_else(|| {
                self.movement_guise_contacts(actor, direction)
                    .map(|(goal, _)| goal)
            })?
        };
        Some(MovementContext {
            from: state.location,
            orientation: state.orientation,
            to,
            next_orientation,
        })
    }

    /// Backend recovery inspects only the bounded admitted work, including
    /// unavailable actors whose work still requires a terminal scheduler result.
    pub fn queued_intentions(&self) -> impl Iterator<Item = &QueuedIntention> {
        self.intentions.entries.values()
    }

    fn intention_actor_unavailable(&self, actor: ActorId) -> bool {
        self.known_actor_region(actor).is_none()
            || self.actors.get(&actor).is_some_and(|actor| !actor.alive())
    }

    fn selected_intention(&self) -> Option<&QueuedIntention> {
        if !self.arena_execution_enabled() {
            return None;
        }
        // Unavailable actors cannot become due. Resolve their work in stable
        // admission order, while preserving unloaded actors in the identity directory.
        let unavailable = self
            .intentions
            .entries
            .values()
            .filter(|entry| self.intention_actor_unavailable(entry.actor))
            .min_by_key(|entry| entry.id);
        unavailable.or_else(|| {
            self.intentions
                .entries
                .get(&self.next_actor()?)
                .filter(|entry| entry.state == IntentionState::Queued)
        })
    }

    /// Host scheduling uses the same selection as execution, including terminal
    /// work whose actor can never become due. Selection has no simulation effects.
    pub fn next_intention_actor(&self) -> Option<ActorId> {
        self.selected_intention().map(|entry| entry.actor)
    }

    /// Preserve the actor scheduler's ordering, including waiting human boundaries.
    /// A failed execution consumes the intention but never substitutes another target.
    pub fn execute_next_intention(&mut self) -> Option<IntentionExecution> {
        self.execute_next_intention_with_stair_fallback(true)
    }

    pub(crate) fn execute_next_intention_with_stair_fallback(
        &mut self,
        allow_stair_fallback: bool,
    ) -> Option<IntentionExecution> {
        let intention = self.selected_intention()?.clone();
        let actor = intention.actor;
        if self.intention_actor_unavailable(actor) {
            self.intentions.entries.remove(&intention.actor);
            let action = match intention.work.clone() {
                IntentionWork::Action(action) => Some(action),
                IntentionWork::AiDecision => None,
                IntentionWork::ResumePreparation { work } => Some(work.action()),
            };
            return Some(IntentionExecution {
                intention,
                action,
                outcome: Err(GameError::UnknownActor),
            });
        }
        let (action, outcome) = match intention.work.clone() {
            IntentionWork::Action(action) => (
                Some(action.clone()),
                if intention.terminal_exit_context.is_some_and(|context| {
                    intention.movement_context.is_some()
                        || !context.live_matches(self, actor, &action)
                }) || (action.selected_step().is_some()
                    && intention.movement_context.is_none()
                    && intention.terminal_exit_context.is_none())
                {
                    Err(GameError::InvalidLocation)
                } else {
                    self.act_with_context(
                        actor,
                        action,
                        Some(intention.id),
                        intention.movement_context,
                        allow_stair_fallback,
                    )
                },
            ),
            IntentionWork::AiDecision => match self.act_ai_with_intention(actor, intention.id) {
                Ok((action, outcome)) => (Some(action), Ok(outcome)),
                Err(error) => (None, Err(error)),
            },
            IntentionWork::ResumePreparation { work } => {
                let action = work.action();
                let mut outcome = self.resume_preparation_with_intention(actor, work, intention.id);
                if outcome.is_err() {
                    if let Err(error) = self.discard_intention_preparation(actor, intention.id) {
                        outcome = Err(error);
                    }
                }
                (Some(action), outcome)
            }
        };
        self.intentions.entries.remove(&actor);
        Some(IntentionExecution {
            intention,
            action,
            outcome,
        })
    }

    fn validate_cancel_intention(
        &self,
        actor: ActorId,
        id: IntentionId,
    ) -> Result<CancelIntention, GameError> {
        if self
            .preparation(actor)
            .is_some_and(|p| p.intention == Some(id) && p.plan.work().item().is_some())
        {
            return Err(GameError::Schedule(crate::ScheduleError::InvalidIntention));
        }
        if self
            .pending_intention(actor)
            .is_some_and(|queued| queued.id == id)
        {
            Ok(CancelIntention::Queue)
        } else if self
            .preparation(actor)
            .is_some_and(|preparation| preparation.intention == Some(id))
        {
            Ok(CancelIntention::Preparation)
        } else {
            Err(GameError::Schedule(crate::ScheduleError::InvalidIntention))
        }
    }

    pub fn cancel_intention(&mut self, actor: ActorId, id: IntentionId) -> Result<(), GameError> {
        self.cancel_intention_with_step(actor, id, None)
    }

    /// Cancel the exact admitted work and optionally make one ordinary step.
    /// Everything, including queue retirement, token consumption and RNG, commits
    /// together only after movement and schedule admission succeed.
    pub fn cancel_intention_with_step(
        &mut self,
        actor: ActorId,
        id: IntentionId,
        step: Option<tor_world::Direction>,
    ) -> Result<(), GameError> {
        let kind = self.validate_cancel_intention(actor, id)?;
        if let Some(direction) = step {
            return self
                .cancel_preparation_with_step(actor, Some(id), direction, None, None)
                .map(|_| ());
        }
        let mut candidate = self.clone();
        if matches!(kind, CancelIntention::Queue) {
            candidate.intentions.entries.remove(&actor);
        }
        candidate.discard_intention_preparation(actor, id)?;
        candidate.seal_participant_events()?;
        *self = candidate;
        Ok(())
    }

    /// One atomic cancellation-and-movement boundary for human and AI actors.
    /// `expected` names the old preparation; a new AI decision keeps its own queue.
    pub(crate) fn cancel_preparation_with_step(
        &mut self,
        actor: ActorId,
        expected: Option<IntentionId>,
        direction: tor_world::Direction,
        movement_context: Option<MovementContext>,
        ai: Option<(crate::ai::Ai, crate::behavior::Behavior)>,
    ) -> Result<crate::ActionOutcome, GameError> {
        if self.actor_frozen(actor)
            || !self.preparation(actor).is_some_and(|pending| {
                pending.intention == expected && pending.plan.ability_id().is_some()
            })
        {
            return Err(GameError::Schedule(crate::ScheduleError::InvalidIntention));
        }
        let mut candidate = self.clone();
        candidate.begin_action_boundary(actor)?;
        if candidate
            .pending_intention(actor)
            .is_some_and(|queued| Some(queued.id) == expected)
        {
            candidate.intentions.entries.remove(&actor);
        }
        let mut discarded = candidate.actor_outcome(actor)?;
        discarded.discard(candidate.actor_clock(actor))?;
        candidate.apply_outcome(discarded)?;
        if let Some((ai, behavior)) = ai {
            candidate.combat.ai.insert(actor, ai);
            candidate.install_behavior(actor, behavior)?;
            candidate.reconcile_recovery_contexts()?;
        }
        let outcome = candidate.escape_step_in_transaction(actor, direction, movement_context)?;
        candidate.seal_participant_events()?;
        *self = candidate;
        Ok(outcome)
    }

    fn discard_intention_preparation(
        &mut self,
        actor: ActorId,
        id: IntentionId,
    ) -> Result<bool, GameError> {
        // Queued work can belong to a detached actor without any preparation.
        // Retire that queue without loading or deriving an actor outcome. Escape
        // steps use their separate ordinary loaded-movement admission above.
        if !self
            .preparation(actor)
            .is_some_and(|pending| pending.intention == Some(id))
        {
            return Ok(false);
        }
        let mut outcome = self.actor_outcome(actor)?;
        outcome.discard(self.actor_clock(actor))?;
        self.apply_outcome(outcome)?;
        Ok(true)
    }

    pub fn suspend_human_intentions(&mut self) {
        for intention in self.intentions.entries.values_mut() {
            if intention.origin == IntentionOrigin::Human {
                intention.state = IntentionState::Suspended;
            }
        }
    }

    pub fn suspend_intention(&mut self, actor: ActorId, id: IntentionId) -> Result<(), GameError> {
        if !self.is_ai(actor)
            && self
                .preparation(actor)
                .is_some_and(|p| p.intention == Some(id) && p.active)
        {
            self.pause_preparation(actor)
                .ok_or(GameError::Schedule(crate::ScheduleError::InvalidIntention))?;
            return Ok(());
        }
        if self.pending_intention(actor).is_none_or(|entry| {
            entry.id != id
                || entry.origin != IntentionOrigin::Human
                || entry.state != IntentionState::Queued
        }) {
            return Err(GameError::Schedule(crate::ScheduleError::InvalidIntention));
        }
        self.intentions
            .entries
            .get_mut(&actor)
            .expect("checked intention")
            .state = IntentionState::Suspended;
        Ok(())
    }

    fn validate_resume_intention(
        &self,
        actor: ActorId,
        id: IntentionId,
    ) -> Result<ResumeIntention, GameError> {
        if !self.condition_allows_voluntary_input(actor) {
            return Err(GameError::ActorBusy);
        }
        if let Some(queued) = self.pending_intention(actor) {
            if queued.id == id
                && queued.origin == IntentionOrigin::Human
                && queued.state == IntentionState::Suspended
            {
                let action = match &queued.work {
                    IntentionWork::Action(action) => Some(action.clone()),
                    IntentionWork::ResumePreparation { work } => Some(work.action()),
                    IntentionWork::AiDecision => None,
                };
                if action.is_some_and(|action| self.condition_blocks_action(actor, &action)) {
                    return Err(GameError::ActorBusy);
                }
                return Ok(ResumeIntention::Queue);
            }
            return Err(GameError::Schedule(crate::ScheduleError::InvalidIntention));
        }
        let preparation = self
            .preparation(actor)
            .ok_or(GameError::Schedule(crate::ScheduleError::InvalidIntention))?;
        if preparation.intention != Some(id) || preparation.active || self.is_ai(actor) {
            return Err(GameError::Schedule(crate::ScheduleError::InvalidIntention));
        }
        if self.condition_blocks_action(actor, &preparation.plan.work().action()) {
            return Err(GameError::ActorBusy);
        }
        if self.intentions.entries.len() >= MAX_QUEUED_INTENTIONS {
            return Err(GameError::Schedule(crate::ScheduleError::QueueFull));
        }
        Ok(ResumeIntention::Preparation {
            work: preparation.plan.work(),
        })
    }

    pub fn resume_intention(&mut self, actor: ActorId, id: IntentionId) -> Result<(), GameError> {
        match self.validate_resume_intention(actor, id)? {
            ResumeIntention::Queue => {
                self.intentions
                    .entries
                    .get_mut(&actor)
                    .expect("checked intention")
                    .state = IntentionState::Queued;
            }
            ResumeIntention::Preparation { work } => {
                self.intentions.entries.insert(
                    actor,
                    QueuedIntention {
                        id,
                        actor,
                        work: IntentionWork::ResumePreparation { work },
                        origin: IntentionOrigin::Human,
                        state: IntentionState::Queued,
                        movement_context: None,
                        terminal_exit_context: None,
                        human_operands: None,
                    },
                );
            }
        }
        Ok(())
    }

    /// An ordinary matching Action also resumes the captured plan, potentially
    /// under a newer queue identity. Deferred AI decisions and genuinely fresh
    /// actions have not selected that existing work and remain ordinary queued work.
    fn condition_pauses_queued_continuation(&self, queued: &QueuedIntention) -> bool {
        self.actors.get(&queued.actor).is_some_and(|state| {
            let Some(pending) = &state.pending else {
                return false;
            };
            let work = pending.plan.work();
            let continues = match &queued.work {
                IntentionWork::ResumePreparation { work: selected } => {
                    pending.intention == Some(queued.id) && work == *selected
                }
                IntentionWork::Action(action) => work.action() == *action,
                IntentionWork::AiDecision => false,
            };
            continues
                && state.combat.as_ref().is_some_and(|combat| {
                    combat
                        .creature()
                        .condition_pauses_preparation(&work.action())
                })
        })
    }

    /// Previously queued continuations cannot silently thaw work. Preserve both
    /// queue and paid-plan identities until a fresh ordinary Resume control.
    pub(crate) fn suspend_condition_denied_continuation(&mut self, actor: ActorId) {
        let suspend = self.pending_intention(actor).is_some_and(|queued| {
            queued.state == IntentionState::Queued
                && self.condition_pauses_queued_continuation(queued)
        });
        if suspend {
            self.intentions
                .entries
                .get_mut(&actor)
                .expect("matched queued resume")
                .state = IntentionState::Suspended;
        }
    }

    pub fn continue_intention_ids(&mut self, later: &Game) {
        self.intentions.next_id = self.intentions.next_id.max(later.intentions.next_id);
    }

    pub(crate) fn intentions_valid(&self) -> bool {
        let mut ids = BTreeSet::new();
        let owners: BTreeMap<_, _> =
            self.intentions
                .entries
                .values()
                .map(|entry| (entry.id, entry.actor))
                .chain(self.actors.iter().filter_map(|(&actor, state)| {
                    Some((state.pending.as_ref()?.intention?, actor))
                }))
                .collect();
        let mut origins = BTreeSet::new();
        self.intentions.next_id != 0
            && self.intentions.entries.len() <= MAX_QUEUED_INTENTIONS
            && self.intentions.entries.iter().all(|(actor, entry)| {
                actor.0 != 0
                    && *actor == entry.actor
                    && entry.id.0 != 0
                    && entry.id.0 < self.intentions.next_id
                    && ids.insert(entry.id)
                    && match (&entry.work, entry.origin, &entry.human_operands) {
                        (IntentionWork::Action(action), IntentionOrigin::Human, Some(binding)) =>
                            self.human_operands_checkpoint_valid(binding, action),
                        (IntentionWork::Action(_), IntentionOrigin::Human, None) => false,
                        (_, _, None) => true,
                        _ => false,
                    }
                    && (entry.state != IntentionState::Queued
                        || !self.condition_pauses_queued_continuation(entry))
                    && (entry.origin != IntentionOrigin::Travel
                        || (entry.state == IntentionState::Queued
                            && matches!(entry.work, IntentionWork::Action(Action::Move(_)))))
                    && match (&entry.work, entry.movement_context, entry.terminal_exit_context) {
                        (IntentionWork::Action(action), None, Some(context)) => {
                            entry.origin != IntentionOrigin::Travel && context.policy_matches(self, *actor, action)
                        }
                        (IntentionWork::Action(action), Some(context), None) if action.selected_step().is_some() => context.valid(),
                        (work, None, None) => !matches!(work, IntentionWork::Action(action) if action.selected_step().is_some()),
                        _ => false,
                    }
                    && match entry.work.clone() {
                        IntentionWork::Action(Action::ChooseGrowthSource { basis, source }) => {
                            basis.valid()
                                && basis.actor == *actor
                                && source.valid(&self.content.rules)
                        }
                        IntentionWork::Action(Action::Respec {
                            basis,
                            replacement,
                            supplies,
                        }) => {
                            basis.valid()
                                && basis.actor == *actor
                                && replacement.structural_valid(&self.content.rules)
                                && supplies.valid(1)
                        }
                        IntentionWork::Action(Action::ChooseIntellectExpertise { basis, choice }) => {
                            basis.valid() && basis.actor == *actor && choice.structural_valid(&self.content.rules)
                        }
                        IntentionWork::Action(Action::Purchase { basis, purchase }) => {
                            // Stale permanent builds remain queued until execution
                            // reports their ordinary terminal failure. Restore checks
                            // shape, catalog references and owner binding only.
                            basis.valid()
                                && basis.actor == *actor
                                && purchase.structural_valid(&self.content.rules)
                        }
                        IntentionWork::Action(Action::UseAbility {
                            id,
                            invocation,
                            supplies,
                        }) => {
                            self.content.rules.ability(&id).is_some_and(|definition| {
                                    if !definition.invocation.accepts(invocation.kind())
                                        || !supplies.valid(definition.supply_costs.len())
                                    {
                                        return false;
                                    }
                                    if matches!(invocation.aim, crate::abilities::AbilityAim::DirectionalRay { .. } | crate::abilities::AbilityAim::CellRay { .. }) {
                                        return invocation.valid() && matches!(definition.reach, crate::rules::Reach::Sight { .. } | crate::rules::Reach::UntilObstruction);
                                    }
                                    if matches!(invocation.aim, crate::abilities::AbilityAim::DirectionalProjectile { .. } | crate::abilities::AbilityAim::CellProjectile { .. }) {
                                        return invocation.valid() && matches!(definition.reach, crate::rules::Reach::Projectile);
                                    }
                                    let target = invocation.target(*actor);
                                    if target.0 == 0 || target.0 >= self.next_actor_id {
                                        return false;
                                    }
                                    match definition.reach {
                                        crate::rules::Reach::Projectile | crate::rules::Reach::UntilObstruction => false,
                                        crate::rules::Reach::SelfOnly => target == *actor,
                                        crate::rules::Reach::SelfOrAlly { .. } => true,
                                        crate::rules::Reach::Melee
                                        | crate::rules::Reach::Sight { .. }
                                        | crate::rules::Reach::MutualSight { .. } => target != *actor,
                                    }
                                })
                        }
                        IntentionWork::Action(Action::TouchApparentObject { subject, quantity }) => subject.0 > 0 && subject.0 < self.next_actor_id && quantity.is_none_or(|n| n == 1),
                        IntentionWork::Action(Action::Escape { hold }) => hold.0 > 0 && hold.0 < self.next_hold_id,
                        IntentionWork::Action(Action::ExamineTrap { trap }) => trap != 0,
                        IntentionWork::Action(Action::SetDoor { door, .. }) => door != 0,
                        IntentionWork::Action(Action::Identify {choice,item})=>self.intention_issued(choice)&&item.is_none_or(|id|id.0>0&&id.0<self.next_item_id),
                        IntentionWork::Action(Action::Equip { item, slot }) => {
                            item.0 > 0 && slot.0 < 64
                        }
                        IntentionWork::Action(
                            Action::Unequip { item } | Action::Ignite { item } | Action::Extinguish { item }
                            | Action::Read {item}
                            | Action::Zap {item,..}
                            | Action::Drink { item }
                            | Action::Eat { item },
                        ) => item.0 > 0,
                        IntentionWork::Action(
                            Action::Take { item, quantity } | Action::Drop { item, quantity },
                        ) => item.0 != 0 && quantity.is_none_or(|quantity| quantity != 0),
                        IntentionWork::Action(Action::Move(_) | Action::Wait | Action::ReturnPlane) => true,
                        IntentionWork::AiDecision => entry.origin == IntentionOrigin::Autonomous,
                        IntentionWork::ResumePreparation { work } => {
                            work.structural_valid(*actor)
                                && entry.origin == IntentionOrigin::Human
                                && if self.actors.contains_key(actor) {
                                    self.preparation(*actor).is_some_and(|preparation| {
                                        preparation.intention == Some(entry.id)
                                            && preparation.plan.work() == work
                                            && !preparation.active
                                    })
                                } else {
                                    self.known_actor_region(*actor).is_some()
                                }
                        }
                    }
            })
            && self
                .actors
                .iter()
                .filter_map(|(actor, state)| Some((*actor, state.pending.as_ref()?)))
                .all(|(actor, preparation)| {
                    (!preparation.plan.needs_item_payment()
                        || preparation.origin_intention().is_some())
                        && preparation.origin_intention().is_none_or(|origin| {
                            origin.0 != 0
                                && origin.0 < self.intentions.next_id
                                && preparation.intention.is_none_or(|latest| origin <= latest)
                                && origins.insert(origin)
                                && owners.get(&origin).is_none_or(|owner| *owner == actor)
                        })
                        && preparation.intention.is_none_or(|id| {
                            id.0 != 0
                                && id.0 < self.intentions.next_id
                                && (ids.insert(id)
                                    || self.pending_intention(actor).is_some_and(|queued| {
                                        queued.id == id
                                            && queued.work
                                                == IntentionWork::ResumePreparation {
                                                    work: preparation.plan.work(),
                                                }
                                    }))
                        })
                })
    }

    /// A deterministic reaction receives provenance without creating a second
    /// user/AI queue entry or replacing the toucher's current intention.
    pub(crate) fn issue_reaction_intention(&mut self) -> Result<IntentionId, GameError> {
        let id = IntentionId(self.intentions.next_id);
        self.intentions.next_id = self
            .intentions
            .next_id
            .checked_add(1)
            .ok_or(GameError::IdentityExhausted)?;
        Ok(id)
    }

    pub(crate) fn intention_issued(&self, id: IntentionId) -> bool {
        id.0 != 0 && id.0 < self.intentions.next_id
    }
}

#[cfg(test)]
#[path = "intention_tests.rs"]
mod tests;
