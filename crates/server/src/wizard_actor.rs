//! Shared wizard addressing for the selected character or a loaded actor.
use crate::{engine::Failure, journal::WizardActorTarget as Target};
use tor_protocol::{ActorId, ErrorCode};
use tor_simulation::{ActorId as SimActor, Game};

pub(crate) fn parse_target(word: &str) -> Option<Target> {
    if word == "self" {
        return Some(Target::Selected {});
    }
    match word.parse::<u64>() {
        Ok(id) if id > 0 && id < u64::MAX => Some(Target::Actor { id: ActorId(id) }),
        _ => None,
    }
}

pub(crate) fn loaded_actor(
    game: &Game,
    selected: ActorId,
    target: Target,
) -> Result<ActorId, Failure> {
    let actor = match target {
        Target::Selected {} => selected,
        Target::Actor { id } => id,
    };
    if game.alive(SimActor(actor.0)) {
        Ok(actor)
    } else {
        Err(Failure::new(
            ErrorCode::InvalidAction,
            "Wizard target must be loaded and alive",
        ))
    }
}
