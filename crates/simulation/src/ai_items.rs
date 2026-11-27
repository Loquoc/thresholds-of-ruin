//! Item decisions inspect definitions only after the acting actor knows them.
use crate::{Action, ActorId, EquipmentSlotId, Game, ItemId, Observation, TravelStep};
use tor_world::Location;

#[path = "ai_equipment.rs"]
mod equipment;
#[cfg(test)]
use crate::EquipmentSpec;
use equipment::{first_nondominated_gear, gear_score, KnownGearEvaluation};

/// Actor-bound facts for one direct-loot decision, not persistent goal memory.
/// Ranged quantities are supplied by the existing reachability-aware pair scan;
/// this owner does not turn that scan into navigation-independent desire.
/// One lazy gear baseline is shared across rows; no new quantity/cache policy.
struct DirectLootNeeds<'a> {
    game: &'a Game,
    actor: ActorId,
    view: &'a Observation,
    profile: &'a crate::ai::AiProfile,
    ranged: std::collections::BTreeMap<ItemId, u64>,
    has_healing: bool,
    supplies: std::collections::BTreeMap<crate::content_ids::SupplyId, u64>,
    wants_food: bool,
    gear: KnownGearEvaluation<'a>,
}

impl<'a> DirectLootNeeds<'a> {
    fn new(
        game: &'a Game,
        actor: ActorId,
        view: &'a Observation,
        profile: &'a crate::ai::AiProfile,
        sight: &[tor_world::SightCell],
        ranged: std::collections::BTreeMap<ItemId, u64>,
    ) -> Self {
        let has_healing = view
            .inventory
            .iter()
            .any(|item| game.known_healing_reserve(actor, item.id));
        let supplies = if profile.care
            && view
                .ground_items
                .iter()
                .any(|item| item.known_supply.is_some())
        {
            game.healing_supply_reserve(actor, sight)
        } else {
            Default::default()
        };
        let wants_food = profile.care
            && required_nutrition(view).is_some()
            && !view
                .inventory
                .iter()
                .any(|item| game.known_food(actor, item.id));
        let gear = KnownGearEvaluation::new(game, actor);
        Self {
            game,
            actor,
            view,
            profile,
            ranged,
            has_healing,
            supplies,
            wants_food,
            gear,
        }
    }

    /// Preserve ranged-first precedence, then the original eager per-row
    /// gear/healing/food/supply checks. A one-unit interest wins over supply need.
    fn quantity_for(&mut self, item: &crate::observation::GroundItemView) -> Option<u64> {
        if let Some(quantity) = self.ranged.get(&item.id) {
            return Some((*quantity).min(item.quantity));
        }
        let useful_gear = self.profile.equipment && self.gear.destination(item.id).is_some();
        let healing = self.profile.care
            && !self.has_healing
            && self.game.known_healing_reserve(self.actor, item.id);
        let food = self.wants_food && self.game.known_food(self.actor, item.id);
        let supply = self
            .profile
            .care
            .then(|| {
                let id = &item.known_supply.as_ref()?.id;
                let required = *self.supplies.get(id)?;
                let carried = self
                    .view
                    .inventory
                    .iter()
                    .filter(|owned| {
                        owned
                            .known_supply
                            .as_ref()
                            .is_some_and(|known| &known.id == id)
                    })
                    .fold(0_u64, |sum, owned| sum.saturating_add(owned.quantity));
                required
                    .checked_sub(carried)
                    .filter(|quantity| *quantity > 0)
            })
            .flatten();
        if useful_gear || healing || food {
            Some(1)
        } else {
            supply.map(|quantity| quantity.min(item.quantity))
        }
    }
}

impl Game {
    pub(crate) fn choose_known_gold(
        &self,
        actor: ActorId,
        view: &Observation,
        profile: &crate::ai::AiProfile,
        route: &mut impl FnMut(Location) -> Option<Vec<TravelStep>>,
    ) -> Option<Action> {
        let mut remaining = crate::ai_approach::DETOUR_NODE_BUDGET;
        self.choose_known_gold_bounded(actor, view, profile, route, &mut remaining)
    }

    // Preserve existing direct choices. Only a disclosed bystander blocking the
    // first remembered step licenses this bounded, knowledge-only alternative.
    pub(crate) fn choose_known_gold_bounded(
        &self,
        actor: ActorId,
        view: &Observation,
        profile: &crate::ai::AiProfile,
        route: &mut impl FnMut(Location) -> Option<Vec<TravelStep>>,
        remaining: &mut usize,
    ) -> Option<Action> {
        if !profile.gold {
            return None;
        }
        let occupied = crate::ai_approach::disclosed_bodies(actor, view);
        let mut blocked_targets = Vec::new();
        let mut direct = Vec::new();
        for item in view
            .ground_items
            .iter()
            .filter(|item| item.known_currency == Some(crate::CurrencyKind::Gold))
        {
            if self.exposed_to_visible_hostile(actor, item.location, view)
                || (profile.idle == crate::behavior::IdleBehavior::Wait
                    && item.location != view.location)
            {
                continue;
            }
            let path = if item.location == view.location {
                Vec::new()
            } else {
                let Some(path) = route(item.location) else {
                    continue;
                };
                path
            };
            let action = if let Some(step) = path.first() {
                let Some((at, _)) = self.actor_translation(actor, step.direction) else {
                    if occupied.contains(&step.destination) {
                        blocked_targets.push((item.id, item.location));
                    }
                    continue;
                };
                if self.exposed_to_visible_hostile(actor, at, view) {
                    continue;
                }
                Action::Move(step.direction)
            } else {
                Action::Take {
                    item: item.id,
                    quantity: Some(item.quantity),
                }
            };
            if !self.condition_blocks_action(actor, &action) {
                direct.push((path.len(), item.id, action));
            }
        }
        if let Some((_, _, action)) = direct.into_iter().min_by_key(|(n, id, _)| (*n, *id)) {
            return Some(action);
        }
        if blocked_targets.is_empty() {
            return None;
        }
        // Actor-owned movement durations are known; no future-world simulation.
        let planner = crate::ai_approach::KnownApproachPlanner::new(self, actor, view, &occupied)?;
        // Preserve item order and repeated destinations: all gold targets share
        // this budget, and an incomplete comparison discards every candidate.
        blocked_targets.sort();
        let mut best: Option<(u128, ItemId, usize, tor_world::Direction)> = None;
        for (item, target) in blocked_targets {
            let Some(approach) = planner.best_detour(target, remaining).ok()? else {
                continue;
            };
            if best
                .as_ref()
                .is_none_or(|old| (approach.cost, item, approach.rank) < (old.0, old.1, old.2))
            {
                best = Some((approach.cost, item, approach.rank, approach.direction));
            }
        }
        best.map(|(_, _, _, direction)| Action::Move(direction))
    }
    pub(crate) fn choose_nearby_loot(
        &self,
        actor: ActorId,
        view: &Observation,
        profile: &crate::ai::AiProfile,
        sight: &[tor_world::SightCell],
        route: &mut impl FnMut(Location) -> Option<Vec<TravelStep>>,
    ) -> Option<Action> {
        // Existing item priorities, direct routes and non-ranged care are the
        // compatibility fast path. Only otherwise-idle ranged pair acquisition
        // may recover a disclosed blocked approach.
        self.choose_nearby_loot_direct(actor, view, profile, sight, route)
            .or_else(|| {
                let mut remaining = crate::ai_approach::DETOUR_NODE_BUDGET;
                self.choose_ranged_loot_detour(actor, view, profile, route, &mut remaining)
            })
    }

    fn choose_nearby_loot_direct(
        &self,
        actor: ActorId,
        view: &Observation,
        profile: &crate::ai::AiProfile,
        sight: &[tor_world::SightCell],
        route: &mut impl FnMut(Location) -> Option<Vec<TravelStep>>,
    ) -> Option<Action> {
        if view.ground_items.is_empty() {
            return None;
        }
        let ranged = self.ranged_loot_quantities(actor, view, profile, route);
        let mut needs = DirectLootNeeds::new(self, actor, view, profile, sight, ranged);
        view.ground_items
            .iter()
            .filter_map(|item| needs.quantity_for(item).map(|quantity| (item, quantity)))
            .filter(|(item, _)| !self.exposed_to_visible_hostile(actor, item.location, view))
            .filter_map(|(item, quantity)| {
                if profile.idle == crate::behavior::IdleBehavior::Wait
                    && item.location != view.location
                {
                    return None;
                }
                // Present underfoot contact needs no remembered route. A newly
                // spawned actor may not yet have a navigation record at all.
                let path = if item.location == view.location {
                    Vec::new()
                } else {
                    route(item.location)?
                };
                let action = if let Some(step) = path.first() {
                    let (at, _) = self.actor_translation(actor, step.direction)?;
                    if self.exposed_to_visible_hostile(actor, at, view) {
                        return None;
                    }
                    Action::Move(step.direction)
                } else {
                    Action::Take {
                        item: item.id,
                        quantity: Some(quantity),
                    }
                };
                if self.condition_blocks_action(actor, &action) {
                    return None;
                }
                Some((path.len(), item.id, action))
            })
            .min_by_key(|(distance, item, _)| (*distance, *item))
            .map(|(_, _, action)| action)
    }

    pub(crate) fn known_food(&self, actor: ActorId, item: ItemId) -> bool {
        self.items.get(&item).is_some_and(|item| {
            item.spec.edible.is_some()
                && (!item.spec.concealed
                    || self
                        .actors
                        .get(&actor)
                        .is_some_and(|owner| owner.knowledge.contains(&item.spec.identity)))
        })
    }

    /// One ordinary use of an owned, exclusively restorative technique. Names,
    /// hidden piles and other actors' claims never grant an interest in supplies.
    fn healing_supply_reserve(
        &self,
        actor: ActorId,
        sight: &[tor_world::SightCell],
    ) -> std::collections::BTreeMap<crate::content_ids::SupplyId, u64> {
        let Some(creature) = self.creature(actor) else {
            return Default::default();
        };
        let rules = creature.build().ruleset();
        let mut choices: Vec<_> = creature
            .derived()
            .abilities
            .iter()
            .filter_map(|id| {
                rules
                    .ability(id)
                    .map(|definition| (definition.ai.priority, id, definition))
            })
            .collect();
        choices.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        for (_, id, definition) in choices {
            if definition.ai.hostile || definition.supply_costs.is_empty() {
                continue;
            }
            if let Some(cost) = &definition.cost {
                let available = creature.costs().available(cost.resource);
                let maximum = creature.costs().resources().maximum(cost.resource);
                if cost
                    .start
                    .checked_add(cost.resolution)
                    .is_none_or(|total| total > available)
                    || u64::from(available) * 100
                        < u64::from(maximum) * u64::from(definition.ai.minimum_resource_percent)
                {
                    continue;
                }
            }
            // Compilation resolves self reach, cooldown, unlocked strikes and
            // effective operands without allocating physical supply claims.
            let Ok(plan) = self.ability_plan_in_scene(actor, id, actor, sight) else {
                continue;
            };
            if !self.ai_plan_resources_available(&plan) || !self.effective_safe_healing_plan(&plan)
            {
                continue;
            }
            // Keep one complete recipe; component-wise minima across alternatives
            // could leave an inventory able to pay for neither.
            let mut required = std::collections::BTreeMap::<_, u64>::new();
            for cost in &definition.supply_costs {
                let count = required.entry(cost.supply.clone()).or_default();
                *count = count.saturating_add(cost.quantity);
            }
            return required;
        }
        Default::default()
    }

    pub(crate) fn exposed_to_visible_hostile(
        &self,
        actor: ActorId,
        at: Location,
        view: &Observation,
    ) -> bool {
        view.visible_actors
            .iter()
            .filter(|other| self.perceived_hostile(actor, other.id, other.disclosure))
            .any(|other| {
                other.location == at
                    || (-1..=1).any(|x| {
                        (-1..=1).any(|y| {
                            (-1..=1).any(|z| {
                                [x, y, z] != [0, 0, 0]
                                    && self.melee_neighbor(other.location, [x, y, z]) == Some(at)
                            })
                        })
                    })
            })
    }

    #[cfg(test)]
    pub(crate) fn gear_destination(
        &self,
        actor: ActorId,
        item: ItemId,
    ) -> Option<(EquipmentSlotId, Option<ItemId>)> {
        KnownGearEvaluation::new(self, actor).destination(item)
    }

    /// Structural admission and remembered scoring share one reusable workspace
    /// across all launchers and sockets. The caller bounds the candidate list.
    pub(crate) fn ranged_gear_destination(
        &self,
        workspace: &mut Option<Game>,
        actor: ActorId,
        item: ItemId,
        prefer_ranged: bool,
    ) -> Option<(EquipmentSlotId, Option<ItemId>)> {
        let gear = self.known_gear(actor, item)?;
        let owner = self.actors.get(&actor)?;
        let projected = workspace.get_or_insert_with(|| self.clone());
        projected.project_known_loadout_in_place(self, actor, owner.equipment.clone())?;
        let before = gear_score(projected, actor)?;
        for index in self.known_gear_socket_indices(actor, &gear.slot)? {
            let slot = EquipmentSlotId(index as u16);
            let Some(previous) = self.known_gear_replacement(actor, slot) else {
                continue;
            };
            match projected.project_gear_candidate_in_place(self, actor, item, slot) {
                Ok(()) => {}
                Err(equipment::CandidateProjectionFailure::Rejected) => continue,
                Err(equipment::CandidateProjectionFailure::ItemUnavailable) => return None,
            }
            if !projected.ranged_loadout_has_budget(actor, item, slot) {
                continue;
            }
            let Some(mut after) = gear_score(projected, actor) else {
                continue;
            };
            if prefer_ranged {
                // Explicitly higher Launch priority permits only the attack
                // trade-off, never a loss of health, protection or recovery.
                after.attack = before.attack.clone();
            }
            if after == before || after.dominates(&before) {
                return Some((slot, previous));
            }
        }
        None
    }

    #[cfg(test)]
    fn gear_upgrade(&self, actor: ActorId, item: ItemId, slot: EquipmentSlotId) -> bool {
        KnownGearEvaluation::new(self, actor).upgrade(item, slot)
    }

    /// Recover a free primary grip when a depleted secondary launcher blocks a
    /// strictly better carried melee option. No candidate changes the real game;
    /// one workspace and at most sixteen candidate folds bound the fallback.
    fn choose_depleted_launcher_recovery(
        &self,
        actor: ActorId,
        view: &Observation,
    ) -> Option<Action> {
        if view.inventory.len() > 128 {
            return None;
        }
        let owner = self.actors.get(&actor)?;
        let primary = owner.anatomy.primary_attack_socket.socket()?;
        if owner.equipment.contains_key(&primary) {
            return None;
        }
        let empty = owner.equipment.iter().find_map(|(_, item)| {
            let family = self.known_launcher_family(actor, *item)?;
            if owner.known_equipment.curse(*item) == Some(true)
                || view
                    .inventory
                    .iter()
                    .filter(|row| row.identified)
                    .any(|row| self.known_payload_family(actor, row.id) == Some(&family))
            {
                return None;
            }
            Some(*item)
        })?;
        let mut workspace = self.clone();
        workspace.project_known_loadout_in_place(self, actor, owner.equipment.clone())?;
        let before = gear_score(&workspace, actor)?;
        let mut equipment = owner.equipment.clone();
        equipment.retain(|_, item| *item != empty);
        let mut candidates = 0;
        for row in view
            .inventory
            .iter()
            .filter(|row| row.identified && row.id != empty)
        {
            if owner.equipment.values().any(|item| *item == row.id) {
                continue;
            }
            let Some(gear) = self.known_gear(actor, row.id) else {
                continue;
            };
            if gear.attack.is_none()
                || owner.anatomy.slots.get(usize::from(primary.0)) != Some(&gear.slot)
            {
                continue;
            }
            candidates += 1;
            if candidates > 16 {
                break;
            }
            workspace.project_known_loadout_in_place(self, actor, equipment.clone())?;
            if workspace
                .capture_equip_allocation(actor, row.id, primary)
                .is_err()
            {
                continue;
            }
            let mut equipped = equipment.clone();
            equipped.insert(primary, row.id);
            if workspace
                .project_known_loadout_in_place(self, actor, equipped)
                .is_none()
            {
                continue;
            }
            if gear_score(&workspace, actor).is_some_and(|after| after.dominates(&before)) {
                return Some(Action::Unequip { item: empty });
            }
        }
        None
    }

    pub(crate) fn choose_gear(&self, actor: ActorId, view: &Observation) -> Option<Action> {
        if let Some(action) = self.choose_ranged_gear(actor, view) {
            return Some(action);
        }
        if let Some(action) = self.choose_depleted_launcher_recovery(actor, view) {
            return Some(action);
        }
        let retained = self.retained_ranged_weapon(actor, view);
        let mut gear = KnownGearEvaluation::new(self, actor);
        let choices: Vec<_> = view
            .inventory
            .iter()
            .filter_map(|item| {
                gear.destination(item.id)
                    .filter(|(_, previous)| previous.is_none() || *previous != retained)
                    .map(|destination| (item.id, destination))
            })
            .collect();
        // Compare complete prospective loadouts, including item interactions,
        // rather than folding a parallel set of equipment-only stat formulas.
        let (item, (slot, old)) = first_nondominated_gear(&choices, |(item, (slot, _))| {
            self.gear_projection(actor, *item, *slot)
                .and_then(|game| gear_score(&game, actor))
        })?;
        Some(old.map_or(
            Action::Equip {
                item: *item,
                slot: *slot,
            },
            |item| Action::Unequip { item },
        ))
    }
}

pub(crate) fn required_nutrition(view: &Observation) -> Option<crate::nutrition::Hunger> {
    match view.combat.as_ref()?.own_stats.as_ref()?.nutrition() {
        crate::nutrition::NutritionView::Required { hunger, .. } => Some(hunger),
        crate::nutrition::NutritionView::Exempt => None,
    }
}
#[cfg(test)]
#[path = "ai_items_tests.rs"]
mod tests;
