//! Presentation timing for the shared beat timeline.
//!
//! The network worker attaches each update's timeline spans to its view. Here
//! they become timed body paths and scheduled happenings on one clock, so
//! overlapping actions overlap and chained steps walk continuously. Ticks map to
//! seconds at a fixed rate with idle gaps compressed; nothing is placed in the
//! past, and fast-forward makes everything already scheduled due at once.
//!
//! Three parts cooperate:
//! - [`Walk`] queues each body's timed legs and blends consecutive ones into
//!   one walk.
//! - [`Governor`] sets the presentation pace from how fast the server supplies
//!   the timeline and how late it discloses steps.
//! - [`StepPaces`] estimates how fast other actors walk, since their step ends
//!   are only known once their next activity is.
use crate::{ActorBody, Game, Happening};
use bevy::prelude::*;
use std::collections::{HashMap, VecDeque};
use tor_client_3d_model::{playback::TickClock, Activity, View};
use tor_client_common::compositor::{ActorSpan, SpanKind};
use tor_protocol::ActorTarget;

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
    /// An own outcome can disclose this step ahead of its picture.
    pub own_frame: Option<u64>,
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

/// Legs this close together, in presentation seconds, are one walk: a leg
/// that starts shortly after the last one ends stretches it to meet, a walk
/// that has only just ended resumes at once, and the observer's own legs
/// chain from where the last one ended. The stop counter counts exactly such
/// gaps that still showed as a pause.
const CONTINUE_SECONDS: f64 = 0.5;

/// How many walked spans a body remembers for late end reports.
const KNOWN_SPANS: usize = 16;

/// A body's queued walk: each disclosed step is a leg that begins where the
/// previous one ends. A step disclosed while an earlier one is still playing
/// waits its turn instead of replacing it, so chained steps (travel, a held
/// key) walk without stopping at each cell.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Walk {
    legs: VecDeque<Path>,
    /// Spans recently walked, so a late report of a finished one is known.
    known: VecDeque<(u64, usize)>,
    finished: Option<FinishedLeg>,
    /// When the walk was last shown, so a stretch keeps the body in place.
    shown_at: Option<f64>,
    /// Previous shown time when this frame continues the same timed leg.
    continuous_since: Option<f64>,
    /// Legs that have begun playing, for diagnostics.
    pub legs_walked: u32,
    /// Pauses shorter than [`CONTINUE_SECONDS`] between walked legs: a step
    /// that stopped at its cell instead of flowing into the next. For
    /// diagnostics.
    pub stops: u32,
    /// When the body last stood on a leg, and whether it did last frame.
    walked_at: Option<f64>,
    walking_now: bool,
    /// Whether the front leg has been counted as begun.
    started: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FinishedLeg {
    to: Vec3,
    end: f64,
    own_frame: Option<u64>,
}

/// Where a walking body is this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Pose {
    /// On a leg at this point.
    At(Vec3),
    /// Waiting for the next leg to begin: stay where it is.
    Hold,
    /// Nothing left to walk: ease toward the goal.
    Free,
}

impl Walk {
    /// Queue a leg after the ones already playing. A later report of a known
    /// span only retimes its end.
    pub fn push(&mut self, leg: Path) {
        if self.retime(leg.span, leg.end) {
            return;
        }
        if self.known.len() == KNOWN_SPANS {
            self.known.pop_front();
        }
        self.known.push_back(leg.span);
        let leg = self.admit(leg);
        self.queue(leg);
    }

    /// A leg never plays from the past: one disclosed too late to follow on
    /// starts now instead of jumping into its middle, and a walk that has
    /// only just ended resumes as soon as its next leg arrives instead of
    /// waiting out the lookahead. Either way the leg keeps its length.
    fn admit(&self, mut leg: Path) -> Path {
        let Some(now) = self.shown_at else {
            return leg;
        };
        let resuming = self.legs.is_empty()
            && self
                .finished
                .is_some_and(|leg| now - leg.end < CONTINUE_SECONDS);
        let due_soon = leg.start > now && leg.start - now <= CONTINUE_SECONDS;
        if leg.start < now || (resuming && due_soon) {
            let length = (leg.end - leg.start).max(0.0);
            leg.start = now;
            leg.end = now + length;
        }
        leg
    }

    /// Append a leg to the walk. Another actor's step end is a guess until
    /// its next step is known, and its pace varies: a leg that begins shortly
    /// after the last one ends continues the walk, so the last one stretches
    /// to meet it. A leg that begins before the last one ends waits for it.
    fn queue(&mut self, mut leg: Path) {
        let gap = self
            .legs
            .back()
            .filter(|last| last.skips == leg.skips)
            .map(|last| leg.start - last.end);
        if gap.is_some_and(|gap| gap > 0.0 && gap <= CONTINUE_SECONDS) {
            self.stretch_last(leg.start);
        }
        if let Some(last) = self.legs.back() {
            if leg.skips == last.skips && leg.start < last.end {
                let length = leg.end - leg.start;
                leg.start = last.end;
                leg.end = leg.end.max(leg.start + length);
            }
        }
        self.legs.push_back(leg);
    }

    /// End the last queued leg at `end` instead. A leg already playing
    /// continues from where it is shown, so its pace bends without a jump; a
    /// leg that has visibly finished keeps its end.
    fn stretch_last(&mut self, end: f64) {
        let shown = self.shown_at;
        let Some(last) = self.legs.back_mut() else {
            return;
        };
        match shown {
            Some(now) if now >= last.end => return,
            Some(now) if now > last.start => {
                last.from = last.at(now, last.skips).0;
                last.start = now;
            }
            _ => {}
        }
        last.end = end;
    }

    /// Where a body's next chained leg begins: the end of the last one, while
    /// the walk is still going or only just ended. A body at rest starts from
    /// its authoritative cell instead, once its picture has caught up.
    pub fn origin(&self, now: f64, picture: u64) -> Option<Vec3> {
        self.legs
            .back()
            .map(|leg| leg.to)
            .or_else(|| self.awaiting_picture(picture))
            .or_else(|| {
                self.finished
                    .filter(|leg| now - leg.end < CONTINUE_SECONDS)
                    .map(|leg| leg.to)
            })
    }

    /// Keep a completed own step's endpoint until the picture includes or
    /// supersedes its outcome. Another actor's goal always comes from the picture.
    pub fn awaiting_picture(&self, picture: u64) -> Option<Vec3> {
        self.finished
            .filter(|leg| leg.own_frame.is_some_and(|frame| picture < frame))
            .map(|leg| leg.to)
    }

    /// A later report of a span this body already walks: retime its end if it
    /// is still queued. Returns whether the span was already known.
    pub fn retime(&mut self, span: (u64, usize), end: f64) -> bool {
        if let Some(queued) = self.legs.iter_mut().find(|queued| queued.span == span) {
            queued.end = end.max(queued.start);
            return true;
        }
        self.known.contains(&span)
    }

    /// The pose at `now`, dropping finished legs. A fast-forward finishes all.
    pub fn at(&mut self, now: f64, skips: u64) -> Pose {
        // The current front must have been shown already and must still own
        // this entire interval. Starting, finishing, or skipping a leg does
        // not establish continuity even if the new endpoint has a timed pose.
        self.continuous_since = self.shown_at.filter(|previous| {
            self.started
                && *previous < now
                && self.legs.front().is_some_and(|leg| {
                    leg.skips == skips && leg.start <= *previous && now < leg.end
                })
        });
        self.shown_at = Some(now);
        let pose = self.pose(now, skips);
        let walking = matches!(pose, Pose::At(_));
        if walking
            && !self.walking_now
            && self.walked_at.is_some_and(|at| now - at < CONTINUE_SECONDS)
        {
            self.stops += 1;
        }
        if walking {
            self.walked_at = Some(now);
        }
        self.walking_now = walking;
        pose
    }

    fn pose(&mut self, now: f64, skips: u64) -> Pose {
        while let Some(leg) = self.legs.front() {
            let (at, done) = leg.at(now, skips);
            if done {
                self.finished = Some(FinishedLeg {
                    to: leg.to,
                    end: leg.end,
                    own_frame: leg.own_frame,
                });
                self.legs.pop_front();
                self.started = false;
            } else if now < leg.start {
                return Pose::Hold;
            } else {
                if !self.started {
                    self.started = true;
                    self.legs_walked += 1;
                }
                return Pose::At(at);
            }
        }
        Pose::Free
    }

    /// Was this large displacement actually sampled along one timed leg?
    pub(crate) fn continues_between(&self, from: Vec3, to: Vec3) -> bool {
        let (Some(previous), Some(now), Some(leg)) =
            (self.continuous_since, self.shown_at, self.legs.front())
        else {
            return false;
        };
        // A retime or an out-of-band root transform need not match the path.
        // Recompute only for a displacement the animation might call a snap.
        const TOLERANCE_SQUARED: f32 = 1e-6;
        leg.at(previous, leg.skips).0.distance_squared(from) < TOLERANCE_SQUARED
            && leg.at(now, leg.skips).0.distance_squared(to) < TOLERANCE_SQUARED
    }

    /// Whether a leg is still queued or playing.
    pub fn walking(&self) -> bool {
        !self.legs.is_empty()
    }
}

/// The presentation pace this frame, as a fraction of real time: 1 plays the
/// timeline as timed, less stretches every animation to match a server that
/// supplies it more slowly. Frame-time animation scales its step by it.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub(crate) struct Pace(pub f32);

impl Default for Pace {
    fn default() -> Self {
        Self(1.0)
    }
}

impl Pace {
    /// A frame's step, in presentation seconds.
    pub fn of(pace: Option<Res<Pace>>, dt: f32) -> f32 {
        dt * pace.map_or(1.0, |pace| pace.0)
    }
}

/// The slowest presentation plays while it waits on the server: half speed,
/// so nothing lasts more than twice its timed length.
const MIN_RATE: f64 = 0.5;

/// Weight of the newest sample in the moving averages.
const AVERAGE_WEIGHT: f64 = 0.3;

/// Arrivals further apart than this, in real seconds, follow a rest: the
/// averages restart instead of reading it as a slow server.
const SUPPLY_RESET_SECONDS: f64 = 1.5;

/// Time constants with which the pace eases toward its target: it slows
/// promptly when the server falls behind and recovers gently, so a starving
/// stream is absorbed early without the pace hunting up and down.
const PACE_SLOW_SECONDS: f64 = 0.15;
const PACE_RECOVER_SECONDS: f64 = 0.5;

/// How long a late disclosure keeps slowing presentation, and the lateness
/// that slows it fully: steps arriving late slow presentation until they
/// arrive in time, which builds a buffer that full pace then holds.
const LATENESS_MEMORY_SECONDS: f64 = 1.0;
const LATENESS_FULL_SLOW_SECONDS: f64 = 0.2;

/// The most buffer, beyond the lookahead, that lateness may build.
const MAX_BUFFER_SECONDS: f64 = 0.4;

/// More scheduled presentation than this ahead of now is a backlog: play at
/// full pace to catch up.
const BACKLOG_SECONDS: f64 = 1.0;

fn average(mean: Option<f64>, value: f64) -> f64 {
    mean.map_or(value, |mean| mean + AVERAGE_WEIGHT * (value - mean))
}

/// Paces presentation to the server while it is still sending a beat. Two
/// signals set how much slower than real time to play, and the larger wins:
/// - **supply:** moving averages of how much presentation each arrival adds
///   and how often arrivals come. A server that supplies less than it plays
///   stretches playback to match, so the client never runs dry.
/// - **lateness:** how late steps have lately been disclosed. An update
///   discloses another actor's step only once the server has passed it, so
///   slowing briefly builds a buffer that lets later steps play on time.
///
/// The pace eases toward that target and never drops below [`MIN_RATE`].
#[derive(Clone, Debug, Default)]
struct Governor {
    /// Real seconds at the last frame; presentation advances from it.
    wall: Option<f64>,
    /// How much slower than real time presentation plays: 0 is full pace, at
    /// most `1 - MIN_RATE`.
    slow: f64,
    /// When timeline content last arrived (real seconds), and moving averages
    /// of the time between arrivals and of the presentation each one added.
    supplied_at: Option<f64>,
    supply_interval: Option<f64>,
    supply_seconds: Option<f64>,
    /// The earliest start of content newly timed since the last supply.
    fresh: Option<f64>,
    /// How late steps have lately been disclosed, in presentation seconds:
    /// the latest lateness, decaying over `LATENESS_MEMORY_SECONDS`.
    lateness: f64,
}

impl Governor {
    fn pace(&self) -> f64 {
        1.0 - self.slow
    }

    /// Advance real time to `wall` and return the presentation seconds it
    /// covers. While the server is still sending (`open`), the pace eases
    /// toward the slower of its two signals; otherwise back to full pace.
    fn advance(&mut self, wall: f64, open: bool, clock: &TickClock, now: f64) -> f64 {
        let elapsed = self.wall.map_or(0.0, |at| (wall - at).max(0.0));
        self.wall = Some(wall);
        self.lateness *= (-elapsed / LATENESS_MEMORY_SECONDS).exp();
        let target = if open {
            (1.0 - self.supply_rate(clock, now)).max(self.buffer_slowing(clock, now))
        } else {
            0.0
        };
        let ease = if target > self.slow {
            PACE_SLOW_SECONDS
        } else {
            PACE_RECOVER_SECONDS
        };
        self.slow += (target - self.slow) * (1.0 - (-elapsed / ease).exp());
        self.slow = self.slow.clamp(0.0, 1.0 - MIN_RATE);
        elapsed * self.pace()
    }

    /// The pace that matches the supply: presentation arriving slower than it
    /// plays stretches it, down to `MIN_RATE`; a backlog plays at full pace.
    fn supply_rate(&self, clock: &TickClock, now: f64) -> f64 {
        match (self.supply_interval, self.supply_seconds) {
            (Some(interval), Some(seconds))
                if interval > 0.0 && clock.ahead(now) <= BACKLOG_SECONDS =>
            {
                (seconds / interval).clamp(MIN_RATE, 1.0)
            }
            _ => 1.0,
        }
    }

    /// How much to slow so presentation trails the stream by as much as steps
    /// have lately arrived late, until the buffer reaches its cap.
    fn buffer_slowing(&self, clock: &TickClock, now: f64) -> f64 {
        if clock.lead(now) >= MAX_BUFFER_SECONDS + clock.lookahead {
            return 0.0;
        }
        (self.lateness / LATENESS_FULL_SLOW_SECONDS).clamp(0.0, 1.0) * (1.0 - MIN_RATE)
    }

    /// A step was placed this late.
    fn late(&mut self, late: f64) {
        self.lateness = self.lateness.max(late);
    }

    /// New content was timed to start at `start`.
    fn timed(&mut self, start: f64) {
        self.fresh = Some(self.fresh.map_or(start, |fresh| fresh.min(start)));
    }

    /// A view was timed: fold the presentation it added past `before` (the
    /// previous horizon) up to `horizon` into the supply averages. Only new
    /// content counts from where it starts: the lookahead gap before content
    /// that arrives late is waiting, not supply.
    fn supplied(&mut self, before: f64, horizon: f64, now: f64) {
        let from = self
            .fresh
            .take()
            .map_or(before, |fresh| fresh.max(before))
            .max(now);
        let added = horizon - from;
        if added <= 0.0 {
            return;
        }
        let wall = self.wall.unwrap_or(now);
        if let Some(previous) = self.supplied_at {
            let interval = (wall - previous).max(0.0);
            if interval > SUPPLY_RESET_SECONDS {
                self.supply_interval = None;
                self.supply_seconds = None;
            } else {
                self.supply_interval = Some(average(self.supply_interval, interval));
                self.supply_seconds = Some(average(self.supply_seconds, added));
            }
        }
        self.supplied_at = Some(wall);
    }

    /// A replacement scope: forget what the old stream supplied, but keep
    /// easing from the current pace.
    fn restart(&mut self) {
        *self = Self {
            wall: self.wall,
            slow: self.slow,
            ..Self::default()
        };
    }
}

/// Steps by another actor further apart than this were separated by a rest:
/// the gap says nothing about its walking pace.
const MAX_PACE_TICKS: u64 = 400;

/// How fast other actors are seen to walk, in ticks per step. Another actor's
/// step ends where its next activity starts, which is unknown while it walks;
/// watching its steps is the only guess, never a disclosed duration.
#[derive(Clone, Debug, Default)]
struct StepPaces {
    /// Each actor's last disclosed step start.
    stepped: HashMap<ActorTarget, u64>,
    /// Each actor's last observed walking pace, kept across its rests.
    walking: HashMap<ActorTarget, u64>,
    /// A moving average over every actor's walking pace.
    typical: Option<f64>,
}

impl StepPaces {
    /// Note a newly disclosed step by `actor` starting at `start`, and return
    /// the pace to walk it at: the ticks since that actor's step before it.
    /// A first step, or the first after a rest, walks at the pace this actor
    /// last walked, else at the pace other actors are seen to walk.
    fn step(&mut self, actor: ActorTarget, start: u64) -> Option<u64> {
        let previous = self.stepped.insert(actor, start);
        let observed = previous
            .and_then(|previous| start.checked_sub(previous))
            .filter(|pace| (1..=MAX_PACE_TICKS).contains(pace));
        if let Some(pace) = observed {
            self.walking.insert(actor, pace);
            self.typical = Some(average(self.typical, pace as f64));
        }
        observed
            .or_else(|| self.walking.get(&actor).copied())
            .or_else(|| self.typical.map(|mean| mean.round() as u64))
    }
}

/// An open span plays for one base step until its end is disclosed.
const OPEN_SPAN_TICKS: u64 = 100;

/// The longest a step of unknown pace walks once its end is known.
const UNPACED_STEP_TICKS: u64 = 2 * OPEN_SPAN_TICKS;

/// The tick at which a span's timed activity ends.
///
/// Another actor's step lasts its observed pace while its end is open, so
/// consecutive steps chain into one walk. Once its next activity is known, a
/// step that ends more than a quarter beyond its pace was followed by a rest:
/// it walks at about its pace and then stands, instead of crawling.
fn end_tick(span: &ActorSpan, pace: Option<u64>) -> u64 {
    let end = match (&span.kind, span.end.tick()) {
        (SpanKind::Step { .. }, Some(end)) => {
            end.min(span.start + pace.map_or(UNPACED_STEP_TICKS, |pace| pace + pace / 4))
        }
        (SpanKind::Step { .. }, None) => span.start + pace.unwrap_or(OPEN_SPAN_TICKS),
        (_, end) => end.unwrap_or(span.start + OPEN_SPAN_TICKS),
    };
    end.max(span.start)
}

fn moves_a_body(span: &ActorSpan) -> bool {
    matches!(span.kind, SpanKind::Step { .. } | SpanKind::OwnStep { .. })
}

/// How far a client plan may run ahead of what is shown.
const PLAN_LEAD_SECONDS: f64 = 1.5;

/// The scene's playback state.
#[derive(Default)]
pub(crate) struct Playback {
    clock: TickClock,
    /// Presentation seconds now; updated every frame before anything is timed.
    pub now: f64,
    pub skips: u64,
    governor: Governor,
    paces: StepPaces,
    /// The timeline generation the span records below belong to.
    generation: u64,
    /// Each timed span's start, and the walking pace it was given.
    spans: HashMap<usize, (f64, Option<u64>)>,
    scheduled: Vec<(f64, Happening)>,
    /// When a disclosed flight reaches each struck actor, in presentation seconds.
    struck: HashMap<ActorTarget, f64>,
}

/// One timed span, ready to apply to a body.
pub(crate) struct Timed {
    pub span: ActorSpan,
    pub key: (u64, usize),
    pub start: f64,
    pub end: f64,
}

impl Playback {
    /// Advance presentation to real time `wall`. While the server is still
    /// sending this beat (`open`), the governor may slow it to half speed
    /// instead of letting it stop.
    pub fn advance(&mut self, wall: f64, open: bool) {
        if self.governor.wall.is_none() {
            self.now = self.now.max(wall);
        }
        self.now += self.governor.advance(wall, open, &self.clock, self.now);
    }

    /// The pace presentation plays at now, as a fraction of real time.
    pub fn pace(&self) -> f64 {
        self.governor.pace()
    }

    /// The scheduled presentation's end.
    fn horizon(&self) -> f64 {
        self.now + self.clock.ahead(self.now)
    }

    /// Whether everything scheduled has played, so input may proceed.
    pub fn drained(&self) -> bool {
        self.scheduled.is_empty() && self.clock.drained(self.now)
    }

    /// Whether the client's own plan may send its next step: presentation is
    /// no more than the bounded lead behind the simulation.
    pub fn within_lead(&self) -> bool {
        self.clock.ahead(self.now) <= PLAN_LEAD_SECONDS
    }

    /// A disclosed flight reaches `actor` this many seconds from now.
    pub fn strike_at(&mut self, actor: ActorTarget, delay: f64) {
        let at = self.now + delay;
        self.clock.reserve(at);
        let entry = self.struck.entry(actor).or_insert(at);
        *entry = entry.max(at);
    }

    /// Seconds until the latest flight still on its way reaches `actor`.
    /// A hurt or death never plays before what caused it arrives.
    pub fn until_struck(&mut self, actor: ActorTarget) -> f32 {
        let now = self.now;
        self.struck.retain(|_, at| *at > now);
        self.struck.get(&actor).map_or(0.0, |at| (*at - now) as f32)
    }

    /// Make everything already scheduled due now.
    pub fn skip(&mut self) {
        self.clock.skip(self.now);
        for (due, _) in &mut self.scheduled {
            *due = due.min(self.now);
        }
        self.struck.clear();
        self.skips += 1;
    }

    /// A replacement scope starts timing afresh.
    pub fn reset(&mut self) {
        self.clock.reset();
        self.governor.restart();
        self.paces = StepPaces::default();
        self.spans.clear();
        self.scheduled.clear();
        self.struck.clear();
        self.skips += 1;
    }

    /// Time one view: its spans, then its happenings at its tick (leading by
    /// `lead` seconds so a strike's impact frame lands on the tick itself),
    /// then what it supplied, which paces playback. Spans are timed before
    /// the tick is placed, so their activity counts as covered rather than
    /// idle. Returns the spans that move a body.
    pub fn time_view(
        &mut self,
        activity: &Activity,
        events: Vec<Happening>,
        lead: f64,
    ) -> Vec<Timed> {
        let before = self.horizon();
        let timed = self.time_spans(activity);
        self.schedule(activity, events, lead);
        self.governor.supplied(before, self.horizon(), self.now);
        timed
    }

    fn time_spans(&mut self, activity: &Activity) -> Vec<Timed> {
        if activity.generation != self.generation {
            self.generation = activity.generation;
            self.spans.clear();
        }
        let mut timed = Vec::new();
        for (index, span) in &activity.spans {
            let (start, pace) = match self.spans.get(index) {
                Some(known) => *known,
                None => {
                    let known = self.first_timing(span);
                    self.spans.insert(*index, known);
                    known
                }
            };
            let end_tick = end_tick(span, pace);
            self.clock.cover(end_tick);
            let end = self.clock.span(start, end_tick - span.start);
            self.clock.reserve(end);
            if moves_a_body(span) {
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

    /// A newly disclosed span's start and walking pace. One disclosed after
    /// its moment would have played starts now: late arrival delays a walk,
    /// it never turns it into a snap.
    fn first_timing(&mut self, span: &ActorSpan) -> (f64, Option<u64>) {
        let placed = self.clock.place(span.start, self.now);
        if moves_a_body(span) {
            self.governor.late(placed.late);
        }
        let start = placed.at.max(self.now);
        self.governor.timed(start);
        let pace = match span.kind {
            SpanKind::Step { .. } => self.paces.step(span.actor, span.start),
            _ => None,
        };
        (start, pace)
    }

    fn schedule(&mut self, activity: &Activity, events: Vec<Happening>, lead: f64) {
        if events.is_empty() {
            return;
        }
        let at = activity
            .tick
            .map_or(self.now, |tick| self.clock.place(tick, self.now).at);
        self.clock.reserve(at);
        self.governor.timed(at);
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
pub(crate) fn release(
    time: Res<Time>,
    mut game: ResMut<Game>,
    pursuit: Option<Res<crate::interact::Pursuit>>,
    pace: Option<ResMut<Pace>>,
) {
    // The server is still sending this beat while a command is unanswered or
    // a clicked errand goes on: presentation then paces itself to its supply.
    let open = game.busy || pursuit.is_some_and(|pursuit| pursuit.active());
    game.playback.advance(time.elapsed_secs_f64(), open);
    if let Some(mut pace) = pace {
        pace.0 = game.playback.pace() as f32;
    }
    let due = game.playback.release();
    game.pending.extend(due);
}

/// Before a view moves bodies: the goals they were walking from.
pub(crate) fn walking_from(
    view: &View,
    game: &Game,
    bodies: &Query<&mut ActorBody>,
) -> HashMap<ActorTarget, Vec3> {
    view.activity
        .spans
        .iter()
        .filter(|(_, span)| moves_a_body(span))
        .filter_map(|(_, span)| {
            let entity = game.actors.get(&span.actor)?;
            Some((span.actor, bodies.get(*entity).ok()?.goal))
        })
        .collect()
}

/// The world-space displacement of one step in `direction`.
fn step_offset(direction: tor_protocol::Direction) -> Vec3 {
    let (x, y, z) = tor_client_3d_model::offset(direction);
    crate::world(IVec3::new(x, y, z))
}

/// Folded views can carry several new steps by one actor, but only its final
/// cell: walk them as one leg from the earliest start to the latest end. The
/// observer's own steps each disclose a direction, so each walks on its own.
fn new_walks(timed: impl IntoIterator<Item = Timed>) -> Vec<Timed> {
    let mut walks: Vec<Timed> = Vec::new();
    for step in timed {
        let own = matches!(step.span.kind, SpanKind::OwnStep { .. });
        match walks
            .iter_mut()
            .find(|walk| !own && walk.span.actor == step.span.actor)
        {
            Some(walk) => {
                walk.start = walk.start.min(step.start);
                if step.end >= walk.end {
                    walk.end = step.end;
                    walk.key = step.key;
                }
            }
            None => walks.push(step),
        }
    }
    walks
}

/// After a view placed bodies: walk each stepping body from its previous goal.
/// A step whose span was retimed keeps walking with its new end; a goal change
/// without a step (a teleport or resynchronization) keeps the ordinary ease.
pub(crate) fn start_walks(
    game: &Game,
    picture: u64,
    timed: Vec<Timed>,
    from: &HashMap<ActorTarget, Vec3>,
    bodies: &mut Query<&mut ActorBody>,
) {
    // A view re-reports an earlier step once its end is known: that only
    // retimes the leg already walking. What remains is new.
    let timed = timed.into_iter().filter(|step| {
        let Some(mut body) = game
            .actors
            .get(&step.span.actor)
            .and_then(|entity| bodies.get_mut(*entity).ok())
        else {
            return true;
        };
        !body.walk.retime(step.key, step.end)
    });
    for timed in new_walks(timed) {
        let Some(entity) = game.actors.get(&timed.span.actor) else {
            continue;
        };
        let Ok(mut body) = bodies.get_mut(*entity) else {
            continue;
        };
        // The observer's own step discloses its direction, while the picture
        // that shows where it went can arrive a step later. Chain own legs by
        // direction from where the last one ends; the goal catches up.
        let own = match timed.span.kind {
            SpanKind::OwnStep { direction } => Some(direction),
            _ => None,
        };
        let Some(start) = own
            .and_then(|_| body.walk.origin(game.playback.now, picture))
            .or_else(|| from.get(&timed.span.actor).copied())
        else {
            continue;
        };
        let to = match own {
            Some(direction) => start + step_offset(direction),
            None => body.goal,
        };
        if start.distance_squared(to) < 1e-6 {
            continue;
        }
        body.walk.push(Path {
            from: start,
            to,
            start: timed.start,
            end: timed.end,
            span: timed.key,
            skips: game.playback.skips,
            own_frame: own.map(|_| timed.span.frame),
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

    fn supply_rate(playback: &Playback) -> f64 {
        playback.governor.supply_rate(&playback.clock, playback.now)
    }

    fn activity(tick: u64, spans: Vec<(usize, ActorSpan)>) -> Activity {
        Activity {
            generation: 0,
            tick: Some(tick),
            spans,
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
            own_frame: None,
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

    /// Travel whose steps arrive every `interval` real seconds, timed as the
    /// scene times them, with the server still sending until the last
    /// arrives. Returns the playback, the own walk's positions per frame and
    /// the slowest pace reached.
    fn travel(interval: f64, steps: u64, paced: bool) -> (Playback, Walk, Vec<Option<f32>>, f64) {
        let mut playback = Playback::default();
        let mut walk = Walk::default();
        let (mut next, mut wall, mut slowest) = (0, 1.0, 1.0_f64);
        let mut walked = Vec::new();
        while wall < 1.0 + interval * steps as f64 + 4.0 {
            playback.advance(wall, paced && next < steps);
            while next < steps && 1.0 + next as f64 * interval <= wall {
                let start = next * 100;
                let timed = playback.time_view(
                    &activity(
                        start,
                        vec![(next as usize, span(start, SpanEnd::Exact(start + 100)))],
                    ),
                    Vec::new(),
                    0.0,
                );
                for step in timed {
                    walk.push(Path {
                        from: Vec3::X * next as f32,
                        to: Vec3::X * (next + 1) as f32,
                        start: step.start,
                        end: step.end,
                        span: step.key,
                        skips: 0,
                        own_frame: None,
                    });
                }
                next += 1;
            }
            slowest = slowest.min(playback.pace());
            walked.push(match walk.at(playback.now, 0) {
                Pose::At(at) => Some(at.x),
                _ => None,
            });
            wall += 1.0 / 60.0;
        }
        (playback, walk, walked, slowest)
    }

    #[test]
    fn a_starving_server_slows_presentation_to_its_supply_without_stopping() {
        // Steps arrive every 0.6 s but play in 0.325 s, each after the one
        // before has finished: at full pace the walk stops at nearly every cell.
        let (_, unpaced, ..) = travel(0.6, 16, false);
        assert!(unpaced.stops >= 12, "unpaced stops {}", unpaced.stops);
        let (_, walk, walked, slowest) = travel(0.6, 16, true);
        assert_eq!(walk.legs_walked, 16);
        assert!(
            walk.stops <= 2,
            "the paced walk stops only while its average forms: {}",
            walk.stops
        );
        assert!((MIN_RATE - 1e-9..0.85).contains(&slowest), "pace {slowest}");
        // Continuous while walking: never back, and never faster than the
        // timed pace. After the warm-up the walk never rests between legs.
        let most = 1.0 / 60.0 / tor_client_3d_model::playback::STANDARD_STEP_SECONDS + 1e-4;
        for pair in walked.windows(2) {
            if let [Some(before), Some(after)] = pair {
                let step = f64::from(after - before);
                assert!((-1e-6..=most).contains(&step), "step {step}");
            }
        }
        let first = walked.iter().position(Option::is_some).unwrap();
        let last = walked.iter().rposition(Option::is_some).unwrap();
        let resting = walked[first..=last]
            .iter()
            .enumerate()
            .filter(|(_, x)| x.is_none())
            .map(|(frame, _)| frame)
            .collect::<Vec<_>>();
        assert!(
            resting.iter().all(|frame| *frame < 120),
            "rests only in the first two seconds: {resting:?}"
        );
        assert_eq!(walked[last].map(|x| x.round()), Some(16.0));
    }

    #[test]
    fn a_very_slow_server_stretches_presentation_at_most_twofold() {
        let (playback, walk, _, slowest) = travel(1.0, 10, true);
        assert!((slowest - MIN_RATE).abs() < 0.03, "slowest {slowest}");
        assert!(slowest >= MIN_RATE - 1e-9);
        assert_eq!(walk.legs_walked, 10);
        assert!(
            (playback.pace() - 1.0).abs() < 1e-3,
            "full pace once it ends"
        );
    }

    #[test]
    fn a_prompt_server_keeps_full_pace() {
        let (_, walk, _, slowest) = travel(0.3, 12, true);
        assert!(slowest > 0.999, "slowest {slowest}");
        assert_eq!(walk.stops, 0);
    }

    #[test]
    fn the_supply_is_a_moving_average_and_the_pace_eases() {
        let mut playback = Playback::default();
        playback.advance(1.0, true);
        let feed = |playback: &mut Playback, wall: f64, step: u64| {
            playback.advance(wall, true);
            playback.time_view(
                &activity(
                    step * 100,
                    vec![(
                        step as usize,
                        span(step * 100, SpanEnd::Exact(step * 100 + 100)),
                    )],
                ),
                Vec::new(),
                0.0,
            );
        };
        for step in 0..6 {
            feed(&mut playback, 1.0 + step as f64 * 0.325, step);
        }
        assert!((supply_rate(&playback) - 1.0).abs() < 1e-6);
        // One late arrival moves the average only partway.
        feed(&mut playback, 1.0 + 5.0 * 0.325 + 0.65, 6);
        let interval = playback.governor.supply_interval.unwrap();
        assert!(
            (interval - (0.325 + AVERAGE_WEIGHT * 0.325)).abs() < 1e-6,
            "interval {interval}"
        );
        let target = supply_rate(&playback);
        assert!(target > 0.7 && target < 0.8, "target {target}");
        // The pace eases toward it instead of jumping.
        let start = playback.governor.wall.unwrap();
        playback.advance(start + 1.0 / 60.0, true);
        assert!(
            playback.pace() > 0.9,
            "one frame slows only partway: {}",
            playback.pace()
        );
        // Presentation then runs slower than real time, never stopping.
        let shown = playback.now;
        playback.advance(start + 2.0 / 60.0, true);
        let step = playback.now - shown;
        assert!(step > 0.0 && step < 1.0 / 60.0, "slowed step {step}");
        // It slows promptly and recovers gently: a quarter second covers most
        // of the slowing, but less than half of the way back.
        playback.advance(start + 2.0 / 60.0 + 0.25, true);
        let slowed = 1.0 - playback.pace();
        assert!(slowed > 0.75 * (1.0 - target), "slowed {slowed}");
        playback.advance(start + 2.0 / 60.0 + 0.5, false);
        assert!(1.0 - playback.pace() > 0.5 * slowed, "recovers gently");
        // Once the server has finished, it eases back to full pace.
        playback.advance(start + 5.0, false);
        assert!(playback.pace() > 0.999);
    }

    /// Travel at exactly real-time supply, where each update also discloses
    /// a mob step that began 60 ticks before its tick. Returns, per update,
    /// whether that mob step was disclosed too late to play at its moment.
    fn mob_steps_inside_updates(buffered: bool) -> Vec<bool> {
        let mob = ActorTarget::from_digest([8; 32]);
        let mut playback = Playback::default();
        let (mut wall, mut due, mut update) = (1.0, 1.0, 0_u64);
        let mut late = Vec::new();
        while update < 40 {
            playback.advance(wall, true);
            if !buffered {
                playback.governor.lateness = 0.0;
            }
            if wall >= due {
                let tick = 100 + update * 100;
                let mob_step = ActorSpan {
                    actor: mob,
                    ..mob_step(tick - 60, SpanEnd::Open)
                };
                let index = 2 * update as usize;
                let now = playback.now;
                let timed = playback.time_view(
                    &activity(
                        tick,
                        vec![
                            (index, span(tick, SpanEnd::Exact(tick + 100))),
                            (index + 1, mob_step),
                        ],
                    ),
                    Vec::new(),
                    0.0,
                );
                let start = timed
                    .iter()
                    .find(|step| step.span.actor == mob)
                    .unwrap()
                    .start;
                late.push(start <= now + 1e-9);
                update += 1;
                due += tor_client_3d_model::playback::STANDARD_STEP_SECONDS;
            }
            wall += 1.0 / 60.0;
        }
        late
    }

    #[test]
    fn steps_disclosed_late_build_a_buffer_so_later_ones_play_on_time() {
        // Recorded menagerie travel: each update covers one player step, and
        // a mob step early in that window is disclosed after its moment.
        // Supply pacing alone gains lag only slowly; slowing by lateness
        // builds the buffer within a few updates and then holds it.
        let on_time_from =
            |late: &[bool]| late.iter().rposition(|late| *late).map_or(0, |at| at + 1);
        let unbuffered = mob_steps_inside_updates(false);
        let buffered = mob_steps_inside_updates(true);
        let (slow, fast) = (on_time_from(&unbuffered), on_time_from(&buffered));
        assert!(
            slow >= 10,
            "without a buffer the steps stay late: {unbuffered:?}"
        );
        assert!(
            fast <= 5 && fast < slow,
            "a buffer builds and holds: {buffered:?}"
        );
        assert!(buffered.len() == 40);
    }

    #[test]
    fn a_backlog_plays_at_full_pace() {
        let mut playback = Playback::default();
        playback.advance(1.0, true);
        playback.governor.supply_interval = Some(0.6);
        playback.governor.supply_seconds = Some(0.325);
        assert!(supply_rate(&playback) < 0.6);
        playback.time_spans(&activity(0, vec![(0, span(0, SpanEnd::Exact(800)))]));
        assert!(playback.clock.ahead(playback.now) > BACKLOG_SECONDS);
        assert_eq!(supply_rate(&playback), 1.0, "behind by more than a second");
    }

    fn leg(from: f32, start: f64, end: f64, index: usize) -> Path {
        Path {
            from: Vec3::X * from,
            to: Vec3::X * (from + 1.0),
            start,
            end,
            span: (1, index),
            skips: 0,
            own_frame: None,
        }
    }

    #[test]
    fn only_matching_samples_inside_the_same_leg_prove_continuous_motion() {
        let mut walk = Walk::default();
        walk.push(leg(0.0, 1.0, 1.25, 0));
        walk.push(leg(1.0, 1.25, 1.5, 1));
        let (a, b, c) = (Vec3::X * 0.25, Vec3::X * 0.75, Vec3::X * 1.25);
        assert_eq!(walk.at(1.0625, 0), Pose::At(a));
        assert!(!walk.continues_between(Vec3::ZERO, a), "first sample");
        assert_eq!(walk.at(1.1875, 0), Pose::At(b));
        assert!(walk.continues_between(a, b));
        assert!(!walk.continues_between(Vec3::ZERO, b), "out-of-band start");
        assert!(!walk.continues_between(a, Vec3::X), "out-of-band end");
        assert_eq!(walk.at(1.3125, 0), Pose::At(c));
        assert!(!walk.continues_between(b, c), "different leg");
        assert_eq!(walk.at(1.375, 1), Pose::Free);
        assert!(!walk.continues_between(c, Vec3::X * 2.0), "fast-forward");
    }

    #[test]
    fn a_step_disclosed_mid_walk_queues_behind_it_without_a_stop() {
        // Travel discloses each step ahead of playback: the next leg arrives
        // while the current one is still walking and must not replace it.
        let mut walk = Walk::default();
        walk.push(leg(0.0, 1.0, 1.25, 0));
        assert_eq!(walk.at(1.125, 0), Pose::At(Vec3::X * 0.5));
        walk.push(leg(1.0, 1.25, 1.5, 1));
        assert_eq!(
            walk.at(1.125, 0),
            Pose::At(Vec3::X * 0.5),
            "the walk in progress keeps going"
        );
        // Sample across the hand-over: speed stays steady, never a hold.
        let mut previous = walk.at(1.125, 0);
        for frame in 1..=20 {
            let now = 1.125 + f64::from(frame) * 0.0125;
            let pose = walk.at(now, 0);
            let (Pose::At(at), Pose::At(before)) = (pose, previous) else {
                panic!("walk stopped at {now}: {pose:?}");
            };
            assert!(
                (at.x - before.x - 0.05).abs() < 1e-4,
                "steady pace at {now}: {} -> {}",
                before.x,
                at.x
            );
            previous = pose;
        }
        assert_eq!(walk.at(1.5, 0), Pose::Free, "the queue empties at its end");
        assert_eq!((walk.legs_walked, walk.stops), (2, 0));
    }

    #[test]
    fn a_closely_following_leg_stretches_the_playing_one_without_a_stop() {
        // Recorded menagerie travel: a mob's next step was disclosed well
        // before its current leg ended, but began 17 to 117 ticks after the
        // pace-guessed end, and the mob stood at its cell for that gap.
        let mut walk = Walk::default();
        walk.push(leg(0.0, 1.0, 1.3, 0));
        assert_eq!(walk.at(1.15, 0), Pose::At(Vec3::X * 0.5));
        walk.push(leg(1.0, 1.4, 1.7, 1));
        assert_eq!(walk.at(1.15, 0), Pose::At(Vec3::X * 0.5), "no jump");
        let mut previous = 0.5;
        let mut now = 1.15;
        while now < 1.69 {
            now += 1.0 / 60.0;
            let Pose::At(at) = walk.at(now, 0) else {
                panic!("the walk stopped at {now}");
            };
            assert!(at.x >= previous - 1e-6, "never back: {at:?} at {now}");
            previous = at.x;
        }
        assert_eq!(walk.at(1.4, 0), Pose::At(Vec3::X), "meets the next leg");
        assert_eq!((walk.legs_walked, walk.stops), (2, 0));
    }

    #[test]
    fn a_leg_disclosed_too_late_walks_from_its_start_instead_of_jumping() {
        // Recorded menagerie travel: a mob's step disclosed before its body
        // existed was queued later with a past start and jumped mid-cell.
        let mut walk = Walk::default();
        assert_eq!(walk.at(5.0, 0), Pose::Free);
        walk.push(leg(0.0, 4.0, 4.5, 0));
        assert_eq!(
            walk.at(5.0, 0),
            Pose::At(Vec3::ZERO),
            "starts from its start"
        );
        assert!((walk.legs[0].end - 5.5).abs() < 1e-9, "keeps its length");
    }

    #[test]
    fn a_walk_that_just_ended_resumes_when_its_next_leg_arrives() {
        // A leg arriving just after the walk ended no longer waits out the
        // lookahead before walking on: the stop is only as long as the delay.
        let mut walk = Walk::default();
        walk.push(leg(0.0, 1.0, 1.25, 0));
        assert!(matches!(walk.at(1.1, 0), Pose::At(_)));
        assert_eq!(walk.at(1.3, 0), Pose::Free);
        walk.push(leg(1.0, 1.42, 1.67, 1));
        assert_eq!(walk.at(1.3, 0), Pose::At(Vec3::X), "walks on at once");
        assert!((walk.legs[0].end - 1.55).abs() < 1e-9, "keeps its length");
    }

    #[test]
    fn a_leg_after_a_rest_leaves_the_previous_one_alone() {
        let mut walk = Walk::default();
        walk.push(leg(0.0, 1.0, 1.3, 0));
        assert!(matches!(walk.at(1.1, 0), Pose::At(_)));
        walk.push(leg(1.0, 2.0, 2.3, 1));
        assert_eq!(walk.legs[0].end, 1.3, "it keeps its own end");
        assert_eq!(walk.at(1.5, 0), Pose::Hold, "and rests at its cell");
    }

    #[test]
    fn a_brief_pause_between_legs_counts_as_a_stop() {
        let mut walk = Walk::default();
        walk.push(leg(0.0, 1.0, 1.25, 0));
        assert!(matches!(walk.at(1.1, 0), Pose::At(_)));
        assert_eq!(walk.at(1.3, 0), Pose::Free);
        walk.push(leg(1.0, 1.4, 1.65, 1));
        assert!(matches!(walk.at(1.45, 0), Pose::At(_)));
        assert_eq!((walk.legs_walked, walk.stops), (2, 1));
        // A walk begun long after the last one is a new walk, not a stop.
        assert_eq!(walk.at(1.7, 0), Pose::Free);
        walk.push(leg(2.0, 5.0, 5.25, 2));
        assert!(matches!(walk.at(5.1, 0), Pose::At(_)));
        assert_eq!((walk.legs_walked, walk.stops), (3, 1));
    }

    #[test]
    fn an_overlapping_leg_starts_where_the_last_ends_and_a_retime_moves_only_its_end() {
        let mut walk = Walk::default();
        walk.push(leg(0.0, 1.0, 1.25, 0));
        walk.push(leg(1.0, 1.1, 1.35, 1));
        assert_eq!(walk.legs[1].start, 1.25);
        assert!((walk.legs[1].end - 1.5).abs() < 1e-9, "keeps its length");
        walk.push(leg(1.0, 1.25, 1.6, 1));
        assert_eq!(
            walk.legs.len(),
            2,
            "a later report of a queued span retimes it"
        );
        assert!((walk.legs[1].end - 1.6).abs() < 1e-9);
        assert_eq!(walk.legs[1].start, 1.25);
        assert_eq!(
            walk.at(1.1, 1),
            Pose::Free,
            "a fast-forward finishes every leg"
        );
    }

    fn timed(index: usize, start: f64, end: f64) -> Timed {
        Timed {
            span: span(0, SpanEnd::Open),
            key: (1, index),
            start,
            end,
        }
    }

    #[test]
    fn a_re_reported_step_only_retimes_its_leg_and_the_new_step_walks_alone() {
        // Recorded travel: each view repeats the previous step with its end
        // now known, beside the next step. Folding them together walked one
        // cell over two steps' time.
        let mut walk = Walk::default();
        walk.push(leg(0.0, 6.143, 6.393, 0));
        let view = vec![timed(0, 6.143, 6.393), timed(1, 6.393, 6.643)];
        let fresh: Vec<_> = view
            .into_iter()
            .filter(|step| !walk.retime(step.key, step.end))
            .collect();
        let walks = new_walks(fresh);
        assert_eq!(walks.len(), 1);
        assert_eq!(walks[0].key, (1, 1));
        assert_eq!(
            (walks[0].start, walks[0].end),
            (6.393, 6.643),
            "one step's own time"
        );
        walk.push(leg(1.0, walks[0].start, walks[0].end, 1));
        // The next view retimes the second step and adds a third.
        assert!(walk.retime((1, 1), 6.748));
        assert_eq!(walk.legs[1].end, 6.748);
        walk.push(leg(2.0, 6.748, 6.998, 2));
        let mut previous = None;
        let mut now = 6.15;
        while now < 6.99 {
            let Pose::At(at) = walk.at(now, 0) else {
                panic!("walk stopped at {now}");
            };
            if let Some(before) = previous {
                assert!(at.x >= before, "never walks back");
            }
            previous = Some(at.x);
            now += 1.0 / 60.0;
        }
        assert_eq!((walk.legs_walked, walk.stops), (3, 0));
        // A late report of a leg that already finished is still known.
        assert!(
            walk.retime((1, 0), 6.393),
            "a finished leg is not walked again"
        );
    }

    #[test]
    fn completed_own_step_does_not_move_back_toward_a_late_picture() {
        use crate::{camera, interact::Panel, CameraRig};
        use std::time::Duration;

        fn own_step(game: Res<Game>, mut bodies: Query<&mut ActorBody>) {
            let actor = *game.actors.keys().next().unwrap();
            let mut step = timed(0, 0.0, 0.325);
            step.span.actor = actor;
            step.span.frame = 2;
            step.span.kind = SpanKind::OwnStep {
                direction: tor_protocol::Direction::East,
            };
            start_walks(
                &game,
                1,
                vec![step],
                &HashMap::from([(actor, crate::scene::stand(IVec3::ZERO))]),
                &mut bodies,
            );
        }

        let sample = |picture_ms| {
            let mut app = App::new();
            app.insert_resource(Time::<()>::default())
                .init_resource::<Game>()
                .init_resource::<CameraRig>()
                .init_resource::<Panel>()
                .add_systems(Update, (camera::rig_camera, camera::follow_actors).chain());
            let world = app.world_mut();
            let mut view = crate::test_support::view(1);
            // These messages all share one tick. The live cursor includes the
            // move outcome, but only the picture sequence dates geometry.
            view.shown.cursor.sequence = 2;
            let mut game = world.resource_mut::<Game>();
            game.view = Some(view);
            game.snap_camera = true;
            world.spawn(Camera3d::default());
            let label = world.spawn_empty().id();
            let ring = world.spawn_empty().id();
            let root = world
                .spawn((
                    ActorBody {
                        appearance_seed: None,
                        appearance_key: String::new(),
                        model: tor_client_3d_model::CreatureModel::Emu,
                        wearing: Default::default(),
                        hostile: false,
                        goal: crate::scene::stand(IVec3::ZERO),
                        walk: Walk::default(),
                        own: true,
                        label,
                        ring,
                        name: String::new(),
                        height: 1.0,
                    },
                    Transform::from_translation(crate::scene::stand(IVec3::ZERO)),
                ))
                .id();
            world
                .resource_mut::<Game>()
                .actors
                .insert(ActorTarget::from_digest([1; 32]), root);
            world.run_system_cached(own_step).unwrap();

            let mut samples = Vec::new();
            for ms in (20..=400).step_by(20) {
                let world = app.world_mut();
                world
                    .resource_mut::<Time>()
                    .advance_by(Duration::from_millis(20));
                world.resource_mut::<Game>().playback.now = ms as f64 / 1000.0;
                if ms == picture_ms {
                    let mut game = world.resource_mut::<Game>();
                    game.feet_world = IVec3::X;
                    game.view.as_mut().unwrap().picture_sequence = 2;
                    world.get_mut::<ActorBody>(root).unwrap().goal = crate::scene::stand(IVec3::X);
                } else if ms == 400 {
                    // A new authoritative relocation can move backward.
                    let mut game = world.resource_mut::<Game>();
                    game.feet_world = -IVec3::X;
                    game.view.as_mut().unwrap().picture_sequence = 3;
                    world.get_mut::<ActorBody>(root).unwrap().goal = crate::scene::stand(-IVec3::X);
                }
                app.update();
                if [320, 340, 380, 400].contains(&ms) {
                    let world = app.world();
                    let at = world.get::<Transform>(root).unwrap().translation.x;
                    let camera = world.resource::<CameraRig>().focus.x;
                    samples.push((at, camera));
                }
            }
            samples
        };

        let prompt = sample(300);
        let late = sample(380);
        assert_eq!(
            prompt[0], late[0],
            "both streams are still walking at 320 ms"
        );
        assert!(
            prompt[1].0 >= prompt[0].0,
            "the prompt picture must not pull backward"
        );
        assert!(
            late[3].0 < late[2].0 && late[3].1 < late[2].1,
            "the new location must supersede the walk"
        );
        assert!(
            late[1].0 == 1.0 && late[1].1 >= late[0].1,
            "late picture pulled the body or camera backward: {late:?}; prompt: {prompt:?}"
        );
    }

    #[test]
    fn awaiting_geometry_follows_the_last_own_leg_not_an_npc_or_a_retime() {
        let mut walk = Walk::default();
        let mut first = leg(0.0, 1.0, 1.25, 0);
        first.own_frame = Some(6);
        let mut second = leg(1.0, 1.25, 1.5, 1);
        second.own_frame = Some(7);
        walk.push(first);
        walk.push(second);
        assert_eq!(walk.at(1.125, 0), Pose::At(Vec3::X * 0.5));
        assert_eq!(walk.at(1.125, 1), Pose::Free, "skip finishes both legs");
        assert!(
            walk.retime(second.span, 4.0),
            "retime cannot replay an old leg"
        );
        assert_eq!(walk.awaiting_picture(6), Some(Vec3::X * 2.0));
        assert_eq!(
            walk.origin(3.0, 6),
            Some(Vec3::X * 2.0),
            "still awaiting the picture"
        );
        assert_eq!(walk.awaiting_picture(7), None);
        assert_eq!(
            walk.origin(3.0, 7),
            None,
            "a caught-up picture wins after a rest"
        );

        let mut other = Walk::default();
        other.push(leg(0.0, 1.0, 1.25, 0));
        assert_eq!(other.at(1.5, 0), Pose::Free);
        assert_eq!(other.awaiting_picture(0), None);
    }

    #[test]
    fn own_steps_chain_by_direction_while_walking_and_rest_on_the_cell() {
        // Recorded travel: the picture that shows the observer's new cell can
        // arrive a step late, so own legs follow the disclosed directions.
        assert_eq!(step_offset(tor_protocol::Direction::East), Vec3::X);
        assert_eq!(step_offset(tor_protocol::Direction::North), -Vec3::Z);
        assert_eq!(step_offset(tor_protocol::Direction::Up), Vec3::Y);
        let mut walk = Walk::default();
        assert_eq!(
            walk.origin(1.0, 0),
            None,
            "a body at rest starts from its cell"
        );
        walk.push(leg(0.0, 1.0, 1.25, 0));
        assert_eq!(
            walk.origin(1.1, 0),
            Some(Vec3::X),
            "the next leg starts where this ends"
        );
        assert!(walk.retime((1, 0), 1.4));
        assert_eq!(walk.at(1.45, 0), Pose::Free);
        assert_eq!(
            walk.origin(1.5, 0),
            Some(Vec3::X),
            "just ended: still chained"
        );
        assert_eq!(
            walk.origin(2.0, 0),
            None,
            "after a rest the cell is authoritative"
        );
        // Several own steps folded into one view each keep their own leg.
        let mut first = timed(1, 1.0, 1.25);
        let mut second = timed(2, 1.25, 1.5);
        for step in [&mut first, &mut second] {
            step.span.kind = SpanKind::OwnStep {
                direction: tor_protocol::Direction::East,
            };
        }
        assert_eq!(new_walks([first, second]).len(), 2);
        let mobs = new_walks([
            Timed {
                span: ActorSpan {
                    kind: SpanKind::Step {
                        from: None,
                        to: None,
                    },
                    ..span(0, SpanEnd::Open)
                },
                ..timed(3, 1.0, 1.25)
            },
            Timed {
                span: ActorSpan {
                    kind: SpanKind::Step {
                        from: None,
                        to: None,
                    },
                    ..span(0, SpanEnd::Open)
                },
                ..timed(4, 1.25, 1.5)
            },
        ]);
        assert_eq!(mobs.len(), 1, "a mob's folded steps walk to its final cell");
        assert_eq!((mobs[0].start, mobs[0].end), (1.0, 1.5));
    }

    fn mob_step(start: u64, end: SpanEnd) -> ActorSpan {
        ActorSpan {
            actor: ActorTarget::from_digest([5; 32]),
            kind: SpanKind::Step {
                from: None,
                to: None,
            },
            ..span(start, end)
        }
    }

    /// A mob stepping at these ticks, as travel discloses it: each view
    /// reports the new step open and the previous one ended where it began.
    /// Returns each step's first timing and the last re-report of each.
    type Timing = (f64, f64);

    fn mob_walk(ticks: &[u64]) -> (Vec<Timing>, HashMap<(u64, usize), Timing>) {
        let mut playback = Playback {
            now: 10.0,
            ..Playback::default()
        };
        let (mut first, mut latest) = (Vec::new(), HashMap::new());
        for (index, tick) in ticks.iter().enumerate() {
            let mut spans = Vec::new();
            if index > 0 {
                spans.push((
                    index - 1,
                    mob_step(ticks[index - 1], SpanEnd::Inferred(*tick)),
                ));
            }
            spans.push((index, mob_step(*tick, SpanEnd::Open)));
            for step in playback.time_spans(&activity(*tick, spans)) {
                if step.key.1 == index {
                    first.push((step.start, step.end));
                }
                latest.insert(step.key, (step.start, step.end));
            }
        }
        (first, latest)
    }

    #[test]
    fn a_mobs_open_steps_last_its_observed_pace_and_chain_into_one_walk() {
        // A slow mob steps every 130 ticks. While each step is open its end
        // is unknown; a fixed base-step guess walked 100 ticks and then stood
        // still for 30 before the next step, at every cell.
        let (first, _) = mob_walk(&[0, 130, 260, 390, 520]);
        let tick = TickClock::standard().seconds_per_tick;
        for pair in first.windows(2).skip(1) {
            let ((start, end), (next, _)) = (pair[0], pair[1]);
            assert!(
                (end - next).abs() < 1e-9,
                "a step ends where the next begins: {end} vs {next}"
            );
            assert!((end - start - 130.0 * tick).abs() < 1e-9, "its own pace");
        }
    }

    #[test]
    fn a_first_step_walks_at_the_pace_actors_are_seen_to_walk() {
        // Recorded menagerie travel: mobs stepping every 200 ticks played
        // their first visible step at a 100-tick guess, then stood until the
        // next one arrived.
        let tick = TickClock::standard().seconds_per_tick;
        let mut playback = Playback {
            now: 10.0,
            ..Playback::default()
        };
        let other = ActorTarget::from_digest([6; 32]);
        let open = |start: u64, actor: ActorTarget| ActorSpan {
            actor,
            ..mob_step(start, SpanEnd::Open)
        };
        let mob = mob_step(0, SpanEnd::Open).actor;
        let length = |timed: Vec<Timed>| {
            let step = timed.last().unwrap();
            ((step.end - step.start) / tick).round() as u64
        };
        assert_eq!(
            length(playback.time_spans(&activity(0, vec![(0, open(0, mob))]))),
            100
        );
        assert_eq!(
            length(playback.time_spans(&activity(200, vec![(1, open(200, mob))]))),
            200
        );
        assert_eq!(
            length(playback.time_spans(&activity(210, vec![(2, open(210, other))]))),
            200,
            "another actor's first step walks at the pace seen so far"
        );
        assert_eq!(
            length(playback.time_spans(&activity(1_000, vec![(3, open(1_000, mob))]))),
            200,
            "after a rest, the pace it walked before"
        );
    }

    #[test]
    fn a_mob_that_rests_after_a_step_stands_instead_of_crawling() {
        let (_, latest) = mob_walk(&[0, 130, 900]);
        let tick = TickClock::standard().seconds_per_tick;
        let (start, end) = latest[&(0, 1)];
        // The step before the rest ends at its pace with some headroom, not
        // stretched over the 770-tick rest.
        assert!(
            (end - start - (130.0 + 32.0) * tick).abs() < 1e-9,
            "step lasted {} ticks",
            (end - start) / tick
        );
    }

    #[test]
    fn a_walk_waits_for_its_first_leg_to_begin() {
        let mut walk = Walk::default();
        walk.push(leg(0.0, 2.0, 2.25, 0));
        assert_eq!(walk.at(1.0, 0), Pose::Hold);
        assert!(walk.walking());
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
        assert!(
            (second[0].end
                - second[0].start
                - tor_client_3d_model::playback::STANDARD_STEP_SECONDS)
                .abs()
                < 1e-9
        );
        assert!(!playback.drained(), "input waits for the walk");
        assert!(playback.within_lead(), "a plan may keep walking");
        playback.skip();
        assert!(playback.drained(), "fast-forward catches up");
    }

    #[test]
    fn a_span_disclosed_late_walks_from_now_instead_of_snapping() {
        let mut playback = Playback {
            now: 1.0,
            ..Playback::default()
        };
        let first = playback.time_spans(&activity(0, vec![(0, span(0, SpanEnd::Exact(100)))]));
        // Another actor's same-tick step arrives after that window has passed.
        playback.now = 3.0;
        let late = playback.time_spans(&Activity {
            generation: 0,
            tick: Some(0),
            spans: vec![(
                1,
                ActorSpan {
                    actor: ActorTarget::from_digest([4; 32]),
                    ..span(0, SpanEnd::Exact(100))
                },
            )],
        });
        assert!(first[0].start < 3.0);
        assert!(
            (late[0].start - 3.0).abs() < 1e-9,
            "starts now, not in the past"
        );
        assert!(
            (late[0].end - late[0].start - tor_client_3d_model::playback::STANDARD_STEP_SECONDS)
                .abs()
                < 1e-9,
            "keeps its full duration"
        );
    }

    #[test]
    fn a_struck_actor_waits_for_the_flight_and_then_clears() {
        let target = ActorTarget::from_digest([7; 32]);
        let mut playback = Playback {
            now: 2.0,
            ..Playback::default()
        };
        assert_eq!(playback.until_struck(target), 0.0);
        playback.strike_at(target, 0.4);
        playback.strike_at(target, 0.1);
        assert!(
            (playback.until_struck(target) - 0.4).abs() < 1e-6,
            "the latest arrival wins"
        );
        assert!(!playback.drained(), "input waits for the flight too");
        playback.now = 2.5;
        assert_eq!(playback.until_struck(target), 0.0);
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
        // 400 covered ticks play for four standard steps after the lookahead; the blow
        // starts 0.2 s early so its impact lands on the tick.
        playback.now =
            5.0 + 0.12 + 4.0 * tor_client_3d_model::playback::STANDARD_STEP_SECONDS - 0.21;
        assert!(playback.release().is_empty());
        playback.now += 0.02;
        assert_eq!(playback.release().len(), 1);
        assert!(playback.release().is_empty(), "released once");
    }
}
