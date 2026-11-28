//! Beat composition: timeline spans, flights and reactions, and client-defined
//! beat ends.
use std::sync::Arc;
use tor_client_common::compositor::*;
use tor_client_common::{test_support, ClientState};
use tor_protocol::*;

const SELF: [u8; 32] = [1; 32];
const ORC: [u8; 32] = [7; 32];
const RAT: [u8; 32] = [8; 32];

fn target(digest: [u8; 32]) -> ActorTarget {
    ActorTarget::from_digest(digest)
}

fn client() -> ClientState {
    let mut observation = test_support::observation();
    observation.self_target = target(SELF);
    ClientState::from_snapshot(Snapshot {
        context: crate::stream_context(0),
        readiness: Readiness {
            revision: 0,
            admission: true,
            resume: vec![],
            cancel: vec![],
        },
        intentions: vec![],
        travel: None,
        actor: observation.actor,
        branch: BranchId("beat-branch".into()),
        cursor: StreamCursor {
            sequence: 0,
            tick: 0,
        },
        state: Arc::new(StateView {
            snapshot: SnapshotId("beat-initial".into()),
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

struct Stream {
    client: ClientState,
    sequence: u64,
}

impl Stream {
    fn new() -> Self {
        Self {
            client: client(),
            sequence: 0,
        }
    }
    fn update(&mut self, tick: u64, body: UpdateBody) -> StreamUpdate {
        self.sequence += 1;
        StreamUpdate {
            context: self.client.context().clone(),
            actor: self.client.actor(),
            branch: self.client.branch().clone(),
            cursor: StreamCursor {
                sequence: self.sequence,
                tick,
            },
            body,
        }
    }
    fn outcome(
        &mut self,
        tick: u64,
        facts: Vec<OutcomeFact>,
        status: Option<OutcomeStatusFacts>,
    ) -> ServerMessage {
        let update = self.update(
            tick,
            UpdateBody::Outcome {
                visual_changed: false,
                facts: OutcomeFacts {
                    self_target: target(SELF),
                    status,
                    actor_names: vec![],
                    facts,
                },
                event: None,
            },
        );
        ServerMessage::Update {
            update: Box::new(update),
        }
    }
}

fn status(hp: u64, recovery: u64, preparation: Option<u64>) -> OutcomeStatusFacts {
    OutcomeStatusFacts {
        hp,
        max_hp: 20,
        dead: false,
        victory: false,
        terminal: false,
        sequence: None,
        preparation_remaining: preparation,
        preparation_active: preparation.is_some(),
        recovery_remaining: recovery,
    }
}

fn step(direction: Direction) -> OutcomeFact {
    OutcomeFact::OwnAction {
        event: Event::Moved { direction },
        name: None,
        basic_attack: false,
        underfoot: None,
    }
}

fn activity(actor: [u8; 32], started_at: u64, activity: ActivityKind) -> OutcomeFact {
    OutcomeFact::Activity {
        actor: target(actor),
        started_at,
        activity,
    }
}

fn walk(x: i32) -> ActivityKind {
    ActivityKind::Move {
        from: Some(Position { x, y: 0, z: 0 }),
        to: Some(Position {
            x: x - 1,
            y: 0,
            z: 0,
        }),
    }
}

fn blow(
    caster: [u8; 32],
    victim: [u8; 32],
    outcome: AbilityOutcome,
    strike: Option<u16>,
) -> OutcomeFact {
    OutcomeFact::Combat {
        event: CombatEventView::Ability {
            caster: Some(target(caster)),
            target: Some(target(victim)),
            ability: test_support::content("bite"),
            outcome,
            strike,
        },
        basic_attack: false,
    }
}

fn push_all<P: BeatPolicy>(
    beat: &mut Compositor<P>,
    stream: &Stream,
    messages: &[ServerMessage],
) -> Vec<Pushed> {
    messages
        .iter()
        .map(|message| beat.push(&stream.client, message))
        .collect()
}

#[test]
fn own_steps_across_settles_chain_into_one_continuous_walk() {
    let mut stream = Stream::new();
    let mut beat = Compositor::new(Caution::default());
    for (index, tick) in [0u64, 100, 200].into_iter().enumerate() {
        let message = stream.outcome(
            tick,
            vec![step(Direction::East)],
            Some(status(20, 100, None)),
        );
        assert_eq!(beat.push(&stream.client, &message), Pushed::Pending);
        // Each client-supplied step is its own settle; the beat continues.
        let more = index < 2;
        assert_eq!(
            beat.settled(&Settled::Done, more),
            if more {
                Pushed::Pending
            } else {
                Pushed::Ended(BeatEnd::PlanEnded)
            }
        );
    }
    let spans = beat.timeline().spans();
    assert_eq!(
        spans
            .iter()
            .map(|span| (span.start, span.end))
            .collect::<Vec<_>>(),
        [
            (0, SpanEnd::Exact(100)),
            (100, SpanEnd::Exact(200)),
            (200, SpanEnd::Exact(300))
        ],
        "each step fills commit to ready, and the next begins where it ends"
    );
    assert!(spans.iter().all(|span| span.kind
        == SpanKind::OwnStep {
            direction: Direction::East
        }));
    assert_eq!(beat.ended(), Some(BeatEnd::PlanEnded));
    let finished = beat.take();
    assert_eq!(finished.spans().len(), 3);
    assert!(beat.timeline().is_empty() && beat.ended().is_none());
}

#[test]
fn overlapping_actors_keep_their_own_disclosed_starts_and_inferred_ends() {
    let mut stream = Stream::new();
    let mut beat = Compositor::new(BeatNever);
    let messages = [
        stream.outcome(
            130,
            vec![activity(ORC, 120, walk(4)), activity(RAT, 125, walk(6))],
            None,
        ),
        stream.outcome(200, vec![activity(ORC, 190, walk(3))], None),
    ];
    push_all(&mut beat, &stream, &messages);
    let spans: Vec<_> = beat
        .timeline()
        .spans()
        .iter()
        .map(|span| (span.actor, span.start, span.end))
        .collect();
    assert_eq!(
        spans,
        [
            (target(ORC), 120, SpanEnd::Inferred(190)),
            (target(RAT), 125, SpanEnd::Open),
            (target(ORC), 190, SpanEnd::Open),
        ]
    );
}

#[test]
fn strikes_mark_their_wind_up_and_reactions_link_same_tick_flights() {
    let mut stream = Stream::new();
    let mut beat = Compositor::new(BeatNever);
    let wind_up = activity(
        ORC,
        100,
        ActivityKind::WindUp {
            posture: ActivityPosture::Ranged,
        },
    );
    let flight = |caster| OutcomeFact::Combat {
        event: CombatEventView::Effect {
            caster: Some(target(caster)),
            ability: None,
            tick: 300,
            timing: EffectTiming::Instant,
            presentation: EffectPresentation {
                form: EffectAppearance::Missile,
                visual: EffectVisual::Neutral,
                style: None,
                object: None,
            },
            anchors: vec![],
            delivery: EffectDelivery::Projectile,
            cue: EffectCue::Physical,
            segments: vec![
                vec![Position { x: 4, y: 0, z: 0 }, Position { x: 3, y: 1, z: 0 }],
                vec![Position { x: 1, y: 2, z: 0 }, Position { x: 0, y: 2, z: 0 }],
            ],
        },
        basic_attack: false,
    };
    let messages = [
        stream.outcome(100, vec![wind_up], None),
        stream.outcome(
            300,
            vec![
                flight(ORC),
                flight(RAT),
                blow(ORC, SELF, AbilityOutcome::Applied, Some(0)),
            ],
            None,
        ),
        stream.outcome(
            350,
            vec![blow(ORC, SELF, AbilityOutcome::Miss, Some(1))],
            None,
        ),
        stream.outcome(
            400,
            vec![OutcomeFact::Combat {
                event: CombatEventView::Died {
                    actor: target(ORC),
                    cause: DeathCause::Injury,
                },
                basic_attack: false,
            }],
            None,
        ),
    ];
    push_all(&mut beat, &stream, &messages);
    let timeline = beat.timeline();
    let span = &timeline.spans()[0];
    assert_eq!(
        span.strikes,
        [
            StrikeMark {
                tick: 300,
                ordinal: Some(0)
            },
            StrikeMark {
                tick: 350,
                ordinal: Some(1)
            }
        ]
    );
    assert_eq!(
        span.end,
        SpanEnd::Inferred(400),
        "death bounds the open span"
    );
    // Segments stay separate: a gap is never joined.
    assert_eq!(timeline.flights().len(), 2);
    assert_eq!(timeline.flights()[0].segments.len(), 2);
    assert_eq!(timeline.flights()[1].caster, Some(target(RAT)));
    assert_eq!(
        timeline
            .reactions()
            .iter()
            .map(|reaction| (
                reaction.target,
                reaction.tick,
                reaction.kind,
                reaction.flight
            ))
            .collect::<Vec<_>>(),
        [
            (target(SELF), 300, ReactionKind::Hit, Some(0)),
            (target(SELF), 350, ReactionKind::Miss, None),
            (target(ORC), 400, ReactionKind::Death, None),
        ]
    );
}

#[test]
fn own_work_ends_at_its_reported_recovery_or_an_earlier_new_action() {
    let mut stream = Stream::new();
    let mut beat = Compositor::new(BeatNever);
    let started = OutcomeFact::OwnAction {
        event: Event::AbilityStarted {
            ability: test_support::content("bite"),
            target: target(ORC),
        },
        name: None,
        basic_attack: true,
        underfoot: None,
    };
    let messages = [
        // Preparing: the end is not yet known.
        stream.outcome(0, vec![started], Some(status(20, 0, Some(100)))),
        stream.outcome(
            100,
            vec![blow(SELF, ORC, AbilityOutcome::Applied, None)],
            Some(status(20, 50, None)),
        ),
    ];
    push_all(&mut beat, &stream, &messages);
    assert_eq!(beat.timeline().spans()[0].end, SpanEnd::Exact(150));
    // Recovery that has not elapsed is bounded by a new disclosed own action.
    let mut beat = Compositor::new(BeatNever);
    let mut stream = Stream::new();
    let messages = [
        stream.outcome(0, vec![step(Direction::North)], Some(status(20, 140, None))),
        stream.outcome(
            120,
            vec![step(Direction::North)],
            Some(status(20, 100, None)),
        ),
    ];
    push_all(&mut beat, &stream, &messages);
    assert_eq!(
        beat.timeline()
            .spans()
            .iter()
            .map(|span| span.end)
            .collect::<Vec<_>>(),
        [SpanEnd::Exact(120), SpanEnd::Exact(220)]
    );
}

#[test]
fn caution_interrupts_for_a_newcomer_or_a_wound_on_authoritative_facts() {
    let noticed = OutcomeFact::Noticed {
        actor: target(RAT),
        name: "rat".into(),
        asset: None,
        position: Position { x: 3, y: 0, z: 0 },
    };
    let mut stream = Stream::new();
    let mut beat = Compositor::new(Caution::default());
    let first = stream.outcome(0, vec![step(Direction::East)], Some(status(20, 100, None)));
    let seen = stream.outcome(
        100,
        vec![step(Direction::East), noticed],
        Some(status(20, 100, None)),
    );
    assert_eq!(
        push_all(&mut beat, &stream, &[first, seen]),
        [Pushed::Pending, Pushed::Ended(BeatEnd::Interrupted)]
    );
    // A later message cannot reopen an ended beat.
    let later = stream.outcome(200, vec![], None);
    assert_eq!(
        beat.push(&stream.client, &later),
        Pushed::Ended(BeatEnd::Interrupted)
    );
    let mut stream = Stream::new();
    let mut beat = Compositor::new(Caution::default());
    let first = stream.outcome(0, vec![], Some(status(20, 100, None)));
    let hurt = stream.outcome(50, vec![], Some(status(17, 100, None)));
    assert_eq!(
        push_all(&mut beat, &stream, &[first, hurt]),
        [Pushed::Pending, Pushed::Ended(BeatEnd::Interrupted)]
    );
}

#[test]
fn only_the_server_side_endings_require_a_decision() {
    let mut stream = Stream::new();
    let control = ServerMessage::Update {
        update: Box::new(stream.update(10, UpdateBody::Control { has_control: false })),
    };
    for (message, expected) in [
        (
            ServerMessage::Waiting {
                on: Waiting::Others,
            },
            Pushed::Ended(BeatEnd::DecisionRequired),
        ),
        (ServerMessage::Waiting { on: Waiting::You }, Pushed::Pending),
        (control, Pushed::Ended(BeatEnd::DecisionRequired)),
    ] {
        let mut beat = Compositor::new(BeatNever);
        assert_eq!(beat.push(&stream.client, &message), expected);
    }
    for (settled, expected) in [
        (Settled::Waiting(None), BeatEnd::DecisionRequired),
        (Settled::Lost, BeatEnd::DecisionRequired),
        (
            Settled::Journey(TravelPhase::DecisionRequired),
            BeatEnd::DecisionRequired,
        ),
        (Settled::Journey(TravelPhase::Blocked), BeatEnd::PlanEnded),
        (Settled::Deferred, BeatEnd::PlanEnded),
        (
            Settled::Rejected {
                code: ErrorCode::InvalidAction,
                detail: None,
            },
            BeatEnd::PlanEnded,
        ),
    ] {
        let mut beat = Compositor::new(BeatNever);
        assert_eq!(
            beat.settled(&settled, true),
            Pushed::Ended(expected),
            "{settled:?}"
        );
    }
    // Another attachment's or branch's updates never enter this timeline.
    let mut beat = Compositor::new(BeatNever);
    let mut foreign = stream.update(20, UpdateBody::Control { has_control: false });
    foreign.branch = BranchId("other".into());
    assert_eq!(
        beat.push(
            &stream.client,
            &ServerMessage::Update {
                update: Box::new(foreign)
            }
        ),
        Pushed::Pending
    );
    assert!(beat.timeline().frames().is_empty());
}

#[test]
fn a_replacement_snapshot_flushes_the_beat() {
    let mut stream = Stream::new();
    let mut beat = Compositor::new(BeatNever);
    let message = stream.outcome(0, vec![step(Direction::East)], Some(status(20, 100, None)));
    beat.push(&stream.client, &message);
    assert!(!beat.timeline().is_empty());
    let snapshot = ServerMessage::Snapshot {
        request_id: "reset".into(),
        snapshot: Box::new(stream.client.reset_snapshot().unwrap()),
    };
    assert_eq!(
        beat.push(&stream.client, &snapshot),
        Pushed::Ended(BeatEnd::DecisionRequired)
    );
    assert!(beat.timeline().is_empty());
}

#[test]
fn replacement_after_outcome_without_permissions_ends_the_pending_settle_and_beat() {
    let mut stream = Stream::new();
    let mut replacement = stream.client.reset_snapshot().unwrap();
    replacement.context = replacement.context.next_reset().unwrap();
    let not_ready = stream.update(
        0,
        UpdateBody::Readiness {
            readiness: Readiness {
                revision: 1,
                admission: false,
                resume: vec![],
                cancel: vec![],
            },
        },
    );
    stream.client.apply(not_ready).unwrap();
    let mut beat = Compositor::new(BeatNever);
    let mut settle = SettleBoundary::new("move", Until::Acted, false);
    let receipt = ServerMessage::Ack {
        request_id: "move".into(),
        context: stream.client.reply_context(),
        receipt: RequestReceipt::Admitted {
            actor: stream.client.actor(),
            branch: stream.client.branch().clone(),
            intention: IntentionId("walk".into()),
            entry_id: EntryId("walk-entry".into()),
            phase: IntentionPhase::Resolved,
        },
    };
    assert_eq!(settle.observe(&stream.client, false, false, &receipt), None);
    let outcome = stream.outcome(0, vec![step(Direction::East)], Some(status(20, 100, None)));
    let ServerMessage::Update { update } = &outcome else {
        unreachable!("outcome is an applied stream update");
    };
    stream.client.apply((**update).clone()).unwrap();
    assert_eq!(beat.push(&stream.client, &outcome), Pushed::Pending);
    assert!(!beat.timeline().is_empty());
    assert_eq!(settle.observe(&stream.client, false, false, &outcome), None);

    stream.client.replace_snapshot(replacement.clone()).unwrap();
    assert!(stream.client.can_admit_intention(), "the new view is ready");
    let snapshot = ServerMessage::Snapshot {
        request_id: "recover".into(),
        snapshot: Box::new(replacement),
    };
    let lost = settle.observe(&stream.client, false, false, &snapshot);
    assert_eq!(
        lost,
        Some(Settled::Lost),
        "the new grant cannot settle the old action"
    );
    let ended = Pushed::Ended(BeatEnd::DecisionRequired);
    assert_eq!(beat.push(&stream.client, &snapshot), ended);
    assert!(beat.timeline().is_empty(), "discard unplayed old work");
    assert_eq!(
        beat.settled(&Settled::Done, true),
        ended,
        "the old plan cannot reopen"
    );
}

#[test]
fn frames_keep_their_ticks_and_spans_their_disclosing_frame() {
    let mut stream = Stream::new();
    let mut beat = Compositor::new(BeatNever);
    for tick in [0u64, 100, 200, 300, 400, 500, 600] {
        let message = stream.outcome(
            tick,
            vec![step(Direction::East)],
            Some(status(20, 100, None)),
        );
        assert_eq!(beat.push(&stream.client, &message), Pushed::Pending);
    }
    assert_eq!(beat.ended(), None);
    // Every update keeps its own frame for later coordinate conversion.
    assert_eq!(
        beat.timeline()
            .frames()
            .iter()
            .map(|frame| (frame.sequence, frame.tick))
            .collect::<Vec<_>>(),
        (1..=7).map(|n| (n, (n - 1) * 100)).collect::<Vec<_>>()
    );
    assert!(beat
        .timeline()
        .spans()
        .iter()
        .zip(1..)
        .all(|(span, frame)| span.frame == frame));
}

/// A policy that never interrupts, so tests isolate timeline behaviour.
struct BeatNever;
impl BeatPolicy for BeatNever {}
