//! Ordinary-player session for automated policies.
use std::{net::SocketAddr, time::Duration};

use tor_client_common::{
    charts::{ChartDisplay, ChartInput},
    ClientState, ConfirmedReply, Connection, PendingRequest, RequestCompletion,
};
use tor_protocol::{
    chart_pages::ChartKind, AccessRole, Action, ActorId, Command, ProjectileChoicePage,
    ProjectileChoiceQuery, ReplyContext, Request, ServerMessage,
};

type TransportError = Box<dyn std::error::Error + Send + Sync>;
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, PartialEq, Eq)]
pub enum Turn {
    Confirmed(ConfirmedReply),
    StaleObservation,
    IntentionUnavailable,
    UnknownOutcome,
}

pub struct Session {
    connection: Connection,
    usable: bool,
    last_request_id: Option<String>,
}

impl Session {
    pub async fn connect(
        address: SocketAddr,
        token: String,
        actor: ActorId,
    ) -> Result<Self, TransportError> {
        let mut connection =
            tokio::time::timeout(TIMEOUT, Connection::connect(address, token, actor, "bot"))
                .await??;
        if connection.role() != AccessRole::Player {
            return Err("Bot sessions require ordinary player credentials".into());
        }
        connection.set_pace(Duration::ZERO);
        let mut session = Self {
            connection,
            usable: true,
            last_request_id: None,
        };
        match session.transact(Request::AcquireControl).await? {
            Turn::Confirmed(ConfirmedReply::Receipt(_))
                if session.connection.state.has_control() =>
            {
                Ok(session)
            }
            outcome => Err(format!("Could not acquire control: {outcome:?}").into()),
        }
    }

    pub fn observation(&self) -> Option<&ClientState> {
        (self.usable
            && self.connection.is_synchronized()
            && self.connection.state.observation_current())
        .then_some(&self.connection.state)
    }

    pub async fn observe(&mut self) -> Result<&ClientState, TransportError> {
        match self.transact(Request::Observe).await? {
            Turn::Confirmed(ConfirmedReply::Completed) => self
                .observation()
                .ok_or_else(|| "No current synchronized observation".into()),
            outcome => Err(format!("Could not refresh observation: {outcome:?}").into()),
        }
    }

    pub async fn act(
        &mut self,
        observation: &ReplyContext,
        action: Action,
    ) -> Result<Turn, TransportError> {
        let Some(current) = self.observation() else {
            return Err("Refresh or reconnect before acting".into());
        };
        if &current.reply_context() != observation {
            return Ok(Turn::StaleObservation);
        }
        let request = current.command_request(Command::Act { action });
        self.transact(request).await
    }

    pub async fn resume(&mut self, observation: &ReplyContext) -> Result<Turn, TransportError> {
        self.change_intention(observation, ClientState::resume_intention_request)
            .await
    }

    pub async fn cancel(&mut self, observation: &ReplyContext) -> Result<Turn, TransportError> {
        self.change_intention(observation, ClientState::cancel_intention_request)
            .await
    }

    async fn change_intention(
        &mut self,
        observation: &ReplyContext,
        request: fn(&ClientState) -> Option<Request>,
    ) -> Result<Turn, TransportError> {
        let Some(current) = self.observation() else {
            return Err("Refresh or reconnect before changing an intention".into());
        };
        if &current.reply_context() != observation {
            return Ok(Turn::StaleObservation);
        }
        let Some(request) = request(current) else {
            return Ok(Turn::IntentionUnavailable);
        };
        self.transact(request).await
    }

    /// Execute an ordinary advancement input through the shared preview and confirmation flow.
    pub async fn advancement(&mut self, input: &str) -> Result<Turn, TransportError> {
        if self.observation().is_none() {
            return Err("Refresh or reconnect before opening advancement".into());
        }
        let input = tor_client_common::advancement::parse(input)?;
        let request = self
            .connection
            .advancement_pages
            .request(input, &self.connection.state)?;
        match request {
            Some(request) => self.transact(request).await,
            None => Ok(Turn::Confirmed(ConfirmedReply::Completed)),
        }
    }

    pub fn advancement_page(&self) -> Option<&tor_protocol::AdvancementPage> {
        self.connection.advancement_pages.page()
    }

    /// Query server-owned projectile choices for the supplied player observation.
    /// A current query with no legal options returns an empty page; None means stale.
    pub async fn projectile_choices(
        &mut self,
        observation: &ReplyContext,
        query: ProjectileChoiceQuery,
    ) -> Result<Option<ProjectileChoicePage>, TransportError> {
        let Some(current) = self.observation() else {
            return Err("Refresh or reconnect before choosing projectiles".into());
        };
        if &current.reply_context() != observation {
            return Ok(None);
        }
        if !query.is_valid() {
            return Err("Invalid projectile choice query".into());
        }
        let request = current.command_request(Command::ProjectileChoices {
            expected_revision: observation.revision,
            query: query.clone(),
        });
        let mut page = None;
        let turn = self
            .transact_with(request, |id, message| {
                if let ServerMessage::ProjectileChoices {
                    context,
                    request_id,
                    page: value,
                } = message
                {
                    if request_id == id && context == observation && value.valid_for(&query) {
                        page = Some(value.as_ref().clone());
                    }
                }
            })
            .await?;
        match turn {
            Turn::Confirmed(ConfirmedReply::Rejected {
                code: tor_protocol::ErrorCode::InvalidAction,
                ..
            }) => {
                // A discovery query can legitimately find an unwielded launcher
                // or no compatible ammunition. Those are player-facing hints,
                // not bot transport errors; let it consider its other abilities.
                if self
                    .observation()
                    .is_none_or(|current| current.reply_context() != *observation)
                {
                    return Ok(None);
                }
                Ok(Some(ProjectileChoicePage {
                    query,
                    choices: Vec::new(),
                    next_offset: None,
                }))
            }
            Turn::Confirmed(ConfirmedReply::Rejected { code, message }) => {
                Err(format!("{code:?}: {message}").into())
            }
            Turn::UnknownOutcome => {
                Err("Projectile query was interrupted; refresh the session".into())
            }
            Turn::Confirmed(ConfirmedReply::Completed) => {
                let Some(current) = self.observation() else {
                    return Ok(None);
                };
                if &current.reply_context() != observation {
                    return Ok(None);
                }
                let page = page.ok_or("Projectile reply did not match the requested choices")?;
                for choice in &page.choices {
                    tor_client_common::projectiles::validate_choice(
                        &current.state().observation,
                        *choice,
                    )?;
                }
                Ok(Some(page))
            }
            _ => Err("Unexpected projectile query outcome".into()),
        }
    }

    /// Assemble a disclosed chart at this observation without spending a turn.
    pub async fn chart(
        &mut self,
        observation: &ReplyContext,
        kind: ChartKind,
    ) -> Result<Option<ChartDisplay>, TransportError> {
        let Some(current) = self.observation() else {
            return Err("Refresh or reconnect before reading a chart".into());
        };
        if &current.reply_context() != observation
            || !current
                .state()
                .observation
                .charts
                .iter()
                .any(|summary| summary.kind == kind && summary.chart.is_some())
        {
            return Ok(None);
        }
        self.connection
            .charts
            .request(ChartInput::Select(kind), &self.connection.state)?;
        let result = tokio::time::timeout(TIMEOUT, async {
            while self.connection.charts.loading() {
                if let Err(error) = self.connection.next().await {
                    self.usable = false;
                    return Err(error);
                }
                if self
                    .observation()
                    .is_none_or(|current| &current.reply_context() != observation)
                {
                    return Ok(None);
                }
            }
            self.connection
                .charts
                .display()
                .cloned()
                .map(Some)
                .ok_or_else(|| "Could not assemble the disclosed chart".into())
        })
        .await;
        self.connection.cancel_charts();
        result.map_err(|_| "Chart query timed out")?
    }

    pub async fn close(mut self) -> Result<(), TransportError> {
        tokio::time::timeout(TIMEOUT, self.connection.close()).await?
    }

    /// Correlation for the latest transaction, including one with an unknown outcome.
    pub fn last_request_id(&self) -> Option<&str> {
        self.last_request_id.as_deref()
    }

    async fn transact(&mut self, request: Request) -> Result<Turn, TransportError> {
        self.transact_with(request, |_, _| {}).await
    }

    async fn transact_with(
        &mut self,
        request: Request,
        observe: impl FnMut(&str, &ServerMessage),
    ) -> Result<Turn, TransportError> {
        if !self.usable {
            return Err("Session disconnected or timed out; reconnect before retrying".into());
        }
        self.last_request_id = None;
        match tokio::time::timeout(
            TIMEOUT,
            transact(
                &mut self.connection,
                request,
                observe,
                &mut self.last_request_id,
            ),
        )
        .await
        {
            Ok(Ok(turn)) => Ok(turn),
            // The send can have reached the server before either failure.
            Ok(Err(_)) | Err(_) => {
                self.usable = false;
                Ok(Turn::UnknownOutcome)
            }
        }
    }
}

async fn transact(
    connection: &mut Connection,
    request: Request,
    mut observe: impl FnMut(&str, &ServerMessage),
    last_request_id: &mut Option<String>,
) -> Result<Turn, TransportError> {
    let id = connection
        .request_tracked(request, |id| *last_request_id = Some(id.to_owned()))
        .await?;
    let mut pending = PendingRequest::new(id.clone());
    loop {
        let message = connection.next().await?;
        observe(&id, &message);
        match pending.observe(connection, &message) {
            Some(RequestCompletion::Reply(reply)) => return Ok(Turn::Confirmed(reply.clone())),
            Some(RequestCompletion::Unknown) => return Ok(Turn::UnknownOutcome),
            None => {}
        }
    }
}

#[cfg(feature = "python")]
mod python;
