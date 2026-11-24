//! Literal reader cues from canonical, already-applied scroll payloads.
//! None of these lines identify an unknown item or report a numerical bonus.
use crate::abilities::{Outcome, OutcomeStatus, ResolvedEffect};
use crate::{ActorId, EquipmentAlteration, Game, GameError};

#[derive(Clone, Copy)]
struct Senses {
    sight: bool,
    hallucinating: bool,
}
impl Senses {
    fn perceived(self, normal: &'static str, distorted: &'static str) -> &'static str {
        if self.hallucinating {
            distorted
        } else {
            normal
        }
    }

    fn visual(self, normal: &'static str, distorted: &'static str) -> Option<&'static str> {
        self.sight.then(|| self.perceived(normal, distorted))
    }

    fn read_cue(self, effect: &ResolvedEffect) -> Option<&'static str> {
        if !effect.changed() {
            return None;
        }
        match effect {
            ResolvedEffect::AlterEquipment { alteration } => match alteration.plan.alteration {
                EquipmentAlteration::EnchantWeapon => self.visual(
                    "A brief glow runs along your weapon.",
                    "Your weapon flashes a checkerboard of colors.",
                ),
                EquipmentAlteration::EnchantArmor => self.visual(
                    "Your armor glows for a moment.",
                    "Your armor ripples with impossible stripes.",
                ),
                EquipmentAlteration::ProtectArmor => self.visual(
                    "A faint sheen settles over your armor.",
                    "Your armor shimmers like a soap bubble.",
                ),
                EquipmentAlteration::RemoveCurse => Some(self.perceived(
                    "You feel a weight lift.",
                    "It feels as if a tiny knot untied itself.",
                )),
            },
            ResolvedEffect::ApplyCondition { id, .. } if id.as_str() == "rogue.sleep" => {
                Some(self.perceived(
                    "A wave of drowsiness overtakes you.",
                    "It feels as if your thoughts are sinking into a pillow.",
                ))
            }
            ResolvedEffect::Relocate { .. } => Some(self.perceived(
                "You feel a sudden shift beneath your feet.",
                "It feels as if the floor skipped a beat.",
            )),
            ResolvedEffect::MapTopology { .. } => Some(self.perceived(
                "You sense the layout of the level.",
                "A maze clicks into place in your head.",
            )),
            ResolvedEffect::DetectFood { .. } => Some(self.perceived(
                "You sense where food lies.",
                "An imaginary dinner bell points the way to food.",
            )),
            _ => None,
        }
    }
}
impl Game {
    pub(crate) fn narrate_read_effects(
        &mut self,
        reader: ActorId,
        outcome: &Outcome,
    ) -> Result<(), GameError> {
        if outcome.result != OutcomeStatus::Applied {
            return Ok(());
        }
        let senses = Senses {
            sight: self
                .actors
                .get(&reader)
                .ok_or(GameError::UnknownActor)?
                .perception()
                .sight,
            hallucinating: self.projected_hallucination(reader)?.is_some(),
        };
        let mut cues = Vec::new();
        for cue in outcome
            .effects
            .iter()
            .filter_map(|effect| senses.read_cue(effect))
        {
            if !cues.contains(&cue) {
                cues.push(cue);
                self.narrate_gameplay(reader, cue.into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blind_hallucination_distorts_a_sensation_but_does_not_supply_sight() {
        let blind = Senses {
            sight: false,
            hallucinating: true,
        };
        assert!(blind.visual("A glow", "A plaid glow").is_none());
        let sleepy = ResolvedEffect::ApplyCondition {
            id: crate::content_ids::ConditionId::new("rogue.sleep").unwrap(),
            duration: 100,
        };
        assert_eq!(
            blind.read_cue(&sleepy),
            Some("It feels as if your thoughts are sinking into a pillow.")
        );
        let sighted = Senses {
            sight: true,
            hallucinating: true,
        };
        assert_eq!(
            sighted.visual("A glow", "A plaid glow"),
            Some("A plaid glow")
        );
        assert_eq!(
            blind.read_cue(&ResolvedEffect::DetectFood { changed: false }),
            None
        );
        assert_eq!(
            blind.read_cue(&ResolvedEffect::MapTopology { changed: true }),
            Some("A maze clicks into place in your head.")
        );
    }
}
