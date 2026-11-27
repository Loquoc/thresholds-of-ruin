//! Active-floor population, separate from once-only entry stock. Queries never
//! draw randomness; clocks and births are committed with the enclosing action.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WanderingPolicy {
    /// Actor stage shared by the enabled profiles; its depth bands and growth
    /// remain the sole source of monster selection and normal kill rewards.
    pub stage_id: String,
    #[serde(deserialize_with = "read_exclusions")]
    pub excluded_wandering_content: BTreeSet<String>,
    pub initial_delay: [u64; 2],
    pub retry_ticks: u64,
    pub chance: CarryChance,
    pub living_cap: u16,
}
impl<'de> Deserialize<'de> for WanderingPolicy {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            stage_id: String,
            #[serde(deserialize_with = "read_exclusions")]
            excluded_wandering_content: BTreeSet<String>,
            initial_delay: [u64; 2],
            retry_ticks: u64,
            chance: CarryChance,
            living_cap: u16,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let policy = Self {
            stage_id: f.stage_id,
            excluded_wandering_content: f.excluded_wandering_content,
            initial_delay: f.initial_delay,
            retry_ticks: f.retry_ticks,
            chance: f.chance,
            living_cap: f.living_cap,
        };
        if !policy.valid() {
            return Err(serde::de::Error::custom("invalid wandering policy"));
        }
        Ok(policy)
    }
}
impl WanderingPolicy {
    pub fn valid(&self) -> bool {
        label(&self.stage_id)
            && self.excluded_wandering_content.len() <= MAX_CHOICES
            && self.excluded_wandering_content.iter().all(|id| label(id))
            && self.initial_delay[0] > 0
            && self.initial_delay[0] <= self.initial_delay[1]
            && self.initial_delay[1] <= u64::from(u32::MAX)
            && self.retry_ticks > 0
            && self.retry_ticks <= u64::from(u32::MAX)
            && self.chance.valid()
            && self.living_cap > 0
            && self.living_cap <= MAX_ATTEMPTS
    }
    fn delay(&self, rng: &mut u64) -> u64 {
        self.initial_delay[0]
            + index(
                rng,
                (self.initial_delay[1] - self.initial_delay[0] + 1) as usize,
            ) as u64
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct FloorClock {
    remaining: u64,
    settled_at: u64,
    active: bool,
}
impl<'de> Deserialize<'de> for FloorClock {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            remaining: u64,
            settled_at: u64,
            active: bool,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            remaining: f.remaining,
            settled_at: f.settled_at,
            active: f.active,
        })
    }
}
impl FloorClock {
    fn settle(&mut self, tick: u64) {
        if self.active {
            self.remaining = self
                .remaining
                .saturating_sub(tick.saturating_sub(self.settled_at));
        }
        self.settled_at = tick;
    }
    fn deadline(&self) -> Option<u64> {
        self.active
            .then(|| self.settled_at.checked_add(self.remaining))
            .flatten()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BirthOrigin {
    Wandering,
    Created,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct LivingBirth {
    scope: String,
    origin: BirthOrigin,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WanderingPopulation {
    policy: WanderingPolicy,
    #[serde(deserialize_with = "read_clocks")]
    clocks: Shared<BTreeMap<String, FloorClock>>,
    #[serde(deserialize_with = "read_births")]
    births: Shared<BTreeMap<ActorId, LivingBirth>>,
}
impl<'de> Deserialize<'de> for LivingBirth {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            scope: String,
            origin: BirthOrigin,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            scope: f.scope,
            origin: f.origin,
        })
    }
}
impl<'de> Deserialize<'de> for WanderingPopulation {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            policy: WanderingPolicy,
            #[serde(deserialize_with = "read_clocks")]
            clocks: Shared<BTreeMap<String, FloorClock>>,
            #[serde(deserialize_with = "read_births")]
            births: Shared<BTreeMap<ActorId, LivingBirth>>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            policy: f.policy,
            clocks: f.clocks,
            births: f.births,
        })
    }
}
fn read_exclusions<'de, D: serde::Deserializer<'de>>(d: D) -> Result<BTreeSet<String>, D::Error> {
    let values = crate::bounded::Bounded::<String, MAX_CHOICES>::deserialize(d)?.0;
    let count = values.len();
    let set: BTreeSet<_> = values.into_iter().collect();
    if set.len() != count {
        return Err(serde::de::Error::custom("duplicate wandering exclusion"));
    }
    Ok(set)
}
fn read_clocks<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Shared<BTreeMap<String, FloorClock>>, D::Error> {
    bounded_map::<_, _, _, MAX_SCOPES>(d).map(Shared::new)
}
fn read_births<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Shared<BTreeMap<ActorId, LivingBirth>>, D::Error> {
    bounded_map::<_, _, _, MAX_RETAINED_DECISIONS>(d).map(Shared::new)
}

impl Game {
    pub(crate) fn same_population_scope(&self, a: Location, b: Location) -> bool {
        match self.activation_scope_at(a.region) {
            Some(scope) => scope.members.contains(&b.region),
            None => a.region == b.region,
        }
    }

    pub fn configure_wandering_population(
        &mut self,
        policy: WanderingPolicy,
    ) -> Result<(), GameError> {
        let state = self.floor_activation.as_ref().ok_or_else(invalid)?;
        if self.tick != 0
            || state.wandering.is_some()
            || !state.floors.is_empty()
            || !policy.valid()
            || state.plan.profiles.values().any(|profile| {
                !profile.stages.iter().any(|stage| {
                    stage.id == policy.stage_id
                        && matches!(stage.population, PopulationContent::Actors { .. })
                })
            })
        {
            return Err(invalid());
        }
        self.floor_activation.as_mut().unwrap().wandering = Some(WanderingPopulation {
            policy,
            clocks: Shared::default(),
            births: Shared::default(),
        });
        Ok(())
    }

    /// Only periodic wanderers have a standing interest in the protagonist.
    /// The birth record outlives their disposable, anonymous starting lead.
    pub(crate) fn wandering_pursuit_target(&self, actor: ActorId) -> Option<ActorId> {
        let floor = self.floor_activation.as_ref()?;
        let birth = floor.wandering.as_ref()?.births.get(&actor)?;
        (birth.origin == BirthOrigin::Wandering).then_some(floor.plan.protagonist)
    }

    pub fn wandering_policy(&self) -> Option<&WanderingPolicy> {
        self.floor_activation
            .as_ref()?
            .wandering
            .as_ref()
            .map(|state| &state.policy)
    }

    /// Called after activation and after residency changes. The caller settles
    /// clocks BEFORE changing residency, so leaving a floor never loses elapsed
    /// active time and entering one never charges inactive global time.
    pub(crate) fn reconcile_wandering_activity(&mut self) -> Result<(), GameError> {
        let Some(state) = self.floor_activation.as_ref() else {
            return Ok(());
        };
        let Some(wandering) = state.wandering.as_ref() else {
            return Ok(());
        };
        let policy = wandering.policy.clone();
        let activity: Vec<_> = state
            .floors
            .keys()
            .map(|key| {
                let scope = &state.scopes[key];
                let active = scope.members.iter().any(|region| {
                    self.world.region(*region).is_some() && !self.region_frozen(*region)
                });
                (key.clone(), active)
            })
            .collect();
        let mut rng = self.rules_rng;
        let wandering = self
            .floor_activation
            .as_mut()
            .unwrap()
            .wandering
            .as_mut()
            .unwrap();
        for (key, active) in activity {
            let clock = wandering.clocks.entry(key).or_insert_with(|| FloorClock {
                remaining: policy.delay(&mut rng),
                settled_at: self.tick,
                active,
            });
            // Entry activation can run between a physics tick increment and
            // its final timer pass. Preserve that old-activity fraction too;
            // transition callers already settled it, making this idempotent.
            clock.settle(self.tick);
            clock.active = active;
            if active && clock.deadline().is_none() {
                return Err(GameError::Schedule(crate::ScheduleError::TimeExhausted));
            }
        }
        self.rules_rng = rng;
        Ok(())
    }

    pub(crate) fn next_wandering_event_tick(&self) -> Option<u64> {
        self.floor_activation
            .as_ref()?
            .wandering
            .as_ref()?
            .clocks
            .values()
            .filter_map(FloorClock::deadline)
            .min()
    }

    pub(crate) fn settle_wandering_population(&mut self) -> Result<(), GameError> {
        let Some(state) = self.floor_activation.as_mut() else {
            return Ok(());
        };
        let Some(wandering) = state.wandering.as_mut() else {
            return Ok(());
        };
        // Detached actors cannot die while frozen. Retire loaded corpses
        // before a transition can hide their health in a region record; the
        // retained detached origins therefore still represent living capacity.
        wandering
            .births
            .retain(|id, _| self.actors.get(id).is_none_or(|actor| actor.alive()));
        let policy = wandering.policy.clone();
        let due: Vec<_> = wandering
            .clocks
            .iter_mut()
            .filter_map(|(key, clock)| {
                clock.settle(self.tick);
                (clock.active && clock.remaining == 0).then(|| key.clone())
            })
            .collect();
        for key in due {
            let mut rng = self.rules_rng;
            let succeeds = policy.chance.succeeds(&mut rng);
            self.rules_rng = rng;
            if succeeds {
                self.spawn_living_population(&key, BirthOrigin::Wandering, None)?;
            }
            let mut rng = self.rules_rng;
            let remaining = if succeeds {
                policy.delay(&mut rng)
            } else {
                policy.retry_ticks
            };
            self.tick
                .checked_add(remaining)
                .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
            self.rules_rng = rng;
            self.floor_activation
                .as_mut()
                .unwrap()
                .wandering
                .as_mut()
                .unwrap()
                .clocks
                .get_mut(&key)
                .unwrap()
                .remaining = remaining;
        }
        Ok(())
    }

    /// Called only when the paid readable commits. A blocked neighborhood is a
    /// successful, consumed read with no birth, never a free validation refusal.
    /// This origin bypasses AND does not consume wandering capacity.
    pub(crate) fn create_population_monster(
        &mut self,
        reader: ActorId,
    ) -> Result<Option<ActorId>, GameError> {
        let actor = self.actors.get(&reader).ok_or(GameError::UnknownActor)?;
        let Some(state) = self.floor_activation.as_ref() else {
            return Ok(None);
        };
        let Some(key) = state.members.get(&actor.location.region).cloned() else {
            return Ok(None);
        };
        self.spawn_living_population(&key, BirthOrigin::Created, Some(reader))
    }

    fn spawn_living_population(
        &mut self,
        key: &str,
        origin: BirthOrigin,
        reader: Option<ActorId>,
    ) -> Result<Option<ActorId>, GameError> {
        let state = self.floor_activation.as_ref().ok_or_else(invalid)?;
        let Some(wandering) = state.wandering.as_ref() else {
            return Ok(None);
        };
        let scope = state.scopes.get(key).ok_or_else(invalid)?.clone();
        let plan = state.plan.clone();
        let profile = &plan.profiles[&scope.profile];
        let stage = profile
            .stages
            .iter()
            .find(|stage| stage.id == wandering.policy.stage_id)
            .ok_or_else(invalid)?;
        let PopulationContent::Actors { bands, .. } = &stage.population else {
            return Err(invalid());
        };
        let Some(band) = eligible_band(bands, scope.depth) else {
            return Ok(None);
        };
        if origin == BirthOrigin::Wandering {
            let living = wandering
                .births
                .iter()
                .filter(|(id, birth)| {
                    birth.origin == BirthOrigin::Wandering
                        && self
                            .known_actor_region(**id)
                            .is_some_and(|region| scope.members.contains(&region))
                        && self.actors.get(id).is_none_or(|actor| actor.alive())
                })
                .count();
            if living >= usize::from(wandering.policy.living_cap) {
                return Ok(None);
            }
        }
        if wandering.births.len() >= MAX_RETAINED_DECISIONS {
            return Err(GameError::IdentityExhausted);
        }
        let choices: Vec<_> = band
            .choices
            .iter()
            .filter(|choice| {
                origin != BirthOrigin::Wandering
                    || !wandering
                        .policy
                        .excluded_wandering_content
                        .contains(&choice.content)
            })
            .cloned()
            .collect();
        if choices.is_empty() {
            return Ok(None);
        }
        let mut rng = self.rules_rng;
        let choice = weighted_choice(&mut rng, &choices, |choice| choice.weight);
        let body = choice.actor.body.clone().unwrap_or_default();
        let protagonist = plan.protagonist;
        let Some(pc) = self
            .actors
            .get(&protagonist)
            .map(|actor| actor.location)
            .or_else(|| reader.and_then(|id| self.actors.get(&id).map(|actor| actor.location)))
        else {
            return Ok(None);
        };
        let seen: BTreeSet<_> = if origin == BirthOrigin::Wandering {
            self.scene(protagonist)?
                .into_iter()
                .map(|cell| cell.location)
                .collect()
        } else {
            BTreeSet::new()
        };
        let room = scope.rooms.iter().find(|room| room.cells.contains(&pc));
        let mut cells = if let Some(reader) = reader {
            let at = self.actors[&reader].location;
            tor_world::Direction::HORIZONTAL
                .into_iter()
                .filter_map(|direction| match self.world.ray_step(at, direction) {
                    tor_world::RayStep::Clear { to, .. } => Some(to),
                    _ => None,
                })
                .collect::<Vec<_>>()
        } else {
            scope.cells.clone()
        };
        cells.sort();
        cells.dedup();
        for i in (1..cells.len()).rev() {
            let j = index(&mut rng, i + 1);
            cells.swap(i, j);
        }
        let static_eligible = |at: Location| {
            scope.members.contains(&at.region)
                && !scope.protected.contains(&at)
                && !seen.contains(&at)
                && (origin != BirthOrigin::Wandering
                    || room.is_none_or(|room| !room.cells.contains(&at)))
        };
        let loaded_eligible = |at: Location| {
            static_eligible(at)
                && self.world.walkable(at)
                && !self
                    .actors
                    .at(&self.world, at)
                    .keys()
                    .any(|id| self.actors[id].alive())
        };
        let mut at = None;
        for candidate in cells {
            if scope.cells.binary_search(&candidate).is_err() || !static_eligible(candidate) {
                continue;
            }
            // Keep the full authored-floor draw. Unknown geometry is a bounded
            // miss, not a reason to preload remote rooms or favor resident cells.
            // Create's cells already came only from resolved adjacent ray steps.
            if self.world.region(candidate.region).is_none() {
                break;
            }
            if loaded_eligible(candidate)
                && self.body_fits(ActorId(0), candidate, 0, &body)
                && self
                    .body_cells(candidate, 0, &body)
                    .is_some_and(|cells| cells.iter().all(|(cell, _)| loaded_eligible(*cell)))
            {
                at = Some(candidate);
                break;
            }
        }
        self.rules_rng = rng;
        let Some(at) = at else {
            return Ok(None);
        };
        let mut rng = self.rules_rng;
        let seed = crate::dice::random_word(&mut rng);
        let build = choice
            .actor
            .instantiate_build(
                self.content.clone(),
                seed,
                choice.depth_growth,
                scope.depth,
                &choice.advancement,
            )
            .map_err(GameError::from)?;
        // Publish the build-seed draw before construction: configuring a
        // creature/AI may commit awareness draws of its own. Never replace
        // that advanced stream with this earlier local snapshot afterwards.
        self.rules_rng = rng;
        let (actor, _) = self.spawn_prepared_actor_in_candidate(
            &choice.actor,
            at,
            NonZeroU64::new(choice.turn_ticks).ok_or_else(invalid)?,
            build,
            None,
        )?;
        self.floor_activation
            .as_mut()
            .unwrap()
            .wandering
            .as_mut()
            .unwrap()
            .births
            .insert(
                actor,
                LivingBirth {
                    scope: key.to_owned(),
                    origin,
                },
            );
        if origin == BirthOrigin::Wandering {
            // One finite spawn-time location lead, never a target identity or
            // a pointer that follows the protagonist's later movement.
            self.install_location_alert(actor, pc)?;
        }
        let scene = self.scene(actor)?;
        self.refresh_navigation_scene(actor, &scene);
        Ok(Some(actor))
    }

    pub(super) fn wandering_state_valid(&self) -> bool {
        let Some(state) = self.floor_activation.as_ref() else {
            return true;
        };
        let Some(wandering) = state.wandering.as_ref() else {
            return true;
        };
        let initial: BTreeSet<_> = state
            .floors
            .values()
            .flat_map(|floor| floor.decisions.iter())
            .flat_map(|decision| decision.result.attempts())
            .filter_map(|attempt| match attempt {
                PlacementDecision::Placed {
                    entity: PopulationEntity::Actor { id, .. },
                    ..
                } => Some(*id),
                _ => None,
            })
            .collect();
        wandering.policy.valid()
            && wandering.clocks.len() == state.floors.len()
            && wandering.births.len() <= MAX_RETAINED_DECISIONS
            && state.plan.profiles.values().all(|profile| {
                profile.stages.iter().any(|stage| {
                    stage.id == wandering.policy.stage_id
                        && matches!(stage.population, PopulationContent::Actors { .. })
                })
            })
            && wandering.clocks.iter().all(|(key, clock)| {
                state.floors.contains_key(key)
                    && clock.settled_at <= self.tick
                    && clock.remaining > 0
                    && clock.remaining
                        <= wandering.policy.initial_delay[1].max(wandering.policy.retry_ticks)
                    && (!clock.active
                        || clock
                            .deadline()
                            .is_some_and(|deadline| deadline >= self.tick))
                    && state.scopes.get(key).is_some_and(|scope| {
                        clock.active
                            == scope.members.iter().any(|region| {
                                self.world.region(*region).is_some() && !self.region_frozen(*region)
                            })
                    })
            })
            && wandering.births.iter().all(|(id, birth)| {
                id.0 > 0
                    && id.0 < self.next_actor_id
                    && state.floors.contains_key(&birth.scope)
                    && !initial.contains(id)
            })
    }
}

#[cfg(test)]
#[path = "wandering_population_tests.rs"]
mod tests;
