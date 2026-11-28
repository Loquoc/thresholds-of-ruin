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
        f.debug_struct("ScriptView")
            .field("tick", &self.game.tick)
            .finish()
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
    /// The run's selected character, when a run is configured.
    pub fn selected(&self) -> Option<ActorId> {
        self.game.combat.selected
    }
    /// An actor's faction, when it has a creature build.
    pub fn faction(&self, actor: ActorId) -> Option<&'a str> {
        let combat = self.game.actors.get(&actor)?.combat.as_ref()?;
        Some(combat.identity.faction.as_str())
    }
    /// The actor's current scripted goal.
    pub fn goal(&self, actor: ActorId) -> Option<&'a super::Goal> {
        self.state.goal(actor)
    }
    /// The name of the plane an actor is on; the material plane is
    /// `"material"`.
    pub fn actor_plane(&self, actor: ActorId) -> Option<&'a str> {
        self.game.plane_name(self.game.actor_plane(actor)?)
    }
    /// Whether an actor knows the named plane. Unknown names are unknown.
    pub fn knows_plane(&self, actor: ActorId, name: &str) -> bool {
        self.game
            .plane_by_name(name)
            .is_some_and(|plane| self.game.knows_plane(actor, plane))
    }
    /// A skill check against `dc` for `actor`, with the same skill, edge and
    /// bonus rules as environmental checks, drawn from the script's own stream.
    /// It spends no next-check sources. `None` for actors without creature
    /// statistics or an out-of-range difficulty.
    pub fn check(
        &self,
        actor: ActorId,
        skill: crate::attributes::Skill,
        dc: i32,
        rng: &mut u64,
    ) -> Option<bool> {
        let creature = self.game.creature(actor)?;
        let captured =
            crate::hazard_checks::capture_hazard_skill_check(&creature, actor, skill, dc, &[])
                .ok()?;
        Some(crate::abilities::resolve_escape_check(&captured.check, rng))
    }
    /// Whether a plane name is one the game declares (or `"material"`).
    pub fn plane_known(&self, name: &str) -> bool {
        self.game.plane_by_name(name).is_some()
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
