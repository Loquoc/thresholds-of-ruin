//! When one sent request counts as finished: one server input wait.
//!
//! This is a synchronous decision over messages the connection has already
//! validated and applied. Clients own sending, timeouts and presentation; the
//! boundary owns only the settlement rules, so every client agrees on them.
use super::AdmissionBoundary;
use crate::ClientState;
use tor_protocol::*;

/// What a request waits for before it counts as finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Until {
    /// An action: acknowledged, its result seen, and the character ready.
    Acted,
    /// A journey: acknowledged, ended, and the character ready.
    Journey,
    /// Resumed play: the character ready.
    Ready,
    /// A reply: the acknowledgement, snapshot or history page.
    Answered,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Settled {
    Done,
    /// The server yielded to another actor; no further work may be sent.
    /// Preserve an observed journey ending for narration, not permission.
    Waiting(Option<TravelPhase>),
    /// Play yielded and the same admitted work is still explicitly queued.
    Pending,
    /// Execution was refused, but the queued intention remains pending.
    Deferred,
    /// The server's detail is carried only for a correlated rejection on
    /// the current attachment.
    Rejected {
        code: ErrorCode,
        detail: Option<String>,
    },
    /// An admitted action ended with an owner-visible failure.
    Failed(IntentionFailure),
    /// A journey ended this way.
    Journey(TravelPhase),
    /// Control was lost, or the state replaced.
    Lost,
}

/// Why silence could not settle a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Silence {
    /// A known failure arrived, but its input permissions did not.
    FailedWithoutPermissions,
    /// The outcome is uncertain; history after reconnecting is authoritative.
    Unanswered,
}

impl Silence {
    pub fn message(self) -> &'static str {
        match self {
            Self::FailedWithoutPermissions => "The action failed, but the server did not finish updating input permissions. Reconnect before acting again.",
            Self::Unanswered => "The server did not answer. The action may have completed; reconnect and check history before trying again.",
        }
    }
}

/// The settlement state of one sent request.
pub struct SettleBoundary {
    id: String,
    until: Until,
    observing: bool,
    acked: bool,
    admission: Option<AdmissionBoundary>,
    execution_failed: bool,
    failure_reason: Option<IntentionFailure>,
    receipt: Option<EntryId>,
    ended: Option<TravelPhase>,
}

fn terminal(state: &ClientState) -> bool {
    state
        .outcome_status()
        .is_some_and(|status| status.terminal || status.dead)
}

impl SettleBoundary {
    /// `observing` marks an `observe` request, which settles on its picture.
    pub fn new(id: impl Into<String>, until: Until, observing: bool) -> Self {
        Self {
            id: id.into(),
            until,
            observing,
            acked: false,
            admission: None,
            execution_failed: false,
            failure_reason: None,
            receipt: None,
            ended: None,
        }
    }

    pub fn until(&self) -> Until {
        self.until
    }

    /// The accepted journal entry of this request, once acknowledged.
    pub fn receipt(&self) -> Option<&EntryId> {
        self.receipt.as_ref()
    }

    fn rejected_execution(&self) -> Settled {
        self.failure_reason.map_or(
            Settled::Rejected {
                code: ErrorCode::InvalidAction,
                detail: None,
            },
            Settled::Failed,
        )
    }

    /// Play stopped, and not for this player: the turn is over.
    fn others(&self) -> Settled {
        if self.execution_failed {
            self.rejected_execution()
        } else {
            match self.ended {
                Some(phase) => Settled::Journey(phase),
                None if self.until == Until::Journey => Settled::Journey(TravelPhase::Active),
                None => Settled::Done,
            }
        }
    }

    fn journey_end(&self, state: &ClientState) -> Option<TravelPhase> {
        state
            .travel()
            .filter(|t| Some(&t.id) == self.receipt.as_ref())
            .map(|t| t.phase)
            .filter(|phase| *phase != TravelPhase::Active)
    }

    /// The connection fell silent for the client's answer deadline.
    pub fn silence(&self, state: &ClientState) -> Result<Settled, Silence> {
        let permissions_missing = !terminal(state) && !state.can_admit_intention();
        let incomplete = match self.until {
            Until::Answered => self.admission.is_some(),
            Until::Acted => {
                self.admission
                    .as_ref()
                    .is_some_and(|boundary| !boundary.settled())
                    || permissions_missing
            }
            Until::Journey => self.ended.is_some() && permissions_missing,
            Until::Ready => false,
        };
        if self.acked && !incomplete && !self.observing {
            return Ok(self.others());
        }
        Err(if self.execution_failed {
            Silence::FailedWithoutPermissions
        } else {
            Silence::Unanswered
        })
    }

    /// Consume one synchronized message already applied to `state`.
    ///
    /// `observation_completed` is the connection's own answer for an observe
    /// request. `refused` reports a current scoped execution refusal notice: an
    /// explicit stop that preserves the queued intention, not a settlement.
    pub fn observe(
        &mut self,
        state: &ClientState,
        observation_completed: bool,
        refused: bool,
        message: &ServerMessage,
    ) -> Option<Settled> {
        if self.observing && observation_completed {
            return Some(Settled::Done);
        }
        if refused {
            return Some(Settled::Deferred);
        }
        match message {
            ServerMessage::Error {
                request_id,
                code,
                scope,
                message,
            } if request_id.as_deref() == Some(self.id.as_str()) => {
                let detail = match scope {
                    ErrorScope::Attached { context }
                        if !message.trim().is_empty()
                            && state.validate_reply_context(context).is_ok() =>
                    {
                        Some(message.clone())
                    }
                    _ => None,
                };
                return Some(Settled::Rejected {
                    code: *code,
                    detail,
                });
            }
            ServerMessage::Ack {
                request_id,
                receipt: accepted,
                ..
            } if *request_id == self.id => {
                self.acked = true;
                self.receipt = accepted.entry_id().cloned();
                self.admission = AdmissionBoundary::new(accepted);
            }
            ServerMessage::Update { update }
                if matches!(&update.body,
                UpdateBody::Intention { status, .. } if matches!(status.phase, IntentionPhase::Failed | IntentionPhase::Cancelled)
                    && self.receipt.as_ref() == Some(&status.entry_id)) =>
            {
                self.execution_failed = true;
                if let UpdateBody::Intention { failure, .. } = &update.body {
                    self.failure_reason = *failure;
                }
            }
            ServerMessage::Snapshot { request_id, .. } if *request_id == self.id => {
                return Some(Settled::Done);
            }
            ServerMessage::WizardReport { request_id, .. }
            | ServerMessage::CreatureCatalog { request_id, .. }
            | ServerMessage::CreatureInspection { request_id, .. }
            | ServerMessage::CreatureStrikePage { request_id, .. }
            | ServerMessage::AdvancementPage { request_id, .. }
            | ServerMessage::History { request_id, .. }
                if *request_id == self.id =>
            {
                return Some(Settled::Done);
            }
            ServerMessage::Snapshot { .. } => return Some(Settled::Lost),
            ServerMessage::Waiting { on } if self.acked && *on != Waiting::You => {
                if self.execution_failed
                    && self
                        .admission
                        .as_ref()
                        .is_some_and(|boundary| !boundary.settled())
                {
                    // A wait can yield retained work, but cannot turn a known
                    // failure's incomplete permissions into a fresh input boundary.
                    return None;
                }
                if self.until == Until::Journey {
                    self.ended = self.journey_end(state);
                }
                return Some(
                    if self.until != Until::Answered
                        && !self.execution_failed
                        && super::is_input_yield(*on)
                        && self
                            .admission
                            .as_ref()
                            .is_some_and(|work| work.remains_queued(state))
                    {
                        Settled::Pending
                    } else if self.until == Until::Answered || self.execution_failed {
                        self.others()
                    } else {
                        Settled::Waiting(self.ended)
                    },
                );
            }
            ServerMessage::Update { update } => {
                if let UpdateBody::Control { has_control: false } = update.body {
                    if self.until != Until::Answered {
                        return Some(Settled::Lost);
                    }
                }
            }
            _ => {}
        }
        if self.observing || !self.acked {
            return None;
        }
        let ended_run = terminal(state);
        let ready = state.character_ready() || ended_run;
        // Observation and journey completion precede their permission update.
        // Keep existing turn pacing, but consume that boundary before new input.
        let permissions_ready = ended_run || state.can_admit_intention();
        let answered = self
            .admission
            .as_mut()
            .is_none_or(|boundary| boundary.observe(message));
        let action_settled = ended_run
            || self
                .admission
                .as_ref()
                .is_none_or(|boundary| boundary.settled());
        let failure_settled = if self.admission.is_some() {
            action_settled
        } else {
            permissions_ready
        };
        if self.execution_failed && failure_settled && (self.until != Until::Answered || answered) {
            return Some(self.rejected_execution());
        }
        let gameplay_ready = ready && permissions_ready;
        match self.until {
            Until::Answered if answered => return Some(Settled::Done),
            Until::Ready if ready => return Some(Settled::Done),
            Until::Acted if gameplay_ready && action_settled => return Some(Settled::Done),
            Until::Journey => {
                self.ended = self.journey_end(state);
                if let Some(phase) = self.ended {
                    if gameplay_ready {
                        return Some(Settled::Journey(phase));
                    }
                }
                if self.receipt.is_none() {
                    return Some(Settled::Journey(TravelPhase::Failed));
                }
            }
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    const ID: &str = "turn-request";

    fn state(admission: bool, ready: bool, travel: Option<TravelPhase>) -> ClientState {
        let mut observation = crate::test_support::observation();
        observation.ready = ready;
        ClientState::from_snapshot(Snapshot {
            context: StreamContext {
                stream: StreamId("settle-stream".into()),
                epoch: 0,
            },
            readiness: Readiness {
                revision: 0,
                admission,
                resume: vec![],
                cancel: vec![],
            },
            intentions: vec![],
            travel: travel.map(|phase| TravelStatus {
                id: EntryId("journey".into()),
                destination: "hall".into(),
                completed_steps: 3,
                phase,
            }),
            actor: observation.actor,
            branch: BranchId("settle-branch".into()),
            cursor: StreamCursor {
                sequence: 0,
                tick: 0,
            },
            state: Arc::new(StateView {
                snapshot: SnapshotId("settle-initial".into()),
                wizard_game: false,
                revision: 1,
                observation,
            }),
            has_control: true,
            history: HistoryPage {
                entries: vec![],
                older_before: None,
            },
        })
        .unwrap()
    }

    fn ack(state: &ClientState, receipt: RequestReceipt) -> ServerMessage {
        ServerMessage::Ack {
            context: state.reply_context(),
            request_id: ID.into(),
            receipt,
        }
    }

    fn immediate(state: &ClientState, entry: &str) -> ServerMessage {
        ack(
            state,
            RequestReceipt::Immediate {
                actor: state.actor(),
                branch: state.branch().clone(),
                entry_id: Some(EntryId(entry.into())),
            },
        )
    }

    fn update(state: &ClientState, sequence: u64, body: UpdateBody) -> ServerMessage {
        ServerMessage::Update {
            update: Box::new(StreamUpdate {
                context: state.context().clone(),
                actor: state.actor(),
                branch: state.branch().clone(),
                cursor: StreamCursor { sequence, tick: 0 },
                body,
            }),
        }
    }

    fn intention(state: &ClientState, sequence: u64, phase: IntentionPhase) -> ServerMessage {
        update(
            state,
            sequence,
            UpdateBody::Intention {
                failure: None,
                status: IntentionStatus {
                    actor: state.actor(),
                    branch: state.branch().clone(),
                    intention: IntentionId("work".into()),
                    entry_id: EntryId("work-entry".into()),
                    phase,
                },
            },
        )
    }

    fn permissions(state: &ClientState, sequence: u64) -> ServerMessage {
        update(
            state,
            sequence,
            UpdateBody::Readiness {
                readiness: Readiness {
                    revision: sequence,
                    admission: true,
                    resume: vec![],
                    cancel: vec![],
                },
            },
        )
    }

    fn waiting(on: Waiting) -> ServerMessage {
        ServerMessage::Waiting { on }
    }

    fn feed(
        boundary: &mut SettleBoundary,
        state: &ClientState,
        messages: &[ServerMessage],
    ) -> Vec<Option<Settled>> {
        messages
            .iter()
            .map(|message| boundary.observe(state, false, false, message))
            .collect()
    }

    #[test]
    fn an_action_settles_only_after_its_ack_and_ready_permissions() {
        let ready = state(true, true, None);
        let mut boundary = SettleBoundary::new(ID, Until::Acted, false);
        // Readiness before the acknowledgement never settles the request.
        assert_eq!(
            boundary.observe(&ready, false, false, &permissions(&ready, 1)),
            None
        );
        assert_eq!(
            boundary.observe(&ready, false, false, &immediate(&ready, "done")),
            Some(Settled::Done)
        );
        assert_eq!(boundary.receipt(), Some(&EntryId("done".into())));
        // An unready character keeps an acknowledged action open.
        let waiting_state = state(true, false, None);
        let mut boundary = SettleBoundary::new(ID, Until::Acted, false);
        assert_eq!(
            boundary.observe(
                &waiting_state,
                false,
                false,
                &immediate(&waiting_state, "e")
            ),
            None
        );
        assert_eq!(
            boundary.observe(&ready, false, false, &permissions(&ready, 2)),
            Some(Settled::Done)
        );
    }

    #[test]
    fn a_queued_admission_waits_for_its_stopped_execution_and_later_permissions() {
        let ready = state(true, true, None);
        let mut boundary = SettleBoundary::new(ID, Until::Acted, false);
        let admitted = ack(
            &ready,
            RequestReceipt::Admitted {
                actor: ready.actor(),
                branch: ready.branch().clone(),
                intention: IntentionId("work".into()),
                entry_id: EntryId("work-entry".into()),
                phase: IntentionPhase::Queued,
            },
        );
        assert_eq!(
            feed(
                &mut boundary,
                &ready,
                &[
                    admitted,
                    intention(&ready, 2, IntentionPhase::Started),
                    permissions(&ready, 3),
                    intention(&ready, 4, IntentionPhase::Resolved),
                ]
            ),
            [None, None, None, None]
        );
        assert_eq!(
            boundary.observe(&ready, false, false, &permissions(&ready, 5)),
            Some(Settled::Done)
        );
    }

    #[test]
    fn only_matching_live_queued_work_is_pending_on_a_gameplay_yield() {
        for (entry, phase, waiting_on, until, expected) in [
            (
                "work-entry",
                IntentionPhase::Queued,
                Waiting::Others,
                Until::Acted,
                Settled::Pending,
            ),
            (
                "other-entry",
                IntentionPhase::Queued,
                Waiting::Others,
                Until::Acted,
                Settled::Waiting(None),
            ),
            (
                "work-entry",
                IntentionPhase::Started,
                Waiting::Others,
                Until::Acted,
                Settled::Waiting(None),
            ),
            (
                "work-entry",
                IntentionPhase::Queued,
                Waiting::Stopped,
                Until::Acted,
                Settled::Waiting(None),
            ),
            (
                "work-entry",
                IntentionPhase::Queued,
                Waiting::Others,
                Until::Answered,
                Settled::Done,
            ),
        ] {
            let mut state = state(false, false, None);
            let receipt = ack(
                &state,
                RequestReceipt::Admitted {
                    actor: state.actor(),
                    branch: state.branch().clone(),
                    intention: IntentionId("work".into()),
                    entry_id: EntryId(entry.into()),
                    phase: IntentionPhase::Queued,
                },
            );
            let mut boundary = SettleBoundary::new(ID, until, false);
            assert_eq!(boundary.observe(&state, false, false, &receipt), None);
            let queued = intention(&state, 1, IntentionPhase::Queued);
            let ServerMessage::Update { update } = &queued else {
                unreachable!()
            };
            state.apply(*update.clone()).unwrap();
            assert_eq!(boundary.observe(&state, false, false, &queued), None);
            if phase != IntentionPhase::Queued {
                let started = intention(&state, 2, phase);
                let ServerMessage::Update { update } = &started else {
                    unreachable!()
                };
                state.apply(*update.clone()).unwrap();
                assert_eq!(boundary.observe(&state, false, false, &started), None);
            }
            assert_eq!(
                boundary.observe(&state, false, false, &waiting(waiting_on)),
                Some(expected)
            );
        }
    }

    #[test]
    fn rejection_waits_losses_and_replies_have_their_own_endings() {
        let ready = state(true, true, None);
        let error = |request: Option<&str>| ServerMessage::Error {
            scope: ErrorScope::Transport {},
            request_id: request.map(str::to_owned),
            code: ErrorCode::ActorBusy,
            message: "busy".into(),
        };
        let mut boundary = SettleBoundary::new(ID, Until::Acted, false);
        assert_eq!(
            boundary.observe(&ready, false, false, &error(Some("other"))),
            None
        );
        assert_eq!(
            boundary.observe(&ready, false, false, &error(Some(ID))),
            Some(Settled::Rejected {
                code: ErrorCode::ActorBusy,
                detail: None
            })
        );
        // A wait for someone else ends an acknowledged turn, not an unacknowledged one.
        let unready = state(true, false, None);
        let mut boundary = SettleBoundary::new(ID, Until::Acted, false);
        assert_eq!(
            boundary.observe(&unready, false, false, &waiting(Waiting::Others)),
            None
        );
        boundary.observe(&unready, false, false, &immediate(&unready, "e"));
        assert_eq!(
            boundary.observe(&unready, false, false, &waiting(Waiting::You)),
            None
        );
        assert_eq!(
            boundary.observe(&unready, false, false, &waiting(Waiting::Others)),
            Some(Settled::Waiting(None))
        );
        // Losing control or receiving another snapshot replaces the turn.
        let mut boundary = SettleBoundary::new(ID, Until::Acted, false);
        assert_eq!(
            boundary.observe(
                &unready,
                false,
                false,
                &update(&unready, 1, UpdateBody::Control { has_control: false })
            ),
            Some(Settled::Lost)
        );
        let mut answered = SettleBoundary::new(ID, Until::Answered, false);
        assert_eq!(
            answered.observe(
                &unready,
                false,
                false,
                &update(&unready, 1, UpdateBody::Control { has_control: false })
            ),
            None
        );
        assert_eq!(
            answered.observe(&unready, false, false, &immediate(&unready, "reply")),
            Some(Settled::Done)
        );
    }

    #[test]
    fn journeys_settle_on_their_own_terminal_phase() {
        let arrived = state(true, true, Some(TravelPhase::Arrived));
        let mut boundary = SettleBoundary::new(ID, Until::Journey, false);
        assert_eq!(
            boundary.observe(&arrived, false, false, &immediate(&arrived, "journey")),
            Some(Settled::Journey(TravelPhase::Arrived))
        );
        // Another journey's ending does not settle this one.
        let mut boundary = SettleBoundary::new(ID, Until::Journey, false);
        assert_eq!(
            boundary.observe(&arrived, false, false, &immediate(&arrived, "elsewhere")),
            None
        );
        let active = state(false, true, Some(TravelPhase::Active));
        let mut boundary = SettleBoundary::new(ID, Until::Journey, false);
        assert_eq!(
            boundary.observe(&active, false, false, &immediate(&active, "journey")),
            None
        );
        // A journey request without an entry has nothing to follow.
        let mut boundary = SettleBoundary::new(ID, Until::Journey, false);
        let none = ack(
            &active,
            RequestReceipt::Immediate {
                actor: active.actor(),
                branch: active.branch().clone(),
                entry_id: None,
            },
        );
        assert_eq!(
            boundary.observe(&active, false, false, &none),
            Some(Settled::Journey(TravelPhase::Failed))
        );
    }

    #[test]
    fn rollback_reason_settles_with_the_failed_admission() {
        let unready = state(false, false, None);
        let ready = state(true, true, None);
        let mut boundary = SettleBoundary::new(ID, Until::Acted, false);
        boundary.observe(&unready, false, false, &immediate(&unready, "work-entry"));
        let mut message = intention(&unready, 2, IntentionPhase::Failed);
        let ServerMessage::Update { update } = &mut message else {
            unreachable!()
        };
        let UpdateBody::Intention { failure, .. } = &mut update.body else {
            unreachable!()
        };
        *failure = Some(IntentionFailure::RolledBack(RollbackAction::Read));
        boundary.observe(&unready, false, false, &message);
        assert_eq!(
            boundary.silence(&ready),
            Ok(Settled::Failed(IntentionFailure::RolledBack(
                RollbackAction::Read
            )))
        );
    }

    #[test]
    fn known_failure_cannot_yield_on_permissions_for_an_earlier_phase() {
        let ready = state(true, true, None);
        let mut boundary = SettleBoundary::new(ID, Until::Acted, false);
        let mut failed = intention(&ready, 4, IntentionPhase::Failed);
        let ServerMessage::Update { update } = &mut failed else {
            unreachable!()
        };
        let UpdateBody::Intention { failure, .. } = &mut update.body else {
            unreachable!()
        };
        let cause = IntentionFailure::RolledBack(RollbackAction::Read);
        *failure = Some(cause);
        assert_eq!(
            feed(
                &mut boundary,
                &ready,
                &[
                    ack(
                        &ready,
                        RequestReceipt::Admitted {
                            actor: ready.actor(),
                            branch: ready.branch().clone(),
                            intention: IntentionId("work".into()),
                            entry_id: EntryId("work-entry".into()),
                            phase: IntentionPhase::Queued,
                        },
                    ),
                    intention(&ready, 2, IntentionPhase::Started),
                    permissions(&ready, 3),
                    failed,
                    waiting(Waiting::Others),
                ],
            ),
            [None, None, None, None, None]
        );
        assert_eq!(
            boundary.observe(&ready, false, false, &permissions(&ready, 5)),
            Some(Settled::Failed(cause))
        );
    }

    #[test]
    fn refusals_observations_and_silence_are_explicit() {
        let ready = state(true, true, None);
        let mut boundary = SettleBoundary::new(ID, Until::Acted, false);
        assert_eq!(
            boundary.observe(&ready, false, true, &waiting(Waiting::You)),
            Some(Settled::Deferred)
        );
        let mut observing = SettleBoundary::new(ID, Until::Answered, true);
        assert_eq!(
            observing.observe(&ready, false, false, &immediate(&ready, "o")),
            None
        );
        assert_eq!(
            observing.observe(&ready, true, false, &waiting(Waiting::You)),
            Some(Settled::Done)
        );
        // Silence before an acknowledgement is never treated as success.
        let boundary = SettleBoundary::new(ID, Until::Acted, false);
        assert_eq!(boundary.silence(&ready), Err(Silence::Unanswered));
        let unready = state(false, false, None);
        let mut boundary = SettleBoundary::new(ID, Until::Acted, false);
        boundary.observe(&unready, false, false, &immediate(&unready, "e"));
        assert_eq!(boundary.silence(&unready), Err(Silence::Unanswered));
        assert_eq!(boundary.silence(&ready), Ok(Settled::Done));
        let mut failed = SettleBoundary::new(ID, Until::Acted, false);
        failed.observe(&unready, false, false, &immediate(&unready, "work-entry"));
        failed.observe(
            &unready,
            false,
            false,
            &intention(&unready, 2, IntentionPhase::Failed),
        );
        assert_eq!(
            failed.silence(&unready),
            Err(Silence::FailedWithoutPermissions)
        );
        assert_eq!(
            failed.silence(&ready),
            Ok(Settled::Rejected {
                code: ErrorCode::InvalidAction,
                detail: None
            })
        );
    }
}
