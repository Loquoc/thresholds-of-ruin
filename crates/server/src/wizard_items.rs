//! Wizard ground and inventory creation share the exact seeded item recipe.
use crate::{
    adapt,
    engine::Failure,
    journal::{WizardOperation, WizardResult},
    regions::Regions,
    wizard_actor::{loaded_actor, parse_target},
};
use tor_protocol::{ActorId, ErrorCode};
use tor_simulation::{ActorId as SimActor, Game};

const GIVE_USAGE: &str =
    "Wizard item give <self|actor-id> <identity|appearance|recipe> <id> [<selector> <id> …]";

#[derive(Default)]
pub(crate) struct Selectors {
    pub identity: Option<String>,
    pub appearance: Option<String>,
    pub recipe: Option<String>,
}

impl Selectors {
    pub(crate) fn parse(words: &[&str]) -> Option<Self> {
        if words.is_empty() || words.len() > 6 || !words.len().is_multiple_of(2) {
            return None;
        }
        let mut parsed = Self::default();
        for &[key, value] in words.as_chunks::<2>().0 {
            if if key == "recipe" {
                value.len() > 80 || !crate::wizard_creatures::valid_selector(value)
            } else {
                !tor_simulation::content_ids::valid_id(value)
                    || !value.contains('.')
                    || (key == "identity" && value.split('.').count() < 3)
            } {
                return None;
            }
            let slot = match key {
                "identity" => &mut parsed.identity,
                "appearance" => &mut parsed.appearance,
                "recipe" => &mut parsed.recipe,
                _ => return None,
            };
            if slot.replace(value.to_owned()).is_some() {
                return None;
            }
        }
        Some(parsed)
    }
}

pub(crate) fn parse_give(actor: &str, words: &[&str]) -> Result<WizardOperation, String> {
    let target = parse_target(actor).ok_or(GIVE_USAGE)?;
    let Selectors {
        identity,
        appearance,
        recipe,
    } = Selectors::parse(words).ok_or(GIVE_USAGE)?;
    Ok(WizardOperation::GiveItem {
        target,
        identity,
        appearance,
        recipe,
    })
}

pub(crate) fn execute(
    game: &mut Game,
    regions: Option<&Regions>,
    selected: ActorId,
    operation: &WizardOperation,
) -> Result<WizardResult, Failure> {
    let invalid = || {
        Failure::new(
            ErrorCode::InvalidAction,
            "Wizard item target or settings are unavailable",
        )
    };
    let resolve =
        |identity: &Option<String>, appearance: &Option<String>, recipe: &Option<String>| {
            let spec = regions
                .ok_or_else(invalid)?
                .wizard_item(
                    identity.as_deref(),
                    appearance.as_deref(),
                    recipe.as_deref(),
                )
                .map_err(|error| Failure::new(ErrorCode::InvalidAction, &error.message))?;
            if game
                .unique_retrieval_policy()
                .is_some_and(|policy| policy.item.identity == spec.identity)
            {
                return Err(invalid());
            }
            Ok(spec)
        };
    let creation_failed = |error| Failure::simulation_failure(error).unwrap_or_else(invalid);
    match operation {
        WizardOperation::SpawnItem {
            position,
            identity,
            appearance,
            recipe,
        } => {
            let spec = resolve(identity, appearance, recipe)?;
            let item = game
                .spawn_item_stack(adapt::location(*position), None, 1, spec)
                .map_err(creation_failed)?;
            Ok(WizardResult::ItemPlaced { item: item.0 })
        }
        WizardOperation::GiveItem {
            target,
            identity,
            appearance,
            recipe,
        } => {
            let actor = loaded_actor(game, selected, *target)?;
            let spec = resolve(identity, appearance, recipe)?;
            let item = game
                .spawn_carried_item_stack(SimActor(actor.0), 1, spec)
                .map_err(creation_failed)?;
            Ok(WizardResult::ItemGiven {
                actor,
                item: item.0,
            })
        }
        _ => unreachable!("caller dispatches only wizard item creation"),
    }
}
