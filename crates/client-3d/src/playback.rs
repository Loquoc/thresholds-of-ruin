//! Presentation timing for the shared beat timeline.
//!
//! The network worker attaches each update's timeline spans to its view. Here
//! they become timed body paths and scheduled happenings on one clock, so
//! overlapping actions overlap and chained steps walk continuously. Ticks map to
//! seconds at a fixed rate with idle gaps compressed; nothing is placed in the
//! past, and fast-forward makes everything already scheduled due at once.
use crate::{ActorBody, Game, Happening};
use bevy::prelude::*;
use std::collections::HashMap;
use tor_client_3d_model::{playback::TickClock, Activity, View};
use tor_client_common::compositor::{ActorSpan, SpanKind};

/// A body's timed walk between two authoritative cells.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Path {
    pub from: Vec3,
    pub to: Vec3,
    pub start: f64,
    pub end: f64,
    /// The timeline span that owns this path, so a later end can retime it.
    pub span: (u64, usize),
    /// Fast-forward count when the path began; a later skip finishes it.
    pub skips: u64,
}

impl Path {
    /// Where the body is at `now`, and whether the walk has finished.
    pub fn at(&self, now: f64, skips: u64) -> (Vec3, bool) {
        if skips != self.skips || now >= self.end || self.end <= self.start {
            return (self.to, true);
        }
        let t = ((now - self.start) / (self.end - self.start)).clamp(0.0, 1.0) as f32;
        (self.from.lerp(self.to, t), false)
    }
}

/// The scene's playback state.
pub(crate) struct Playback {
    clock: TickClock,
    /// Presentation seconds now; updated every frame before anything is timed.
    pub now: f64,
    generation: u64,
    starts: HashMap<usize, f64>,
    scheduled: Vec<(f64, Happening)>,
    pub skips: u64,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            clock: TickClock::standard(),
            now: 0.0,
            generation: 0,
            starts: HashMap::new(),
            scheduled: Vec::new(),
            skips: 0,
        }
    }
}

/// How far a client plan may run ahead of what is shown.
const PLAN_LEAD_SECONDS: f64 = 1.5;

/// An open span plays for one base step until its end is disclosed.
const OPEN_SPAN_TICKS: u64 = 100;

/// One timed span, ready to apply to a body.
pub(crate) struct Timed {
    pub span: ActorSpan,
    pub key: (u64, usize),
    pub start: f64,
    pub end: f64,
}

impl Playback {
    /// Whether everything scheduled has played, so input may proceed.
    pub fn drained(&self) -> bool {
        self.scheduled.is_empty() && self.clock.drained(self.now)
    }

    /// Whether the client's own plan may send its next step: presentation is
    /// no more than the bounded lead behind the simulation.
    pub fn within_lead(&self) -> bool {
        self.clock.ahead(self.now) <= PLAN_LEAD_SECONDS
    }

    /// Make everything already scheduled due now.
    pub fn skip(&mut self) {
        self.clock.skip(self.now);
        for (due, _) in &mut self.scheduled {
            *due = due.min(self.now);
        }
        self.skips += 1;
    }

    /// A replacement scope starts timing afresh.
    pub fn reset(&mut self) {
        self.clock.reset();
        self.starts.clear();
        self.scheduled.clear();
        self.skips += 1;
    }

    /// Time this view's spans, returning those that move a body.
    pub fn time_spans(&mut self, activity: &Activity) -> Vec<Timed> {
        if activity.generation != self.generation {
            self.generation = activity.generation;
            self.starts.clear();
        }
        let mut timed = Vec::new();
        for (index, span) in &activity.spans {
            let start = *self
                .starts
                .entry(*index)
                .or_insert_with(|| self.clock.place(span.start, self.now));
            let end_tick = span
                .end
                .tick()
                .unwrap_or(span.start + OPEN_SPAN_TICKS)
                .max(span.start);
            self.clock.cover(end_tick);
            let end = self.clock.span(start, end_tick - span.start);
            self.clock.reserve(end);
            if matches!(span.kind, SpanKind::Step { .. } | SpanKind::OwnStep { .. }) {
                timed.push(Timed {
                    span: span.clone(),
                    key: (activity.generation, *index),
                    start,
                    end,
                });
            }
        }
        timed
    }

    /// Schedule this update's happenings for its tick, leading by `lead`
    /// seconds so a strike's impact frame lands on the tick itself.
    pub fn schedule(&mut self, activity: &Activity, events: Vec<Happening>, lead: f64) {
        if events.is_empty() {
            return;
        }
        let at = activity
            .tick
            .map_or(self.now, |tick| self.clock.place(tick, self.now));
        self.clock.reserve(at);
        let due = (at - lead).max(self.now);
        self.scheduled
            .extend(events.into_iter().map(|event| (due, event)));
    }

    /// Happenings whose time has come, in the order they were scheduled.
    pub fn release(&mut self) -> Vec<Happening> {
        let now = self.now;
        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.scheduled)
            .into_iter()
            .partition(|(at, _)| *at <= now);
        self.scheduled = later;
        due.into_iter().map(|(_, event)| event).collect()
    }
}
/// Advance the clock and release due happenings before they are acted out.
pub(crate) fn release(time: Res<Time>, mut game: ResMut<Game>) {
    game.playback.now = time.elapsed_secs_f64();
    let due = game.playback.release();
    game.pending.extend(due);
}

/// Before a view moves bodies: the goals they were walking from.
pub(crate) fn walking_from(
    view: &View,
    game: &Game,
    bodies: &Query<&mut ActorBody>,
) -> HashMap<tor_protocol::ActorTarget, Vec3> {
    view.activity
        .spans
        .iter()
        .filter(|(_, span)| matches!(span.kind, SpanKind::Step { .. } | SpanKind::OwnStep { .. }))
        .filter_map(|(_, span)| {
            let entity = game.actors.get(&span.actor)?;
            Some((span.actor, bodies.get(*entity).ok()?.goal))
        })
        .collect()
}

/// After a view placed bodies: walk each stepping body from its previous goal.
/// A step whose span was retimed keeps walking with its new end; a goal change
/// without a step (a teleport or resynchronization) keeps the ordinary ease.
pub(crate) fn start_walks(
    game: &Game,
    timed: Vec<Timed>,
    from: &HashMap<tor_protocol::ActorTarget, Vec3>,
    bodies: &mut Query<&mut ActorBody>,
) {
    for timed in timed {
        let Some(entity) = game.actors.get(&timed.span.actor) else {
            continue;
        };
        let Ok(mut body) = bodies.get_mut(*entity) else {
            continue;
        };
        if let Some(path) = &mut body.path {
            if path.span == timed.key {
                path.end = timed.end;
                continue;
            }
        }
        let Some(start) = from.get(&timed.span.actor).copied() else {
            continue;
        };
        if start.distance_squared(body.goal) < 1e-6 {
            continue;
        }
        body.path = Some(Path {
            from: start,
            to: body.goal,
            start: timed.start,
            end: timed.end,
            span: timed.key,
            skips: game.playback.skips,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tor_client_common::compositor::SpanEnd;
    use tor_protocol::{ActorTarget, Direction};

    fn span(start: u64, end: SpanEnd) -> ActorSpan {
        ActorSpan {
            actor: ActorTarget::from_digest([3; 32]),
            kind: SpanKind::OwnStep {
                direction: Direction::East,
            },
            start,
            end,
            strikes: vec![],
            frame: 1,
        }
    }

    fn activity(tick: u64, spans: Vec<(usize, ActorSpan)>) -> Activity {
        Activity {
            generation: 0,
            tick: Some(tick),
            spans,
            ended: None,
        }
    }

    #[test]
    fn a_path_walks_steadily_and_a_skip_finishes_it() {
        let path = Path {
            from: Vec3::ZERO,
            to: Vec3::X * 2.0,
            start: 1.0,
            end: 2.0,
            span: (0, 0),
            skips: 4,
        };
        assert_eq!(path.at(1.5, 4), (Vec3::X, false));
        assert_eq!(path.at(0.5, 4), (Vec3::ZERO, false), "waits at its start");
        assert_eq!(path.at(2.0, 4), (Vec3::X * 2.0, true));
        assert_eq!(
            path.at(1.2, 5),
            (Vec3::X * 2.0, true),
            "a later skip finishes it"
        );
    }

    #[test]
    fn chained_steps_are_timed_back_to_back_and_input_waits_for_them() {
        let mut playback = Playback {
            now: 10.0,
            ..Playback::default()
        };
        let first = playback.time_spans(&activity(0, vec![(0, span(0, SpanEnd::Exact(100)))]));
        let second = playback.time_spans(&activity(100, vec![(1, span(100, SpanEnd::Exact(200)))]));
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        // The second step begins exactly where the first ends: one walk.
        assert!((first[0].end - second[0].start).abs() < 1e-9);
        assert!((second[0].end - second[0].start - 0.25).abs() < 1e-9);
        assert!(!playback.drained(), "input waits for the walk");
        assert!(playback.within_lead(), "a plan may keep walking");
        playback.skip();
        assert!(playback.drained(), "fast-forward catches up");
    }

    #[test]
    fn happenings_wait_for_their_tick_less_the_impact_lead() {
        let mut playback = Playback {
            now: 5.0,
            ..Playback::default()
        };
        playback.time_spans(&activity(0, vec![(0, span(0, SpanEnd::Exact(400)))]));
        let died = Happening::Died(ActorTarget::from_digest([9; 32]));
        playback.schedule(&activity(400, vec![]), vec![died], 0.2);
        // 400 covered ticks play for a second after the lookahead; the blow
        // starts 0.2 s early so its impact lands on the tick.
        playback.now = 5.0 + 0.12 + 1.0 - 0.21;
        assert!(playback.release().is_empty());
        playback.now += 0.02;
        assert_eq!(playback.release().len(), 1);
        assert!(playback.release().is_empty(), "released once");
    }
}
