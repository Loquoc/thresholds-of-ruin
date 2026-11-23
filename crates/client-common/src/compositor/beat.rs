//! Client-defined beats over the shared timeline.
//!
//! A beat can span many settles and include actions the client supplies
//! itself. Each client decides through its [`BeatPolicy`] what interrupts a
//! beat; the compositor applies the shared rules for what the server alone can
//! end.
use super::{Settled, Timeline};
use crate::ClientState;
use tor_protocol::*;

/// Why a beat ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeatEnd {
    /// The client's own plan finished, failed or was refused.
    PlanEnded,
    /// The client judged that something needs the human.
    Interrupted,
    /// Only the human, another player or a replacement state can continue:
    /// a decision phase, lost control, a wait on someone else, or a reset.
    DecisionRequired,
}

/// The result of one pushed message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pushed {
    Pending,
    Ended(BeatEnd),
}

/// A client's own beat rules.
pub trait BeatPolicy {
    /// Whether this already applied outcome needs the human before the
    /// client's plan continues. Decided on authoritative state, never on what
    /// is currently drawn.
    fn interrupts(&mut self, _state: &ClientState, _facts: &OutcomeFacts) -> bool {
        false
    }
}

/// The ordinary caution most plans want: stop for a newcomer or a wound.
#[derive(Clone, Debug, Default)]
pub struct Caution {
    hp: Option<u64>,
}

impl BeatPolicy for Caution {
    fn interrupts(&mut self, _state: &ClientState, facts: &OutcomeFacts) -> bool {
        let noticed = facts
            .facts
            .iter()
            .any(|fact| matches!(fact, OutcomeFact::Noticed { .. }));
        let wounded = match (self.hp, &facts.status) {
            (Some(before), Some(status)) => status.hp < before,
            _ => false,
        };
        if let Some(status) = &facts.status {
            self.hp = Some(status.hp);
        }
        noticed || wounded
    }
}

/// One beat being composed, and the beats it has completed.
pub struct Compositor<P> {
    policy: P,
    timeline: Timeline,
    ended: Option<BeatEnd>,
}

impl<P: BeatPolicy> Compositor<P> {
    pub fn new(policy: P) -> Self {
        Self {
            policy,
            timeline: Timeline::default(),
            ended: None,
        }
    }
    pub fn policy(&self) -> &P {
        &self.policy
    }
    pub fn timeline(&self) -> &Timeline {
        &self.timeline
    }
    /// Why the current beat ended, once it has.
    pub fn ended(&self) -> Option<BeatEnd> {
        self.ended
    }
    /// Consume one message already validated and applied to `state`.
    pub fn push(&mut self, state: &ClientState, message: &ServerMessage) -> Pushed {
        if let Some(end) = self.ended {
            return Pushed::Ended(end);
        }
        match message {
            // A replacement state flushes what was being composed.
            ServerMessage::Snapshot { .. } => {
                self.timeline = Timeline::default();
                return self.end(BeatEnd::DecisionRequired);
            }
            ServerMessage::Waiting { on } if *on != Waiting::You => {
                return self.end(BeatEnd::DecisionRequired);
            }
            ServerMessage::Update { update } => {
                if update.actor != state.actor() || update.branch != *state.branch() {
                    return Pushed::Pending;
                }
                self.timeline.push(update);
                match &update.body {
                    UpdateBody::Control { has_control: false } => {
                        return self.end(BeatEnd::DecisionRequired)
                    }
                    UpdateBody::Travel { status, .. }
                        if matches!(
                            status.phase,
                            TravelPhase::DecisionRequired | TravelPhase::ControlLost
                        ) =>
                    {
                        return self.end(BeatEnd::DecisionRequired)
                    }
                    UpdateBody::Outcome { facts, .. } if self.policy.interrupts(state, facts) => {
                        return self.end(BeatEnd::Interrupted);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Pushed::Pending
    }

    /// One settle of the client's plan finished. The beat continues only if
    /// the plan has another step and the settle left the client free to act.
    pub fn settled(&mut self, settled: &Settled, plan_continues: bool) -> Pushed {
        if let Some(end) = self.ended {
            return Pushed::Ended(end);
        }
        match settled {
            Settled::Waiting(_) | Settled::Lost => self.end(BeatEnd::DecisionRequired),
            Settled::Journey(TravelPhase::DecisionRequired | TravelPhase::ControlLost) => {
                self.end(BeatEnd::DecisionRequired)
            }
            Settled::Done | Settled::Journey(TravelPhase::Active | TravelPhase::Arrived)
                if plan_continues =>
            {
                Pushed::Pending
            }
            _ => self.end(BeatEnd::PlanEnded),
        }
    }

    fn end(&mut self, end: BeatEnd) -> Pushed {
        self.ended = Some(end);
        Pushed::Ended(end)
    }

    /// Start the next beat, returning the finished one's timeline.
    pub fn take(&mut self) -> Timeline {
        self.ended = None;
        std::mem::take(&mut self.timeline)
    }
}
