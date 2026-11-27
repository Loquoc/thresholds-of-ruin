//! Explicit authoritatively named projections; clients receive no rules catalog.
use s::{content_ids::*, rules::Ruleset};
use tor_protocol as p;
use tor_simulation as s;

pub(crate) fn label(id: &str) -> String {
    let text = id.replace(['_', '.'], " ");
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    })
}
fn named(id: impl ToString, name: Option<&str>) -> p::ContentRef {
    let id = id.to_string();
    p::ContentRef {
        name: name.map(str::to_owned).unwrap_or_else(|| label(&id)),
        id,
    }
}
fn primitive(value: impl serde::Serialize) -> p::ContentRef {
    // Mechanism enums have their independent stable text form; content is
    // always resolved through the selected ruleset below.
    let text = serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default();
    named(text, None)
}
pub(crate) fn resource(value: s::resources::Resource) -> p::ContentRef {
    primitive(value)
}
fn skill(value: s::attributes::Skill) -> p::ContentRef {
    primitive(value)
}
fn attribute(value: s::attributes::Attribute) -> p::ContentRef {
    primitive(value)
}
fn kind(value: &TypeId, rules: &Ruleset) -> p::ContentRef {
    named(
        value,
        rules
            .creature_type(value)
            .map(|definition| definition.name.as_str()),
    )
}
fn subtype(value: &SubtypeId, rules: &Ruleset) -> p::ContentRef {
    named(
        value,
        rules
            .subtype(value)
            .map(|definition| definition.name.as_str()),
    )
}
fn talent(value: &TalentId, rules: &Ruleset) -> p::ContentRef {
    named(
        value,
        rules
            .talent(value)
            .map(|definition| definition.name.as_str()),
    )
}
pub(crate) fn ability(value: &AbilityId, rules: &Ruleset) -> p::ContentRef {
    named(
        value,
        rules
            .ability(value)
            .map(|definition| definition.name.as_str()),
    )
}
fn condition(value: &ConditionId, rules: &Ruleset) -> p::ContentRef {
    named(
        value,
        rules
            .condition(value)
            .map(|definition| definition.name.as_str()),
    )
}
fn descriptor(value: &DescriptorId, rules: Option<&Ruleset>) -> p::ContentRef {
    named(
        value,
        rules
            .and_then(|rules| rules.descriptor(value))
            .map(|definition| definition.name.as_str()),
    )
}
fn category(value: s::combat::DamageType) -> p::DamageType {
    match value {
        s::combat::DamageType::Energy => p::DamageType::Energy,
        s::combat::DamageType::Impact => p::DamageType::Impact,
        s::combat::DamageType::Keen => p::DamageType::Keen,
        s::combat::DamageType::Spirit => p::DamageType::Spirit,
        s::combat::DamageType::Vital => p::DamageType::Vital,
        s::combat::DamageType::Psychic => p::DamageType::Psychic,
    }
}
fn attributes(value: s::attributes::Attributes) -> p::AttributeView {
    use s::attributes::Attribute as A;
    p::AttributeView {
        strength: value.get(A::Strength),
        speed: value.get(A::Speed),
        intellect: value.get(A::Intellect),
        willpower: value.get(A::Willpower),
        awareness: value.get(A::Awareness),
        presence: value.get(A::Presence),
    }
}
fn hd_source(value: s::progression::HdSource, rules: &Ruleset) -> p::ContentRef {
    match value {
        s::progression::HdSource::Racial => named("racial", Some("Racial")),
        s::progression::HdSource::Class(id) => named(
            &id,
            rules.class(&id).map(|definition| definition.name.as_str()),
        ),
    }
}
fn hd_source_kind(value: &s::progression::HdSource) -> p::HitDieSourceKind {
    match value {
        s::progression::HdSource::Racial => p::HitDieSourceKind::Racial,
        s::progression::HdSource::Class(_) => p::HitDieSourceKind::Class,
    }
}
#[path = "combat_diagnostics_view.rs"]
mod diagnostics;
pub(crate) use diagnostics::{combat_diagnostics, cost_bundle, DiagnosticProjectionError};
#[path = "creature_inspection_view.rs"]
mod inspection;
pub(crate) use inspection::pages::{project as strike_page, PageBudget, ProjectedPage};
pub(crate) use inspection::{attack_view, inspect, modifier};

pub(crate) fn expertise_rows(
    ownership: &s::expertise::ExpertiseOwnership,
    rules: &Ruleset,
) -> Vec<p::AdvancementExpertiseRow> {
    fn reference(id: impl ToString, name: Option<&str>) -> p::AdvancementContentRef {
        let value = named(id, name);
        p::AdvancementContentRef {
            id: value.id,
            name: value.name,
        }
    }
    fn status(active: bool) -> p::AdvancementStatus {
        if active {
            p::AdvancementStatus::Active
        } else {
            p::AdvancementStatus::Dormant
        }
    }
    ownership
        .grants()
        .iter()
        .map(|(id, origins)| p::AdvancementExpertiseRow {
            expertise: reference(id, rules.expertise(id).map(|def| def.name.as_str())),
            status: status(ownership.active(id)),
            source_count: u16::try_from(origins.len()).expect("bounded expertise sources"),
        })
        .collect()
}

pub(crate) fn own_stats(stats: s::creatures::OwnStats) -> p::OwnStats {
    let rules = &stats.rules;
    // The condition is authored. Neither the displayed name nor generic
    // action denial identifies the scenario's hunger-induced faint.
    let fainting = match &rules.document().nutrition_condition_trigger {
        s::nutrition::NutritionConditionTrigger::OnEmptyDepletion { condition, .. }
            if rules.condition(condition).is_some_and(|definition| {
                definition
                    .restrictions
                    .contains(&s::rules::ConditionRestriction::VoluntaryActions)
            }) =>
        {
            stats.conditions.iter().any(|entry| {
                &entry.condition == condition
                    && entry.phase == s::conditions::ConditionPhase::Active
            })
        }
        _ => false,
    };
    let mut sources: Vec<_> = stats
        .hit_dice
        .classes
        .iter()
        .map(|(class, count)| p::HitDiceSourceCount {
            source_kind: p::HitDieSourceKind::Class,
            source: hd_source(s::progression::HdSource::Class(class.clone()), rules),
            count: count.get(),
        })
        .collect();
    if stats.hit_dice.racial.get() > 0 {
        sources.push(p::HitDiceSourceCount {
            source_kind: p::HitDieSourceKind::Racial,
            source: hd_source(s::progression::HdSource::Racial, rules),
            count: stats.hit_dice.racial.get(),
        });
    }
    p::OwnStats {
        expertises: expertise_rows(&stats.expertises, rules),
        kind: kind(&stats.kind, rules),
        subtypes: stats
            .subtypes
            .iter()
            .map(|value| subtype(value, rules))
            .collect(),
        hit_dice: p::HitDiceSummary {
            total: stats.hit_dice.total.get(),
            sources,
        },
        attributes: attributes(stats.attributes),
        skills: s::attributes::Skill::ALL
            .into_iter()
            .map(|value| p::SkillView {
                skill: skill(value),
                rank: stats.skills.get(value),
            })
            .collect(),
        defenses: p::DefenseView {
            physical: stats.defenses.physical,
            cognitive: stats.defenses.cognitive,
            spiritual: stats.defenses.spiritual,
        },
        binding: primitive(stats.binding),
        restorable_losses: p::RestorableLossesView {
            hit_dice: stats.restorable_hit_dice.get(),
            attributes: p::AttributeView {
                strength: stats.restorable_losses.attributes.strength,
                speed: stats.restorable_losses.attributes.speed,
                intellect: stats.restorable_losses.attributes.intellect,
                willpower: stats.restorable_losses.attributes.willpower,
                awareness: stats.restorable_losses.attributes.awareness,
                presence: stats.restorable_losses.attributes.presence,
            },
            maximum_health: stats.restorable_losses.maximum_health,
        },
        nutrition: match stats.nutrition {
            s::nutrition::NutritionView::Exempt => p::NutritionView::Exempt,
            s::nutrition::NutritionView::Required {
                reserve,
                maximum,
                hunger,
                unit_ticks,
                elapsed,
                starve_at,
            } => p::NutritionView::Required {
                reserve,
                maximum,
                fainting,
                unit_ticks,
                elapsed,
                starve_at,
                hunger: match hunger {
                    s::nutrition::Hunger::Nourished => p::Hunger::Nourished,
                    s::nutrition::Hunger::Hungry => p::Hunger::Hungry,
                    s::nutrition::Hunger::Weak => p::Hunger::Weak,
                    s::nutrition::Hunger::Empty => p::Hunger::Empty,
                },
            },
        },
        conditions: stats
            .conditions
            .into_iter()
            .map(|entry| p::PersonalCondition {
                condition: condition(&entry.condition, rules),
                phase: match entry.phase {
                    s::conditions::ConditionPhase::Active => p::ConditionPhase::Active,
                    s::conditions::ConditionPhase::Protection => p::ConditionPhase::Protection,
                },
                remaining_ticks: entry.remaining_ticks,
                appearance: p::EffectAppearance::StatusIndicator,
                cue: p::EffectCue::Status,
            })
            .collect(),
        health_recovery: p::HealthRecoveryView {
            period_ticks: stats.health_recovery.period(),
            elapsed_ticks: stats.health_recovery.elapsed(),
            stopped: stats.health_recovery.stopped(),
            reset_percent: stats.health_recovery_reset_percent,
        },
        resources: stats
            .resources
            .into_iter()
            .map(|value| p::ResourceView {
                resource: resource(value.resource),
                balance: value.balance,
                maximum: value.maximum,
                available: value.available,
                reserved: value.reserved,
            })
            .collect(),
        active_talents: stats
            .active_talents
            .iter()
            .map(|value| talent(value, rules))
            .collect(),
        dormant_talents: stats
            .dormant_talents
            .iter()
            .map(|value| talent(value, rules))
            .collect(),
        abilities: stats
            .abilities
            .iter()
            .filter_map(|id| rules.ability(id))
            .map(|def| p::AbilityView {
                id: def.id.to_string(),
                name: def.name.clone(),
                basic_attack: def.basic_attack,
                weapon_selection: stats.weapon_abilities.contains(&def.id),
                cooldown_remaining: stats.ability_cooldowns[&def.id],
                supply_costs: def
                    .supply_costs
                    .iter()
                    .map(|cost| p::AbilitySupplyCost {
                        supply: cost.supply.to_string(),
                        name: rules
                            .supply(&cost.supply)
                            .expect("validated supply")
                            .name
                            .clone(),
                        quantity: cost.quantity,
                        access: match cost.access {
                            s::rules::SupplyAccess::CarriedOnly => p::SupplyAccess::CarriedOnly,
                            s::rules::SupplyAccess::CarriedOrGroundTouch => {
                                p::SupplyAccess::CarriedOrGroundTouch
                            }
                        },
                    })
                    .collect(),
                cost: def.cost.as_ref().map(|value| p::AbilityCost {
                    resource: resource(value.resource),
                    start: value.start,
                    resolution: value.resolution,
                }),
                reach: match stats.ability_reaches[&def.id] {
                    s::rules::Reach::Melee => p::AbilityReach::Melee,
                    s::rules::Reach::Projectile => p::AbilityReach::Projectile,
                    s::rules::Reach::Sight { range } | s::rules::Reach::MutualSight { range } => {
                        p::AbilityReach::Sight { range }
                    }
                    s::rules::Reach::UntilObstruction => p::AbilityReach::UntilObstruction,
                    s::rules::Reach::SelfOnly => p::AbilityReach::SelfOnly,
                    s::rules::Reach::SelfOrAlly { range } => p::AbilityReach::SelfOrAlly {
                        range: match range {
                            s::rules::AllyRange::Touch => p::AbilityAllyRange::Touch,
                            s::rules::AllyRange::Sight { range } => {
                                p::AbilityAllyRange::Sight { range }
                            }
                        },
                    },
                },
                target_kind: if def.invocation == s::abilities::InvocationKind::SelfStep {
                    p::AbilityTargetKind::SelfStep
                } else if def.invocation == s::abilities::InvocationKind::DirectionalProjectile {
                    p::AbilityTargetKind::DirectionalProjectile
                } else if def.invocation.is_directional_ray() {
                    p::AbilityTargetKind::DirectionalRay
                } else {
                    match def.reach {
                        s::rules::Reach::UntilObstruction => p::AbilityTargetKind::DirectionalRay,
                        s::rules::Reach::Projectile => p::AbilityTargetKind::DirectionalProjectile,
                        s::rules::Reach::SelfOnly => p::AbilityTargetKind::SelfOnly,
                        s::rules::Reach::SelfOrAlly { .. } => p::AbilityTargetKind::SelfOrAlly,
                        s::rules::Reach::Melee
                        | s::rules::Reach::Sight { .. }
                        | s::rules::Reach::MutualSight { .. } => p::AbilityTargetKind::Actor,
                    }
                },
            })
            .collect(),
    }
}

pub(crate) fn outcome_status(value: s::abilities::OutcomeStatus) -> p::AbilityOutcome {
    match value {
        s::abilities::OutcomeStatus::Applied => p::AbilityOutcome::Applied,
        s::abilities::OutcomeStatus::Unaffected => p::AbilityOutcome::Unaffected,
        s::abilities::OutcomeStatus::Miss => p::AbilityOutcome::Miss,
    }
}

#[cfg(test)]
#[path = "creature_view_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "strike_inspection_tests.rs"]
pub(crate) mod strike_page_tests;
