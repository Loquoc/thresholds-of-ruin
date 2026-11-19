//! Trusted, bounded backend inspection. Never part of an ordinary observation.
use crate::{ActorId, Game, ItemId, ItemLocation};
use tor_world::Location;

#[derive(Clone, Debug)]
pub struct WizardTile {
    pub wall: bool,
    pub actors: Vec<ActorId>,
    pub items: Vec<ItemId>,
    pub actors_truncated: bool,
    pub items_truncated: bool,
}
impl Game {
    pub fn wizard_location(&self, actor: ActorId) -> Option<Location> {
        self.actors.get(&actor).map(|actor| actor.location)
    }
    pub fn wizard_tile(&self, at: Location) -> Option<WizardTile> {
        if !self.world.contains(at) {
            return None;
        }
        const LIMIT: usize = 24;
        let occupants = self.actors.at(&self.world, at);
        let mut actors: Vec<_> = occupants.keys().copied().take(LIMIT + 1).collect();
        let mut items: Vec<_> = self
            .items
            .at(ItemLocation::Ground(at))
            .take(LIMIT + 1)
            .collect();
        let actors_truncated = actors.len() > LIMIT;
        let items_truncated = items.len() > LIMIT;
        actors.truncate(LIMIT);
        items.truncate(LIMIT);
        Some(WizardTile {
            wall: self.world.is_wall(at),
            actors,
            items,
            actors_truncated,
            items_truncated,
        })
    }
}

/// Actual loaded instance facts, not a freshly materialized catalog recipe.
/// This trusted backend view never grants discovery or updates equipment knowledge.
pub struct WizardItemInstance<'a> {
    pub spec: &'a crate::ItemSpec,
    pub quantity: u64,
    pub charges: Option<u16>,
    pub fuel: Option<(u64, bool)>,
    pub ground: Option<Location>,
    pub carrier: Option<ActorId>,
    pub equipped: Vec<String>,
    pub cursed: bool,
    pub protected: bool,
    pub corrosion: u32,
    pub armor_penalty: u32,
    pub armor_enchantment: u32,
    pub hit_enchantment: i32,
    pub damage_enchantment: i32,
    pub potency: i32,
}
impl Game {
    pub fn wizard_item_instance(&self, id: ItemId) -> Option<WizardItemInstance<'_>> {
        let item = self.items.get(&id)?;
        let (ground, carrier) = match item.location {
            ItemLocation::Ground(at) => (Some(at), None),
            ItemLocation::Carried(actor) => (None, Some(actor)),
        };
        let equipped = carrier
            .and_then(|actor| self.actors.get(&actor))
            .map(|actor| {
                actor
                    .equipment
                    .iter()
                    .filter(|(_, item)| **item == id)
                    .map(|(slot, _)| slot.0.to_string())
                    .collect()
            })
            .unwrap_or_default();
        Some(WizardItemInstance {
            spec: &item.spec,
            quantity: item.quantity,
            charges: item.charges,
            fuel: item.fuel.map(|fuel| (fuel.remaining, fuel.lit)),
            ground,
            carrier,
            equipped,
            cursed: item.condition.cursed,
            protected: item.condition.protected,
            corrosion: item.condition.corrosion,
            armor_penalty: item.condition.armor_penalty,
            armor_enchantment: item.condition.armor_enchantment,
            hit_enchantment: item.condition.hit_enchantment,
            damage_enchantment: item.condition.damage_enchantment,
            potency: item.condition.potency,
        })
    }
}
