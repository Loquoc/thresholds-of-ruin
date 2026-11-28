//! Coexisting planes in the simulation: each plane's rules, the stays of
//! actors away from their home plane, and shifting between planes.
//!
//! Geometry lives in [`tor_world`]: a region's plane and the overlaps that
//! assert two volumes are the same place. This module adds what acts on that
//! geometry. A shift moves an actor's whole, unscaled body to the counterpart
//! of its reference cell, composing its frame with the overlap's rotation.
//! Carried items follow their carrier. A shift that has no counterpart, or
//! whose body would not fit there, is rejected and nothing moves.
//!
//! A game without planes has no [`PlaneState`] and saves exactly as before.

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};
use tor_world::{compose_rotation, Location, PlaneId};

use crate::abilities::Placement;
use crate::attributes::Skill;
use crate::{ActorId, Game, GameError};

/// Most planes one game may declare, besides the material plane.
pub const MAX_PLANES: usize = 32;
/// Largest check difficulty a plane rule may set.
pub const MAX_PLANE_DC: i32 = 100;

/// The check a plane-granted movement mode needs on every move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlideRule {
    pub skill: Skill,
    pub dc: i32,
}

/// One plane's rules. The material plane has none.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaneSpec {
    /// The package's name for the plane, used by scripts and authoring.
    pub name: String,
    /// A movement mode the plane grants everyone on it who does not fly:
    /// unsupported movement in any direction, after a check on each move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glide: Option<GlideRule>,
}

/// How a stay away from home may end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReturnPolicy {
    /// An ordinary timed return action is available.
    #[default]
    Action,
    /// Only the stay's expiry returns the actor.
    ExpiryOnly,
    /// No return is offered; the stay lasts until another effect ends it.
    None,
}

/// An actor away from its home plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaneStay {
    pub home: PlaneId,
    pub returns: ReturnPolicy,
    /// When the stay ends by itself. The forced return is attempted at the
    /// actor's next action boundary at or after this tick; a blocked return
    /// keeps the stay and is retried at the following boundary.
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub expires_at: Option<u64>,
}

/// A request to move an actor to another plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShiftRequest {
    pub plane: PlaneId,
    pub returns: ReturnPolicy,
    pub duration: Option<NonZeroU64>,
}

impl ShiftRequest {
    /// A shift with the default return action and no expiry.
    pub fn to(plane: PlaneId) -> Self {
        Self {
            plane,
            returns: ReturnPolicy::Action,
            duration: None,
        }
    }
}

/// What a shift did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShiftResult {
    Shifted {
        from: Location,
        to: Location,
    },
    /// No counterpart, or the body would not fit there. Nothing moved.
    Blocked,
}

/// Plane rules and stays. Saved only by games that declare planes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaneState {
    specs: BTreeMap<PlaneId, PlaneSpec>,
    stays: BTreeMap<ActorId, PlaneStay>,
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name != "material"
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-'))
}

impl PlaneState {
    /// Whether the rules are well formed and every stay names a known home.
    pub(crate) fn valid(&self) -> bool {
        let mut names = std::collections::BTreeSet::new();
        self.specs.len() <= MAX_PLANES
            && self.specs.iter().all(|(id, spec)| {
                *id != PlaneId::MATERIAL
                    && valid_name(&spec.name)
                    && names.insert(spec.name.as_str())
                    && spec
                        .glide
                        .is_none_or(|rule| (0..=MAX_PLANE_DC).contains(&rule.dc))
            })
            && self
                .stays
                .values()
                .all(|stay| stay.home == PlaneId::MATERIAL || self.specs.contains_key(&stay.home))
    }
}

impl Game {
    /// Install the package's plane rules. A game that declares no planes
    /// keeps no plane state.
    ///
    /// # Errors
    ///
    /// [`GameError::InvalidLocation`] for more than [`MAX_PLANES`] planes, the
    /// material plane's identity, a duplicate or malformed name, or a
    /// difficulty outside `0..=MAX_PLANE_DC`.
    pub fn install_planes(&mut self, specs: BTreeMap<PlaneId, PlaneSpec>) -> Result<(), GameError> {
        if specs.is_empty() {
            self.planes = None;
            return Ok(());
        }
        let state = PlaneState {
            specs,
            stays: self
                .planes
                .as_ref()
                .map(|state| state.stays.clone())
                .unwrap_or_default(),
        };
        if !state.valid() {
            return Err(GameError::InvalidLocation);
        }
        self.planes = Some(state);
        Ok(())
    }

    /// Put a region on a plane.
    pub fn set_region_plane(
        &mut self,
        region: tor_world::RegionId,
        plane: PlaneId,
    ) -> Result<(), GameError> {
        if plane != PlaneId::MATERIAL && self.plane_spec(plane).is_none() {
            return Err(GameError::InvalidLocation);
        }
        self.world
            .set_region_plane(region, plane)
            .map_err(|_| GameError::InvalidLocation)
    }

    /// Declare an overlap between two regions on different planes.
    pub fn add_overlap(&mut self, overlap: tor_world::Overlap) -> Result<(), GameError> {
        self.world
            .add_overlap(overlap)
            .map_err(|_| GameError::InvalidLocation)
    }

    pub fn plane_spec(&self, plane: PlaneId) -> Option<&PlaneSpec> {
        self.planes.as_ref()?.specs.get(&plane)
    }

    /// The plane a name refers to; `"material"` is the material plane.
    pub fn plane_by_name(&self, name: &str) -> Option<PlaneId> {
        if name == "material" {
            return Some(PlaneId::MATERIAL);
        }
        self.planes
            .as_ref()?
            .specs
            .iter()
            .find_map(|(id, spec)| (spec.name == name).then_some(*id))
    }

    /// The name of a plane; the material plane is `"material"`.
    pub fn plane_name(&self, plane: PlaneId) -> Option<&str> {
        if plane == PlaneId::MATERIAL {
            return Some("material");
        }
        self.plane_spec(plane).map(|spec| spec.name.as_str())
    }

    /// The plane an actor is on now.
    pub fn actor_plane(&self, id: ActorId) -> Option<PlaneId> {
        self.actors
            .get(&id)
            .map(|actor| self.world.plane_of(actor.location.region))
    }

    /// The actor's stay away from home, if it is away.
    pub fn plane_stay(&self, id: ActorId) -> Option<PlaneStay> {
        self.planes.as_ref()?.stays.get(&id).copied()
    }

    /// Where a shift to `plane` would put the actor, if anywhere. The body
    /// keeps its shape; its frame composes with the overlap's rotation.
    pub(crate) fn shift_destination(&self, id: ActorId, plane: PlaneId) -> Option<Placement> {
        let actor = self.actors.get(&id)?;
        if !actor.alive() {
            return None;
        }
        let (location, rotation) = self.world.counterpart(actor.location, plane)?;
        let orientation = compose_rotation(actor.orientation, rotation);
        self.body_fits(id, location, orientation, &actor.body)
            .then_some(Placement {
                location,
                orientation,
            })
    }

    /// Move an actor to another plane inside the caller's transaction.
    /// Rejection changes nothing and notes a `plane_shift_blocked` fact.
    pub(crate) fn shift_actor_in_transaction(
        &mut self,
        id: ActorId,
        request: ShiftRequest,
    ) -> Result<ShiftResult, GameError> {
        crate::diagnostics::plane_shift();
        let from_plane = self.actor_plane(id).ok_or(GameError::UnknownActor)?;
        if request.plane != PlaneId::MATERIAL && self.plane_spec(request.plane).is_none() {
            return Err(GameError::InvalidLocation);
        }
        let from = self.actors[&id].location;
        let Some(destination) = (request.plane != from_plane)
            .then(|| self.shift_destination(id, request.plane))
            .flatten()
        else {
            self.note_plane_shift_blocked(id, from, request.plane);
            return Ok(ShiftResult::Blocked);
        };
        let first = !self.actors[&id]
            .visited
            .contains(&destination.location.region);
        let mut outcome = self.settled_actor_outcome(id)?;
        outcome.interrupt(
            crate::lifecycle::Interruption::Discard,
            self.actor_clock(id),
        )?;
        outcome.state.location = destination.location;
        outcome.state.orientation = destination.orientation;
        outcome.state.motion = crate::MotionState::default();
        outcome.state.visited.insert(destination.location.region);
        outcome.entry_cause = crate::floor_activation::EntryCause::Teleport;
        self.apply_outcome(outcome)?;
        self.sync_actor_lifecycle(id)?;
        self.record_stay(id, from_plane, request);
        let to = destination.location;
        self.note_moved(id, from, to);
        self.note_plane_shifted(id, from, to, from_plane, request.plane);
        if first {
            self.note_first_visit(id, to.region);
        }
        Ok(ShiftResult::Shifted { from, to })
    }

    fn record_stay(&mut self, id: ActorId, from_plane: PlaneId, request: ShiftRequest) {
        let tick = self.tick;
        let Some(state) = self.planes.as_mut() else {
            return;
        };
        let home = state.stays.get(&id).map_or(from_plane, |stay| stay.home);
        if home == request.plane {
            state.stays.remove(&id);
            return;
        }
        state.stays.insert(
            id,
            PlaneStay {
                home,
                returns: request.returns,
                expires_at: request
                    .duration
                    .map(|duration| tick.saturating_add(duration.get())),
            },
        );
    }

    /// Setup and script relocation to another plane, outside any action.
    pub fn shift_actor(
        &mut self,
        id: ActorId,
        request: ShiftRequest,
    ) -> Result<ShiftResult, GameError> {
        if !self.actors.contains_key(&id) {
            return Err(GameError::UnknownActor);
        }
        self.with_recovery_transition(true, |candidate| {
            candidate.shift_actor_in_transaction(id, request)
        })
    }

    /// Where the voluntary return action would put the actor.
    ///
    /// # Errors
    ///
    /// [`GameError::NoPlaneReturn`] when the actor is not away or its stay
    /// offers no return action, and [`GameError::PlaneShiftBlocked`] when its
    /// home counterpart is missing or the body would not fit there.
    pub(crate) fn plane_return_destination(&self, id: ActorId) -> Result<Placement, GameError> {
        let stay = self
            .plane_stay(id)
            .filter(|stay| stay.returns == ReturnPolicy::Action)
            .ok_or(GameError::NoPlaneReturn)?;
        self.shift_destination(id, stay.home)
            .ok_or(GameError::PlaneShiftBlocked)
    }

    /// Perform an admitted return action.
    pub(crate) fn apply_plane_return(&mut self, id: ActorId) -> Result<(), GameError> {
        let stay = self.plane_stay(id).ok_or(GameError::NoPlaneReturn)?;
        match self.shift_actor_in_transaction(id, ShiftRequest::to(stay.home))? {
            ShiftResult::Shifted { .. } => Ok(()),
            ShiftResult::Blocked => Err(GameError::PlaneShiftBlocked),
        }
    }

    /// At the actor's action boundary, end an expired stay. A blocked forced
    /// return keeps the stay; the next boundary tries again.
    pub(crate) fn settle_plane_expiry(&mut self, id: ActorId) -> Result<(), GameError> {
        let Some(stay) = self.plane_stay(id) else {
            return Ok(());
        };
        if stay.expires_at.is_none_or(|due| due > self.tick)
            || !self.actors.get(&id).is_some_and(crate::Actor::alive)
        {
            return Ok(());
        }
        let request = ShiftRequest {
            plane: stay.home,
            returns: stay.returns,
            duration: None,
        };
        self.shift_actor_in_transaction(id, request).map(|_| ())
    }

    /// The plane-granted movement mode an actor would use to move now: only
    /// for actors that do not fly by themselves.
    pub(crate) fn glide_rule(&self, id: ActorId) -> Option<GlideRule> {
        let state = self.planes.as_ref()?;
        let actor = self.actors.get(&id)?;
        if actor.locomotion().flying {
            return None;
        }
        state
            .specs
            .get(&self.world.plane_of(actor.location.region))?
            .glide
    }

    /// Locomotion for ordinary movement admission: a plane-granted movement
    /// mode lets its holder move unsupported, like flight.
    pub(crate) fn movement_locomotion(
        &self,
        id: ActorId,
    ) -> crate::locomotion::LocomotionCapabilities {
        let mut locomotion = self.actors[&id].locomotion();
        if self.glide_rule(id).is_some() {
            locomotion.flying = true;
        }
        locomotion
    }
}

#[cfg(test)]
#[path = "planes_tests.rs"]
mod tests;
