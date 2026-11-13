//! Spatial delivery is captured by the execution path, never reconstructed by clients.
use super::{PlannedEffect, RayPlan};
use crate::combat::{EffectCue, EffectDelivery, SourceAbilityDisclosure, SpatialEffect};
use crate::{Game, GameError};
use tor_world::{Direction, Location};

/// Never draw an ordinary straight segment across a physical join. Repeated
/// bounce cells remain ordered; region and rotated/nonlocal edges break the strip.
pub(super) fn push_step(
    segments: &mut Vec<Vec<Location>>,
    from: Location,
    to: Location,
    direction: Direction,
    rotation: u8,
) {
    let (dx, dy, dz) = direction.delta();
    let ordinary = from.region == to.region
        && rotation == 0
        && i64::from(to.position.x) - i64::from(from.position.x) == i64::from(dx)
        && i64::from(to.position.y) - i64::from(from.position.y) == i64::from(dy)
        && i64::from(to.position.z) - i64::from(from.position.z) == i64::from(dz);
    if ordinary {
        segments.last_mut().expect("path starts at origin").push(to);
    } else {
        segments.push(vec![to]);
    }
}

pub(super) fn planned_cue(effects: &[PlannedEffect]) -> EffectCue {
    for effect in effects {
        if let PlannedEffect::Damage { damage } = effect {
            for component in damage.components() {
                if let Some(descriptor) = component.key().descriptor {
                    match descriptor.as_str() {
                        "fire" => return EffectCue::Fire,
                        "cold" => return EffectCue::Cold,
                        "lightning" => return EffectCue::Lightning,
                        "poison" => return EffectCue::Poison,
                        _ => {}
                    }
                }
            }
        }
    }
    if effects.iter().any(|effect| {
        matches!(
            effect,
            PlannedEffect::Heal { .. } | PlannedEffect::HealPerHitDie { .. }
        )
    }) {
        EffectCue::Healing
    } else if effects.iter().any(|effect| {
        matches!(
            effect,
            PlannedEffect::ApplyCondition { .. }
                | PlannedEffect::ApplyPersistentCondition { .. }
                | PlannedEffect::ApplyConditionForTurns { .. }
                | PlannedEffect::RemoveCondition { .. }
                | PlannedEffect::HasteDose { .. }
                | PlannedEffect::BalefulPolymorph { .. }
                | PlannedEffect::CancelSpecials
        )
    }) {
        EffectCue::Status
    } else {
        EffectCue::Neutral
    }
}

impl Game {
    pub(super) fn publish_ray_effect(
        &mut self,
        ray: &RayPlan,
        segments: Vec<Vec<Location>>,
        source_disclosure: SourceAbilityDisclosure,
        origin: crate::event_history::SpatialEffectOrigin,
    ) -> Result<(), GameError> {
        self.publish_spatial_effect(
            SpatialEffect {
                actor: ray.actor,
                target: None,
                ability: ray.ability.clone(),
                source_disclosure,
                tick: self.tick(),
                delivery: EffectDelivery::Ray,
                timing: crate::combat::EffectTiming::Instant,
                appearance: crate::combat::EffectAppearance::Bolt,
                cue: planned_cue(&ray.effects),
                segments,
            },
            origin,
        )
    }
}

impl Game {
    /// A target-local receipt marks the place the action resolved, not a beam,
    /// radius, or promise that an effect applied. Existing outcome events own that.
    pub(crate) fn publish_target_effect(
        &mut self,
        outcome: &super::Outcome,
        source_disclosure: SourceAbilityDisclosure,
    ) -> Result<(), GameError> {
        let Some(ability) = outcome.ability.as_ref() else {
            return Ok(());
        };
        let Some(definition) = self.content.rules.ability(ability) else {
            return Ok(());
        };
        if !matches!(
            definition.invocation,
            super::InvocationKind::ActorTarget | super::InvocationKind::SelfStep
        ) {
            return Ok(());
        }
        let Some(target) = self.actors.get(&outcome.target) else {
            return Ok(());
        };
        let location = target.location;
        let cue = if outcome.effects.iter().any(|effect| {
            matches!(
                effect,
                super::ResolvedEffect::Heal { .. }
                    | super::ResolvedEffect::HealAndImproveHitDice { .. }
            )
        }) {
            EffectCue::Healing
        } else if outcome.effects.iter().any(|effect| {
            matches!(
                effect,
                super::ResolvedEffect::ApplyCondition { .. }
                    | super::ResolvedEffect::ApplyPersistentCondition { .. }
                    | super::ResolvedEffect::RemoveCondition { .. }
                    | super::ResolvedEffect::HasteDose { .. }
            )
        }) {
            EffectCue::Status
        } else {
            EffectCue::Neutral
        };
        let origin = self.capture_spatial_effect_origin(outcome.actor)?;
        self.publish_spatial_effect(
            SpatialEffect {
                actor: outcome.actor,
                target: Some(outcome.target),
                ability: ability.clone(),
                source_disclosure,
                tick: self.tick(),
                delivery: EffectDelivery::Target,
                timing: crate::combat::EffectTiming::Instant,
                appearance: crate::combat::EffectAppearance::Flash,
                cue,
                segments: vec![vec![location]],
            },
            origin,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tor_world::{Position, RegionId};
    #[test]
    fn path_breaks_at_physical_joins_instead_of_drawing_an_invented_bridge() {
        let at = |r, x| Location {
            region: RegionId(r),
            position: Position { x, y: 0, z: 0 },
        };
        let mut segments = vec![vec![at(1, 0)]];
        push_step(&mut segments, at(1, 0), at(1, 1), Direction::East, 0);
        push_step(&mut segments, at(1, 1), at(2, 8), Direction::East, 0);
        push_step(&mut segments, at(2, 8), at(2, 9), Direction::East, 0);
        push_step(&mut segments, at(2, 9), at(2, 10), Direction::East, 1);
        assert_eq!(
            segments,
            vec![
                vec![at(1, 0), at(1, 1)],
                vec![at(2, 8), at(2, 9)],
                vec![at(2, 10)]
            ]
        );
    }
}
