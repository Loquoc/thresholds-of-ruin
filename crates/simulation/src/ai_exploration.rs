//! Short, actor-owned exploration interests. Selection uses disclosed cells;
//! continuation uses remembered navigation, never undisclosed future terrain.
//! Combat, care and acquisition suspend this interest; a successful erratic
//! deviation releases its draft goal, committed only with ordinary execution.
use super::{Ai, State};
use crate::behavior::Behavior;
use crate::travel::RouteSearch;
use crate::{Action, ActorId, Game, Observation, TravelStep};
use std::collections::BTreeSet;
use tor_world::{Direction, Location};

const DESTINATION_ATTEMPTS: usize = 16;
const NODE_BUDGET: usize = 512;
const MIN_DESTINATION_HOPS: usize = 3;
const MAX_DESTINATION_HOPS: usize = 12;

/// Policy ranking of an already legal, remembered route. Endpoint visits stay
/// primary; mean interior visits avoids preferring a short retraced route merely
/// because it contains fewer cells. Scores only reconstructed routes and owned visits.
struct ExplorationProposal {
    endpoint_visits: u64,
    interior_visits: u128,
    interior_steps: usize,
    destination: Location,
    action: Action,
}

impl ExplorationProposal {
    fn improves(&self, other: &Self) -> bool {
        self.endpoint_visits < other.endpoint_visits
            || (self.endpoint_visits == other.endpoint_visits
                && self.interior_visits * (other.interior_steps as u128)
                    < other.interior_visits * (self.interior_steps as u128))
    }
}

impl Game {
    /// Revalidate only the actual next move against current disclosure/body
    /// legality. Suffix occupancy is limited to already disclosed creatures.
    fn exploration_first_step(
        &self,
        id: ActorId,
        view: &Observation,
        occupied: &BTreeSet<Location>,
        route: &[TravelStep],
    ) -> Option<Action> {
        if route.len() > MAX_DESTINATION_HOPS
            || route
                .iter()
                .any(|step| occupied.contains(&step.destination))
        {
            return None;
        }
        let first = route.first()?;
        let action = Action::Move(first.direction);
        if self.condition_blocks_action(id, &action) {
            return None;
        }
        let (at, frame) = self.actor_translation(id, first.direction)?;
        (at == first.destination
            && at != view.location
            && view
                .visible_cells
                .iter()
                .any(|cell| cell.location == at && cell.frame == frame && !cell.wall))
        .then_some(action)
    }

    fn choose_exploration_destination(
        &self,
        id: ActorId,
        ai: &mut Ai,
        behavior: &mut Behavior,
        view: &Observation,
        search: Option<&mut RouteSearch<'_>>,
    ) -> Option<Action> {
        if ai.exploration_destination == Some(view.location) {
            ai.exploration_destination = None;
            return Some(Action::Wait);
        }
        let Some(search) = search else {
            ai.exploration_destination = None;
            return None;
        };
        let occupied = crate::ai_approach::disclosed_bodies(id, view);
        let mut remaining = NODE_BUDGET;
        if let Some(destination) = ai.exploration_destination {
            match search.route_bounded(destination, &mut remaining) {
                Ok(Some(route)) => {
                    if let Some(action) = self.exploration_first_step(id, view, &occupied, &route) {
                        return Some(action);
                    }
                    ai.exploration_destination = None;
                }
                Err(_) => ai.exploration_destination = None,
                // An interest has a cost limit. Abandon on exhaustion without
                // claiming the remembered destination is unreachable. This also
                // prevents stale goals surviving a distant relocation forever.
                Ok(None) => ai.exploration_destination = None,
            }
            // Do not immediately reselect a failed/occupied destination.
            return None;
        }
        let mut candidates: Vec<_> = view
            .visible_cells
            .iter()
            .filter(|cell| {
                !cell.wall && cell.location != view.location && !occupied.contains(&cell.location)
            })
            .map(|cell| (*ai.visits.get(&cell.location).unwrap_or(&0), cell.location))
            .collect();
        candidates.sort_unstable();
        candidates.dedup();
        let least_visits = candidates.first()?.0;
        // Sample each visit tier before taking another candidate from a tier.
        // Disclosed but unreachable cells (for example air seen by a walker)
        // must not permanently exclude every visited, reachable destination.
        let mut tiers = Vec::new();
        let mut begin = 0;
        while begin < candidates.len() {
            let visits = candidates[begin].0;
            let end = begin + candidates[begin..].partition_point(|entry| entry.0 == visits);
            tiers.push((begin, end, None));
            begin = end;
        }
        let mut rng = behavior.rng;
        let mut attempts = 0;
        let mut best: Option<ExplorationProposal> = None;
        'samples: for offset in 0..DESTINATION_ATTEMPTS {
            for (begin, end, start) in &mut tiers {
                let visits = candidates[*begin].0;
                // Higher endpoint tiers cannot improve the proposal. Equal
                // tiers can improve its remembered interior novelty; retain
                // the first seeded candidate when their exact means tie.
                if best
                    .as_ref()
                    .is_some_and(|choice| choice.endpoint_visits < visits)
                {
                    continue;
                }
                let count = *end - *begin;
                if offset >= count {
                    continue;
                }
                if attempts == DESTINATION_ATTEMPTS {
                    break 'samples;
                }
                attempts += 1;
                // Draw lazily, once per sampled tied tier. A unique first-tier
                // success draws nothing. Failed proposals leave fallback RNG
                // untouched; all successful proposal draws commit with it.
                let start = *start.get_or_insert_with(|| {
                    if count > 1 {
                        (crate::dice::random_word(&mut rng) % count as u64) as usize
                    } else {
                        0
                    }
                });
                let destination = candidates[*begin + (start + offset) % count].1;
                let route = match search.route_bounded(destination, &mut remaining) {
                    Ok(Some(route)) => route,
                    // Other candidates may already be settled in this shared
                    // frontier, even after its expansion allowance is spent.
                    Ok(None) | Err(_) => continue,
                };
                if route.len() < MIN_DESTINATION_HOPS {
                    continue;
                }
                if let Some(action) = self.exploration_first_step(id, view, &occupied, &route) {
                    let interior = &route[..route.len() - 1];
                    let proposal = ExplorationProposal {
                        endpoint_visits: visits,
                        interior_visits: interior
                            .iter()
                            .map(|step| u128::from(*ai.visits.get(&step.destination).unwrap_or(&0)))
                            .sum(),
                        interior_steps: interior.len(),
                        destination,
                        action,
                    };
                    let optimal = visits == least_visits && proposal.interior_visits == 0;
                    if best
                        .as_ref()
                        .is_none_or(|previous| proposal.improves(previous))
                    {
                        best = Some(proposal);
                    }
                    if optimal {
                        break 'samples;
                    }
                }
            }
        }
        let choice = best?;
        ai.exploration_destination = Some(choice.destination);
        behavior.rng = rng;
        Some(choice.action)
    }
    /// Only proposed visits/RNG change; the source Game remains untouched.
    pub(super) fn choose_exploration(
        &self,
        id: ActorId,
        ai: &mut Ai,
        behavior: &mut Behavior,
        has_retained: bool,
        view: &Observation,
        search: Option<&mut RouteSearch<'_>>,
    ) -> Option<Action> {
        if ai.state == State::Search
            && ai.profile.idle == crate::behavior::IdleBehavior::Explore
            && self.condition_allows_voluntary_input(id)
        {
            let can_move = !self.condition_blocks_action(id, &Action::Move(Direction::North));
            if !has_retained && can_move {
                let count = ai
                    .visits
                    .get(&view.location)
                    .copied()
                    .unwrap_or(0)
                    .saturating_add(1);
                crate::diagnostics::ai_visit_entries_copied(
                    ai.visits.copy_on_write_entries(view.location.region),
                );
                ai.visits.insert(view.location, count);
            }
            if !can_move {
                return Some(Action::Wait);
            }
            if let Some(action) =
                self.choose_exploration_destination(id, ai, behavior, view, search)
            {
                return Some(action);
            }
            let mut candidates = Vec::new();
            for direction in Direction::HORIZONTAL
                .into_iter()
                .chain([Direction::Up, Direction::Down])
            {
                if let Some((at, _)) = self.actor_translation(id, direction) {
                    if view
                        .visible_cells
                        .iter()
                        .any(|c| c.location == at && !c.wall)
                    {
                        candidates.push((*ai.visits.get(&at).unwrap_or(&0), at, direction));
                    }
                }
            }
            candidates.sort_by_key(|(visits, at, _)| (*visits, *at));
            if let Some((least_visits, _, _)) = candidates.first() {
                // Stable ordering makes replay independent of container iteration,
                // but coordinates must not decide between equally good destinations.
                // Advance only the private decision's actor-owned stream; execution
                // commits it together with the chosen action and visit history.
                let tied = candidates
                    .iter()
                    .take_while(|(visits, _, _)| visits == least_visits)
                    .count();
                let index = if tied > 1 {
                    (crate::dice::random_word(&mut behavior.rng) % tied as u64) as usize
                } else {
                    0
                };
                return Some(Action::Move(candidates[index].2));
            }
        }
        None
    }
}
