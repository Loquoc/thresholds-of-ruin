//! Bounded acquisition of known launcher/payload pairs. This policy never
//! compiles a numerical projectile or inspects an opponent's private state.
use crate::abilities::{ProjectileChoice, ProjectileChoiceQuery};
use crate::content_ids::{AbilityId, LauncherFamilyId};
use crate::{Action, ActorId, EquipmentSlotId, Game, ItemId, Observation};
use std::collections::BTreeMap;

const INVENTORY_BOUND: usize = 128;
const LAUNCHER_BOUND: usize = 16;
const SHOT_RESERVE: u64 = 6;

struct LauncherChoice {
    item: ItemId,
    socket: EquipmentSlotId,
    previous: Option<ItemId>,
    family: LauncherFamilyId,
}

impl Game {
    /// The existing explicit ability ordering supplies the preference. Expertise
    /// changes legal capabilities and prices, never an actor's tactical role.
    fn acquisition_launch_ability(&self, actor: ActorId) -> Option<(AbilityId, bool)> {
        let creature = self.creature(actor)?;
        let rules = self.content.rules();
        let basic_priority = rules.ability(&self.basic_attack(actor)?)?.ai.priority;
        creature
            .derived()
            .abilities
            .iter()
            .filter_map(|id| {
                let def = rules.ability(id)?;
                (def.ai.hostile
                    && matches!(
                        def.strikes,
                        crate::rules::StrikeSource::Projectile {
                            delivery: crate::projectile_profiles::ProjectileDeliveryKind::Launch
                        }
                    ))
                .then_some((def.ai.priority, id))
            })
            .max_by(|(ap, ai), (bp, bi)| ap.cmp(bp).then_with(|| bi.cmp(ai)))
            .map(|(priority, id)| (id.clone(), priority > basic_priority))
    }

    pub(crate) fn known_launcher_family(
        &self,
        actor: ActorId,
        item: ItemId,
    ) -> Option<LauncherFamilyId> {
        let owner = self.actors.get(&actor)?;
        let spec = &self.items.get(&item)?.spec;
        if spec.concealed && !owner.knowledge.contains(&spec.identity) {
            return None;
        }
        let profile = spec.equipment.as_ref()?.weapon_profile.as_ref()?;
        profile
            .validate(self.content.rules())
            .ok()?
            .state(
                self.creature(actor)?
                    .derived()
                    .expertises
                    .active(&profile.expertise),
            )
            .launcher()
            .map(|launcher| launcher.family.clone())
    }

    pub(crate) fn known_payload_family(
        &self,
        actor: ActorId,
        item: ItemId,
    ) -> Option<&LauncherFamilyId> {
        let owner = self.actors.get(&actor)?;
        if owner.equipment.values().any(|equipped| *equipped == item) {
            return None;
        }
        let spec = &self.items.get(&item)?.spec;
        if spec.concealed && !owner.knowledge.contains(&spec.identity) {
            return None;
        }
        Some(&spec.projectile.as_ref()?.family)
    }

    /// Collection can wait for ordinary recovery, but must not provision a shot
    /// whose complete known resource bundle exceeds this loadout's maxima.
    pub(crate) fn ranged_loadout_has_budget(
        &self,
        actor: ActorId,
        item: ItemId,
        socket: EquipmentSlotId,
    ) -> bool {
        let Some((ability, _)) = self.acquisition_launch_ability(actor) else {
            return false;
        };
        let Some(creature) = self.creature(actor) else {
            return false;
        };
        let Some(spec) = self
            .items
            .get(&item)
            .and_then(|item| item.spec.equipment.as_ref())
            .and_then(|gear| gear.weapon_profile.as_ref())
        else {
            return false;
        };
        let profile = crate::abilities::CapturedWeaponProfile::Profile {
            spec: spec.clone(),
            expert: creature.derived().expertises.active(&spec.expertise),
        };
        let Some(definition) = self.content.rules().ability(&ability) else {
            return false;
        };
        let Ok(base) = definition
            .cost
            .as_ref()
            .map(crate::costs::CostBundle::try_from)
            .transpose()
        else {
            return false;
        };
        let secondary = creature.derived().anatomy.primary_attack_socket.socket() != Some(socket);
        let Ok(cost) =
            crate::abilities::selected_weapon_cost(self.content.rules(), &profile, secondary, base)
        else {
            return false;
        };
        cost.is_none_or(|bundle| {
            bundle.entries().all(|entry| {
                entry
                    .start
                    .checked_add(entry.resolution)
                    .is_some_and(|total| {
                        total <= creature.costs().resources().maximum(entry.resource)
                    })
            })
        })
    }

    /// Bounded pairing uses only supplied disclosed candidates. Inventory comes
    /// before ground loot and equipped launchers come first, preventing spare
    /// collection and replacement oscillation. Large inventories fail closed:
    /// truncating the reserve count could otherwise keep collecting forever.
    fn known_launcher_choice(
        &self,
        actor: ActorId,
        view: &Observation,
        ground: &[ItemId],
    ) -> Option<LauncherChoice> {
        if view.inventory.len() > INVENTORY_BOUND {
            return None;
        }
        let (ability, prefer_ranged) = self.acquisition_launch_ability(actor)?;
        let owner = self.actors.get(&actor)?;
        let mut ranked: Vec<_> = view
            .inventory
            .iter()
            .filter(|row| row.identified)
            .map(|row| {
                (
                    !owner.equipment.values().any(|id| *id == row.id),
                    false,
                    row.id,
                )
            })
            .chain(ground.iter().map(|item| (true, true, *item)))
            .collect();
        ranked.sort_unstable();
        let items: Vec<_> = ranked.into_iter().map(|(_, _, item)| item).collect();
        let mut launchers = 0;
        let mut workspace = None;
        for item in items.iter().copied() {
            let Some(family) = self.known_launcher_family(actor, item) else {
                continue;
            };
            launchers += 1;
            if launchers > LAUNCHER_BOUND {
                break;
            }
            if !items.iter().any(|payload| {
                *payload != item && self.known_payload_family(actor, *payload) == Some(&family)
            }) {
                continue;
            }
            if let Some(ProjectileChoice::Launcher { launcher }) = self.projectile_choice_for_item(
                actor,
                &ability,
                ProjectileChoiceQuery::Launchers,
                item,
            ) {
                if !self.ranged_loadout_has_budget(actor, item, launcher.socket) {
                    continue;
                }
                return Some(LauncherChoice {
                    item,
                    socket: launcher.socket,
                    previous: Some(item),
                    family,
                });
            }
            // Do not try to relocate an already-equipped but presently unusable
            // launcher into another socket to evade a capacity deficit.
            if owner.equipment.values().any(|id| *id == item) {
                continue;
            }
            if let Some((socket, previous)) =
                self.ranged_gear_destination(&mut workspace, actor, item, prefer_ranged)
            {
                return Some(LauncherChoice {
                    item,
                    socket,
                    previous,
                    family,
                });
            }
        }
        None
    }

    pub(crate) fn choose_ranged_gear(&self, actor: ActorId, view: &Observation) -> Option<Action> {
        let choice = self.known_launcher_choice(actor, view, &[])?;
        if choice.previous == Some(choice.item) {
            return None;
        }
        Some(choice.previous.map_or(
            Action::Equip {
                item: choice.item,
                slot: choice.socket,
            },
            |item| Action::Unequip { item },
        ))
    }

    /// Ordinary gear improvements may not immediately remove the usable launcher
    /// selected by a strictly higher authored ability priority. Depletion removes
    /// this protection, so normal safe melee recovery needs no saved AI plan.
    pub(crate) fn retained_ranged_weapon(
        &self,
        actor: ActorId,
        view: &Observation,
    ) -> Option<ItemId> {
        if !self.acquisition_launch_ability(actor)?.1 {
            return None;
        }
        if view.inventory.len() > INVENTORY_BOUND {
            return None;
        }
        let (ability, _) = self.acquisition_launch_ability(actor)?;
        let owner = self.actors.get(&actor)?;
        owner.equipment.values().find_map(|item| {
            let family = self.known_launcher_family(actor, *item)?;
            let Some(ProjectileChoice::Launcher { launcher }) = self.projectile_choice_for_item(
                actor,
                &ability,
                ProjectileChoiceQuery::Launchers,
                *item,
            ) else {
                return None;
            };
            (self.ranged_loadout_has_budget(actor, *item, launcher.socket)
                && view
                    .inventory
                    .iter()
                    .filter(|row| row.identified)
                    .any(|row| self.known_payload_family(actor, row.id) == Some(&family)))
            .then_some(*item)
        })
    }

    pub(crate) fn ranged_loot_quantities(
        &self,
        actor: ActorId,
        view: &Observation,
        profile: &crate::ai::AiProfile,
        route: &mut impl FnMut(tor_world::Location) -> Option<Vec<crate::TravelStep>>,
    ) -> BTreeMap<ItemId, u64> {
        let result = BTreeMap::new();
        if !profile.equipment
            || !profile.loot
            || view.inventory.len() > INVENTORY_BOUND
            || self.acquisition_launch_ability(actor).is_none()
        {
            return result;
        }
        let ground: Vec<_> = view
            .ground_items
            .iter()
            .take(INVENTORY_BOUND)
            .filter(|row| row.identified)
            .filter(|row| {
                self.known_launcher_family(actor, row.id).is_some()
                    || self.known_payload_family(actor, row.id).is_some()
            })
            .filter(|row| !self.exposed_to_visible_hostile(actor, row.location, view))
            .filter(|row| {
                if row.location == view.location {
                    return true;
                }
                if profile.idle == crate::behavior::IdleBehavior::Wait {
                    return false;
                }
                route(row.location).is_some_and(|path| {
                    path.first().is_some_and(|step| {
                        self.actor_translation(actor, step.direction)
                            .is_some_and(|(at, _)| {
                                !self.exposed_to_visible_hostile(actor, at, view)
                            })
                    })
                })
            })
            .map(|row| row.id)
            .collect();
        self.ranged_pair_quantities(actor, view, &ground)
    }

    // One pairing/reserve authority for ordinary and recovered approaches.
    fn ranged_pair_quantities(
        &self,
        actor: ActorId,
        view: &Observation,
        ground: &[ItemId],
    ) -> BTreeMap<ItemId, u64> {
        let mut result = BTreeMap::new();
        let Some(choice) = self.known_launcher_choice(actor, view, ground) else {
            return result;
        };
        if ground.contains(&choice.item) {
            result.insert(choice.item, 1);
        }
        let carried = view
            .inventory
            .iter()
            .filter(|row| row.identified)
            .filter(|row| {
                row.id != choice.item
                    && self.known_payload_family(actor, row.id) == Some(&choice.family)
            })
            .fold(0_u64, |sum, row| sum.saturating_add(row.quantity));
        let deficit = SHOT_RESERVE.saturating_sub(carried);
        if deficit > 0 {
            for &item in ground {
                if self.known_payload_family(actor, item) == Some(&choice.family) {
                    result.insert(item, deficit);
                }
            }
        }
        result
    }
    /// Reconsider only known ranged pairs after the ordinary item chooser found
    /// no action. Eligibility and execution share the same completed approach
    /// facts, so an underfoot half does not disappear when its partner requires
    /// a disclosed detour. Nothing here predicts undisclosed body occupancy.
    pub(crate) fn choose_ranged_loot_detour(
        &self,
        actor: ActorId,
        view: &Observation,
        profile: &crate::ai::AiProfile,
        route: &mut impl FnMut(tor_world::Location) -> Option<Vec<crate::TravelStep>>,
        remaining: &mut usize,
    ) -> Option<Action> {
        if view.ground_items.is_empty()
            || !profile.equipment
            || !profile.loot
            || profile.idle == crate::behavior::IdleBehavior::Wait
            || view.inventory.len() > INVENTORY_BOUND
            || self.acquisition_launch_ability(actor).is_none()
        {
            return None;
        }
        let ground: Vec<_> = view
            .ground_items
            .iter()
            .take(INVENTORY_BOUND)
            .filter(|row| row.identified)
            .filter(|row| {
                self.known_launcher_family(actor, row.id).is_some()
                    || self.known_payload_family(actor, row.id).is_some()
            })
            .filter(|row| !self.exposed_to_visible_hostile(actor, row.location, view))
            .collect();
        let occupied: std::collections::BTreeSet<_> = view
            .visible_actors
            .iter()
            .filter(|row| {
                row.id != actor && row.disclosure == crate::perception::ActorDisclosure::Creature
            })
            .map(|row| row.location)
            .collect();
        let destinations: std::collections::BTreeSet<_> =
            ground.iter().map(|row| row.location).collect();
        // (hop count, first direction). None direction means underfoot Take.
        let mut approaches = BTreeMap::new();
        let mut blocked = Vec::new();
        for destination in destinations {
            if destination == view.location {
                approaches.insert(destination, (0, None));
                continue;
            }
            let Some(path) = route(destination) else {
                continue;
            };
            let Some(step) = path.first() else {
                continue;
            };
            match self.actor_translation(actor, step.direction) {
                Some((at, _)) if !self.exposed_to_visible_hostile(actor, at, view) => {
                    approaches.insert(destination, (path.len(), Some(step.direction)));
                }
                None if occupied.contains(&step.destination) => blocked.push(destination),
                _ => {}
            }
        }
        if blocked.is_empty() {
            return None;
        }
        let owner = self.actors.get(&actor)?;
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
            if self.condition_blocks_action(actor, &Action::Move(direction)) {
                continue;
            }
            let Some((at, frame)) = self.actor_translation(actor, direction) else {
                continue;
            };
            if at == view.location
                || !disclosed.contains(&(at, frame))
                || occupied.contains(&at)
                || self.exposed_to_visible_hostile(actor, at, view)
            {
                continue;
            }
            let Ok(cost) = owner.movement_duration(direction) else {
                continue;
            };
            alternatives.push((rank, direction, at, frame, u128::from(cost)));
        }
        for target in blocked {
            let mut best = None;
            for &(rank, direction, at, frame, first_cost) in &alternatives {
                let Ok(mut search) = self.route_search_from(actor, at, frame) else {
                    continue;
                };
                let suffix = match search.route_bounded(target, remaining) {
                    Ok(Some(path)) => path,
                    // Never let an early partial comparison license a pair.
                    Ok(None) => return None,
                    Err(_) => continue,
                };
                if suffix.iter().any(|step| {
                    step.destination == view.location || occupied.contains(&step.destination)
                }) {
                    continue;
                }
                let Some(cost) = suffix.iter().try_fold(first_cost, |total, step| {
                    total.checked_add(u128::from(owner.movement_duration(step.direction).ok()?))
                }) else {
                    continue;
                };
                if best
                    .as_ref()
                    .is_none_or(|old: &(u128, usize, usize, tor_world::Direction)| {
                        (cost, rank) < (old.0, old.1)
                    })
                {
                    best = Some((cost, rank, suffix.len() + 1, direction));
                }
            }
            if let Some((_, _, hops, direction)) = best {
                approaches.insert(target, (hops, Some(direction)));
            }
        }
        let eligible: Vec<_> = ground
            .iter()
            .filter(|row| approaches.contains_key(&row.location))
            .map(|row| row.id)
            .collect();
        let quantities = self.ranged_pair_quantities(actor, view, &eligible);
        ground
            .into_iter()
            .filter_map(|row| {
                let quantity = quantities.get(&row.id)?.min(&row.quantity);
                let &(hops, direction) = approaches.get(&row.location)?;
                let action = direction.map_or(
                    Action::Take {
                        item: row.id,
                        quantity: Some(*quantity),
                    },
                    Action::Move,
                );
                (!self.condition_blocks_action(actor, &action)).then_some((hops, row.id, action))
            })
            .min_by_key(|(hops, id, _)| (*hops, *id))
            .map(|(_, _, action)| action)
    }
}
