//! Conservative avoidance of disclosed nonhostiles and walls before a target.
//! Alternatives require a complete canonical disclosed prefix, not merely an
//! absence of a veto. No hidden collisions, downstream hits or reflections.
use super::*;
use crate::abilities::FlightAim;
use crate::perception::ActorDisclosure;
use std::collections::{BTreeMap, BTreeSet};

const ALTERNATE_TARGET_CELLS: usize = 16;

/// Only a canonical, monotone primary prefix can produce a definite veto.
/// This inclusive box is a negative bound, not a proof that a flight is clear.
pub(super) fn within_canonical_primary_box(
    origin: Location,
    target: Location,
    location: Location,
) -> bool {
    origin.region == target.region
        && location.region == origin.region
        && [
            (origin.position.x, target.position.x, location.position.x),
            (origin.position.y, target.position.y, location.position.y),
            (origin.position.z, target.position.z, location.position.z),
        ]
        .into_iter()
        .all(|(a, b, value)| (a.min(b)..=a.max(b)).contains(&value))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrefixEvidence {
    Unknown,
    Clear,
    Intercepted,
    Obstructed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DisclosedFlightCell {
    location: Location,
    wall: bool,
}

struct DisclosedFlightChart {
    origin: Location,
    orientation: u8,
    nonhostile: BTreeSet<Location>,
    occurrences: BTreeMap<[i32; 3], Option<DisclosedFlightCell>>,
    offsets: BTreeMap<Location, BTreeSet<[i32; 3]>>,
}

impl DisclosedFlightChart {
    fn target_offset(&self, target: Location) -> Option<[i32; 3]> {
        if target.region != self.origin.region {
            return None;
        }
        let offset = tor_world::rotate_vector(
            tor_world::inverse_rotation(self.orientation),
            [
                i64::from(target.position.x) - i64::from(self.origin.position.x),
                i64::from(target.position.y) - i64::from(self.origin.position.y),
                i64::from(target.position.z) - i64::from(self.origin.position.z),
            ],
        );
        Some([
            i32::try_from(offset[0]).ok()?,
            i32::try_from(offset[1]).ok()?,
            i32::try_from(offset[2]).ok()?,
        ])
    }

    fn assess(&self, target: Location, aim: FlightAim, range: Option<u16>) -> PrefixEvidence {
        let Some(target_offset) = self.target_offset(target) else {
            return PrefixEvidence::Unknown;
        };
        let distance = target_offset
            .iter()
            .map(|n| n.unsigned_abs())
            .max()
            .unwrap_or(0);
        if distance < 2 || range.is_some_and(|limit| distance > u32::from(limit)) {
            return PrefixEvidence::Unknown;
        }
        match aim {
            FlightAim::Cell { offset } if offset != target_offset => {
                return PrefixEvidence::Unknown
            }
            FlightAim::Direction { .. }
                if target_offset
                    .iter()
                    .zip(aim.vector())
                    .any(|(v, d)| i64::from(*v) != i64::from(d) * i64::from(distance)) =>
            {
                return PrefixEvidence::Unknown
            }
            _ => {}
        }
        if self
            .offsets
            .get(&target)
            .is_some_and(|offsets| offsets.iter().any(|at| *at != target_offset))
        {
            return PrefixEvidence::Unknown;
        }
        // Every step of a proven canonical prefix needs a distinct disclosed
        // occurrence. Bound proof work by knowledge, not an invented ray range.
        if u64::from(distance) > self.occurrences.len() as u64 {
            return PrefixEvidence::Unknown;
        }
        let Some(mut trajectory) = tor_world::RayTrajectory::new(aim.vector()) else {
            return PrefixEvidence::Unknown;
        };
        let mut at = [0_i32; 3];
        let mut intercepted = false;
        let mut obstructed = false;
        for step in 1..=distance {
            let delta = trajectory.advance();
            for axis in 0..3 {
                let Some(next) = at[axis].checked_add(delta[axis]) else {
                    return PrefixEvidence::Unknown;
                };
                at[axis] = next;
            }
            let Some(Some(cell)) = self.occurrences.get(&at) else {
                return PrefixEvidence::Unknown;
            };
            // A target reported inside a wall is conflicting evidence, not a
            // useful obstruction. Never stop at an earlier veto: the entire
            // nominal prefix must remain canonical and disclosed.
            if step == distance && cell.wall {
                return PrefixEvidence::Unknown;
            }
            if step < distance {
                obstructed |= cell.wall;
                intercepted |= self.nonhostile.contains(&cell.location);
            }
        }
        if at != target_offset {
            PrefixEvidence::Unknown
        } else if obstructed {
            PrefixEvidence::Obstructed
        } else if intercepted {
            PrefixEvidence::Intercepted
        } else {
            PrefixEvidence::Clear
        }
    }
}

impl Game {
    fn disclosed_flight_chart(
        &self,
        actor: ActorId,
        target: ActorId,
        sight: &[tor_world::SightCell],
        view: &crate::Observation,
    ) -> Option<DisclosedFlightChart> {
        let origin = self.actors.get(&actor)?;
        // Alias evidence is needed only for this target's disclosed body cells;
        // the occurrence chart below still includes every disclosed prefix cell.
        let target_locations: BTreeSet<_> = view
            .visible_actors
            .iter()
            .filter(|row| row.id == target && row.disclosure == ActorDisclosure::Creature)
            .map(|row| row.location)
            .collect();
        let nonhostile = view
            .visible_actors
            .iter()
            .filter(|row| {
                row.id != actor
                    && row.id != target
                    && row.disclosure == ActorDisclosure::Creature
                    && !self.perceived_hostile(actor, row.id, row.disclosure)
            })
            .map(|row| row.location)
            .collect();
        let disclosed: BTreeMap<_, Option<bool>> =
            view.visible_cells
                .iter()
                .fold(BTreeMap::new(), |mut map, cell| {
                    map.entry((cell.location, cell.frame))
                        .and_modify(|wall| {
                            if *wall != Some(cell.wall) {
                                *wall = None;
                            }
                        })
                        .or_insert(Some(cell.wall));
                    map
                });
        let mut occurrences = BTreeMap::new();
        let mut offsets: BTreeMap<Location, BTreeSet<[i32; 3]>> = BTreeMap::new();
        for cell in sight.iter().filter(|cell| cell.physical) {
            let Some(&shown_wall) = disclosed.get(&(cell.location, cell.rotation)) else {
                continue;
            };
            let Some(x) = cell.offset.x.checked_add(origin.body.eye[0]) else {
                continue;
            };
            let Some(y) = cell.offset.y.checked_add(origin.body.eye[1]) else {
                continue;
            };
            let Some(z) = cell.offset.z.checked_add(origin.body.eye[2]) else {
                continue;
            };
            let body_offset = [x, y, z];
            if target_locations.contains(&cell.location) {
                offsets
                    .entry(cell.location)
                    .or_default()
                    .insert(body_offset);
            }
            let delta = tor_world::rotate_vector(origin.orientation, body_offset.map(i64::from));
            let canonical = cell.rotation == origin.orientation
                && cell.location.region == origin.location.region
                && [
                    i64::from(cell.location.position.x),
                    i64::from(cell.location.position.y),
                    i64::from(cell.location.position.z),
                ] == [
                    i64::from(origin.location.position.x) + delta[0],
                    i64::from(origin.location.position.y) + delta[1],
                    i64::from(origin.location.position.z) + delta[2],
                ];
            let witness = shown_wall
                .filter(|_| canonical)
                .map(|wall| DisclosedFlightCell {
                    location: cell.location,
                    wall,
                });
            occurrences
                .entry(body_offset)
                .and_modify(|prior| {
                    if *prior != witness {
                        *prior = None;
                    }
                })
                .or_insert(witness);
        }
        Some(DisclosedFlightChart {
            origin: origin.location,
            orientation: origin.orientation,
            nonhostile,
            occurrences,
            offsets,
        })
    }

    pub(super) fn ai_disclosed_flight_aim(
        &self,
        actor: ActorId,
        target: ActorId,
        sight: &[tor_world::SightCell],
        view: &crate::Observation,
        range: u16,
    ) -> Option<FlightAim> {
        self.ai_disclosed_flight_aim_with_range(actor, target, sight, view, Some(range))
    }

    /// None selects an observed bearing without imposing a gameplay range.
    /// First-body physics, not this knowledge-only planner, owns stopping.
    pub(super) fn ai_disclosed_flight_aim_with_range(
        &self,
        actor: ActorId,
        target: ActorId,
        sight: &[tor_world::SightCell],
        view: &crate::Observation,
        range: Option<u16>,
    ) -> Option<FlightAim> {
        let origin = self.actors.get(&actor)?;
        // Keep the legacy first-row geometry and no-aim behavior unchanged.
        let aim = disclosed_flight_aim_with_range(target, sight, view, origin.body.eye, range)?;
        let primary = view
            .visible_actors
            .iter()
            .find(|row| row.id == target && row.disclosure == ActorDisclosure::Creature)?
            .location;
        // Only disclosed bodies or walls inside this primary's canonical box
        // can produce a definite veto. Check every body row, not just anchors
        // or row-relative portal coordinates. Ambiguous/folded prefixes retain
        // the original aim; this negative bound does not certify safety.
        if !view.visible_actors.iter().any(|row| {
            row.id != actor
                && row.id != target
                && row.disclosure == ActorDisclosure::Creature
                && !self.perceived_hostile(actor, row.id, row.disclosure)
                && within_canonical_primary_box(origin.location, primary, row.location)
        }) && !view.visible_cells.iter().any(|cell| {
            cell.wall && within_canonical_primary_box(origin.location, primary, cell.location)
        }) {
            return Some(aim);
        }
        let chart = self.disclosed_flight_chart(actor, target, sight, view)?;
        if !matches!(
            chart.assess(primary, aim, range),
            PrefixEvidence::Intercepted | PrefixEvidence::Obstructed
        ) {
            return Some(aim);
        }
        // Only a definite old-path veto permits a new choice. Unique disclosed
        // locations have canonical order, independent of duplicate body rows.
        let alternatives: BTreeSet<_> = view
            .visible_actors
            .iter()
            .filter(|row| {
                row.id == target
                    && row.disclosure == ActorDisclosure::Creature
                    && row.location != primary
            })
            .map(|row| row.location)
            .collect();
        for location in alternatives.into_iter().take(ALTERNATE_TARGET_CELLS) {
            let Some(offset) = chart.target_offset(location) else {
                continue;
            };
            let distance = offset.iter().map(|n| n.unsigned_abs()).max().unwrap_or(0);
            let alternative = if distance > 0
                && offset
                    .iter()
                    .all(|n| *n == 0 || n.unsigned_abs() == distance)
            {
                let Some(direction) = Direction::from_delta(offset.map(|n| i64::from(n.signum())))
                else {
                    continue;
                };
                let Ok(direction) = crate::abilities::StepDirection::try_from(direction) else {
                    continue;
                };
                FlightAim::from(direction)
            } else {
                FlightAim::Cell { offset }
            };
            if chart.assess(location, alternative, range) == PrefixEvidence::Clear {
                return Some(alternative);
            }
        }
        None
    }

    // Compatibility predicate retained for focused primary-veto regression tests.
    #[cfg(test)]
    pub(super) fn disclosed_nonhostile_interception(
        &self,
        actor: ActorId,
        target: ActorId,
        aim: FlightAim,
        sight: &[tor_world::SightCell],
        view: &crate::Observation,
        range: u16,
    ) -> bool {
        let Some(primary) = view
            .visible_actors
            .iter()
            .find(|row| row.id == target && row.disclosure == ActorDisclosure::Creature)
        else {
            return false;
        };
        let Some(origin) = self.actors.get(&actor) else {
            return false;
        };
        if primary.location.region != origin.location.region {
            return false;
        }
        if !view.visible_actors.iter().any(|row| {
            row.id != actor
                && row.id != target
                && row.disclosure == ActorDisclosure::Creature
                && !self.perceived_hostile(actor, row.id, row.disclosure)
        }) {
            return false;
        }
        let Some(chart) = self.disclosed_flight_chart(actor, target, sight, view) else {
            return false;
        };
        chart.assess(primary.location, aim, Some(range)) == PrefixEvidence::Intercepted
    }
}
