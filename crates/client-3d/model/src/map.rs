//! Exact disclosed map destinations and revision-bound, explicitly confirmed journeys.
use crate::{Cmd, View};
use std::collections::{BTreeMap, BTreeSet};
use tor_protocol::{Position, ReplyContext};
/// One unambiguous, locally selectable destination in the displayed map.
/// It is a candidate for server travel, not a proof of a reachable route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapDestination {
    /// Disclosed cell identity sent to the server, never reconstructed from a name.
    pub key: String,
    /// Cell location in the current observer frame.
    pub position: Position,
    /// Presentation category, independent of the destination's identity.
    pub label: &'static str,
}
/// A destination pinned to the complete context in which the player selected it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapSelection {
    /// Original displayed context; do not replace it when refreshing the map.
    pub shown: ReplyContext,
    /// Exact key, position and category that confirmation must still match.
    pub destination: MapDestination,
}
/// List known current-level destinations, ordered by y, x, z and cell key.
///
/// Current appearances take precedence over memory. Repeated keys/positions,
/// the observer's cell, other actors, closed doors and traps are excluded; solid
/// cells are excluded unless they disclose stairs. Neither presence in this
/// list nor remembered geometry establishes server reachability.
pub fn map_destinations(view: &View) -> Vec<MapDestination> {
    let mut columns = BTreeMap::new();
    for cell in view.terrain().filter(|cell| cell.position.z == view.feet.z) {
        let entry = columns.entry(cell.position).or_insert((cell.seen, 0usize));
        if cell.seen && !entry.0 {
            *entry = (cell.seen, 1);
        } else if cell.seen == entry.0 {
            entry.1 += 1;
        }
    }
    let current: Vec<_> = view
        .terrain()
        .filter(|cell| {
            columns
                .get(&cell.position)
                .is_some_and(|(seen, _)| *seen == cell.seen)
        })
        .collect();
    let mut keys = BTreeMap::new();
    for cell in &current {
        *keys.entry(cell.key).or_insert(0usize) += 1;
    }
    let occupied: BTreeSet<_> = view
        .actors
        .iter()
        .filter(|a| !a.is_self)
        .map(|a| a.position)
        .collect();
    let mut rows: Vec<_> = current
        .into_iter()
        .filter(|cell| {
            columns[&cell.position].1 == 1
                && keys[cell.key] == 1
                && cell.position != view.feet
                && !occupied.contains(&cell.position)
                && (!cell.wall || cell.stairs_up || cell.stairs_down)
                && cell.door != Some(false)
                && cell.trap.is_none()
        })
        .map(|cell| MapDestination {
            key: cell.key.to_owned(),
            position: cell.position,
            label: if cell.stairs_up {
                "Stairs up"
            } else if cell.stairs_down {
                "Stairs down"
            } else if cell.door.is_some() {
                "Open doorway"
            } else {
                "Known floor"
            },
        })
        .collect();
    rows.sort_by(|a, b| {
        (a.position.y, a.position.x, a.position.z, &a.key).cmp(&(
            b.position.y,
            b.position.x,
            b.position.z,
            &b.key,
        ))
    });
    rows
}
impl MapSelection {
    /// Pin an eligible key and its current display context, or return `None`.
    pub fn select(view: &View, key: &str) -> Option<Self> {
        map_destinations(view)
            .into_iter()
            .find(|cell| cell.key == key)
            .map(|destination| Self {
                shown: view.shown.clone(),
                destination,
            })
    }
    /// Build a travel command only when the exact selection remains current.
    ///
    /// The caller supplies transport/UI readiness in `ready`. The view must
    /// additionally grant control and action readiness, have no identification
    /// prompt, and retain the identical context and destination. A changed view
    /// requires a fresh selection, not silent rebinding. Nothing is sent here.
    pub fn command(&self, view: &View, ready: bool) -> Option<Cmd> {
        (ready
            && view.identify.is_none()
            && view.has_control
            && view.can_act
            && self.shown == view.shown
            && map_destinations(view)
                .iter()
                .any(|current| current == &self.destination))
        .then(|| Cmd::Travel(self.destination.key.clone()))
    }
}
