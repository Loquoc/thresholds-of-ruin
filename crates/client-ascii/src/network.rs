use std::{
    future::Future,
    net::SocketAddr,
    sync::mpsc::{self, Receiver, SyncSender},
    thread::JoinHandle,
    time::Duration,
};
use tokio::{sync::mpsc as async_mpsc, time::timeout};
use tor_client_common::{server_before_local, Connection, FirstReady, Palette};
use tor_protocol::*;

type Error = Box<dyn std::error::Error + Send + Sync>;
pub enum Event {
    Palette(Palette),
    Role(AccessRole),
    Snapshot(Box<Snapshot>),
    Update(Box<StreamUpdate>),
    MapMemory(Box<tor_client_common::MapMemoryUpdate>),
    ChartPresentation(tor_client_common::charts::ChartPresentation),
    ChartCancelled(chart_pages::ChartQuery),
    Status(String),
    Admission(tor_protocol::RequestReceipt),
    InputYield,
    /// The server refused a request, in the player's words.
    Refused(String),
    Ready,
    History(HistoryPage),
    Discoveries(Vec<ItemDiscoveryView>),
    CreatureInspection(Box<CreatureInspectionView>),
    CreatureCatalog(Box<CreatureCatalogPage>),
    WizardReport(Box<WizardReport>),
    ProjectileChoicesSent(String),
    ProjectileChoices {
        context: ReplyContext,
        request_id: String,
        page: Box<ProjectileChoicePage>,
    },
    WeaponPreviewSent(String),
    AbilityWeaponPreview {
        context: ReplyContext,
        request_id: String,
        preview: Box<AbilityWeaponPreview>,
    },
    StrikeQuerySent(String),
    OverviewPrepared(u64),
    AdvancementQuerySent(String),
    AdvancementPage {
        context: ReplyContext,
        request_id: String,
        page: Box<AdvancementPage>,
    },
    CreatureStrikePage {
        context: ReplyContext,
        request_id: String,
        page: Box<CreatureStrikePage>,
    },
    CombatDiagnostics(Box<CombatDiagnosticsView>),
    Fatal(String),
}

/// What the window asks of the network worker.
pub enum Command {
    Capture(tor_client_common::captures::CapturedAttachment),
    CancelCharts,
    OpenOverview(Box<tor_client_common::advancement::OpenOverview>),
    Request {
        context: StreamContext,
        request: Box<Request>,
    },
    /// Show what has arrived without waiting.
    Skip,
    /// Space shown updates this far apart.
    Pace(Duration),
}

pub struct Network {
    pub commands: async_mpsc::Sender<Command>,
    pub events: Receiver<Event>,
    worker: JoinHandle<Result<(), String>>,
}

impl Network {
    pub fn start(
        address: SocketAddr,
        token: String,
        actor: ActorId,
        observe: bool,
        pace: Duration,
    ) -> Self {
        let (commands, rx) = async_mpsc::channel(4);
        let (tx, events) = mpsc::sync_channel(64);
        let worker = std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| -> Error { e.into() })
                .and_then(|runtime| {
                    runtime.block_on(run(address, token, actor, observe, pace, rx, &tx))
                });
            if let Err(error) = &result {
                let _=tx.try_send(Event::Fatal(format!("Connection ended: {error}. Relaunch to reconnect; inspect history before retrying.")));
            }
            result.map_err(|error| error.to_string())
        });
        Self {
            commands,
            events,
            worker,
        }
    }

    pub fn shutdown(self) -> Result<(), Error> {
        drop(self.commands);
        // Drain presentation while the worker performs save-and-quit.
        for _ in self.events {}
        self.worker
            .join()
            .map_err(|_| "Network worker failed")?
            .map_err(Into::into)
    }
}

// Backpressure stays on this dedicated worker, never the native event loop.
// The server retains its bounded slow-client disconnect/snapshot-on-relaunch
// contract. Do not drop intermediate observations or grow this queue.
fn publish(tx: &SyncSender<Event>, event: Event) -> Result<(), Error> {
    tx.send(event)
        .map_err(|_| "Window stopped consuming network updates".into())
}

/// Apply an already-delivered server boundary before considering a queued
/// native command. Preserve its original input context and any query revision.
async fn next_worker_input(
    server: impl Future<Output = Result<ServerMessage, Error>>,
    command: impl Future<Output = Option<Command>>,
) -> FirstReady<Result<ServerMessage, Error>, Option<Command>> {
    server_before_local(server, command).await
}

async fn run(
    address: SocketAddr,
    token: String,
    actor: ActorId,
    observe: bool,
    pace: Duration,
    mut rx: async_mpsc::Receiver<Command>,
    tx: &SyncSender<Event>,
) -> Result<(), Error> {
    let mut connection = Connection::connect(address, token, actor, "ascii").await?;
    connection.use_external_advancement_presentation();
    connection.set_pace(pace);
    publish(tx, Event::Role(connection.role()))?;
    let mut shown_palette = (None, false);
    publish_palette_change(&mut shown_palette, &connection.palette, tx)?;
    publish(
        tx,
        Event::Snapshot(Box::new(connection.state.reset_snapshot().map_err(
            |error| format!("Initial session is not a reset: {error:?}"),
        )?)),
    )?;
    if connection.role() == AccessRole::Spectator {
        publish(
            tx,
            Event::Status("Spectator access is read-only. F2: history; Esc: close.".into()),
        )?;
    } else if !observe {
        transact(
            &mut connection,
            &mut shown_palette,
            Request::AcquireControl,
            tx,
        )
        .await?;
    } else {
        publish(
            tx,
            Event::Status("Observing. Press F3 to request control.".into()),
        )?;
    }
    publish(tx, Event::Ready)?;
    loop {
        match next_worker_input(next(&mut connection, &mut shown_palette, tx), rx.recv()).await {
            FirstReady::Server(message) => {
                let message = message?;
                let reset = matches!(message, ServerMessage::Snapshot { .. });
                present(message, tx)?;
                if reset {
                    publish(tx, Event::Ready)?;
                }
            }
            FirstReady::Local(command) => match command {
                None => {
                    connection.close().await?;
                    return Ok(());
                }
                Some(Command::Capture(capture)) => {
                    let mut charts_loading = connection.charts.loading();
                    let result = tor_client_common::captures::upload(&mut connection, capture, |connection, message| {
                        if matches!(message, ServerMessage::MapPage { page, .. } if page.next.is_none()) {
                            if let Some(update) = connection.state.map_memory_update() {
                                publish(tx, Event::MapMemory(Box::new(update)))?;
                            }
                        }
                        publish_chart_result(charts_loading, &connection.charts, message, tx)?;
                        charts_loading = connection.charts.loading();
                        publish_palette_change(&mut shown_palette, &connection.palette, tx)?;
                        present(message.clone(), tx)
                    }).await;
                    match result {
                        Ok(()) => publish(tx, Event::Status("Screenshot accepted into game history; normal save flushing still applies.".into()))?,
                        Err(error) => publish(tx, Event::Refused(format!("Screenshot upload: {error}")))?,
                    }
                    publish(tx, Event::Ready)?;
                }
                Some(Command::OpenOverview(plan)) => {
                    if !connection.is_synchronized() {
                        publish(
                            tx,
                            Event::Refused("Resynchronizing; overview was not sent.".into()),
                        )?;
                        publish(tx, Event::Ready)?;
                        continue;
                    }
                    if let Err(error) = plan.validate_attachment(&connection.state) {
                        publish(tx, Event::Refused(error))?;
                        publish(tx, Event::Ready)?;
                        continue;
                    }
                    if !transact_observation_result(
                        &mut connection,
                        &mut shown_palette,
                        plan.observation_request(),
                        tx,
                    )
                    .await?
                    {
                        publish(
                            tx,
                            Event::Refused(
                                "Observation was not confirmed. Open advancement again.".into(),
                            ),
                        )?;
                        publish(tx, Event::Ready)?;
                        continue;
                    }
                    let request = plan.finish(&connection.state).and_then(|()| {
                        connection.advancement_pages.request(
                            tor_client_common::advancement::AdvancementInput::Overview,
                            &connection.state,
                        )
                    });
                    match request {
                        Ok(Some(request)) => {
                            publish(
                                tx,
                                Event::OverviewPrepared(connection.state.state().revision),
                            )?;
                            transact(&mut connection, &mut shown_palette, request, tx).await?;
                        }
                        Ok(None) => {}
                        Err(error) => publish(tx, Event::Refused(error))?,
                    }
                    publish(tx, Event::Ready)?;
                }
                Some(Command::Request { context, request }) => {
                    if !connection.is_synchronized() {
                        publish(
                            tx,
                            Event::Status("Resynchronizing; request was not sent.".into()),
                        )?;
                        continue;
                    }
                    let stale_input = matches!(request.as_ref(), Request::Command { context, .. }
                        if context != &connection.state.input_context());
                    if &context != connection.state.context() || stale_input {
                        publish(
                            tx,
                            Event::Refused(
                                "State changed; request was not sent. Review the new state.".into(),
                            ),
                        )?;
                        publish(tx, Event::Ready)?;
                        continue;
                    }
                    if matches!(request.as_ref(), Request::Chart { query } if query.base != connection.state.observation_base()
                        || query.context != *connection.state.context() || query.branch != *connection.state.branch())
                    {
                        publish(
                            tx,
                            Event::Refused("Chart view changed; select the chart again.".into()),
                        )?;
                        publish(tx, Event::Ready)?;
                        continue;
                    }
                    // Observation revisions can advance without a new input
                    // generation while the native UI is still presenting old
                    // state. Refuse that queued query before local page tracking
                    // mistakes its stale revision for a fatal transport failure.
                    let stale_advancement = matches!(request.as_ref(), Request::Command {
                        command: tor_protocol::Command::Advancement { expected_revision, .. }, ..
                    } if *expected_revision != connection.state.state().revision);
                    if stale_advancement {
                        publish(tx, Event::Refused("Advancement changed; request was not sent. Request overview and explicitly preview again.".into()))?;
                        publish(tx, Event::Ready)?;
                        continue;
                    }
                    transact(&mut connection, &mut shown_palette, *request, tx).await?;
                    publish(tx, Event::Ready)?;
                }
                Some(Command::CancelCharts) => connection.cancel_charts(),
                Some(Command::Skip) => connection.skip(),
                Some(Command::Pace(pace)) => connection.set_pace(pace),
            },
        }
    }
}

/// Transport owns recovery; the UI gets a copy only when the palette revision
/// or currentness changes. In particular a missed delta redraws the fallback
/// immediately, and a full recovery redraws without an observation update.
async fn next(
    connection: &mut Connection,
    shown: &mut (Option<u64>, bool),
    tx: &SyncSender<Event>,
) -> Result<ServerMessage, Error> {
    next_with_chart_timeout(connection, shown, tx, Duration::from_secs(10)).await
}

async fn next_with_chart_timeout(
    connection: &mut Connection,
    shown: &mut (Option<u64>, bool),
    tx: &SyncSender<Event>,
    chart_timeout: Duration,
) -> Result<ServerMessage, Error> {
    loop {
        // Re-entry also preserves palette invalidations when a command cancels
        // the previous receive future during recovery I/O.
        publish_palette_change(shown, &connection.palette, tx)?;
        let was_loading = connection.charts.loading();
        let cancellation = was_loading
            .then(|| {
                connection
                    .state
                    .state()
                    .observation
                    .charts
                    .iter()
                    .find(|summary| Some(summary.kind) == connection.charts.selected())
                    .and_then(|summary| summary.chart.as_ref())
                    .map(|chart| chart_pages::ChartQuery {
                        context: connection.state.context().clone(),
                        branch: connection.state.branch().clone(),
                        base: connection.state.observation_base(),
                        chart: chart.clone(),
                        cursor: None,
                    })
            })
            .flatten();
        let message = if let Some(query) = cancellation {
            match timeout(chart_timeout, connection.next()).await {
                Ok(result) => result?,
                Err(_) => {
                    // This is local presentation failure, not a fabricated
                    // server reply or a change to the advertised sample status.
                    connection.cancel_charts();
                    publish(tx, Event::ChartCancelled(query))?;
                    continue;
                }
            }
        } else {
            connection.next().await?
        };
        if matches!(&message, ServerMessage::MapPage {page,..} if page.next.is_none()) {
            if let Some(update) = connection.state.map_memory_update() {
                publish(tx, Event::MapMemory(Box::new(update)))?;
            }
        }
        publish_chart_result(was_loading, &connection.charts, &message, tx)?;
        publish_palette_change(shown, &connection.palette, tx)?;
        if matches!(&message, ServerMessage::Waiting { on }
            if tor_client_common::compositor::is_input_yield(*on))
            && connection.is_synchronized()
        {
            publish(tx, Event::InputYield)?;
        }
        return Ok(message);
    }
}

fn publish_chart_result(
    was_loading: bool,
    charts: &tor_client_common::charts::ChartPages,
    message: &ServerMessage,
    tx: &SyncSender<Event>,
) -> Result<(), Error> {
    if (was_loading && !charts.loading())
        || matches!(message, ServerMessage::ChartPage { page, .. } if page.next.is_none())
        || matches!(message, ServerMessage::Error { request_id: Some(id), .. } if id.starts_with("chart-"))
    {
        if let Some(update) = charts.presentation() {
            publish(tx, Event::ChartPresentation(update))?;
        }
    }
    Ok(())
}

fn publish_palette_change(
    shown: &mut (Option<u64>, bool),
    palette: &Palette,
    tx: &SyncSender<Event>,
) -> Result<(), Error> {
    let current = (palette.revision(), palette.stale());
    if *shown != current {
        publish(tx, Event::Palette(palette.clone()))?;
        *shown = current;
    }
    Ok(())
}

async fn transact(
    connection: &mut Connection,
    shown_palette: &mut (Option<u64>, bool),
    request: Request,
    tx: &SyncSender<Event>,
) -> Result<(), Error> {
    transact_observation_result(connection, shown_palette, request, tx)
        .await
        .map(|_| ())
}

/// True only when the exact explicit Observe demand completed successfully.
async fn transact_observation_result(
    connection: &mut Connection,
    shown_palette: &mut (Option<u64>, bool),
    request: Request,
    tx: &SyncSender<Event>,
) -> Result<bool, Error> {
    let advancement_query = matches!(
        &request,
        Request::Command {
            command: tor_protocol::Command::Advancement { .. },
            ..
        }
    );
    let strike_query = matches!(
        &request,
        Request::Command {
            command: tor_protocol::Command::InspectStrikeSource { .. },
            ..
        }
    );
    let projectile_choices = matches!(
        &request,
        Request::Command {
            command: tor_protocol::Command::ProjectileChoices { .. },
            ..
        }
    );
    let weapon_preview = matches!(
        &request,
        Request::Command {
            command: tor_protocol::Command::PreviewAbilityWeapon { .. },
            ..
        }
    );
    let id = connection.request(request).await?;
    if projectile_choices {
        publish(tx, Event::ProjectileChoicesSent(id.clone()))?;
    }
    if weapon_preview {
        publish(tx, Event::WeaponPreviewSent(id.clone()))?;
    }
    if advancement_query {
        publish(tx, Event::AdvancementQuerySent(id.clone()))?;
    }
    if strike_query {
        publish(tx, Event::StrikeQuerySent(id.clone()))?;
    }
    let mut refresh_preview = weapon_preview || projectile_choices;
    let mut pending = tor_client_common::PendingRequest::new(id.clone());
    let mut observed = false;
    timeout(Duration::from_secs(10), async {
        loop {
            let message = next(connection, shown_palette, tx).await?;
            observed |= connection.completed_observation_request(&id, &message);
            let completion = pending.observe(connection, &message);
            let complete = completion.is_some();
            if let Some(tor_client_common::RequestCompletion::Reply(
                tor_client_common::ConfirmedReply::Receipt(receipt @ tor_protocol::RequestReceipt::Admitted { .. }),
            )) = completion.as_ref()
            {
                publish(tx, Event::Admission((*receipt).clone()))?;
            }
            let unknown = matches!(completion, Some(tor_client_common::RequestCompletion::Unknown));
            let stale_preview = refresh_preview && complete && matches!(&message, ServerMessage::Error {
                code: ErrorCode::StaleRevision | ErrorCode::StaleContext | ErrorCode::WrongBranch, ..
            });
            present(message, tx)?;
            if stale_preview {
                refresh_preview = false;
                let id = connection.request(Request::Snapshot).await?;
                pending = tor_client_common::PendingRequest::new(id);
                publish(tx, Event::Status("Observation changed; refreshing before you choose the exact action again.".into()))?;
                continue;
            }
            if unknown {
                publish(tx, Event::Status("State resynchronized; the previous request may have completed. Inspect history before retrying.".into()))?;
            }
            if complete { return Ok::<(), Error>(()); }
        }
    })
    .await
    .map_err(|_| "Response timed out; the last request may have committed")??;
    Ok(observed)
}

fn present(message: ServerMessage, tx: &SyncSender<Event>) -> Result<(), Error> {
    match message {
        ServerMessage::ProjectileChoices {
            context,
            request_id,
            page,
        } => publish(
            tx,
            Event::ProjectileChoices {
                context,
                request_id,
                page,
            },
        ),
        ServerMessage::AbilityWeaponPreview {
            context,
            request_id,
            preview,
        } => publish(
            tx,
            Event::AbilityWeaponPreview {
                context,
                request_id,
                preview,
            },
        ),
        ServerMessage::AdvancementPage {
            context,
            request_id,
            page,
        } => publish(
            tx,
            Event::AdvancementPage {
                context,
                request_id,
                page,
            },
        ),
        ServerMessage::Update { update } => publish(tx, Event::Update(update)),
        ServerMessage::Snapshot { snapshot, .. } => publish(tx, Event::Snapshot(snapshot)),
        ServerMessage::Ack {
            receipt: tor_protocol::RequestReceipt::Admitted { .. },
            ..
        } => Ok(()),
        ServerMessage::Ack {
            receipt: tor_protocol::RequestReceipt::Immediate { .. },
            ..
        } => publish(tx, Event::Status("Done.".into())),
        ServerMessage::Error { code, message, .. } => publish(
            tx,
            Event::Refused(tor_client_ascii::plain_error(code, &message)),
        ),
        ServerMessage::History { page, .. } => {
            publish(tx, Event::History(page))?;
            publish(tx, Event::Status("History loaded.".into()))
        }
        ServerMessage::Discoveries { rows, .. } => publish(tx, Event::Discoveries(rows)),
        ServerMessage::CreatureCatalog { page, .. } => publish(tx, Event::CreatureCatalog(page)),
        ServerMessage::WizardReport { report, .. } => publish(tx, Event::WizardReport(report)),
        ServerMessage::CombatDiagnostics { report, .. } => {
            publish(tx, Event::CombatDiagnostics(report))
        }
        ServerMessage::CreatureStrikePage {
            context,
            request_id,
            page,
        } => publish(
            tx,
            Event::CreatureStrikePage {
                context,
                request_id,
                page,
            },
        ),
        ServerMessage::CreatureInspection { report, .. } => {
            publish(tx, Event::CreatureInspection(report))
        }
        ServerMessage::Welcome { .. } => Err("Unexpected repeated welcome".into()),
        ServerMessage::SnapshotPart { .. } => Err("Unexpected unassembled snapshot part".into()),
        // `next` already forwarded the current palette, including stale changes.
        ServerMessage::Palette { .. }
        | ServerMessage::MapPage { .. }
        | ServerMessage::ChartPage { .. } => Ok(()),
        // Input permissions arrive through ordered readiness updates.
        ServerMessage::Waiting { .. }
        | ServerMessage::AttachmentUpload { .. }
        | ServerMessage::AttachmentData { .. } => Ok(()),
    }
}

#[cfg(test)]
#[path = "network_tests.rs"]
mod tests;
