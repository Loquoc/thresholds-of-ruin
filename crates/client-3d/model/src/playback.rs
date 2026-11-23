//! Renderer-free playback timing: ticks to presentation seconds, and straight
//! missile runs within disclosed effect segments.
//!
//! The clock plays disclosed activity at a fixed rate and compresses idle
//! stretches, so the player never waits on empty time. It never schedules a
//! newly disclosed tick in the past, so a late update plays rather than skips.
use tor_protocol::Position;

/// Presentation seconds for one base step (100 ticks) at the standard rate.
pub const STANDARD_STEP_SECONDS: f64 = 0.325;

/// Maps simulation ticks to presentation seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct TickClock {
    /// Presentation seconds per simulation tick while activity is shown.
    pub seconds_per_tick: f64,
    /// The longest idle stretch, in seconds, between disclosed activity.
    pub gap_cap: f64,
    /// Delay before the first or a late update plays, to gather lookahead.
    pub lookahead: f64,
    anchor: Option<(u64, f64)>,
    active_until: u64,
    horizon: f64,
}

/// Where a disclosed tick plays, and how far in the past its own moment
/// already was when it was placed: zero when it was disclosed in time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub at: f64,
    pub late: f64,
}

impl Default for TickClock {
    fn default() -> Self {
        Self::standard()
    }
}

impl TickClock {
    pub fn new(seconds_per_tick: f64, gap_cap: f64, lookahead: f64) -> Self {
        Self {
            seconds_per_tick,
            gap_cap,
            lookahead,
            anchor: None,
            active_until: 0,
            horizon: f64::NEG_INFINITY,
        }
    }

    /// The ordinary presentation rate: a base step of 100 ticks in 0.325 s,
    /// 30% longer than a quarter second so a walk reads unhurried. Every
    /// action shares the rate, so strikes stay in step with the walks.
    pub fn standard() -> Self {
        Self::new(STANDARD_STEP_SECONDS / 100.0, 0.12, 0.12)
    }

    /// Known activity covers time up to this tick, so it plays in full.
    pub fn cover(&mut self, until: u64) {
        self.active_until = self.active_until.max(until);
    }

    /// The presentation time of a disclosed tick. A tick after the anchor
    /// advances it; an earlier tick (a start disclosed late) is placed relative
    /// to the anchor without moving it.
    pub fn place(&mut self, tick: u64, now: f64) -> Placement {
        let Some((at, time)) = self.anchor else {
            let placed = now + self.lookahead;
            self.anchor = Some((tick, placed));
            return Placement {
                at: placed,
                late: 0.0,
            };
        };
        if tick <= at {
            let placed = time - (at - tick) as f64 * self.seconds_per_tick;
            return Placement {
                at: placed,
                late: (now - placed).max(0.0),
            };
        }
        let covered = self.active_until.clamp(at, tick) - at;
        let idle = tick - at - covered;
        let mut placed = time
            + covered as f64 * self.seconds_per_tick
            + (idle as f64 * self.seconds_per_tick).min(self.gap_cap);
        // A stream that fell behind resumes from now instead of skipping ahead.
        let late = (now - placed).max(0.0);
        if late > 0.0 {
            placed = now + self.lookahead;
        }
        self.anchor = Some((tick, placed));
        Placement { at: placed, late }
    }

    /// How far presentation trails the newest placed tick: the buffer that
    /// lets steps disclosed after their tick still play on time.
    pub fn lead(&self, now: f64) -> f64 {
        self.anchor.map_or(0.0, |(_, time)| (time - now).max(0.0))
    }

    /// The presentation time of a later tick at the full rate, without
    /// moving the anchor: for the end of a span that starts at `from`.
    pub fn span(&self, from: f64, ticks: u64) -> f64 {
        from + ticks as f64 * self.seconds_per_tick
    }

    /// Note that presentation is scheduled until `time`.
    pub fn reserve(&mut self, time: f64) {
        self.horizon = self.horizon.max(time);
    }

    /// Seconds of scheduled presentation still ahead of `now`.
    pub fn ahead(&self, now: f64) -> f64 {
        (self.horizon - now).max(0.0)
    }

    /// Whether everything scheduled so far has played.
    pub fn drained(&self, now: f64) -> bool {
        now >= self.horizon
    }

    /// Fast-forward: everything already scheduled is due now.
    pub fn skip(&mut self, now: f64) {
        if let Some((tick, time)) = self.anchor {
            self.anchor = Some((tick, time.min(now)));
        }
        self.horizon = self.horizon.min(now);
    }

    /// A replacement state starts timing afresh.
    pub fn reset(&mut self) {
        *self = Self::new(self.seconds_per_tick, self.gap_cap, self.lookahead);
    }
}

/// The largest perpendicular offset, in cells, of a stepped cell path that
/// still reads as one straight flight.
const RUN_TOLERANCE: f64 = 0.75;

/// Split one disclosed segment into maximal straight runs, as index pairs.
///
/// A stepped off-axis path (each step moves at most one cell per axis) becomes
/// one run along its overall direction instead of zigzagging. A reversal or
/// bounce starts a new run. Runs never extend past the segment's own disclosed
/// cells, and separate segments are never joined.
pub fn straight_runs<P: Cell>(segment: &[P]) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut start = 0;
    while start + 1 < segment.len() {
        let mut end = start + 1;
        while end + 1 < segment.len() && fits(&segment[start..=end + 1]) {
            end += 1;
        }
        runs.push((start, end));
        start = end;
    }
    runs
}

fn fits<P: Cell>(cells: &[P]) -> bool {
    let point = |p: &P| p.xyz();
    let origin = point(&cells[0]);
    let last = point(&cells[cells.len() - 1]);
    let direction = [
        last[0] - origin[0],
        last[1] - origin[1],
        last[2] - origin[2],
    ];
    let length2: f64 = direction.iter().map(|d| d * d).sum();
    if length2 == 0.0 {
        return false;
    }
    let mut previous = 0.0;
    for cell in &cells[1..cells.len() - 1] {
        let p = point(cell);
        let offset = [p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]];
        let along: f64 = offset
            .iter()
            .zip(direction)
            .map(|(o, d)| o * d)
            .sum::<f64>()
            / length2;
        if along <= previous || along >= 1.0 {
            return false;
        }
        previous = along;
        let across2: f64 = offset
            .iter()
            .zip(direction)
            .map(|(o, d)| (o - along * d).powi(2))
            .sum();
        if across2 > RUN_TOLERANCE * RUN_TOLERANCE {
            return false;
        }
    }
    true
}

/// A point in cell units: a disclosed cell, or a renderer's world point.
pub trait Cell: Copy + PartialEq {
    fn xyz(&self) -> [f64; 3];
}

impl Cell for Position {
    fn xyz(&self) -> [f64; 3] {
        [f64::from(self.x), f64::from(self.y), f64::from(self.z)]
    }
}

impl Cell for [f32; 3] {
    fn xyz(&self) -> [f64; 3] {
        self.map(f64::from)
    }
}

fn delta<P: Cell>(from: P, to: P) -> [f64; 3] {
    let (a, b) = (from.xyz(), to.xyz());
    [b[0] - a[0], b[1] - a[1], b[2] - a[2]]
}

fn length(d: [f64; 3]) -> f64 {
    d.iter().map(|v| v * v).sum::<f64>().sqrt()
}

/// One straight piece of a missile's flight, timed from the effect's start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leg<P> {
    pub from: P,
    pub to: P,
    /// Seconds after the effect starts.
    pub delay: f64,
    pub duration: f64,
}

/// Time the straight runs of every disclosed segment at `cells_per_second`.
/// A hidden gap between segments is not drawn, but it delays the next
/// segment by the distance between the two disclosed endpoints.
pub fn flight_legs<P: Cell>(segments: &[Vec<P>], cells_per_second: f64) -> Vec<Leg<P>> {
    let mut legs = Vec::new();
    let mut clock = 0.0;
    let mut last: Option<P> = None;
    for segment in segments {
        let Some(&first) = segment.first() else {
            continue;
        };
        if let Some(previous) = last {
            clock += length(delta(previous, first)) / cells_per_second;
        }
        for (start, end) in straight_runs(segment) {
            let (from, to) = (segment[start], segment[end]);
            let duration = length(delta(from, to)) / cells_per_second;
            legs.push(Leg {
                from,
                to,
                delay: clock,
                duration,
            });
            clock += duration;
        }
        last = segment.last().copied();
    }
    legs
}

/// When a flight reaches `cell`, only if that exact cell was disclosed on a
/// segment. A target anywhere else gets no arrival: its reaction plays at the
/// effect, and the flight is never drawn toward it.
pub fn arrival<P: Cell>(segments: &[Vec<P>], legs: &[Leg<P>], cell: P) -> Option<f64> {
    if !segments
        .iter()
        .flatten()
        .any(|disclosed| *disclosed == cell)
    {
        return None;
    }
    legs.iter().find_map(|leg| {
        let total = delta(leg.from, leg.to);
        let offset = delta(leg.from, cell);
        let length2: f64 = total.iter().map(|d| d * d).sum();
        if length2 == 0.0 {
            return (offset == [0.0; 3]).then_some(leg.delay);
        }
        let along =
            (offset.iter().zip(total).map(|(o, d)| o * d).sum::<f64>() / length2).clamp(0.0, 1.0);
        let across2: f64 = offset
            .iter()
            .zip(total)
            .map(|(o, d)| (o - along * d).powi(2))
            .sum();
        (across2 <= RUN_TOLERANCE * RUN_TOLERANCE).then_some(leg.delay + along * leg.duration)
    })
}
