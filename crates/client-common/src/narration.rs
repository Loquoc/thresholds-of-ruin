//! Presentation facts derived exclusively from consecutive disclosed views.
use std::collections::BTreeMap;
use tor_protocol::*;

fn label(name: &str, fallback: &str) -> String {
    let name: String = name.chars().filter(|c| !c.is_control()).collect();
    if name.trim().is_empty() {
        fallback.into()
    } else {
        name
    }
}

/// What a narrated line reports, so a frontend can choose which lines to show
/// and merge related ones. The text stays the same whichever it picks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Topic {
    /// The observer's own action, as its history event records it.
    Own(Event),
    WorkChanged,
    /// Another actor came into sight.
    Noticed(ActorTarget),
    /// Another actor left sight.
    LostSight(ActorTarget),
    /// A door changed state without the observer acting on it.
    Door,
    /// A newly disclosed entrance, trap presence, or trap family.
    Discovery,
    DoorDiscovered,
    TrapDiscovered(Option<TrapFamily>),
    /// Involuntary motion or a collision.
    Motion,
    /// The observer finished timed item work.
    Finished,
    /// A disclosed combat event.
    Combat(CombatEventView),
    /// Another visible actor's item handling, named as the observer sees it.
    Activity(ActorTarget),
    /// A newly paid historical observation, owned by this observer.
    PublicPlan,
    /// Hit points with preparation, recovery, victory or death.
    Status,
    /// Readiness changed: the observer can act again, or must wait.
    Pacing,
    /// Server-authored tutorial, hint or narration text. The line's text names
    /// controls neutrally; a client with key bindings re-renders the segments
    /// with [`user_message_text`].
    UserMessage {
        style: UserMessageStyle,
        segments: Vec<UserMessageSegment>,
    },
}

/// A client's own name for each control it offers, such as `,` or `take`.
/// Messages never carry a key, so the server can't claim a binding.
pub trait ControlNames {
    fn control_name(&self, control: ControlHint) -> String;
}

impl<F: Fn(ControlHint) -> String> ControlNames for F {
    fn control_name(&self, control: ControlHint) -> String {
        self(control)
    }
}

/// Client-neutral wording for a control, used only where no client names it.
pub fn neutral_control_name(control: ControlHint) -> String {
    let words = match control {
        ControlHint::Move => "move",
        ControlHint::Wait => "wait",
        ControlHint::Search => "search",
        ControlHint::PickUp => "pick up",
        ControlHint::Inventory => "inventory",
        ControlHint::Equip => "equip",
        ControlHint::Drink => "drink",
        ControlHint::Read => "read",
        ControlHint::Zap => "zap",
        ControlHint::Throw => "throw",
        ControlHint::Travel => "travel",
        ControlHint::Look => "look",
        ControlHint::OpenDoor => "open door",
        ControlHint::Rest => "rest",
        ControlHint::Save => "save",
        ControlHint::Help => "help",
    };
    format!("[{words}]")
}

/// A server message as reported by clients without keys of their own
/// (headless, bot): neutral `text`, plus raw `segments` whose closed control
/// ids automation maps onto its own inputs.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct UserMessageView {
    pub style: UserMessageStyle,
    pub text: String,
    pub segments: Vec<UserMessageSegment>,
}

/// The server messages among narrated lines, in order.
pub fn user_messages(lines: &[Line]) -> Vec<UserMessageView> {
    lines
        .iter()
        .filter_map(|line| match &line.topic {
            Topic::UserMessage { style, segments } => Some(UserMessageView {
                style: *style,
                text: line.text.clone(),
                segments: segments.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// One message's text with each control named by `names`. Text segments are
/// kept verbatim apart from removing any control characters.
pub fn user_message_text(segments: &[UserMessageSegment], names: &dyn ControlNames) -> String {
    segments
        .iter()
        .map(|segment| match segment {
            UserMessageSegment::Text(text) => text.chars().filter(|c| !c.is_control()).collect(),
            UserMessageSegment::Control(control) => names.control_name(*control),
        })
        .collect()
}

/// One narrated sentence and what it reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub topic: Topic,
    pub text: String,
    /// Optional compact presentation for an observer-owned basic attack.
    /// The ordinary text and underlying event remain unchanged.
    pub basic_attack: Option<BasicAttack>,
}

/// Basic-attack facts derived from the observer's disclosed ability catalog,
/// never from an ability's name or an undisclosed opponent capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BasicAttack {
    Preparing,
    Resolved {
        /// An already-disclosed object phrase, such as "the ruin scout".
        target: String,
        outcome: AbilityOutcome,
    },
}

impl Line {
    fn new(topic: Topic, text: impl Into<String>) -> Self {
        Self {
            topic,
            text: text.into(),
            basic_attack: None,
        }
    }

    #[cfg(test)]
    fn with_basic_attack(mut self, before: &Observation, after: &Observation) -> Self {
        // Prefer a current disclosed capability over an older one. The prior
        // view still covers a strike that removes a grant or ends the run.
        let basic = |ability: &ContentRef| {
            crate::abilities::available(after)
                .iter()
                .chain(crate::abilities::available(before))
                .find(|candidate| candidate.id == ability.id)
                .is_some_and(|candidate| candidate.basic_attack)
        };
        self.basic_attack = match &self.topic {
            Topic::Own(Event::AbilityStarted { ability, .. }) if basic(ability) => {
                Some(BasicAttack::Preparing)
            }
            Topic::Combat(CombatEventView::Ability {
                caster: Some(caster),
                target,
                ability,
                outcome,
                ..
            }) if *caster == after.self_target && basic(ability) => {
                let target = match target {
                    Some(actor) if *actor == after.self_target => "yourself".into(),
                    Some(actor) => format!("the {}", actor_name(*actor, before, after)),
                    None => "something".into(),
                };
                Some(BasicAttack::Resolved {
                    target,
                    outcome: *outcome,
                })
            }
            _ => None,
        };
        self
    }
}

/// Sight transitions describe knowledge, never movement, death, or hidden causes.
/// Duplicate portal views count once; self sightings never count as discoveries.
pub fn changes(before: &Observation, after: &Observation) -> Vec<String> {
    describe_changes(before, after, None)
        .into_iter()
        .map(|line| line.text)
        .collect()
}

/// Narrate the observer's completed item work from disclosed inventories only.
pub fn completed_items(before: &Observation, after: &Observation) -> Vec<String> {
    let Some(interactions) = &after.interactions else {
        return Vec::new();
    };
    if before.tick == after.tick
        && before.interactions.as_ref().is_some_and(|old| {
            old.completed == interactions.completed
                && old.refused == interactions.refused
                && old.curse_blocked == interactions.curse_blocked
        })
    {
        return Vec::new();
    }
    interactions
        .completed
        .iter()
        .filter_map(|action| {
            let (verb, item) = match action {
                tor_protocol::Action::Equip { item, .. } => ("equipping", item),
                tor_protocol::Action::Unequip { item } => ("removing", item),
                tor_protocol::Action::Drink { item } => ("drinking", item),
                tor_protocol::Action::Eat { item } => ("eating", item),
                tor_protocol::Action::Read { item } => ("reading", item),
                tor_protocol::Action::Ignite { item } => ("igniting", item),
                tor_protocol::Action::Extinguish { item } => ("extinguishing", item),
                tor_protocol::Action::Zap { item, .. } => ("zap", item),
                _ => return None,
            };
            let name = before
                .inventory
                .iter()
                .chain(&after.inventory)
                .find(|candidate| candidate.id == *item)
                .map_or_else(|| "item".into(), |candidate| label(&candidate.name, "item"));
            Some(if verb == "zap" {
                format!("You zap the {name}.")
            } else {
                format!("You finish {verb} the {name}.")
            })
        })
        .chain(
            interactions
                .refused
                .iter()
                .map(|refusal| match refusal.reason {
                    tor_protocol::ItemRefusalReason::TooWeak => {
                        "You are too weak to use it.".to_owned()
                    }
                }),
        )
        .chain(interactions.curse_blocked.iter().map(|item| {
            let name = before
                .inventory
                .iter()
                .chain(&after.inventory)
                .find(|candidate| candidate.id == *item)
                .map_or_else(|| "item".into(), |candidate| label(&candidate.name, "item"));
            format!("You cannot remove the {name}; it is cursed.")
        }))
        .collect()
}

/// Combine action results and sight changes without repeating a door action.
pub fn observation(
    before: &Observation,
    after: &Observation,
    event: Option<&Event>,
) -> Vec<String> {
    observation_lines(before, after, event)
        .into_iter()
        .map(|line| line.text)
        .collect()
}

/// [`observation`], with what each line reports.
pub fn observation_lines(
    before: &Observation,
    after: &Observation,
    event: Option<&Event>,
) -> Vec<Line> {
    let lines = outcome_lines(&OutcomeFacts::capture(before, after, event));
    #[cfg(test)]
    {
        let mut legacy = lines.clone();
        for line in &mut legacy {
            if matches!(line.topic, Topic::DoorDiscovered | Topic::TrapDiscovered(_)) {
                line.topic = Topic::Discovery;
            }
        }
        assert_eq!(legacy, observation_lines_legacy(before, after, event));
    }
    lines
}

/// Render ordered disclosed outcomes independently of visual refresh cadence.
pub fn outcome_lines(outcomes: &OutcomeFacts) -> Vec<Line> {
    let facts = &outcomes.facts;
    let replaced = facts.iter().find(|fact| {
        matches!(
            fact,
            OutcomeFact::OwnWorkChanged {
                change: OutcomeWorkChange::Replaced { .. },
                ..
            }
        )
    });
    facts
        .iter()
        .enumerate()
        .filter_map(|(index, fact)| match (fact, replaced) {
            (
                OutcomeFact::OwnAction { event, name, .. },
                Some(OutcomeFact::OwnWorkChanged { work, .. }),
            ) => {
                let action = match event {
                    Event::ItemStarted { action } => {
                        ItemWorkKind::from_action(action).map(|(kind, _)| {
                            work_phrase(&OutcomeWork::Item {
                                work: kind,
                                name: name.as_ref().map(|name| name.text.clone()),
                            })
                        })
                    }
                    Event::AbilityStarted { ability, .. } => {
                        Some(format!("using {}", label(&ability.name, "an ability")))
                    }
                    _ => None,
                };
                action.map_or_else(
                    || outcome_line(outcomes, fact),
                    |action| {
                        Some(Line::new(
                            Topic::WorkChanged,
                            format!("You abandon {} and begin {action}.", work_phrase(work)),
                        ))
                    },
                )
            }
            (
                OutcomeFact::OwnWorkChanged {
                    change: OutcomeWorkChange::Replaced { .. },
                    ..
                },
                _,
            ) if facts.iter().any(|fact| {
                matches!(
                    fact,
                    OutcomeFact::OwnAction {
                        event: Event::AbilityStarted { .. } | Event::ItemStarted { .. },
                        ..
                    }
                )
            }) =>
            {
                None
            }
            (OutcomeFact::Combat { event, .. }, _)
                if matches!(
                    facts.get(index + 1),
                    Some(OutcomeFact::Combat { event: next, .. })
                        if redundant_magic_missile_flight(outcomes.self_target, event, next)
                ) =>
            {
                None
            }
            _ => outcome_line(outcomes, fact),
        })
        .collect()
}

fn redundant_magic_missile_flight(
    observer: ActorTarget,
    event: &CombatEventView,
    next: &CombatEventView,
) -> bool {
    let CombatEventView::Effect {
        caster: Some(caster),
        ability: Some(ability),
        delivery: EffectDelivery::Ray,
        segments,
        ..
    } = event
    else {
        return false;
    };
    *caster == observer
        && ability.id == "rogue.wand_magic_missile"
        && segments.len() == 1
        && matches!(
            next,
            CombatEventView::Ability {
                caster: Some(source),
                target: Some(_),
                ability: result,
                ..
            } if source == caster && result.id == ability.id
        )
}

fn work_phrase(work: &OutcomeWork) -> String {
    match work {
        OutcomeWork::Attack => "your attack".into(),
        OutcomeWork::Ability { name } => format!("using {}", label(name, "an ability")),
        OutcomeWork::Action => "your action".into(),
        OutcomeWork::Item { work, name } => {
            let verb = match work {
                ItemWorkKind::Equip => "equipping",
                ItemWorkKind::Unequip => "removing",
                ItemWorkKind::Drink => "drinking",
                ItemWorkKind::Eat => "eating",
                ItemWorkKind::Read => "reading",
                ItemWorkKind::Ignite => "lighting",
                ItemWorkKind::Extinguish => "extinguishing",
                ItemWorkKind::Zap => "zapping",
            };
            format!(
                "{verb} the {}",
                label(name.as_deref().unwrap_or(""), "item")
            )
        }
    }
}

/// Render one disclosed fact without coupling callers to a parallel line list.
pub fn outcome_line(outcomes: &OutcomeFacts, fact: &OutcomeFact) -> Option<Line> {
    let actor_name = |id| {
        outcomes
            .actor_names
            .iter()
            .find(|entry| entry.actor == id)
            .map_or_else(|| "figure".into(), |entry| label(&entry.name, "figure"))
    };
    let mut line = match fact {
        // Goals are standing state: clients show them, not the common log.
        OutcomeFact::Injured { .. } | OutcomeFact::Goal { .. } => return None,
        // Steps, wind-ups and follow-throughs are presentation timing, not prose.
        OutcomeFact::Activity {
            actor,
            activity: ActivityKind::ItemHandling { work, name },
            ..
        } => {
            let verb = match work {
                ItemWorkKind::Equip => "equips",
                ItemWorkKind::Unequip => "removes",
                ItemWorkKind::Drink => "drinks",
                ItemWorkKind::Eat => "eats",
                ItemWorkKind::Read => "reads",
                ItemWorkKind::Ignite => "lights",
                ItemWorkKind::Extinguish => "extinguishes",
                ItemWorkKind::Zap => "zaps",
            };
            Line::new(
                Topic::Activity(*actor),
                format!(
                    "The {} {verb} {}.",
                    actor_name(*actor),
                    name.as_deref().map_or_else(
                        || "something".to_owned(),
                        |name| format!("the {}", label(name, "item"))
                    )
                ),
            )
        }
        OutcomeFact::Activity { .. } => return None,
        OutcomeFact::UserMessage { style, segments } => Line::new(
            Topic::UserMessage {
                style: *style,
                segments: segments.clone(),
            },
            user_message_text(segments, &neutral_control_name),
        ),
        OutcomeFact::OwnWorkChanged { work, change, .. } => {
            let subject = work_phrase(work);
            let text = match change {
                OutcomeWorkChange::Paused => format!("You pause {subject}."),
                OutcomeWorkChange::Cancelled | OutcomeWorkChange::Replaced { .. } => {
                    format!("You abandon {subject}.")
                }
                OutcomeWorkChange::Failed => format!("You cannot complete {subject}."),
            };
            Line::new(Topic::WorkChanged, text)
        }
        OutcomeFact::OwnAction { event, name, .. } => Line::new(
            Topic::Own(event.clone()),
            action_with_name(event, name.as_ref()),
        ),
        OutcomeFact::TrapDiscovered { family } => Line::new(
            Topic::TrapDiscovered(*family),
            family.map_or_else(
                || "You notice a trap.".into(),
                |family| format!("You recognize a {}.", family.name()),
            ),
        ),
        OutcomeFact::DoorDiscovered => Line::new(Topic::DoorDiscovered, "You notice a door."),
        OutcomeFact::Displaced => Line::new(Topic::Motion, "You move involuntarily."),
        OutcomeFact::Impacted => Line::new(Topic::Motion, "You collide with an obstruction."),
        OutcomeFact::Noticed { actor, name, .. } => {
            let name = label(name, "figure");
            let article = if name.to_lowercase().starts_with(['a', 'e', 'i', 'o', 'u']) {
                "an"
            } else {
                "a"
            };
            Line::new(
                Topic::Noticed(*actor),
                format!("You notice {article} {name}."),
            )
        }
        OutcomeFact::LostSight { actor, name, .. } => Line::new(
            Topic::LostSight(*actor),
            format!("You can no longer see the {}.", label(name, "figure")),
        ),
        OutcomeFact::DoorChanged { name, open, .. } => Line::new(
            Topic::Door,
            format!(
                "The {} is now {}.",
                label(name, "door"),
                if *open { "open" } else { "closed" }
            ),
        ),
        OutcomeFact::Pacing { ready } => Line::new(
            Topic::Pacing,
            if *ready {
                "You can act again."
            } else {
                "You must wait."
            },
        ),
        OutcomeFact::ItemCompleted {
            work: ItemWorkKind::Zap,
            name,
        } => Line::new(
            Topic::Finished,
            format!(
                "You zap the {}.",
                label(name.as_deref().unwrap_or(""), "item")
            ),
        ),
        OutcomeFact::ItemCompleted { work, name } => {
            let verb = match work {
                ItemWorkKind::Equip => "equipping",
                ItemWorkKind::Unequip => "removing",
                ItemWorkKind::Drink => "drinking",
                ItemWorkKind::Eat => "eating",
                ItemWorkKind::Read => "reading",
                ItemWorkKind::Ignite => "igniting",
                ItemWorkKind::Extinguish => "extinguishing",
                ItemWorkKind::Zap => "zapping",
            };
            Line::new(
                Topic::Finished,
                format!(
                    "You finish {verb} the {}.",
                    label(name.as_deref().unwrap_or(""), "item")
                ),
            )
        }
        OutcomeFact::ItemRefused {
            reason: ItemRefusalReason::TooWeak,
        } => Line::new(Topic::Finished, "You are too weak to use it."),
        OutcomeFact::CurseBlocked { name } => Line::new(
            Topic::Finished,
            format!(
                "You cannot remove the {}; it is cursed.",
                label(name.as_deref().unwrap_or(""), "item")
            ),
        ),
        OutcomeFact::PublicPlan { record } => {
            Line::new(Topic::PublicPlan, crate::public_plan::line(record))
        }
        OutcomeFact::Combat { event, .. } => Line::new(
            Topic::Combat(event.clone()),
            combat_event_with_names(event, outcomes.self_target, &actor_name),
        ),
        OutcomeFact::Status { status } => {
            Line::new(Topic::Status, combat_status_facts(status, false))
        }
    };
    match fact {
        OutcomeFact::OwnAction {
            basic_attack: true, ..
        } => line.basic_attack = Some(BasicAttack::Preparing),
        OutcomeFact::Combat {
            event: CombatEventView::Ability {
                target, outcome, ..
            },
            basic_attack: true,
        } => {
            let target = match target {
                Some(actor) if *actor == outcomes.self_target => "yourself".into(),
                Some(actor) => format!("the {}", actor_name(*actor)),
                None => "something".into(),
            };
            line.basic_attack = Some(BasicAttack::Resolved {
                target,
                outcome: *outcome,
            });
        }
        _ => {}
    }
    Some(line)
}

#[cfg(test)]
fn observation_lines_legacy(
    before: &Observation,
    after: &Observation,
    event: Option<&Event>,
) -> Vec<Line> {
    let door = match event {
        Some(Event::DoorChanged { door, .. }) => Some(*door),
        _ => None,
    };
    let mut lines = describe_changes(before, after, door);
    if let Some(event) = event {
        lines.insert(
            0,
            Line::new(Topic::Own(event.clone()), action_legacy(event, after))
                .with_basic_attack(before, after),
        );
    }
    lines.extend(
        completed_items(before, after)
            .into_iter()
            .map(|text| Line::new(Topic::Finished, text)),
    );
    if let Some(combat) = &after.combat {
        if let Some(text) = crate::public_plan::changed_line(
            before
                .combat
                .as_ref()
                .and_then(|value| value.public_plan.as_ref()),
            combat.public_plan.as_ref(),
        ) {
            lines.push(Line::new(Topic::PublicPlan, text));
        }
        if before.tick != after.tick || before.combat != after.combat {
            lines.extend(
                combat
                    .events
                    .iter()
                    .enumerate()
                    .filter_map(|(index, event)| {
                        if combat.events.get(index + 1).is_some_and(|next| {
                            redundant_magic_missile_flight(after.self_target, event, next)
                        }) {
                            return None;
                        }
                        Some(
                            Line::new(
                                Topic::Combat(event.clone()),
                                combat_event(event, before, after),
                            )
                            .with_basic_attack(before, after),
                        )
                    }),
            );
            if before.combat.as_ref().is_none_or(|c| {
                c.hp != combat.hp
                    || c.victory != combat.victory
                    || c.dead != combat.dead
                    || c.sequence != combat.sequence
                    || (combat.sequence.is_some()
                        && c.preparation_active != combat.preparation_active)
            }) {
                lines.push(Line::new(
                    Topic::Status,
                    combat_status_facts(&OutcomeStatusFacts::from(combat), false),
                ));
            }
        }
    }
    lines
}

/// A disclosed actor's name from either view, for an actor that may have
/// just died or moved out of sight.
fn actor_name(actor: ActorTarget, before: &Observation, after: &Observation) -> String {
    after
        .visible_actors
        .iter()
        .chain(&before.visible_actors)
        .find(|a| a.id == actor && !a.name.trim().is_empty())
        .map_or_else(|| "figure".into(), |a| label(&a.name, "figure"))
}

/// One line for a combat event, written from the observer's point of view.
pub fn combat_event(event: &CombatEventView, before: &Observation, after: &Observation) -> String {
    combat_event_with_names(event, after.self_target, &|actor| {
        actor_name(actor, before, after)
    })
}

fn combat_event_with_names(
    event: &CombatEventView,
    observer: ActorTarget,
    actor_name: &impl Fn(ActorTarget) -> String,
) -> String {
    let subject = |actor: Option<ActorTarget>| match actor {
        Some(id) if id == observer => "You".to_owned(),
        Some(id) => format!("The {}", actor_name(id)),
        None => "Something".into(),
    };
    let object = |actor: Option<ActorTarget>| match actor {
        Some(id) if id == observer => "you".to_owned(),
        Some(id) => format!("the {}", actor_name(id)),
        None => "something".into(),
    };
    match event {
        CombatEventView::Effect {
            caster,
            presentation,
            delivery,
            cue,
            segments,
            ..
        } => effect_description(
            *caster,
            observer,
            actor_name,
            presentation,
            *delivery,
            *cue,
            segments,
        ),
        CombatEventView::UnseenAbility {
            target, outcome, ..
        } => {
            let object = object(Some(*target));
            if *outcome == AbilityOutcome::Miss {
                format!("Something attacks {object}, but misses.")
            } else {
                format!("Something attacks {object}.")
            }
        }
        CombatEventView::AnonymousAbility {
            caster,
            target,
            outcome,
        } => {
            let (subject, object) = (subject(*caster), object(*target));
            match outcome {
                AbilityOutcome::Applied | AbilityOutcome::Unaffected => {
                    format!("{subject} used an unidentified effect on {object}.")
                }
                AbilityOutcome::Miss => {
                    format!("{subject} used an unidentified effect on {object}, but missed.")
                }
            }
        }
        CombatEventView::Ability {
            caster,
            target,
            ability,
            outcome,
            strike,
        } => {
            let incoming_basic_attack = ability.id == "basic_melee"
                && *target == Some(observer)
                && caster.is_some_and(|id| id != observer);
            let own = *caster == Some(observer);
            let (subject, object) = (subject(*caster), object(*target));
            let ordinal = strike
                .filter(|ordinal| *ordinal > 0)
                .map(|ordinal| format!(" (strike {})", u32::from(ordinal) + 1))
                .unwrap_or_default();
            if incoming_basic_attack {
                let result = if *outcome == AbilityOutcome::Miss {
                    ", but misses"
                } else {
                    ""
                };
                return format!("{subject} attacks {object}{ordinal}{result}.");
            }
            if ability.id == "rogue.wand_magic_missile" {
                return match outcome {
                    AbilityOutcome::Applied => format!("The magic missile hits {object}."),
                    AbilityOutcome::Miss => format!("The magic missile misses {object}."),
                    AbilityOutcome::Unaffected => {
                        format!("The magic missile has no effect on {object}.")
                    }
                };
            }
            let name = label(&ability.name, "ability");
            if ability.id == "rattling_blow" && *outcome == AbilityOutcome::Applied {
                let verb = if own { "hit" } else { "hits" };
                return format!("{subject} {verb} {object} with {name}{ordinal}.");
            }
            match outcome {
                // This marker describes the primary, not every possible rider.
                // Uniform neutral wording does not disclose hidden rider state.
                AbilityOutcome::Applied | AbilityOutcome::Unaffected => {
                    format!("{subject} used {name}{ordinal} on {object}.")
                }
                AbilityOutcome::Miss => {
                    format!("{subject} used {name}{ordinal} on {object}, but missed.")
                }
            }
        }
        CombatEventView::EscapeResolved { success, .. } => {
            if *success {
                "You escape one hold.".into()
            } else {
                "You fail to escape the hold.".into()
            }
        }
        CombatEventView::Condition { condition } => {
            format!("You are affected by {}.", condition.name)
        }
        CombatEventView::Interrupted { .. } => "Your attack was interrupted.".into(),
        CombatEventView::Died { actor, cause } => match (*actor == observer, cause) {
            (true, DeathCause::Starvation) => "You starved to death.".into(),
            (true, DeathCause::Injury) => "You died.".into(),
            (false, DeathCause::Starvation) => {
                format!("{} starved to death.", subject(Some(*actor)))
            }
            (false, DeathCause::Injury) => format!("{} died.", subject(Some(*actor))),
        },
    }
}

/// The scenario's current goal, with each control named by `names`.
pub fn goal_text(segments: &[UserMessageSegment], names: &dyn ControlNames) -> String {
    user_message_text(segments, names)
}

/// A figure's disclosed visible equipment, as display names in the server's
/// canonical order: armour it wears and weapons it holds. Both are empty when
/// nothing is disclosed, which says nothing about what is out of sight.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attire {
    pub wears: Vec<String>,
    pub holds: Vec<String>,
}

impl Attire {
    pub fn of(worn: &[WornView]) -> Self {
        let names = |class: ItemClass| {
            worn.iter()
                .filter(|piece| piece.class == class)
                .map(|piece| label(&piece.appearance, ""))
                .filter(|name| !name.trim().is_empty())
                .collect()
        };
        Self {
            wears: names(ItemClass::Armor),
            holds: names(ItemClass::Weapon),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.wears.is_empty() && self.holds.is_empty()
    }
}

/// An injury level as an adjective phrase.
pub fn injury(injury: Injury) -> &'static str {
    match injury {
        Injury::Healthy => "healthy",
        Injury::Wounded => "wounded",
        Injury::BadlyWounded => "badly wounded",
        Injury::NearDeath => "near death",
    }
}

pub fn combat_status(c: &CombatView) -> String {
    combat_status_facts(&OutcomeStatusFacts::from(c), true)
}

fn combat_status_facts(c: &OutcomeStatusFacts, show_recovery: bool) -> String {
    let mut text = format!("HP {}/{}", c.hp, c.max_hp);
    if c.dead {
        text.push_str(" — You died. This run has ended.");
    } else if c.victory {
        text.push_str(if c.terminal {
            " — Victory! This run has ended."
        } else {
            " — Victory! You may continue exploring."
        });
        if !c.terminal {
            if let Some(sequence) = sequence_status_facts(c) {
                text.push_str(" — ");
                text.push_str(&sequence);
            }
        }
    } else if let Some(sequence) = sequence_status_facts(c) {
        text.push_str(" — ");
        text.push_str(&sequence);
    } else if let Some(remaining) = c.preparation_remaining {
        text.push_str(&format!(
            " — Attack preparation: {remaining} ticks remaining{}.",
            if c.preparation_active {
                ""
            } else {
                " (interrupted)"
            }
        ));
    } else if show_recovery && c.recovery_remaining > 0 {
        text.push_str(&format!(" — Recovering: {} ticks.", c.recovery_remaining));
    }
    text
}

/// Exact active-time progress and cancellation commitment for the observed actor.
/// This contains no action permissions; frontends offer controls from readiness.
pub fn sequence_status(c: &CombatView) -> Option<String> {
    sequence_status_facts(&OutcomeStatusFacts::from(c))
}

pub fn sequence_status_facts(c: &OutcomeStatusFacts) -> Option<String> {
    let sequence = c.sequence.as_ref()?;
    let phase = if sequence.completed == 0 {
        format!(
            "Attack preparation: {} ticks remaining{}. {}-strike sequence.",
            sequence.next_strike_remaining,
            if c.preparation_active {
                ""
            } else {
                " (interrupted)"
            },
            sequence.total
        )
    } else if c.preparation_active {
        format!(
            "Strike {} of {} complete.",
            sequence.completed, sequence.total
        )
    } else {
        format!(
            "Sequence paused after strike {} of {}.",
            sequence.completed, sequence.total
        )
    };
    Some(format!(
        "{phase} Next strike in {} ticks. Cancel leaves {} ticks of recovery.",
        sequence.next_strike_remaining, sequence.cancel_recovery_remaining
    ))
}

pub fn discovery_changes(before: &Observation, after: &Observation) -> Vec<String> {
    let mut lines = Vec::new();
    let traps = |view: &Observation| -> BTreeMap<TrapTarget, Option<TrapFamily>> {
        view.visible_cells
            .iter()
            .filter_map(|cell| cell.trap.as_ref())
            .map(|trap| (trap.id, trap.family))
            .collect()
    };
    let old = traps(before);
    for (id, family) in traps(after) {
        match (old.get(&id), family) {
            (None, None) => lines.push("You notice a trap.".into()),
            (None | Some(None), Some(family)) => {
                lines.push(format!("You recognize a {}.", family.name()))
            }
            _ => {}
        }
    }
    let old: BTreeMap<_, _> = before
        .visible_cells
        .iter()
        .map(|cell| (&cell.key, cell))
        .collect();
    let mut doors = std::collections::BTreeSet::new();
    for cell in &after.visible_cells {
        if let Some(door) = &cell.door {
            if old
                .get(&cell.key)
                .is_some_and(|old| old.wall && old.door.is_none())
                && doors.insert(door.id)
            {
                lines.push("You notice a door.".into());
            }
        }
    }
    lines
}

fn describe_changes(
    before: &Observation,
    after: &Observation,
    acted_door: Option<DoorTarget>,
) -> Vec<Line> {
    let actors = |view: &Observation| -> BTreeMap<ActorTarget, String> {
        view.visible_actors
            .iter()
            .filter(|a| a.id != view.self_target)
            .map(|a| (a.id, label(&a.name, "figure")))
            .collect()
    };
    let old = actors(before);
    let new = actors(after);
    let mut lines: Vec<_> = discovery_changes(before, after)
        .into_iter()
        .map(|text| Line::new(Topic::Discovery, text))
        .collect();
    if after.tick != before.tick {
        if let Some(motion) = &after.motion {
            if motion.displaced {
                lines.push(Line::new(Topic::Motion, "You move involuntarily."));
            }
            if motion.impacted {
                lines.push(Line::new(Topic::Motion, "You collide with an obstruction."));
            }
        }
    }
    for (id, name) in &new {
        if !old.contains_key(id) {
            let article = if name.to_lowercase().starts_with(['a', 'e', 'i', 'o', 'u']) {
                "an"
            } else {
                "a"
            };
            lines.push(Line::new(
                Topic::Noticed(*id),
                format!("You notice {article} {name}."),
            ));
        }
    }
    for (id, name) in &old {
        if !new.contains_key(id) {
            lines.push(Line::new(
                Topic::LostSight(*id),
                format!("You can no longer see the {name}."),
            ));
        }
    }
    let doors = |view: &Observation| -> BTreeMap<DoorTarget, (bool, String)> {
        view.visible_cells
            .iter()
            .filter_map(|c| c.door.as_ref())
            .map(|d| (d.id, (d.open, label(&d.name, "door"))))
            .collect()
    };
    let old = doors(before);
    for (id, (open, name)) in doors(after) {
        if Some(id) != acted_door && old.get(&id).is_some_and(|(was_open, _)| *was_open != open) {
            lines.push(Line::new(
                Topic::Door,
                format!(
                    "The {name} is now {}.",
                    if open { "open" } else { "closed" }
                ),
            ));
        }
    }
    if before.ready != after.ready && after.combat.as_ref().is_none_or(|c| !c.terminal) {
        lines.push(Line::new(
            Topic::Pacing,
            if after.ready {
                "You can act again."
            } else {
                "You must wait."
            },
        ));
    }
    lines
}

/// Ordinary action narration resolves names only from the resulting disclosed view.
pub fn action(event: &Event, view: &Observation) -> String {
    action_with_name(event, outcome_action_name(event, view).as_ref())
}

fn action_with_name(event: &Event, name: Option<&OutcomeName>) -> String {
    let named = || {
        name.map(|name| {
            label(
                &name.text,
                match name.fallback {
                    OutcomeNameFallback::Item => "item",
                    OutcomeNameFallback::Object => "object",
                    OutcomeNameFallback::Stairs => "stairs",
                    OutcomeNameFallback::Door => "door",
                },
            )
        })
    };
    match event {
        Event::EscapeStarted { .. } => "You begin trying to escape a hold.".into(),
        Event::Escaped {} => "You take the way out.".into(),
        Event::ItemStarted { action } => {
            let Some((work, _)) = ItemWorkKind::from_action(action) else {
                return "You begin preparing.".into();
            };
            let verb = match work {
                ItemWorkKind::Equip => "equip",
                ItemWorkKind::Unequip => "remove",
                ItemWorkKind::Drink => "drink",
                ItemWorkKind::Eat => "eat",
                ItemWorkKind::Read => "read",
                ItemWorkKind::Ignite => "ignite",
                ItemWorkKind::Extinguish => "extinguish",
                ItemWorkKind::Zap => "zap",
            };
            format!(
                "You begin to {verb} the {}.",
                named().unwrap_or_else(|| "item".into())
            )
        }
        Event::Moved { direction } => format!("You move {}.", crate::direction_name(*direction)),
        Event::Crumbled { quantity, .. } => {
            if *quantity == 1 {
                "The item crumbles as you try to pick it up.".into()
            } else {
                format!("{quantity} items crumble as you try to pick them up.")
            }
        }
        Event::Taken { quantity, .. } => named().map_or_else(
            || "You pick up the item.".into(),
            |name| {
                if *quantity == 1 {
                    format!("You pick up the {name}.")
                } else {
                    format!("You pick up {quantity} x {name}.")
                }
            },
        ),
        Event::Dropped { quantity, .. } => named().map_or_else(
            || "You drop the item.".into(),
            |name| format!("You drop {quantity} x {name}."),
        ),
        Event::DoorChanged { open, .. } => format!(
            "You {} the {}.",
            if *open { "open" } else { "close" },
            named().unwrap_or_else(|| "door".into())
        ),
        Event::Waited => "Time passes.".into(),
        Event::PreparationPaused => "Your preparation is paused until you act again.".into(),
        Event::AbilityStarted { ability, .. } => {
            format!("You prepare to use {}.", label(&ability.name, "ability"))
        }
    }
}

#[cfg(test)]
fn action_legacy(event: &Event, view: &Observation) -> String {
    match event {
        Event::EscapeStarted { .. } => "You begin trying to escape a hold.".into(),
        Event::Escaped {} => "You take the way out.".into(),
        Event::ItemStarted { action } => {
            let (verb, item) = match action {
                tor_protocol::Action::Equip { item, .. } => ("equip", item),
                tor_protocol::Action::Unequip { item } => ("remove", item),
                tor_protocol::Action::Drink { item } => ("drink", item),
                tor_protocol::Action::Eat { item } => ("eat", item),
                tor_protocol::Action::Read { item } => ("read", item),
                tor_protocol::Action::Ignite { item } => ("ignite", item),
                tor_protocol::Action::Extinguish { item } => ("extinguish", item),
                tor_protocol::Action::Zap { item, .. } => ("zap", item),
                _ => return "You begin preparing.".into(),
            };
            let name = view
                .inventory
                .iter()
                .find(|candidate| candidate.id == *item)
                .map_or_else(|| "item".into(), |candidate| label(&candidate.name, "item"));
            format!("You begin to {verb} the {name}.")
        }

        Event::Moved { direction } => format!("You move {}.", crate::direction_name(*direction)),
        Event::Crumbled { quantity, .. } => {
            if *quantity == 1 {
                "The item crumbles as you try to pick it up.".into()
            } else {
                format!("{quantity} items crumble as you try to pick them up.")
            }
        }
        Event::Taken {
            result, quantity, ..
        } => view.inventory.iter().find(|i| i.id == *result).map_or_else(
            || "You pick up the item.".into(),
            |i| {
                if *quantity == 1 {
                    format!("You pick up the {}.", label(&i.name, "item"))
                } else {
                    format!("You pick up {} x {}.", quantity, label(&i.name, "item"))
                }
            },
        ),
        Event::Dropped {
            result, quantity, ..
        } => view
            .ground_items
            .iter()
            .find(|i| i.item.id == *result)
            .map_or_else(
                || "You drop the item.".into(),
                |i| {
                    format!(
                        "You drop {} x {}.",
                        quantity,
                        label(crate::items::ground_name(i), "item")
                    )
                },
            ),
        Event::DoorChanged { door, open } => {
            let name = view
                .visible_cells
                .iter()
                .filter_map(|c| c.door.as_ref())
                .find(|d| d.id == *door)
                .map_or_else(|| "door".into(), |d| label(&d.name, "door"));
            format!("You {} the {name}.", if *open { "open" } else { "close" })
        }
        Event::Waited => "Time passes.".into(),
        Event::PreparationPaused => "Your preparation is paused until you act again.".into(),
        Event::AbilityStarted { ability, .. } => {
            format!("You prepare to use {}.", label(&ability.name, "ability"))
        }
    }
}

#[cfg(test)]
#[path = "narration_tests.rs"]
mod tests;

/// Describe only the visible presentation. A path is not proof of a hit,
/// damage, a wall collision, or anything that happened between clipped strips.
/// Ability/outcome lines retain those separately disclosed facts; without a
/// correlation identity we must not merge nearby same-ability resolutions.
fn effect_description(
    caster: Option<ActorTarget>,
    observer: ActorTarget,
    actor_name: &impl Fn(ActorTarget) -> String,
    presentation: &tor_protocol::EffectPresentation,
    delivery: EffectDelivery,
    _cue: EffectCue,
    segments: &[Vec<Position>],
) -> String {
    let appearance = presentation.form;
    let material = effect_visual_label(presentation.visual);
    let kind = presentation.object.as_ref().map_or_else(
        || effect_appearance_label(appearance),
        |object| object.name.as_str(),
    );
    let path = matches!(delivery, EffectDelivery::Ray | EffectDelivery::Projectile);
    let subject = match caster.filter(|_| path) {
        Some(id) if id == observer => format!("Your {material}{kind}"),
        Some(id) => format!("The {}'s {material}{kind}", actor_name(id)),
        None => format!(
            "{} {material}{kind}",
            if kind.starts_with(['a', 'e', 'i', 'o', 'u']) && material.is_empty() {
                "An"
            } else {
                "A"
            }
        ),
    };
    let moving = segments
        .iter()
        .any(|segment| segment.windows(2).any(|pair| pair[0] != pair[1]));
    if path && moving {
        // Exact A-B-A reversal is sufficient to describe a visible return.
        // Never compare across strip boundaries: they may hide a turn or join.
        let returns = segments.iter().any(|segment| {
            segment
                .windows(3)
                .any(|points| points[0] == points[2] && points[0] != points[1])
        });
        let verb = match appearance {
            EffectAppearance::Bolt => "streaks",
            EffectAppearance::Missile => "flies",
            _ => "passes",
        };
        let course = if segments.len() > 1 {
            "through separate visible stretches"
        } else {
            "through view"
        };
        return format!(
            "{subject} {verb} {course}{}.",
            if returns {
                ", doubling back along a visible stretch"
            } else {
                ""
            }
        );
    }
    let area = if delivery == EffectDelivery::Area
        && segments
            .iter()
            .flatten()
            .next()
            .is_some_and(|first| segments.iter().flatten().any(|position| position != first))
    {
        " across an area"
    } else {
        ""
    };
    let verb = match appearance {
        EffectAppearance::Flash => "flares briefly",
        EffectAppearance::Halo => "shines briefly",
        _ => "briefly appears",
    };
    format!("{subject} {verb}{area}.")
}

/// Client wording for a server-authored presentation form, independent of delivery.
pub fn effect_visual_label(visual: tor_protocol::EffectVisual) -> &'static str {
    match visual {
        tor_protocol::EffectVisual::Neutral => "",
        tor_protocol::EffectVisual::Flame => "fiery ",
        tor_protocol::EffectVisual::Frost => "frosty ",
        tor_protocol::EffectVisual::Sparks => "sparking ",
        tor_protocol::EffectVisual::Vapor => "misty ",
        tor_protocol::EffectVisual::Radiance => "radiant ",
    }
}

pub fn effect_appearance_label(appearance: EffectAppearance) -> &'static str {
    match appearance {
        EffectAppearance::Bolt => "bolt",
        EffectAppearance::Missile => "missile",
        EffectAppearance::Flash => "flash",
        EffectAppearance::Halo => "halo",
        EffectAppearance::Cloud => "cloud",
        EffectAppearance::StatusIndicator => "status indicator",
    }
}

pub fn effect_cue_label(cue: EffectCue) -> &'static str {
    match cue {
        EffectCue::Neutral | EffectCue::Physical => "",
        EffectCue::Fire => "fiery ",
        EffectCue::Cold => "frosty ",
        EffectCue::Lightning => "electrical ",
        EffectCue::Poison => "poisonous ",
        EffectCue::Healing => "healing ",
        EffectCue::Status => "status-changing ",
    }
}

#[cfg(test)]
mod authoritative_effect_tests {
    use super::*;

    #[test]
    fn spatial_effect_narration_uses_only_disclosed_semantics() {
        let observer = ActorTarget::from_digest([1; 32]);
        let event = CombatEventView::Effect {
            caster: None,
            ability: None,
            tick: 12,
            anchors: vec![],
            timing: EffectTiming::Instant,
            presentation: tor_protocol::EffectPresentation::neutral(EffectAppearance::Bolt),
            delivery: EffectDelivery::Ray,
            cue: EffectCue::Neutral,
            segments: vec![vec![Position { x: 2, y: 3, z: 0 }]],
        };
        assert_eq!(
            combat_event_with_names(&event, observer, &|_| panic!("no actor lookup")),
            "A bolt briefly appears."
        );
        let fiery = CombatEventView::Effect {
            caster: None,
            ability: None,
            tick: 12,
            anchors: vec![],
            timing: EffectTiming::Instant,
            presentation: tor_protocol::EffectPresentation {
                visual: tor_protocol::EffectVisual::Flame,
                ..tor_protocol::EffectPresentation::neutral(EffectAppearance::Cloud)
            },
            delivery: EffectDelivery::Area,
            // Appearance is authored independently of the mechanical cue.
            cue: EffectCue::Neutral,
            segments: vec![vec![Position { x: 2, y: 3, z: 0 }]],
        };
        assert_eq!(
            combat_event_with_names(&fiery, observer, &|_| panic!("no actor lookup")),
            "A fiery cloud briefly appears."
        );
    }

    fn narrate(
        appearance: EffectAppearance,
        delivery: EffectDelivery,
        cue: EffectCue,
        caster: Option<ActorTarget>,
        strips: &[&[i32]],
    ) -> String {
        let segments = strips
            .iter()
            .map(|strip| {
                strip
                    .iter()
                    .map(|x| Position { x: *x, y: 0, z: 0 })
                    .collect()
            })
            .collect::<Vec<_>>();
        effect_description(
            caster,
            ActorTarget::from_digest([1; 32]),
            &|_| "dragon".into(),
            &tor_protocol::EffectPresentation {
                visual: match cue {
                    EffectCue::Fire => tor_protocol::EffectVisual::Flame,
                    EffectCue::Cold => tor_protocol::EffectVisual::Frost,
                    EffectCue::Lightning => tor_protocol::EffectVisual::Sparks,
                    _ => tor_protocol::EffectVisual::Neutral,
                },
                ..tor_protocol::EffectPresentation::neutral(appearance)
            },
            delivery,
            cue,
            &segments,
        )
    }

    #[test]
    fn rays_describe_disclosed_return_without_inventing_collision_or_impact() {
        assert_eq!(
            narrate(
                EffectAppearance::Bolt,
                EffectDelivery::Ray,
                EffectCue::Fire,
                Some(ActorTarget::from_digest([1; 32])),
                &[&[0, 1, 2, 1, 0]]
            ),
            "Your fiery bolt streaks through view, doubling back along a visible stretch."
        );
        assert_eq!(
            narrate(
                EffectAppearance::Bolt,
                EffectDelivery::Ray,
                EffectCue::Cold,
                Some(ActorTarget::from_digest([2; 32])),
                &[&[0, 1]]
            ),
            "The dragon's frosty bolt streaks through view."
        );
    }

    #[test]
    fn clipped_strips_never_supply_a_hidden_join_or_return() {
        assert_eq!(
            narrate(
                EffectAppearance::Bolt,
                EffectDelivery::Ray,
                EffectCue::Lightning,
                None,
                &[&[0, 1], &[0, -1]]
            ),
            "A sparking bolt streaks through separate visible stretches."
        );
        // Isolated disclosed points are not evidence of a visible flight.
        assert_eq!(
            narrate(
                EffectAppearance::Missile,
                EffectDelivery::Projectile,
                EffectCue::Physical,
                None,
                &[&[0], &[9]]
            ),
            "A missile briefly appears."
        );
    }

    #[test]
    fn missile_motion_does_not_guess_weapon_or_target() {
        assert_eq!(
            narrate(
                EffectAppearance::Missile,
                EffectDelivery::Projectile,
                EffectCue::Physical,
                None,
                &[&[0, 1, 2]]
            ),
            "A missile flies through view."
        );
    }

    #[test]
    fn target_and_area_cues_do_not_invent_appearance_recipient_or_full_shape() {
        // Healing/poison cues are not evidence of an authored visible style.
        assert_eq!(
            narrate(
                EffectAppearance::Flash,
                EffectDelivery::Target,
                EffectCue::Status,
                Some(ActorTarget::from_digest([1; 32])),
                &[&[1]]
            ),
            "A flash flares briefly."
        );
        assert_eq!(
            narrate(
                EffectAppearance::Halo,
                EffectDelivery::Target,
                EffectCue::Healing,
                None,
                &[&[1]]
            ),
            "A halo shines briefly."
        );
        assert_eq!(
            narrate(
                EffectAppearance::Cloud,
                EffectDelivery::Area,
                EffectCue::Poison,
                None,
                &[&[1, 2], &[5]]
            ),
            "A cloud briefly appears across an area."
        );
        assert_eq!(
            narrate(
                EffectAppearance::Cloud,
                EffectDelivery::Area,
                EffectCue::Neutral,
                None,
                &[&[1], &[1]]
            ),
            "A cloud briefly appears."
        );
    }

    #[test]
    fn magic_missile_names_the_result_and_elides_only_a_matching_flight_line() {
        let before = crate::test_support::observation();
        let mut after = before.clone();
        let target = before.visible_actors[0].id;
        let ability = crate::test_support::content("rogue.wand_magic_missile");
        let flight = CombatEventView::Effect {
            caster: Some(before.self_target),
            ability: Some(ability.clone()),
            tick: 12,
            anchors: vec![],
            timing: EffectTiming::Instant,
            presentation: tor_protocol::EffectPresentation::neutral(EffectAppearance::Bolt),
            delivery: EffectDelivery::Ray,
            cue: EffectCue::Neutral,
            segments: vec![vec![
                Position { x: 0, y: 0, z: 0 },
                Position { x: 1, y: 0, z: 0 },
            ]],
        };
        after.combat.as_mut().unwrap().events = vec![flight.clone()];
        assert!(observation_lines(&before, &after, None)
            .iter()
            .any(|line| line.topic == Topic::Combat(flight.clone())));

        for (outcome, verb) in [
            (AbilityOutcome::Applied, "hits"),
            (AbilityOutcome::Miss, "misses"),
            (AbilityOutcome::Unaffected, "has no effect on"),
        ] {
            let result = CombatEventView::Ability {
                caster: Some(before.self_target),
                target: Some(target),
                ability: ability.clone(),
                outcome,
                strike: Some(0),
            };
            after.combat.as_mut().unwrap().events = vec![flight.clone(), result.clone()];
            let lines = observation_lines(&before, &after, None);
            assert!(!lines
                .iter()
                .any(|line| line.topic == Topic::Combat(flight.clone())));
            assert!(lines.iter().any(|line| {
                line.topic == Topic::Combat(result.clone())
                    && line
                        .text
                        .starts_with(&format!("The magic missile {verb} the "))
            }));
        }
    }

    #[test]
    fn motion_complements_authoritative_outcome_without_claiming_a_hit() {
        let before = crate::test_support::observation();
        for (outcome, ending) in [
            (AbilityOutcome::Applied, "something."),
            (AbilityOutcome::Unaffected, "something."),
            (AbilityOutcome::Miss, "something, but missed."),
        ] {
            let mut after = before.clone();
            let effect = CombatEventView::Effect {
                caster: Some(after.self_target),
                ability: None,
                tick: 12,
                anchors: vec![],
                timing: EffectTiming::Instant,
                presentation: tor_protocol::EffectPresentation::neutral(EffectAppearance::Bolt),
                delivery: EffectDelivery::Ray,
                cue: EffectCue::Fire,
                segments: vec![vec![
                    Position { x: 0, y: 0, z: 0 },
                    Position { x: 1, y: 0, z: 0 },
                ]],
            };
            let ability = CombatEventView::Ability {
                strike: None,
                caster: Some(after.self_target),
                target: None,
                ability: crate::test_support::content("magic_bolt"),
                outcome,
            };
            after.combat.as_mut().unwrap().events = vec![effect.clone(), ability.clone()];
            let lines = observation_lines(&before, &after, None);
            let text = lines
                .iter()
                .filter(|line| matches!(line.topic, Topic::Combat(_)))
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>();
            assert_eq!(
                text,
                vec![
                    // A Fire cue cannot override explicitly neutral presentation.
                    "Your bolt streaks through view.",
                    &format!("You used Magic Bolt on {ending}")
                ]
            );
            assert!(lines
                .iter()
                .any(|line| line.topic == Topic::Combat(effect.clone())));
            assert!(lines
                .iter()
                .any(|line| line.topic == Topic::Combat(ability.clone())));
        }
    }
}
