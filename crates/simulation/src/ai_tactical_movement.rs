//! Tactical movement proposals, separate from encounter/commitment priority.
//! Retreat and ranged spacing have different proofs and failure semantics; they
//! deliberately remain distinct policies rather than a generic movement score.
//! Neither executes work, changes memory, or simulates hypothetical Game clones.
use super::{AiChoice, AttackSelectionMode};
use crate::{Action, ActorId, Game};
use tor_world::{Direction, Location};

/// One decision uses one unit of distance. Remembered hops cannot compete with
/// unwrapped coordinate distance, and unavailable evidence is not a third score.
#[derive(Clone, Copy)]
enum RetreatMetric {
    RememberedRoute { current: u128 },
    LocalChart { current: u128 },
}

impl RetreatMetric {
    fn current(self) -> u128 {
        match self {
            Self::RememberedRoute { current } | Self::LocalChart { current } => current,
        }
    }

    fn uses_remembered_routes(self) -> bool {
        matches!(self, Self::RememberedRoute { .. })
    }
}

fn displacement(from: Location, to: Location) -> [i64; 3] {
    [
        i64::from(to.position.x) - i64::from(from.position.x),
        i64::from(to.position.y) - i64::from(from.position.y),
        i64::from(to.position.z) - i64::from(from.position.z),
    ]
}

fn coordinate_distance(at: Location, threat: Location) -> u128 {
    [
        (at.position.x, threat.position.x),
        (at.position.y, threat.position.y),
        (at.position.z, threat.position.z),
    ]
    .into_iter()
    .map(|(at, threat)| u128::from(i64::from(at).abs_diff(i64::from(threat))))
    .sum::<u128>()
}

/// Borrowed disclosed evidence, not a navigation cache or planner. Constructing
/// this frame must not scan the chart: callers retain the original lazy gates.
struct LocalRetreatFrame<'a> {
    origin: Location,
    orientation: u8,
    scene: &'a [tor_world::SightCell],
}

impl LocalRetreatFrame<'_> {
    fn is_canonical(&self) -> bool {
        self.scene
            .iter()
            .filter(|cell| cell.physical && cell.location.region == self.origin.region)
            .all(|cell| {
                cell.rotation == self.orientation
                    && displacement(self.origin, cell.location)
                        == tor_world::rotate_vector(
                            self.orientation,
                            [
                                i64::from(cell.offset.x),
                                i64::from(cell.offset.y),
                                i64::from(cell.offset.z),
                            ],
                        )
            })
    }

    // Geometry only, not occupancy, support or admission. The caller establishes
    // `from`; only the supplied endpoint is checked here.
    // Plateau steps use the observer's region, unlike direct local scoring's
    // explicit threat-region predicate. Do not silently merge those policies.
    fn ordinary_step(
        &self,
        from: Location,
        to: Location,
        orientation: u8,
        direction: Direction,
    ) -> bool {
        let (x, y, z) = direction.rotated(self.orientation).delta();
        to.region == self.origin.region
            && orientation == self.orientation
            && displacement(from, to) == [i64::from(x), i64::from(y), i64::from(z)]
    }
}

impl Game {
    pub(super) fn ai_retreat_direction(
        &self,
        actor: ActorId,
        threat: Location,
        current_route: Option<usize>,
        view: &crate::Observation,
        scene: &[tor_world::SightCell],
    ) -> Option<Direction> {
        let actor_state = self.actors.get(&actor)?;
        let frame = LocalRetreatFrame {
            origin: view.location,
            orientation: actor_state.orientation,
            scene,
        };
        // Current visibility does not require a retained navigation map. Keep
        // ordinary local retreat available in that case, but select one metric
        // for the whole decision: local coordinates never compete with hops.
        // Same-region portals can fold coordinates too. Use only an unwrapped
        // already-disclosed chart in this region, never inspect hidden links
        // to justify it. Other-region occurrences do not invalidate ordinary
        // local steps; any disclosed fold back into this region still does.
        let metric = match current_route {
            Some(distance) => RetreatMetric::RememberedRoute {
                current: distance as u128,
            },
            None if view.location.region == threat.region && frame.is_canonical() => {
                RetreatMetric::LocalChart {
                    current: coordinate_distance(view.location, threat),
                }
            }
            None => return None,
        };
        let current = metric.current();
        let mut plateau = Vec::new();
        let direct = Direction::HORIZONTAL
            .into_iter()
            .chain([Direction::Up, Direction::Down])
            .filter_map(|direction| {
                let (at, orientation) = self.actor_translation(actor, direction)?;
                if !view
                    .visible_cells
                    .iter()
                    .any(|c| c.location == at && !c.wall)
                {
                    return None;
                }
                let distance = if metric.uses_remembered_routes() {
                    let mut search = self.route_search_from(actor, at, orientation).ok()?;
                    // A directed escape can remove every remembered return path
                    // to this known threat: unbounded distance in this graph.
                    search
                        .route(threat)
                        .map_or(u128::MAX, |route| route.len() as u128)
                } else if at.region == threat.region
                    && orientation == actor_state.orientation
                    && displacement(view.location, at) == {
                        let (x, y, z) = direction.rotated(actor_state.orientation).delta();
                        [i64::from(x), i64::from(y), i64::from(z)]
                    }
                {
                    coordinate_distance(at, threat)
                } else {
                    // Local coordinates cannot rank a portal destination, and
                    // this decision has no known current route to compare.
                    return None;
                };
                if distance == current {
                    plateau.push((direction, at, orientation));
                }
                (distance > current).then_some((distance, direction))
            })
            .max_by_key(|(distance, _)| *distance)
            .map(|(_, direction)| direction);
        if direct.is_some()
            || !metric.uses_remembered_routes()
            || actor_state.body.cells != [[0, 0, 0]]
            || !frame.is_canonical()
        {
            return direct;
        }
        // Anchor routing cannot certify a larger footprint's second-step fit.
        // Only single-cell, two-step local witnesses; no state or assumed future
        // occupancy. Execution rechecks the first step and the next decision
        // re-evaluates the threat. Unknown or exhausted searches are not escape.
        let mut remaining = 512;
        let mut distances = std::collections::BTreeMap::new();
        for (first, at, orientation) in plateau {
            if !frame.ordinary_step(view.location, at, orientation, first) {
                continue;
            }
            let Ok(search) = self.route_search_from(actor, at, orientation) else {
                continue;
            };
            for (second, end, end_orientation) in search.remembered_steps() {
                if !frame.ordinary_step(at, end, end_orientation, second) {
                    continue;
                }
                let distance = *distances.entry(end).or_insert_with(|| {
                    self.route_search_from(actor, end, end_orientation)
                        .ok()?
                        .route_bounded(threat, &mut remaining)
                        .ok()??
                        .len()
                        .try_into()
                        .ok()
                });
                if distance.is_some_and(|distance: u128| distance > current) {
                    return Some(first);
                }
            }
        }
        None
    }

    // Experimental conservative local positioning. Ordinary attack planning
    // proves own funding/cooldowns; the movement policy reads only disclosed cells.
    pub(super) fn ai_ranged_reposition(
        &self,
        actor: ActorId,
        target: ActorId,
        sight: &[tor_world::SightCell],
        scene: &[tor_world::SightCell],
        view: &crate::Observation,
    ) -> Option<Direction> {
        let creature = self.creature(actor)?;
        // Ordinary melee-only actors never pay for a second attack-plan search.
        if !creature.derived().abilities.iter().any(|ability| {
            creature
                .build()
                .ruleset()
                .ability(ability)
                .is_some_and(|definition| {
                    definition.ai.hostile
                        && definition.ai.range_preference
                            == crate::rules::RangePreference::OutsideMelee
                        && definition.invocation == crate::abilities::InvocationKind::ActorTarget
                        && matches!(definition.reach, crate::rules::Reach::Sight { .. })
                })
        }) {
            return None;
        }
        if self.condition_blocks_action(actor, &Action::Move(Direction::North))
            || !self.attack_available_in_scene(actor, target, sight)
        {
            return None;
        }
        let AiChoice::Frozen(plan) = self.choose_ai_ability(
            actor,
            target,
            sight,
            view,
            AttackSelectionMode::RepositionProbe,
        )?
        else {
            return None;
        };
        let crate::Work::UseAbility { id, invocation, .. } = plan.work() else {
            return None;
        };
        if !matches!(
            invocation.aim,
            crate::abilities::AbilityAim::ActorTarget { .. }
        ) {
            return None;
        }
        let definition = creature.build().ruleset().ability(&id)?;
        if definition.ai.range_preference != crate::rules::RangePreference::OutsideMelee {
            return None;
        }
        let crate::rules::Reach::Sight { range } = plan.as_ability()?.reach else {
            return None;
        };
        let origin = self.actors.get(&actor)?;
        // This small policy supports only upright columns. Own morphology is
        // known; unsupported forms keep the ordinary attack policy unchanged.
        if origin.body.eye[0] != 0
            || origin.body.eye[1] != 0
            || origin.body.eye[2] < 0
            || origin
                .body
                .cells
                .iter()
                .any(|cell| cell[0] != 0 || cell[1] != 0 || cell[2] < 0)
            || tor_world::rotate_vector(origin.orientation, [0, 0, 1]) != [0, 0, 1]
            || u32::try_from(origin.body.eye[2]).ok()? > u32::from(range)
        {
            return None;
        }
        // The narrow ground policy understands downward/absent current gravity.
        // It does not inspect undisclosed gravity fields at hypothetical locations.
        if !origin.locomotion().hovers()
            && self
                .world
                .gravity(view.location)
                .is_some_and(|g| g[0] != 0 || g[1] != 0 || g[2] >= 0)
        {
            return None;
        }
        let target = view
            .visible_actors
            .iter()
            .find(|other| other.id == target)?
            .location;
        if target.region != view.location.region || target.position.z != view.location.position.z {
            return None;
        }
        let distance = |a: Location, b: Location| {
            a.position
                .x
                .abs_diff(b.position.x)
                .max(a.position.y.abs_diff(b.position.y))
        };
        // Scene offsets are body-relative; the attack sight above is eye-relative.
        // Do not mix unwrapped coordinates with a disclosed folded chart.
        if scene
            .iter()
            .filter(|cell| cell.physical && cell.location.region == view.location.region)
            .any(|cell| {
                cell.rotation != origin.orientation
                    || [
                        i64::from(cell.location.position.x) - i64::from(view.location.position.x),
                        i64::from(cell.location.position.y) - i64::from(view.location.position.y),
                        i64::from(cell.location.position.z) - i64::from(view.location.position.z),
                    ] != tor_world::rotate_vector(
                        origin.orientation,
                        [
                            i64::from(cell.offset.x),
                            i64::from(cell.offset.y),
                            i64::from(cell.offset.z),
                        ],
                    )
            })
        {
            return None;
        }
        Direction::HORIZONTAL.into_iter().find(|direction| {
            let Some((at, orientation)) = self.actor_translation(actor, *direction) else {
                return false;
            };
            if at.region != target.region
                || at.position.z != target.position.z
                || orientation != origin.orientation
                || distance(at, target) != 2
                || distance(at, target) > u32::from(range)
            {
                return false;
            }
            // An ordinary legal step may fall. Require presently disclosed solid
            // ground, rather than predicting support using hidden destination state.
            if !origin.locomotion().hovers() {
                let Some(z) = at.position.z.checked_sub(1) else {
                    return false;
                };
                let below = Location {
                    region: at.region,
                    position: tor_world::Position { z, ..at.position },
                };
                let support_offset = tor_world::rotate_vector(
                    tor_world::inverse_rotation(origin.orientation),
                    [
                        i64::from(below.position.x) - i64::from(view.location.position.x),
                        i64::from(below.position.y) - i64::from(view.location.position.y),
                        i64::from(below.position.z) - i64::from(view.location.position.z),
                    ],
                );
                // A location disclosed through a different occurrence is not
                // proof of footing at this step. Use only the current chart,
                // never a hidden topology query or hypothetical awareness.
                if !scene.iter().any(|cell| {
                    cell.physical
                        && cell.wall
                        && cell.location == below
                        && cell.rotation == origin.orientation
                        && [
                            i64::from(cell.offset.x),
                            i64::from(cell.offset.y),
                            i64::from(cell.offset.z),
                        ] == support_offset
                }) || !view
                    .visible_cells
                    .iter()
                    .any(|cell| cell.location == below && cell.wall)
                {
                    return false;
                }
            }
            let Some(eye_z) = at.position.z.checked_add(origin.body.eye[2]) else {
                return false;
            };
            // Every destination body cell must already be disclosed. ActorView
            // has one row per disclosed hostile body cell, not just an anchor.
            if origin.body.cells.iter().any(|cell| {
                let Some(z) = at.position.z.checked_add(cell[2]) else {
                    return true;
                };
                !view.visible_cells.iter().any(|visible| {
                    visible.location
                        == Location {
                            region: at.region,
                            position: tor_world::Position { z, ..at.position },
                        }
                        && !visible.wall
                })
            }) {
                return false;
            }
            if view
                .visible_actors
                .iter()
                .filter(|other| self.perceived_hostile(actor, other.id, other.disclosure))
                .any(|other| {
                    other.location.region != at.region || distance(at, other.location) <= 1
                })
            {
                return false;
            }
            // A completely disclosed clear rectangle is deliberately stricter
            // than sight-line prediction; unseen cells cannot justify a move.
            (at.position.x.min(target.position.x)..=at.position.x.max(target.position.x)).all(|x| {
                (at.position.y.min(target.position.y)..=at.position.y.max(target.position.y)).all(
                    |y| {
                        (at.position.z..=eye_z).all(|z| {
                            view.visible_cells.iter().any(|cell| {
                                cell.location
                                    == Location {
                                        region: at.region,
                                        position: tor_world::Position { x, y, z },
                                    }
                                    && !cell.wall
                            })
                        })
                    },
                )
            })
        })
    }
}

#[cfg(test)]
#[path = "ai_tactical_movement_tests.rs"]
mod tests;
