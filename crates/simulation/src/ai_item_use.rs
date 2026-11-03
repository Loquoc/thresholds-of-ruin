//! Conservative known-item care. Selection never grants knowledge or executes effects.
use crate::ai_effect_safety::{condition_safety, modifier_improvement, ConditionSafety};
use crate::rules::{AmountExpr, Effect};
use crate::{ActorId, Game, Observation, Work};

fn positive(amount: &AmountExpr) -> bool {
    match amount {
        AmountExpr::Fixed { amount } => *amount > 0,
        AmountExpr::Rolled { dice } => dice.count > 0 && dice.sides > 0 && dice.bonus >= 0,
    }
}
impl Game {
    fn useful_safe_item_effects(
        &self,
        actor: ActorId,
        view: &Observation,
        effects: &[Effect],
        threat: Option<ActorId>,
    ) -> bool {
        let Some(creature) = self.creature(actor) else {
            return false;
        };
        let rules = creature.build().ruleset();
        let active: Vec<_> = creature.conditions().instances().collect();
        let Some((hp, maximum)) = self.health(actor) else {
            return false;
        };
        let mut useful = false;
        let mut added_families = std::collections::BTreeSet::new();
        for effect in effects {
            match effect {
                Effect::Heal { amount } if positive(amount) => {
                    useful |= u128::from(hp) * 2 <= u128::from(maximum)
                }
                Effect::HealPerHitDie { sides, bonus, .. } if *sides > 0 && *bonus >= 0 => {
                    useful |= u128::from(hp) * 2 <= u128::from(maximum)
                }
                Effect::RestoreAttribute { attribute, amount } if positive(amount) => {
                    useful |= *creature.restorable_losses().attributes.get(*attribute) > 0
                }
                Effect::RestoreMaximumHealth { amount } if positive(amount) => {
                    useful |= creature.restorable_losses().maximum_health > 0
                }
                Effect::RemoveCondition { condition } => {
                    let Some(definition) = rules
                        .condition(condition)
                        .filter(|d| condition_safety(d) == ConditionSafety::Harmful)
                    else {
                        return false;
                    };
                    useful |= active.iter().any(|entry| entry.condition == definition.id);
                }
                Effect::ApplyCondition { condition, .. }
                | Effect::ApplyConditionForTurns { condition, .. } => {
                    let positive_duration = match effect {
                        Effect::ApplyCondition { duration, .. } => *duration > 0,
                        Effect::ApplyConditionForTurns { duration, .. } => positive(duration),
                        _ => false,
                    };
                    if !positive_duration {
                        return false;
                    }
                    let Some(definition) = rules.condition(condition).filter(|d| {
                        matches!(
                            condition_safety(d),
                            ConditionSafety::Defensive | ConditionSafety::Beneficial
                        )
                    }) else {
                        return false;
                    };
                    if threat.is_none()
                        || active.len() + added_families.len() >= crate::limits::CONDITIONS
                        || !added_families.insert(&definition.family)
                        || active.iter().any(|entry| {
                            rules
                                .condition(&entry.condition)
                                .is_some_and(|other| other.family == definition.family)
                        })
                    {
                        return false;
                    }
                    let derived = creature.derived();
                    if definition.descriptors.iter().any(|descriptor| {
                        derived
                            .protection
                            .has_immunity(&crate::rules::Selector::Descriptor {
                                descriptor: descriptor.clone(),
                            })
                    }) {
                        return false;
                    }
                    let Some(modifiers_improve) = modifier_improvement(derived, definition) else {
                        return false;
                    };
                    let Some(edges_improve) = threat
                        .and_then(|target| derived.incoming_condition_edges(rules, target).ok())
                        .and_then(|incoming| incoming.improves(definition))
                    else {
                        return false;
                    };
                    if !modifiers_improve && !edges_improve {
                        return false;
                    }
                    useful = true;
                }
                Effect::AlterEquipment {
                    selection,
                    alteration,
                } => {
                    let Some(rows) = view.interactions.as_ref().map(|i| &i.inventory) else {
                        return false;
                    };
                    match (selection, alteration) {
                        (
                            crate::EquippedSelection::All,
                            crate::EquipmentAlteration::RemoveCurse,
                        ) => {
                            useful |= rows.iter().any(|row| {
                                row.equipped_slot.is_some() && row.known_cursed == Some(true)
                            });
                        }
                        (
                            crate::EquippedSelection::Slot {
                                slot,
                                class: crate::ItemClass::Armor,
                                last,
                            },
                            crate::EquipmentAlteration::ProtectArmor,
                        ) => {
                            let mut matches: Vec<_> = rows
                                .iter()
                                .filter(|row| {
                                    row.equipped_slot.is_some()
                                        && row.slot.as_ref().is_some_and(|s| s.id == *slot)
                                        && view.inventory.iter().any(|item| {
                                            item.id == row.item
                                                && item.class == crate::ItemClass::Armor
                                        })
                                })
                                .collect();
                            // If any potential preceding target is not understood,
                            // retain the scroll rather than infer a hidden profile.
                            if matches.iter().any(|row| row.known_equipment.is_none()) {
                                return false;
                            }
                            // The remembered view folds armor into modifiers and
                            // clears its source profile. Type knowledge above
                            // licenses this authored eligibility check, not any
                            // inspection of current hidden instance condition.
                            matches.retain(|row| {
                                self.items
                                    .get(&row.item)
                                    .and_then(|item| item.spec.equipment.as_ref())
                                    .is_some_and(|equipment| equipment.armor.is_some())
                            });
                            matches.sort_by_key(|row| row.equipped_slot);
                            let selected = if *last {
                                matches.last()
                            } else {
                                matches.first()
                            };
                            useful |=
                                selected.is_some_and(|row| row.known_protected == Some(false));
                        }
                        _ => return false,
                    }
                }
                _ => return false,
            }
        }
        useful
    }

    pub(crate) fn choose_ai_item_care(
        &self,
        actor: ActorId,
        view: &Observation,
        threat: Option<ActorId>,
    ) -> Option<crate::abilities::Plan> {
        // Match the established bounded equipment search; inventory can remain
        // larger, but care must never perform an unbounded planning scan.
        if view.inventory.len() > 128 {
            return None;
        }
        let rows = &view.interactions.as_ref()?.inventory;
        for disclosed in &view.inventory {
            if !disclosed.identified {
                continue;
            }
            let Some(row) = rows.iter().find(|row| row.item == disclosed.id) else {
                continue;
            };
            let Some(item) = self.items.get(&disclosed.id) else {
                continue;
            };
            if item.spec.concealed
                && !self
                    .actors
                    .get(&actor)?
                    .knowledge
                    .contains(&item.spec.identity)
            {
                continue;
            }
            let candidate = if row.drinkable {
                item.spec.consumable.as_ref().map(|c| {
                    (
                        Work::Drink { item: disclosed.id },
                        &c.effects,
                        &c.descriptors,
                    )
                })
            } else if row.readable {
                item.spec
                    .readable
                    .as_ref()
                    .filter(|r| r.control.is_none() && r.identify.is_none())
                    .map(|r| {
                        (
                            Work::Read { item: disclosed.id },
                            &r.effects,
                            &r.descriptors,
                        )
                    })
            } else {
                None
            };
            let Some((work, effects, descriptors)) = candidate else {
                continue;
            };
            let creature = self.creature(actor)?;
            let derived = creature.derived();
            if descriptors.iter().any(|descriptor| {
                derived
                    .protection
                    .has_immunity(&crate::rules::Selector::Descriptor {
                        descriptor: descriptor.clone(),
                    })
            }) {
                continue;
            }
            if !self.useful_safe_item_effects(actor, view, effects, threat) {
                continue;
            }
            if let Ok(plan) = self.plan_work(actor, work) {
                if let Some(plan) = self.physical_ai_plan(plan, view) {
                    return Some(plan);
                }
            }
        }
        None
    }
}
