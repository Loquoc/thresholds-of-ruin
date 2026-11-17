//! Actor-known equipment estimates and pure hypothetical loadout evaluation.
//! Acquisition policy chooses items and acceptance rules in the parent module.
use crate::{ActorId, EquipmentSlotId, EquipmentSpec, Game, ItemId};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum CandidateProjectionFailure {
    Rejected,
    ItemUnavailable,
}

impl Game {
    pub(super) fn known_gear(&self, actor: ActorId, item: ItemId) -> Option<EquipmentSpec> {
        let mut estimate = self.items.get(&item)?.clone();
        estimate.condition = self.gear_condition_estimate(actor, item)?;
        estimate.equipment_view()
    }

    /// Private planning estimate, never a disclosed numerical item view. An
    /// identified archetype supplies its ordinary baseline until this actor has
    /// observed exact instance numbers; hidden mutations never refresh memory.
    pub(super) fn gear_condition_estimate(
        &self,
        actor: ActorId,
        item: ItemId,
    ) -> Option<crate::armor::ItemCondition> {
        let owner = self.actors.get(&actor)?;
        let definition = &self.items.get(&item)?.spec;
        if definition.concealed && !owner.knowledge.contains(&definition.identity) {
            return None;
        }
        definition.equipment.as_ref()?;
        Some(
            owner
                .known_equipment
                .remembered_condition(item)
                .unwrap_or_default(),
        )
    }

    /// Candidate evaluation uses the production modifier fold. It changes only
    /// a private snapshot; the chosen ordinary action still owns admission.
    pub(super) fn gear_projection(
        &self,
        actor: ActorId,
        item: ItemId,
        slot: EquipmentSlotId,
    ) -> Option<Game> {
        crate::diagnostics::gear_projection();
        // Keep the same one-clone-per-candidate accounting. Admission and both
        // folds below run afresh; no candidate legality is cached.
        self.actors.get(&actor)?;
        let mut candidate = self.clone();
        candidate
            .project_gear_candidate_in_place(self, actor, item, slot)
            .ok()?;
        Some(candidate)
    }

    /// Baseline and candidates use the same remembered facts or ordinary
    /// archetype estimates. Reading live hidden condition would make AI an oracle.
    pub(super) fn known_loadout_projection(
        &self,
        actor: ActorId,
        equipment: std::collections::BTreeMap<EquipmentSlotId, ItemId>,
    ) -> Option<Game> {
        let mut candidate = self.clone();
        candidate.project_known_loadout_in_place(self, actor, equipment)?;
        Some(candidate)
    }

    /// Reuse one private workspace across a bounded candidate scan. Restore the
    /// original actor before each fold so resource/Health clamping from a rejected
    /// candidate cannot affect the next comparison. Item values always come from
    /// the original actor's remembered facts, never earlier hypothetical state.
    pub(super) fn project_known_loadout_in_place(
        &mut self,
        source: &Game,
        actor: ActorId,
        equipment: std::collections::BTreeMap<EquipmentSlotId, ItemId>,
    ) -> Option<()> {
        *self.actors.get_mut(&actor)? = source.actors.get(&actor)?.clone();
        for id in equipment.values() {
            let condition = source.gear_condition_estimate(actor, *id)?;
            if !source.item_equip_unit_available(*id) {
                return None;
            }
            self.items.edit(*id, |item| {
                // A private loadout estimates the same single physical unit
                // that ordinary Equip reserves and extracts at completion.
                item.quantity = 1;
                item.condition = condition;
                item.location = crate::ItemLocation::Carried(actor);
            })?;
        }
        let sources = equipment
            .iter()
            .map(|(socket, id)| {
                self.items
                    .get(id)?
                    .equipment_source(*id, *socket, &self.content.rules)
            })
            .collect::<Option<Vec<_>>>()?;
        let context = self.equipment_recovery_context(actor, &sources).ok()?;
        let mut owner = self.actors.get_mut(&actor)?;
        let result = owner
            .combat
            .as_mut()?
            .project_equipment_immediately_in_context(sources, context)
            .ok()?;
        if result.died {
            return None;
        }
        owner.equipment = equipment;
        Some(())
    }

    /// Canonical empty-before-occupied socket order. Index conversion remains
    /// with each policy, preserving its established failure behavior.
    pub(super) fn known_gear_socket_indices(
        &self,
        actor: ActorId,
        kind: &crate::EquipmentSlot,
    ) -> Option<Vec<usize>> {
        let owner = self.actors.get(&actor)?;
        // Filtering by kind commutes with this key and avoids a second
        // allocation while retaining the old matching-socket order.
        let mut sockets: Vec<_> = owner
            .anatomy
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| (candidate == kind).then_some(index))
            .collect();
        sockets.sort_by_key(|index| {
            (
                owner
                    .equipment
                    .contains_key(&EquipmentSlotId(*index as u16)),
                *index,
            )
        });
        Some(sockets)
    }

    /// None rejects a replacement; Some(None) is an available empty socket.
    pub(super) fn known_gear_replacement(
        &self,
        actor: ActorId,
        slot: EquipmentSlotId,
    ) -> Option<Option<ItemId>> {
        let owner = self.actors.get(&actor)?;
        let old = owner.equipment.get(&slot).copied();
        if old.is_some_and(|item| {
            self.known_gear(actor, item).is_none()
                || owner.known_equipment.curse(item) == Some(true)
        }) {
            return None;
        }
        Some(old)
    }

    /// Reset, expose the proposed carried unit, admit Equip, then fold the new
    /// loadout. Item lookup failure remains distinct from candidate rejection:
    /// ranged acquisition historically abandons its comparison on that failure.
    pub(super) fn project_gear_candidate_in_place(
        &mut self,
        source: &Game,
        actor: ActorId,
        item: ItemId,
        slot: EquipmentSlotId,
    ) -> Result<(), CandidateProjectionFailure> {
        use CandidateProjectionFailure::{ItemUnavailable, Rejected};
        let mut equipment = source.actors.get(&actor).ok_or(Rejected)?.equipment.clone();
        equipment.remove(&slot);
        self.project_known_loadout_in_place(source, actor, equipment.clone())
            .ok_or(Rejected)?;
        self.items
            .edit(item, |item| {
                item.location = crate::ItemLocation::Carried(actor)
            })
            .ok_or(ItemUnavailable)?;
        self.capture_equip_allocation(actor, item, slot)
            .map_err(|_| Rejected)?;
        equipment.insert(slot, item);
        self.project_known_loadout_in_place(source, actor, equipment)
            .ok_or(Rejected)
    }
}

/// One immutable actor's ordinary equipment comparison. Cache only the known
/// baseline score, including failure, never a candidate's legality or score.
/// The borrow binds this scratch state to its owner and prevents reuse after a
/// world mutation. Ranged policy has different acceptance rules and stays separate.
pub(super) struct KnownGearEvaluation<'a> {
    game: &'a Game,
    actor: ActorId,
    pub(super) baseline: Option<Option<GearScore>>,
}

impl<'a> KnownGearEvaluation<'a> {
    pub(super) fn new(game: &'a Game, actor: ActorId) -> Self {
        Self {
            game,
            actor,
            baseline: None,
        }
    }

    pub(super) fn destination(
        &mut self,
        item: ItemId,
    ) -> Option<(EquipmentSlotId, Option<ItemId>)> {
        let game = self.game;
        let actor = self.actor;
        let candidate = game.known_gear(actor, item)?;
        let owner = game.actors.get(&actor)?;
        if owner.equipment.values().any(|equipped| *equipped == item) {
            return None;
        }
        for index in game.known_gear_socket_indices(actor, &candidate.slot)? {
            let slot = EquipmentSlotId(u16::try_from(index).ok()?);
            let Some(old) = game.known_gear_replacement(actor, slot) else {
                continue;
            };
            if self.upgrade(item, slot) {
                return Some((slot, old));
            }
        }
        None
    }

    pub(super) fn upgrade(&mut self, item: ItemId, slot: EquipmentSlotId) -> bool {
        let game = self.game;
        let actor = self.actor;
        // Keep the candidate first: unavailable candidates do not force a
        // baseline projection. Every slot still runs ordinary equip admission.
        let Some(candidate) = game.gear_projection(actor, item, slot) else {
            return false;
        };
        let before = self.baseline.get_or_insert_with(|| {
            let owner = game.actors.get(&actor)?;
            let projected = game.known_loadout_projection(actor, owner.equipment.clone())?;
            gear_score(&projected, actor)
        });
        let (Some(before), Some(after)) = (before.as_ref(), gear_score(&candidate, actor)) else {
            return false;
        };
        after.dominates(before)
    }
}

/// Score each choice once against the same immutable source. Keep unavailable
/// scores and input order: neither can dominate, and ties/incomparable loadouts
/// still select the first nondominated choice. Only scores, not Games, survive
/// projection; production derivation remains the authority for each loadout.
pub(super) fn first_nondominated_gear<T>(
    choices: &[T],
    score: impl FnMut(&T) -> Option<GearScore>,
) -> Option<&T> {
    let scores: Vec<_> = choices.iter().map(score).collect();
    scores
        .iter()
        .position(|candidate| {
            candidate.as_ref().is_some_and(|candidate| {
                !scores
                    .iter()
                    .flatten()
                    .any(|other| other.dominates(candidate))
            })
        })
        .map(|index| &choices[index])
}

#[derive(PartialEq, Eq)]
pub(super) struct GearScore {
    pub(super) maximum_health: u64,
    // Lower effective upkeep is useful, but higher upkeep alone does not veto
    // a combat upgrade. Exempt creatures and a floored-zero rate gain nothing.
    pub(super) nutrition_rate_thirtieths: i64,
    pub(super) sustained_attributes: std::collections::BTreeSet<crate::attributes::Attribute>,
    pub(super) protects_armor: bool,
    pub(super) see_invisible: bool,
    pub(super) stats: std::collections::BTreeMap<crate::rules::Stat, i64>,
    pub(super) flags: std::collections::BTreeSet<crate::content_ids::FlagId>,
    pub(super) abilities: std::collections::BTreeSet<crate::content_ids::AbilityId>,
    pub(super) protection: std::collections::BTreeMap<crate::rules::Selector, (bool, u32)>,
    pub(super) attack: AttackScore,
}
#[derive(Clone, PartialEq, Eq)]
pub(super) struct AttackScore {
    pub(super) bonus: i64,
    pub(super) prepare: u64,
    pub(super) recovery: u64,
    pub(super) damage: std::collections::BTreeMap<
        (
            crate::combat::DamageType,
            Option<crate::content_ids::DescriptorId>,
        ),
        i64,
    >,
}
impl GearScore {
    pub(super) fn dominates(&self, old: &Self) -> bool {
        let improved = self
            .stats
            .iter()
            .any(|(stat, value)| *value > old.stats.get(stat).copied().unwrap_or(0))
            || self.maximum_health > old.maximum_health
            || self.nutrition_rate_thirtieths < old.nutrition_rate_thirtieths
            || self.sustained_attributes.len() > old.sustained_attributes.len()
            || (self.protects_armor && !old.protects_armor)
            || (self.see_invisible && !old.see_invisible)
            || self.flags.len() > old.flags.len()
            || self.abilities.len() > old.abilities.len()
            || self
                .protection
                .iter()
                .any(|(selector, (immune, reduction))| {
                    let (old_immune, old_reduction) =
                        old.protection.get(selector).copied().unwrap_or_default();
                    (*immune && !old_immune) || (!old_immune && *reduction > old_reduction)
                })
            || self.attack.bonus > old.attack.bonus
            || self.attack.prepare < old.attack.prepare
            || self.attack.recovery < old.attack.recovery
            || self
                .attack
                .damage
                .iter()
                .any(|(key, value)| *value > old.attack.damage.get(key).copied().unwrap_or(0));
        improved
            && self.maximum_health >= old.maximum_health
            && self
                .sustained_attributes
                .is_superset(&old.sustained_attributes)
            && (!old.protects_armor || self.protects_armor)
            && (!old.see_invisible || self.see_invisible)
            && old
                .stats
                .iter()
                .all(|(stat, value)| self.stats.get(stat).copied().unwrap_or(0) >= *value)
            && self
                .stats
                .iter()
                .all(|(stat, value)| *value >= old.stats.get(stat).copied().unwrap_or(0))
            && self.flags.is_superset(&old.flags)
            && self.abilities.is_superset(&old.abilities)
            && old
                .protection
                .iter()
                .all(|(selector, (immune, reduction))| {
                    let (new_immune, new_reduction) =
                        self.protection.get(selector).copied().unwrap_or_default();
                    (!immune || new_immune) && (new_immune || new_reduction >= *reduction)
                })
            && self.attack.bonus >= old.attack.bonus
            && self.attack.prepare <= old.attack.prepare
            && self.attack.recovery <= old.attack.recovery
            && old
                .attack
                .damage
                .iter()
                .all(|(key, amount)| self.attack.damage.get(key).copied().unwrap_or(0) >= *amount)
    }
}

/// Read cache values only. Every candidate above was derived by the same source
/// fold as actual equipment; damage estimates never inspect enemy protection.
pub(super) fn gear_score(game: &Game, actor: ActorId) -> Option<GearScore> {
    use crate::rules::{AbilityStat, DefenseKind, ModifierOp, Stat};
    let creature = game.creature(actor)?;
    let derived = creature.derived();
    let mut stats = std::collections::BTreeMap::new();
    for attribute in crate::attributes::Attribute::ALL {
        stats.insert(
            Stat::Attribute { attribute },
            i64::from(derived.attributes.get(attribute)),
        );
    }
    for (defense, value) in [
        (DefenseKind::Physical, derived.defenses.physical),
        (DefenseKind::Cognitive, derived.defenses.cognitive),
        (DefenseKind::Spiritual, derived.defenses.spiritual),
    ] {
        stats.insert(Stat::Defense { defense }, i64::from(value));
    }
    // Lower derived periods/reset percentages are improvements. Comparing final
    // values also accounts for HD scaling and all ordinary source interactions.
    stats.insert(
        Stat::HealthRecoveryPeriod,
        -(derived.health_recovery_period as i64),
    );
    stats.insert(
        Stat::HealthRecoveryResetPercent,
        -i64::from(derived.health_recovery_reset_percent),
    );
    // Food cost is an upkeep tradeoff, not a Pareto combat regression.
    stats.insert(
        Stat::AttackDamageRollBonus,
        i64::from(derived.attack_damage_roll_bonus),
    );
    for resource in crate::resources::Resource::ALL {
        stats.insert(
            Stat::ResourceMax { resource },
            i64::from(creature.costs().resources().maximum(resource)),
        );
    }
    for (ability, values) in &derived.ability_values {
        for (stat, value) in [
            (AbilityStat::Dice, i64::from(values.dice)),
            (AbilityStat::FlatBonus, i64::from(values.flat)),
            (AbilityStat::CheckBonus, i64::from(values.check_bonus)),
            (AbilityStat::Difficulty, i64::from(values.difficulty)),
            (AbilityStat::Duration, values.duration as i64),
            (AbilityStat::Range, i64::from(values.range)),
            (AbilityStat::Preparation, -values.preparation),
            (AbilityStat::Recovery, -values.recovery),
        ] {
            stats.insert(
                Stat::Ability {
                    ability: ability.clone(),
                    stat,
                },
                value,
            );
        }
    }
    let mut protection = std::collections::BTreeMap::new();
    for modifier in derived.sources.values().flatten() {
        if let ModifierOp::Immunity { selector }
        | ModifierOp::Reduction { selector, .. }
        | ModifierOp::SignedReduction { selector, .. } = &modifier.op
        {
            protection.insert(
                selector.clone(),
                (
                    derived.protection.has_immunity(selector),
                    derived.protection.reduction(selector),
                ),
            );
        }
    }
    let attack = game.selected_melee_attack(actor)?;
    let basic = game.basic_attack(actor)?;
    let values = derived.ability_values(&basic);
    let damage = match attack.damage_with_modifiers(values.dice, values.flat, 0) {
        Ok(damage) => Some(damage),
        Err(crate::attacks::SelectedDamageError::NoPrimaryDamage) => None,
        Err(crate::attacks::SelectedDamageError::InvalidDefinition) => return None,
    };
    let damage = damage
        .map(|damage| damage.with_roll_bonus(derived.attack_damage_roll_bonus))
        .transpose()
        .ok()?;
    let mut estimates = std::collections::BTreeMap::new();
    for component in damage.iter().flat_map(|damage| damage.components()) {
        let estimate = match component.amount() {
            crate::damage::DamageAmount::Fixed(value) => i64::from(value) * 2,
            crate::damage::DamageAmount::Rolled(pool) => (i64::from(pool.count())
                * (i64::from(pool.sides()) + 1)
                + i64::from(pool.bonus()) * 2)
                .max(0),
        };
        let key = component.key();
        *estimates.entry((key.category, key.descriptor)).or_insert(0) += estimate;
    }
    let bonus = derived.selected_attack.check_bonus;
    // Candidate scoring consumes the same cached timing as the ability planner.
    let timing = game.ability_timing(actor, &basic).ok()?;
    Some(GearScore {
        maximum_health: derived.maximum_health,
        nutrition_rate_thirtieths: match creature.nutrition() {
            crate::nutrition::Nutrition::Exempt => 0,
            crate::nutrition::Nutrition::Required { .. } => {
                (30 + derived.nutrition_rate_thirtieths).max(0)
            }
        },
        sustained_attributes: derived.sustained_attributes.clone(),
        protects_armor: derived.protects_armor,
        see_invisible: derived.perception.see_invisible,
        stats,
        flags: derived.flags.clone(),
        abilities: derived.abilities.clone(),
        protection,
        attack: AttackScore {
            bonus,
            prepare: timing.preparation,
            recovery: timing.recovery,
            damage: estimates,
        },
    })
}
