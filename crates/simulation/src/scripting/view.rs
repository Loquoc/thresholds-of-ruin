//! Read-only queries a host may answer scripts from.
//!
//! The view is authoritative scenario state, not a client observation: script
//! authority is the scenario's own. Hosts decide what a script may reveal to a
//! player; this view never addresses clients.

use super::{ScriptState, TimerId, Value, VarScope};
use crate::{ActorId, Game, ItemId, ItemIdentity, ItemLocation};
use tor_world::Location;

#[derive(Clone, Copy)]
pub struct ScriptView<'a> {
    pub(crate) game: &'a Game,
    pub(crate) state: &'a ScriptState,
}

impl std::fmt::Debug for ScriptView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptView").field("tick", &self.game.tick).finish()
    }
}

impl<'a> ScriptView<'a> {
    pub fn tick(&self) -> u64 {
        self.game.tick
    }
    pub fn state(&self) -> &'a ScriptState {
        self.state
    }
    /// The identity the next placed item will receive.
    pub fn next_item(&self) -> ItemId {
        ItemId(self.game.next_item_id)
    }
    pub fn actor_exists(&self, actor: ActorId) -> bool {
        self.game.actors.contains_key(&actor)
    }
    pub fn alive(&self, actor: ActorId) -> bool {
        self.game.actors.contains_key(&actor) && self.game.alive(actor)
    }
    pub fn is_character(&self, actor: ActorId) -> bool {
        self.game.combat.characters.contains(&actor)
    }
    pub fn characters(&self) -> impl Iterator<Item = ActorId> + 'a {
        self.game.combat.characters.iter().copied()
    }
    pub fn actor_location(&self, actor: ActorId) -> Option<Location> {
        self.game.actors.get(&actor).map(|actor| actor.location)
    }
    /// Current and maximum Health.
    pub fn health(&self, actor: ActorId) -> Option<(u64, u64)> {
        self.game.health(actor)
    }
    /// Living actors whose anchor cell is `at`, in identity order.
    pub fn actors_at(&self, at: Location) -> Vec<ActorId> {
        self.game
            .actors
            .iter()
            .filter(|(_, actor)| actor.location == at)
            .map(|(&id, _)| id)
            .collect()
    }
    pub fn item_identity(&self, item: ItemId) -> Option<&'a ItemIdentity> {
        self.game.items.get(&item).map(|item| &item.spec.identity)
    }
    /// The ground cell, or the carrier, of an item.
    pub fn item_place(&self, item: ItemId) -> Option<Result<Location, ActorId>> {
        self.game.items.get(&item).map(|item| match item.location {
            ItemLocation::Ground(at) => Ok(at),
            ItemLocation::Carried(actor) => Err(actor),
        })
    }
    /// Items an actor carries, in identity order.
    pub fn carried(&self, actor: ActorId) -> Vec<ItemId> {
        self.game.items.at(ItemLocation::Carried(actor)).collect()
    }
    pub fn items_at(&self, at: Location) -> Vec<ItemId> {
        self.game.items.at(ItemLocation::Ground(at)).collect()
    }
    pub fn door_open(&self, at: Location) -> Option<bool> {
        self.game.world.door(at).map(|door| door.open)
    }
    pub fn var(&self, scope: VarScope, name: &str) -> Option<&'a Value> {
        self.state.var(scope, name)
    }
    /// Ticks until the timer fires, or its frozen remainder.
    pub fn timer_remaining(&self, timer: TimerId) -> Option<u64> {
        self.state.timer(timer).map(|timer| match timer.clock {
            super::state::TimerClock::Due(due) => due.saturating_sub(self.game.tick),
            super::state::TimerClock::Frozen { remaining } => remaining,
        })
    }
}

/// Draw a uniform integer in `0..bound` from a script stream. Hosts use this
/// exact function so every runtime advances the persisted stream identically.
/// Returns `None` for a zero bound.
pub fn script_random(state: &mut u64, bound: u64) -> Option<u64> {
    if bound == 0 {
        return None;
    }
    // Rejection sampling keeps the distribution exact.
    let zone = u64::MAX - u64::MAX % bound;
    loop {
        let word = crate::dice::random_word(state);
        if word < zone {
            return Some(word % bound);
        }
    }
}
