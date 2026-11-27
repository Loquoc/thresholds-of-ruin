//! What happened, as structured beats derived from ordered event-time facts.
//!
//! Beats name things as the character knew them at the time, so a passage
//! can still name a figure that has since died or gone out of sight.
use std::collections::BTreeMap;

use tor_client_common::Palette;
use tor_protocol::*;

use super::scene::whereabouts;
use super::scene::{Kind, Scene};

/// A figure as it was known when the beat happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Figure {
    pub id: ActorTarget,
    pub name: String,
}

/// Who did or suffered something.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Who {
    Me,
    Figure(Figure),
    /// Someone the character couldn't see.
    Unseen,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Beat {
    Discovery(String),
    Condition(String),
    /// One step of the character's own movement.
    Stepped(Direction),
    /// The character deliberately traversed its terminal objective endpoint.
    Escaped(String),
    Crumbled {
        quantity: u64,
    },
    Took {
        name: String,
        quantity: u64,
    },
    Dropped {
        name: String,
        quantity: u64,
    },
    /// The character opened or closed a door.
    SetDoor {
        name: String,
        open: bool,
    },
    Waited,
    ItemBegan(String),
    ItemFinished(String),
    AbilityBegan(String),
    /// Generic linked-hold progress never carries the holder identity.
    EscapeHold(String),
    AbilityResolved {
        ability: ContentRef,
        attacker: Who,
        target: Who,
        outcome: AbilityOutcome,
        strike: Option<u16>,
        /// A plain attack rather than a technique.
        basic: bool,
        /// Which telling to use, so blows in a row read differently.
        variant: u32,
    },
    /// A figure's visible condition changed, short of death.
    Injured {
        figure: Figure,
        level: Injury,
    },
    SequenceStatus(String),
    /// A paid observation expressed entirely from its immutable public record.
    PublicPlan(String),
    /// The server refused execution while the disclosed intention stays queued.
    ExecutionRefused(String),
    /// The character's attack was interrupted.
    Interrupted,
    Died(Who),
    /// The character's food reserve ran out.
    Starved,
    Appeared {
        figure: Figure,
        whereabouts: String,
    },
    /// A walk stopped because this came into view: "a copper token".
    Spotted {
        what: String,
        whereabouts: String,
    },
    Vanished(Figure),
    /// A door changed state, not by the character's hand.
    DoorChanged {
        name: String,
        whereabouts: String,
        open: bool,
    },
    Displaced,
    Impacted,
    Hp {
        from: u64,
        to: u64,
        max: u64,
    },
    Victory {
        terminal: bool,
    },
    /// The scenario's new goal, with controls named as this client's commands.
    Goal(String),
    /// Server-authored tutorial, hint or narration text, with controls
    /// already named as this client's commands. Told on its own line.
    UserMessage {
        style: UserMessageStyle,
        text: String,
    },
    /// A journey ended.
    Journey {
        phase: TravelPhase,
    },
    Control(bool),
    /// A figure stands where a blocked journey would have gone.
    Barred(Figure),
    /// The state was replaced, as by a rewind or resynchronization.
    Resync,
}

/// Remembers names, so beats can name figures after they're gone.
#[derive(Clone, Debug, Default)]
pub struct Chronicler {
    names: BTreeMap<ActorTarget, String>,
    /// Blows told so far, so each is worded afresh.
    blows: u32,
    status: Option<OutcomeStatusFacts>,
}

impl Chronicler {
    /// Learn the names in a view without recording anything.
    pub fn learn(&mut self, state: &StateView, palette: &Palette) {
        if self.status.is_none() {
            self.status = state
                .observation
                .combat
                .as_ref()
                .map(OutcomeStatusFacts::from);
        }
        for r in Scene::new(state, palette).of(Kind::Figure) {
            if let super::scene::Key::Actor(id) = r.key {
                self.names.insert(id, r.name.clone());
            }
        }
    }

    /// A snapshot is a new authoritative baseline, not an ordinary picture.
    /// Do not compare the next outcome with HP or names from a discarded branch.
    pub fn reset_baseline(&mut self, state: &StateView, palette: &Palette) {
        self.names.clear();
        self.status = None;
        self.learn(state, palette);
    }

    fn who(&self, actor: Option<ActorTarget>, me: ActorTarget) -> Who {
        match actor {
            Some(id) if id == me => Who::Me,
            Some(id) => Who::Figure(self.figure(id)),
            None => Who::Unseen,
        }
    }

    fn figure(&self, id: ActorTarget) -> Figure {
        Figure {
            id,
            name: self
                .names
                .get(&id)
                .filter(|name| !name.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| "figure".into()),
        }
    }

    /// Consume event-time facts; never reconstruct action results from visuals.
    pub fn outcome(&mut self, facts: &OutcomeFacts, palette: &Palette) -> Vec<Beat> {
        for actor in &facts.actor_names {
            let (name, asset) = actor
                .current_disclosed_appearance
                .as_ref()
                .map_or((&actor.name, actor.asset.as_deref()), |current| {
                    (&current.name, current.asset.as_deref())
                });
            self.names
                .insert(actor.actor, disclosed_figure_name(name, asset, palette));
        }
        for fact in &facts.facts {
            if let OutcomeFact::Noticed {
                actor, name, asset, ..
            }
            | OutcomeFact::LostSight { actor, name, asset } = fact
            {
                self.names.insert(
                    *actor,
                    disclosed_figure_name(name, asset.as_deref(), palette),
                );
            }
        }
        // Prose has always told combat, wounds, plans, then progress. Sort
        // typed facts directly; rendering never relies on positional pairing.
        let mut ordered: Vec<_> = facts.facts.iter().collect();
        ordered.sort_by_key(|fact| match fact {
            OutcomeFact::OwnAction { .. } => 0,
            OutcomeFact::OwnWorkChanged { .. } => 4,
            OutcomeFact::ItemCompleted { .. }
            | OutcomeFact::ItemRefused { .. }
            | OutcomeFact::CurseBlocked { .. } => 1,
            OutcomeFact::TrapDiscovered { .. } | OutcomeFact::DoorDiscovered => 2,
            OutcomeFact::Displaced | OutcomeFact::Impacted => 3,
            OutcomeFact::Combat { .. } => 4,
            OutcomeFact::Injured { .. } => 5,
            OutcomeFact::PublicPlan { .. } => 6,
            OutcomeFact::Status { .. } => 7,
            OutcomeFact::Goal { .. } => 8,
            OutcomeFact::Noticed { .. } | OutcomeFact::Activity { .. } => 9,
            OutcomeFact::LostSight { .. } => 10,
            OutcomeFact::DoorChanged { .. } => 11,
            OutcomeFact::Pacing { .. } => 12,
            OutcomeFact::UserMessage { .. } => 13,
        });
        let mut beats = Vec::new();
        for fact in ordered {
            match fact {
                OutcomeFact::Injured { actor, injury } => {
                    beats.push(Beat::Injured {
                        figure: self.figure(*actor),
                        level: *injury,
                    });
                    continue;
                }
                OutcomeFact::Goal { segments } => {
                    beats.push(Beat::Goal(tor_client_common::narration::goal_text(
                        segments,
                        &crate::controls::command,
                    )));
                    continue;
                }
                OutcomeFact::UserMessage { style, segments } => {
                    beats.push(Beat::UserMessage {
                        style: *style,
                        text: tor_client_common::narration::user_message_text(
                            segments,
                            &crate::controls::command,
                        ),
                    });
                    continue;
                }
                _ => {}
            }
            let Some(line) = tor_client_common::narration::outcome_line(facts, fact) else {
                continue;
            };
            match fact {
                OutcomeFact::OwnAction {
                    event,
                    name,
                    basic_attack,
                    ..
                } => {
                    let name = own_action_name(event, name.as_ref());
                    match event {
                        Event::Moved { direction } => beats.push(Beat::Stepped(*direction)),
                        Event::Waited => beats.push(Beat::Waited),
                        Event::Taken { quantity, .. } => beats.push(Beat::Took {
                            name,
                            quantity: *quantity,
                        }),
                        Event::Dropped { quantity, .. } => beats.push(Beat::Dropped {
                            name,
                            quantity: *quantity,
                        }),
                        Event::Crumbled { quantity, .. } => beats.push(Beat::Crumbled {
                            quantity: *quantity,
                        }),
                        Event::DoorChanged { open, .. } => {
                            beats.push(Beat::SetDoor { name, open: *open })
                        }
                        Event::Escaped { .. } => beats.push(Beat::Escaped(line.text)),
                        Event::EscapeStarted { .. } => beats.push(Beat::EscapeHold(line.text)),
                        Event::ItemStarted { .. } => beats.push(Beat::ItemBegan(line.text)),
                        Event::AbilityStarted { .. } if *basic_attack => {
                            beats.push(Beat::AbilityBegan("You ready yourself to strike.".into()));
                        }
                        Event::AbilityStarted { .. } => beats.push(Beat::AbilityBegan(line.text)),
                        Event::PreparationPaused => {}
                    }
                }
                OutcomeFact::Combat {
                    event,
                    basic_attack,
                } => beats.push(match event {
                    CombatEventView::Ability {
                        caster,
                        target,
                        ability,
                        outcome,
                        strike,
                    } => {
                        self.blows = self.blows.wrapping_add(1);
                        Beat::AbilityResolved {
                            ability: ability.clone(),
                            attacker: self.who(*caster, facts.self_target),
                            target: self.who(*target, facts.self_target),
                            outcome: *outcome,
                            strike: *strike,
                            basic: *basic_attack,
                            variant: self.blows,
                        }
                    }
                    CombatEventView::Died {
                        actor,
                        cause: DeathCause::Starvation,
                    } if *actor == facts.self_target => Beat::Starved,
                    CombatEventView::Died { actor, .. } => {
                        Beat::Died(self.who(Some(*actor), facts.self_target))
                    }
                    CombatEventView::Interrupted { .. } => Beat::Interrupted,
                    CombatEventView::Condition { condition } => {
                        Beat::Condition(condition.name.clone())
                    }
                    _ => Beat::EscapeHold(line.text),
                }),
                OutcomeFact::Status { status } => {
                    if self.status.as_ref().is_none_or(|old| {
                        old.sequence != status.sequence
                            || old.preparation_active != status.preparation_active
                    }) {
                        if let Some(line) =
                            tor_client_common::narration::sequence_status_facts(status)
                        {
                            beats.push(Beat::SequenceStatus(line));
                        }
                    }
                    if let Some(old) = &self.status {
                        if old.hp != status.hp {
                            beats.push(Beat::Hp {
                                from: old.hp,
                                to: status.hp,
                                max: status.max_hp,
                            });
                        }
                    }
                    if status.victory && self.status.as_ref().is_none_or(|old| !old.victory) {
                        beats.push(Beat::Victory {
                            terminal: status.terminal,
                        });
                    }
                    self.status = Some(status.clone());
                }
                OutcomeFact::OwnWorkChanged { .. } => beats.push(Beat::ItemBegan(line.text)),
                OutcomeFact::ItemCompleted { .. } => beats.push(Beat::ItemFinished(line.text)),
                OutcomeFact::PublicPlan { .. } => beats.push(Beat::PublicPlan(line.text)),
                OutcomeFact::Displaced => beats.push(Beat::Displaced),
                OutcomeFact::Impacted => beats.push(Beat::Impacted),
                OutcomeFact::LostSight { actor, .. } => {
                    beats.push(Beat::Vanished(self.figure(*actor)))
                }
                OutcomeFact::Noticed {
                    actor, position, ..
                } => beats.push(Beat::Appeared {
                    figure: self.figure(*actor),
                    whereabouts: whereabouts(*position),
                }),
                OutcomeFact::DoorChanged {
                    name,
                    open,
                    position,
                } => beats.push(Beat::DoorChanged {
                    name: if name.trim().is_empty() {
                        "door".into()
                    } else {
                        crate::safe(name).to_lowercase()
                    },
                    whereabouts: whereabouts(*position),
                    open: *open,
                }),
                OutcomeFact::TrapDiscovered { .. } | OutcomeFact::DoorDiscovered => {
                    beats.push(Beat::Discovery(line.text))
                }
                OutcomeFact::ItemRefused { .. } | OutcomeFact::CurseBlocked { .. } => {
                    beats.push(Beat::ItemFinished(line.text))
                }
                OutcomeFact::Pacing { .. } => {}
                // Only item handling has a line; it is told as it was seen.
                OutcomeFact::Activity { .. } => beats.push(Beat::EscapeHold(line.text)),
                OutcomeFact::Injured { .. }
                | OutcomeFact::Goal { .. }
                | OutcomeFact::UserMessage { .. } => unreachable!(),
            }
        }
        self.status = facts.status.clone();
        beats
    }

    /// The beats between two consecutive observations, given the history
    /// event that came with the second.
    #[cfg(test)]
    fn observe(
        &mut self,
        before: &StateView,
        after: &StateView,
        event: Option<&HistoryEntry>,
        palette: &Palette,
    ) -> Vec<Beat> {
        let (b, a) = (&before.observation, &after.observation);
        let then = Scene::new(before, palette);
        let now = Scene::new(after, palette);
        self.learn(before, palette);
        self.learn(after, palette);
        let mut beats = Vec::new();
        let mut own_door = None;
        if let Some(HistoryEntry {
            content: HistoryContent::Action { event, .. },
            ..
        }) = event
        {
            match event.as_ref() {
                Event::EscapeStarted { .. } => beats.push(Beat::EscapeHold(
                    tor_client_common::narration::action(event, a),
                )),
                Event::Moved { direction } => beats.push(Beat::Stepped(*direction)),
                Event::Escaped { .. } => beats.push(Beat::Escaped(
                    tor_client_common::narration::action(event, a),
                )),
                Event::Crumbled { quantity, .. } => beats.push(Beat::Crumbled {
                    quantity: *quantity,
                }),
                Event::Taken {
                    result, quantity, ..
                } => beats.push(Beat::Took {
                    name: item_name(&a.inventory, *result),
                    quantity: *quantity,
                }),
                Event::Dropped {
                    result, quantity, ..
                } => beats.push(Beat::Dropped {
                    name: a
                        .ground_items
                        .iter()
                        .find(|g| g.item.id == *result)
                        .map_or_else(
                            || "thing".into(),
                            |g| crate::safe(tor_client_common::items::ground_name(g)),
                        ),
                    quantity: *quantity,
                }),
                Event::DoorChanged { door, open } => {
                    own_door = Some(*door);
                    beats.push(Beat::SetDoor {
                        name: door_name(a, *door),
                        open: *open,
                    })
                }
                Event::Waited => beats.push(Beat::Waited),
                Event::ItemStarted { .. } => beats.push(Beat::ItemBegan(
                    tor_client_common::narration::action(event, a),
                )),
                Event::AbilityStarted { ability, .. } if basic(a, ability) => {
                    beats.push(Beat::AbilityBegan("You ready yourself to strike.".into()))
                }
                Event::AbilityStarted { .. } => beats.push(Beat::AbilityBegan(
                    tor_client_common::narration::action(event, a),
                )),
                Event::PreparationPaused => {}
            }
        }
        beats.extend(
            tor_client_common::narration::completed_items(b, a)
                .into_iter()
                .map(Beat::ItemFinished),
        );
        beats.extend(
            tor_client_common::narration::discovery_changes(b, a)
                .into_iter()
                .map(Beat::Discovery),
        );
        if a.tick != b.tick {
            if let Some(motion) = &a.motion {
                if motion.displaced {
                    beats.push(Beat::Displaced);
                }
                if motion.impacted {
                    beats.push(Beat::Impacted);
                }
            }
        }
        if let Some(combat) = &a.combat {
            let fresh = a.tick != b.tick
                || event.is_some()
                || b.combat.as_ref().is_none_or(|c| c.events != combat.events);
            if fresh {
                for e in &combat.events {
                    beats.push(match *e {
                        CombatEventView::Ability {
                            caster,
                            target,
                            ref ability,
                            outcome,
                            strike,
                        } => {
                            let attacker = self.who(caster, a.self_target);
                            let basic = attacker == Who::Me && basic(a, ability);
                            self.blows = self.blows.wrapping_add(1);
                            Beat::AbilityResolved {
                                ability: ability.clone(),
                                attacker,
                                target: self.who(target, a.self_target),
                                outcome,
                                strike,
                                basic,
                                variant: self.blows,
                            }
                        }
                        CombatEventView::Effect { .. }
                        | CombatEventView::UnseenAbility { .. }
                        | CombatEventView::AnonymousAbility { .. }
                        | CombatEventView::EscapeResolved { .. } => {
                            Beat::EscapeHold(tor_client_common::narration::combat_event(e, b, a))
                        }
                        CombatEventView::Condition { ref condition } => {
                            Beat::Condition(condition.name.clone())
                        }
                        CombatEventView::Interrupted { .. } => Beat::Interrupted,
                        CombatEventView::Died {
                            actor,
                            cause: DeathCause::Starvation,
                        } if actor == a.self_target => Beat::Starved,
                        CombatEventView::Died { actor, .. } => {
                            Beat::Died(self.who(Some(actor), a.self_target))
                        }
                    });
                }
            }
            let old = b.combat.as_ref();
            // Wounds told as they show: "it staggers, near death".
            let dying: Vec<ActorTarget> = combat
                .events
                .iter()
                .filter_map(|e| match e {
                    CombatEventView::Died { actor, .. } => Some(*actor),
                    _ => None,
                })
                .collect();
            if let Some(old) = old {
                for now in &combat.actors {
                    let was = old.actors.iter().find(|o| o.actor == now.actor);
                    if was.is_some_and(|w| w.injury != now.injury)
                        && !dying.contains(&now.actor)
                        && now.actor != a.self_target
                    {
                        beats.push(Beat::Injured {
                            figure: self.figure(now.actor),
                            level: now.injury,
                        });
                    }
                }
            }
            if let Some(line) = tor_client_common::public_plan::changed_line(
                old.and_then(|combat| combat.public_plan.as_ref()),
                combat.public_plan.as_ref(),
            ) {
                beats.push(Beat::PublicPlan(line));
            }
            if old.is_none_or(|c| {
                c.sequence != combat.sequence || c.preparation_active != combat.preparation_active
            }) {
                if let Some(status) = tor_client_common::narration::sequence_status(combat) {
                    beats.push(Beat::SequenceStatus(status));
                }
            }
            if old.is_none_or(|c| c.hp != combat.hp) {
                if let Some(old) = old {
                    beats.push(Beat::Hp {
                        from: old.hp,
                        to: combat.hp,
                        max: combat.max_hp,
                    });
                }
            }
            if combat.victory && !old.is_some_and(|c| c.victory) {
                beats.push(Beat::Victory {
                    terminal: combat.terminal,
                });
            }
            if let Some(goal) = &combat.goal {
                if old.is_none_or(|c| c.goal.as_ref().map(|g| &g.segments) != Some(&goal.segments))
                {
                    beats.push(Beat::Goal(tor_client_common::narration::goal_text(
                        &goal.segments,
                        &crate::controls::command,
                    )));
                }
            }
        }
        let figures = |scene: &Scene| -> BTreeMap<ActorTarget, String> {
            scene
                .of(Kind::Figure)
                .filter_map(|r| match r.key {
                    super::scene::Key::Actor(id) => Some((id, whereabouts(r.position?))),
                    _ => None,
                })
                .collect()
        };
        let (seen, seeing) = (figures(&then), figures(&now));
        for (id, place) in &seeing {
            if !seen.contains_key(id) {
                beats.push(Beat::Appeared {
                    figure: self.figure(*id),
                    whereabouts: place.clone(),
                });
            }
        }
        for id in seen.keys() {
            if !seeing.contains_key(id) {
                beats.push(Beat::Vanished(self.figure(*id)));
            }
        }
        let doors = |o: &Observation| -> BTreeMap<DoorTarget, (bool, Position)> {
            o.visible_cells
                .iter()
                .filter_map(|c| c.door.as_ref().map(|d| (d.id, (d.open, c.position))))
                .collect()
        };
        let old_doors = doors(b);
        for (id, (open, position)) in doors(a) {
            if Some(id) != own_door
                && old_doors
                    .get(&id)
                    .is_some_and(|(was_open, _)| *was_open != open)
            {
                beats.push(Beat::DoorChanged {
                    name: door_name(a, id),
                    whereabouts: whereabouts(position),
                    open,
                });
            }
        }
        beats
    }
}

fn disclosed_figure_name(name: &str, asset: Option<&str>, palette: &Palette) -> String {
    if name.trim().is_empty() {
        tor_client_common::presentation::resolve(palette, asset)
            .map_or("figure", |entry| entry.noun.singular)
            .to_owned()
    } else {
        crate::safe(name).to_lowercase()
    }
}

/// Preserve Text's existing nouns without consulting a newer picture.
fn own_action_name(event: &Event, name: Option<&OutcomeName>) -> String {
    match event {
        Event::Taken { .. } | Event::DoorChanged { .. } => {
            let fallback = if matches!(event, Event::DoorChanged { .. }) {
                "door"
            } else {
                "thing"
            };
            name.filter(|name| !name.text.trim().is_empty())
                .map_or_else(
                    || fallback.into(),
                    |name| crate::safe(&name.text).to_lowercase(),
                )
        }
        Event::Dropped { .. } => name.map_or_else(
            || "thing".into(),
            |name| {
                let text = if name.text.trim().is_empty() {
                    match name.fallback {
                        OutcomeNameFallback::Object => "object",
                        OutcomeNameFallback::Stairs => "stairs",
                        _ => &name.text,
                    }
                } else {
                    &name.text
                };
                crate::safe(text)
            },
        ),
        _ => name.map_or_else(|| "thing".into(), |name| crate::safe(&name.text)),
    }
}

/// Whether the character's own disclosed ability is a plain attack. Names
/// and IDs identify content; they never supply missing source-owned semantics.
#[cfg(test)]
fn basic(o: &Observation, ability: &ContentRef) -> bool {
    o.combat
        .as_ref()
        .and_then(|c| c.own_stats.as_ref())
        .is_some_and(|s| {
            s.abilities
                .iter()
                .any(|a| a.basic_attack && a.id == ability.id)
        })
}

#[cfg(test)]
fn item_name(items: &[ItemView], id: ItemTarget) -> String {
    items
        .iter()
        .find(|i| i.id == id)
        .filter(|i| !i.name.trim().is_empty())
        .map_or_else(|| "thing".into(), |i| crate::safe(&i.name).to_lowercase())
}

#[cfg(test)]
fn door_name(o: &Observation, id: DoorTarget) -> String {
    o.visible_cells
        .iter()
        .filter_map(|c| c.door.as_ref())
        .find(|d| d.id == id)
        .filter(|d| !d.name.trim().is_empty())
        .map_or_else(|| "door".into(), |d| crate::safe(&d.name).to_lowercase())
}

#[cfg(test)]
#[path = "chronicle_tests.rs"]
mod tests;
