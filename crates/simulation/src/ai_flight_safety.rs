//! Conservative avoidance of disclosed nonhostile interception before a target.
//! Not a prediction of hidden collisions, downstream hits or reflections.
use super::*;
use crate::abilities::FlightAim;
use crate::perception::ActorDisclosure;
use std::collections::{BTreeMap, BTreeSet};

impl Game {
    pub(super) fn ai_disclosed_flight_aim(
        &self,
        actor: ActorId,
        target: ActorId,
        sight: &[tor_world::SightCell],
        view: &crate::Observation,
        range: u16,
    ) -> Option<FlightAim> {
        let origin = self.actors.get(&actor)?;
        let aim = disclosed_flight_aim(target, sight, view, origin.body.eye, range)?;
        (!self.disclosed_nonhostile_interception(actor, target, aim, sight, view, range))
            .then_some(aim)
    }

    pub(super) fn disclosed_nonhostile_interception(
        &self,
        actor: ActorId,
        target: ActorId,
        aim: FlightAim,
        sight: &[tor_world::SightCell],
        view: &crate::Observation,
        range: u16,
    ) -> bool {
        let Some(origin) = self.actors.get(&actor) else {
            return false;
        };
        // Keep exactly the target row used by disclosed_flight_aim. ActorView
        // has rows for disclosed body cells, not a private shape/anchor query.
        let Some(target_row) = view
            .visible_actors
            .iter()
            .find(|row| row.id == target && row.disclosure == ActorDisclosure::Creature)
        else {
            return false;
        };
        if target_row.location.region != origin.location.region {
            return false;
        }
        let nonhostile: BTreeSet<_> = view
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
        if nonhostile.is_empty() {
            return false;
        }
        let offset = tor_world::rotate_vector(
            tor_world::inverse_rotation(origin.orientation),
            [
                i64::from(target_row.location.position.x) - i64::from(origin.location.position.x),
                i64::from(target_row.location.position.y) - i64::from(origin.location.position.y),
                i64::from(target_row.location.position.z) - i64::from(origin.location.position.z),
            ],
        );
        let Ok(target_offset) = offset
            .into_iter()
            .map(i32::try_from)
            .collect::<Result<Vec<_>, _>>()
        else {
            return false;
        };
        let target_offset = [target_offset[0], target_offset[1], target_offset[2]];
        let distance = target_offset
            .iter()
            .map(|value| value.unsigned_abs())
            .max()
            .unwrap_or(0);
        if distance < 2 || distance > u32::from(range) {
            return false;
        }
        // A Direction bearing has unit length, not the target distance.
        // Verify the selected bearing's endpoint in the canonical local chart.
        match aim {
            FlightAim::Cell { offset } if offset != target_offset => return false,
            FlightAim::Direction { .. }
                if target_offset
                    .iter()
                    .zip(aim.vector())
                    .any(|(value, direction)| {
                        i64::from(*value) != i64::from(direction) * i64::from(distance)
                    }) =>
            {
                return false
            }
            _ => {}
        }
        // Build disclosure lookup once. A projected wall (e.g. a concealed
        // door) is authoritative; raw physical wall state must not override it.
        let disclosed: BTreeMap<_, bool> =
            view.visible_cells
                .iter()
                .fold(BTreeMap::new(), |mut map, cell| {
                    map.entry((cell.location, cell.frame))
                        .and_modify(|wall| *wall |= cell.wall)
                        .or_insert(cell.wall);
                    map
                });
        // None means a disclosed but unsupported/ambiguous occurrence at this
        // offset. Do not discard those entries: a later valid duplicate must
        // never overwrite their ambiguity.
        let mut occurrences: BTreeMap<[i32; 3], Option<Location>> = BTreeMap::new();
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
            // Direction aim forgets distance. A shorter portal alias could have
            // supplied the chosen aim even when a farther canonical row exists.
            // Conservatively decline the new veto for aliased target rows.
            if cell.location == target_row.location && body_offset != target_offset {
                return false;
            }
            let delta = tor_world::rotate_vector(origin.orientation, body_offset.map(i64::from));
            let canonical = cell.rotation == origin.orientation
                && cell.location.region == origin.location.region
                && !shown_wall
                && [
                    i64::from(cell.location.position.x),
                    i64::from(cell.location.position.y),
                    i64::from(cell.location.position.z),
                ] == [
                    i64::from(origin.location.position.x) + delta[0],
                    i64::from(origin.location.position.y) + delta[1],
                    i64::from(origin.location.position.z) + delta[2],
                ];
            let witness = canonical.then_some(cell.location);
            occurrences
                .entry(body_offset)
                .and_modify(|prior| {
                    if *prior != witness {
                        *prior = None;
                    }
                })
                .or_insert(witness);
        }
        let Some(mut trajectory) = tor_world::RayTrajectory::new(aim.vector()) else {
            return false;
        };
        let mut at = [0_i32; 3];
        let mut intercepted = false;
        for step in 1..=distance {
            let delta = trajectory.advance();
            for axis in 0..3 {
                let Some(next) = at[axis].checked_add(delta[axis]) else {
                    return false;
                };
                at[axis] = next;
            }
            let Some(Some(location)) = occurrences.get(&at) else {
                return false;
            };
            if step < distance && nonhostile.contains(location) {
                intercepted = true;
            }
        }
        at == target_offset && intercepted
    }
}
