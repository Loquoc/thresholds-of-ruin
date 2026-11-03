//! Shared conservative condition-shape analysis for ability and item decisions.
//! This inspects known authored mechanics, never source names or live hidden facts.
use crate::rules::{
    ConditionDef, ConditionPreparation, Eviction, ModifierOp, Scope, Stacking, Stat,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConditionSafety {
    Defensive,
    Beneficial,
    Harmful,
    Unsupported,
}

pub(crate) fn condition_safety(condition: &ConditionDef) -> ConditionSafety {
    if condition.source_scoped || condition.detecting_monsters {
        return ConditionSafety::Unsupported;
    }
    let harmful = condition.modifiers.iter().all(|modifier| {
        modifier.scope == Scope::Always
            && matches!(
                modifier.op,
                ModifierOp::ActionDuration {
                    value: crate::action_duration::ActionDuration::Slow
                } | ModifierOp::Perception {
                    value: crate::perception::PerceptionValue::Sight { enabled: false }
                }
            )
    });
    if harmful
        && (condition.hallucinating
            || !condition.restrictions.is_empty()
            || !condition.modifiers.is_empty())
    {
        return ConditionSafety::Harmful;
    }
    if condition.hallucinating
        || !condition.restrictions.is_empty()
        || condition.preparation != ConditionPreparation::Preserve
        || condition.capacity != Eviction::Reject
        || matches!(condition.stacking, Stacking::Stack { .. })
        || condition.modifiers.is_empty()
    {
        return ConditionSafety::Unsupported;
    }
    let mut defensive_only = true;
    for modifier in &condition.modifiers {
        if modifier.scope != Scope::Always {
            return ConditionSafety::Unsupported;
        }
        match modifier.op {
            ModifierOp::Add {
                stat: Stat::IncomingEdge { .. },
                value,
            } if value < 0 => {}
            ModifierOp::TalentReduction { amount, .. } if amount > 0 => {}
            ModifierOp::ActionDuration {
                value: crate::action_duration::ActionDuration::Haste,
            }
            | ModifierOp::Perception {
                value: crate::perception::PerceptionValue::Invisible { enabled: true },
            } => defensive_only = false,
            _ => return ConditionSafety::Unsupported,
        }
    }
    if defensive_only {
        ConditionSafety::Defensive
    } else {
        ConditionSafety::Beneficial
    }
}

/// Compare non-edge modifiers to the actor's existing derived capabilities.
/// Incoming family-edge comparison remains the cached shared derivation query.
pub(crate) fn modifier_improvement(
    derived: &crate::creatures::DerivedCreature,
    condition: &ConditionDef,
) -> Option<bool> {
    let mut improves = false;
    for modifier in &condition.modifiers {
        match modifier.op {
            ModifierOp::TalentReduction { category, amount } => {
                let selector = crate::rules::Selector::Category { category };
                let current = derived.protection.talent_reduction(&selector);
                let total = derived.protection.reduction(&selector) - current + current.max(amount);
                if total > crate::limits::NUMERIC as u32 {
                    return None;
                }
                improves |= !derived.protection.has_immunity(&selector) && amount > current;
            }
            ModifierOp::ActionDuration {
                value: crate::action_duration::ActionDuration::Haste,
            } => {
                improves |= derived.action_duration != crate::action_duration::ActionDuration::Haste
            }
            ModifierOp::Perception {
                value: crate::perception::PerceptionValue::Invisible { enabled: true },
            } => improves |= !derived.perception.invisible,
            _ => {}
        }
    }
    Some(improves)
}
