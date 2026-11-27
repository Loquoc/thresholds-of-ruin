#[path = "attachment_session.rs"]
mod attachment_session;
use crate::engine::{valid_label, OwnerIntentionFailure};
use crate::{Engine, Failure};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use tokio::sync::watch;
use tor_protocol::*;

#[path = "chart_page_session.rs"]
mod chart_pages;
#[path = "session_commands.rs"]
mod commands;
#[path = "map_page_session.rs"]
mod map_pages;
#[path = "presentation_history.rs"]
mod presentation_history;
use presentation_history::{IssuedPresentation, PresentationHistory};

#[cfg(feature = "service-contract-host")]
#[path = "service_contract_host.rs"]
pub(crate) mod contract_host;

/// Trusted startup authority for an authenticated connection.
///
/// [`serve`](crate::serve) matches the token and validates the account configuration;
/// the client cannot choose its user, role or authorized actors. Attachment alone
/// does not grant an actor's exclusive control lease. Tokens never enter game history.
#[derive(Clone)]
pub struct Account {
    /// Authenticated receipt namespace, shared across this user's frontends.
    pub user: String,
    /// Nonempty secret used for the initial loopback handshake. Do not log it.
    pub token: String,
    /// Server-assigned permission class, independent of the frontend label.
    pub role: AccessRole,
    /// Actors this account may attach to; attachment fixes one viewing perspective.
    pub actors: BTreeSet<ActorId>,
}

/// Keep the sharing fast path beside the restriction it must exactly match.
/// Presence, including a known false curse/protection fact, is private knowledge.
fn has_private_item_disclosure(state: &StateView) -> bool {
    !state.observation.known_item_uses.is_empty()
        || state
            .observation
            .inventory
            .iter()
            .chain(state.observation.ground_items.iter().map(|row| &row.item))
            .any(|item| {
                item.identity != ItemClass::Misc.unknown_identity()
                    || item.knowledge != ItemKnowledge::default()
            })
        || state.observation.interactions.as_ref().is_some_and(|view| {
            view.identification.is_some()
                || !view.curse_blocked.is_empty()
                || view.inventory.iter().any(|item| {
                    item.charges.is_some()
                        || item.known_equipment.is_some()
                        || item.known_projectile_bonus.is_some()
                        || item.known_cursed.is_some()
                        || item.known_protected.is_some()
                })
        })
}

/// Ordinary preparation remains visible; paid selection, exact-instance counts,
/// and learned equipment facts belong only to account-authorized owner attachments.
fn restrict_item_disclosure(state: &mut StateView, role: AccessRole) {
    if role == AccessRole::Spectator {
        state.observation.known_item_uses.clear();
        // Machine identity and learned instance facts are observer-owned. Keep
        // legacy public names, identified flags and currency/supply affordances;
        // identified does not imply a disclosed concrete machine identity.
        for item in state.observation.inventory.iter_mut().chain(
            state
                .observation
                .ground_items
                .iter_mut()
                .map(|row| &mut row.item),
        ) {
            item.identity = ItemClass::Misc.unknown_identity();
            item.knowledge = ItemKnowledge::default();
        }
        if let Some(interaction) = &mut state.observation.interactions {
            interaction.identification = None;
            interaction.curse_blocked.clear();
            for item in &mut interaction.inventory {
                item.charges = None;
                item.known_equipment = None;
                item.known_projectile_bonus = None;
                item.known_cursed = None;
                item.known_protected = None;
            }
        }
    }
}

fn semantic_item_facts(
    capture: &crate::engine::SemanticObservation,
    role: AccessRole,
) -> std::borrow::Cow<'_, OutcomeObservation> {
    let mut facts = std::borrow::Cow::Borrowed(&capture.facts);
    if role == AccessRole::Spectator
        && facts
            .interactions
            .as_ref()
            .is_some_and(|view| !view.curse_blocked.is_empty())
    {
        facts
            .to_mut()
            .interactions
            .as_mut()
            .expect("restricted interaction")
            .curse_blocked
            .clear();
    }
    facts
}

#[cfg(test)]
thread_local! {
    /// Before-captures served from the previous outcome's after-captures.
    pub(crate) static REUSED_SEMANTIC_CAPTURES: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

fn reused_semantic_capture() {
    #[cfg(test)]
    REUSED_SEMANTIC_CAPTURES.with(|count| count.set(count.get() + 1));
}

/// Ephemeral semantic capture shared by all attachments watching each actor.
/// Receipt provenance is private and never encoded into an outcome or snapshot.
#[derive(Clone)]
struct SemanticStates {
    views: BTreeMap<ActorId, Arc<crate::engine::SemanticObservation>>,
    events: EventProvenance,
}

/// Canonical fallback ownership belongs to the observer, not each connection.
/// These two pictures live only through one action's publication.
struct SemanticPicturePair {
    before: Arc<StateView>,
    after: Arc<StateView>,
}

struct DisclosedObservation {
    branch: BranchId,
    base: ObservationBase,
    state: Arc<StateView>,
}

/// Ownership is part of the generation even when permission vectors stay empty.
#[derive(Clone, Debug, PartialEq, Eq)]
struct DisclosedReadiness {
    // Private monotonic facts contribute to the already public authority revision.
    engine_input_generation: u64,
    control_generation: u64,
    has_control: bool,
    permissions: Readiness,
}

/// Payloads prepared by a request and published after its ordered effects.
enum RequestReply {
    ProjectileChoices(Box<ProjectileChoicePage>),
    AbilityWeaponPreview(Box<tor_protocol::AbilityWeaponPreview>),
    Receipt(RequestReceipt),
    History(HistoryPage),
    Discoveries(Vec<tor_protocol::ItemDiscoveryView>),
    Palette(PaletteUpdate),
    CreatureInspection(Box<CreatureInspectionView>),
    CreatureCatalog(Box<CreatureCatalogPage>),
    WizardReport(Box<tor_protocol::WizardReport>),
    StrikePage(Box<CreatureStrikePage>),
    AdvancementPage(Box<AdvancementPage>),
    CombatDiagnostics(Box<tor_protocol::CombatDiagnosticsView>),
}

impl RequestReply {
    fn message(self, request_id: String, context: ReplyContext) -> ServerMessage {
        match self {
            Self::ProjectileChoices(page) => ServerMessage::ProjectileChoices {
                context,
                request_id,
                page,
            },
            Self::AbilityWeaponPreview(preview) => ServerMessage::AbilityWeaponPreview {
                context,
                request_id,
                preview,
            },
            Self::Receipt(receipt) => ServerMessage::Ack {
                context,
                request_id,
                receipt,
            },
            Self::Discoveries(rows) => ServerMessage::Discoveries {
                context,
                request_id,
                rows,
            },
            Self::History(page) => ServerMessage::History {
                context,
                request_id,
                page,
            },
            Self::CombatDiagnostics(report) => ServerMessage::CombatDiagnostics {
                context,
                request_id,
                report,
            },
            Self::AdvancementPage(page) => ServerMessage::AdvancementPage {
                context,
                request_id,
                page,
            },
            Self::StrikePage(page) => ServerMessage::CreatureStrikePage {
                context,
                request_id,
                page,
            },
            Self::WizardReport(report) => ServerMessage::WizardReport {
                context,
                request_id,
                report,
            },
            Self::CreatureCatalog(page) => ServerMessage::CreatureCatalog {
                context,
                request_id,
                page,
            },
            Self::CreatureInspection(report) => ServerMessage::CreatureInspection {
                context,
                request_id,
                report,
            },
            Self::Palette(palette) => ServerMessage::Palette {
                context,
                request_id: Some(request_id),
                palette,
            },
        }
    }
}

enum ProcessedRequest {
    Reply(RequestReply),
    /// Map cursors advance only after their correlated page is admitted.
    Map(Box<map_pages::MapTransfer>),
    Chart(Box<chart_pages::ChartTransfer>),
    /// Snapshot publication owns the stream reset; no additional reply is due.
    Published,
    /// Persistence owns completion; poll it at the common publication boundary.
    SaveQueued,
}

/// One ephemeral read capability, discarded on reset/disconnect or replacement.
/// This is not saved, an access grant, a journal receipt, or simulation randomness.
struct StrikeInspectionHandle {
    id: StrikeInspectionId,
    context: InputContext,
    branch: BranchId,
    observer_revision: u64,
    target: ActorId,
    target_revision: u64,
    source: StrikeSourceSelector,
    resolved: StrikeSourceAttribution,
    catalog: (String, BranchId, String, Option<String>),
}

struct Client {
    strike_inspection: Option<StrikeInspectionHandle>,
    map_transfer: Option<map_pages::MapTransfer>,
    chart_transfer: Option<chart_pages::ChartTransfer>,
    role: AccessRole,
    user: String,
    frontend: String,
    allowed: BTreeSet<ActorId>,
    actor: Option<ActorId>,
    context: Option<StreamContext>,
    readiness: Option<DisclosedReadiness>,
    sequence: u64,
    observation_tick: u64,
    /// Last full state disclosed on this stream; the base for the next delta.
    last_observation: Option<DisclosedObservation>,
    presentations: PresentationHistory,
    /// The palette last sent and its revision: the base for the next delta.
    palette: Option<(u64, BTreeSet<String>)>,
    /// The region that palette was forecast from.
    palette_region: Option<u64>,
    messages: crate::outbound::Sender,
    close: watch::Sender<Option<DisconnectMode>>,
    /// What this client was last told play waits for, since its last update
    /// or snapshot; `None` once anything has changed.
    waiting: Option<Waiting>,
    /// Last queued failure delivered to this attachment. Never persisted.
    reported_execution_failure: Option<(StreamContext, BranchId, EntryId, ErrorCode)>,
}

/// A resource failure discards stale output; an intentional detach may deliver
/// its final explanation before closing. Neither changes simulation authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisconnectMode {
    Abort,
    Drain,
}

pub(crate) struct Connection {
    pub id: u64,
    pub messages: crate::outbound::Receiver,
    pub close: watch::Receiver<Option<DisconnectMode>>,
}

/// Each client's outgoing queue. A client whose queue overflows is
/// disconnected; it never silently misses an update.
pub(crate) const QUEUE: usize = 256;

/// A run pauses while an attached client's queue has less room than this. One
/// action sends each client only a few messages, so a run never overflows a
/// client that keeps reading.
pub(crate) const HEADROOM: usize = 16;

/// What [`Service::step`] did.
pub(crate) enum Step {
    /// It took an action or ended a journey; call it again.
    Progress,
    /// It needs input from a client, or nobody is playing.
    Blocked,
    /// These clients' queues are nearly full; wait for them to read.
    Full(Vec<(u64, crate::outbound::Sender)>),
}

/// Until hostility and environmental danger are modeled, another perceived
/// actor is conservatively a potential hazard. Never consult hidden actors,
/// and do not classify another portal view of the observer as a threat.
fn potential_hazards(observation: &Observation) -> BTreeSet<tor_protocol::ActorTarget> {
    observation
        .visible_actors
        .iter()
        .filter(|other| other.id != observation.self_target)
        .map(|other| other.id)
        .collect()
}

struct TravelJob {
    hp: Option<u64>,
    owner: u64,
    steps: VecDeque<tor_simulation::TravelStep>,
    hazards: BTreeSet<tor_protocol::ActorTarget>,
    pending: Option<EntryId>,
}

impl TravelJob {
    fn hazard_in(&self, observation: &Observation) -> bool {
        !potential_hazards(observation).is_subset(&self.hazards)
            || self
                .hp
                .zip(observation.combat.as_ref().map(|combat| combat.hp))
                .is_some_and(|(before, after)| after < before)
    }
}

#[derive(Clone, Copy)]
enum TravelExecution {
    Moved,
    Blocked,
    Redirected,
}

/// Session authority and ordered publication around one owned [`Engine`].
///
/// Operations are serialized by [`Simulation`](crate::Simulation), keeping account,
/// attachment, control and output state consistent with accepted game boundaries.
/// Those ephemeral permissions are not recovered from an old client stream.
pub struct Service {
    attachments: BTreeMap<u64, attachment_session::Upload>,
    outbound: crate::outbound::Pool,
    diagnostics: Option<crate::diagnostics::Diagnostics>,
    pending_pauses: BTreeSet<ActorId>,
    pending_travel_cancellations: BTreeSet<ActorId>,
    /// The latest outcome's after-captures. The next action's before-capture
    /// is the same committed state, so it reuses them while nothing else has
    /// run: any request, connection change or save poll clears them, and they
    /// apply only at their exact commit.
    settled_views: Option<SemanticStates>,
    autonomous_enabled: bool,
    travels: BTreeMap<ActorId, TravelJob>,
    travel_status: BTreeMap<ActorId, TravelStatus>,
    resetting_streams: bool,
    broadcasting_travel: bool,
    engine: Engine,
    /// Client, request, target sequence and when it was queued.
    pending_saves: Vec<(u64, String, u64, std::time::Instant)>,
    save_warning: Option<String>,
    /// How long an explicit save waits for durability before `SaveBehind`.
    save_reply_wait: std::time::Duration,
    clients: BTreeMap<u64, Client>,
    controllers: BTreeMap<ActorId, u64>,
    control_generations: BTreeMap<ActorId, Option<u64>>,
    next_client: u64,
}

impl Service {
    /// Create a session with default outbound limits and reconcile saved work.
    ///
    /// See [`with_outbound_limits`](Self::with_outbound_limits) for startup effects
    /// and the fallible constructor.
    ///
    /// # Panics
    ///
    /// Panics if startup reconciliation fails, for example when storage rejects
    /// cancellation of a saved journey or suspension of a queued human intention.
    pub fn new(engine: Engine) -> Self {
        Self::with_outbound_limits(engine, crate::OutboundLimits::default())
            .expect("valid default outbound limits")
    }

    /// Create a session with validated, non-persisted host output bounds.
    ///
    /// Before clients connect, saved journey work is cancelled and queued human
    /// intentions are suspended. Existing preparation is paused except for AI in
    /// an unattended arena. These are ordinary engine transitions and can append
    /// journal records; construction is not a read-only wrapper around an engine.
    ///
    /// Invalid limits or failed journey/intention reconciliation return a
    /// [`Failure`]. Preparation-pause failures instead become the session's save
    /// warning. Earlier successful reconciliation steps are not rolled back if a
    /// later one fails. Construction itself does not promise a durability barrier.
    pub fn with_outbound_limits(
        mut engine: Engine,
        limits: crate::OutboundLimits,
    ) -> Result<Self, Failure> {
        if !limits.is_valid() {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Invalid outbound byte limits",
            ));
        }
        let mut save_warning = None;
        // Journeys are session-owned and never resume implicitly after restart.
        // Their admitted steps must be terminal before clients can acquire control.
        for actor in engine.queued_travel_actors() {
            engine.cancel_travel(actor)?;
        }
        for actor in engine.queued_human_actors() {
            engine.suspend_queued_intention(actor)?;
        }
        for actor in engine.actors() {
            // Arena AI has permanent autonomous authority; preserve its paid
            // preparation through restart without admitting replacement work.
            if engine.unattended_arena() && engine.is_ai(actor) {
                continue;
            }
            if let Err(error) = engine.pause_preparation(actor) {
                save_warning = Some(error.to_string());
            }
        }
        Ok(Self {
            attachments: BTreeMap::new(),
            outbound: crate::outbound::Pool::new(limits),
            diagnostics: None,
            pending_pauses: BTreeSet::new(),
            pending_travel_cancellations: BTreeSet::new(),
            settled_views: None,
            autonomous_enabled: engine.unattended_arena() && engine.arena_execution_enabled(),
            travels: BTreeMap::new(),
            travel_status: BTreeMap::new(),
            resetting_streams: false,
            broadcasting_travel: false,
            engine,
            pending_saves: Vec::new(),
            save_reply_wait: SAVE_REPLY_WAIT,
            save_warning,
            clients: BTreeMap::new(),
            controllers: BTreeMap::new(),
            control_generations: BTreeMap::new(),
            next_client: 1,
        })
    }

    /// Change how long an explicit save waits before answering `SaveBehind`.
    /// The duration must be positive and at most one minute. Timing out the reply
    /// does not cancel the pending write or report it as durable.
    pub fn set_save_reply_wait(&mut self, wait: std::time::Duration) -> Result<(), Failure> {
        if wait.is_zero() || wait > std::time::Duration::from_secs(60) {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Invalid save reply wait",
            ));
        }
        self.save_reply_wait = wait;
        Ok(())
    }

    pub(crate) fn set_diagnostics(&mut self, diagnostics: Option<crate::diagnostics::Diagnostics>) {
        self.diagnostics = diagnostics;
    }

    pub(crate) fn connect(
        &mut self,
        account: &Account,
        frontend: String,
    ) -> Result<Connection, Failure> {
        self.settled_views = None;
        if !valid_label(&frontend) {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Connection is unavailable",
            ));
        }
        if self.clients.len() >= self.outbound.connection_limit() {
            return Err(Failure::new(
                ErrorCode::ResourceLimit,
                "Connection is unavailable",
            ));
        }
        let next = self.next_client.checked_add(1).ok_or_else(|| {
            Failure::new(ErrorCode::InvalidRequest, "Connection identity exhausted")
        })?;
        let id = self.next_client;
        let (messages, receiver) = self
            .outbound
            .channel(QUEUE)
            .map_err(|_| Failure::new(ErrorCode::ResourceLimit, "Connection is unavailable"))?;
        self.next_client = next;
        let (close, closing) = watch::channel(None);
        let actors: Vec<_> = self
            .engine
            .actors()
            .into_iter()
            .filter(|actor| account.role == AccessRole::Wizard || account.actors.contains(actor))
            .collect();
        self.clients.insert(
            id,
            Client {
                strike_inspection: None,
                map_transfer: None,
                chart_transfer: None,
                role: account.role,
                user: account.user.clone(),
                frontend,
                allowed: actors.iter().copied().collect(),
                actor: None,
                context: None,
                readiness: None,
                sequence: 0,
                observation_tick: 0,
                last_observation: None,
                presentations: PresentationHistory::default(),
                palette: None,
                palette_region: None,
                messages,
                close,
                waiting: None,
                reported_execution_failure: None,
            },
        );
        self.send(
            id,
            ServerMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                capabilities: self.outbound.capabilities(),
                user: account.user.clone(),
                actors,
                role: account.role,
            },
        );
        Ok(Connection {
            id,
            messages: receiver,
            close: closing,
        })
    }

    pub(crate) fn handle(&mut self, id: u64, request_id: String, request: Request) {
        self.settled_views = None;
        if !self.clients.contains_key(&id) {
            return;
        }
        let result = if valid_label(&request_id) {
            self.process(id, &request_id, request)
        } else {
            Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Invalid request ID",
            ))
        };
        if matches!(result, Ok(ProcessedRequest::SaveQueued)) {
            self.poll_saves();
            return;
        }
        // Publish causing effects and permissions before completing the request.
        // Receipt identity comes from the original operation, never this stream.
        self.refresh_readiness();
        let clients = self.clients.len();
        match result {
            Ok(ProcessedRequest::Reply(reply)) => self.publish_reply(id, request_id, reply),
            Ok(ProcessedRequest::Map(transfer)) => {
                if let Err(error) = self.publish_map_page(id, &request_id, *transfer) {
                    self.send(
                        id,
                        ServerMessage::Error {
                            scope: self.error_scope(id),
                            request_id: Some(request_id),
                            code: error.code,
                            message: error.message,
                        },
                    );
                }
            }
            Ok(ProcessedRequest::Chart(transfer)) => {
                if let Err(error) = self.publish_chart_page(id, &request_id, *transfer) {
                    self.send(
                        id,
                        ServerMessage::Error {
                            scope: self.error_scope(id),
                            request_id: Some(request_id),
                            code: error.code,
                            message: error.message,
                        },
                    );
                }
            }
            Ok(ProcessedRequest::Published) => {}
            Ok(ProcessedRequest::SaveQueued) => unreachable!("handled by persistence"),
            Err(error) => self.send(
                id,
                ServerMessage::Error {
                    scope: self.error_scope(id),
                    request_id: Some(request_id),
                    code: error.code,
                    message: error.message,
                },
            ),
        }
        // Rejected output can remove a controller and change other clients' input.
        if self.clients.len() != clients {
            self.refresh_readiness();
        }
    }

    fn process(
        &mut self,
        id: u64,
        request_id: &str,
        request: Request,
    ) -> Result<ProcessedRequest, Failure> {
        if matches!(
            &request,
            Request::Command {
                command: Command::Wizard { .. } | Command::InspectStrikeSource { .. },
                ..
            }
        ) && (self.clients[&id].role != AccessRole::Wizard || !self.engine.wizard_enabled())
        {
            return Err(Failure::new(
                ErrorCode::Unauthorized,
                "Wizard authority is required",
            ));
        }
        // Check authority before attachment, receipt lookup, or any mutation.
        if !self.clients[&id].role.permits(&request) {
            return Err(Failure::new(
                ErrorCode::Unauthorized,
                "Spectator access is read-only",
            ));
        }
        if let Request::Attach { actor } = request {
            let client = self.clients.get_mut(&id).expect("connected client");
            if client.actor.is_some() {
                return Err(Failure::new(
                    ErrorCode::AlreadyAttached,
                    "Reconnect to attach another actor",
                ));
            }
            if !client.allowed.contains(&actor) {
                return Err(Failure::new(
                    ErrorCode::Unauthorized,
                    "Actor is unavailable",
                ));
            }
            self.engine.register_input_observer(actor)?;
            let client = self.clients.get_mut(&id).expect("connected client");
            client.actor = Some(actor);
            client.context = Some(StreamContext {
                stream: StreamId(uuid::Uuid::new_v4().to_string()),
                epoch: 0,
            });
            self.snapshot(id, request_id)?;
            self.opening_messages(id, actor);
            if let Some(message) = self.save_warning.clone() {
                self.send(
                    id,
                    ServerMessage::Error {
                        scope: self.error_scope(id),
                        request_id: None,
                        code: ErrorCode::StorageFailure,
                        message,
                    },
                );
            }
            return Ok(ProcessedRequest::Published);
        }
        let client = &self.clients[&id];
        let actor = client
            .actor
            .ok_or_else(|| Failure::new(ErrorCode::NotAttached, "Attach an actor first"))?;
        let user = client.user.clone();
        let frontend = client.frontend.clone();
        self.expire_attachments();
        match request {
            request @ (Request::BeginAttachment { .. }
            | Request::AttachmentChunk { .. }
            | Request::CommitAttachment { .. }
            | Request::AbortAttachment { .. }
            | Request::ReadAttachment { .. }) => self.process_attachment(id, request_id, request),
            Request::Chart { query } => self
                .prepare_chart_page(id, query)
                .map(|transfer| ProcessedRequest::Chart(Box::new(transfer))),
            Request::Map { query } => self
                .prepare_map_page(id, query)
                .map(|transfer| ProcessedRequest::Map(Box::new(transfer))),
            Request::Continue => {
                if self.controllers.get(&actor) != Some(&id) {
                    return Err(Failure::new(
                        ErrorCode::NotController,
                        "Acquire control before continuing",
                    ));
                }
                self.autonomous_enabled = true;
                Ok(self
                    .session_receipt(id)
                    .map(|receipt| ProcessedRequest::Reply(RequestReply::Receipt(receipt)))
                    .unwrap_or(ProcessedRequest::Published))
            }
            Request::Save => {
                if self.pending_saves.iter().any(|(client, ..)| *client == id) {
                    return Err(Failure::new(
                        ErrorCode::InvalidRequest,
                        "A save is already pending",
                    ));
                }
                let target = self.engine.request_save();
                self.pending_saves
                    .push((id, request_id.into(), target, std::time::Instant::now()));
                Ok(ProcessedRequest::SaveQueued)
            }
            Request::AcquireControl => {
                self.apply_pending_pauses();
                if self.pending_pauses.contains(&actor) {
                    return Err(Failure::new(
                        ErrorCode::StorageFailure,
                        "Pending gameplay suspension could not be saved",
                    ));
                }
                if self.engine.is_ai(actor) {
                    return Err(Failure::new(
                        ErrorCode::Unauthorized,
                        "This actor is controlled by scenario AI",
                    ));
                }
                match self.controllers.get(&actor) {
                    Some(owner) if *owner != id => {
                        return Err(Failure::new(
                            ErrorCode::ControlTaken,
                            "Another client controls this actor",
                        ))
                    }
                    Some(_) => {}
                    None => {
                        self.set_controller(actor, Some(id));
                        self.control_update(actor);
                    }
                }
                Ok(self
                    .session_receipt(id)
                    .map(|receipt| ProcessedRequest::Reply(RequestReply::Receipt(receipt)))
                    .unwrap_or(ProcessedRequest::Published))
            }
            Request::ReleaseControl => {
                if self
                    .controllers
                    .get(&actor)
                    .is_some_and(|owner| *owner != id)
                {
                    return Err(Failure::new(
                        ErrorCode::NotController,
                        "This client does not control the actor",
                    ));
                }
                self.stop_travel(actor, TravelPhase::ControlLost);
                if self.set_controller(actor, None) {
                    self.pending_pauses.insert(actor);
                    if !self.engine.unattended_arena() {
                        self.autonomous_enabled = false;
                    }
                    self.control_update(actor);
                }
                Ok(self
                    .session_receipt(id)
                    .map(|receipt| ProcessedRequest::Reply(RequestReply::Receipt(receipt)))
                    .unwrap_or(ProcessedRequest::Published))
            }
            Request::Observe => self.observe(id).map(|()| ProcessedRequest::Published),
            Request::Snapshot => self
                .snapshot(id, request_id)
                .map(|()| ProcessedRequest::Published),
            Request::Discoveries => {
                let rows = self.engine.item_discoveries(actor);
                if rows.len() > tor_protocol::MAX_ITEM_DISCOVERIES {
                    return Err(Failure::new(
                        ErrorCode::InvalidRequest,
                        "Too many item discoveries",
                    ));
                }
                Ok(ProcessedRequest::Reply(RequestReply::Discoveries(rows)))
            }
            Request::Palette => Ok(self
                .prepare_palette(id, true)
                .map(|palette| ProcessedRequest::Reply(RequestReply::Palette(palette)))
                .unwrap_or(ProcessedRequest::Published)),
            Request::HistoryBranch {
                branch,
                before,
                limit,
            } => {
                let page = self.engine.history_branch(
                    actor,
                    &user,
                    &branch,
                    before.as_ref(),
                    usize::from(limit),
                )?;
                Ok(ProcessedRequest::Reply(RequestReply::History(page)))
            }
            Request::History { before, limit } => {
                let page =
                    self.engine
                        .history(actor, &user, before.as_ref(), usize::from(limit))?;
                Ok(ProcessedRequest::Reply(RequestReply::History(page)))
            }
            Request::Command {
                context,
                branch,
                command,
            } => {
                let caller = commands::Caller {
                    id,
                    actor,
                    user,
                    frontend,
                };
                self.process_command(&caller, request_id, context, branch, &command)
            }
            Request::Attach { .. } => unreachable!("handled before attachment lookup"),
        }
    }

    fn action_result_update(
        &mut self,
        before: &SemanticStates,
        result: &crate::CommandResult,
    ) -> Result<(), Failure> {
        let entry = self.engine.disclose_entry(&result.entry);
        self.action_update(before, entry.as_ref(), Some(result))?;
        self.intention_update(result);
        Ok(())
    }

    fn intention_update(&mut self, result: &crate::CommandResult) {
        if let (Some(diagnostics), Some((_, OwnerIntentionFailure::RolledBack(action, error)))) =
            (&self.diagnostics, &result.intention_failure)
        {
            diagnostics.warning(&format!(
                "Rolled back {action:?} after simulation error {error:?}"
            ));
        }
        for status in self.engine.intention_updates(result) {
            let recipients: Vec<_> = self
                .clients
                .iter()
                .filter(|(_, client)| client.actor == Some(status.actor))
                .map(|(&id, _)| id)
                .collect();
            for recipient in recipients {
                let owner_failure =
                    result
                        .intention_failure
                        .as_ref()
                        .and_then(|(admission, reason)| {
                            (status.phase == IntentionPhase::Failed
                                && admission == &status.entry_id
                                && self.controllers.get(&status.actor) == Some(&recipient))
                            .then_some(reason)
                        });
                self.update(
                    recipient,
                    UpdateBody::Intention {
                        failure: match owner_failure {
                            Some(OwnerIntentionFailure::Status(reason)) => Some(*reason),
                            Some(OwnerIntentionFailure::RolledBack(action, _)) => {
                                Some(tor_protocol::IntentionFailure::RolledBack(*action))
                            }
                            _ => None,
                        },
                        status: status.clone(),
                    },
                );
                // Keep the committed failed status even if the reason can't
                // be sent. This is private, attachment-scoped guidance, not
                // a retryable execution error or a second journal outcome.
                if let Some(OwnerIntentionFailure::Requirement(reason)) = owner_failure {
                    self.send(
                        recipient,
                        ServerMessage::Error {
                            scope: self.error_scope(recipient),
                            request_id: None,
                            code: reason.code,
                            message: reason.message.clone(),
                        },
                    );
                }
            }
        }
    }

    /// Project a role only when it has private item details to remove. Equal
    /// projections retain the same allocation and per-connection stream bases.
    fn item_observation(&self, state: Arc<StateView>, role: AccessRole) -> Arc<StateView> {
        if role != AccessRole::Spectator {
            return state;
        }
        if has_private_item_disclosure(&state) {
            let mut projected = state.as_ref().clone();
            restrict_item_disclosure(&mut projected, role);
            Arc::new(projected)
        } else {
            state
        }
    }

    /// Capture only attached observers. These transient values are narration
    /// inputs, not public display snapshots and not persisted revision state.
    fn semantic_states(&mut self) -> Result<SemanticStates, Failure> {
        let boundary = self.engine.presentation_boundary();
        let observers = self
            .clients
            .values()
            .filter_map(|client| client.actor)
            .collect::<BTreeSet<_>>();
        if let Some(settled) = self.settled_views.take() {
            if observers.iter().all(|actor| {
                settled
                    .views
                    .get(actor)
                    .is_some_and(|view| view.taken_at(&boundary))
            }) {
                reused_semantic_capture();
                let views = settled
                    .views
                    .into_iter()
                    .filter(|(actor, _)| observers.contains(actor))
                    .collect();
                return Ok(SemanticStates {
                    views,
                    events: settled.events,
                });
            }
        }
        let events = self.engine.event_history_boundary_at(&boundary)?;
        let views = observers
            .into_iter()
            .map(|actor| {
                self.engine
                    .semantic_observation(actor)
                    .map(|state| (actor, Arc::new(state)))
            })
            .collect::<Result<_, _>>()?;
        Ok(SemanticStates { views, events })
    }

    fn action_update(
        &mut self,
        before: &SemanticStates,
        entry: Option<&HistoryEntry>,
        result: Option<&crate::CommandResult>,
    ) -> Result<(), Failure> {
        let recipients: Vec<_> = self
            .clients
            .iter()
            .filter_map(|(&id, client)| client.actor.map(|actor| (id, actor, client.role)))
            .collect();
        let events = match self
            .engine
            .event_history_boundary_at(&self.engine.presentation_boundary())
        {
            Ok(events) => events,
            Err(error) => {
                for (id, _, _) in recipients {
                    self.disconnect_failed_projection(id, &error.to_string());
                }
                return Ok(());
            }
        };
        let after: BTreeMap<_, _> = recipients
            .iter()
            .map(|(_, actor, _)| *actor)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|actor| (actor, self.engine.semantic_observation(actor).map(Arc::new)))
            .collect();
        self.settled_views = Some(SemanticStates {
            views: after
                .iter()
                .filter_map(|(actor, view)| Some((*actor, Arc::clone(view.as_ref().ok()?))))
                .collect(),
            events,
        });
        let pictures: BTreeMap<_, _> = after
            .iter()
            .filter_map(|(actor, current)| {
                let (Ok(current), Some(previous)) = (current, before.views.get(actor)) else {
                    return None;
                };
                let comparison = (|| {
                    if self.engine.semantic_picture_unchanged(previous, current)? {
                        return Ok(None);
                    }
                    Ok::<_, Failure>(Some(SemanticPicturePair {
                        before: self.engine.semantic_picture(previous)?,
                        after: self.engine.semantic_picture(current)?,
                    }))
                })();
                Some((*actor, comparison))
            })
            .collect();
        for (id, actor, role) in recipients {
            if !self.clients.contains_key(&id) {
                continue;
            }
            let state = match &after[&actor] {
                Ok(state) => state,
                Err(error) => {
                    if self.engine.actors().contains(&actor) {
                        self.disconnect_failed_projection(id, &error.to_string());
                    } else {
                        self.detach_unloaded(id);
                    }
                    continue;
                }
            };
            let Some(previous) = before.views.get(&actor) else {
                self.disconnect_failed_projection(id, "Missing pre-commit observer capture");
                continue;
            };
            let previous_facts = semantic_item_facts(previous, role);
            let current_facts = semantic_item_facts(state, role);
            let own_entry = entry.filter(|entry| entry.actor == actor);
            let own_event = own_entry.and_then(|entry| match &entry.content {
                HistoryContent::Action { event, .. } => Some(event.as_ref()),
                _ => None,
            });
            let facts = OutcomeFacts::capture_compact_causal(
                &previous_facts,
                &current_facts,
                own_event,
                before.events,
                events,
            )
            .and_then(|mut facts| {
                if let Some(result) = result {
                    facts
                        .facts
                        .extend(self.engine.transaction_user_messages(result, actor));
                    let activities = self.engine.outcome_activities(
                        result,
                        actor,
                        &previous_facts,
                        &current_facts,
                    );
                    facts.extend_activities(activities, &previous_facts, &current_facts);
                    let changes = self
                        .engine
                        .outcome_work_changes(result, actor, &previous_facts);
                    if !changes.is_empty() {
                        facts.facts.retain(|fact| {
                            !matches!(fact, OutcomeFact::Combat {
                            event: CombatEventView::Interrupted { actor },
                            ..
                        } if *actor == facts.self_target)
                        });
                        facts.facts.extend(changes);
                    }
                }
                facts.validate_at(current_facts.tick)?;
                Ok(facts)
            });
            let facts = match facts {
                Ok(facts) => facts,
                Err(error) => {
                    // The action is already committed. Close this stream rather
                    // than skip its outcome; healthy peers still receive theirs.
                    self.disconnect_failed_projection(id, &format!("{error:?}"));
                    continue;
                }
            };
            let visual_changed = match &pictures[&actor] {
                Ok(None) => false,
                Ok(Some(pictures)) => {
                    let old = self.item_observation(Arc::clone(&pictures.before), role);
                    let new = self.item_observation(Arc::clone(&pictures.after), role);
                    OutcomeFacts::picture_changed(&old.observation, &new.observation)
                }
                Err(error) => {
                    self.disconnect_failed_projection(id, &error.to_string());
                    continue;
                }
            };
            if visual_changed || !facts.facts.is_empty() || own_entry.is_some() {
                // Narration uses event-time appearances from both sides of a
                // region transition. Admit new forecast keys before the event,
                // and retire old keys only after that event has been admitted.
                let palette = self.prepare_palette_with_retention(id, false, true);
                self.publish_palette(id, palette);
                if !self.clients.contains_key(&id) {
                    continue;
                }
                self.update_at(
                    id,
                    UpdateBody::Outcome {
                        visual_changed,
                        facts,
                        event: own_entry.map(|entry| Box::new(entry.clone())),
                    },
                    Some(current_facts.tick),
                );
            }
            self.palette_update(id);
        }
        Ok(())
    }

    /// A game nobody has acted in yet shows what its scripts said when the run
    /// started, right after the attachment snapshot.
    fn opening_messages(&mut self, id: u64, actor: ActorId) {
        let facts = self.engine.opening_user_messages(actor);
        let Some(observation) = self
            .clients
            .get(&id)
            .and_then(|client| client.last_observation.as_ref())
            .map(|last| Arc::clone(&last.state))
        else {
            return;
        };
        if facts.is_empty() {
            return;
        }
        let facts = tor_protocol::OutcomeFacts {
            self_target: observation.observation.self_target,
            status: None,
            actor_names: Vec::new(),
            facts,
        };
        if facts.validate_at(observation.observation.tick).is_err() {
            self.disconnect_failed_projection(id, "Invalid opening script messages");
            return;
        }
        self.update_at(
            id,
            UpdateBody::Outcome {
                visual_changed: false,
                facts,
                event: None,
            },
            Some(observation.observation.tick),
        );
    }

    /// Outcome delivery failures are not persistence failures and must not
    /// overwrite the save worker's warning/recovery state.
    fn disconnect_failed_projection(&mut self, id: u64, reason: &str) {
        if let Some(diagnostics) = &self.diagnostics {
            diagnostics.warning(&format!(
                "Committed outcome projection failed for connection {id}; disconnected: {reason}"
            ));
        }
        self.disconnect(id);
    }

    /// Send a client its actor's palette: the whole palette when asked, or
    /// the first time; otherwise the changes, if any, as a delta. Nothing is
    /// acknowledged. Scenarios that name no assets send no palettes, but a
    /// palette request still gets an (empty) answer.
    fn prepare_palette(&mut self, id: u64, full: bool) -> Option<PaletteUpdate> {
        self.prepare_palette_with_retention(id, full, false)
    }

    /// Keep previously authorized keys through one causal Outcome. This is a
    /// transient publication union, never an observation-derived asset registry.
    fn prepare_palette_with_retention(
        &mut self,
        id: u64,
        full: bool,
        retain_previous: bool,
    ) -> Option<PaletteUpdate> {
        let client = self.clients.get(&id)?;
        let actor = client.actor?;
        // A palette depends only on the actor's region: skip recomputing it
        // while that stays the same.
        let region = self.engine.palette_region(actor);
        if !full && client.palette.is_some() && client.palette_region == region {
            return None;
        }
        let mut assets = match self.engine.palette(actor) {
            Some(assets) => assets,
            None if full => BTreeSet::new(),
            None => return None,
        };
        let mut retained_old_keys = false;
        if retain_previous {
            if let Some((_, previous)) = &client.palette {
                retained_old_keys = !previous.is_subset(&assets);
                assets.extend(previous.iter().cloned());
            }
        }
        // A retained old key requires the normal post-Outcome update to revisit
        // this forecast and retire it; otherwise keep the usual region fast path.
        let cached_region = if retained_old_keys { None } else { region };
        let client = self.clients.get_mut(&id).expect("connected client");
        if !full
            && client
                .palette
                .as_ref()
                .is_some_and(|(_, previous)| *previous == assets)
        {
            client.palette_region = cached_region;
            return None;
        }
        let body = match &client.palette {
            Some((base, previous)) if !full => PaletteBody::Delta {
                base: *base,
                added: assets.difference(previous).cloned().collect(),
                removed: previous.difference(&assets).cloned().collect(),
            },
            _ => PaletteBody::Full {
                assets: assets.clone(),
            },
        };
        let revision = client.palette.as_ref().map_or(1, |(r, _)| r + 1);
        client.palette = Some((revision, assets));
        client.palette_region = cached_region;
        Some(PaletteUpdate { revision, body })
    }

    fn palette_update(&mut self, id: u64) {
        let palette = self.prepare_palette(id, false);
        self.publish_palette(id, palette);
    }

    fn publish_palette(&mut self, id: u64, palette: Option<PaletteUpdate>) {
        if let Some(palette) = palette {
            if let Some(context) = self.reply_context(id) {
                self.send(
                    id,
                    ServerMessage::Palette {
                        context,
                        request_id: None,
                        palette,
                    },
                );
            }
        }
    }

    fn travel_update(&mut self, actor: ActorId, entry: Option<HistoryEntry>) {
        let Some(initial) = self.travel_status.get(&actor).cloned() else {
            return;
        };
        let recipients: Vec<_> = self
            .clients
            .iter()
            .filter(|(_, c)| c.actor == Some(actor))
            .map(|(&id, _)| id)
            .collect();
        self.broadcasting_travel = true;
        for id in recipients {
            // Sending can disconnect a slow controller and stop its job. Never
            // follow that terminal status with an obsolete active status.
            let status = self.travel_status[&actor].clone();
            self.update(
                id,
                UpdateBody::Travel {
                    status,
                    entry: entry.clone().map(Box::new),
                },
            );
        }
        self.broadcasting_travel = false;
        if self.travel_status.get(&actor) != Some(&initial) {
            self.travel_update(actor, None);
        }
    }

    fn stop_travel(&mut self, actor: ActorId, phase: TravelPhase) {
        self.travels.remove(&actor);
        self.cancel_travel_work(actor);
        if let Some(status) = self.travel_status.get_mut(&actor) {
            if status.phase == TravelPhase::Active {
                status.phase = phase;
                if !self.broadcasting_travel {
                    self.travel_update(actor, None);
                }
            }
        }
    }

    fn cancel_travel_work(&mut self, actor: ActorId) {
        match self.engine.cancel_travel(actor) {
            Ok(_) => {
                self.pending_travel_cancellations.remove(&actor);
            }
            Err(error) => {
                self.pending_travel_cancellations.insert(actor);
                self.save_warning = Some(error.to_string());
            }
        }
    }

    /// Apply the pauses a released or disconnected controller left behind.
    fn apply_pending_pauses(&mut self) {
        // Retain ownership until both operations succeed. In particular, a
        // fallible observer capture must not discard this actor or the rest of
        // the batch and accidentally reopen admission/simulation.
        let pending: Vec<_> = self.pending_pauses.iter().copied().collect();
        for actor in pending {
            let before = match self.semantic_states() {
                Ok(states) => states,
                Err(error) => {
                    if let Some(diagnostics) = &self.diagnostics {
                        diagnostics
                            .warning(&format!("Pending pause observer capture failed: {error}"));
                    }
                    return;
                }
            };
            match self.engine.suspend_queued_intention(actor) {
                Ok(Some(result)) => self.intention_update(&result),
                Ok(None) => {}
                Err(error) => {
                    self.save_warning = Some(error.to_string());
                    continue;
                }
            }
            match self.engine.pause_preparation(actor) {
                Ok(Some(result)) => {
                    let _ = self.action_result_update(&before, &result);
                    self.pending_pauses.remove(&actor);
                }
                Ok(None) => {
                    self.pending_pauses.remove(&actor);
                }
                Err(error) => self.save_warning = Some(error.to_string()),
            }
        }
    }

    /// Take one action, or say why the simulation can't. The runner calls this
    /// until it blocks and handles waiting mail between calls, so a request
    /// waits for at most one action. What happens depends only on the game and
    /// the commands it received, never on wall-clock time.
    pub(crate) fn step(&mut self) -> Step {
        let step = self.step_inner();
        self.refresh_readiness();
        step
    }

    fn ai_has_driver(&self) -> bool {
        self.engine.unattended_arena()
            || self
                .controllers
                .keys()
                .any(|&actor| self.engine.alive(actor))
    }

    fn step_inner(&mut self) -> Step {
        for actor in std::mem::take(&mut self.pending_travel_cancellations) {
            self.cancel_travel_work(actor);
        }
        self.apply_pending_pauses();
        if !self.pending_pauses.is_empty() || !self.pending_travel_cancellations.is_empty() {
            return Step::Blocked;
        }
        let full: Vec<_> = self
            .clients
            .iter()
            .filter(|(_, client)| client.actor.is_some() && !client.messages.has_headroom(HEADROOM))
            .map(|(&id, client)| (id, client.messages.clone()))
            .collect();
        if !full.is_empty() {
            return Step::Full(full);
        }
        if !self.engine.arena_execution_enabled() {
            self.announce_waiting();
            return Step::Blocked;
        }
        if let Some(next) = self.engine.next_intention_actor() {
            return self.execute_queued_intention(next);
        }
        let Some(next) = self.engine.next_actor() else {
            self.announce_waiting();
            return Step::Blocked;
        };
        if self.travels.contains_key(&next) {
            return self.travel_step(next);
        }
        // Adventures need a living controller; arenas have bounded unattended
        // execution, including after selected death.
        if !self.engine.is_ai(next) || !self.autonomous_enabled || !self.ai_has_driver() {
            self.announce_waiting();
            return Step::Blocked;
        }
        self.admit_ai(next);
        match self.engine.next_intention_actor() {
            Some(next) => self.execute_queued_intention(next),
            None => Step::Blocked,
        }
    }

    /// All due work uses this path, including an AI decision just admitted above.
    fn execute_queued_intention(&mut self, next: ActorId) -> Step {
        let allowed = match self.engine.next_intention_origin() {
            Some(tor_simulation::IntentionOrigin::Human) => {
                !self.engine.alive(next) || self.controllers.contains_key(&next)
            }
            Some(tor_simulation::IntentionOrigin::Autonomous) => {
                self.autonomous_enabled && self.ai_has_driver()
            }
            Some(tor_simulation::IntentionOrigin::Travel) => {
                let linked = self
                    .travels
                    .get(&next)
                    .and_then(|job| job.pending.as_ref())
                    .is_some_and(|pending| {
                        self.engine.queued_travel_admission(next) == Some(pending)
                    });
                if !linked {
                    self.stop_travel(next, TravelPhase::Failed);
                    return Step::Progress;
                }
                if !self.travel_ready(next) {
                    return if self.pending_travel_cancellations.is_empty() {
                        Step::Progress
                    } else {
                        Step::Blocked
                    };
                }
                true
            }
            None => false,
        };
        if !allowed {
            self.announce_waiting();
            return Step::Blocked;
        }
        let before_states = self.semantic_states();
        match before_states.and_then(|before_states| {
            self.engine
                .execute_next_intention()
                .map(|result| (before_states, result))
        }) {
            Ok((before_states, Some(result))) => {
                // Account committed movement before output can disconnect its owner.
                let travel = self.commit_travel_progress(next, &result);
                if let Err(error) = self.action_result_update(&before_states, &result) {
                    self.save_warning = Some(error.to_string());
                }
                if let Some(outcome) = travel {
                    self.finish_travel_execution(next, outcome);
                }
                return Step::Progress;
            }
            Ok((_, None)) => {}
            // An execution error leaves its admission queued. Save-worker polling
            // tracks a separate condition and cannot acknowledge this failure.
            Err(error) => self.report_execution_failure(next, error),
        }
        self.announce_waiting();
        Step::Blocked
    }

    fn report_execution_failure(&mut self, actor: ActorId, failure: Failure) {
        let Some(admission) = self.engine.queued_intention_admission(actor).cloned() else {
            return;
        };
        let owner = self.controllers.get(&actor).copied();
        let shared = matches!(
            failure.code,
            ErrorCode::StorageFailure | ErrorCode::InvalidArchive
        ) || self.engine.next_intention_origin()
            == Some(tor_simulation::IntentionOrigin::Autonomous);
        let recipients: Vec<_> = if shared {
            self.controllers.values().copied().collect()
        } else {
            owner.into_iter().collect()
        };
        for id in recipients {
            let Some(client) = self.clients.get(&id) else {
                continue;
            };
            let Some(stream) = client.context.clone() else {
                continue;
            };
            let key = (
                stream,
                self.engine.branch().clone(),
                admission.clone(),
                failure.code,
            );
            if client.reported_execution_failure.as_ref() == Some(&key) {
                continue;
            }
            // A controller waiting on someone else's work can learn that
            // advancement stopped, but neither the actor nor its action.
            let reason = if owner != Some(id) {
                "Play is paused. Another queued action could not finish.".into()
            } else {
                match failure.code {
                    ErrorCode::StorageFailure => "Play is paused because the game could not save. Your action is still queued.".into(),
                    ErrorCode::InvalidArchive => "Play is paused because the game could not verify the journal. Your action is still queued.".into(),
                    _ => format!("{}. Your action is still queued. You can cancel it to choose another action.", failure.message.trim_end_matches('.')),
                }
            };
            let message = ServerMessage::Error {
                scope: self.error_scope(id),
                request_id: None,
                code: failure.code,
                message: reason,
            };
            if self.admit_output(id, &message) {
                if let Some(client) = self.clients.get_mut(&id) {
                    client.reported_execution_failure = Some(key);
                }
            }
        }
    }

    /// Tell each attached client what stopped play waits for, once per stop.
    fn announce_waiting(&mut self) {
        let next = self.engine.next_actor();
        let anyone = self.ai_has_driver();
        let ids: Vec<u64> = self
            .clients
            .iter()
            .filter(|(_, c)| c.actor.is_some())
            .map(|(&id, _)| id)
            .collect();
        for id in ids {
            let on = match next {
                None => Waiting::Stopped,
                // Adventure AI needs a player; arena AI has its own driver.
                Some(actor) if self.engine.is_ai(actor) && !anyone => Waiting::Stopped,
                Some(actor) if self.engine.is_ai(actor) => Waiting::Paused,
                Some(actor) => match self.controllers.get(&actor) {
                    Some(&owner) if owner == id => Waiting::You,
                    Some(_) => Waiting::Others,
                    None => Waiting::Unclaimed,
                },
            };
            if self.clients[&id].waiting != Some(on) {
                self.send(id, ServerMessage::Waiting { on });
                if let Some(client) = self.clients.get_mut(&id) {
                    client.waiting = Some(on);
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn run_until_blocked(&mut self) {
        while matches!(self.step(), Step::Progress) {}
    }

    fn travel_observation(&self, actor: ActorId) -> Result<Observation, TravelPhase> {
        let job = self.travels.get(&actor).ok_or(TravelPhase::Failed)?;
        if self.controllers.get(&actor) != Some(&job.owner) {
            return Err(TravelPhase::ControlLost);
        }
        self.engine
            .observation(actor)
            .map_err(|_| TravelPhase::Failed)
    }

    /// Revalidate session authority and disclosed hazards after each mailbox pass.
    fn travel_ready(&mut self, actor: ActorId) -> bool {
        let phase = match self.travel_observation(actor) {
            Err(phase) => Some(phase),
            Ok(observation) => self
                .travels
                .get(&actor)
                .filter(|job| job.hazard_in(&observation))
                .map(|_| TravelPhase::Hazard),
        };
        if let Some(phase) = phase {
            self.stop_travel(actor, phase);
            return false;
        }
        true
    }

    /// Admit only; the next scheduler boundary executes this original step.
    fn travel_step(&mut self, actor: ActorId) -> Step {
        if !self.travel_ready(actor) {
            return Step::Progress;
        }
        let job = &self.travels[&actor];
        let Some(step) = job.steps.front().copied() else {
            self.stop_travel(actor, TravelPhase::Arrived);
            return Step::Progress;
        };
        let status = &self.travel_status[&actor];
        let Some(ordinal) = status.completed_steps.checked_add(1) else {
            self.stop_travel(actor, TravelPhase::Failed);
            return Step::Progress;
        };
        match self
            .engine
            .admit_travel(actor, &status.id.clone(), ordinal, step)
        {
            Ok(result) => {
                self.travels
                    .get_mut(&actor)
                    .expect("admitted journey")
                    .pending = Some(result.entry.id);
                Step::Progress
            }
            Err(error) if error.code == ErrorCode::StorageFailure => {
                self.save_warning = Some(error.to_string());
                Step::Blocked
            }
            Err(error) => {
                self.stop_travel(
                    actor,
                    if error.code == ErrorCode::InvalidAction {
                        TravelPhase::Blocked
                    } else {
                        TravelPhase::Failed
                    },
                );
                Step::Progress
            }
        }
    }

    /// The route advances only when its exact admitted step has committed.
    fn commit_travel_progress(
        &mut self,
        actor: ActorId,
        result: &crate::CommandResult,
    ) -> Option<TravelExecution> {
        let job = self.travels.get_mut(&actor)?;
        let (admission, outcome) = match &result.entry.content {
            crate::journal::JournalContent::IntentionStarted {
                admission, event, ..
            } => {
                let matched = matches!(event, crate::journal::Event::Moved { to, .. }
                    if job.steps.front().is_some_and(|step| *to == crate::adapt::position(step.destination)));
                (
                    admission,
                    if matched {
                        TravelExecution::Moved
                    } else {
                        TravelExecution::Redirected
                    },
                )
            }
            crate::journal::JournalContent::IntentionFailed { admission, .. } => {
                (admission, TravelExecution::Blocked)
            }
            _ => return None,
        };
        if job.pending.as_ref() != Some(admission) {
            return None;
        }
        job.pending = None;
        if matches!(outcome, TravelExecution::Moved) {
            job.steps.pop_front().expect("admitted route step");
            self.travel_status
                .get_mut(&actor)
                .expect("active journey status")
                .completed_steps += 1;
        }
        Some(outcome)
    }

    fn finish_travel_execution(&mut self, actor: ActorId, outcome: TravelExecution) {
        // Output may already have disconnected the controller and ended its job.
        let Some(job) = self.travels.get(&actor) else {
            return;
        };
        if matches!(
            outcome,
            TravelExecution::Blocked | TravelExecution::Redirected
        ) {
            self.stop_travel(
                actor,
                if matches!(outcome, TravelExecution::Redirected) {
                    TravelPhase::DecisionRequired
                } else {
                    TravelPhase::Blocked
                },
            );
            return;
        }
        let observation = match self.travel_observation(actor) {
            Ok(observation) => observation,
            Err(phase) => {
                self.stop_travel(actor, phase);
                return;
            }
        };
        let phase = if observation
            .motion
            .as_ref()
            .is_some_and(|motion| motion.displaced || motion.impacted)
        {
            Some(TravelPhase::DecisionRequired)
        } else if job.steps.is_empty() {
            Some(TravelPhase::Arrived)
        } else if job.hazard_in(&observation) {
            Some(TravelPhase::Hazard)
        } else {
            None
        };
        match phase {
            Some(phase) => self.stop_travel(actor, phase),
            None => self.travel_update(actor, None),
        }
    }

    fn admit_ai(&mut self, actor: ActorId) {
        let before_states = match self.semantic_states() {
            Ok(states) => states,
            Err(error) => {
                self.save_warning = Some(error.to_string());
                return;
            }
        };
        let result = self.engine.admit_ai(actor);
        match result {
            Ok(result) => {
                let _ = self.action_result_update(&before_states, &result);
            }
            Err(error) => {
                self.autonomous_enabled = false;
                for client in self.controllers.values().copied().collect::<Vec<_>>() {
                    self.send(
                        client,
                        ServerMessage::Error {
                            scope: self.error_scope(client),
                            request_id: None,
                            code: error.code,
                            message: "Autonomous action failed; simulation paused.".into(),
                        },
                    );
                }
            }
        }
    }

    fn observe(&mut self, id: u64) -> Result<(), Failure> {
        let (boundary, state) = self.prepare_presentation(id)?;
        let identity = state.snapshot.clone();
        self.update(
            id,
            UpdateBody::Observation {
                state: Arc::new(state),
                event: None,
            },
        );
        if self
            .clients
            .get(&id)
            .and_then(|client| client.last_observation.as_ref())
            .is_some_and(|observed| observed.state.snapshot == identity)
        {
            self.commit_admitted_presentation(id, boundary, false);
        }
        Ok(())
    }

    fn snapshot(&mut self, id: u64, request_id: &str) -> Result<(), Failure> {
        let client = &self.clients[&id];
        let actor = client
            .actor
            .ok_or_else(|| Failure::new(ErrorCode::NotAttached, "Attach an actor first"))?;
        let context = match client.context.as_ref().and_then(StreamContext::next_reset) {
            Some(context) => context,
            None => {
                self.disconnect(id);
                return Err(Failure::new(
                    ErrorCode::InvalidRequest,
                    "Stream exhausted; reconnect",
                ));
            }
        };
        let (boundary, state) = self.prepare_presentation(id)?;
        let readiness = match self.current_readiness(id) {
            Some(readiness) => readiness,
            None => {
                self.disconnect(id);
                return Err(Failure::new(
                    ErrorCode::InvalidRequest,
                    "Readiness exhausted; reconnect",
                ));
            }
        };
        let snapshot = Snapshot {
            context: context.clone(),
            readiness: readiness.permissions.clone(),
            intentions: self.engine.pending_intentions(actor),
            travel: self.travel_status.get(&actor).cloned(),
            actor,
            branch: self.engine.branch().clone(),
            cursor: StreamCursor {
                sequence: client.sequence,
                tick: state.observation.tick,
            },
            state: Arc::new(state),
            has_control: readiness.has_control,
            history: self
                .engine
                .history(actor, &client.user, None, MAX_HISTORY_PAGE)?,
        };
        let message = ServerMessage::Snapshot {
            request_id: request_id.into(),
            snapshot: Box::new(snapshot),
        };
        if self.clients[&id]
            .messages
            .try_send_snapshot(&message)
            .is_err()
        {
            // Output rejection closes the connection; it is not a request error.
            self.disconnect(id);
            return Ok(());
        }
        let ServerMessage::Snapshot { snapshot, .. } = message else {
            unreachable!()
        };
        let client = self.clients.get_mut(&id).expect("admitted client");
        client.context = Some(context);
        client.strike_inspection = None;
        client.map_transfer = None;
        client.chart_transfer = None;
        client.readiness = Some(readiness);
        client.waiting = None;
        client.observation_tick = snapshot.state.observation.tick;
        client.last_observation = Some(DisclosedObservation {
            branch: snapshot.branch,
            base: ObservationBase {
                snapshot: snapshot.state.snapshot.clone(),
                cursor: snapshot.cursor,
                revision: snapshot.state.revision,
            },
            state: snapshot.state,
        });
        self.commit_admitted_presentation(id, boundary, true);
        // Attaching sends the whole palette; a later snapshot, what changed.
        self.palette_update(id);
        Ok(())
    }

    /// Trusted backend entry point; not exposed as a client request.
    pub fn annotate_backend(
        &mut self,
        actor: ActorId,
        component: &str,
        anchor: Anchor,
        category: AnnotationCategory,
        text: &str,
    ) -> Result<HistoryEntry, Failure> {
        let entry = self
            .engine
            .annotate_backend(actor, component, anchor, category, text)?;
        let entry = self.engine.disclose_entry(&entry).ok_or_else(|| {
            Failure::new(
                ErrorCode::InvalidRequest,
                "Backend note has no disclosed event",
            )
        })?;
        self.annotation_update(&entry);
        Ok(entry)
    }

    fn annotation_update(&mut self, entry: &HistoryEntry) {
        let recipients: Vec<_> = self
            .clients
            .iter()
            .filter(|(_, client)| {
                client
                    .actor
                    .is_some_and(|actor| entry.visible_to(actor, &client.user))
            })
            .map(|(&id, _)| id)
            .collect();
        for id in recipients {
            self.update(
                id,
                UpdateBody::Annotation {
                    entry: Box::new(entry.clone()),
                },
            );
        }
    }

    /// Record every actual ownership transition, even if refresh is batched.
    /// Exhaustion fails input closed; it never wraps or rolls simulation back.
    fn set_controller(&mut self, actor: ActorId, owner: Option<u64>) -> bool {
        if self.controllers.get(&actor).copied() == owner {
            return false;
        }
        match owner {
            Some(id) => {
                self.controllers.insert(actor, id);
            }
            None => {
                self.controllers.remove(&actor);
            }
        }
        let generation = self.control_generations.entry(actor).or_insert(Some(0));
        *generation = generation.and_then(|value| value.checked_add(1));
        true
    }

    fn input_context(&self, id: u64) -> Option<InputContext> {
        let client = self.clients.get(&id)?;
        Some(InputContext {
            stream: client.context.clone()?,
            readiness_revision: client.readiness.as_ref()?.permissions.revision,
        })
    }

    fn current_readiness(&self, id: u64) -> Option<DisclosedReadiness> {
        let actor = self.clients.get(&id)?.actor?;
        self.readiness_for_client(id, self.engine.input_readiness(actor))
    }

    fn readiness_for_client(
        &self,
        id: u64,
        mut permissions: Readiness,
    ) -> Option<DisclosedReadiness> {
        let client = self.clients.get(&id)?;
        let actor = client.actor?;
        let has_control =
            client.role != AccessRole::Spectator && self.controllers.get(&actor) == Some(&id);
        let enabled = has_control
            && self.pending_pauses.is_empty()
            && self.pending_travel_cancellations.is_empty();
        if !enabled {
            permissions.admission = false;
            permissions.resume.clear();
            permissions.cancel.clear();
        } else if self.travels.contains_key(&actor) {
            permissions.admission = false;
        }
        // Read-only spectators have no action authority to invalidate. Owner
        // operand changes and control ABA must not become spectator readiness
        // signals when their disabled permissions remain unchanged.
        let (engine_input_generation, control_generation) = if client.role == AccessRole::Spectator
        {
            (0, 0)
        } else {
            (
                self.engine.input_generation(actor).ok()?,
                self.control_generations
                    .get(&actor)
                    .copied()
                    .unwrap_or(Some(0))?,
            )
        };
        let mut readiness = DisclosedReadiness {
            engine_input_generation,
            control_generation,
            has_control,
            permissions,
        };
        if let Some(previous) = &client.readiness {
            readiness.permissions.revision = previous.permissions.revision;
            if &readiness != previous {
                readiness.permissions.revision = previous.permissions.revision.checked_add(1)?;
            }
        }
        Some(readiness)
    }

    /// Query each actor once per publication pass. Output rejection can remove a
    /// controller and affect clients visited earlier; only such removals require
    /// another pass. The client count strictly decreases on every repeated pass.
    fn refresh_readiness(&mut self) {
        loop {
            let count = self.clients.len();
            let recipients: Vec<_> = self
                .clients
                .iter()
                .filter_map(|(&id, client)| {
                    client.readiness.as_ref()?;
                    Some((id, client.actor?))
                })
                .collect();
            let mut inputs = BTreeMap::new();
            for (id, actor) in recipients {
                if !self.clients.contains_key(&id) {
                    continue;
                }
                let input = inputs
                    .entry(actor)
                    .or_insert_with(|| self.engine.input_readiness(actor))
                    .clone();
                let Some(readiness) = self.readiness_for_client(id, input) else {
                    self.disconnect(id);
                    continue;
                };
                let client = self.clients.get_mut(&id).expect("connected recipient");
                if client.readiness.as_ref() == Some(&readiness) {
                    continue;
                }
                client.readiness = Some(readiness.clone());
                self.update(
                    id,
                    UpdateBody::Readiness {
                        readiness: readiness.permissions,
                    },
                );
            }
            if self.clients.len() == count {
                break;
            }
        }
    }

    fn control_update(&mut self, actor: ActorId) {
        let recipients: Vec<_> = self
            .clients
            .iter()
            .filter(|(_, client)| client.actor == Some(actor))
            .map(|(&id, _)| id)
            .collect();
        for id in recipients {
            self.update(
                id,
                UpdateBody::Control {
                    has_control: self.controllers.get(&actor) == Some(&id),
                },
            );
        }
    }

    fn update(&mut self, id: u64, body: UpdateBody) {
        self.update_at(id, body, None);
    }

    fn update_at(&mut self, id: u64, mut body: UpdateBody, committed_tick: Option<u64>) {
        if let Some(client) = self.clients.get(&id) {
            if let UpdateBody::Observation { state, .. } = &mut body {
                *state = self.item_observation(Arc::clone(state), client.role);
            }
        }
        let branch = self.engine.branch().clone();
        let Some(client) = self.clients.get_mut(&id) else {
            return;
        };
        let Some(sequence) = client.sequence.checked_add(1) else {
            self.disconnect(id);
            return;
        };
        let actor = client.actor.expect("only attached clients receive updates");
        // Control changes during a broadcast retain the last admitted tick.
        let tick = match &body {
            UpdateBody::Observation { state, .. } => state.observation.tick,
            _ => committed_tick.unwrap_or(client.observation_tick),
        };
        let previous = client
            .last_observation
            .as_ref()
            .filter(|previous| previous.branch == branch)
            .map(|previous| (previous.base.clone(), previous.state.as_ref()));
        let message = ServerMessage::Update {
            update: Box::new(StreamUpdate {
                context: client.context.clone().expect("attached stream context"),
                actor,
                branch: branch.clone(),
                cursor: StreamCursor { sequence, tick },
                body,
            }),
        };
        if client
            .messages
            .try_send_with(&message, |message, limit| {
                encode_response(message, previous, limit).map(|encoded| encoded.text)
            })
            .is_err()
        {
            self.disconnect(id);
            return;
        }
        // Only an admitted output establishes the next disclosure base.
        client.sequence = sequence;
        client.observation_tick = tick;
        client.waiting = None;
        let ServerMessage::Update { update } = message else {
            unreachable!()
        };
        if let UpdateBody::Observation { state, .. } = update.body {
            client.last_observation = Some(DisclosedObservation {
                branch,
                base: ObservationBase {
                    snapshot: state.snapshot.clone(),
                    cursor: StreamCursor { sequence, tick },
                    revision: state.revision,
                },
                state,
            });
        }
    }

    fn reply_context(&self, id: u64) -> Option<ReplyContext> {
        let client = self.clients.get(&id)?;
        let observed = client.last_observation.as_ref()?;
        Some(ReplyContext {
            snapshot: observed.state.snapshot.clone(),
            input: self.input_context(id)?,
            actor: client.actor?,
            branch: observed.branch.clone(),
            cursor: StreamCursor {
                sequence: client.sequence,
                tick: client.observation_tick,
            },
            revision: observed.state.revision,
        })
    }

    fn publish_reply(&mut self, id: u64, request_id: String, reply: RequestReply) {
        if let Some(context) = self.reply_context(id) {
            self.send(id, reply.message(request_id, context));
        }
    }

    fn error_scope(&self, id: u64) -> ErrorScope {
        match self.reply_context(id) {
            Some(context) => ErrorScope::Attached { context },
            None => ErrorScope::Unattached {},
        }
    }

    fn session_receipt(&self, id: u64) -> Option<RequestReceipt> {
        let actor = self.clients.get(&id)?.actor?;
        Some(RequestReceipt::Immediate {
            actor,
            branch: self.engine.branch().clone(),
            entry_id: None,
        })
    }

    /// Admit an ordinary response before the caller commits disclosure state.
    /// Output failure ends this stream; it must never silently skip a response.
    fn admit_output(&mut self, id: u64, message: &ServerMessage) -> bool {
        let Some(client) = self.clients.get(&id) else {
            return false;
        };
        if client
            .messages
            .try_send_with(message, encode_bounded_json)
            .is_err()
        {
            self.disconnect(id);
            return false;
        }
        true
    }

    fn send(&mut self, id: u64, message: ServerMessage) {
        if !self.admit_output(id, &message) {
            return;
        }
        // A successfully admitted answer or disclosure needs a fresh waiting word.
        if matches!(
            message,
            ServerMessage::Update { .. }
                | ServerMessage::Snapshot { .. }
                | ServerMessage::Ack { .. }
        ) {
            self.clients.get_mut(&id).expect("admitted client").waiting = None;
        }
    }

    /// Tell a client its actor left the loaded world, then disconnect it.
    fn detach_unloaded(&mut self, id: u64) {
        self.send(
            id,
            ServerMessage::Error {
                scope: self.error_scope(id),
                request_id: None,
                code: ErrorCode::NotAttached,
                message: "Your actor left the loaded world; attach again once it's back in play"
                    .into(),
            },
        );
        self.disconnect_with(id, DisconnectMode::Drain);
    }

    /// Whether this stream still owns session state. Deferred ingress must not
    /// retain requests after explicit, output-pressure or lifecycle disconnects.
    pub(crate) fn is_connected(&self, id: u64) -> bool {
        self.clients.contains_key(&id)
    }

    pub(crate) fn disconnect(&mut self, id: u64) {
        self.settled_views = None;
        self.disconnect_with(id, DisconnectMode::Abort);
    }

    fn disconnect_with(&mut self, id: u64, mode: DisconnectMode) {
        self.attachments.remove(&id);
        let Some(client) = self.clients.remove(&id) else {
            return;
        };
        let _ = client.close.send(Some(mode));
        if let Some(actor) = client.actor {
            if self.controllers.get(&actor) == Some(&id) {
                self.stop_travel(actor, TravelPhase::ControlLost);
            }
            if self.controllers.get(&actor) == Some(&id) {
                self.set_controller(actor, None);
                self.pending_pauses.insert(actor);
                if !self.engine.unattended_arena() {
                    self.autonomous_enabled = false;
                }
                if !self.resetting_streams {
                    self.control_update(actor);
                }
            }
            if !self
                .clients
                .values()
                .any(|other| other.actor == Some(actor))
            {
                self.engine.unregister_input_observer(actor);
                self.control_generations.remove(&actor);
            }
        }
    }

    /// Saving is behind schedule: play is held until the writer catches up.
    pub(crate) fn saving_overdue(&self) -> bool {
        self.engine.save_status().overdue
    }

    pub(crate) fn poll_saves(&mut self) {
        self.settled_views = None;
        self.expire_attachments();
        let clients_before_warnings = self.clients.len();
        let status = self.engine.save_status();
        let warning = status.error.clone().or_else(|| {
            status.overdue.then(|| {
                "Saving is behind schedule; recent play may be lost if the server stops.".into()
            })
        }).or_else(|| {
            status.checkpoint_deferred_sequence.map(|_| {
                "Checkpoint deferred because it exceeds capacity; journal saving continues. Restart may take longer.".into()
            })
        });
        if warning != self.save_warning {
            if let Some(message) = &warning {
                if let Some(diagnostics) = &self.diagnostics {
                    diagnostics.warning(message);
                }
                // Never interleave a warning with the welcome/attach handshake.
                for id in self
                    .clients
                    .iter()
                    .filter_map(|(id, client)| client.actor.map(|_| *id))
                    .collect::<Vec<_>>()
                {
                    self.send(
                        id,
                        ServerMessage::Error {
                            scope: self.error_scope(id),
                            request_id: None,
                            code: ErrorCode::StorageFailure,
                            message: message.clone(),
                        },
                    );
                }
            }
            self.save_warning = warning;
        }
        let pending = std::mem::take(&mut self.pending_saves);
        if pending.is_empty() {
            if self.clients.len() != clients_before_warnings {
                self.refresh_readiness();
            }
            return;
        }
        self.refresh_readiness();
        let clients = self.clients.len();
        for (id, request_id, target, queued) in pending {
            if !self.clients.contains_key(&id) {
                continue;
            }
            match save_reply(&status, target, queued.elapsed(), self.save_reply_wait) {
                SaveReply::Durable => {
                    self.engine.survival_save_barrier(status.durable_sequence);
                    if let Some(receipt) = self.session_receipt(id) {
                        self.publish_reply(id, request_id, RequestReply::Receipt(receipt));
                    }
                }
                SaveReply::Failed(message) => self.send(
                    id,
                    ServerMessage::Error {
                        scope: self.error_scope(id),
                        request_id: Some(request_id),
                        code: ErrorCode::StorageFailure,
                        message,
                    },
                ),
                SaveReply::Behind => self.send(
                    id,
                    ServerMessage::Error {
                        scope: self.error_scope(id),
                        request_id: Some(request_id),
                        code: ErrorCode::SaveBehind,
                        message: format!(
                            "Saving is behind schedule: saved through {}, this save needs {}.                              It stays queued and completes when the writer catches up.",
                            status.durable_sequence, target
                        ),
                    },
                ),
                SaveReply::Wait => self.pending_saves.push((id, request_id, target, queued)),
            }
        }
        if self.clients.len() != clients {
            self.refresh_readiness();
        }
    }
    pub(crate) fn flush_handle(&self) -> Option<crate::storage::Store> {
        self.engine.flush_handle()
    }
    pub(crate) fn shutdown(&mut self) {
        self.settled_views = None;
        for id in self.clients.keys().copied().collect::<Vec<_>>() {
            self.disconnect(id);
        }
    }
}

/// How long an explicit save waits for durability before the client is told
/// saving is behind; well inside a client's reply deadline.
pub(crate) const SAVE_REPLY_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Debug, PartialEq, Eq)]
enum SaveReply {
    Durable,
    Failed(String),
    /// Not durable within the reply wait; the save stays queued.
    Behind,
    Wait,
}

/// A pending explicit save's answer: durable, failed, behind or not yet due.
fn save_reply(
    status: &crate::SaveStatus,
    target: u64,
    waited: std::time::Duration,
    limit: std::time::Duration,
) -> SaveReply {
    if status.durable_sequence >= target {
        SaveReply::Durable
    } else if let Some(message) = &status.error {
        SaveReply::Failed(message.clone())
    } else if waited >= limit {
        SaveReply::Behind
    } else {
        SaveReply::Wait
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;

/// Running until blocked: journeys, outgoing queues and slow clients, and the
/// runner over its mailbox (see docs/architecture/simulation.md).
#[cfg(test)]
#[path = "run_tests.rs"]
mod run_tests;

#[cfg(test)]
#[path = "strike_page_session_tests.rs"]
mod strike_page_tests;

#[cfg(test)]
#[path = "advancement_session_tests.rs"]
mod advancement_tests;

#[cfg(test)]
#[path = "../test_support/public_plan_sessions.rs"]
mod public_plan_observation_tests;

#[cfg(test)]
#[path = "locomotion_session_tests.rs"]
mod locomotion_tests;

#[cfg(test)]
#[path = "perception_session_tests.rs"]
mod perception_tests;

#[cfg(test)]
#[path = "corrosion_session_tests.rs"]
mod corrosion_tests;

#[cfg(test)]
#[path = "ray_session_tests.rs"]
mod ray_tests;

#[cfg(test)]
#[path = "magic_item_session_tests.rs"]
mod magic_item_tests;

#[cfg(test)]
#[path = "weapon_selection_session_tests.rs"]
mod weapon_selection_tests;

#[cfg(test)]
mod input_operand_context_tests {

    #[test]
    fn spectator_readiness_ignores_owner_inventory_and_control_generations() {
        for inventory_change in [true, false] {
            let (mut service, controller) = fixture();
            let actor = ActorId(1);
            let account = Account {
                role: AccessRole::Spectator,
                user: "readiness-spectator".into(),
                token: "spectator-test".into(),
                actors: BTreeSet::from([actor]),
            };
            let mut spectator = service.connect(&account, "headless".into()).unwrap();
            service.handle(
                spectator.id,
                "attach-spectator".into(),
                Request::Attach { actor },
            );
            while spectator.messages.try_recv().is_ok() {}
            let spectator_context = service.input_context(spectator.id).unwrap();
            let spectator_readiness = service.current_readiness(spectator.id).unwrap();
            assert!(!spectator_readiness.permissions.admission);
            assert!(spectator_readiness.permissions.resume.is_empty());
            assert!(spectator_readiness.permissions.cancel.is_empty());
            let controller_readiness = service.current_readiness(controller.id).unwrap();
            let before = service.engine.state(actor).unwrap();
            if inventory_change {
                // Inventory presence is public. Its private authority generation must
                // nevertheless not turn spectator readiness into an owner-input clock.
                let item = before.observation.ground_items[0].item.id;
                let generation = service.engine.input_generation(actor).unwrap();
                let wire = Command::Act {
                    action: Action::Take {
                        item,
                        quantity: None,
                    },
                };
                let decoded = crate::wire_adapter::decode_command(&wire).unwrap();
                let branch = service.engine.branch().clone();
                let command = service
                    .engine
                    .resolve_command(actor, &branch, decoded)
                    .unwrap();
                service
                    .engine
                    .command(
                        "input-test",
                        "headless",
                        actor,
                        "take-for-authority",
                        &branch,
                        command,
                    )
                    .unwrap();
                service.engine.execute_next_intention().unwrap().unwrap();
                assert!(service
                    .engine
                    .state(actor)
                    .unwrap()
                    .observation
                    .inventory
                    .iter()
                    .any(|held| held.id == item));
                assert!(service.engine.input_generation(actor).unwrap() > generation);
            } else {
                assert!(service.set_controller(actor, None));
                assert!(service.set_controller(actor, Some(controller.id)));
                assert_eq!(
                    service.engine.state(actor).unwrap(),
                    before,
                    "control ABA changes authority, not the disclosed observation"
                );
            }
            assert_ne!(
                service.current_readiness(controller.id).unwrap(),
                controller_readiness,
                "owner authority must still invalidate"
            );
            assert_eq!(
                service.current_readiness(spectator.id).unwrap(),
                spectator_readiness,
                "spectator permissions and input context must not expose owner generation changes"
            );
            service.refresh_readiness();
            assert_eq!(
                service.input_context(spectator.id).unwrap(),
                spectator_context
            );
            let updates: Vec<_> =
                std::iter::from_fn(|| spectator.messages.try_recv().ok()).collect();
            assert!(
                !updates.iter().any(|message| matches!(message,
                    ServerMessage::Update { update, .. }
                        if matches!(&update.body, UpdateBody::Readiness { .. })
                )),
                "spurious spectator readiness update: {updates:?}"
            );
        }
    }

    #[test]
    fn input_registration_survives_one_viewer_and_ends_with_last_disconnect() {
        let actor = ActorId(1);
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        engine.enable_wizard().unwrap();
        let mut service = Service::new(engine);
        let account = Account {
            role: AccessRole::Player,
            user: "registration-test".into(),
            token: "test".into(),
            actors: BTreeSet::from([actor]),
        };
        let first = service.connect(&account, "first-viewer".into()).unwrap();
        service.handle(first.id, "attach-first".into(), Request::Attach { actor });
        assert_eq!(service.engine.input_generation(actor).unwrap(), 0);
        assert!(
            service.controllers.is_empty(),
            "observers acquire no control"
        );
        teleport(
            &mut service,
            actor,
            "registration-move",
            Position {
                region: 2,
                x: 1,
                y: 1,
                z: 0,
            },
        );
        let generation = service.engine.input_generation(actor).unwrap();
        assert!(generation > 0, "positive noninitial registration witness");
        let second = service.connect(&account, "second-viewer".into()).unwrap();
        service.handle(second.id, "attach-second".into(), Request::Attach { actor });
        assert_eq!(service.engine.input_generation(actor).unwrap(), generation);
        assert_eq!(service.clients[&first.id].actor, Some(actor));
        assert_eq!(service.clients[&second.id].actor, Some(actor));
        service.disconnect(first.id);
        assert!(!service.clients.contains_key(&first.id));
        assert!(service.clients.contains_key(&second.id));
        assert_eq!(service.engine.input_generation(actor).unwrap(), generation);
        assert!(service.pending_pauses.is_empty());
        service.disconnect(second.id);
        assert!(service.engine.input_generation(actor).is_err());
        assert!(!service.control_generations.contains_key(&actor));
        assert!(service.pending_pauses.is_empty());
    }
    use super::*;
    use crate::journal::{Command as BackendCommand, Position, WizardOperation};
    use crate::{ActorSetup, Scenario};

    fn fixture() -> (Service, Connection) {
        let mut scenario = Scenario::two_room(42);
        scenario.actors.push(ActorSetup {
            position: Position {
                region: 1,
                x: 2,
                y: 1,
                z: 0,
            },
            turn_ticks: 100,
        });
        let mut engine = Engine::memory(scenario).unwrap();
        engine.enable_wizard().unwrap();
        let mut service = Service::new(engine);
        let account = Account {
            role: AccessRole::Player,
            user: "input-test".into(),
            token: "test".into(),
            actors: BTreeSet::from([ActorId(1)]),
        };
        let mut client = service.connect(&account, "headless".into()).unwrap();
        service.handle(
            client.id,
            "attach".into(),
            Request::Attach { actor: ActorId(1) },
        );
        service.handle(client.id, "control".into(), Request::AcquireControl);
        while client.messages.try_recv().is_ok() {}
        (service, client)
    }
    fn teleport(service: &mut Service, target: ActorId, request: &str, position: Position) {
        let actor = ActorId(1);
        service
            .engine
            .command(
                "test",
                "test",
                actor,
                request,
                &service.engine.branch().clone(),
                BackendCommand::Wizard {
                    display_basis: None,
                    expected_boundary: service.engine.causal_boundary(actor).unwrap(),
                    operation: WizardOperation::Teleport {
                        actor: target,
                        position,
                    },
                },
            )
            .unwrap();
    }
    fn wait(service: &mut Service, client: &Connection, context: InputContext, request: &str) {
        service.handle(
            client.id,
            request.into(),
            Request::Command {
                context,
                branch: service.engine.branch().clone(),
                command: Command::Act {
                    action: Action::Wait,
                },
            },
        );
    }
    #[test]
    fn unrelated_display_change_accepts_original_context_and_retry_exactly_once() {
        let (mut service, mut client) = fixture();
        let context = service.input_context(client.id).unwrap();
        let revision = service.engine.causal_boundary(ActorId(1)).unwrap();
        teleport(
            &mut service,
            ActorId(2),
            "neighbor",
            Position {
                region: 1,
                x: 3,
                y: 1,
                z: 0,
            },
        );
        assert!(service.engine.causal_boundary(ActorId(1)).unwrap() > revision);
        assert_eq!(
            service.current_readiness(client.id),
            service.clients[&client.id].readiness
        );
        wait(&mut service, &client, context.clone(), "original");
        let admitted: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
        assert!(
            admitted
                .iter()
                .any(|m| matches!(m, ServerMessage::Ack { .. })),
            "{admitted:?}"
        );
        assert!(service.engine.has_pending_intention(ActorId(1)));
        let sequence = service.engine.save_status().accepted_sequence;
        // The admission changed input authority. Exact receipt retry still wins first.
        assert_ne!(service.input_context(client.id).unwrap(), context);
        wait(&mut service, &client, context, "original");
        let retry: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
        assert!(
            retry.iter().any(|m| matches!(m, ServerMessage::Ack { .. })),
            "{retry:?}"
        );
        assert_eq!(service.engine.save_status().accepted_sequence, sequence);
    }
    #[test]
    fn origin_and_control_aba_reject_without_publication_or_admission() {
        for control in [false, true] {
            let (mut service, mut client) = fixture();
            let context = service.input_context(client.id).unwrap();
            if control {
                assert!(service.set_controller(ActorId(1), None));
                assert!(service.set_controller(ActorId(1), Some(client.id)));
            } else {
                teleport(
                    &mut service,
                    ActorId(1),
                    "away",
                    Position {
                        region: 2,
                        x: 1,
                        y: 1,
                        z: 0,
                    },
                );
                teleport(
                    &mut service,
                    ActorId(1),
                    "back",
                    Position {
                        region: 1,
                        x: 1,
                        y: 1,
                        z: 0,
                    },
                );
            }
            assert_eq!(
                service.input_context(client.id).unwrap(),
                context,
                "no intermediate publication"
            );
            let sequence = service.engine.save_status().accepted_sequence;
            wait(&mut service, &client, context, "stale");
            let replies: Vec<_> = std::iter::from_fn(|| client.messages.try_recv().ok()).collect();
            assert!(
                replies.iter().any(|m| matches!(
                    m,
                    ServerMessage::Error {
                        code: ErrorCode::StaleContext,
                        ..
                    }
                )),
                "{replies:?}"
            );
            assert!(!service.engine.has_pending_intention(ActorId(1)));
            assert_eq!(service.engine.save_status().accepted_sequence, sequence);
        }
    }
}

#[path = "presentation_queries.rs"]
mod presentation_queries;

#[cfg(test)]
#[path = "palette_outcome_tests.rs"]
mod palette_outcome_tests;

#[cfg(test)]
#[path = "outcome_activity_tests.rs"]
mod outcome_activity_tests;
