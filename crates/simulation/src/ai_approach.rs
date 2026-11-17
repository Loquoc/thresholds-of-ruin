//! Knowledge-only fallback movement shared by gold and ranged-kit acquisition.
//! This module does not choose interests, targets, quantities, or direct routes.
use crate::{Action, ActorId, Game, Observation};
use std::collections::BTreeSet;
use tor_world::{Direction, Location};

/// Shared cap per acquisition comparison, not per target or first step.
pub(crate) const DETOUR_NODE_BUDGET: usize = 512;

pub(crate) fn disclosed_bodies(actor: ActorId, view: &Observation) -> BTreeSet<Location> {
    view.visible_actors
        .iter()
        .filter(|row| {
            row.id != actor && row.disclosure == crate::perception::ActorDisclosure::Creature
        })
        .map(|row| row.location)
        .collect()
}

pub(crate) struct Approach {
    pub cost: u128,
    pub rank: usize,
    pub hops: usize,
    pub direction: Direction,
}

/// Exhaustion is distinct from a completed search with no safe route. Callers
/// must discard their entire fallback comparison, including earlier successes.
pub(crate) struct IncompleteComparison;

struct FirstStep {
    rank: usize,
    direction: Direction,
    at: Location,
    frame: u8,
    cost: u128,
}

/// Game access is limited to the acting body's present movement legality,
/// actor-owned movement costs/navigation, and exposure to disclosed hostiles.
/// Never inspect undisclosed future occupancy to improve a remembered suffix.
pub(crate) struct KnownApproachPlanner<'a> {
    game: &'a Game,
    actor: ActorId,
    view: &'a Observation,
    occupied: &'a BTreeSet<Location>,
    alternatives: Vec<FirstStep>,
}

impl<'a> KnownApproachPlanner<'a> {
    pub fn new(
        game: &'a Game,
        actor: ActorId,
        view: &'a Observation,
        occupied: &'a BTreeSet<Location>,
    ) -> Option<Self> {
        let owner = game.actors.get(&actor)?;
        let disclosed: std::collections::BTreeSet<_> = view
            .visible_cells
            .iter()
            .filter(|cell| !cell.wall)
            .map(|cell| (cell.location, cell.frame))
            .collect();
        let mut alternatives = Vec::new();
        for (rank, direction) in tor_world::Direction::HORIZONTAL
            .into_iter()
            .chain([tor_world::Direction::Up, tor_world::Direction::Down])
            .enumerate()
        {
            if game.condition_blocks_action(actor, &Action::Move(direction)) {
                continue;
            }
            let Some((at, frame)) = game.actor_translation(actor, direction) else {
                continue;
            };
            if at == view.location
                || !disclosed.contains(&(at, frame))
                || occupied.contains(&at)
                || game.exposed_to_visible_hostile(actor, at, view)
            {
                continue;
            }
            let Ok(cost) = owner.movement_duration(direction) else {
                continue;
            };
            alternatives.push(FirstStep {
                rank,
                direction,
                at,
                frame,
                cost: u128::from(cost),
            });
        }
        Some(Self {
            game,
            actor,
            view,
            occupied,
            alternatives,
        })
    }

    /// Visit first steps in the original horizontal/up/down order. Every target
    /// consumes the caller's shared budget, even repeated targets. No caching or
    /// deduplication is done here: gold orders items, ranged kits order locations.
    pub fn best_detour(
        &self,
        target: Location,
        remaining: &mut usize,
    ) -> Result<Option<Approach>, IncompleteComparison> {
        let owner = &self.game.actors[&self.actor];
        let mut best: Option<Approach> = None;
        for first in &self.alternatives {
            let Ok(mut search) = self
                .game
                .route_search_from(self.actor, first.at, first.frame)
            else {
                continue;
            };
            let suffix = match search.route_bounded(target, remaining) {
                Ok(Some(path)) => path,
                Ok(None) => return Err(IncompleteComparison),
                Err(_) => continue,
            };
            if suffix.iter().any(|step| {
                step.destination == self.view.location || self.occupied.contains(&step.destination)
            }) {
                continue;
            }
            let Some(cost) = suffix.iter().try_fold(first.cost, |total, step| {
                total.checked_add(u128::from(owner.movement_duration(step.direction).ok()?))
            }) else {
                continue;
            };
            if best
                .as_ref()
                .is_none_or(|old| (cost, first.rank) < (old.cost, old.rank))
            {
                best = Some(Approach {
                    cost,
                    rank: first.rank,
                    hops: suffix.len() + 1,
                    direction: first.direction,
                });
            }
        }
        Ok(best)
    }
}
