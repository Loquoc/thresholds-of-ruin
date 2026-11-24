//! Encounter bookkeeping and engagement proposals at explicit coordinator stages.
//! Inputs borrow the decision's one snapshot. Only copied Ai/Behavior drafts may
//! change here; actions are proposals, never executed work or persistent caches.
use super::decision_stages::GoldChoiceCache;
use super::{low_health, Ai, AiChoice, State, TRANSITIONS};
use crate::behavior::{Behavior, BehaviorPolicy};
use crate::observation::DecisionSnapshot;
use crate::{Action, ActorId, Game, TravelStep};
use tor_world::Location;

pub(super) struct EncounterInputs<'a> {
    pub actor: ActorId,
    pub policy: BehaviorPolicy,
    pub has_retained: bool,
    pub snapshot: &'a DecisionSnapshot,
}

/// Facts valid only during this immutable decision. A sensed target permits
/// defense; remembered engagement or an anonymous cue alone never does.
pub(super) struct EncounterFacts {
    pub sensed_target: Option<ActorId>,
    pub threat: Option<Location>,
    pub alert: Option<Location>,
    pub health: (u64, u64),
}

impl Game {
    /// Candidate routing precedes the retained/voluntary refresh gate exactly as
    /// in the coordinator. Gold retains its decision-local lazy negative cache.
    pub(super) fn refresh_ai_encounter(
        &self,
        inputs: EncounterInputs<'_>,
        ai: &mut Ai,
        behavior: &mut Behavior,
        route: &mut impl FnMut(Location) -> Option<Vec<TravelStep>>,
        gold_choice: &mut GoldChoiceCache,
    ) -> Option<EncounterFacts> {
        let EncounterInputs {
            actor: id,
            policy,
            has_retained,
            snapshot,
        } = inputs;
        let actor = self.actors.get(&id)?;
        let view = &snapshot.view;
        let eligible = |other: &&crate::observation::ActorView| {
            other.id != id
                && self.awareness_allows_target(id, other.id)
                && self.perceived_hostile(id, other.id, other.disclosure)
        };
        let refresh = !has_retained && self.condition_allows_voluntary_input(id);
        // A periodic wanderer acquires the protagonist on sight, independent
        // of its species disposition or its anonymous spawn-time lead.
        let wandering_target = if refresh && policy.memory_ticks > 0 {
            self.wandering_pursuit_target(id)
                .filter(|target| {
                    behavior
                        .engagement
                        .is_none_or(|entry| entry.target != *target)
                })
                .and_then(|target| {
                    view.visible_actors.iter().find(|other| {
                        other.id == target
                            && eligible(other)
                            && self.actor_visible_in_sight(id, target, &snapshot.sight)
                    })
                })
        } else {
            None
        };
        let candidate = wandering_target.or_else(|| {
            view.visible_actors
                .iter()
                .filter(eligible)
                .min_by_key(|other| {
                    (
                        // Rooted creatures cannot rank foes by pursuit distance:
                        // every off-cell route is unavailable. Prefer disclosed
                        // contact before the existing route/identity tie-breaks.
                        !has_retained
                            && actor.locomotion().stationary
                            && !self.attack_available_in_scene(id, other.id, &snapshot.sight),
                        route(other.location).map_or(usize::MAX, |r| r.len()),
                        other.id,
                    )
                })
        });
        // Retained work resumes its frozen operands; it does not acquire a new
        // enemy, refresh memory or reroll a sensory cue.
        if refresh {
            if let Some(engagement) = behavior.engagement.as_mut() {
                if let Some(visible) = view.visible_actors.iter().find(|other| {
                    other.id == engagement.target
                        && self.awareness_allows_target(id, other.id)
                        && other.disclosure.reference() == Some(engagement.reference)
                }) {
                    engagement.location = visible.location;
                    engagement.last_seen = self.actor_clock(id);
                } else if view.location == engagement.location {
                    behavior.engagement = None;
                }
            }
            // A greedy species pursues known Gold it can safely collect and
            // acquires a visible hostile only when there is none.
            let acquires = wandering_target.is_some()
                || match policy.disposition {
                    crate::behavior::Disposition::Passive => false,
                    crate::behavior::Disposition::Mean => true,
                    crate::behavior::Disposition::Greedy => {
                        self.exposed_to_visible_hostile(id, view.location, view)
                            || !gold_choice.has_action_or_choose(|| {
                                self.choose_known_gold(id, view, &ai.profile, route)
                            })
                    }
                };
            if behavior.engagement.is_none() && policy.memory_ticks > 0 && acquires {
                if let Some(visible) = candidate {
                    behavior.engagement = Some(crate::behavior::Engagement {
                        target: visible.id,
                        reference: visible.disclosure.reference().expect("disclosed foe"),
                        location: visible.location,
                        last_seen: self.actor_clock(id),
                    });
                }
            }
        }
        let target = behavior.engagement.and_then(|entry| {
            view.visible_actors.iter().find(|other| {
                other.id == entry.target
                    && self.awareness_allows_target(id, other.id)
                    && other.disclosure.reference() == Some(entry.reference)
            })
        });
        let forced_flee = self.forced_flee_location(id);
        let alert = actor.awareness.alert_at(self.actor_clock(id));
        let threat = forced_flee
            .or_else(|| {
                behavior.engagement.map(|entry| entry.location).or_else(|| {
                    has_retained
                        .then_some(candidate)
                        .flatten()
                        .map(|other| other.location)
                })
            })
            .or(alert);
        let (hp, max) = self.health(id)?;
        let frightened = forced_flee.is_some()
            || (self.scenario_capabilities().low_health_flight
                && low_health(hp, max, ai.profile.flee_percent));
        ai.state = TRANSITIONS
            .iter()
            .find(|(low, known, _)| *low == frightened && *known == threat.is_some())
            .unwrap()
            .2;
        Some(EncounterFacts {
            sensed_target: target.map(|target| target.id),
            threat,
            alert,
            health: (hp, max),
        })
    }

    /// After care, erratic movement, escape and frozen-prefix fallback: defense,
    /// tactical attack/reposition, then remembered pursuit. Failed pursuit clears
    /// only the drafts before the coordinator considers anonymous investigation.
    pub(super) fn choose_ai_engagement(
        &self,
        id: ActorId,
        ai: &mut Ai,
        behavior: &mut Behavior,
        snapshot: &DecisionSnapshot,
        route: &mut impl FnMut(Location) -> Option<Vec<TravelStep>>,
        sensed_target: Option<ActorId>,
    ) -> Option<AiChoice> {
        let view = &snapshot.view;
        // Only a presently sensed threat warrants proactive self-defense.
        // Healing, an admissible escape and paid strike prefixes retain priority.
        if let Some(threat) = sensed_target {
            if let Some(plan) = self.choose_ai_defense(id, threat, &snapshot.sight, view) {
                return Some(AiChoice::Frozen(plan));
            }
        }
        if let Some(crate::behavior::Engagement {
            target,
            reference,
            location: at,
            ..
        }) = behavior.engagement
        {
            // Remembered positions support pursuit, but only a currently
            // disclosed target can prompt a paid technique.
            if ai.state != State::Flee
                && view.visible_actors.iter().any(|actor| {
                    actor.id == target
                        && self.awareness_allows_target(id, target)
                        && actor.disclosure.reference() == Some(reference)
                })
            {
                if let Some(direction) =
                    self.ai_ranged_reposition(id, target, &snapshot.sight, &snapshot.scene, view)
                {
                    return Some(AiChoice::Request(Action::Move(direction)));
                }
                if let Some(ability) = self.choose_ai_ability(
                    id,
                    target,
                    &snapshot.sight,
                    view,
                    super::AttackSelectionMode::Engage,
                ) {
                    return Some(ability);
                }
            }
            if ai.state == State::Flee
                && view.visible_actors.iter().any(|actor| {
                    actor.id == target
                        && self.awareness_allows_target(id, target)
                        && actor.disclosure.reference() == Some(reference)
                })
            {
                if let Some(ability) = self.choose_ai_ability(
                    id,
                    target,
                    &snapshot.sight,
                    view,
                    super::AttackSelectionMode::CorneredBasic,
                ) {
                    return Some(ability);
                }
            }
            if ai.state != State::Flee {
                if let Some(step) = route(at).and_then(|r| r.first().copied()) {
                    if self.actor_translation(id, step.direction).is_some() {
                        return Some(AiChoice::Request(Action::Move(step.direction)));
                    }
                }
                behavior.engagement = None;
                ai.state = State::Search;
            }
        }
        None
    }
}
