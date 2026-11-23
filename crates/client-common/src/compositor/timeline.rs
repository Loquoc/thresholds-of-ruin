//! A tick-domain record of what an observer was shown during one beat.
//!
//! Every position keeps the observation frame of the update that disclosed it
//! (`frame` is that update's stream sequence): the observer may move during a
//! beat, and only a renderer that resolves cell keys can map frames together.
//! Nothing here infers undisclosed timing. Another actor's activity ends where
//! its next disclosed activity starts; the observer's own work ends at the
//! recovery its status facts report.
use tor_protocol::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpanKind {
    /// Another actor's step between disclosed cells.
    Step {
        from: Option<Position>,
        to: Option<Position>,
    },
    /// The observer's own step; its body is at the origin of `frame`.
    OwnStep {
        direction: Direction,
    },
    /// The observer's own timed ability or item work.
    OwnWork,
    WindUp {
        posture: ActivityPosture,
    },
    FollowThrough {
        after: ActivityFollowThrough,
    },
    ItemHandling {
        work: ItemWorkKind,
        name: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanEnd {
    /// The observer's own disclosed recovery end.
    Exact(u64),
    /// Where the same actor's next disclosed activity started.
    Inferred(u64),
    /// Not yet known; a renderer caps it, never the timeline.
    Open,
}

impl SpanEnd {
    pub fn tick(self) -> Option<u64> {
        match self {
            Self::Exact(tick) | Self::Inferred(tick) => Some(tick),
            Self::Open => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StrikeMark {
    pub tick: u64,
    /// The disclosed zero-based ordinal, never a future total.
    pub ordinal: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorSpan {
    pub actor: ActorTarget,
    pub kind: SpanKind,
    pub start: u64,
    pub end: SpanEnd,
    pub strikes: Vec<StrikeMark>,
    pub frame: u64,
}

/// One disclosed effect: its visible segments, never joined across a gap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flight {
    pub caster: Option<ActorTarget>,
    pub tick: u64,
    pub delivery: EffectDelivery,
    pub segments: Vec<Vec<Position>>,
    pub frame: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReactionKind {
    Hit,
    Unaffected,
    Miss,
    Death,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reaction {
    pub target: ActorTarget,
    pub tick: u64,
    pub kind: ReactionKind,
    /// A same-tick flight from the same disclosed caster, if any.
    pub flight: Option<usize>,
    pub frame: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub sequence: u64,
    pub tick: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Timeline {
    spans: Vec<ActorSpan>,
    flights: Vec<Flight>,
    reactions: Vec<Reaction>,
    frames: Vec<Frame>,
    observer: Option<ActorTarget>,
}

impl Timeline {
    pub fn spans(&self) -> &[ActorSpan] {
        &self.spans
    }
    pub fn flights(&self) -> &[Flight] {
        &self.flights
    }
    pub fn reactions(&self) -> &[Reaction] {
        &self.reactions
    }
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty() && self.flights.is_empty() && self.reactions.is_empty()
    }

    fn open(&mut self, actor: ActorTarget, start: u64, kind: SpanKind, frame: u64, own: bool) {
        for span in self
            .spans
            .iter_mut()
            .rev()
            .filter(|span| span.actor == actor)
        {
            if span.start <= start {
                // A later disclosed start bounds every earlier span, including
                // own spans whose reported recovery has not yet been reached.
                match span.end {
                    SpanEnd::Open => span.end = SpanEnd::Inferred(start),
                    SpanEnd::Exact(end) | SpanEnd::Inferred(end) if end > start => {
                        span.end = if own {
                            SpanEnd::Exact(start)
                        } else {
                            SpanEnd::Inferred(start)
                        }
                    }
                    _ => {}
                }
            }
        }
        self.spans.push(ActorSpan {
            actor,
            kind,
            start,
            end: SpanEnd::Open,
            strikes: Vec::new(),
            frame,
        });
    }

    fn close(&mut self, actor: ActorTarget, tick: u64) {
        for span in self.spans.iter_mut().filter(|span| span.actor == actor) {
            if span.end == SpanEnd::Open && span.start <= tick {
                span.end = SpanEnd::Inferred(tick);
            }
        }
    }

    /// Record one validated, applied stream update.
    pub fn push(&mut self, update: &StreamUpdate) {
        let tick = update.cursor.tick;
        let frame = update.cursor.sequence;
        self.frames.push(Frame {
            sequence: frame,
            tick,
        });
        let UpdateBody::Outcome { facts, .. } = &update.body else {
            return;
        };
        let observer = facts.self_target;
        self.observer = Some(observer);
        for fact in &facts.facts {
            match fact {
                OutcomeFact::Activity {
                    actor,
                    started_at,
                    activity,
                } => {
                    let kind = match activity {
                        ActivityKind::Move { from, to } => SpanKind::Step {
                            from: *from,
                            to: *to,
                        },
                        ActivityKind::WindUp { posture } => SpanKind::WindUp { posture: *posture },
                        ActivityKind::FollowThrough { after } => {
                            SpanKind::FollowThrough { after: *after }
                        }
                        ActivityKind::ItemHandling { work, name } => SpanKind::ItemHandling {
                            work: *work,
                            name: name.clone(),
                        },
                    };
                    self.open(*actor, *started_at, kind, frame, false);
                }
                OutcomeFact::OwnAction { event, .. } => {
                    let kind = match event {
                        Event::Moved { direction } => SpanKind::OwnStep {
                            direction: *direction,
                        },
                        Event::AbilityStarted { .. } | Event::ItemStarted { .. } => {
                            SpanKind::OwnWork
                        }
                        _ => continue,
                    };
                    self.open(observer, tick, kind, frame, true);
                }
                OutcomeFact::Combat { event, .. } => self.combat(event, tick, frame),
                _ => {}
            }
        }
        // The observer's latest span ends at its reported recovery once no
        // preparation remains; before that, its end is still unknown.
        if let Some(status) = &facts.status {
            if let Some(span) = self
                .spans
                .iter_mut()
                .rev()
                .find(|span| span.actor == observer)
            {
                if !matches!(span.end, SpanEnd::Exact(end) if end <= tick)
                    && status.preparation_remaining.is_none()
                    && span.start <= tick
                {
                    span.end = SpanEnd::Exact(tick.saturating_add(status.recovery_remaining));
                }
            }
        }
    }

    fn combat(&mut self, event: &CombatEventView, tick: u64, frame: u64) {
        let flight = |flights: &[Flight], caster: Option<ActorTarget>| {
            caster.and_then(|caster| {
                flights
                    .iter()
                    .rposition(|flight| flight.caster == Some(caster) && flight.tick == tick)
            })
        };
        match event {
            CombatEventView::Effect {
                caster,
                tick: effect_tick,
                delivery,
                segments,
                ..
            } => self.flights.push(Flight {
                caster: *caster,
                tick: *effect_tick,
                delivery: *delivery,
                segments: segments.clone(),
                frame,
            }),
            CombatEventView::Ability {
                caster,
                target,
                outcome,
                strike,
                ..
            } => {
                if let Some(caster) = caster {
                    if let Some(span) = self
                        .spans
                        .iter_mut()
                        .rev()
                        .find(|span| span.actor == *caster && span.start <= tick)
                    {
                        span.strikes.push(StrikeMark {
                            tick,
                            ordinal: *strike,
                        });
                    }
                }
                if let Some(target) = target {
                    let linked = flight(&self.flights, *caster);
                    self.react(*target, tick, outcome_kind(*outcome), linked, frame);
                }
            }
            CombatEventView::UnseenAbility {
                caster,
                target,
                outcome,
            } => {
                let linked = flight(&self.flights, *caster);
                self.react(*target, tick, outcome_kind(*outcome), linked, frame);
            }
            CombatEventView::AnonymousAbility {
                caster,
                target: Some(target),
                outcome,
            } => {
                let linked = flight(&self.flights, *caster);
                self.react(*target, tick, outcome_kind(*outcome), linked, frame);
            }
            CombatEventView::Died { actor, .. } => {
                self.close(*actor, tick);
                self.react(*actor, tick, ReactionKind::Death, None, frame);
            }
            _ => {}
        }
    }

    fn react(
        &mut self,
        target: ActorTarget,
        tick: u64,
        kind: ReactionKind,
        flight: Option<usize>,
        frame: u64,
    ) {
        self.reactions.push(Reaction {
            target,
            tick,
            kind,
            flight,
            frame,
        });
    }
}

fn outcome_kind(outcome: AbilityOutcome) -> ReactionKind {
    match outcome {
        AbilityOutcome::Applied => ReactionKind::Hit,
        AbilityOutcome::Unaffected => ReactionKind::Unaffected,
        AbilityOutcome::Miss => ReactionKind::Miss,
    }
}
