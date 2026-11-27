//! One correlated `Request::Command` from an attached client.
//!
//! Processing has three phases:
//!
//! 1. **Admission** decodes the command, answers durable retries with their
//!    original receipt, and checks the input context, branch, control and
//!    published permissions. Wizard text resolves here into a report or an
//!    operation.
//! 2. **Private queries** reply from the latest issued picture without
//!    committing anything.
//! 3. **Commit and publication** journal the command, then publish its
//!    consequences to every affected client before the receipt.
use super::*;
use crate::engine::{CommandResult, PresentationBoundary};
use crate::wire_adapter::{DecodedCommand, DisplayCommand};

/// The attached client that issued one command.
pub(super) struct Caller {
    pub(super) id: u64,
    pub(super) actor: ActorId,
    pub(super) user: String,
    pub(super) frontend: String,
}

/// The outcome of command admission.
enum Admission {
    /// Answered already: a durable retry receipt or a wizard report.
    Replied(ProcessedRequest),
    /// A fresh command that passed every session check.
    Fresh(DecodedCommand),
}

impl Service {
    pub(super) fn process_command(
        &mut self,
        caller: &Caller,
        request_id: &str,
        context: InputContext,
        branch: BranchId,
        command: &Command,
    ) -> Result<ProcessedRequest, Failure> {
        let command = match self.admit_command(caller, request_id, &context, &branch, command)? {
            Admission::Replied(reply) => return Ok(reply),
            Admission::Fresh(command) => command,
        };
        let id = caller.id;
        let reply = match command {
            DecodedCommand::ProjectileChoices {
                expected_revision,
                query,
            } => {
                let context = self.current_reply_context(id)?;
                let budget = crate::engine::ProjectileChoicePageBudget {
                    context: &context,
                    request_id,
                    limit: self.response_limit(),
                };
                let boundary = self.query_boundary(id, expected_revision)?;
                let page = self.engine.projectile_choices_at(
                    &boundary,
                    caller.actor,
                    expected_revision,
                    &query,
                    &budget,
                )?;
                RequestReply::ProjectileChoices(Box::new(page))
            }
            DecodedCommand::AbilityWeaponPreview {
                expected_revision,
                ability,
                invocation,
                supplies,
            } => {
                let boundary = self.query_boundary(id, expected_revision)?;
                let preview = self.engine.ability_weapon_preview_at(
                    &boundary,
                    caller.actor,
                    &ability,
                    invocation,
                    &supplies,
                )?;
                let message = ServerMessage::AbilityWeaponPreview {
                    context: self.current_reply_context(id)?,
                    request_id: request_id.into(),
                    preview: Box::new(preview.clone()),
                };
                if !self.reply_fits(&message, "Invalid preview")? {
                    return Err(Failure::new(
                        ErrorCode::ResourceLimit,
                        "Preview exceeds response limit",
                    ));
                }
                RequestReply::AbilityWeaponPreview(Box::new(preview))
            }
            DecodedCommand::Advancement {
                expected_revision,
                query,
            } => {
                let reply = self.current_reply_context(id)?;
                let budget = crate::advancement_page::PageBudget {
                    context: &reply,
                    request_id,
                    limit: self.response_limit(),
                };
                let boundary = self.query_boundary(id, expected_revision)?;
                let page = self.engine.advancement_query_at(
                    &boundary,
                    caller.actor,
                    expected_revision,
                    &query,
                    &budget,
                )?;
                RequestReply::AdvancementPage(Box::new(page))
            }
            DecodedCommand::StrikeInspection {
                expected_revision,
                target,
                target_revision,
                source,
                cursor,
            } => {
                let query = StrikePageQuery {
                    expected_revision,
                    target,
                    target_revision,
                    source,
                    cursor,
                };
                self.strike_page(id, request_id, context, branch, query)?
            }
            DecodedCommand::CombatDiagnostics {
                expected_revision,
                operation,
            } => {
                let boundary = self.query_boundary(id, expected_revision)?;
                let report = self
                    .engine
                    .combat_diagnostics_query_at(&boundary, operation)?;
                RequestReply::CombatDiagnostics(Box::new(report))
            }
            DecodedCommand::CreatureCatalog {
                expected_revision,
                depth,
                after,
            } => {
                let boundary = self.query_boundary(id, expected_revision)?;
                let page = self.engine.creature_catalog_query_at(
                    &boundary,
                    caller.actor,
                    depth,
                    after.as_deref(),
                )?;
                let context = self.current_reply_context(id)?;
                let page = crate::wizard_creatures::fit_reply(
                    page,
                    &context,
                    request_id,
                    self.response_limit(),
                )?;
                RequestReply::CreatureCatalog(Box::new(page))
            }
            DecodedCommand::CreatureInspection {
                expected_revision,
                target,
            } => {
                let boundary = self.query_boundary(id, expected_revision)?;
                let report =
                    self.engine
                        .inspect_creature_query_at(&boundary, target, expected_revision)?;
                RequestReply::CreatureInspection(Box::new(report))
            }
            command => return self.commit_command(caller, request_id, &branch, command),
        };
        Ok(ProcessedRequest::Reply(reply))
    }

    /// Decode and check one command before any query or commit.
    fn admit_command(
        &mut self,
        caller: &Caller,
        request_id: &str,
        context: &InputContext,
        branch: &BranchId,
        command: &Command,
    ) -> Result<Admission, Failure> {
        let id = caller.id;
        let mut command = crate::wire_adapter::decode_command(command)?;
        // Wizard text that is not a query resolves against the current picture
        // first; its durable retry identity is the resolved operation.
        let relative = matches!(&command, DecodedCommand::WizardContext { text, .. }
            if !crate::developer::is_wizard_query(text));
        if !relative {
            if let Some(previous) = self.retry_receipt(caller, request_id, branch, &command)? {
                return Ok(Admission::Replied(previous));
            }
        }
        // Ownership and availability can change without an observation
        // revision. Resolve receipts first, then reject fresh input built
        // for another attachment, reset or authority generation.
        let current = self.current_readiness(id).ok_or_else(|| {
            Failure::new(
                ErrorCode::StaleContext,
                "Input context exhausted; reconnect",
            )
        })?;
        let expected = self
            .input_context(id)
            .ok_or_else(|| Failure::new(ErrorCode::NotAttached, "Attach an actor first"))?;
        if *context != expected || current.permissions.revision != expected.readiness_revision {
            return Err(Failure::new(
                ErrorCode::StaleContext,
                "Input context changed; use the current disclosed state",
            ));
        }
        // Read-only private queries return before Engine::command, so
        // they must validate the same branch after durable retry lookup.
        if branch != self.engine.branch() {
            return Err(Failure::new(
                ErrorCode::WrongBranch,
                "The game branch changed",
            ));
        }
        if let DecodedCommand::WizardContext {
            expected_revision,
            text,
        } = &command
        {
            let boundary = self.query_boundary(id, *expected_revision)?;
            let origin = self.engine.wizard_origin_at(&boundary, caller.actor)?;
            let invalid = |message: String| Failure::new(ErrorCode::InvalidRequest, &message);
            if let Some(query) =
                crate::developer::parse_wizard_query(text, origin).map_err(invalid)?
            {
                let report = self
                    .engine
                    .wizard_report_at(&boundary, caller.actor, query)?;
                let message = ServerMessage::WizardReport {
                    context: self.current_reply_context(id)?,
                    request_id: request_id.into(),
                    report: Box::new(report.clone()),
                };
                if !self.reply_fits(&message, "Invalid wizard report")? {
                    return Err(Failure::new(
                        ErrorCode::InvalidAction,
                        "Wizard report exceeds negotiated response budget",
                    ));
                }
                return Ok(Admission::Replied(ProcessedRequest::Reply(
                    RequestReply::WizardReport(Box::new(report)),
                )));
            }
            let operation = self
                .engine
                .resolve_wizard_at(&boundary, caller.actor, text)?;
            command = DecodedCommand::Display {
                expected_revision: *expected_revision,
                operation: DisplayCommand::Wizard { operation },
            };
            if let Some(previous) = self.retry_receipt(caller, request_id, branch, &command)? {
                return Ok(Admission::Replied(previous));
            }
        }
        if command.requires_control() && self.controllers.get(&caller.actor) != Some(&id) {
            return Err(Failure::new(
                ErrorCode::NotController,
                "Acquire control before acting",
            ));
        }
        if let DecodedCommand::Gameplay {
            action:
                Action::ChooseGrowthSource { basis, .. }
                | Action::ChooseIntellectExpertise { basis, .. }
                | Action::Purchase { basis, .. }
                | Action::Respec { basis, .. },
        } = &command
        {
            let reply = self.current_reply_context(id)?;
            self.engine.check_advancement_basis(&reply, basis)?;
        }
        check_published_permissions(&command, &current.permissions)?;
        Ok(Admission::Fresh(command))
    }

    /// The original receipt when this exact request was already journaled.
    fn retry_receipt(
        &self,
        caller: &Caller,
        request_id: &str,
        branch: &BranchId,
        command: &DecodedCommand,
    ) -> Result<Option<ProcessedRequest>, Failure> {
        let previous = self.engine.retry_decoded(
            &caller.user,
            &caller.frontend,
            caller.actor,
            request_id,
            branch,
            command,
        )?;
        Ok(previous.map(|previous| self.receipt_reply(&previous)))
    }

    /// One page of a strike source inspection, bound to an ephemeral handle.
    fn strike_page(
        &mut self,
        id: u64,
        request_id: &str,
        context: InputContext,
        branch: BranchId,
        query: StrikePageQuery,
    ) -> Result<RequestReply, Failure> {
        let StrikePageQuery {
            expected_revision,
            target,
            target_revision,
            source,
            cursor,
        } = query;
        let catalog = self.engine.strike_catalog_identity();
        let stale = || {
            Failure::new(
                ErrorCode::StaleContext,
                "Strike inspection changed; reopen the source",
            )
        };
        if let Some(cursor) = &cursor {
            let handle = self.clients[&id]
                .strike_inspection
                .as_ref()
                .ok_or_else(stale)?;
            if handle.id != cursor.inspection
                || handle.context != context
                || handle.branch != branch
                || handle.observer_revision != expected_revision
                || handle.target != target
                || handle.target_revision != target_revision
                || handle.source != source
                || handle.catalog != catalog
            {
                return Err(stale());
            }
        }
        let inspection = cursor
            .as_ref()
            .map(|cursor| cursor.inspection.clone())
            .unwrap_or_else(|| StrikeInspectionId(uuid::Uuid::new_v4().to_string()));
        let reply_context = self.reply_context(id).ok_or_else(stale)?;
        let budget = crate::creature_view::PageBudget {
            context: &reply_context,
            request_id,
            inspection: &inspection,
            actor: target,
            target_revision,
            source: &source,
            limit: self.response_limit(),
        };
        let boundary = self.query_boundary(id, expected_revision)?;
        let projected = self.engine.strike_page_query_at(
            &boundary,
            expected_revision,
            (target, target_revision),
            (&source, cursor.as_ref().map(|cursor| &cursor.position)),
            &budget,
        )?;
        if cursor.is_some()
            && self.clients[&id]
                .strike_inspection
                .as_ref()
                .is_some_and(|handle| handle.resolved != projected.resolved)
        {
            return Err(stale());
        }
        let page = CreatureStrikePage {
            inspection: inspection.clone(),
            actor: target,
            target_revision,
            source: source.clone(),
            resolved: projected.resolved.clone(),
            strike_count: projected.strike_count,
            start: projected.start,
            entries: projected.entries,
            next: projected.next.map(|position| StrikePageCursor {
                inspection: inspection.clone(),
                position,
            }),
        };
        page.validate().map_err(|_| {
            Failure::new(ErrorCode::InvalidArchive, "Invalid strike inspection page")
        })?;
        // Defense in depth on the actual complete correlated reply, without
        // cloning the projected page or allocating its encoded JSON.
        budget.check(&page)?;
        self.clients
            .get_mut(&id)
            .expect("connected client")
            .strike_inspection = Some(StrikeInspectionHandle {
            id: inspection,
            context,
            branch,
            observer_revision: expected_revision,
            target,
            target_revision,
            source,
            resolved: projected.resolved,
            catalog,
        });
        Ok(RequestReply::StrikePage(Box::new(page)))
    }

    /// Journal an admitted command and publish its consequences.
    fn commit_command(
        &mut self,
        caller: &Caller,
        request_id: &str,
        branch: &BranchId,
        command: DecodedCommand,
    ) -> Result<ProcessedRequest, Failure> {
        let display_boundary = match command.display_basis() {
            Some(revision) => Some((self.query_boundary(caller.id, revision)?, revision)),
            None => None,
        };
        let command = self.engine.resolve_command_with_display(
            caller.actor,
            branch,
            command,
            display_boundary
                .as_ref()
                .map(|(boundary, revision)| (boundary, *revision)),
        )?;
        let before_states = self.semantic_states()?;
        let normalized_annotation = match &command {
            crate::journal::Command::Annotate {
                anchor: Anchor::State { snapshot },
                ..
            } => {
                let issued = self.presentation_by_snapshot(caller.id, snapshot)?;
                Some(crate::journal::RecordedAnchor::State {
                    snapshot: snapshot.clone(),
                    boundary: issued.boundary.entry().cloned(),
                })
            }
            _ => None,
        };
        let result = if let Some(anchor) = normalized_annotation {
            let receipt = crate::engine::Receipt {
                user: caller.user.clone(),
                frontend: caller.frontend.clone(),
                request_id: request_id.into(),
                actor: caller.actor,
                branch: branch.clone(),
                command,
            };
            self.engine.command_with_annotation(&receipt, anchor)?
        } else {
            self.engine.command(
                &caller.user,
                &caller.frontend,
                caller.actor,
                request_id,
                branch,
                command,
            )?
        };
        self.publish_command_result(caller, &before_states, &result)?;
        Ok(self.receipt_reply(&result))
    }

    /// Publish a committed command's consequences; its receipt follows.
    fn publish_command_result(
        &mut self,
        caller: &Caller,
        before_states: &SemanticStates,
        result: &CommandResult,
    ) -> Result<(), Failure> {
        use crate::journal::{IntentionChange, JournalContent, WizardOperation};
        if let JournalContent::Wizard { operation, .. } = &result.entry.content {
            match operation {
                WizardOperation::ArenaControl { .. } => {
                    self.autonomous_enabled = self.engine.arena_execution_enabled();
                    // Scheduler controls change ordinary readiness, not world
                    // topology. Preserve streams, preparations and private history.
                    return self.action_update(before_states, None, None);
                }
                WizardOperation::RestoreResource { .. }
                | WizardOperation::ApplyCondition { .. }
                | WizardOperation::ClearCondition { .. }
                | WizardOperation::GiveItem { .. } => {
                    // These commands change actor state, not world topology.
                    // In particular, do not pause other actors or rebroadcast
                    // the private wizard result as an actor-wide outcome.
                    self.action_update(before_states, None, Some(result))?;
                    self.intention_update(result);
                    if self.clients.contains_key(&caller.id)
                        && self.snapshot(caller.id, "").is_err()
                    {
                        self.disconnect(caller.id);
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
        let Some(visible_entry) = self.engine.disclose_entry(&result.entry) else {
            if matches!(
                result.entry.content,
                JournalContent::IntentionAdmitted { .. }
                    | JournalContent::IntentionChanged {
                        change: IntentionChange::Resumed,
                        ..
                    }
            ) {
                self.autonomous_enabled = true;
            }
            return self.action_result_update(before_states, result);
        };
        match &visible_entry.content {
            HistoryContent::Travel { destination } => {
                self.autonomous_enabled = true;
                let destination = destination.clone();
                self.start_travel(caller, result, &destination, visible_entry)?;
            }
            HistoryContent::Wizard { rewind, .. } => self.reset_after_wizard(*rewind),
            HistoryContent::Narration {} => {
                unreachable!("history-only records do not become command receipts");
            }
            HistoryContent::Annotation { .. } => self.annotation_update(&visible_entry),
            HistoryContent::Action { .. } => {
                self.autonomous_enabled = true;
                self.action_result_update(before_states, result)?;
            }
            HistoryContent::PlaceRenamed { .. } => {
                self.action_result_update(before_states, result)?;
            }
        }
        Ok(())
    }

    /// Record an accepted journey and queue its steps unless already arrived.
    fn start_travel(
        &mut self,
        caller: &Caller,
        result: &CommandResult,
        destination: &str,
        visible_entry: HistoryEntry,
    ) -> Result<(), Failure> {
        let actor = caller.actor;
        let steps = self.engine.travel_route(actor, destination)?;
        let observation = self.engine.observation(actor)?;
        let phase = if steps.is_empty() {
            TravelPhase::Arrived
        } else {
            TravelPhase::Active
        };
        self.travel_status.insert(
            actor,
            TravelStatus {
                id: result.entry.id.clone(),
                destination: destination.to_owned(),
                completed_steps: 0,
                phase,
            },
        );
        if phase == TravelPhase::Active {
            self.travels.insert(
                actor,
                TravelJob {
                    hp: observation.combat.as_ref().map(|c| c.hp),
                    owner: caller.id,
                    steps: steps.into(),
                    hazards: potential_hazards(&observation),
                    pending: None,
                },
            );
        }
        self.travel_update(actor, Some(visible_entry));
        Ok(())
    }

    /// A committed wizard setup or rewind pauses autonomous work and starts an
    /// explicit stream boundary for every attached client.
    fn reset_after_wizard(&mut self, rewind: bool) {
        self.autonomous_enabled = false;
        if rewind {
            self.pending_pauses.clear();
            self.pending_travel_cancellations.clear();
            for actor in self.engine.queued_travel_actors() {
                self.cancel_travel_work(actor);
            }
            for actor in self.engine.queued_human_actors() {
                if let Err(error) = self.engine.suspend_queued_intention(actor) {
                    self.pending_pauses.insert(actor);
                    self.save_warning = Some(error.to_string());
                }
            }
        }
        for actor in self.engine.actors() {
            if let Err(error) = self.engine.pause_preparation(actor) {
                self.save_warning = Some(error.to_string());
            }
        }
        if rewind {
            // The committed rewind already changed the branch. Publish
            // only the explicit snapshot boundary into the old stream.
            self.travels.clear();
            self.travel_status.clear();
        } else {
            for travelling in self.travels.keys().copied().collect::<Vec<_>>() {
                self.stop_travel(travelling, TravelPhase::WorldChanged);
            }
        }
        // Clients attached to actors removed by rewind must reattach.
        let recipients: Vec<_> = self
            .clients
            .iter()
            .filter_map(|(&id, c)| c.actor.map(|a| (id, a)))
            .collect();
        let previous_controllers = self.controllers.clone();
        let actors = self.engine.actors();
        self.controllers.retain(|actor, _| actors.contains(actor));
        self.resetting_streams = true;
        for (recipient, observer) in recipients {
            if !self.clients.contains_key(&recipient) {
                continue;
            }
            let disconnect = self.engine.causal_boundary(observer).is_err()
                || self.snapshot(recipient, "").is_err();
            if disconnect {
                self.disconnect(recipient);
            }
        }
        self.resetting_streams = false;
        for (actor, owner) in previous_controllers {
            if self.controllers.get(&actor) != Some(&owner) && actors.contains(&actor) {
                self.control_update(actor);
            }
        }
    }

    fn receipt_reply(&self, result: &CommandResult) -> ProcessedRequest {
        ProcessedRequest::Reply(RequestReply::Receipt(self.engine.request_receipt(result)))
    }

    /// The negotiated maximum encoded size of one correlated reply.
    fn response_limit(&self) -> usize {
        self.outbound.capabilities().max_response_bytes as usize
    }

    /// Whether a complete correlated reply fits the negotiated response budget.
    fn reply_fits(&self, message: &ServerMessage, invalid: &str) -> Result<bool, Failure> {
        tor_protocol::encoded_length(message, self.response_limit())
            .map(|length| length.is_some())
            .map_err(|_| Failure::new(ErrorCode::InvalidAction, invalid))
    }

    /// The reply context for a correlated answer to the current picture.
    fn current_reply_context(&self, id: u64) -> Result<ReplyContext, Failure> {
        self.reply_context(id)
            .ok_or_else(|| Failure::new(ErrorCode::StaleContext, "Input context changed"))
    }

    /// The presentation boundary of the picture a read-only query names.
    fn query_boundary(
        &self,
        id: u64,
        expected_revision: u64,
    ) -> Result<PresentationBoundary, Failure> {
        Ok(self
            .query_presentation(id, expected_revision)?
            .boundary
            .clone())
    }
}

/// A decoded strike inspection request.
struct StrikePageQuery {
    expected_revision: u64,
    target: ActorId,
    target_revision: u64,
    source: StrikeSourceSelector,
    cursor: Option<StrikePageCursor>,
}

/// Fresh gameplay must honor the same derived permissions that were published.
/// Simulation still validates the action itself; session policy cannot be
/// bypassed by a correctly stamped request.
fn check_published_permissions(
    command: &DecodedCommand,
    permissions: &Readiness,
) -> Result<(), Failure> {
    match command {
        DecodedCommand::Gameplay { .. }
        | DecodedCommand::Display {
            operation: DisplayCommand::Travel { .. },
            ..
        } if !permissions.admission => Err(Failure::new(
            ErrorCode::ActorBusy,
            "New gameplay is unavailable in the current input state",
        )),
        DecodedCommand::ResumeIntention { admission }
            if !permissions.resume.iter().any(|id| id.0 == admission.0) =>
        {
            Err(Failure::new(
                ErrorCode::InvalidAction,
                "Intention cannot be resumed in the current input state",
            ))
        }
        DecodedCommand::CancelIntention { admission, .. }
            if !permissions.cancel.iter().any(|id| id.0 == admission.0) =>
        {
            Err(Failure::new(
                ErrorCode::InvalidAction,
                "Intention cannot be cancelled in the current input state",
            ))
        }
        _ => Ok(()),
    }
}
