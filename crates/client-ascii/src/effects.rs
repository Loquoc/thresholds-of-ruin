//! Temporary glyphs, never map memory. Inspired by NetHack's beam trail versus
//! moving missile presentation, implemented independently over disclosed anchors.
use std::{
    collections::{BTreeMap, VecDeque},
    time::Duration,
};
use tor_protocol::{
    CombatEventView, EffectAppearance, EffectCue, EffectDelivery, Observation, Position,
};

const MAX_ANIMATIONS: usize = 64;
const STEP_MS: u64 = 60;
const MAX_DURATION_MS: u64 = 1800;

#[derive(Clone, Debug)]
struct Point {
    key: String,
    glyph: char,
    direction_neighbor: Option<String>,
}
#[derive(Clone, Debug)]
struct Animation {
    segments: Vec<Vec<Point>>,
    trail: bool,
    color: u32,
    elapsed: Duration,
    duration_ms: u64,
    steps: usize,
}

#[derive(Default, Debug)]
pub struct EffectAnimations {
    active: VecDeque<Animation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectGlyph {
    pub position: Position,
    pub glyph: char,
    pub color: u32,
}

impl EffectAnimations {
    pub fn clear(&mut self) {
        self.active.clear();
    }
    /// Consume only newly validated ordered outcomes, never retained pictures.
    pub fn push(&mut self, event: &CombatEventView) {
        let CombatEventView::Effect {
            anchors,
            delivery,
            appearance,
            cue,
            segments,
            ..
        } = event
        else {
            return;
        };
        if *appearance == EffectAppearance::StatusIndicator || *delivery == EffectDelivery::Area {
            return;
        }
        let keys: BTreeMap<_, _> = anchors
            .iter()
            .map(|a| (a.position, a.key.as_str()))
            .collect();
        let mut paths = Vec::new();
        for segment in segments {
            // Missing anchors break, rather than bridge, an unrenderable path.
            let mut path = Vec::new();
            for (i, position) in segment.iter().enumerate() {
                if let Some(key) = keys.get(position) {
                    let neighbor = if i > 0 {
                        segment.get(i - 1)
                    } else {
                        segment.get(i + 1)
                    };
                    let glyph = match appearance {
                        EffectAppearance::Bolt => {
                            neighbor.map_or('*', |p| direction(*position, *p))
                        }
                        EffectAppearance::Missile => '*', // item identity is not disclosed by this contract
                        EffectAppearance::Halo => 'o',
                        EffectAppearance::Cloud => '#',
                        _ => '*',
                    };
                    path.push(Point {
                        key: (*key).to_owned(),
                        glyph,
                        direction_neighbor: (*appearance == EffectAppearance::Bolt)
                            .then(|| neighbor.and_then(|p| keys.get(p)).map(|k| (*k).to_owned()))
                            .flatten(),
                    });
                } else if !path.is_empty() {
                    paths.push(std::mem::take(&mut path));
                }
            }
            if !path.is_empty() {
                paths.push(path);
            }
        }
        let steps = paths.iter().map(Vec::len).sum::<usize>();
        if steps == 0 {
            return;
        }
        if self.active.len() == MAX_ANIMATIONS {
            self.active.pop_front();
        }
        self.active.push_back(Animation {
            segments: paths,
            trail: *delivery == EffectDelivery::Ray,
            color: color(*cue),
            elapsed: Duration::ZERO,
            duration_ms: STEP_MS.saturating_mul(steps as u64).min(MAX_DURATION_MS),
            steps,
        });
    }
    /// Cosmetic time only; never sleeps, delays input, or advances simulation.
    pub fn advance(&mut self, elapsed: Duration) -> bool {
        let was_active = !self.active.is_empty();
        for animation in &mut self.active {
            animation.elapsed = animation.elapsed.saturating_add(elapsed);
        }
        self.active
            .retain(|a| a.elapsed.as_millis() < u128::from(a.duration_ms));
        was_active
    }
    pub fn glyphs(&self, observation: &Observation) -> Vec<EffectGlyph> {
        // Cell identity, not old coordinates, anchors effects across observer movement.
        // No remembered or now-hidden cell may receive a transient glyph.
        // A physical cell may have multiple portal images. Do not choose one
        // arbitrarily, even when its duplicate happens to share coordinates.
        let mut cells = BTreeMap::new();
        for cell in &observation.visible_cells {
            cells
                .entry(cell.key.as_str())
                .and_modify(|position| *position = None)
                .or_insert(Some(cell.position));
        }
        let mut glyphs = Vec::new();
        for a in &self.active {
            let step =
                (a.elapsed.as_millis() * a.steps as u128 / u128::from(a.duration_ms)) as usize;
            let mut offset = 0;
            for path in &a.segments {
                let end = offset + path.len();
                if step < end {
                    let index = step.saturating_sub(offset);
                    let range = if a.trail {
                        0..index + 1
                    } else {
                        index..index + 1
                    };
                    for point in &path[range] {
                        if let Some(Some(position)) = cells.get(point.key.as_str()) {
                            glyphs.push(EffectGlyph {
                                position: *position,
                                glyph: if matches!(point.glyph, '-' | '|' | '/' | '\\') {
                                    point
                                        .direction_neighbor
                                        .as_deref()
                                        .and_then(|key| cells.get(key).copied().flatten())
                                        .filter(|p| {
                                            p.x.abs_diff(position.x) <= 1
                                                && p.y.abs_diff(position.y) <= 1
                                                && p.z == position.z
                                        })
                                        .map_or('*', |p| direction(*position, p))
                                } else {
                                    point.glyph
                                },
                                color: a.color,
                            });
                        }
                    }
                    break; // segment boundaries clear the old trail
                }
                offset = end;
            }
        }
        glyphs
    }
}

fn direction(a: Position, b: Position) -> char {
    let dx = i64::from(a.x) - i64::from(b.x);
    let dy = i64::from(a.y) - i64::from(b.y);
    match (dx.signum(), dy.signum()) {
        (0, 0) => '*',
        (0, _) => '|',
        (_, 0) => '-',
        (x, y) if x == y => '\\',
        _ => '/',
    }
}
fn color(cue: EffectCue) -> u32 {
    match cue {
        EffectCue::Fire => 0xff8b38,
        EffectCue::Cold => 0x8ae9ff,
        EffectCue::Lightning => 0xffed71,
        EffectCue::Poison => 0x91d966,
        EffectCue::Healing => 0x6ee4b8,
        EffectCue::Status => 0xd69aff,
        EffectCue::Physical => 0xe1dfca,
        EffectCue::Neutral => 0xe7e7ff,
    }
}
