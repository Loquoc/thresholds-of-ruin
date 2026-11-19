//! Private wizard adapter to ordinary simulation condition transactions.
use crate::engine::Failure;
use crate::journal::{WizardConditionOutcome as Outcome, WizardOperation, WizardResult};
use crate::wizard_actor::{loaded_actor, parse_target};
use tor_protocol::{ActorId, ErrorCode};
use tor_simulation::{
    conditions::ConditionUpdate, content_ids::ConditionId, rules::ConditionTicks,
    ActorId as SimActor, Game,
};

const USAGE: &str = "Wizard conditions: condition apply <self|actor-id> <condition-id> <active-ticks> [source <self|actor-id>]; condition clear <self|actor-id> <condition-id>";

pub(crate) fn parse(words: &[&str]) -> Result<WizardOperation, String> {
    let condition = |word: &str| ConditionId::new(word).map_err(|_| USAGE.to_owned());
    match words {
        ["condition", "apply", recipient, id, ticks, rest @ ..] => {
            let source = match rest {
                [] => None,
                ["source", word] => Some(parse_target(word).ok_or(USAGE)?),
                _ => return Err(USAGE.into()),
            };
            Ok(WizardOperation::ApplyCondition {
                target: parse_target(recipient).ok_or(USAGE)?,
                condition: condition(id)?,
                ticks: ConditionTicks::new(ticks.parse().map_err(|_| USAGE.to_owned())?)
                    .map_err(|_| USAGE.to_owned())?,
                source,
            })
        }
        ["condition", "clear", recipient, id] => Ok(WizardOperation::ClearCondition {
            target: parse_target(recipient).ok_or(USAGE)?,
            condition: condition(id)?,
        }),
        _ => Err(USAGE.into()),
    }
}

pub(crate) fn execute(
    game: &mut Game,
    selected: ActorId,
    operation: &WizardOperation,
) -> Result<WizardResult, Failure> {
    let failure = |error| {
        if matches!(
            error,
            tor_simulation::GameError::Rule(
                tor_simulation::abilities::RuleError::ConditionCapacity
            )
        ) {
            return Failure::new(ErrorCode::ResourceLimit, "Condition capacity exhausted");
        }
        Failure::simulation_failure(error).unwrap_or_else(|| {
            Failure::new(
                ErrorCode::InvalidAction,
                "Wizard condition or duration is not applicable",
            )
        })
    };
    let (actor, condition, outcome) = match operation {
        WizardOperation::ApplyCondition {
            target,
            condition,
            ticks,
            source,
        } => {
            let actor = loaded_actor(game, selected, *target)?;
            let source = source
                .map(|s| loaded_actor(game, selected, s))
                .transpose()?
                .unwrap_or(actor);
            let update = game
                .apply_condition(
                    SimActor(actor.0),
                    condition,
                    SimActor(source.0),
                    ticks.get(),
                )
                .map_err(failure)?;
            let outcome = match update {
                ConditionUpdate::Applied => Outcome::Applied,
                ConditionUpdate::Refreshed => Outcome::Refreshed,
                ConditionUpdate::Stacked => Outcome::Stacked,
                ConditionUpdate::Immune => Outcome::Immune,
                ConditionUpdate::Evicted => Outcome::Evicted,
                ConditionUpdate::Reserved => Outcome::Reserved,
                ConditionUpdate::Unchanged => Outcome::Unchanged,
            };
            (actor, condition, outcome)
        }
        WizardOperation::ClearCondition { target, condition } => {
            let actor = loaded_actor(game, selected, *target)?;
            let changed = game
                .remove_condition(SimActor(actor.0), condition)
                .map_err(failure)?;
            (
                actor,
                condition,
                if changed {
                    Outcome::Cleared
                } else {
                    Outcome::NotActive
                },
            )
        }
        _ => unreachable!("caller dispatches only wizard condition operations"),
    };
    Ok(WizardResult::ConditionChanged {
        actor,
        condition: condition.clone(),
        outcome,
    })
}

pub(crate) fn summary(actor: ActorId, condition: &ConditionId, outcome: Outcome) -> String {
    let label = match outcome {
        Outcome::Applied => "applied",
        Outcome::Refreshed => "refreshed",
        Outcome::Stacked => "stacked",
        Outcome::Immune => "immune",
        Outcome::Evicted => "applied with eviction",
        Outcome::Reserved => "held by paid work",
        Outcome::Unchanged => "unchanged",
        Outcome::Cleared => "cleared",
        Outcome::NotActive => "not active",
    };
    format!("Actor {} condition {condition}: {label}.", actor.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::developer::parse_wizard;
    use crate::journal::WizardActorTarget as Target;

    #[test]
    fn condition_commands_validate_target_duration_and_complete_source_clause() {
        let apply = parse_wizard("condition apply 42 readied 300 source self").unwrap();
        assert!(matches!(apply, WizardOperation::ApplyCondition {
            target: Target::Actor { id: ActorId(42) },
            ticks,
            source: Some(Target::Selected {}),
            ..
        } if ticks.get() == 300));
        assert!(matches!(
            parse_wizard("condition clear self readied").unwrap(),
            WizardOperation::ClearCondition {
                target: Target::Selected {},
                ..
            }
        ));
        assert_eq!(
            serde_json::from_str::<WizardOperation>(&serde_json::to_string(&apply).unwrap())
                .unwrap(),
            apply
        );
        for invalid in [
            "condition apply self readied 0",
            "condition apply self readied 18446744073709551615",
            "condition apply 0 readied 100",
            "condition apply self not/a/condition 100",
            "condition apply self readied 100 source",
            "condition apply self readied 100 source 0",
            "condition clear self readied source self",
        ] {
            assert!(parse_wizard(invalid).is_err(), "{invalid}");
        }
    }
}
