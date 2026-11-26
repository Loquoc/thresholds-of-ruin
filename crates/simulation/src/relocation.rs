//! Ordinary effect relocation: bounded live destinations, shared lifecycle and
//! no privileged setup shortcuts. Random selection belongs to the owning RNG.
use crate::abilities::Placement;
use crate::{ActorId, Game, GameError};
use std::{collections::BTreeMap, sync::Arc};
use tor_world::{compose_rotation, Extent, Location, Position, RegionId};

const SCAN_BUDGET: i128 = 65_536;

fn volume(bounds: Extent) -> i128 {
    let (width, depth, height) = bounds.dimensions();
    i128::from(width) * i128::from(depth) * i128::from(height)
}

/// Bounded authored groups for a read scroll, derived from the pinned package.
/// Empty membership at a known region marks an oversize group, not a fallback.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthoredTeleportLevels(Arc<BTreeMap<RegionId, Arc<[RegionId]>>>);

impl AuthoredTeleportLevels {
    /// Groups are disjoint; each row supplies its room's actual storage bounds.
    pub fn new(
        groups: impl IntoIterator<Item = Vec<(RegionId, Extent)>>,
    ) -> Result<Self, GameError> {
        let mut by_region = BTreeMap::new();
        for mut group in groups {
            group.sort_unstable_by_key(|(id, _)| *id);
            let size = group
                .iter()
                .map(|(_, bounds)| volume(*bounds))
                .sum::<i128>();
            let members: Arc<[_]> = if size <= SCAN_BUDGET {
                group.iter().map(|(id, _)| *id).collect()
            } else {
                Arc::from([])
            };
            for (id, _) in group {
                if id.0 == 0 || by_region.insert(id, members.clone()).is_some() {
                    return Err(GameError::InvalidLocation);
                }
            }
        }
        Ok(Self(Arc::new(by_region)))
    }

    fn members(&self, at: RegionId) -> Option<&[RegionId]> {
        self.0.get(&at).map(AsRef::as_ref)
    }
}

impl Game {
    /// Install the package-pinned definition on fresh or restored game state.
    pub fn bind_authored_teleport_levels(&mut self, levels: AuthoredTeleportLevels) {
        self.authored_teleport_levels = levels;
    }

    pub(crate) fn scroll_teleport_regions(&self, actor: ActorId) -> Option<&[RegionId]> {
        self.actors.get(&actor).and_then(|source| {
            self.authored_teleport_levels
                .members(source.location.region)
        })
    }

    pub(crate) fn scroll_teleport_candidates(
        &self,
        actor: ActorId,
    ) -> Result<Vec<Placement>, GameError> {
        self.random_relocation_candidates_in(actor, self.scroll_teleport_regions(actor))
    }

    pub(crate) fn random_relocation_candidates(
        &self,
        actor: ActorId,
    ) -> Result<Vec<Placement>, GameError> {
        self.random_relocation_candidates_in(actor, None)
    }

    fn random_relocation_candidates_in(
        &self,
        actor: ActorId,
        authored: Option<&[RegionId]>,
    ) -> Result<Vec<Placement>, GameError> {
        let source = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        if !source.alive() {
            return Ok(Vec::new());
        }
        let scope = authored
            .is_none()
            .then(|| self.activation_scope_at(source.location.region))
            .flatten();
        let cells = if let Some(scope) = scope {
            // Do not change the generated draw or its directory order based on
            // which parts of the floor are resident.
            if scope
                .members
                .iter()
                .any(|id| self.world.region(*id).is_none())
            {
                return Ok(Vec::new());
            }
            scope.cells.clone()
        } else {
            let members = authored.unwrap_or_else(|| std::slice::from_ref(&source.location.region));
            let mut bounds = Vec::with_capacity(members.len());
            let mut total = 0;
            for id in members {
                let Some(region) = self.world.region(*id) else {
                    return if authored.is_some() {
                        Ok(Vec::new())
                    } else {
                        Err(GameError::InvalidLocation)
                    };
                };
                total += volume(region.bounds);
                if total > SCAN_BUDGET {
                    return Ok(Vec::new());
                }
                bounds.push((*id, region.bounds));
            }
            let mut cells = Vec::with_capacity(total as usize);
            for (region, bounds) in bounds {
                let (width, depth, height) = bounds.dimensions();
                let origin = bounds.origin();
                for z in origin.z..origin.z + height {
                    for y in origin.y..origin.y + depth {
                        for x in origin.x..origin.x + width {
                            cells.push(Location {
                                region,
                                position: Position { x, y, z },
                            });
                        }
                    }
                }
            }
            cells
        };
        let in_scope = |id| match authored {
            Some(members) => members.binary_search(&id).is_ok(),
            None => scope.map_or(id == source.location.region, |scope| {
                scope.members.contains(&id)
            }),
        };
        Ok(cells
            .into_iter()
            .filter(|at| {
                *at != source.location
                    && self
                        .body_cells(*at, source.orientation, &source.body)
                        .is_some_and(|cells| cells.iter().all(|(cell, _)| in_scope(cell.region)))
                    && self.stable_landing(actor, *at, source.orientation, &source.body)
            })
            .map(|location| Placement {
                location,
                orientation: source.orientation,
            })
            .collect())
    }

    /// The adjacent endpoint follows the frozen launch frame through *current*
    /// physical topology. Recipient placement/body remain live; no nearby search.
    pub(crate) fn ray_relocation_destination(
        &self,
        actor: ActorId,
        origin: Placement,
        direction: impl Into<crate::abilities::FlightAim>,
    ) -> Result<Option<Placement>, GameError> {
        let source = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        let mut trajectory = direction.into().trajectory(origin.orientation)?;
        let tor_world::RayVectorStep::Clear { to, rotation } = self
            .world
            .ray_vector_step(origin.location, trajectory.advance())
        else {
            return Ok(None);
        };
        let destination = Placement {
            location: to,
            orientation: compose_rotation(source.orientation, rotation),
        };
        Ok(self
            .relocation_destination_valid(actor, destination)
            .then_some(destination))
    }

    pub(crate) fn relocation_destination_valid(&self, actor: ActorId, to: Placement) -> bool {
        self.actors.get(&actor).is_some_and(|source| {
            source.alive()
                && to.orientation < 24
                && (source.location != to.location || source.orientation != to.orientation)
                && self.stable_landing(actor, to.location, to.orientation, &source.body)
        })
    }

    pub(crate) fn relocate_exact_in_transaction(
        &mut self,
        actor: ActorId,
        destination: Placement,
        preserve_completed_work: bool,
    ) -> Result<bool, GameError> {
        if !self.relocation_destination_valid(actor, destination) {
            return Ok(false);
        }
        let mut outcome = self.settled_actor_outcome(actor)?;
        if !outcome.state.alive() {
            self.apply_outcome(outcome)?;
            return Ok(false);
        }
        if !preserve_completed_work {
            outcome.interrupt(
                crate::lifecycle::Interruption::Discard,
                self.actor_clock(actor),
            )?;
        }
        outcome.state.location = destination.location;
        outcome.state.orientation = destination.orientation;
        outcome.state.motion = crate::MotionState::default();
        outcome.state.visited.insert(destination.location.region);
        outcome.entry_cause = crate::floor_activation::EntryCause::Teleport;
        self.apply_outcome(outcome)?;
        self.sync_actor_lifecycle(actor)?;
        Ok(true)
    }

    pub(crate) fn relocate_random_in_transaction(
        &mut self,
        actor: ActorId,
        rng: &mut u64,
        preserve_completed_work: bool,
    ) -> Result<Option<Placement>, GameError> {
        let candidates = self.random_relocation_candidates(actor)?;
        if candidates.is_empty() {
            return Ok(None);
        }
        let index =
            crate::quantity::QuantityRange::new(1, candidates.len() as u64)?.sample(rng)? - 1;
        let destination = candidates[index as usize];
        // A destination contact can itself draw (or relocate). Publish this
        // draw before entering it, then return the fully evolved stream.
        self.rules_rng = *rng;
        let moved =
            self.relocate_exact_in_transaction(actor, destination, preserve_completed_work)?;
        *rng = self.rules_rng;
        Ok(moved.then_some(destination))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_teleport_bounds_whole_groups_before_requesting_residency() {
        let at = RegionId(1);
        let other = RegionId(2);
        let room = Extent::new(128, 128, 2).unwrap();
        let exact = AuthoredTeleportLevels::new([vec![(other, room), (at, room)]]).unwrap();
        assert_eq!(exact.members(at), Some([at, other].as_slice()));
        assert_eq!(
            exact.members(RegionId(3)),
            None,
            "unassigned room falls back"
        );

        let overflow = AuthoredTeleportLevels::new([vec![
            (at, room),
            (other, room),
            (RegionId(3), Extent::new(1, 1, 1).unwrap()),
        ]])
        .unwrap();
        assert_eq!(overflow.members(at), Some([].as_slice()));
        assert_eq!(overflow.members(other), Some([].as_slice()));
        assert!(
            AuthoredTeleportLevels::new([vec![(at, room)], vec![(at, room)]]).is_err(),
            "one room cannot contribute to multiple authored levels"
        );
    }
}
