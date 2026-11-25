//! Running a turn: each goal's steps against the server, until the game is
//! waiting for this player again.
use std::time::Duration;

use tor_client_common::{ClientState, Connection, Palette};
use tor_protocol::*;

use super::{
    chronicle::{Beat, Chronicler},
    narrate::{End, Entry, Episode, Record},
    prose,
    scene::{direction_name, Key, Kind, Scene},
    verbs::Goal,
};

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// How long to wait for an answer before giving up on the server.
const ANSWER: Duration = Duration::from_secs(10);

/// The connection a turn runs over. Tests use scripted links.
#[allow(async_fn_in_trait)]
pub trait Link {
    fn chart_request(
        &mut self,
        _input: tor_client_common::charts::ChartInput,
    ) -> Result<Option<Request>, String> {
        Err("Chart presentation is unavailable on this connection.".into())
    }
    fn chart_lines(&self) -> Vec<String> {
        Vec::new()
    }
    fn chart_loading(&self) -> bool {
        false
    }
    fn cancel_chart(&mut self) {}

    fn advancement_request(
        &mut self,
        _input: tor_client_common::advancement::AdvancementInput,
    ) -> Result<Option<Request>, String> {
        Err("Advancement is not available on this connection.".into())
    }
    fn advancement_lines(&self) -> Vec<String> {
        Vec::new()
    }
    fn client(&self) -> &ClientState;
    fn palette(&self) -> &Palette;
    fn role(&self) -> AccessRole;
    fn is_synchronized(&self) -> bool;
    fn completed_observation_request(&self, _id: &str, _message: &ServerMessage) -> bool {
        false
    }
    fn set_pace(&mut self, pace: Duration);
    fn pace(&self) -> Duration;
    fn inspection_pages(
        &mut self,
    ) -> Option<&mut tor_client_common::inspection_pages::InspectionPages> {
        None
    }
    fn inspection_page_lines(&self) -> Option<&[String]> {
        None
    }
    /// Send a request; returns its id.
    async fn send(&mut self, request: Request) -> Result<String, Error>;
    /// The next server message, already applied to the client state; `None`
    /// if none arrives within `wait`.
    async fn next(&mut self, wait: Duration) -> Result<Option<ServerMessage>, Error>;
}

impl Link for Connection {
    fn chart_request(
        &mut self,
        input: tor_client_common::charts::ChartInput,
    ) -> Result<Option<Request>, String> {
        self.charts.request(input, &self.state)
    }
    fn chart_lines(&self) -> Vec<String> {
        self.charts.lines(&self.state)
    }
    fn chart_loading(&self) -> bool {
        self.charts.loading()
    }
    fn cancel_chart(&mut self) {
        self.cancel_charts();
    }

    fn advancement_request(
        &mut self,
        input: tor_client_common::advancement::AdvancementInput,
    ) -> Result<Option<Request>, String> {
        self.advancement_pages.request(input, &self.state)
    }
    fn advancement_lines(&self) -> Vec<String> {
        self.advancement_pages
            .display(&self.state.state().observation)
    }
    fn client(&self) -> &ClientState {
        &self.state
    }
    fn palette(&self) -> &Palette {
        &self.palette
    }
    fn role(&self) -> AccessRole {
        Connection::role(self)
    }
    fn is_synchronized(&self) -> bool {
        Connection::is_synchronized(self)
    }
    fn completed_observation_request(&self, id: &str, message: &ServerMessage) -> bool {
        Connection::completed_observation_request(self, id, message)
    }
    fn set_pace(&mut self, pace: Duration) {
        Connection::set_pace(self, pace);
    }
    fn pace(&self) -> Duration {
        Connection::pace(self)
    }
    fn inspection_pages(
        &mut self,
    ) -> Option<&mut tor_client_common::inspection_pages::InspectionPages> {
        Some(&mut self.inspection_pages)
    }
    fn inspection_page_lines(&self) -> Option<&[String]> {
        self.inspection_pages.lines()
    }
    async fn send(&mut self, request: Request) -> Result<String, Error> {
        self.request(request).await
    }
    async fn next(&mut self, wait: Duration) -> Result<Option<ServerMessage>, Error> {
        match tokio::time::timeout(wait, Connection::next(self)).await {
            Ok(message) => message.map(Some),
            Err(_) => Ok(None),
        }
    }
}

/// Hydrate a whole selected chart through pure queries; never send a command or
/// turn disclosed chart coordinates into scenery/action referents.
pub async fn chart(
    link: &mut impl Link,
    chronicler: &mut Chronicler,
    record: &mut Record,
    input: tor_client_common::charts::ChartInput,
) -> Result<(), Error> {
    let request = match link.chart_request(input) {
        Ok(request) => request,
        Err(reason) => {
            record.say(reason);
            return Ok(());
        }
    };
    if let Some(request) = request {
        if !link.is_synchronized() || !link.role().permits(&request) {
            link.cancel_chart();
            record.say("Chart view is not synchronized; no query was sent.");
            return Ok(());
        }
        link.send(request).await?;
        let deadline = tokio::time::Instant::now() + ANSWER;
        let mut reader = Reader {
            chronicler,
            beats: Vec::new(),
            pages: Vec::new(),
            resynced: false,
            waiting: None,
        };
        while link.chart_loading() {
            let before = link.client().state().clone();
            let Some(message) = link
                .next(deadline.saturating_duration_since(tokio::time::Instant::now()))
                .await?
            else {
                link.cancel_chart();
                record.say("Chart display timed out; no action was sent.");
                break;
            };
            reader.record(&before, link, &message);
            if !link.is_synchronized() {
                link.cancel_chart();
                record.say("Chart view changed; stale rows are withheld.");
                break;
            }
            if tokio::time::Instant::now() >= deadline && link.chart_loading() {
                link.cancel_chart();
                record.say("Chart display timed out; no action was sent.");
                break;
            }
        }
        if !reader.beats.is_empty() {
            record.entries.push(Entry::Beats(reader.beats));
        }
        if !reader.pages.is_empty() {
            record.say(reader.pages.join("\n"));
        }
    }
    record.say(link.chart_lines().join("\n"));
    Ok(())
}

/// Query one complete selected action without executing it. Invalidated replies
/// cannot grant confirmation, and the query is never retried as gameplay.
pub async fn preview_weapon(
    link: &mut impl Link,
    chronicler: &mut Chronicler,
    record: &mut Record,
    action: Action,
) -> Result<Option<tor_client_common::weapon_preview::WeaponPreview>, Error> {
    let mut preview =
        match tor_client_common::weapon_preview::WeaponPreview::new(link.client(), action) {
            Ok(value) => value,
            Err(reason) => {
                record.say(reason);
                return Ok(None);
            }
        };
    let request = link.client().command_request(preview.command());
    if !link.role().permits(&request) {
        record.say("Spectator access is read-only.");
        return Ok(None);
    }
    let id = link.send(request).await?;
    preview.bind_request(&id)?;
    let deadline = tokio::time::Instant::now() + ANSWER;
    let mut reader = Reader {
        chronicler,
        beats: Vec::new(),
        pages: Vec::new(),
        resynced: false,
        waiting: None,
    };
    let result = loop {
        if tokio::time::Instant::now() >= deadline {
            record.say("Weapon preview timed out. Choose again; no action was sent.");
            break None;
        }
        let before = link.client().state().clone();
        let Some(message) = link
            .next(deadline.saturating_duration_since(tokio::time::Instant::now()))
            .await?
        else {
            record.say("Weapon preview timed out. Choose again; no action was sent.");
            break None;
        };
        reader.record(&before, link, &message);
        if !link.is_synchronized() || !preview.current(link.client()) {
            record.say("Things changed while previewing. Choose again; no action was sent.");
            break None;
        }
        match message {
            ServerMessage::AbilityWeaponPreview {
                context,
                request_id,
                preview: value,
            } if request_id == id => {
                match preview.accept(link.client(), &context, &request_id, &value) {
                    Ok(()) => {
                        record.say(preview.lines().join("\n"));
                        break Some(preview);
                    }
                    Err(reason) => {
                        record.say(reason);
                        break None;
                    }
                }
            }
            ServerMessage::Error {
                request_id: Some(request_id),
                message,
                code,
                ..
            } if request_id == id => {
                record.say(crate::safe(&message));
                if matches!(
                    code,
                    ErrorCode::StaleRevision | ErrorCode::StaleContext | ErrorCode::WrongBranch
                ) {
                    settle(link, &mut reader, Request::Snapshot, Until::Answered).await?;
                    record.say("Observation refreshed. Choose the exact action again.");
                }
                break None;
            }
            _ => {}
        }
        if tokio::time::Instant::now() >= deadline {
            record.say("Weapon preview timed out. Choose again; no action was sent.");
            break None;
        }
    };
    if !reader.beats.is_empty() {
        record.entries.push(Entry::Beats(reader.beats));
    }
    if !reader.pages.is_empty() {
        record.say(reader.pages.join("\n"));
    }
    Ok(result)
}

/// Obtain one bounded source/ammunition page without retrying or sending gameplay.
pub async fn projectile_choices(
    link: &mut impl Link,
    chronicler: &mut Chronicler,
    record: &mut Record,
    menu: &mut tor_client_common::projectiles::ProjectileMenu,
) -> Result<bool, Error> {
    let request = link.client().command_request(menu.command()?);
    if !link.role().permits(&request) {
        record.say("Spectator access is read-only.");
        return Ok(false);
    }
    let id = link.send(request).await?;
    menu.bind_request(&id)?;
    let deadline = tokio::time::Instant::now() + ANSWER;
    let mut reader = Reader {
        chronicler,
        beats: Vec::new(),
        pages: Vec::new(),
        resynced: false,
        waiting: None,
    };
    let result = loop {
        if tokio::time::Instant::now() >= deadline {
            record.say("Projectile choices timed out. Choose again; no action was sent.");
            break false;
        }
        let before = link.client().state().clone();
        let Some(message) = link
            .next(deadline.saturating_duration_since(tokio::time::Instant::now()))
            .await?
        else {
            record.say("Projectile choices timed out. Choose again; no action was sent.");
            break false;
        };
        reader.record(&before, link, &message);
        if !link.is_synchronized() || !menu.current(link.client()) {
            record.say(
                "Things changed while choosing a projectile. Choose again; no action was sent.",
            );
            break false;
        }
        match message {
            ServerMessage::ProjectileChoices {
                context,
                request_id,
                page: value,
            } if request_id == id => {
                match menu.accept(link.client(), &context, &request_id, &value) {
                    Ok(()) => {
                        record.say(crate::projectiles::lines(menu).join("\n"));
                        break true;
                    }
                    Err(reason) => {
                        record.say(reason);
                        break false;
                    }
                }
            }
            ServerMessage::Error {
                request_id: Some(request_id),
                message,
                code,
                ..
            } if request_id == id => {
                record.say(crate::safe(&message));
                if matches!(
                    code,
                    ErrorCode::StaleRevision | ErrorCode::StaleContext | ErrorCode::WrongBranch
                ) {
                    settle(link, &mut reader, Request::Snapshot, Until::Answered).await?;
                    record.say("Observation refreshed. Choose the exact action again.");
                }
                break false;
            }
            _ => {}
        }
        if tokio::time::Instant::now() >= deadline {
            record.say("Projectile choices timed out. Choose again; no action was sent.");
            break false;
        }
    };
    if !reader.beats.is_empty() {
        record.entries.push(Entry::Beats(reader.beats));
    }
    if !reader.pages.is_empty() {
        record.say(reader.pages.join("\n"));
    }
    Ok(result)
}

pub use tor_client_common::compositor::Settled;
use tor_client_common::compositor::{SettleBoundary, Until};

/// Reads updates for a turn, recording beats as they come.
pub struct Reader<'a> {
    pub chronicler: &'a mut Chronicler,
    pub beats: Vec<Beat>,
    /// History pages answered during the turn, as text.
    pub pages: Vec<String>,
    pub resynced: bool,
    /// What the server last said play waits for.
    pub waiting: Option<Waiting>,
}

impl Reader<'_> {
    pub(super) fn execution_refused(&self) -> bool {
        self.beats
            .iter()
            .any(|beat| matches!(beat, Beat::ExecutionRefused(_)))
    }

    /// Apply one message and record its beats.
    pub fn record(&mut self, _before: &StateView, link: &impl Link, message: &ServerMessage) {
        let after = link.client().state();
        match message {
            ServerMessage::Update { update } => match &update.body {
                UpdateBody::Outcome { facts, .. } => {
                    self.beats
                        .extend(self.chronicler.outcome(facts, link.palette()));
                }
                UpdateBody::Observation { .. } | UpdateBody::ObservationDelta { .. } => {
                    self.chronicler.learn(after, link.palette());
                }
                UpdateBody::Travel { status, .. } if status.phase != TravelPhase::Active => {
                    self.beats.push(Beat::Journey {
                        phase: status.phase,
                    });
                }
                UpdateBody::Control { has_control } => {
                    self.beats.push(Beat::Control(*has_control));
                }
                UpdateBody::Readiness { .. }
                | UpdateBody::Travel { .. }
                | UpdateBody::Annotation { .. }
                | UpdateBody::Intention { .. } => {}
            },
            ServerMessage::Snapshot { .. } => {
                self.chronicler.reset_baseline(after, link.palette());
                self.resynced = true;
                self.beats.push(Beat::Resync);
            }
            ServerMessage::Error { .. } if link.is_synchronized() => {
                if let Some(reason) = link.client().queued_execution_notice(message) {
                    self.beats.push(Beat::ExecutionRefused(crate::safe(reason)));
                }
            }
            ServerMessage::WizardReport { report, .. } => {
                self.pages.push(report.title.clone());
                self.pages.extend(report.lines.iter().cloned());
            }
            ServerMessage::CreatureCatalog { page, .. } => {
                match tor_client_common::creature_catalog::lines(page) {
                    Ok(rows) => self.pages.extend(rows),
                    Err(_) => self.pages.push("Invalid creature catalog.".into()),
                }
            }
            ServerMessage::CreatureStrikePage { .. } => {
                if let Some(rows) = link.inspection_page_lines() {
                    self.pages.extend(rows.iter().cloned());
                } else {
                    self.pages.push("Strike detail was not requested or is stale. Reopen the creature inspection.".into());
                }
            }
            ServerMessage::AdvancementPage { .. } => self.pages.extend(link.advancement_lines()),
            ServerMessage::CreatureInspection { report, .. } => {
                match tor_client_common::inspection::lines(report) {
                    Ok(rows) => self.pages.extend(rows),
                    Err(_) => self
                        .pages
                        .push("Invalid creature inspection report.".into()),
                }
            }
            ServerMessage::History { page, .. } => {
                if page.entries.is_empty() {
                    self.pages.push("There is no history to show.".into());
                }
                for entry in &page.entries {
                    self.pages.push(crate::history(entry));
                }
                if let Some(before) = &page.older_before {
                    self.pages
                        .push(format!("Older entries: history {}", crate::safe(&before.0)));
                }
            }
            _ => {}
        }
    }
}

fn terminal(state: &ClientState) -> bool {
    state
        .outcome_status()
        .is_some_and(|status| status.terminal || status.dead)
}

/// Send `request` and read until it's finished as `until` says.
async fn settle(
    link: &mut impl Link,
    reader: &mut Reader<'_>,
    request: Request,
    until: Until,
) -> Result<Settled, Error> {
    let observing = matches!(request, Request::Observe);
    let id = link.send(request).await?;
    let mut boundary = SettleBoundary::new(id.clone(), until, observing);
    loop {
        let before = link.client().state().clone();
        let Some(message) = link.next(ANSWER).await? else {
            return boundary
                .silence(link.client())
                .map_err(|silence| silence.message().into());
        };
        reader.record(&before, link, &message);
        if !link.is_synchronized() {
            continue;
        }
        let completed = observing && link.completed_observation_request(&id, &message);
        let settled = boundary.observe(
            link.client(),
            completed,
            reader.execution_refused(),
            &message,
        );
        // Neither a finished observation nor a refusal notice records a wait.
        if let ServerMessage::Waiting { on } = &message {
            if !completed && settled != Some(Settled::Deferred) {
                reader.waiting = Some(*on);
            }
        }
        if let Some(settled) = settled {
            return Ok(settled);
        }
    }
}

/// One step of a goal, decided from the scene in hand.
enum Step {
    Act(Action),
    Travel(String),
    Finish(End),
}

fn step(goal: &Goal, scene: &Scene, acted: bool, approached: bool) -> Step {
    if acted {
        return Step::Finish(End::Done);
    }
    let act = Step::Act;
    let travel = |key: Key| match scene.approach(key) {
        Some(cell) => Step::Travel(cell),
        None => Step::Finish(End::Refused("You can't see a way to get there.".into())),
    };
    match goal {
        Goal::MagicItem { action } => {
            let view = &scene.state.observation;
            let current = match action {
                Action::Zap { item, aim } => match aim {
                    tor_protocol::DeviceAim::Direction { direction } => tor_client_common::items::zap_action(view, *item, *direction),
                    tor_protocol::DeviceAim::Cell { offset } => tor_client_common::items::zap_cell_action(view, *item, *offset),
                    tor_protocol::DeviceAim::SelfTarget {} => tor_client_common::items::item_action(view, *item, tor_client_common::items::ItemOperation::Zap),
                },
                Action::Identify { choice, item } => tor_client_common::items::identification_action(view, *item).and_then(|current| {
                    if matches!(&current, Action::Identify { choice: active, .. } if active == choice) { Ok(current) } else { Err("That identification choice is no longer available.".into()) }
                }),
                _ => Err("That item action is unavailable.".into()),
            };
            match current {
                Ok(action) => act(action),
                Err(reason) => Step::Finish(End::Refused(reason)),
            }
        }
        Goal::Escape { hold } => {
            if scene
                .state
                .observation
                .linked_holds
                .iter()
                .any(|offer| offer.target == *hold)
            {
                act(Action::Escape { hold: *hold })
            } else {
                Step::Finish(End::Refused("That hold is no longer available.".into()))
            }
        }
        Goal::UseItem { item, operation } => {
            match tor_client_common::items::item_action(&scene.state.observation, *item, *operation)
            {
                Ok(action) => act(action),
                Err(reason) => Step::Finish(End::Refused(reason)),
            }
        }
        Goal::UseAbility {
            ability,
            invocation,
            supplies,
        } => {
            match tor_client_common::abilities::invocation_action(
                &scene.state.observation,
                ability,
                *invocation,
                supplies.clone(),
            ) {
                Ok(action) => act(action),
                Err(reason) => Step::Finish(End::Refused(reason)),
            }
        }
        Goal::Take { item, quantity } => match scene.get(Key::Item(*item)) {
            Some(r) if r.reachable => act(Action::Take {
                item: *item,
                quantity: *quantity,
            }),
            Some(_) if !approached => travel(Key::Item(*item)),
            _ => Step::Finish(End::Gone),
        },
        Goal::Drop { item, quantity } => act(Action::Drop {
            item: *item,
            quantity: *quantity,
        }),
        Goal::ExamineTrap { trap } => match scene.get(Key::Trap(*trap)) {
            Some(referent) if referent.reachable => act(Action::ExamineTrap { trap: *trap }),
            _ => Step::Finish(End::Refused(
                "Come within reach to examine the trap's mechanism.".into(),
            )),
        },
        Goal::Door { door, open } => match scene.get(Key::Door(*door)) {
            Some(r) if r.open == Some(*open) => Step::Finish(End::Done),
            Some(r) if r.reachable => act(Action::SetDoor {
                door: *door,
                open: *open,
            }),
            Some(_) if !approached => travel(Key::Door(*door)),
            _ => Step::Finish(End::Gone),
        },
        Goal::Attack { target } => match scene.get(Key::Actor(*target)) {
            Some(r) if r.reachable || approached => {
                match tor_client_common::abilities::basic_action(&scene.state.observation, *target)
                {
                    Ok(action) => act(action),
                    Err(reason) => Step::Finish(End::Refused(reason)),
                }
            }
            Some(_) => match scene.approach(Key::Actor(*target)) {
                Some(cell) => Step::Travel(cell),
                None => match tor_client_common::abilities::basic_action(
                    &scene.state.observation,
                    *target,
                ) {
                    Ok(action) => act(action),
                    Err(reason) => Step::Finish(End::Refused(reason)),
                },
            },
            None => Step::Finish(End::Gone),
        },
        Goal::Approach { target } if !approached => travel(*target),
        Goal::Go { destination, .. } | Goal::Visit { destination, .. } if !approached => {
            Step::Travel(destination.clone())
        }
        Goal::Approach { .. } | Goal::Go { .. } | Goal::Visit { .. } => Step::Finish(End::Done),
        Goal::Step { direction } => act(Action::Move {
            direction: *direction,
        }),
        Goal::Wait => act(Action::Wait),
    }
}

/// How the goal's object is named as it begins.
fn object(goal: &Goal, scene: &Scene) -> String {
    let named = |key: Key| scene.get(key).map(|r| r.the());
    match goal {
        Goal::Take { item, quantity } | Goal::Drop { item, quantity } => {
            match (scene.get(Key::Item(*item)), quantity) {
                // A count is "two arrows" unless it's every alike thing there
                // is, whatever stacks they're in.
                (Some(r), Some(q))
                    if *q
                        < scene
                            .referents
                            .iter()
                            .filter(|o| o.identity == r.identity)
                            .map(|o| o.quantity)
                            .sum::<u64>() =>
                {
                    Some(prose::counted(*q, &r.name))
                }
                (Some(r), _) => Some(r.the()),
                (None, _) => None,
            }
        }
        Goal::UseItem { item, .. } => named(Key::Item(*item)),
        Goal::ExamineTrap { trap } => named(Key::Trap(*trap)),
        Goal::Door { door, .. } => named(Key::Door(*door)),
        Goal::Attack { target }
        | Goal::UseAbility {
            invocation:
                tor_protocol::AbilityInvocation {
                    aim: tor_protocol::AbilityAim::ActorTarget { target },
                    ..
                },
            ..
        } => named(Key::Actor(*target)),
        Goal::UseAbility {
            invocation:
                tor_protocol::AbilityInvocation {
                    aim: tor_protocol::AbilityAim::SelfStep { direction },
                    ..
                }
                | tor_protocol::AbilityInvocation {
                    aim: tor_protocol::AbilityAim::DirectionalRay { direction },
                    ..
                },
            ..
        } => Some(direction_name(*direction).to_owned()),
        Goal::UseAbility {
            invocation:
                tor_protocol::AbilityInvocation {
                    aim: tor_protocol::AbilityAim::DirectionalProjectile { direction, .. },
                    ..
                },
            ..
        } => Some(direction_name(*direction).to_owned()),
        Goal::UseAbility {
            invocation:
                tor_protocol::AbilityInvocation {
                    aim:
                        tor_protocol::AbilityAim::CellRay { offset }
                        | tor_protocol::AbilityAim::CellProjectile { offset, .. },
                    ..
                },
            ..
        } => Some(format!("cell {}, {}, {}", offset[0], offset[1], offset[2])),
        Goal::Approach { target } => named(*target),
        Goal::Visit { name, .. } => Some(name.clone()),
        Goal::Go { direction, .. } | Goal::Step { direction } => {
            Some(direction_name(*direction).to_owned())
        }
        Goal::Escape { .. } => Some("the hold".into()),
        Goal::MagicItem {
            action: Action::Identify {
                item: Some(item), ..
            },
        } => named(Key::Item(*item)),
        Goal::MagicItem { .. } | Goal::Wait => None,
    }
    .unwrap_or_else(|| "it".into())
}

/// How many things a take or drop is of, and their singular name.
fn counted(goal: &Goal, scene: &Scene) -> Option<(u64, String)> {
    match goal {
        Goal::Take { item, quantity } | Goal::Drop { item, quantity } => {
            let r = scene.get(Key::Item(*item))?;
            let n = quantity.map_or(r.quantity, |q| q.min(r.quantity));
            Some((n, r.name.clone()))
        }
        _ => None,
    }
}

fn refusal(code: ErrorCode, goal: &Goal) -> String {
    match code {
        ErrorCode::InvalidAction => match goal {
            Goal::Take { .. } => "You can't pick that up from here.",
            Goal::Drop { .. } => "You can't drop that here.",
            Goal::UseItem { .. } => "You can't use that item right now.",
            Goal::ExamineTrap { .. } => "You can't examine that trap right now.",
            Goal::Door { .. } => "You can't reach it from here.",
            Goal::Attack { .. } => "You can't reach it from here.",
            Goal::UseAbility { .. } => "You can't use that ability right now.",
            Goal::Step { .. } => "You can't go that way.",
            Goal::Go { .. } | Goal::Approach { .. } | Goal::Visit { .. } => {
                "You can't find a way there."
            }
            Goal::Escape { .. } => "That hold is no longer available.",
            Goal::MagicItem { .. } => "That item action is no longer available.",
            Goal::Wait => "You can't wait right now.",
        },
        ErrorCode::NotController | ErrorCode::ControlTaken => {
            "Another player has control. You are only watching."
        }
        ErrorCode::ActorBusy => "You are still on your way.",
        ErrorCode::Unauthorized => "You aren't allowed to do that.",
        ErrorCode::StaleRevision | ErrorCode::WrongBranch => {
            "Things have changed. Look around and try again."
        }
        ErrorCode::StorageFailure => "The game couldn't save that, so it didn't happen.",
        _ => "That couldn't be done.",
    }
    .into()
}

pub(super) fn waiting_reason(waiting: Option<Waiting>) -> &'static str {
    match waiting {
        Some(Waiting::Unclaimed) => {
            "Another character is to act first, and no one is controlling it."
        }
        Some(Waiting::Stopped) => "Nothing more can happen in this game.",
        _ => "Someone else is to act first.",
    }
}

/// Whether this connection may act, or why not.
pub fn may_act(link: &impl Link) -> Result<(), &'static str> {
    if !link.is_synchronized() {
        Err("Resynchronizing; input was not sent.")
    } else if link.role() == AccessRole::Spectator {
        Err("Spectator access is read-only.")
    } else if !link.client().has_control() {
        Err("You are observing. Use control to take over when it is available.")
    } else if terminal(link.client()) {
        Err("This run has ended. You can still look around, read history, or quit.")
    } else {
        Ok(())
    }
}

/// Run one goal to its end, adding its episode to the record. Returns
/// whether it was done, so the chain goes on.
pub async fn run_goal(
    link: &mut impl Link,
    chronicler: &mut Chronicler,
    record: &mut Record,
    goal: Goal,
) -> Result<bool, Error> {
    let start = Scene::new(link.client().state(), link.palette());
    let object = object(&goal, &start);
    let counted = counted(&goal, &start);
    let known = start.figure_ids();
    // What a walk has seen already, so only what's new stops it.
    let seen: std::collections::BTreeSet<String> = crate::adventure::sights(start.state)
        .into_iter()
        .map(|(id, ..)| id)
        .collect();
    drop(start);
    let mut episode = Episode {
        goal: goal.clone(),
        object,
        counted,
        approached: false,
        beats: Vec::new(),
        end: End::Done,
    };
    let mut reader = Reader {
        chronicler,
        beats: Vec::new(),
        pages: Vec::new(),
        resynced: false,
        waiting: None,
    };
    // After reconnecting during recovery, play resumes before anything new.
    if (!link.client().character_ready() || !link.client().can_admit_intention())
        && link
            .client()
            .travel()
            .is_none_or(|t| t.phase != TravelPhase::Active)
    {
        let resumed = settle(link, &mut reader, Request::Continue, Until::Ready).await?;
        if let Settled::Failed(reason) = resumed {
            episode.end = End::Refused(reason.message());
        } else if resumed == Settled::Lost {
            episode.end = End::Lost;
        } else if resumed == Settled::Deferred {
            episode.end = End::Deferred;
        } else if resumed == Settled::Pending {
            episode.end = End::Pending;
        } else if matches!(resumed, Settled::Waiting(_)) {
            episode.end = End::Waiting;
        } else if !link.client().character_ready() || !link.client().can_admit_intention() {
            // Say why play won't come round to this character.
            episode.end = End::Refused(waiting_reason(reader.waiting).into());
        }
    }
    let mut acted = false;
    while episode.end == End::Done {
        let scene = Scene::new(link.client().state(), link.palette());
        let next = step(&goal, &scene, acted, episode.approached);
        let revision = scene.state.revision;
        let branch = link.client().branch().clone();
        drop(scene);
        match next {
            Step::Finish(end) => {
                episode.end = end;
                break;
            }
            Step::Act(action) => {
                let request = Request::Command {
                    context: link.client().input_context(),
                    branch,
                    command: Command::Act { action },
                };
                match settle(link, &mut reader, request, Until::Acted).await? {
                    Settled::Done | Settled::Journey(_) => acted = true,
                    Settled::Waiting(_) => episode.end = End::Waiting,
                    Settled::Pending => episode.end = End::Pending,
                    Settled::Deferred => episode.end = End::Deferred,
                    Settled::Failed(reason) => episode.end = End::Refused(reason.message()),
                    Settled::Rejected { code, detail } => {
                        // A figure that moved off while you closed in is out
                        // of reach, not unreachable.
                        let now = Scene::new(link.client().state(), link.palette());
                        let away = matches!(goal, Goal::Attack { target }
                            if code == ErrorCode::InvalidAction
                                && now.get(Key::Actor(target)).is_some_and(|r| !r.reachable));
                        let fallback = || {
                            if away {
                                OUT_OF_REACH.into()
                            } else {
                                refusal(code, &goal)
                            }
                        };
                        let text = if code == ErrorCode::InvalidAction
                            && matches!(goal, Goal::UseAbility { .. } | Goal::UseItem { .. })
                        {
                            detail.map_or_else(fallback, |message| crate::safe(&message))
                        } else {
                            fallback()
                        };
                        episode.end = End::Refused(text);
                    }
                    Settled::Lost => episode.end = End::Lost,
                }
                if episode.end == End::Done
                    && !link.client().observation_current()
                    && settle(link, &mut reader, Request::Observe, Until::Answered).await?
                        == Settled::Lost
                {
                    episode.end = End::Lost;
                }
            }
            Step::Travel(destination) => {
                let (mut destination, mut branch, mut revision) = (destination, branch, revision);
                let mut legs = 0;
                let mut hazards = 0;
                let hp_start = hp(link);
                loop {
                    let before = super::seen(link.client());
                    let request = Request::Command {
                        context: link.client().input_context(),
                        branch,
                        command: Command::Travel {
                            expected_revision: revision,
                            destination,
                        },
                    };
                    episode.approached = true;
                    let mut next = None;
                    let settled = settle(link, &mut reader, request, Until::Journey).await?;
                    let waiting = matches!(settled, Settled::Waiting(_));
                    if !matches!(settled, Settled::Lost | Settled::Deferred)
                        && !link.client().observation_current()
                        && settle(link, &mut reader, Request::Observe, Until::Answered).await?
                            == Settled::Lost
                    {
                        episode.end = End::Lost;
                        break;
                    }
                    // A yielded journey can still have an authoritative ending.
                    // Tell that outcome without treating it as permission for
                    // another leg, the goal's action, or the remaining chain.
                    let outcome = match settled {
                        Settled::Waiting(Some(phase)) => Settled::Journey(phase),
                        other => other,
                    };
                    match outcome {
                        // A walk in a direction goes on while there's nothing
                        // to see, and stops at the first thing that comes into
                        // view.
                        Settled::Journey(TravelPhase::Arrived)
                            if !waiting
                                && matches!(goal, Goal::Go { .. })
                                && Scene::new(link.client().state(), link.palette())
                                    .figure_ids()
                                    .is_subset(&known) =>
                        {
                            legs += 1;
                            let Goal::Go { direction, .. } = &episode.goal else {
                                unreachable!("a walk")
                            };
                            // What's new is what's in sight; where to go on
                            // is read from what's remembered too.
                            let new = crate::adventure::sights(link.client().state())
                                .into_iter()
                                .find(|(id, ..)| !seen.contains(id));
                            if let Some((_, what, whereabouts)) = new {
                                reader.beats.push(Beat::Spotted { what, whereabouts });
                            } else if legs < MAX_LEGS {
                                let now = super::seen(link.client());
                                if let Some((way, to)) =
                                    crate::adventure::onward(&before, &now, *direction)
                                {
                                    episode.goal = Goal::Go {
                                        direction: way,
                                        destination: to.clone(),
                                    };
                                    next = Some(to);
                                }
                            }
                        }
                        Settled::Journey(TravelPhase::Arrived) => {
                            // Arrival never authorizes what comes next if someone
                            // new came into view.
                            let now = Scene::new(link.client().state(), link.palette());
                            let newcomer = now.figure_ids().difference(&known).next().is_some();
                            let target_is_figure = matches!(goal, Goal::Attack { .. })
                                || matches!(goal, Goal::Approach { target } if now.get(target).is_some_and(|r| r.is(Kind::Figure)));
                            if newcomer
                                && !matches!(
                                    goal,
                                    Goal::Go { .. } | Goal::Approach { .. } | Goal::Visit { .. }
                                )
                                && !target_is_figure
                            {
                                episode.end = End::Wary;
                            }
                        }
                        // Closing in on a figure that steps up to meet you: the
                        // way is "blocked" because it's here, so attack it, or
                        // you're where you meant to be.
                        Settled::Journey(TravelPhase::Blocked)
                            if !waiting
                                && figure_target(&goal).is_some_and(|target| {
                                    Scene::new(link.client().state(), link.palette())
                                        .get(Key::Actor(target))
                                        .is_some_and(|r| r.reachable)
                                }) => {}
                        // Stopped short of a figure for no reason in sight:
                        // nothing new came into view and nothing hurt. It
                        // moved, so close in on where it is now.
                        Settled::Journey(TravelPhase::Hazard)
                            if !waiting
                                && hazards < HAZARD_RETRIES
                                && hp(link) >= hp_start
                                && figure_target(&goal).is_some()
                                && Scene::new(link.client().state(), link.palette())
                                    .figure_ids()
                                    .is_subset(&known) =>
                        {
                            hazards += 1;
                            let target = figure_target(&goal).expect("checked");
                            let now = Scene::new(link.client().state(), link.palette());
                            match now.get(Key::Actor(target)) {
                                Some(r) if r.reachable => {}
                                Some(_) => next = now.approach(Key::Actor(target)),
                                None => episode.end = End::Gone,
                            }
                        }
                        Settled::Journey(phase) => {
                            if phase == TravelPhase::Blocked {
                                // Name who's in the way, when someone is.
                                let now = Scene::new(link.client().state(), link.palette());
                                let blocker = now.of(Kind::Figure).find(|r| r.reachable);
                                if let Some(r) = blocker {
                                    if let Key::Actor(id) = r.key {
                                        reader.beats.push(Beat::Barred(super::chronicle::Figure {
                                            id,
                                            name: r.name.clone(),
                                        }));
                                    }
                                }
                            }
                            episode.end = End::Stopped(phase);
                        }
                        Settled::Done => episode.end = End::Stopped(TravelPhase::Active),
                        Settled::Waiting(_) => episode.end = End::Waiting,
                        Settled::Pending => episode.end = End::Pending,
                        Settled::Deferred => episode.end = End::Deferred,
                        Settled::Failed(reason) => episode.end = End::Refused(reason.message()),
                        Settled::Rejected { code, .. } => {
                            episode.approached = false;
                            episode.end = End::Refused(refusal(code, &goal));
                        }
                        Settled::Lost => episode.end = End::Lost,
                    }
                    if waiting && episode.end == End::Done {
                        episode.end = End::Waiting;
                    }
                    match next {
                        Some(to) => {
                            destination = to;
                            branch = link.client().branch().clone();
                            revision = link.client().state().revision;
                        }
                        None => break,
                    }
                }
            }
        }
    }
    let done = episode.end == End::Done;
    if episode.end == End::Waiting && reader.beats.is_empty() {
        record.say(waiting_reason(reader.waiting));
    }
    episode.beats = std::mem::take(&mut reader.beats);
    let resynced = reader.resynced;
    record.entries.push(Entry::Episode(Box::new(episode)));
    Ok(done && !resynced)
}

/// How often an approach to a figure goes on after an unexplained stop.
const HAZARD_RETRIES: usize = 2;

/// The figure a goal closes in on, if it closes in on one.
fn figure_target(goal: &Goal) -> Option<ActorTarget> {
    match goal {
        Goal::Attack { target } => Some(*target),
        Goal::Approach {
            target: Key::Actor(target),
        } => Some(*target),
        _ => None,
    }
}

fn hp(link: &impl Link) -> u64 {
    link.client().outcome_status().map_or(0, |status| status.hp)
}

/// Why an attack ended when its target moved off as the character closed in.
pub const OUT_OF_REACH: &str = "It keeps out of reach.";

/// The most journeys one walk in a direction makes before stopping anyway.
const MAX_LEGS: usize = 12;

/// Send a session request and record what comes back.
pub async fn run_request(
    link: &mut impl Link,
    chronicler: &mut Chronicler,
    record: &mut Record,
    request: Request,
) -> Result<bool, Error> {
    if !link.role().permits(&request) {
        record.say("Spectator access is read-only.");
        return Ok(false);
    }
    let refresh_renamed_place = matches!(
        &request,
        Request::Command {
            command: Command::RenamePlace { .. },
            ..
        }
    );
    let mut reader = Reader {
        chronicler,
        beats: Vec::new(),
        pages: Vec::new(),
        resynced: false,
        waiting: None,
    };
    let mut settled = settle(link, &mut reader, request, Until::Answered).await?;
    if refresh_renamed_place && settled == Settled::Done {
        // The committed receipt can precede its demanded picture. Do not
        // confirm the rename or accept another local `places` command until
        // this coalesced demand's correlated observation has been applied.
        settled = settle(link, &mut reader, Request::Observe, Until::Answered).await?;
    };

    let beats = std::mem::take(&mut reader.beats);
    let pages = std::mem::take(&mut reader.pages);
    if !beats.is_empty() {
        record.entries.push(Entry::Beats(beats));
    }
    if !pages.is_empty() {
        record.say(pages.join("\n"));
    }
    if let Settled::Failed(reason) = settled {
        record.say(reason.message());
        return Ok(false);
    }
    if let Settled::Rejected { code, .. } = settled {
        record.say(match code {
            ErrorCode::NotController | ErrorCode::ControlTaken => {
                "Another player has control. You are only watching."
            }
            ErrorCode::Unauthorized => "You aren't allowed to do that.",
            ErrorCode::StaleRevision | ErrorCode::WrongBranch => {
                "Things have changed. Look around and try again."
            }
            ErrorCode::StorageFailure => "The game couldn't save that.",
            ErrorCode::InvalidAnchor | ErrorCode::InvalidAnnotation => {
                "That note can't be attached there."
            }
            _ => "That couldn't be done.",
        });
        return Ok(false);
    }
    Ok(settled == Settled::Done)
}
