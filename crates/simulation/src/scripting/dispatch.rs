//! Fact noting, boundary dispatch and effect application.

use super::state::{PendingFact, Timer, TimerClock};
use super::{
    ActorClass, Audience, Binding, FactSubject, FailureReason, FilterKey, HandlerId, HookFact,
    HookKind, MessageSegment, Owner, RunEnd, ScriptEffect, ScriptFailure, ScriptMessage,
    ScriptReport, ScriptSetup, ScriptState, ScriptView, DispatchRecord, MAX_CASCADE_DEPTH,
    MAX_EFFECTS_PER_CALL, MAX_MESSAGE_SEGMENTS, MAX_PENDING_FACTS, MAX_TEXT_BYTES,
};
use crate::{ActorId, Game, GameError, ItemLocation, OutcomeKind};
use std::sync::Arc;
use tor_world::{Location, RegionId};

/// Deterministic work allowance shared by one root invocation and every
/// cascade call it causes. Hosts charge it; exhausting it rejects the root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkBudget {
    pub remaining: u64,
}

impl WorkBudget {
    /// Work units available to one root invocation and its cascade.
    pub const ROOT: u64 = 1_000_000;
    pub fn root() -> Self {
        Self {
            remaining: Self::ROOT,
        }
    }
    /// Charge `units`; `false` means the budget is exhausted.
    pub fn charge(&mut self, units: u64) -> bool {
        match self.remaining.checked_sub(units) {
            Some(remaining) => {
                self.remaining = remaining;
                true
            }
            None => {
                self.remaining = 0;
                false
            }
        }
    }
}

/// One batched handler call.
#[derive(Debug)]
pub struct HookCall<'a> {
    pub handler: HandlerId,
    pub facts: &'a [HookFact],
    pub depth: u8,
}

/// What a host returns for one call. `rng` is the script stream state after
/// the call's draws, advanced only with [`super::view::script_random`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookOutput {
    pub effects: Vec<ScriptEffect>,
    pub rng: u64,
}

/// A script runtime. It receives a read-only view and proposes effects; it
/// never mutates simulation state and holds none of it.
pub trait HookHost: Send + Sync {
    fn dispatch(
        &self,
        view: ScriptView<'_>,
        call: HookCall<'_>,
        budget: &mut WorkBudget,
    ) -> Result<HookOutput, FailureReason>;
}

/// The installed host. It is not game state: clones share it, equality and
/// checkpoints ignore it, and restore requires installing it again.
#[derive(Clone, Default)]
pub struct HookPort(Option<Arc<dyn HookHost>>);

impl HookPort {
    pub fn new(host: Arc<dyn HookHost>) -> Self {
        Self(Some(host))
    }
    pub fn is_installed(&self) -> bool {
        self.0.is_some()
    }
}
impl std::fmt::Debug for HookPort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() { "HookPort(installed)" } else { "HookPort(none)" })
    }
}
impl PartialEq for HookPort {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for HookPort {}

fn invariant(detail: &str) -> FailureReason {
    FailureReason::Invariant {
        effect: 0,
        detail: detail.into(),
    }
}

impl Game {
    /// Start scripted play. Must be called once, before the first action, on
    /// a game that has no script state.
    pub fn install_scripts(&mut self, setup: ScriptSetup) -> Result<(), GameError> {
        if self.script.is_some() {
            return Err(GameError::Script);
        }
        let state = ScriptState::new(setup, self.seed).ok_or(GameError::Script)?;
        self.hook_mask = state.mask();
        self.script = Some(state);
        Ok(())
    }

    /// Install the runtime that answers hook calls. Required after building or
    /// restoring a scripted game; hosts are never saved.
    pub fn set_hook_host(&mut self, port: HookPort) {
        self.hook_port = port;
    }

    pub fn script_state(&self) -> Option<&ScriptState> {
        self.script.as_ref()
    }

    /// Messages, handler calls and rejected roots since the last drain.
    pub fn take_script_report(&mut self) -> ScriptReport {
        std::mem::take(&mut self.script_report)
    }

    /// Note `run_started` and dispatch it. Hosts call this once for a new game
    /// after installing scripts and the host.
    pub fn start_scripts(&mut self) -> Result<(), GameError> {
        self.note_hook(HookKind::RunStarted, None, |_| {
            Some((FactSubject::None, Vec::new()))
        });
        self.settle_script_hooks()
    }

    /// Match one fact against live bindings. Unsubscribed kinds cost one AND;
    /// `make` runs only for subscribed kinds and returns the fact's subject
    /// and its candidate keys beyond `Any` and the actor.
    #[inline]
    pub(crate) fn note_hook(
        &mut self,
        kind: HookKind,
        actor: Option<ActorId>,
        make: impl FnOnce(&Game) -> Option<(FactSubject, Vec<FilterKey>)>,
    ) {
        crate::diagnostics::hook_site_checked();
        if self.hook_mask & kind.bit() == 0 {
            return;
        }
        self.match_fact(kind, actor, make);
    }

    #[cold]
    fn match_fact(
        &mut self,
        kind: HookKind,
        actor: Option<ActorId>,
        make: impl FnOnce(&Game) -> Option<(FactSubject, Vec<FilterKey>)>,
    ) {
        let Some((subject, mut keys)) = make(self) else {
            return;
        };
        keys.push(FilterKey::Any);
        if let Some(actor) = actor {
            keys.push(FilterKey::Actor(actor));
        }
        let character = actor.map(|actor| self.combat.characters.contains(&actor));
        let Some(state) = &self.script else {
            return;
        };
        let mut matched = Vec::new();
        for key in &keys {
            for id in state.index.matching(kind, key) {
                let binding = &state.bindings[&id];
                let hears = match (binding.actors, character) {
                    (_, None) | (ActorClass::Any, _) => true,
                    (ActorClass::Character, Some(character)) => character,
                    (ActorClass::Creature, Some(character)) => !character,
                };
                if hears && self.owner_exists(binding.owner) {
                    matched.push((id, binding.handler));
                }
            }
        }
        if matched.is_empty() {
            return;
        }
        crate::diagnostics::hook_fact_noted();
        let fact = HookFact {
            kind,
            tick: self.tick,
            actor,
            subject,
        };
        let state = self.script.as_mut().expect("checked above");
        for (binding, handler) in matched {
            state.pending.push(PendingFact {
                handler,
                binding: Some(binding),
                fact: fact.clone(),
            });
        }
    }

    fn owner_exists(&self, owner: Option<Owner>) -> bool {
        match owner {
            None | Some(Owner::Region(_)) => true,
            Some(Owner::Actor(actor)) => self.actors.contains_key(&actor),
            Some(Owner::Item(item)) => self.items.get(&item).is_some(),
        }
    }

    fn owner_region(&self, owner: Owner) -> Option<RegionId> {
        match owner {
            Owner::Region(region) => Some(region),
            Owner::Actor(actor) => self.actors.get(&actor).map(|a| a.location.region),
            Owner::Item(item) => self.items.get(&item).and_then(|item| match item.location {
                ItemLocation::Ground(at) => Some(at.region),
                ItemLocation::Carried(actor) => {
                    self.actors.get(&actor).map(|a| a.location.region)
                }
            }),
        }
    }

    /// Facts an ordinary action outcome offers.
    pub(crate) fn note_action_hooks(&mut self, actor: ActorId, kind: &OutcomeKind) {
        if self.hook_mask == 0 {
            crate::diagnostics::hook_site_checked();
            return;
        }
        match *kind {
            OutcomeKind::Moved { from, to } => {
                self.note_hook(HookKind::Moved, Some(actor), |_| {
                    Some((
                        FactSubject::Moved { from, to },
                        vec![FilterKey::Cell(to), FilterKey::Region(to.region)],
                    ))
                });
            }
            OutcomeKind::DoorChanged { door, open } => {
                self.note_hook(HookKind::DoorChanged, Some(actor), |game| {
                    let at = game.world.door_location(door)?;
                    Some((
                        FactSubject::Door { at, open },
                        vec![FilterKey::Cell(at), FilterKey::Region(at.region)],
                    ))
                });
            }
            OutcomeKind::Taken { result, quantity, .. } => {
                self.note_item_hook(HookKind::ItemTaken, actor, result, quantity);
            }
            OutcomeKind::Dropped { result, quantity, .. } => {
                self.note_item_hook(HookKind::ItemDropped, actor, result, quantity);
            }
            OutcomeKind::Waited => {
                self.note_hook(HookKind::Waited, Some(actor), |game| {
                    let at = game.actors.get(&actor)?.location;
                    Some((
                        FactSubject::Waited { at },
                        vec![FilterKey::Cell(at), FilterKey::Region(at.region)],
                    ))
                });
            }
            _ => {}
        }
    }

    fn note_item_hook(
        &mut self,
        kind: HookKind,
        actor: ActorId,
        item: crate::ItemId,
        quantity: u64,
    ) {
        self.note_hook(kind, Some(actor), |game| {
            let at = game.actors.get(&actor)?.location;
            let identity = game.items.get(&item)?.spec.identity.clone();
            Some((
                FactSubject::Item {
                    item,
                    identity: identity.clone(),
                    quantity,
                    at,
                },
                vec![
                    FilterKey::Item(item),
                    FilterKey::ItemIdentity(identity),
                    FilterKey::Cell(at),
                    FilterKey::Region(at.region),
                ],
            ))
        });
    }

    /// Facts a combat event offers. Called from the single event funnel.
    pub(crate) fn note_event_hooks(&mut self, event: &crate::combat::CombatEvent) {
        if self.hook_mask == 0 {
            crate::diagnostics::hook_site_checked();
            return;
        }
        match event {
            crate::combat::CombatEvent::Resolved { outcome, .. } => {
                let (actor, target) = (outcome.actor, outcome.target);
                self.note_hook(HookKind::AttackResolved, Some(actor), |_| {
                    Some((FactSubject::Attack { target }, vec![]))
                });
            }
            crate::combat::CombatEvent::Died { actor, .. } => {
                let actor = *actor;
                self.note_hook(HookKind::Died, Some(actor), |game| {
                    let at = game.actors.get(&actor).map(|a| a.location);
                    let keys = at
                        .map(|at| vec![FilterKey::Cell(at), FilterKey::Region(at.region)])
                        .unwrap_or_default();
                    Some((FactSubject::Death { at }, keys))
                });
            }
            _ => {}
        }
    }

    /// Call where an actor's visited set grew.
    pub(crate) fn note_first_visit(&mut self, actor: ActorId, region: RegionId) {
        self.note_hook(HookKind::RegionFirstVisit, Some(actor), |_| {
            Some((
                FactSubject::Region { region },
                vec![FilterKey::Region(region)],
            ))
        });
    }

    /// The earliest running script timer, for the scheduler.
    pub(crate) fn next_script_timer_tick(&self) -> Option<u64> {
        self.script.as_ref().and_then(ScriptState::next_due)
    }

    /// Freeze or thaw timers whose owner lies in `region`.
    pub(crate) fn shift_script_timers(&mut self, region: RegionId, freeze: bool) {
        let Some(state) = &self.script else {
            return;
        };
        if state.timers.is_empty() {
            return;
        }
        let owners: std::collections::BTreeSet<Owner> = state
            .timers
            .values()
            .filter_map(|timer| timer.owner)
            .filter(|&owner| self.owner_region(owner) == Some(region))
            .collect();
        let tick = self.tick;
        if let Some(state) = &mut self.script {
            state.shift_timers(tick, freeze, |owner| owners.contains(owner));
        }
    }

    /// Dispatch every pending fact and due timer at a tick boundary. Each root
    /// invocation is its own transaction: a rejected root changes nothing and
    /// leaves sibling roots applied.
    pub(crate) fn settle_script_hooks(&mut self) -> Result<(), GameError> {
        let Some(state) = &self.script else {
            return Ok(());
        };
        if state.pending.is_empty() && state.next_due().is_none_or(|due| due > self.tick) {
            return Ok(());
        }
        self.fire_due_timers();
        let pending = std::mem::take(&mut self.script.as_mut().expect("checked").pending);
        if pending.len() > MAX_PENDING_FACTS {
            let handler = pending[0].handler;
            self.script_report.failures.push(ScriptFailure {
                tick: self.tick,
                handler,
                depth: 0,
                reason: FailureReason::Overflow,
            });
            return Ok(());
        }
        for (handler, batch) in group(pending) {
            let mut candidate = self.clone();
            let mut budget = WorkBudget::root();
            match candidate.invoke(handler, batch, 0, &mut budget) {
                Ok(()) => *self = candidate,
                Err((depth, reason)) => self.script_report.failures.push(ScriptFailure {
                    tick: self.tick,
                    handler,
                    depth,
                    reason,
                }),
            }
        }
        Ok(())
    }

    fn fire_due_timers(&mut self) {
        let tick = self.tick;
        let fired = self.script.as_mut().expect("scripted").take_due(tick);
        for (timer, Timer { handler, owner, arg, .. }) in fired {
            if !self.owner_exists(owner) {
                continue;
            }
            crate::diagnostics::hook_fact_noted();
            self.script.as_mut().expect("scripted").pending.push(PendingFact {
                handler,
                binding: None,
                fact: HookFact {
                    kind: HookKind::Timer,
                    tick,
                    actor: None,
                    subject: FactSubject::Timer { timer, arg },
                },
            });
        }
    }

    /// One handler call plus the cascade its effects cause.
    fn invoke(
        &mut self,
        handler: HandlerId,
        batch: Vec<PendingFact>,
        depth: u8,
        budget: &mut WorkBudget,
    ) -> Result<(), (u8, FailureReason)> {
        if depth > MAX_CASCADE_DEPTH {
            return Err((depth, FailureReason::Cascade));
        }
        let fail = |reason| (depth, reason);
        let state = self.script.as_mut().expect("scripted");
        let mut retired = false;
        for pending in &batch {
            if let Some(id) = pending.binding {
                if state.binding(id).is_some_and(|binding| binding.once) {
                    state.unbind(id);
                    retired = true;
                }
            }
        }
        if retired {
            self.hook_mask = state.mask();
        }
        let facts: Vec<HookFact> = batch.into_iter().map(|pending| pending.fact).collect();
        let host = self.hook_port.0.clone().ok_or(fail(FailureReason::NoHost))?;
        crate::diagnostics::hook_handler_called();
        let before = budget.remaining;
        let output = host
            .dispatch(
                ScriptView {
                    game: self,
                    state: self.script.as_ref().expect("scripted"),
                },
                HookCall {
                    handler,
                    facts: &facts,
                    depth,
                },
                budget,
            )
            .map_err(fail)?;
        if output.effects.len() > MAX_EFFECTS_PER_CALL {
            return Err(fail(invariant("too many effects")));
        }
        self.script.as_mut().expect("scripted").rng = output.rng;
        for (index, effect) in output.effects.iter().enumerate() {
            self.apply_script_effect(effect).map_err(|detail| {
                fail(FailureReason::Invariant {
                    effect: index as u32,
                    detail: detail.into(),
                })
            })?;
        }
        self.script_report.dispatches.push(DispatchRecord {
            tick: self.tick,
            handler,
            depth,
            facts: facts.len() as u32,
            work: before - budget.remaining,
            effects: output.effects,
        });
        let cascade = std::mem::take(&mut self.script.as_mut().expect("scripted").pending);
        if cascade.len() > MAX_PENDING_FACTS {
            return Err(fail(FailureReason::Overflow));
        }
        for (handler, batch) in group(cascade) {
            self.invoke(handler, batch, depth + 1, budget)?;
        }
        Ok(())
    }

    fn apply_script_effect(&mut self, effect: &ScriptEffect) -> Result<(), &'static str> {
        let tick = self.tick;
        match effect {
            ScriptEffect::Message {
                audience,
                style,
                segments,
            } => {
                if segments.is_empty()
                    || segments.len() > MAX_MESSAGE_SEGMENTS
                    || segments.iter().any(|segment| match segment {
                        MessageSegment::Text(text) => text.len() > MAX_TEXT_BYTES,
                        MessageSegment::Control(id) => !super::bounded_name(id),
                    })
                {
                    return Err("invalid message");
                }
                if let Audience::Actor(actor) = audience {
                    if !self.actors.contains_key(actor) {
                        return Err("unknown message audience");
                    }
                }
                self.script_report.messages.push(ScriptMessage {
                    tick,
                    audience: *audience,
                    style: *style,
                    segments: segments.clone(),
                });
            }
            ScriptEffect::SetVar { scope, name, value } => {
                self.scope_exists(*scope)?;
                self.script_mut()
                    .set_var(*scope, name.clone(), value.clone())
                    .ok_or("undeclared variable or wrong type")?;
            }
            ScriptEffect::ClearVar { scope, name } => {
                self.scope_exists(*scope)?;
                self.script_mut()
                    .clear_var(*scope, name.clone())
                    .ok_or("undeclared variable")?;
            }
            ScriptEffect::StartTimer {
                id,
                handler,
                owner,
                after,
                arg,
            } => {
                if !self.owner_exists(*owner) {
                    return Err("unknown timer owner");
                }
                let frozen = owner
                    .and_then(|owner| self.owner_region(owner))
                    .is_some_and(|region| self.region_state(region) == Some(crate::streaming::RegionState::Frozen));
                let clock = if frozen {
                    TimerClock::Frozen { remaining: *after }
                } else {
                    TimerClock::Due(tick.checked_add(*after).ok_or("timer overflows time")?)
                };
                self.script_mut()
                    .start_timer(
                        *id,
                        Timer {
                            handler: *handler,
                            owner: *owner,
                            arg: *arg,
                            clock,
                        },
                    )
                    .ok_or("invalid timer")?;
            }
            ScriptEffect::CancelTimer { id } => {
                self.script_mut().cancel_timer(*id);
            }
            ScriptEffect::Bind { id, binding } => {
                if !self.owner_exists(binding.owner) {
                    return Err("unknown binding owner");
                }
                self.bind_script(*id, binding.clone())?;
            }
            ScriptEffect::Unbind { id } => {
                if self.script_mut().unbind(*id).is_some() {
                    self.hook_mask = self.script_mut().mask();
                }
            }
            ScriptEffect::SetDoor { at, open } => self.script_set_door(*at, *open)?,
            ScriptEffect::PlaceItem {
                id,
                identity,
                name,
                at,
            } => {
                if *id != crate::ItemId(self.next_item_id) || name.len() > MAX_TEXT_BYTES {
                    return Err("unexpected item identity");
                }
                let placed = self
                    .place_item(*at, identity.clone(), name.clone())
                    .map_err(|_| "item cannot be placed there")?;
                if placed != *id {
                    return Err("unexpected item identity");
                }
            }
            ScriptEffect::RemoveItem { item } => {
                let removable = self.items.get(item).is_some_and(|stock| {
                    matches!(stock.location, ItemLocation::Ground(_)) && stock.holds.is_empty()
                }) && self
                    .unique_retrieval_policy()
                    .is_none_or(|policy| {
                        self.items
                            .get(item)
                            .is_some_and(|stock| stock.spec.identity != policy.item.identity)
                    });
                if !removable {
                    return Err("item cannot be removed");
                }
                self.items.remove(item);
            }
            ScriptEffect::Teleport { actor, to } => {
                let from = self
                    .actors
                    .get(actor)
                    .map(|a| a.location)
                    .ok_or("unknown actor")?;
                let first = !self.actors[actor].visited.contains(&to.region);
                self.teleport(*actor, *to).map_err(|_| "destination unavailable")?;
                let (actor, to) = (*actor, *to);
                self.note_hook(HookKind::Moved, Some(actor), |_| {
                    Some((
                        FactSubject::Moved { from, to },
                        vec![FilterKey::Cell(to), FilterKey::Region(to.region)],
                    ))
                });
                if first {
                    self.note_first_visit(actor, to.region);
                }
            }
            ScriptEffect::EndRun { end, actor } => self.script_end_run(*end, *actor)?,
        }
        Ok(())
    }

    fn script_mut(&mut self) -> &mut ScriptState {
        self.script.as_mut().expect("scripted")
    }

    fn bind_script(&mut self, id: super::BindingId, binding: Binding) -> Result<(), &'static str> {
        self.script_mut()
            .bind(id, binding)
            .ok_or("invalid or unexpected binding")?;
        self.hook_mask = self.script_mut().mask();
        Ok(())
    }

    fn scope_exists(&self, scope: super::VarScope) -> Result<(), &'static str> {
        let exists = match scope {
            super::VarScope::Scenario | super::VarScope::Region(_) => true,
            super::VarScope::Actor(actor) => self.actors.contains_key(&actor),
            super::VarScope::Item(item) => self.items.get(&item).is_some(),
        };
        exists.then_some(()).ok_or("unknown variable scope")
    }

    /// Scenario authority over a door: no reach or cost, but a door never
    /// closes onto an occupant.
    fn script_set_door(&mut self, at: Location, open: bool) -> Result<(), &'static str> {
        let door = self.world.door(at).ok_or("no door there")?;
        if door.open == open {
            return Ok(());
        }
        let base = self.world.door_location(door.id).ok_or("no door there")?;
        if !open && self.door_obstructed(base, door.height) {
            return Err("door is obstructed");
        }
        let affected = self.hold_geometry_candidates(self.world.door_cells(base));
        self.world.set_door(base, open);
        self.reconcile_holds_for_actors(affected)
            .map_err(|_| "door change breaks a hold")?;
        self.note_hook(HookKind::DoorChanged, None, |_| {
            Some((
                FactSubject::Door { at: base, open },
                vec![FilterKey::Cell(base), FilterKey::Region(base.region)],
            ))
        });
        Ok(())
    }

    fn script_end_run(&mut self, end: RunEnd, actor: Option<ActorId>) -> Result<(), &'static str> {
        if let Some(actor) = actor {
            if !self.combat.characters.contains(&actor) {
                return Err("run end names a non-character");
            }
        }
        let crate::run_policy::RunMode::Adventure(run) = &mut self.combat.run else {
            return Err("arena runs end by their own policy");
        };
        if run.outcome.terminal {
            return Ok(());
        }
        match end {
            RunEnd::Victory => {
                let victor = actor
                    .or_else(|| self.combat.characters.iter().next().copied())
                    .ok_or("no character to win")?;
                run.outcome.victor = Some(victor);
            }
            RunEnd::Defeat => {}
        }
        run.outcome.terminal = true;
        Ok(())
    }
}

/// Group facts by handler in first-appearance order, preserving fact order.
fn group(pending: Vec<PendingFact>) -> Vec<(HandlerId, Vec<PendingFact>)> {
    let mut groups: Vec<(HandlerId, Vec<PendingFact>)> = Vec::new();
    for fact in pending {
        match groups.iter_mut().find(|(handler, _)| *handler == fact.handler) {
            Some((_, batch)) => batch.push(fact),
            None => groups.push((fact.handler, vec![fact])),
        }
    }
    groups
}
