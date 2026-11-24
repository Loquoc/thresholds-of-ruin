//! Owner-scoped structural choices. This deliberately never constructs attacks
//! or reads numeric condition lanes; final exact-action previews own those terms.
use super::{EquippedProjectileWeapon, ProjectileDeliverySelection, ThrownSourceSelection};
use crate::content_ids::AbilityId;
use crate::projectile_profiles::ProjectileDeliveryKind;
use crate::weapon_profiles::EffectiveWeaponProfile;
use crate::{ActionRequirement, ActorId, EquipmentSlotId, Game, GameError, ItemId, ItemLocation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectileChoiceQuery {
    ThrowSources,
    Launchers,
    Ammunition { launcher: EquippedProjectileWeapon },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectileChoice {
    Throw { source: ThrownSourceSelection },
    Launcher { launcher: EquippedProjectileWeapon },
    Ammunition { item: ItemId },
}
impl Game {
    pub(crate) fn projectile_item_profile(
        &self,
        actor: ActorId,
        item: ItemId,
    ) -> Option<EffectiveWeaponProfile> {
        let owner = self.creature(actor)?;
        let item = self.items.get(&item)?;
        if item.location != ItemLocation::Carried(actor) {
            return None;
        }
        let gear = item.spec.equipment.as_ref()?;
        gear.attack.as_ref()?;
        let spec = gear.weapon_profile.as_ref()?;
        Some(
            spec.validate(self.content.rules())
                .ok()?
                .state(owner.derived().expertises.active(&spec.expertise))
                .clone(),
        )
    }
    fn projectile_equipped_profile(
        &self,
        actor: ActorId,
        weapon: EquippedProjectileWeapon,
    ) -> Option<EffectiveWeaponProfile> {
        let owner = self.actors.get(&actor)?;
        if !weapon.valid() || owner.equipment.get(&weapon.socket) != Some(&weapon.item) {
            return None;
        }
        let slot = self
            .content
            .rules()
            .equipment_slot(owner.anatomy.slots.get(usize::from(weapon.socket.0))?)?;
        if !slot.requires_attack
            || slot.capacity.resource().is_some_and(|resource| {
                self.creature(actor).is_none_or(|creature| {
                    creature
                        .derived()
                        .equipment_capacity_deficits
                        .contains_key(resource)
                })
            })
        {
            return None;
        }
        self.projectile_item_profile(actor, weapon.item)
    }
    fn carried_projectile_unit_available(&self, actor: ActorId, item: ItemId) -> bool {
        self.items.get(&item).is_some_and(|item| {
            item.location == ItemLocation::Carried(actor)
                && item
                    .free_quantity(None)
                    .is_some_and(|quantity| quantity > 0)
        })
    }

    fn available_ammunition(
        &self,
        actor: ActorId,
        ammunition: ItemId,
    ) -> Option<&crate::projectile_profiles::ProjectileSpec> {
        let item = self.items.get(&ammunition)?;
        let payload = item.spec.projectile.as_ref()?;
        let owner = self.actors.get(&actor)?;
        (item.location == ItemLocation::Carried(actor)
            && item
                .free_quantity(None)
                .is_some_and(|quantity| quantity > 0)
            && !owner.equipment.values().any(|item| *item == ammunition))
        .then_some(payload)
    }

    fn equipped_launcher(
        &self,
        actor: ActorId,
        launcher: EquippedProjectileWeapon,
    ) -> Option<crate::weapon_profiles::LauncherProfile> {
        self.projectile_equipped_profile(actor, launcher)?
            .launcher()
            .cloned()
    }

    fn launch_requirements(
        &self,
        actor: ActorId,
        launcher: EquippedProjectileWeapon,
        ammunition: ItemId,
    ) -> Result<(), ActionRequirement> {
        let profile = self
            .equipped_launcher(actor, launcher)
            .ok_or(ActionRequirement::WieldLauncher)?;
        self.available_ammunition(actor, ammunition)
            .filter(|payload| payload.compatible_with(&profile))
            .map(|_| ())
            .ok_or(ActionRequirement::CompatibleAmmunition)
    }

    fn knows_carried_projectile_type(&self, actor: ActorId, item: ItemId) -> bool {
        self.items.get(&item).is_some_and(|item| {
            item.location == ItemLocation::Carried(actor)
                && (!item.spec.concealed
                    || self
                        .actors
                        .get(&actor)
                        .is_some_and(|owner| owner.knowledge.contains(&item.spec.identity)))
        })
    }

    /// This is a positive hint about a known item, not a census of inventory.
    /// Unidentified types and false or stale item references imply no hint.
    pub fn known_unwielded_launcher(&self, actor: ActorId, item: ItemId) -> bool {
        self.knows_carried_projectile_type(actor, item)
            && self
                .actors
                .get(&actor)
                .is_some_and(|owner| !owner.equipment.values().any(|equipped| *equipped == item))
            && self
                .projectile_item_profile(actor, item)
                .is_some_and(|profile| profile.launcher().is_some())
    }

    /// Explain only requirements the owner could infer from known carried
    /// types. The structural result does not consult curses, numeric item
    /// details, a target or a collision.
    pub(super) fn check_projectile_source(
        &self,
        actor: ActorId,
        selection: ProjectileDeliverySelection,
    ) -> Result<(), GameError> {
        if let ProjectileDeliverySelection::Launch {
            launcher,
            ammunition,
        } = selection
        {
            return self
                .launch_requirements(actor, launcher, ammunition)
                .map_err(|reason| {
                    let disclosed = match reason {
                        ActionRequirement::WieldLauncher => {
                            self.known_unwielded_launcher(actor, launcher.item)
                        }
                        ActionRequirement::CompatibleAmmunition => {
                            self.knows_carried_projectile_type(actor, launcher.item)
                                && self.knows_carried_projectile_type(actor, ammunition)
                        }
                    };
                    if disclosed {
                        GameError::ActionRequirement(reason)
                    } else {
                        GameError::ItemUnavailable
                    }
                });
        }
        self.projectile_source_eligible(actor, selection)
            .then_some(())
            .ok_or(GameError::ItemUnavailable)
    }

    /// Shared with fresh capture. Curse, enchantments, funding, targets and future
    /// collisions cannot change this capability decision.
    pub(super) fn projectile_source_eligible(
        &self,
        actor: ActorId,
        selection: ProjectileDeliverySelection,
    ) -> bool {
        if !selection.valid() {
            return false;
        }
        match selection {
            ProjectileDeliverySelection::Throw {
                source: ThrownSourceSelection::Equipped { item, socket },
            } => {
                self.carried_projectile_unit_available(actor, item)
                    && self
                        .projectile_equipped_profile(
                            actor,
                            EquippedProjectileWeapon { item, socket },
                        )
                        .is_some_and(|profile| profile.thrown().is_some())
            }
            ProjectileDeliverySelection::Throw {
                source: ThrownSourceSelection::Carried { item, socket },
            } => {
                self.carried_projectile_unit_available(actor, item)
                    && self
                        .projectile_item_profile(actor, item)
                        .is_some_and(|profile| profile.thrown().is_some())
                    && self
                        .capture_carried_weapon_allocation(actor, item, socket)
                        .is_ok()
            }
            ProjectileDeliverySelection::Launch {
                launcher,
                ammunition,
            } => {
                // Menu scans inspect many unrelated carried rows. Eliminate
                // those cheaply before materializing the launcher profile.
                self.available_ammunition(actor, ammunition)
                    .is_some_and(|payload| {
                        self.equipped_launcher(actor, launcher)
                            .is_some_and(|profile| payload.compatible_with(&profile))
                    })
            }
        }
    }
    /// Evaluate one disclosed inventory row. Callers bound scans and pagination;
    /// this method returns at most one deterministic source socket per item.
    pub fn projectile_choice_for_item(
        &self,
        actor: ActorId,
        ability: &AbilityId,
        query: ProjectileChoiceQuery,
        item: ItemId,
    ) -> Option<ProjectileChoice> {
        let creature = self.creature(actor)?;
        if creature.health().dead() || !creature.derived().abilities.contains(ability) {
            return None;
        }
        let definition = self.content.rules().ability(ability)?;
        if !super::projectile::definition_shape_valid(definition) {
            return None;
        }
        let crate::rules::StrikeSource::Projectile { delivery } = definition.strikes else {
            return None;
        };
        if delivery
            != match query {
                ProjectileChoiceQuery::ThrowSources => ProjectileDeliveryKind::Throw,
                _ => ProjectileDeliveryKind::Launch,
            }
        {
            return None;
        }
        let owner = self.actors.get(&actor)?;
        match query {
            ProjectileChoiceQuery::ThrowSources => {
                if let Some((socket, _)) = owner
                    .equipment
                    .iter()
                    .find(|(_, equipped)| **equipped == item)
                {
                    let source = ThrownSourceSelection::Equipped {
                        item,
                        socket: *socket,
                    };
                    return self
                        .projectile_source_eligible(
                            actor,
                            ProjectileDeliverySelection::Throw { source },
                        )
                        .then_some(ProjectileChoice::Throw { source });
                }
                let primary = owner.anatomy.primary_attack_socket.socket();
                primary
                    .into_iter()
                    .chain(
                        (0..owner.anatomy.slots.len())
                            .map(|index| EquipmentSlotId(index as u16))
                            .filter(|socket| Some(*socket) != primary),
                    )
                    .find_map(|socket| {
                        let source = ThrownSourceSelection::Carried { item, socket };
                        self.projectile_source_eligible(
                            actor,
                            ProjectileDeliverySelection::Throw { source },
                        )
                        .then_some(ProjectileChoice::Throw { source })
                    })
            }
            ProjectileChoiceQuery::Launchers => {
                let (socket, _) = owner
                    .equipment
                    .iter()
                    .find(|(_, equipped)| **equipped == item)?;
                let launcher = EquippedProjectileWeapon {
                    item,
                    socket: *socket,
                };
                self.projectile_equipped_profile(actor, launcher)?
                    .launcher()?;
                Some(ProjectileChoice::Launcher { launcher })
            }
            ProjectileChoiceQuery::Ammunition { launcher } => self
                .projectile_source_eligible(
                    actor,
                    ProjectileDeliverySelection::Launch {
                        launcher,
                        ammunition: item,
                    },
                )
                .then_some(ProjectileChoice::Ammunition { item }),
        }
    }
}
