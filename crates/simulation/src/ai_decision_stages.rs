//! Concrete fresh-decision stages. The coordinator owns their priority order.
//! Helpers return proposals, never execute work or commit actor memory. They
//! borrow the same immutable snapshot and the one copied decision state.
use super::{low_health, Ai, AiChoice, State};
use crate::behavior::Behavior;
use crate::observation::DecisionSnapshot;
use crate::{Action, ActorId, Game, Observation, TravelStep};
use tor_world::{Direction, Location};

/// One decision's lazy gold result, including a checked absence. This is not
/// actor memory and is never serialized or shared between decisions.
#[derive(Default)]
pub(super) struct GoldChoiceCache(Option<Option<Action>>);

impl GoldChoiceCache {
    pub(super) fn has_action_or_choose(&mut self, choose: impl FnOnce() -> Option<Action>) -> bool {
        self.0.get_or_insert_with(choose).is_some()
    }

    fn take_or_choose(&mut self, choose: impl FnOnce() -> Option<Action>) -> Option<Action> {
        self.0.take().unwrap_or_else(choose)
    }
}

impl Game {
    pub(super) fn choose_fresh_care(
        &self,
        id: ActorId,
        ai: &Ai,
        has_retained: bool,
        (hp, max): (u64, u64),
        snapshot: &DecisionSnapshot,
    ) -> Option<AiChoice> {
        let view = &snapshot.view;
        // Use a known restorative before fleeing or attacking. Unknown potions
        // and mixed sequences with harm never qualify as safe healing.
        if ai.profile.care && !has_retained && low_health(hp, max, 50) {
            if let Some(item) = view
                .inventory
                .iter()
                .find(|item| self.known_healing(id, item.id))
            {
                return Some(AiChoice::Request(Action::Drink { item: item.id }));
            }
            if let Some(plan) = self.choose_ai_healing(id, &snapshot.sight, view) {
                return Some(AiChoice::Frozen(plan));
            }
        }
        if ai.profile.care && !has_retained {
            let visible_threat = view
                .visible_actors
                .iter()
                .find(|other| self.perceived_hostile(id, other.id, other.disclosure))
                .map(|other| other.id);
            if let Some(plan) = self.choose_ai_item_care(id, view, visible_threat) {
                return Some(AiChoice::Frozen(plan));
            }
        }
        if ai.profile.care
            && !has_retained
            && ai.state == State::Search
            && !self.exposed_to_visible_hostile(id, view.location, view)
            && crate::ai_items::required_nutrition(view)
                .is_some_and(|hunger| hunger != crate::nutrition::Hunger::Nourished)
        {
            if let Some(item) = view
                .inventory
                .iter()
                .find(|item| self.known_food(id, item.id))
            {
                return Some(AiChoice::Request(Action::Eat { item: item.id }));
            }
        }
        None
    }

    /// Greedy engagement and acquisition share cached positive AND negative
    /// gold results. Do not recreate the route search or eagerly evaluate loot.
    pub(super) fn choose_safe_acquisition(
        &self,
        id: ActorId,
        ai: &Ai,
        snapshot: &DecisionSnapshot,
        route: &mut impl FnMut(Location) -> Option<Vec<TravelStep>>,
        gold_choice: &mut GoldChoiceCache,
    ) -> Option<Action> {
        let view = &snapshot.view;
        if ai.state == State::Search && !self.exposed_to_visible_hostile(id, view.location, view) {
            if let Some(action) = ai
                .profile
                .equipment
                .then(|| self.choose_gear(id, view))
                .flatten()
            {
                return Some(action);
            }
            if let Some(action) = ai
                .profile
                .loot
                .then(|| self.choose_nearby_loot(id, view, &ai.profile, &snapshot.sight, route))
                .flatten()
            {
                return Some(action);
            }
            let gold =
                gold_choice.take_or_choose(|| self.choose_known_gold(id, view, &ai.profile, route));
            if let Some(action) = gold {
                return Some(action);
            }
        }
        None
    }

    /// Only proposed visits/RNG change; the source Game remains untouched.
    pub(super) fn choose_exploration(
        &self,
        id: ActorId,
        ai: &mut Ai,
        behavior: &mut Behavior,
        has_retained: bool,
        view: &Observation,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gold_memo_reuses_positive_and_negative_results_without_eager_work() {
        for expected in [None, Some(Action::Move(Direction::East))] {
            let mut cache = GoldChoiceCache::default();
            let calls = std::cell::Cell::new(0);
            let choose = || {
                calls.set(calls.get() + 1);
                expected.clone()
            };
            assert_eq!(cache.has_action_or_choose(choose), expected.is_some());
            assert_eq!(cache.has_action_or_choose(choose), expected.is_some());
            assert_eq!(cache.take_or_choose(choose), expected);
            assert_eq!(calls.get(), 1);
        }
    }

    #[test]
    fn gold_memo_uncached_acquisition_evaluates_once_and_consumes_result() {
        let mut cache = GoldChoiceCache::default();
        let expected = Action::Wait;
        let calls = std::cell::Cell::new(0);
        assert_eq!(
            cache.take_or_choose(|| {
                calls.set(calls.get() + 1);
                Some(expected.clone())
            }),
            Some(expected)
        );
        assert_eq!(calls.get(), 1);
        assert!(cache.0.is_none());
    }
}
