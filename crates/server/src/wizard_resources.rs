//! Private wizard restoration through the ordinary actor resource transaction.
use std::num::NonZeroU32;

use crate::engine::Failure;
use crate::journal::{WizardOperation, WizardResult};
use crate::wizard_actor::{loaded_actor, parse_target};
use tor_protocol::{ActorId, ErrorCode};
use tor_simulation::{resources::Resource, ActorId as SimActor, Game};

const USAGE: &str = "Wizard resources: resource restore <self|actor-id> <stamina|focus|mana> <positive-u32>; resource fill <self|actor-id> <pool>";

pub(crate) fn parse(words: &[&str]) -> Result<WizardOperation, String> {
    let (recipient, pool, amount) = match words {
        ["resource", "restore", recipient, pool, amount] => (
            *recipient,
            *pool,
            amount.parse::<NonZeroU32>().map_err(|_| USAGE)?,
        ),
        ["resource", "fill", recipient, pool] => (*recipient, *pool, NonZeroU32::MAX),
        _ => return Err(USAGE.into()),
    };
    let resource = match pool {
        "stamina" => Resource::Stamina,
        "focus" => Resource::Focus,
        "mana" => Resource::Mana,
        _ => return Err(USAGE.into()),
    };
    Ok(WizardOperation::RestoreResource {
        target: parse_target(recipient).ok_or(USAGE)?,
        resource,
        amount,
    })
}

pub(crate) fn execute(
    game: &mut Game,
    selected: ActorId,
    operation: &WizardOperation,
) -> Result<WizardResult, Failure> {
    let WizardOperation::RestoreResource {
        target,
        resource,
        amount,
    } = operation
    else {
        return Err(Failure::new(
            ErrorCode::InvalidAction,
            "Expected resource restoration",
        ));
    };
    let actor = loaded_actor(game, selected, *target)?;
    let restored = game
        .restore_resource(SimActor(actor.0), *resource, amount.get())
        .map_err(|error| {
            Failure::simulation_failure(error).unwrap_or_else(|| {
                Failure::new(
                    ErrorCode::InvalidAction,
                    "Wizard target resource is unavailable",
                )
            })
        })?;
    Ok(WizardResult::ResourceRestored {
        actor,
        resource: *resource,
        restored,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::WizardActorTarget;

    #[test]
    fn wizard_resource_parser_keeps_fill_and_restore_in_one_bounded_operation() {
        for (words, expected) in [
            (
                "resource fill self focus",
                (WizardActorTarget::Selected {}, Resource::Focus, u32::MAX),
            ),
            (
                "resource restore 12 stamina 3",
                (
                    WizardActorTarget::Actor { id: ActorId(12) },
                    Resource::Stamina,
                    3,
                ),
            ),
            (
                "resource restore self mana 4294967295",
                (WizardActorTarget::Selected {}, Resource::Mana, u32::MAX),
            ),
        ] {
            let op = parse(&words.split_whitespace().collect::<Vec<_>>()).unwrap();
            assert!(
                matches!(op, WizardOperation::RestoreResource { target, resource, amount }
                if (target, resource, amount.get()) == expected)
            );
            let bytes = serde_json::to_vec(&op).unwrap();
            assert_eq!(
                serde_json::from_slice::<WizardOperation>(&bytes).unwrap(),
                op
            );
        }
        for words in [
            "resource restore self stamina 0",
            "resource restore self stamina -1",
            "resource restore self stamina 4294967296",
            "resource restore 0 mana 5",
            "resource fill 18446744073709551615 mana",
            "resource fill self health",
            "resource restore self nutrition 1",
            "resource fill self mana 1",
            "resource set self mana 1",
            "resource drain self mana 1",
        ] {
            assert!(
                parse(&words.split_whitespace().collect::<Vec<_>>()).is_err(),
                "{words}"
            );
        }
    }
}
