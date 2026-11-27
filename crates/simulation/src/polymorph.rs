//! Atomic baleful body changes. Spell delivery and paid selection are separate.
use crate::actor_creatures::CombatState;
use crate::creatures::{BodyPackage, BuildError};
use crate::health_recovery::RecoveryContext;
use crate::lifecycle::ActorOutcome;
use crate::{ActorId, Game, GameError};
use std::collections::BTreeMap;
use std::sync::Arc;
use tor_world::Shared;

/// Only an in-flight physics tick/host continuation owns this bounded queue.
/// It is never serialized and must be empty at a public checkpoint.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BodyReturnFinalization {
    pub(crate) deferring: bool,
    queued: BTreeMap<ActorId, u64>,
    authority: Option<BodyReturnAuthority>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BodyReturnAuthority {
    actor: ActorId,
    location: tor_world::Location,
    orientation: u8,
    clock: u64,
}
impl BodyReturnFinalization {
    pub(crate) fn settled(&self) -> bool {
        !self.deferring && self.queued.is_empty() && self.authority.is_none()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyTransitionResult {
    Changed,
    Unchanged,
    /// A paid application may fail to fit without refunding or rerolling.
    Blocked,
}
impl Game {
    pub fn polymorph_creature(
        &mut self,
        actor: ActorId,
        package: Arc<BodyPackage>,
    ) -> Result<BodyTransitionResult, GameError> {
        self.with_effect_publication(|game| {
            let mut outcome = game.settled_actor_outcome(actor)?;
            let result = game.stage_body_transition(&mut outcome, package)?;
            if result == BodyTransitionResult::Changed {
                let prepared = game.prepare_actor_outcome(outcome)?;
                game.publish_actor_outcome(prepared)?;
            }
            Ok(result)
        })
    }

    /// Only assign the candidate after all geometry, equipment and derivation
    /// checks succeed. A caller may keep its already-paid effect on Blocked.
    pub(crate) fn stage_body_transition(
        &self,
        outcome: &mut ActorOutcome,
        package: Arc<BodyPackage>,
    ) -> Result<BodyTransitionResult, GameError> {
        package.validate(&self.content).map_err(GameError::Build)?;
        if self
            .content
            .body_package(&package.id)
            .is_none_or(|authored| authored.as_ref() != package.as_ref())
        {
            return Err(GameError::Build(BuildError::InvalidDefinition));
        }
        let actor = &outcome.state;
        let combat = actor.combat.as_ref().ok_or(GameError::UnknownActor)?;
        if !actor.alive() {
            return Ok(BodyTransitionResult::Unchanged);
        }
        if combat.creature().build().content().as_ref() != self.content.as_ref() {
            return Err(GameError::Build(BuildError::InvalidDefinition));
        }
        if combat
            .creature()
            .build()
            .active_body_package()
            .is_some_and(|active| active.as_ref() == package.as_ref())
        {
            return Ok(BodyTransitionResult::Unchanged);
        }
        // Cross-package loss redistribution is deliberately unsupported. It is
        // a paid no-change boundary, not a malformed-definition rollback.
        if combat.creature().build().body_package_loss_count().get() > 0 {
            return Ok(BodyTransitionResult::Blocked);
        }
        if !self.body_fits(outcome.id, actor.location, actor.orientation, &package.body) {
            return Ok(BodyTransitionResult::Blocked);
        }
        let replacement = combat
            .creature()
            .build()
            .validated_body_replacement(package.clone())
            .map_err(GameError::Build)?;
        let selection = replacement.selection();
        let anatomy = selection.anatomy.clone();
        let mut equipment = BTreeMap::new();
        // Existing socket identity must still denote the same kind. No implicit
        // equipment remapping, curse checks, transfer, duplication or destruction.
        for (socket, item) in &actor.equipment {
            let index = usize::from(socket.0);
            if actor.anatomy.slots.get(index) != anatomy.slots.get(index)
                || anatomy.slots.get(index).is_none()
            {
                continue;
            }
            equipment.insert(*socket, *item);
            if self
                .equipment_capacity_state_for_facts(
                    outcome.id,
                    &anatomy,
                    &selection.expertises,
                    &equipment,
                )
                .is_none_or(|deficits| !deficits.is_empty())
            {
                equipment.remove(socket);
            }
        }
        let sources = equipment
            .iter()
            .map(|(socket, id)| {
                self.items
                    .get(id)
                    .and_then(|item| item.equipment_source(*id, *socket, &self.content.rules))
                    .ok_or(GameError::ItemUnavailable)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut creature = combat.creature().clone();
        creature
            .stage_body_rebuild_with_equipment(replacement, sources)
            .map_err(GameError::Creature)?;
        // Equipment-dependent expertise cannot silently make a retained kit
        // structurally invalid. Unsupported circular capacity changes reject.
        if self
            .equipment_capacity_state_for_facts(
                outcome.id,
                &anatomy,
                &creature.derived().expertises,
                &equipment,
            )
            .is_none_or(|deficits| !deficits.is_empty())
        {
            return Err(GameError::Build(BuildError::InvalidDefinition));
        }
        let extinguish = actor
            .equipment
            .values()
            .filter(|item| {
                !equipment.values().any(|retained| retained == *item)
                    && self
                        .items
                        .get(*item)
                        .is_some_and(|item| item.fuel.is_some_and(|fuel| fuel.lit))
            })
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let mut candidate = actor.clone();
        candidate.anatomy = Shared::new(anatomy);
        candidate.equipment = equipment;
        if *candidate.body != package.body {
            candidate.readied_ambush.retire();
        }
        candidate.body = Shared::new(package.body.clone());
        candidate.motion.acceleration_remainder = [0; 3];
        // Keep continuing forced-fall attribution; the publisher retires it
        // only when the final geometry/locomotion ends the causal episode.
        candidate.combat = Some(CombatState::from_creature(
            combat.identity.clone(),
            creature.clone(),
        ));
        if !creature.health().dead()
            && creature.derived().contextual_health_recovery()
            && (!self.actor_frozen(outcome.id)
                || self.body_return_continuation_authorized(outcome.id))
        {
            // A read-only candidate scene uses the NEW eyes/body/capabilities,
            // never the published predecessor's perception. No lifecycle is run
            // in this projection and no elapsed time belongs to the new form.
            let mut projection = self.clone();
            projection.actors.insert(outcome.id, candidate.clone());
            for item in outcome.extinguish_lights.iter().chain(extinguish.iter()) {
                projection.set_item_light(*item, false)?;
            }
            let sensed_hostile = projection.senses_hostile(outcome.id)?;
            creature.resolve_body_recovery_context(RecoveryContext::Known { sensed_hostile });
            candidate.combat = Some(CombatState::from_creature(
                combat.identity.clone(),
                creature,
            ));
        }
        outcome.extinguish_lights.extend(extinguish);
        outcome.state = candidate;
        Ok(BodyTransitionResult::Changed)
    }

    /// Suppression plus a legal return has one final Health boundary. Returning
    /// None delegates ordinary/blocked Cancellation to its existing path.
    pub(crate) fn stage_cancellation_return(
        &self,
        outcome: &mut ActorOutcome,
    ) -> Result<Option<bool>, GameError> {
        let Some(combat) = &outcome.state.combat else {
            return Ok(None);
        };
        let Some((base, active)) = combat
            .creature()
            .build()
            .base_body_package()
            .zip(combat.creature().build().active_body_package())
        else {
            return Ok(None);
        };
        if base.id == active.id
            || !outcome.state.alive()
            || !self.body_fits(
                outcome.id,
                outcome.state.location,
                outcome.state.orientation,
                &base.body,
            )
        {
            return Ok(None);
        }
        let base = base.clone();
        let mut creature = combat.creature().clone();
        creature
            .stage_cancellation_inputs()
            .map_err(GameError::Creature)?;
        let mut candidate = outcome.state.clone();
        candidate.combat = Some(CombatState::from_creature(
            combat.identity.clone(),
            creature,
        ));
        let original = std::mem::replace(&mut outcome.state, candidate);
        match self.stage_body_transition(outcome, base) {
            Ok(BodyTransitionResult::Changed) => Ok(Some(true)),
            Ok(_) => {
                outcome.state = original;
                Ok(None)
            }
            Err(error) => {
                outcome.state = original;
                Err(error)
            }
        }
    }

    pub(crate) fn stage_polymorph_return(
        &self,
        outcome: &mut ActorOutcome,
    ) -> Result<bool, GameError> {
        let Some(combat) = &outcome.state.combat else {
            return Ok(false);
        };
        let Some(base) = combat.creature().build().base_body_package().cloned() else {
            return Ok(false);
        };
        if !outcome.state.alive() {
            self.set_return_pending(outcome, false)?;
            return Ok(false);
        }
        let result = self.stage_body_transition(outcome, base)?;
        self.set_return_pending(outcome, result == BodyTransitionResult::Blocked)?;
        Ok(result == BodyTransitionResult::Changed)
    }
    pub(crate) fn set_return_pending(
        &self,
        outcome: &mut ActorOutcome,
        pending: bool,
    ) -> Result<(), GameError> {
        let Some(combat) = &outcome.state.combat else {
            return Ok(());
        };
        if combat.creature().polymorph_return_pending() == pending {
            return Ok(());
        }
        let mut creature = combat.creature().clone();
        creature
            .set_polymorph_return_pending(pending)
            .map_err(GameError::Creature)?;
        outcome.state.combat = Some(CombatState::from_creature(
            combat.identity.clone(),
            creature,
        ));
        Ok(())
    }
    pub(crate) fn body_return_continuation_authorized(&self, id: ActorId) -> bool {
        self.body_return_finalization
            .authority
            .is_some_and(|authority| {
                authority.actor == id
                    && authority.clock == self.tick
                    && self.actor_clock(id) == authority.clock
                    && self.actors.get(&id).is_some_and(|actor| {
                        actor.location == authority.location
                            && actor.orientation == authority.orientation
                    })
            })
    }

    /// Complete only the captured active actor's in-flight return. Frozen clock
    /// stamps and timer membership are untouched; no neighboring actor receives
    /// this authority. Ordinary lifecycle synchronization runs after the scope.
    pub(crate) fn with_body_return_authority<T>(
        &mut self,
        id: ActorId,
        clock: u64,
        returned_contact: bool,
        change: impl FnOnce(&mut Self) -> Result<T, GameError>,
    ) -> Result<T, GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        if clock != self.tick
            || self.actor_clock(id) != clock
            || !actor.alive()
            || !actor.combat.as_ref().is_some_and(|combat| {
                let creature = combat.creature();
                if returned_contact {
                    !creature.polymorph_return_pending()
                        && creature
                            .build()
                            .base_body_package()
                            .zip(creature.build().active_body_package())
                            .is_some_and(|(base, active)| base.id == active.id)
                } else {
                    creature.polymorph_return_pending()
                }
            })
        {
            return Err(GameError::InvalidLocation);
        }
        let authority = BodyReturnAuthority {
            actor: id,
            location: actor.location,
            orientation: actor.orientation,
            clock,
        };
        let previous = self.body_return_finalization.authority.replace(authority);
        let result = change(self);
        self.body_return_finalization.authority = previous;
        let value = result?;
        self.sync_actor_lifecycle(id)?;
        Ok(value)
    }

    /// A nested trap may commit a real relocation of the same continuation
    /// owner. Follow that committed anchor only; unrelated actors and stale
    /// prior placements cannot inherit the authority.
    pub(crate) fn relocate_body_return_authority(
        &mut self,
        id: ActorId,
        from: tor_world::Location,
        frame: u8,
    ) {
        let Some(authority) = self.body_return_finalization.authority.as_mut() else {
            return;
        };
        if authority.actor == id
            && authority.location == from
            && authority.orientation == frame
            && authority.clock == self.tick
        {
            if let Some(actor) = self.actors.get(&id) {
                authority.location = actor.location;
                authority.orientation = actor.orientation;
            }
        }
    }

    pub(crate) fn with_deferred_body_returns<T>(
        &mut self,
        change: impl FnOnce(&mut Self) -> Result<T, GameError>,
    ) -> Result<T, GameError> {
        if self.body_return_finalization.deferring {
            return change(self);
        }
        self.body_return_finalization.deferring = true;
        let result = change(self);
        self.body_return_finalization.deferring = false;
        match result {
            Ok(value) => {
                if let Err(error) = self.flush_pending_body_returns() {
                    self.body_return_finalization.queued.clear();
                    return Err(error);
                }
                Ok(value)
            }
            Err(error) => {
                self.body_return_finalization.queued.clear();
                Err(error)
            }
        }
    }

    pub(crate) fn flush_pending_body_returns(&mut self) -> Result<(), GameError> {
        if self.body_return_finalization.deferring {
            return Ok(());
        }
        while !self.trap_contacts_unsettled() {
            let Some((id, clock)) = self.body_return_finalization.queued.pop_first() else {
                break;
            };
            if self.actors.get(&id).is_some_and(|actor| {
                actor.alive()
                    && actor
                        .combat
                        .as_ref()
                        .is_some_and(|combat| combat.creature().polymorph_return_pending())
            }) {
                self.with_body_return_authority(id, clock, false, |game| {
                    game.finish_pending_body_return(id)
                })?;
            }
        }
        Ok(())
    }

    /// The old form's placement, held state, guise and complete trap chain
    /// settle first. Residency only delays this final return, never replays
    /// the selected movement or advances another actor while it is pending.
    pub(crate) fn finish_pending_body_return(&mut self, id: ActorId) -> Result<(), GameError> {
        if (self.actor_frozen(id) && !self.body_return_continuation_authorized(id))
            || !self.actors.get(&id).is_some_and(|actor| {
                actor.alive()
                    && actor
                        .combat
                        .as_ref()
                        .is_some_and(|combat| combat.creature().polymorph_return_pending())
            })
        {
            return Ok(());
        }
        if self.body_return_finalization.deferring {
            let clock = self.actor_clock(id);
            self.body_return_finalization
                .queued
                .entry(id)
                .or_insert(clock);
            return Ok(());
        }
        if self.trap_contacts_unsettled() {
            return Ok(());
        }
        let members = self.pending_body_return_at_current_regions(id);
        if members
            .iter()
            .any(|region| self.world.region(*region).is_none())
        {
            if !self.host_region_continuation {
                return Err(GameError::InvalidLocation);
            }
            let actor = &self.actors[&id];
            self.trap_region_boundary = Some(crate::traps::TrapRegionBoundary {
                phase: crate::traps::TrapRegionPhase::PostContactBodyReturn,
                active_clock: Some(self.actor_clock(id)),
                actor: id,
                location: actor.location,
                orientation: actor.orientation,
                avoidable_traps: Default::default(),
                imposed_normal: None,
                members,
            });
            return Ok(());
        }
        let mut outcome = self.settled_actor_outcome(id)?;
        if !outcome.state.alive() {
            self.set_return_pending(&mut outcome, false)?;
            return self.apply_outcome(outcome);
        }
        if self.stage_polymorph_return(&mut outcome)? {
            let prepared = self.prepare_actor_outcome(outcome)?;
            self.publish_actor_outcome(prepared)?;
        }
        Ok(())
    }
}

// Residency is a pre-payment dependency, not a fit verdict. Resolve the same
// bounded cardinal offset paths as rigid bodies, recording a known destination
// before a missing slice stops that path. The host's load-only continuation
// repeats this probe; it never selects a form or activates its population.
fn body_footprint_regions(
    world: &tor_world::World,
    at: tor_world::Location,
    frame: u8,
    body: &crate::BodySpec,
) -> std::collections::BTreeSet<tor_world::RegionId> {
    type Cell = (tor_world::Location, u8);
    fn resolve(
        world: &tor_world::World,
        offset: [i32; 3],
        cache: &mut BTreeMap<[i32; 3], Option<Cell>>,
        regions: &mut std::collections::BTreeSet<tor_world::RegionId>,
    ) -> Option<Cell> {
        if let Some(cell) = cache.get(&offset) {
            return *cell;
        }
        let mut result = None;
        for axis in 0..3 {
            if offset[axis] == 0 {
                continue;
            }
            let mut previous = offset;
            previous[axis] -= offset[axis].signum();
            let Some((from, frame)) = resolve(world, previous, cache, regions) else {
                continue;
            };
            let mut delta = [0; 3];
            delta[axis] = i64::from(offset[axis].signum());
            let Some(direction) =
                tor_world::Direction::from_delta(tor_world::rotate_vector(frame, delta))
            else {
                continue;
            };
            let Some((to, turn)) = world.physics_neighbor(from, direction) else {
                continue;
            };
            if world.known_region(to.region).is_some() {
                regions.insert(to.region);
            }
            if world.contains(to) {
                // Conflicting paths cannot fit; retaining either path here is
                // sufficient for residency and leaves legality to resolve_body.
                result.get_or_insert((to, tor_world::compose_rotation(frame, turn)));
            }
        }
        cache.insert(offset, result);
        result
    }
    let mut regions = std::collections::BTreeSet::from([at.region]);
    let mut cache = BTreeMap::from([([0; 3], Some((at, frame)))]);
    for offset in &body.cells {
        resolve(world, *offset, &mut cache, &mut regions);
    }
    regions
}

impl Game {
    pub(crate) fn pending_body_return_at_current_regions(
        &self,
        id: ActorId,
    ) -> std::collections::BTreeSet<tor_world::RegionId> {
        let Some(actor) = self.actors.get(&id) else {
            return Default::default();
        };
        let Some(creature) = actor
            .combat
            .as_ref()
            .map(|combat| combat.creature())
            .filter(|creature| creature.polymorph_return_pending())
        else {
            return Default::default();
        };
        creature
            .build()
            .base_body_package()
            .map_or_else(Default::default, |base| {
                body_footprint_regions(&self.world, actor.location, actor.orientation, &base.body)
            })
    }

    pub(crate) fn pending_body_return_regions(
        &self,
        id: ActorId,
    ) -> std::collections::BTreeSet<tor_world::RegionId> {
        let mut regions = std::collections::BTreeSet::new();
        let Some(actor) = self.actors.get(&id) else {
            return regions;
        };
        let Some(creature) = actor
            .combat
            .as_ref()
            .map(|combat| combat.creature())
            .filter(|creature| creature.polymorph_return_pending())
        else {
            return regions;
        };
        let Some(base) = creature.build().base_body_package() else {
            return regions;
        };
        let mut placements =
            std::collections::BTreeSet::from([(actor.location, actor.orientation)]);
        // Ordinary immediate movement can be admitted without a preparation.
        // Probe its finite neighboring placements, never neighboring populations.
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    let Some(direction) = tor_world::Direction::from_delta([x, y, z]) else {
                        continue;
                    };
                    if let Some(context) = self.movement_context(id, direction) {
                        placements
                            .insert((context.destination(), context.destination_orientation()));
                    }
                }
            }
        }
        for intention in self
            .queued_intentions()
            .filter(|intention| intention.actor == id)
        {
            if let Some(context) = intention.movement_context {
                placements.insert((context.destination(), context.destination_orientation()));
            }
        }
        if let Some(step) = actor
            .pending
            .as_ref()
            .and_then(|preparation| preparation.plan.as_ability())
            .and_then(|ability| ability.step)
        {
            placements.insert((step.destination(), step.context.destination_orientation()));
        }
        for (at, frame) in placements {
            regions.extend(body_footprint_regions(&self.world, at, frame, &base.body));
        }
        regions
    }

    pub(crate) fn body_effect_required_regions(
        &self,
        target: ActorId,
        effects: &[crate::abilities::PlannedEffect],
    ) -> Result<std::collections::BTreeSet<tor_world::RegionId>, GameError> {
        let mut regions = std::collections::BTreeSet::new();
        if !effects.iter().any(|effect| {
            matches!(
                effect,
                crate::abilities::PlannedEffect::BalefulPolymorph { .. }
                    | crate::abilities::PlannedEffect::CancelSpecials
            )
        }) {
            return Ok(regions);
        }
        let Some(actor) = self.actors.get(&target) else {
            return Ok(regions);
        };
        let Some(creature) = actor.combat.as_ref().map(|combat| combat.creature()) else {
            return Ok(regions);
        };
        let Some(active) = creature.build().active_body_package() else {
            return Ok(regions);
        };
        for effect in effects {
            match effect {
                crate::abilities::PlannedEffect::BalefulPolymorph { forms } => {
                    for id in forms.iter().filter(|id| **id != active.id) {
                        let package = self
                            .content
                            .polymorph_body_package(id)
                            .ok_or(GameError::Rule(crate::abilities::RuleError::InvalidPlan))?;
                        regions.extend(body_footprint_regions(
                            &self.world,
                            actor.location,
                            actor.orientation,
                            &package.body,
                        ));
                    }
                }
                crate::abilities::PlannedEffect::CancelSpecials => {
                    if let Some(base) = creature
                        .build()
                        .base_body_package()
                        .filter(|base| base.id != active.id)
                    {
                        regions.extend(body_footprint_regions(
                            &self.world,
                            actor.location,
                            actor.orientation,
                            &base.body,
                        ));
                    }
                }
                _ => {}
            }
        }
        Ok(regions)
    }
}

#[cfg(test)]
#[path = "polymorph_tests.rs"]
mod tests;

#[cfg(test)]
mod residency_tests {
    use super::body_footprint_regions;
    use std::collections::BTreeSet;
    use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};

    #[test]
    fn off_ray_body_neighbor_is_a_load_only_dependency_before_fit() {
        let mut world = World::new(
            (1..=2)
                .map(|id| Region {
                    id: RegionId(id),
                    name: format!("body dependency {id}"),
                    bounds: Extent::new(3, 3, 1).unwrap(),
                })
                .collect(),
            vec![],
        )
        .unwrap();
        let at = Location {
            region: RegionId(1),
            position: Position { x: 1, y: 2, z: 0 },
        };
        let side = Location {
            region: RegionId(2),
            position: Position { x: 1, y: 0, z: 0 },
        };
        // An eastward contact ray never visits this southward body extension.
        world
            .connect(
                Passage {
                    from: at,
                    direction: Direction::South,
                    to: side,
                },
                0,
            )
            .unwrap();
        let body = crate::BodySpec {
            cells: vec![[0, 0, 0], [0, 1, 0], [0, 2, 0]],
            ..crate::BodySpec::default()
        };
        let expected = BTreeSet::from([RegionId(1), RegionId(2)]);
        assert_eq!(body_footprint_regions(&world, at, 0, &body), expected);
        let loaded_fit = crate::physics::resolve_body(&world, at, 0, &body).unwrap();
        assert_eq!(loaded_fit[1].0, side);
        let slice = world.detach_region(RegionId(2)).unwrap();
        let before = world.clone();
        assert!(crate::physics::resolve_body(&world, at, 0, &body).is_none());
        assert_eq!(body_footprint_regions(&world, at, 0, &body), expected);
        assert_eq!(
            world, before,
            "dependency probing does not load or activate geometry"
        );
        world.attach_region(slice).unwrap();
        assert_eq!(
            crate::physics::resolve_body(&world, at, 0, &body).unwrap(),
            loaded_fit
        );
        assert_eq!(body_footprint_regions(&world, at, 0, &body), expected);
    }
}
