//! Item decisions inspect definitions only after the acting actor knows them.
use crate::{Action, ActorId, EquipmentSlotId, Game, ItemId, Observation, TravelStep};
use tor_world::Location;

#[path = "ai_equipment.rs"]
mod equipment;
#[cfg(test)]
use crate::EquipmentSpec;
use equipment::{first_nondominated_gear, gear_score, KnownGearEvaluation};

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
        let has_healing = view
            .inventory
            .iter()
            .any(|item| self.known_healing_reserve(actor, item.id));
        let supplies = if profile.care
            && view
                .ground_items
                .iter()
                .any(|item| item.known_supply.is_some())
        {
            self.healing_supply_reserve(actor, sight)
        } else {
            Default::default()
        };
        let wants_food = profile.care
            && required_nutrition(view).is_some()
            && !view
                .inventory
                .iter()
                .any(|item| self.known_food(actor, item.id));
        let mut gear = KnownGearEvaluation::new(self, actor);
        view.ground_items
            .iter()
            .filter_map(|item| {
                if let Some(quantity) = ranged.get(&item.id) {
                    return Some((item, (*quantity).min(item.quantity)));
                }
                let useful_gear = profile.equipment && gear.destination(item.id).is_some();
                let healing =
                    profile.care && !has_healing && self.known_healing_reserve(actor, item.id);
                let food = wants_food && self.known_food(actor, item.id);
                let supply = profile
                    .care
                    .then(|| {
                        let id = &item.known_supply.as_ref()?.id;
                        let required = *supplies.get(id)?;
                        let carried = view
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
                    Some((item, 1))
                } else {
                    supply.map(|quantity| (item, quantity.min(item.quantity)))
                }
            })
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
mod tests {
    use super::*;
    use crate::EquipmentSlot;
    use crate::{
        combat::DamageType,
        content_ids::DescriptorId,
        rules::{DefenseKind, Modifier, ModifierOp, Scope, Selector, Stat},
    };

    fn fixture() -> (Game, ActorId, Location) {
        let mut species = crate::test_creatures::species();
        species.default_attributes =
            crate::attributes::Attributes::new(crate::attributes::ByAttribute {
                strength: 5,
                speed: 1,
                ..Default::default()
            })
            .unwrap();
        let mut game = Game::two_room_in_stone(
            42,
            crate::test_creatures::content_with(species.clone(), vec![]),
        );
        let at = Location {
            region: tor_world::RegionId(1),
            position: tor_world::Position { x: 1, y: 1, z: 0 },
        };
        let actor = game
            .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        crate::test_creatures::configure(&mut game, actor, "neutral", species);
        (game, actor, at)
    }

    fn gold_fixture() -> (Game, ActorId, Location) {
        let (mut game, actor, at) = fixture();
        game.configure_ai(
            actor,
            crate::ai::AiProfile {
                idle: crate::behavior::IdleBehavior::Wait,
                gold: true,
                ..Default::default()
            },
        )
        .unwrap();
        game.refresh_navigation();
        (game, actor, at)
    }

    fn gold(concealed: bool) -> crate::ItemSpec {
        let mut item =
            crate::ItemSpec::ordinary("item.test.gold-coin".parse().unwrap(), "gold coin".into());
        item.class = crate::ItemClass::Coin;
        item.stackable = true;
        item.currency = Some(crate::CurrencyKind::Gold);
        item.concealed = concealed;
        item
    }

    fn healing_potion() -> crate::ItemSpec {
        let mut item = crate::ItemSpec::ordinary(
            "item.test.known-restorative".parse().unwrap(),
            "known restorative".into(),
        );
        item.class = crate::ItemClass::Potion;
        item.consumable = Some(crate::ConsumableSpec {
            descriptors: vec![],
            discovery: crate::ItemDiscovery::SelfEffect,
            effects: vec![crate::rules::Effect::Heal {
                amount: crate::rules::AmountExpr::Fixed { amount: 2 },
            }],
        });
        item
    }

    fn neighbor(game: &mut Game, actor: ActorId, at: Location) -> ActorId {
        let build = game.creature(actor).unwrap().build().clone();
        let other = game
            .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_creature(
            other,
            crate::CreatureIdentity {
                corpse_identity: "item.corpse.test.neutral-neighbor".parse().unwrap(),
                name: "neutral neighbor".into(),
                faction: "neutral".into(),
            },
            build,
        )
        .unwrap();
        game.refresh_navigation();
        other
    }

    #[test]
    fn known_gold_has_no_gameplay_distance_cutoff_and_preserves_disclosure() {
        let (mut game, actor, at) = gold_fixture();
        let origin = Location {
            position: tor_world::Position {
                x: 0,
                ..at.position
            },
            ..at
        };
        game.teleport(actor, origin).unwrap();
        let destination = Location {
            position: tor_world::Position {
                x: 4,
                ..at.position
            },
            ..at
        };
        game.place_item_stack(20, destination, None, 7, gold(false))
            .unwrap();
        game.refresh_navigation();
        let view = game.observe(actor).unwrap();
        let mut profile = game.combat.ai[&actor].profile.clone();
        let path = game
            .route_search(actor)
            .unwrap()
            .route(destination)
            .unwrap();
        assert_eq!(path.len(), 4);
        let mut calls = 0;
        let mut route = |_| {
            calls += 1;
            Some(path.clone())
        };
        assert_eq!(
            game.choose_known_gold(actor, &view, &profile, &mut route),
            None,
            "IdleWait retains its station"
        );
        assert_eq!(calls, 0);
        profile.idle = crate::behavior::IdleBehavior::Explore;
        let before = game.clone();
        let mut calls = 0;
        let mut route = |_| {
            calls += 1;
            Some(path.clone())
        };
        assert_eq!(
            game.choose_known_gold(actor, &view, &profile, &mut route),
            Some(Action::Move(path[0].direction))
        );
        assert_eq!(calls, 1, "one route lookup per disclosed candidate");
        assert_eq!(game, before, "choice remains pure");
        game.configure_ai(actor, profile.clone()).unwrap();
        let searches = crate::diagnostics::work_counts().route_searches;
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Move(path[0].direction)
        );
        assert_eq!(
            crate::diagnostics::work_counts().route_searches - searches,
            1,
            "ordinary decision shares one remembered-topology search"
        );

        assert_eq!(
            game.choose_known_gold(actor, &view, &profile, &mut |_| None),
            None,
            "unreachable goal"
        );
        let mut unknown = view.clone();
        unknown
            .ground_items
            .iter_mut()
            .find(|item| item.id == ItemId(20))
            .unwrap()
            .known_currency = None;
        assert_eq!(
            game.choose_known_gold(actor, &unknown, &profile, &mut |_| panic!(
                "unknown currency must not route"
            )),
            None
        );
    }

    #[test]
    fn distant_gold_candidates_keep_distance_then_id_ranking() {
        let (mut game, actor, _) = gold_fixture();
        let region = tor_world::RegionId(901);
        game.world
            .add_chamber(tor_world::Region {
                id: region,
                name: "competing gold".into(),
                bounds: tor_world::Extent::new(6, 9, 2).unwrap(),
            })
            .unwrap();
        let at = Location {
            region,
            position: tor_world::Position { x: 0, y: 4, z: 0 },
        };
        let origin = Location {
            position: tor_world::Position {
                x: 0,
                ..at.position
            },
            ..at
        };
        game.teleport(actor, origin).unwrap();
        let east = Location {
            position: tor_world::Position {
                x: 4,
                y: 0,
                ..at.position
            },
            ..at
        };
        let diagonal = Location {
            position: tor_world::Position {
                x: 4,
                y: 8,
                ..at.position
            },
            ..at
        };
        game.place_item_stack(20, east, None, 7, gold(false))
            .unwrap();
        game.place_item_stack(19, diagonal, None, 7, gold(false))
            .unwrap();
        game.refresh_navigation();
        let mut profile = game.combat.ai[&actor].profile.clone();
        profile.idle = crate::behavior::IdleBehavior::Explore;
        let mut search = game.route_search(actor).unwrap();
        let a = search.route(east).unwrap();
        let b = search.route(diagonal).unwrap();
        assert_eq!(a.len(), 4);
        assert_eq!(b.len(), 4);
        assert_ne!(
            a[0].direction, b[0].direction,
            "ranking witness must distinguish first actions"
        );
        let view = game.observe(actor).unwrap();
        assert_eq!(
            game.choose_known_gold(actor, &view, &profile, &mut |at| search.route(at).ok()),
            Some(Action::Move(b[0].direction)),
            "same-distance lower item id wins"
        );
        let nearer = Location {
            position: tor_world::Position {
                x: 1,
                ..at.position
            },
            ..at
        };
        game.place_item_stack(21, nearer, None, 7, gold(false))
            .unwrap();
        game.refresh_navigation();
        let view = game.observe(actor).unwrap();
        let mut search = game.route_search(actor).unwrap();
        let nearest = search.route(nearer).unwrap();
        assert_eq!(nearest.len(), 1);
        assert_eq!(
            game.choose_known_gold(actor, &view, &profile, &mut |at| search.route(at).ok()),
            Some(Action::Move(nearest[0].direction)),
            "shorter route outranks lower item id"
        );
    }

    #[test]
    fn useful_loot_has_no_gameplay_distance_cutoff_but_blocked_steps_reject() {
        let (mut game, actor, at) = gold_fixture();
        let origin = Location {
            position: tor_world::Position {
                x: 0,
                y: 2,
                ..at.position
            },
            ..at
        };
        game.teleport(actor, origin).unwrap();
        let destination = Location {
            position: tor_world::Position {
                x: 4,
                y: 2,
                ..at.position
            },
            ..at
        };
        game.place_item_stack(21, destination, None, 1, healing_potion())
            .unwrap();
        game.refresh_navigation();
        let view = game.observe(actor).unwrap();
        let path = game
            .route_search(actor)
            .unwrap()
            .route(destination)
            .unwrap();
        assert_eq!(path.len(), 4);
        let profile = crate::ai::AiProfile::general();
        assert_eq!(
            game.choose_nearby_loot(actor, &view, &profile, &[], &mut |_| Some(path.clone())),
            Some(Action::Move(path[0].direction))
        );
        let mut waiting = profile.clone();
        waiting.idle = crate::behavior::IdleBehavior::Wait;
        assert_eq!(
            game.choose_nearby_loot(actor, &view, &waiting, &[], &mut |_| panic!(
                "stationary policy must not route off-cell"
            )),
            None
        );
        assert_eq!(
            game.choose_nearby_loot(actor, &view, &profile, &[], &mut |_| None),
            None
        );
        let mut gone = view.clone();
        gone.ground_items.clear();
        assert_eq!(
            game.choose_nearby_loot(actor, &gone, &profile, &[], &mut |_| panic!(
                "absent item must not route"
            )),
            None
        );
        // A formerly remembered path does not authorize movement through a new
        // obstacle. Ordinary authoritative movement remains the final gate.
        game.set_wall(path[0].destination, true).unwrap();
        assert_eq!(
            game.choose_nearby_loot(actor, &view, &profile, &[], &mut |_| Some(path.clone())),
            None
        );
    }

    #[test]
    fn known_gold_precedes_idle_without_enabling_other_loot_policies() {
        let (mut game, actor, at) = gold_fixture();
        game.place_item_stack(19, at, None, 1, healing_potion())
            .unwrap();
        assert_eq!(game.choose_ai(actor).unwrap().action, Action::Wait);
        let profile = &game.combat.ai[&actor].profile;
        assert!(!profile.care && !profile.equipment && !profile.loot && !profile.doors);
        game.place_item_stack(20, at, None, 7, gold(false)).unwrap();
        let view = game.observe(actor).unwrap();
        assert_eq!(
            view.ground_items
                .iter()
                .find(|item| item.id == ItemId(20))
                .unwrap()
                .known_currency,
            Some(crate::CurrencyKind::Gold)
        );
        let before = game.clone();
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Take {
                item: ItemId(20),
                quantity: Some(7)
            }
        );
        assert_eq!(
            game, before,
            "A preview must not move gold or advance its RNG"
        );
        game.combat.ai.get_mut(&actor).unwrap().profile.gold = false;
        assert_eq!(game.choose_ai(actor).unwrap().action, Action::Wait);
    }

    #[test]
    fn known_gold_uses_a_disclosed_short_route_only_when_idle_policy_allows_it() {
        let (mut game, actor, at) = gold_fixture();
        let destination = Location {
            position: tor_world::Position {
                x: at.position.x + 2,
                ..at.position
            },
            ..at
        };
        game.place_item_stack(20, destination, None, 7, gold(false))
            .unwrap();
        game.refresh_navigation();
        assert_eq!(game.choose_ai(actor).unwrap().action, Action::Wait);
        game.combat.ai.get_mut(&actor).unwrap().profile.idle =
            crate::behavior::IdleBehavior::Explore;
        let before = game.clone();
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Move(tor_world::Direction::East)
        );
        assert_eq!(game, before);
    }

    #[test]
    fn concealed_gold_and_another_actors_private_purse_do_not_change_idle_choice() {
        let (mut game, actor, at) = gold_fixture();
        let other_at = Location {
            position: tor_world::Position {
                x: at.position.x + 1,
                ..at.position
            },
            ..at
        };
        let other = neighbor(&mut game, actor, other_at);
        let mut hidden = game.clone();
        hidden
            .place_item_stack(21, other_at, Some(other), 99, gold(false))
            .unwrap();
        assert_eq!(game.observe(actor).unwrap(), hidden.observe(actor).unwrap());
        assert_eq!(hidden.choose_ai(actor).unwrap().action, Action::Wait);
        let mut ordinary_lookalike = game.clone();
        game.place_item_stack(20, at, None, 7, gold(true)).unwrap();
        let mut unknown_non_currency = gold(true);
        unknown_non_currency.currency = None;
        ordinary_lookalike
            .place_item_stack(20, at, None, 7, unknown_non_currency)
            .unwrap();
        assert_eq!(
            game.observe(actor).unwrap(),
            ordinary_lookalike.observe(actor).unwrap()
        );
        assert_eq!(game.choose_ai(actor).unwrap().action, Action::Wait);
        assert_eq!(
            ordinary_lookalike.choose_ai(actor).unwrap().action,
            Action::Wait
        );
        game.identify_item(actor, ItemId(20)).unwrap();
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Take {
                item: ItemId(20),
                quantity: Some(7)
            }
        );
    }

    #[test]
    fn healing_retaliation_and_escape_retain_priority_over_visible_gold() {
        let (mut game, actor, at) = gold_fixture();
        game.place_item_stack(20, at, None, 7, gold(false)).unwrap();
        let mut wounded = game.clone();
        wounded.combat.ai.get_mut(&actor).unwrap().profile.care = true;
        wounded
            .place_item_stack(21, at, Some(actor), 1, healing_potion())
            .unwrap();
        let hp = wounded.health(actor).unwrap().0;
        crate::test_creatures::injure(&mut wounded, actor, u32::try_from(hp - 1).unwrap()).unwrap();
        assert_eq!(
            wounded.choose_ai(actor).unwrap().action,
            Action::Drink { item: ItemId(21) }
        );

        let other_at = Location {
            position: tor_world::Position {
                x: at.position.x + 1,
                ..at.position
            },
            ..at
        };
        let other = neighbor(&mut game, actor, other_at);
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Take {
                item: ItemId(20),
                quantity: Some(7)
            },
            "A neutral visible neighbor alone is not provocation"
        );
        let basic = crate::content_ids::AbilityId::new("basic_melee").unwrap();
        let plan = game.ability_plan(other, &basic, actor).unwrap();
        let outcome = game
            .resolve_strike(
                plan.as_ability().unwrap(),
                0,
                None,
                &mut crate::combat::PreparationProgress::Ability {
                    next_strike: 1,
                    successful_primaries: 0,
                    attempted_riders: [0; 2],
                    linked_hold: None,
                },
            )
            .unwrap();
        assert!(outcome.target_participated());
        let retaliation = game.choose_ai(actor).unwrap();
        assert_eq!(retaliation.ai.state, crate::ai::State::Attack);
        assert_eq!(
            retaliation.action,
            Action::UseAbility {
                id: basic,
                invocation: crate::abilities::AbilityInvocation::actor_target(other),
                supplies: Default::default(),
            }
        );
        let hp = game.health(actor).unwrap().0;
        crate::test_creatures::injure(&mut game, actor, u32::try_from(hp - 1).unwrap()).unwrap();
        let escape = game.choose_ai(actor).unwrap();
        assert_eq!(escape.ai.state, crate::ai::State::Flee);
        assert!(matches!(escape.action, Action::Move(_)));
    }

    fn add(game: &mut Game, actor: ActorId, at: Location, id: u64, gear: EquipmentSpec) {
        let mut item = crate::ItemSpec::ordinary(
            format!("item.test.gear-{id}").parse().unwrap(),
            format!("gear {id}"),
        );
        item.class = if gear.slot == EquipmentSlot::new("weapon").unwrap() {
            crate::ItemClass::Weapon
        } else {
            crate::ItemClass::Armor
        };
        item.equipment = Some(gear);
        game.place_item_stack(id, at, Some(actor), 1, item).unwrap();
    }
    fn defense(value: i64) -> Modifier {
        Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::Defense {
                    defense: DefenseKind::Cognitive,
                },
                value,
            },
        }
    }
    fn reduction(amount: u32) -> Modifier {
        Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Reduction {
                selector: Selector::Category {
                    category: DamageType::Impact,
                },
                amount,
            },
        }
    }
    fn weapon(
        skill: crate::attributes::Skill,
        bonus: i32,
        wind: u64,
        descriptor: &str,
        count: u16,
    ) -> EquipmentSpec {
        let component = crate::damage::DamageComponent::rolled(
            DamageType::Energy,
            Some(DescriptorId::new(descriptor).unwrap()),
            crate::dice::DicePool::new(count, 6, 0).unwrap(),
        );
        let primary = component.key();
        EquipmentSpec {
            periodic_relocation: None,
            potency_modifiers: Vec::new(),
            weapon_profile: None,
            armor: None,
            slot: EquipmentSlot::new("weapon").unwrap(),
            attack: Some(
                crate::attacks::MeleeAttack::new(
                    skill,
                    bonus,
                    wind,
                    40,
                    crate::damage::DamageSpec::new(vec![component], Some(primary)).unwrap(),
                )
                .unwrap(),
            ),
            modifiers: vec![],
        }
    }
    #[test]
    fn gear_ranking_caches_available_and_unavailable_scores_in_input_order() {
        let (game, actor, _) = fixture();
        // Independent expected positions cover empty, unavailable, increasing,
        // decreasing, equal and incomparable scores. Health and defense are
        // independent dominance dimensions; unavailable entries never win.
        type RankingCase = (Vec<Option<(u64, i64)>>, Option<usize>);
        let cases: &[RankingCase] = &[
            (vec![], None),
            (vec![None], None),
            (vec![Some((1, 1))], Some(0)),
            (vec![Some((1, 1)), Some((2, 2)), Some((3, 3))], Some(2)),
            (vec![Some((3, 3)), Some((2, 2)), Some((1, 1))], Some(0)),
            (vec![Some((2, 2)), Some((2, 2))], Some(0)),
            (vec![Some((3, 1)), Some((1, 3))], Some(0)),
            (vec![None, Some((1, 1)), None, Some((2, 2))], Some(3)),
            (vec![None, None], None),
        ];
        for (values, expected) in cases {
            let choices: Vec<_> = (0..values.len()).collect();
            let mut calls = vec![0; choices.len()];
            let selected = first_nondominated_gear(&choices, |&index| {
                calls[index] += 1;
                let (health, defense_value) = values[index]?;
                let mut score = gear_score(&game, actor).unwrap();
                score.maximum_health = health;
                score.stats.insert(
                    Stat::Defense {
                        defense: DefenseKind::Cognitive,
                    },
                    defense_value,
                );
                Some(score)
            });
            assert_eq!(selected.copied(), *expected);
            assert_eq!(
                calls,
                vec![1; choices.len()],
                "including unavailable scores"
            );
        }
    }

    fn ranking_armor(value: i64) -> EquipmentSpec {
        EquipmentSpec {
            periodic_relocation: None,
            potency_modifiers: Vec::new(),
            weapon_profile: None,
            armor: None,
            slot: EquipmentSlot::new("head_armor").unwrap(),
            attack: None,
            modifiers: vec![defense(value)],
        }
    }

    #[test]
    fn shared_gear_projection_preserves_failure_kinds_and_resets_workspace() {
        let (mut game, actor, at) = fixture();
        add(&mut game, actor, at, 10, ranking_armor(3));
        let original = game.clone();
        let slot = EquipmentSlotId(3);
        let mut workspace = game.clone();
        assert_eq!(
            workspace.project_gear_candidate_in_place(&game, actor, ItemId(10), slot),
            Ok(()),
            "positive candidate must pass ordinary admission and both folds"
        );
        let expected = gear_score(&workspace, actor).unwrap();
        assert_eq!(
            workspace.project_gear_candidate_in_place(&game, actor, ItemId(9999), slot),
            Err(equipment::CandidateProjectionFailure::ItemUnavailable),
            "missing unit aborts ranged comparison, not merely this socket"
        );
        assert_eq!(
            workspace.project_gear_candidate_in_place(
                &game,
                actor,
                ItemId(10),
                EquipmentSlotId(u16::MAX),
            ),
            Err(equipment::CandidateProjectionFailure::Rejected),
            "ordinary admission rejection allows the next socket"
        );
        assert_eq!(
            workspace.project_gear_candidate_in_place(&game, actor, ItemId(10), slot),
            Ok(())
        );
        assert!(gear_score(&workspace, actor).unwrap() == expected);
        assert_eq!(game, original);
    }

    #[test]
    fn ordinary_gear_scan_shares_one_known_baseline_and_preserves_choice() {
        for count in [1_u64, 4, 16, 64] {
            let (mut game, actor, at) = fixture();
            for index in 0..count {
                add(
                    &mut game,
                    actor,
                    at,
                    100 + index,
                    ranking_armor(index as i64 + 1),
                );
            }
            let view = game.observe(actor).unwrap();
            let original = game.clone();
            let before = crate::diagnostics::work_counts();
            assert_eq!(
                game.choose_gear(actor, &view),
                Some(Action::Equip {
                    item: ItemId(99 + count),
                    slot: EquipmentSlotId(3),
                })
            );
            let after = crate::diagnostics::work_counts();
            // Each candidate still gets eligibility AND final ranking. Only
            // the immutable known baseline is shared, never equip admission.
            assert_eq!(
                after.gear_projections - before.gear_projections,
                2 * count as usize
            );
            assert_eq!(
                after.game_clones - before.game_clones,
                2 * count as usize + 1
            );
            assert_eq!(game, original);
        }
    }

    #[test]
    fn known_gear_shared_baseline_matches_fresh_loadouts_after_rejection() {
        let (mut game, actor, at) = fixture();
        for (id, value) in [(10, 3), (11, 1), (12, 3), (13, 5)] {
            add(&mut game, actor, at, id, ranking_armor(value));
        }
        let slot = EquipmentSlotId(3);
        game.equip_authored(actor, ItemId(10), slot).unwrap();
        let original = game.clone();
        for order in [[11, 12, 13], [13, 12, 11]] {
            let mut comparison = KnownGearEvaluation::new(&game, actor);
            assert_eq!(comparison.destination(ItemId(9999)), None);
            assert!(
                comparison.baseline.is_none(),
                "unavailable item must remain lazy"
            );
            for id in order {
                let candidate = game.gear_projection(actor, ItemId(id), slot).unwrap();
                let baseline = game
                    .known_loadout_projection(actor, game.actors[&actor].equipment.clone())
                    .unwrap();
                let expected = gear_score(&candidate, actor)
                    .unwrap()
                    .dominates(&gear_score(&baseline, actor).unwrap());
                assert_eq!(expected, id == 13, "equal or weaker gear is not an upgrade");
                assert_eq!(
                    comparison.destination(ItemId(id)),
                    expected.then_some((slot, Some(ItemId(10))))
                );
                assert!(comparison.baseline.as_ref().is_some_and(Option::is_some));
            }
        }
        assert_eq!(game, original);
    }

    #[test]
    fn increasing_gear_ranking_projects_once_per_choice_and_keeps_original_game() {
        for count in [1_u64, 4, 16, 64] {
            let (mut game, actor, at) = fixture();
            let slot = EquipmentSlotId(3);
            for index in 0..count {
                add(
                    &mut game,
                    actor,
                    at,
                    100 + index,
                    ranking_armor(index as i64 + 1),
                );
            }
            let original = game.clone();
            // Eligibility is deliberately completed before counting ranking
            // projections, so setup/baseline folds are not misreported as saved.
            let choices: Vec<_> = (0..count)
                .map(|index| {
                    let item = ItemId(100 + index);
                    assert_eq!(game.gear_destination(actor, item), Some((slot, None)));
                    (item, (slot, None::<ItemId>))
                })
                .collect();
            let ranking_start = crate::diagnostics::work_counts();
            let mut ranking_projections = 0;
            let selected = first_nondominated_gear(&choices, |(item, (slot, _))| {
                ranking_projections += 1;
                let projected = game
                    .gear_projection(actor, *item, *slot)
                    .expect("validated fixture must project successfully");
                Some(gear_score(&projected, actor).expect("fixture must be scoreable"))
            });
            assert_eq!(selected, Some(&(ItemId(99 + count), (slot, None))));
            assert_eq!(ranking_projections, count);
            let ranking_end = crate::diagnostics::work_counts();
            assert_eq!(
                ranking_end.gear_projections - ranking_start.gear_projections,
                usize::try_from(count).unwrap()
            );
            assert_eq!(
                ranking_end.game_clones - ranking_start.game_clones,
                usize::try_from(count).unwrap()
            );
            assert_eq!(game, original);
            assert_eq!(
                game.choose_gear(actor, &game.observe(actor).unwrap()),
                Some(Action::Equip {
                    item: ItemId(99 + count),
                    slot
                })
            );
            assert_eq!(game, original);
        }
    }

    #[test]
    fn ordinary_gear_ranking_preserves_equal_order_and_occupied_socket_action() {
        for occupied in [false, true] {
            let (mut game, actor, at) = fixture();
            let slot = EquipmentSlotId(3);
            if occupied {
                add(&mut game, actor, at, 90, ranking_armor(1));
                game.equip_authored(actor, ItemId(90), slot).unwrap();
            }
            add(&mut game, actor, at, 100, ranking_armor(2));
            add(&mut game, actor, at, 101, ranking_armor(2));
            game.configure_ai(
                actor,
                crate::ai::AiProfile {
                    idle: crate::behavior::IdleBehavior::Wait,
                    equipment: true,
                    ..Default::default()
                },
            )
            .unwrap();
            let view = game.observe(actor).unwrap();
            let before = game.clone();
            let expected = if occupied {
                Action::Unequip { item: ItemId(90) }
            } else {
                Action::Equip {
                    item: ItemId(100),
                    slot,
                }
            };
            assert_eq!(game.choose_gear(actor, &view), Some(expected.clone()));
            assert_eq!(game.choose_ai(actor).unwrap().action, expected);
            assert_eq!(game, before);
        }
    }

    #[test]
    fn ordered_strikes_no_damage_natural_source_still_has_a_gear_score() {
        let mut species = crate::test_creatures::species();
        species.melee = crate::attacks::NaturalAttack::new(
            crate::attributes::Skill::HeavyWeaponry,
            0,
            60,
            40,
            vec![crate::rules::StrikeDef {
                cancellable: false,
                offset: 0,
                unlock: None,
                check: None,
                on_success: vec![],
                on_failure: vec![],
                descriptors: vec![],
                riders: vec![],
            }],
        )
        .unwrap();
        let mut game = Game::two_room_in_stone(
            42,
            crate::test_creatures::content_with(species.clone(), vec![]),
        );
        let actor = game
            .spawn_actor(
                Location {
                    region: tor_world::RegionId(1),
                    position: tor_world::Position { x: 1, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        crate::test_creatures::configure(&mut game, actor, "neutral", species);
        let before = game.clone();
        let score = gear_score(&game, actor).expect("status-only attacks remain scoreable");
        assert!(score.attack.damage.is_empty());
        assert_eq!(game, before);
    }

    #[test]
    fn wide_scalar_gear_health_dominance_does_not_wrap_through_signed_stats() {
        let (game, actor, _) = fixture();
        let mut old = gear_score(&game, actor).unwrap();
        let mut new = gear_score(&game, actor).unwrap();
        old.maximum_health = i64::MAX as u64;
        new.maximum_health = u64::MAX;
        assert!(new.dominates(&old));
        assert!(!old.dominates(&new));
        old.maximum_health = u64::MAX;
        assert!(!new.dominates(&old));
    }

    #[test]
    fn upgrades_do_not_trade_away_cached_protection_for_more_defense() {
        let (mut game, actor, at) = fixture();
        for (id, modifiers) in [
            (10, vec![defense(1), reduction(2)]),
            (11, vec![defense(3), reduction(2)]),
            (12, vec![defense(3)]),
            (13, vec![defense(1), reduction(2)]),
        ] {
            add(
                &mut game,
                actor,
                at,
                id,
                EquipmentSpec {
                    periodic_relocation: None,
                    potency_modifiers: Vec::new(),
                    weapon_profile: None,
                    armor: None,
                    slot: EquipmentSlot::new("head_armor").unwrap(),
                    attack: None,
                    modifiers,
                },
            );
        }
        game.equip_authored(actor, ItemId(10), EquipmentSlotId(3))
            .unwrap();
        assert!(game.gear_upgrade(actor, ItemId(11), EquipmentSlotId(3)));
        assert!(!game.gear_upgrade(actor, ItemId(12), EquipmentSlotId(3)));
        assert!(!game.gear_upgrade(actor, ItemId(13), EquipmentSlotId(3)));
    }
    #[test]
    fn recovery_gear_uses_derived_period_and_quiet_reset_policy() {
        let (mut game, actor, at) = fixture();
        for (id, stat, value) in [
            (10, Stat::HealthRecoveryPeriodPercent, -50),
            (11, Stat::HealthRecoveryResetPercent, -50),
        ] {
            add(
                &mut game,
                actor,
                at,
                id,
                EquipmentSpec {
                    periodic_relocation: None,
                    potency_modifiers: Vec::new(),
                    weapon_profile: None,
                    armor: None,
                    slot: EquipmentSlot::new("head_armor").unwrap(),
                    attack: None,
                    modifiers: vec![Modifier {
                        cancellable: false,
                        scope: Scope::Always,
                        op: ModifierOp::Add { stat, value },
                    }],
                },
            );
            assert!(game.gear_upgrade(actor, ItemId(id), EquipmentSlotId(3)));
        }
        let before = game.clone();
        assert_eq!(
            game.choose_gear(actor, &game.observe(actor).unwrap()),
            Some(Action::Equip {
                item: ItemId(10),
                slot: EquipmentSlotId(3)
            }),
        );
        assert_eq!(game, before);
    }
    #[test]
    fn defense_upgrades_do_not_trade_away_recovery_period_or_quiet_progress() {
        let (mut game, actor, at) = fixture();
        for (id, stat, value) in [
            (10, Stat::HealthRecoveryPeriodPercent, 50),
            (11, Stat::HealthRecoveryResetPercent, 50),
        ] {
            // A lower baseline reset policy makes worsening it observable.
            // Use authored gear for both current and candidate projections.
            if id == 10 {
                add(
                    &mut game,
                    actor,
                    at,
                    20,
                    EquipmentSpec {
                        periodic_relocation: None,
                        potency_modifiers: Vec::new(),
                        weapon_profile: None,
                        armor: None,
                        slot: EquipmentSlot::new("body_armor").unwrap(),
                        attack: None,
                        modifiers: vec![Modifier {
                            cancellable: false,
                            scope: Scope::Always,
                            op: ModifierOp::Add {
                                stat: Stat::HealthRecoveryResetPercent,
                                value: -50,
                            },
                        }],
                    },
                );
                game.equip_authored(actor, ItemId(20), EquipmentSlotId(2))
                    .unwrap();
            }
            add(
                &mut game,
                actor,
                at,
                id,
                EquipmentSpec {
                    periodic_relocation: None,
                    potency_modifiers: Vec::new(),
                    weapon_profile: None,
                    armor: None,
                    slot: EquipmentSlot::new("head_armor").unwrap(),
                    attack: None,
                    modifiers: vec![
                        defense(5),
                        Modifier {
                            cancellable: false,
                            scope: Scope::Always,
                            op: ModifierOp::Add { stat, value },
                        },
                    ],
                },
            );
            assert!(!game.gear_upgrade(actor, ItemId(id), EquipmentSlotId(3)));
        }
    }
    #[test]
    fn weapon_upgrades_use_owned_attributes_and_preserve_descriptor_damage_and_timing() {
        use crate::attributes::Skill;
        let (mut game, actor, at) = fixture();
        for (id, gear) in [
            (10, weapon(Skill::HeavyWeaponry, 0, 60, "fire", 1)),
            (11, weapon(Skill::LightWeaponry, 1, 60, "fire", 2)),
            (12, weapon(Skill::HeavyWeaponry, 0, 60, "fire", 2)),
            (13, weapon(Skill::HeavyWeaponry, 0, 60, "cold", 3)),
            (14, weapon(Skill::HeavyWeaponry, 1, 100, "fire", 2)),
        ] {
            add(&mut game, actor, at, id, gear);
        }
        game.equip_authored(actor, ItemId(10), EquipmentSlotId(0))
            .unwrap();
        let before = game.clone();
        assert!(!game.gear_upgrade(actor, ItemId(11), EquipmentSlotId(0)));
        assert!(game.gear_upgrade(actor, ItemId(12), EquipmentSlotId(0)));
        assert!(!game.gear_upgrade(actor, ItemId(13), EquipmentSlotId(0)));
        assert!(!game.gear_upgrade(actor, ItemId(14), EquipmentSlotId(0)));
        assert_eq!(game, before);
    }
}
