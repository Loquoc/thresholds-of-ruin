//! Past-tense activity facts about other visible actors.
//!
//! These describe only what an observer could learn by watching at the
//! committing boundary: a step between disclosed cells, the category of a
//! public wind-up, or visible item handling under the observer's own item
//! names. Ready ticks, remaining work, aims and ability identities are never
//! derived here; idle, recovering and private work stay indistinguishable.
use super::*;

impl Engine {
    pub(crate) fn outcome_activities(
        &self,
        result: &CommandResult,
        observer: ActorId,
        before: &tor_protocol::OutcomeObservation,
        after: &tor_protocol::OutcomeObservation,
    ) -> Vec<tor_protocol::OutcomeFact> {
        use tor_protocol::{ActivityKind, ActivityPosture, ItemWorkKind, OutcomeFact};
        use tor_simulation::public_plan::PublicPosture;

        let entry = &result.entry;
        if result.duplicate || entry.actor == observer {
            return Vec::new();
        }
        let action = match &entry.content {
            JournalContent::IntentionStarted { action, .. }
            | JournalContent::IntentionContinued { action, .. }
            | JournalContent::Action { action, .. } => action,
            _ => return Vec::new(),
        };
        let subject = SimActor(entry.actor.0);
        let scope = self.target_scope(observer);
        let handles = [scope.actor(subject), scope.contact(subject)];
        // The scene's single referent for a multi-cell body: lowest, then nearest.
        let sighting = |view: &tor_protocol::OutcomeObservation| {
            view.visible_actors
                .iter()
                .filter(|row| handles.contains(&row.id))
                .min_by_key(|row| {
                    let p = row.position;
                    (
                        p.z,
                        u64::from(p.x.unsigned_abs())
                            + u64::from(p.y.unsigned_abs())
                            + u64::from(p.z.unsigned_abs()),
                    )
                })
                .map(|row| (row.id, row.position))
        };
        let (was, now) = (sighting(before), sighting(after));
        let Some((target, _)) = now.or(was) else {
            return Vec::new();
        };
        let activity = match action {
            Action::Move { .. } => {
                // An observer displaced in the same boundary changes frame, so
                // the earlier offset would describe a different cell.
                let shifted = after
                    .motion
                    .as_ref()
                    .is_some_and(|motion| motion.displaced || motion.impacted);
                let from = was.map(|(_, p)| p).filter(|_| !shifted);
                let to = now.map(|(_, p)| p);
                if from.is_none() && to.is_none() || (from.is_some() && from == to) {
                    return Vec::new();
                }
                ActivityKind::Move { from, to }
            }
            Action::UseAbility { .. } => {
                // The observer's current sighting carries the creature handle
                // exactly when perception classified the subject as a creature;
                // a contact-only sighting discloses no plan, as for paid observation.
                if now.is_none_or(|(id, _)| id != handles[0]) {
                    return Vec::new();
                }
                let Some(posture) = self.game.public_posture(subject) else {
                    return Vec::new();
                };
                ActivityKind::WindUp {
                    posture: match posture {
                        PublicPosture::Melee => ActivityPosture::Melee,
                        PublicPosture::Ranged => ActivityPosture::Ranged,
                        PublicPosture::Casting => ActivityPosture::Casting,
                        PublicPosture::Other => ActivityPosture::Other,
                    },
                }
            }
            Action::Equip { item, .. }
            | Action::Unequip { item }
            | Action::Drink { item }
            | Action::Eat { item }
            | Action::Read { item }
            | Action::Ignite { item }
            | Action::Extinguish { item }
            | Action::Zap { item, .. } => {
                // Item handling is a physical act: it needs a current sighting.
                if now.is_none() {
                    return Vec::new();
                }
                let work = match action {
                    Action::Equip { .. } => ItemWorkKind::Equip,
                    Action::Unequip { .. } => ItemWorkKind::Unequip,
                    Action::Drink { .. } => ItemWorkKind::Drink,
                    Action::Eat { .. } => ItemWorkKind::Eat,
                    Action::Read { .. } => ItemWorkKind::Read,
                    Action::Ignite { .. } => ItemWorkKind::Ignite,
                    Action::Extinguish { .. } => ItemWorkKind::Extinguish,
                    _ => ItemWorkKind::Zap,
                };
                let name = self
                    .game
                    .observed_carried_item_name(SimActor(observer.0), tor_simulation::ItemId(*item))
                    .filter(|name| {
                        !name.trim().is_empty()
                            && name.len() <= 256
                            && !name.chars().any(char::is_control)
                    });
                ActivityKind::ItemHandling { work, name }
            }
            _ => return Vec::new(),
        };
        vec![OutcomeFact::Activity {
            actor: target,
            started_at: entry.tick,
            activity,
        }]
    }
}

#[cfg(test)]
#[path = "outcome_activity_engine_tests.rs"]
mod tests;
