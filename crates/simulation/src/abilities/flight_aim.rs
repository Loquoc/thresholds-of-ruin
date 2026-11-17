//! Frozen flight bearing, independent of a future recipient or collision path.
use super::{AbilityInvocation, ProjectileDeliverySelection, RuleError, StepDirection};
use crate::{ActorId, Game, GameError};
use serde::{Deserialize, Serialize};
use tor_world::{Direction, RayTrajectory};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FlightAim {
    Direction { direction: StepDirection },
    Cell { offset: [i32; 3] },
}
impl<'de> Deserialize<'de> for FlightAim {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Direction { direction: StepDirection },
            Cell { offset: [i32; 3] },
        }
        let value = match crate::bounded::named_map(d)? {
            Fields::Direction { direction } => Self::Direction { direction },
            Fields::Cell { offset } => Self::Cell { offset },
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("zero flight bearing"));
        }
        Ok(value)
    }
}
impl From<StepDirection> for FlightAim {
    fn from(direction: StepDirection) -> Self {
        Self::Direction { direction }
    }
}
impl FlightAim {
    pub fn valid(self) -> bool {
        !matches!(self, Self::Cell { offset: [0, 0, 0] })
    }
    pub fn direction(self) -> Option<StepDirection> {
        match self {
            Self::Direction { direction } => Some(direction),
            Self::Cell { .. } => None,
        }
    }
    pub(crate) fn vector(self) -> [i32; 3] {
        match self {
            Self::Direction { direction } => {
                let (x, y, z) = Direction::from(direction).delta();
                [x, y, z]
            }
            Self::Cell { offset } => offset,
        }
    }
    pub(crate) fn trajectory(self, rotation: u8) -> Result<RayTrajectory, GameError> {
        let mut trajectory =
            RayTrajectory::new(self.vector()).ok_or(GameError::Rule(RuleError::InvalidPlan))?;
        trajectory.rotate(rotation);
        Ok(trajectory)
    }
    pub(crate) fn ray_invocation(self) -> AbilityInvocation {
        match self {
            Self::Direction { direction } => AbilityInvocation::directional_ray(direction),
            Self::Cell { offset } => AbilityInvocation::cell_ray(offset),
        }
    }
    pub(crate) fn projectile_invocation(
        self,
        delivery: ProjectileDeliverySelection,
    ) -> AbilityInvocation {
        match self {
            Self::Direction { direction } => {
                AbilityInvocation::directional_projectile(direction, delivery)
            }
            Self::Cell { offset } => AbilityInvocation::cell_projectile(offset, delivery),
        }
    }
    pub(crate) fn device(self) -> crate::DeviceAim {
        match self {
            Self::Direction { direction } => crate::DeviceAim::Direction { direction },
            Self::Cell { offset } => crate::DeviceAim::Cell { offset },
        }
    }
}
impl Game {
    /// Admits an already disclosed physical occurrence, never a hidden lookup.
    /// Unknown devices pass no range here: their paid flight owns actual range.
    pub(crate) fn validate_flight_aim(
        &self,
        actor: ActorId,
        aim: FlightAim,
        range: Option<u32>,
    ) -> Result<(), GameError> {
        let FlightAim::Cell { offset } = aim else {
            return Ok(());
        };
        let refused = || GameError::Rule(RuleError::OutOfReach);
        if !aim.valid() || range.is_some_and(|r| offset.iter().any(|n| n.unsigned_abs() > r)) {
            return Err(refused());
        }
        let (scene, _) = self.scene_snapshot(actor)?;
        if !scene
            .iter()
            .any(|cell| cell.physical && [cell.offset.x, cell.offset.y, cell.offset.z] == offset)
        {
            return Err(refused());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frozen_cell_bearings_are_strict_and_roundtrip() {
        let aim = FlightAim::Cell { offset: [4, -2, 1] };
        assert_eq!(
            serde_json::from_value::<FlightAim>(serde_json::to_value(aim).unwrap()).unwrap(),
            aim
        );
        for value in [
            serde_json::json!({"kind":"cell","offset":[0,0,0]}),
            serde_json::json!({"kind":"cell","offset":[1,2]}),
            serde_json::json!({"kind":"cell","offset":[1,2,3],"target":99}),
        ] {
            assert!(serde_json::from_value::<FlightAim>(value).is_err());
        }
    }
}
