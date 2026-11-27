//! Presentation-thread state and modal input reduction over validated disclosure.
//! Native input becomes an effect for the shell; this module does not own a socket.
use serde::{Deserialize, Serialize};
use tor_client_common::annotations::{play_test_report, MAX_PLAY_TEST_REPORT_BYTES};
use tor_client_common::holds::{hold_choices, render_holds, HoldChoice};
use tor_client_common::items::{
    identification_action, item_action, item_choices, zap_action, ItemOperation,
};
use tor_client_common::ClientState;
use tor_protocol::*;

mod controls;
mod input;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AimBand {
    Origin,
    Hidden,
    Short,
    Long,
    Outside,
}
impl AimBand {
    fn label(self) -> &'static str {
        match self {
            Self::Origin => "Choose a square other than your own",
            Self::Hidden => "Choose a visible square",
            Self::Short => "Short range",
            Self::Long => "Long range",
            Self::Outside => "Out of range",
        }
    }

    fn permits_shot(self) -> bool {
        matches!(self, Self::Short | Self::Long)
    }
}

struct ProjectileCursor {
    // The observation is pinned by the projectile picker.
    origin: Position,
    position: Position,
    range: ProjectileRangeView,
    visible: std::collections::BTreeSet<Position>,
}
impl ProjectileCursor {
    fn offset(&self) -> [i32; 3] {
        [
            self.position.x - self.origin.x,
            self.position.y - self.origin.y,
            self.position.z - self.origin.z,
        ]
    }

    fn band(&self) -> AimBand {
        let cells = self
            .offset()
            .into_iter()
            .map(i32::unsigned_abs)
            .max()
            .unwrap_or(0);
        match cells {
            0 => AimBand::Origin,
            cells if cells > self.range.long_cells => AimBand::Outside,
            _ if !self.visible.contains(&self.position) => AimBand::Hidden,
            cells if cells <= self.range.short_cells => AimBand::Short,
            _ => AimBand::Long,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProjectileShortcut {
    Manual,
    Fire,
    Quiver,
}

struct PendingWeaponSwap {
    incoming: ItemTarget,
    removing: bool,
    update_secondary: bool,
    outgoing: ItemTarget,
    slot: u16,
    intention: Option<IntentionId>,
    resolved: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    /// Open read-only detection charts.
    Charts,
    Discoveries,
    Advancement,
    Attack,
    Abilities,
    Search,
    Throw,
    Fire,
    Quiver,
    Apply,
    Wield,
    SetSecondaryWeapon,
    SwapWeapon,
    Wear,
    PutOn,
    TakeOff,
    RemoveAccessory,
    Commands,
    SaveQuit,
    RunPrefix,
    MoveOnlyPrefix,
    Places,
    Travel,
    Up,
    Down,
    Left,
    Right,
    NorthEast,
    SouthEast,
    SouthWest,
    NorthWest,
    Ascend,
    Descend,
    Wait,
    Pickup,
    Drop,
    Equip,
    Unequip,
    Drink,
    Eat,
    Read,
    Zap,
    Ignite,
    Extinguish,
    OpenDoor,
    CloseDoor,
    Control,
    Release,
    ResumeIntention,
    CancelIntention,
    Note,
    PlayTestReport,
    Screenshot,
    /// Open the authenticated wizard command editor.
    Wizard,
    Enter,
    Escape,
    /// Try to escape one exact disclosed incoming hold.
    EscapeHold,
    Backspace,
    Tab,
    History,
    OlderHistory,
    RecentHistory,
    /// Show journey steps more slowly.
    Slower,
    /// Show journey steps more quickly.
    Faster,
    /// Open the log of earlier messages.
    MessageLog,
    /// Show everything carried.
    Inventory,
    /// Inspect the attached actor's disclosed creature stats.
    Stats,
    /// Describe a map cell without taking a turn.
    Look,
    /// Show commands and map symbols.
    Help,
    /// Keep moving in a direction until something interesting happens.
    RunUp,
    RunDown,
    RunLeft,
    RunRight,
    RunNorthEast,
    RunSouthEast,
    RunSouthWest,
    RunNorthWest,
}

/// The native keyboard and opt-in process-test driver share this input boundary.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Input {
    Click { x: usize, y: usize },
    Key { key: Key },
    Text { text: String },
}

#[derive(Debug, PartialEq, Eq)]
/// Work for the native shell or connection worker after local input reduction.
/// A request still requires current shared admission checks and a server response.
pub enum Effect {
    /// Capture only the next genuinely presented game framebuffer.
    Screenshot,
    /// Stop only chart hydration; never a gameplay request.
    CancelCharts,
    /// A fresh, basis-free overview. The worker presents Observe before querying.
    OpenOverview(Box<tor_client_common::advancement::OpenOverview>),
    None,
    Quit,
    Request(Box<Request>),
    /// Show the rest of a journey without waiting.
    Skip,
    /// Space shown updates this many milliseconds apart.
    Pace(u64),
}

/// The step delays [`Key::Slower`] and [`Key::Faster`] move between.
pub const PACES_MS: [u64; 9] = [0, 25, 50, 75, 100, 150, 200, 300, 500];

/// The default delay between shown journey steps.
pub const DEFAULT_PACE_MS: u64 = 75;

pub struct NoteDraft {
    pub report: bool,
    pub text: String,
    pub audience: Audience,
    snapshot: SnapshotId,
}

#[derive(serde::Serialize)]
pub enum SupplyPurpose {
    Ability {
        ability: Box<AbilityView>,
        invocation: AbilityInvocation,
    },
    Respec,
}
#[derive(serde::Serialize)]
pub struct SupplyDraft {
    pub purpose: SupplyPurpose,
    pub label: String,
    pub costs: Vec<AbilitySupplyCost>,
    pub candidates: Vec<tor_client_common::abilities::SupplyCandidate>,
    pub checked: std::collections::BTreeSet<ItemTarget>,
}

#[derive(serde::Serialize)]
pub struct WeaponDraft {
    pub choices: Vec<tor_client_common::weapons::WeaponChoice>,
    previous_selected: usize,
    previous_status: String,
}

/// Local screens, selections and the displayed shared state for one ASCII window.
///
/// Update this reducer for every ordered disclosure before interpreting input.
/// Public fields support rendering and diagnostics; their values are not a second
/// rules engine. Transport and native-window ownership remain outside this type.
pub struct App {
    pub effect_animations: crate::effects::EffectAnimations,
    pub charts_open: bool,
    pub charts: tor_client_common::charts::ChartPages,
    pub chart_selected: usize,
    pub chart_scroll: usize,
    /// Current connection palette, including stale state during recovery.
    pub palette: tor_client_common::Palette,
    pub advancement_open: bool,
    pub advancement_command: String,
    pub advancement_scroll: usize,
    advancement_pages: tor_client_common::advancement::AdvancementPages,
    advancement_pending: Option<(u64, AdvancementQuery)>,
    pub bump_attacks: BumpAttacks,
    pub pace_ms: u64,
    pub attack_targets: Vec<ActorView>,
    pub ability_choices: Vec<AbilityView>,
    /// Frozen exact offers for the explicit hold picker, never rebound by index.
    pub hold_choices: Vec<HoldChoice>,
    pub selected_ability: Option<AbilityView>,
    pub supply_selection: Option<SupplyDraft>,
    pub selected_weapon: WeaponSelection,
    pub weapon_selection: Option<WeaponDraft>,
    pub weapon_confirmation: Option<tor_client_common::weapon_preview::WeaponPreview>,
    pub projectile_selection: Option<tor_client_common::projectiles::ProjectileMenu>,
    fire_alternate: Option<String>,
    projectile_cursor: Option<ProjectileCursor>,
    projectile_shortcut: ProjectileShortcut,
    /// Sole candidate seen so far during the server-authoritative source scan.
    launcher_candidate: Option<EquippedProjectileWeapon>,
    scan_launchers: bool,
    quiver: Option<ItemTarget>,
    /// The look cursor, while choosing a cell to describe.
    pub look_cursor: Option<Position>,
    pub inventory_open: bool,
    secondary_weapon: Option<ItemTarget>,
    choosing_secondary_weapon: bool,
    weapon_swap: Option<PendingWeaponSwap>,
    pub help_page: Option<HelpPage>,
    pub discovery_rows: Option<Vec<String>>,
    pub discovery_scroll: usize,
    pub stats_open: bool,
    inspection: Option<Vec<String>>,
    inspection_title: &'static str,
    strike_pages: tor_client_common::inspection_pages::InspectionPages,
    strike_sources_open: bool,
    strike_source_selected: usize,
    strike_detail_open: bool,
    pub stats_scroll: usize,
    /// The end-of-run screen was closed.
    pub end_dismissed: bool,
    /// The direction being run in, between steps.
    pub running: Option<Direction>,
    /// Inventory letters, kept for as long as an item is carried.
    pub letters: std::collections::BTreeMap<ItemTarget, char>,
    /// The last request sent was a one-cell move, so a refusal means the
    /// way is blocked.
    last_move: bool,
    pub places_open: bool,
    pub place_selected: usize,
    pub place_name: Option<String>,
    pub travel_cursor: Option<Position>,
    pub role: AccessRole,
    pub state: Option<ClientState>,
    pub connected: bool,
    pub busy: bool,
    submitted: Option<(tor_client_common::compositor::AdmissionBoundary, bool)>,
    pub status: String,
    pub note: Option<NoteDraft>,
    pub wizard_command: Option<String>,
    /// Client command names, never wizard commands or undisclosed capabilities.
    pub command_line: Option<String>,
    pub command_selected: usize,
    command_selected_explicit: bool,
    movement_prefix: Option<controls::MovementPrefix>,
    pub pickup: Vec<ItemView>,
    /// Apply selects one disclosed usable portable light, not a generic game tool.
    pub applying: bool,
    pub dropping: bool,
    pub item_operation: Option<ItemOperation>,
    /// Exact carried device bound to the displayed observation, before aiming.
    zap_selection: Option<(ItemTarget, ReplyContext)>,
    pub identification_selected: usize,
    pub quantity: String,
    pub door_direction: Option<bool>,
    pub selected: usize,
    pub history_page: Option<HistoryPage>,
    pub history_scroll: usize,
    /// What happened since the player last acted, and earlier turns.
    pub messages: crate::messages::Messages,
    /// Rows scrolled back from the newest in the message log, when it's open.
    pub message_log: Option<usize>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            effect_animations: Default::default(),
            charts_open: false,
            charts: Default::default(),
            chart_selected: 0,
            chart_scroll: 0,
            palette: Default::default(),
            advancement_open: false,
            advancement_command: String::new(),
            advancement_scroll: 0,
            advancement_pages: Default::default(),
            advancement_pending: None,
            bump_attacks: BumpAttacks::Hostile,
            pace_ms: DEFAULT_PACE_MS,
            attack_targets: Vec::new(),
            ability_choices: Vec::new(),
            hold_choices: Vec::new(),
            selected_ability: None,
            supply_selection: None,
            selected_weapon: WeaponSelection::Default,
            weapon_selection: None,
            weapon_confirmation: None,
            projectile_selection: None,
            fire_alternate: None,
            projectile_cursor: None,
            projectile_shortcut: ProjectileShortcut::Manual,
            launcher_candidate: None,
            scan_launchers: false,
            quiver: None,
            look_cursor: None,
            inventory_open: false,
            secondary_weapon: None,
            choosing_secondary_weapon: false,
            weapon_swap: None,
            help_page: None,
            discovery_rows: None,
            discovery_scroll: 0,
            stats_open: false,
            inspection: None,
            inspection_title: "CREATURE INSPECTION",
            strike_pages: Default::default(),
            strike_sources_open: false,
            strike_source_selected: 0,
            strike_detail_open: false,
            stats_scroll: 0,
            end_dismissed: false,
            running: None,
            letters: Default::default(),
            last_move: false,
            places_open: false,
            place_selected: 0,
            place_name: None,
            travel_cursor: None,
            role: AccessRole::Spectator,
            state: None,
            connected: false,
            busy: true,
            submitted: None,
            status: "Connecting to the local server...".into(),
            note: None,
            wizard_command: None,
            command_line: None,
            command_selected: 0,
            command_selected_explicit: false,
            movement_prefix: None,
            pickup: vec![],
            applying: false,
            dropping: false,
            item_operation: None,
            zap_selection: None,
            identification_selected: 0,
            quantity: String::new(),
            door_direction: None,
            selected: 0,
            history_page: None,
            history_scroll: 0,
            messages: Default::default(),
            message_log: None,
        }
    }

    fn clear_magic_selection(&mut self) {
        self.zap_selection = None;
        self.identification_selected = 0;
        if self.applying
            || matches!(
                self.item_operation,
                Some(
                    ItemOperation::Read
                        | ItemOperation::Zap
                        | ItemOperation::Ignite
                        | ItemOperation::Extinguish
                )
            )
        {
            self.item_operation = None;
            self.pickup.clear();
            self.applying = false;
        }
    }

    pub fn set_state(&mut self, state: ClientState) {
        self.effect_animations.clear();
        self.movement_prefix = self
            .movement_prefix
            .map(|_| controls::MovementPrefix::Cancelled);
        let old = self
            .state
            .as_ref()
            .map(|s| (s.branch().clone(), s.state().revision));
        self.state = Some(state);
        self.quiver = None;
        self.secondary_weapon = None;
        self.close_secondary_weapon_picker();
        self.weapon_swap = None;
        self.submitted = None;
        // A new transport owner has no matching local chart selection.
        self.charts.clear();
        self.chart_selected = 0;
        self.clear_magic_selection();
        self.hold_choices.clear();
        self.state_changed(old);
    }

    /// Apply every disclosed boundary, even when several updates share a frame.
    pub fn update(&mut self, update: StreamUpdate) -> Result<(), tor_client_common::StreamError> {
        let spatial = match &update.body {
            UpdateBody::Outcome { facts, .. } => facts
                .facts
                .iter()
                .filter_map(|fact| match fact {
                    OutcomeFact::Combat {
                        event: event @ CombatEventView::Effect { .. },
                        ..
                    } => Some(event.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        let action_message = match &update.body {
            UpdateBody::Intention { status, failure } if status.phase != IntentionPhase::Queued => {
                Some((
                    failure.is_some(),
                    failure.map_or_else(
                        || phase_status(status.phase).to_owned(),
                        |reason| reason.message(),
                    ),
                ))
            }
            _ => None,
        };
        let swap_phase = match &update.body {
            UpdateBody::Intention { status, .. }
                if self
                    .weapon_swap
                    .as_ref()
                    .and_then(|swap| swap.intention.as_ref())
                    == Some(&status.intention) =>
            {
                Some(status.phase)
            }
            _ => None,
        };
        let observed = matches!(
            update.body,
            UpdateBody::Observation { .. } | UpdateBody::ObservationDelta { .. }
        );
        let outcome = matches!(update.body, UpdateBody::Outcome { .. });
        let underfoot = match &update.body {
            UpdateBody::Outcome { facts, .. } => facts.facts.iter().find_map(|fact| match fact {
                tor_protocol::OutcomeFact::OwnAction { underfoot, .. } => underfoot.clone(),
                _ => None,
            }),
            _ => None,
        };
        let control_changed = matches!(update.body, UpdateBody::Control { .. });
        let travel = match &update.body {
            UpdateBody::Travel { status, .. } => Some(status.phase),
            _ => None,
        };
        let state = self
            .state
            .as_mut()
            .ok_or(tor_client_common::StreamError::InconsistentState)?;
        let strength_loss = |observation: &Observation| {
            observation
                .combat
                .as_ref()?
                .own_stats
                .as_ref()
                .map(|stats| stats.restorable_losses.attributes.strength)
        };
        let previous_loss = strength_loss(&state.state().observation);
        // Escape checks send their own authoritative result. The timed hazard
        // has no such result, so infer only its older wait/expiry feedback.
        let was_bear_trapped = active_bear_trap(&state.state().observation)
            .is_some_and(|condition| condition.remaining_ticks.is_some());
        let struggling = self.last_move && was_bear_trapped;
        let old = Some((state.branch().clone(), state.state().revision));
        state.apply(update)?;
        for event in &spatial {
            self.effect_animations.push(event);
        }
        if control_changed {
            if !state.has_control() {
                self.submitted = None;
            }
            self.movement_prefix = self
                .movement_prefix
                .map(|_| controls::MovementPrefix::Cancelled);
        }
        if observed || control_changed {
            if self.applying
                || matches!(
                    self.item_operation,
                    Some(
                        ItemOperation::Read
                            | ItemOperation::Zap
                            | ItemOperation::Ignite
                            | ItemOperation::Extinguish
                    )
                )
            {
                self.item_operation = None;
                self.pickup.clear();
                self.applying = false;
            }
            self.zap_selection = None;
            self.identification_selected = 0;
            self.hold_choices.clear();
        }
        if outcome {
            let lines = state.narration_lines();
            // A spectator never sends commands; the followed actor's next
            // action starts its turn instead.
            if self.role == AccessRole::Spectator
                && lines
                    .iter()
                    .any(|line| matches!(line.topic, tor_client_common::narration::Topic::Own(_)))
            {
                self.messages.begin_turn();
            }
            if struggling
                && lines.iter().any(|line| {
                    matches!(
                        line.topic,
                        tor_client_common::narration::Topic::Own(tor_protocol::Event::Waited)
                    )
                })
            {
                self.messages.push("You struggle against the bear trap.");
            }
            self.messages.absorb_outcome_with_underfoot(
                lines,
                state.cursor().tick,
                state.outcome_status(),
                underfoot.as_ref(),
            );
        }
        if observed
            && previous_loss
                .zip(strength_loss(&state.state().observation))
                .is_some_and(|(before, after)| after > before)
        {
            self.messages.push("You feel weaker.");
        }
        if observed && was_bear_trapped && !bear_trapped(&state.state().observation) {
            self.messages.push("You get free of the bear trap.");
        }
        if let Some(reason) = travel.and_then(travel_stop) {
            self.messages.push(reason);
        }
        self.strike_pages.clear();
        self.strike_sources_open = false;
        self.strike_detail_open = false;
        self.inspection = None;
        self.state_changed(old);
        self.observe_weapon_swap(swap_phase);
        if self.role == AccessRole::Player {
            if let Some((failed, message)) = action_message {
                if failed {
                    self.messages.push(message.clone());
                }
                self.status = message;
            }
        }
        Ok(())
    }

    pub fn replace_snapshot(
        &mut self,
        snapshot: Snapshot,
    ) -> Result<(), tor_client_common::StreamError> {
        let old = self
            .state
            .as_ref()
            .map(|s| (s.branch().clone(), s.state().revision));
        if let Some(state) = &mut self.state {
            state.replace_snapshot(snapshot)?;
        } else {
            self.state = Some(ClientState::from_snapshot(snapshot)?);
        }
        self.movement_prefix = self
            .movement_prefix
            .map(|_| controls::MovementPrefix::Cancelled);
        self.effect_animations.clear();
        self.submitted = None;
        self.weapon_swap = None;
        self.close_secondary_weapon_picker();
        // Recovery cancels the transport's chart request. Return to the catalog
        // only after validation succeeds rather than leaving a loading selection
        // that has no corresponding worker query.
        self.discovery_rows = None;
        self.charts.clear();
        self.chart_selected = 0;
        self.hold_choices.clear();
        self.clear_magic_selection();
        self.inspection = None;
        self.strike_pages.clear();
        self.strike_sources_open = false;
        self.strike_detail_open = false;
        self.wizard_command = None;
        self.stats_open = false;
        self.close_advancement();
        self.state_changed(old);
        Ok(())
    }

    fn state_changed(&mut self, old: Option<(BranchId, u64)>) {
        if let Some(state) = &self.state {
            self.charts.observe(state);
        }
        self.chart_scroll = 0;
        if self
            .projectile_selection
            .as_ref()
            .is_some_and(|menu| self.state.as_ref().is_none_or(|state| !menu.current(state)))
        {
            self.close_projectile_picker();
            self.status = "Projectile choices changed; choose the ability again.".into();
        }
        if self.weapon_confirmation.as_ref().is_some_and(|preview| {
            self.state
                .as_ref()
                .is_none_or(|state| !preview.current(state))
        }) {
            self.weapon_confirmation = None;
            self.status = "Weapon preview changed; choose again before confirming.".into();
        }
        if (self.weapon_selection.is_some() || self.selected_weapon != WeaponSelection::Default)
            && self.state.as_ref().is_some_and(|state| {
                !state.has_control()
                    || old.as_ref().is_some_and(|(branch, revision)| {
                        branch != state.branch() || *revision != state.state().revision
                    })
            })
        {
            self.status = "Observation changed; choose your weapon again.".into();
        }
        if let Some(state) = &self.state {
            let had_private_page = self.advancement_pages.page().is_some();
            self.advancement_pages
                .observe(&state.reply_context(), state.has_control());
            if !state.has_control() || (had_private_page && self.advancement_pages.page().is_none())
            {
                self.close_advancement();
            }
        }
        let state = self.state.as_ref().expect("validated state");
        if self
            .zap_selection
            .as_ref()
            .is_some_and(|(_, context)| *context != state.reply_context())
        {
            self.zap_selection = None;
        }
        if old
            .as_ref()
            .is_some_and(|(branch, _)| branch != state.branch())
        {
            self.travel_cursor = None;
            self.movement_prefix = self
                .movement_prefix
                .map(|_| controls::MovementPrefix::Cancelled);
            self.command_line = None;
            self.note = None;
            self.pickup.clear();
            self.applying = false;
            self.item_operation = None;
            self.zap_selection = None;
            self.attack_targets.clear();
            self.ability_choices.clear();
            self.hold_choices.clear();
            self.selected_ability = None;
            self.supply_selection = None;
            self.weapon_selection = None;
            self.selected_weapon = WeaponSelection::Default;
            self.door_direction = None;
            self.places_open = false;
            self.place_name = None;
            self.history_page = None;
            self.history_scroll = 0;
            self.look_cursor = None;
            self.running = None;
            self.end_dismissed = false;
            self.messages.begin_turn();
            self.messages.reset_sightings();
            self.message_log = None;
            self.quiver = None;
            self.secondary_weapon = None;
            self.choosing_secondary_weapon = false;
            self.weapon_swap = None;
            self.status = "Timeline changed; pending selections cleared.".into();
        }
        if old.is_some_and(|(_, revision)| revision != state.state().revision) {
            self.place_name = None;
            self.place_selected = self
                .place_selected
                .min(state.state().observation.places.len().saturating_sub(1));
            self.pickup.clear();
            self.applying = false;
            self.item_operation = None;
            self.zap_selection = None;
            self.attack_targets.clear();
            self.ability_choices.clear();
            self.hold_choices.clear();
            self.selected_ability = None;
            self.supply_selection = None;
            self.weapon_selection = None;
            self.selected_weapon = WeaponSelection::Default;
            self.door_direction = None;
            self.travel_cursor = None;
        }
        if !state.has_control() {
            self.choosing_secondary_weapon = false;
            self.weapon_swap = None;
            self.movement_prefix = self
                .movement_prefix
                .map(|_| controls::MovementPrefix::Cancelled);
            self.pickup.clear();
            self.applying = false;
            self.item_operation = None;
            self.zap_selection = None;
            self.attack_targets.clear();
            self.ability_choices.clear();
            self.hold_choices.clear();
            self.selected_ability = None;
            self.supply_selection = None;
            self.weapon_selection = None;
            self.selected_weapon = WeaponSelection::Default;
            self.place_name = None;
            self.door_direction = None;
            self.travel_cursor = None;
        }
        let carried: std::collections::BTreeSet<_> = state
            .state()
            .observation
            .inventory
            .iter()
            .map(|item| item.id)
            .collect();
        // The preference is local, but its item must still be in this owner's picture.
        self.quiver = self.quiver.filter(|item| carried.contains(item));
        self.letters.retain(|item, _| carried.contains(item));
        for item in &state.state().observation.inventory {
            if !self.letters.contains_key(&item.id) {
                let used: std::collections::BTreeSet<_> = self.letters.values().copied().collect();
                if let Some(letter) = ('a'..='z')
                    .chain('A'..='Z')
                    .find(|letter| !used.contains(letter))
                {
                    self.letters.insert(item.id, letter);
                }
            }
        }
        self.connected = true;
    }

    pub fn ready(&mut self) {
        self.busy = false;
    }

    /// The server refused a request: stop any run and say why, in terms of
    /// what the player tried.
    pub fn refused(&mut self, status: String) {
        let status = if self.weapon_swap.take().is_some_and(|swap| !swap.removing) {
            format!("The old weapon was removed, but the new weapon was not equipped. {status}")
        } else {
            status
        };
        self.close_projectile_picker();
        self.weapon_confirmation = None;
        if self.advancement_open {
            self.advancement_pages.invalidate();
            self.advancement_pending = None;
        }
        if self.strike_detail_open || self.strike_sources_open {
            self.strike_pages.clear();
            self.strike_detail_open = false;
            self.strike_sources_open = false;
            self.inspection = None;
        }
        self.running = None;
        self.status = if self.last_move && status == "You can't do that now." {
            "You can't go that way.".into()
        } else {
            status
        };
    }

    /// Conservative run eligibility from the current picture, never map art.
    fn open_ahead(&self, direction: Direction) -> bool {
        let Some(state) = &self.state else {
            return false;
        };
        let o = &state.state().observation;
        let Some((dx, dy)) = horizontal_step(direction) else {
            return false;
        };
        let (Some(x), Some(y)) = (o.position.x.checked_add(dx), o.position.y.checked_add(dy))
        else {
            return false;
        };
        let mut feet = None;
        let mut known_drop = false;
        for cell in &o.visible_cells {
            if (cell.position.x, cell.position.y) != (x, y) {
                continue;
            }
            match i64::from(cell.position.z) - i64::from(o.position.z) {
                0 => feet = Some(cell),
                1 if cell.door.as_ref().is_some_and(|door| !door.open) => return false,
                -1 => known_drop = !cell.wall,
                _ => {}
            }
        }
        feet.is_some_and(|cell| {
            !cell.wall
                && cell.trap.is_none()
                && cell.door.as_ref().is_none_or(|door| door.open)
                && (!known_drop || cell.stairs_up || cell.stairs_down)
        })
    }

    /// A menu row's letter: an item's inventory letter when choosing from
    /// what's carried, otherwise a, b, c... down the list.
    pub fn choice_letter(&self, index: usize) -> char {
        let carried = self.dropping || self.item_operation.is_some() || self.applying;
        self.pickup
            .get(index)
            .and_then(|item| {
                carried
                    .then(|| self.letters.get(&item.id).copied())
                    .flatten()
            })
            .unwrap_or_else(|| ('a'..='z').chain('A'..='Z').nth(index).unwrap_or('?'))
    }

    /// Item picker rows use only this exact instance's disclosed charge count.
    pub fn item_rows(&self) -> Vec<String> {
        self.pickup
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let name = if !self.dropping && self.item_operation.is_none() && !self.applying {
                    self.state
                        .as_ref()
                        .and_then(|state| {
                            state
                                .state()
                                .observation
                                .ground_items
                                .iter()
                                .find(|g| g.item.id == item.id)
                        })
                        .map_or(item.name.as_str(), tor_client_common::items::ground_name)
                } else {
                    &item.name
                };
                let mut label = item_phrase(name, item.quantity);
                if let Some(charges) = self
                    .state
                    .as_ref()
                    .and_then(|state| state.state().observation.interactions.as_ref())
                    .and_then(|view| view.inventory.iter().find(|offer| offer.item == item.id))
                    .and_then(|offer| offer.charges)
                {
                    label.push_str(&format!(" [charges: {charges}]"));
                }
                format!(
                    "{} {} - {}",
                    if index == self.selected { ">" } else { " " },
                    self.choice_letter(index),
                    label
                )
            })
            .collect()
    }

    /// Whether another creature is in sight, which stops a run.
    pub fn creature_in_view(&self) -> bool {
        self.state.as_ref().is_some_and(|state| {
            let o = &state.state().observation;
            o.visible_actors.iter().any(|a| a.id != o.self_target)
        })
    }

    /// Take the next step of a run, or end it when something interesting
    /// happened: a message, a creature in sight, or the way ahead not open.
    pub fn continue_run(&mut self) -> Effect {
        let Some(direction) = self.running else {
            return Effect::None;
        };
        let Some(state) = &self.state else {
            self.running = None;
            return Effect::None;
        };
        if ended(state) {
            self.running = None;
            return Effect::None;
        }
        // Admission opens a queue slot even while other actors are advancing.
        // A run must inspect their settled results before choosing its next step;
        // unlike explicit one-step input it must not queue from an intermediate view.
        if self.busy
            || !state.observation_current()
            || !state.character_ready()
            || !state.can_admit_intention()
        {
            return Effect::None;
        }
        if !self.messages.text().is_empty()
            || self.creature_in_view()
            || !self.open_ahead(direction)
        {
            self.running = None;
            return Effect::None;
        }
        self.act(Action::Move { direction })
    }

    pub fn note_admission(&mut self, receipt: &RequestReceipt) {
        if let (Some(swap), RequestReceipt::Admitted { intention, .. }) =
            (&mut self.weapon_swap, receipt)
        {
            if swap.intention.is_none() {
                swap.intention = Some(intention.clone());
            }
        }
        self.submitted = tor_client_common::compositor::AdmissionBoundary::new(receipt)
            .map(|work| (work, false));
    }

    pub fn input_yielded(&mut self) {
        if let (Some((work, yielded)), Some(state)) = (&mut self.submitted, &self.state) {
            *yielded = work.remains_queued(state);
        }
    }

    pub fn intention_hint(&self) -> Option<String> {
        if self.role == AccessRole::Spectator {
            return None;
        }
        let state = self.state.as_ref()?;
        let pending = state.intention_for_input()?;
        let label = match pending.phase {
            IntentionPhase::Paused => "Attack paused.",
            IntentionPhase::Started => "Attack in progress.",
            IntentionPhase::Suspended => "Action suspended.",
            _ if self
                .submitted
                .as_ref()
                .is_some_and(|(work, yielded)| *yielded && work.remains_queued(state)) =>
            {
                tor_client_common::compositor::ACTION_PENDING
            }
            _ => "",
        };
        let mut hint = String::from(label);
        if state.can_resume_intention() {
            hint.push_str(" F8 resume;");
        }
        if state.can_cancel_intention() {
            hint.push_str(" F9 cancel;");
        }
        if state.can_cancel_sequence() {
            hint.push_str(" move to cancel and step;");
        }
        hint.push_str(if state.has_control() {
            " #release control; F2 history; S save/close"
        } else {
            " F3 acquire control; F2 history; S save/close"
        });
        Some(hint)
    }

    pub fn disconnect(&mut self, message: String) {
        self.weapon_swap = None;
        self.choosing_secondary_weapon = false;
        self.discovery_rows = None;
        self.command_line = None;
        self.movement_prefix = self
            .movement_prefix
            .map(|_| controls::MovementPrefix::Cancelled);
        self.applying = false;
        self.charts.clear();
        self.charts_open = false;
        self.close_projectile_picker();
        self.close_advancement();
        self.travel_cursor = None;
        self.place_name = None;
        self.places_open = false;
        self.connected = false;
        self.busy = false;
        self.inspection = None;
        self.strike_pages.clear();
        self.strike_sources_open = false;
        self.strike_detail_open = false;
        self.wizard_command = None;
        self.stats_open = false;
        self.note = None;
        self.pickup.clear();
        self.applying = false;
        self.item_operation = None;
        self.zap_selection = None;
        self.identification_selected = 0;
        self.attack_targets.clear();
        self.ability_choices.clear();
        self.hold_choices.clear();
        self.selected_ability = None;
        self.supply_selection = None;
        self.weapon_selection = None;
        self.selected_weapon = WeaponSelection::Default;
        self.door_direction = None;
        self.weapon_confirmation = None;
        self.status = message;
    }

    pub fn show_inspection(&mut self, report: &CreatureInspectionView) -> Result<(), String> {
        if self.role != AccessRole::Wizard {
            return Err("Creature inspection requires wizard authority".into());
        }
        let context = self
            .state
            .as_ref()
            .ok_or("No current attachment")?
            .reply_context();
        self.strike_pages.set_report(report, &context)?;
        self.strike_sources_open = false;
        self.strike_detail_open = false;
        let rows = tor_client_common::inspection::lines(report)
            .map_err(|error| error.to_string())?
            .into_iter()
            .flat_map(|line| crate::messages::word_wrap(&line, STATS_WIDTH))
            .collect();
        self.inspection_title = "CREATURE INSPECTION";
        self.inspection = Some(rows);
        self.stats_open = true;
        self.stats_scroll = 0;
        Ok(())
    }

    pub fn bind_strike_request(&mut self, request_id: &str) {
        self.strike_pages.bind_request(request_id);
    }

    pub fn show_strike_page(
        &mut self,
        context: &ReplyContext,
        request_id: &str,
        page: &CreatureStrikePage,
    ) -> Result<(), String> {
        if self.role != AccessRole::Wizard {
            return Err("Strike inspection requires wizard authority".into());
        }
        if !self.stats_open || !(self.strike_sources_open || self.strike_detail_open) {
            self.strike_pages.clear();
            return Err("Strike detail is no longer open.".into());
        }
        let current = self
            .state
            .as_ref()
            .ok_or("No current attachment")?
            .reply_context();
        if &current != context {
            self.strike_pages.clear();
            return Err("Strike detail is stale. Reopen the creature inspection.".into());
        }
        self.strike_pages.accept(context, request_id, page)?;
        self.strike_detail_open = true;
        self.strike_sources_open = false;
        self.stats_open = true;
        self.stats_scroll = 0;
        Ok(())
    }

    pub fn show_combat_diagnostics(
        &mut self,
        report: &tor_protocol::CombatDiagnosticsView,
    ) -> Result<(), String> {
        if self.role != AccessRole::Wizard {
            return Err("Combat diagnostics require wizard authority".into());
        }
        self.strike_pages.clear();
        self.strike_sources_open = false;
        self.strike_detail_open = false;
        let rows = tor_client_common::combat_diagnostics::lines(report)
            .map_err(|error| error.to_string())?
            .into_iter()
            .flat_map(|line| crate::messages::word_wrap(&line, STATS_WIDTH))
            .collect();
        self.inspection_title = "CREATURE INSPECTION";
        self.inspection = Some(rows);
        self.stats_open = true;
        self.stats_scroll = 0;
        Ok(())
    }

    pub fn show_wizard_report(&mut self, report: &WizardReport) -> Result<(), String> {
        if self.role != AccessRole::Wizard {
            return Err("Wizard report requires wizard authority".into());
        }
        report.validate().map_err(|error| error.to_string())?;
        self.strike_pages.clear();
        self.strike_sources_open = false;
        self.strike_detail_open = false;
        self.inspection_title = "WIZARD REPORT";
        self.inspection = Some(
            std::iter::once(&report.title)
                .chain(&report.lines)
                .flat_map(|line| crate::messages::word_wrap(line, STATS_WIDTH))
                .collect(),
        );
        self.stats_open = true;
        self.stats_scroll = 0;
        Ok(())
    }

    pub fn show_creature_catalog(&mut self, page: &CreatureCatalogPage) -> Result<(), String> {
        if self.role != AccessRole::Wizard {
            return Err("Creature catalog requires wizard authority".into());
        }
        let rows = tor_client_common::creature_catalog::lines(page)
            .map_err(|error| error.to_string())?
            .into_iter()
            .flat_map(|line| crate::messages::word_wrap(&line, STATS_WIDTH))
            .collect();
        self.strike_pages.clear();
        self.strike_sources_open = false;
        self.strike_detail_open = false;
        self.inspection_title = "CREATURE CATALOG";
        self.inspection = Some(rows);
        self.stats_open = true;
        self.stats_scroll = 0;
        Ok(())
    }

    pub fn stats_title(&self) -> &'static str {
        if self.has_inspection() {
            self.inspection_title
        } else {
            "CREATURE STATS"
        }
    }

    pub fn has_inspection(&self) -> bool {
        self.inspection.is_some()
    }

    pub fn stats_rows(&self) -> std::borrow::Cow<'_, [String]> {
        if self.strike_sources_open {
            let mut rows =
                vec!["Strike sources: Up/Down choose; Enter opens; Escape returns".into()];
            rows.extend(
                self.strike_pages
                    .sources()
                    .iter()
                    .enumerate()
                    .map(|(index, source)| {
                        format!(
                            "{} {}",
                            if index == self.strike_source_selected {
                                ">"
                            } else {
                                " "
                            },
                            source.label
                        )
                    }),
            );
            return std::borrow::Cow::Owned(rows);
        }
        if self.strike_detail_open {
            if let Some(rows) = self.strike_pages.lines() {
                let mut rows: Vec<String> = rows
                    .iter()
                    .flat_map(|line| crate::messages::word_wrap(line, STATS_WIDTH))
                    .collect();
                rows.push("Tab: sources; [: previous page; ]: next page; Escape: back".into());
                return std::borrow::Cow::Owned(rows);
            }
        }
        if let Some(rows) = &self.inspection {
            return std::borrow::Cow::Borrowed(rows);
        }
        let combat = self
            .state
            .as_ref()
            .and_then(|state| state.state().observation.combat.as_ref());
        std::borrow::Cow::Owned(
            tor_client_common::stats::lines(combat)
                .into_iter()
                .chain(self.hold_rows())
                .flat_map(|line| crate::messages::word_wrap(&line, STATS_WIDTH))
                .collect(),
        )
    }

    /// Read-only hold rows use only the current observation's disclosed names.
    pub fn hold_rows(&self) -> Vec<String> {
        self.state
            .as_ref()
            .map(|state| render_holds(&state.state().observation))
            .unwrap_or_default()
    }

    /// Native reports retain the wire offer shape and the modal's exact order.
    pub fn hold_choice_offers(&self) -> Vec<LinkedHold> {
        let Some(state) = &self.state else {
            return Vec::new();
        };
        self.hold_choices
            .iter()
            .filter_map(|choice| {
                state
                    .state()
                    .observation
                    .linked_holds
                    .iter()
                    .find(|offer| offer.target == choice.target)
                    .cloned()
            })
            .collect()
    }

    fn escape_hold(&mut self) -> Effect {
        let Some(state) = &self.state else {
            return Effect::None;
        };
        if self.role == AccessRole::Spectator || !state.has_control() {
            self.status = "Escaping a hold requires control of your character.".into();
            return Effect::None;
        }
        if !state.can_admit_intention() {
            self.status = "You can't act right now.".into();
            return Effect::None;
        }
        let choices = hold_choices(&state.state().observation);
        match choices.as_slice() {
            [] => {
                self.status = "You are not held.".into();
                Effect::None
            }
            [only] => self.act(Action::Escape { hold: only.target }),
            _ => {
                self.hold_choices = choices;
                self.selected = 0;
                self.status =
                    "Choose a hold. Up/Down select, Enter confirms, Esc or F11 cancels.".into();
                Effect::None
            }
        }
    }

    fn hold_input(&mut self, input: Input) -> Effect {
        match input {
            Input::Key {
                key: Key::Escape | Key::EscapeHold,
            } => {
                self.hold_choices.clear();
                self.status = "Never mind.".into();
            }
            Input::Key { key: Key::Up } => self.selected = self.selected.saturating_sub(1),
            Input::Key { key: Key::Down } => {
                self.selected = self
                    .selected
                    .saturating_add(1)
                    .min(self.hold_choices.len() - 1);
            }
            Input::Key { key: Key::Enter } if self.connected && !self.busy => {
                let hold = self
                    .hold_choices
                    .get(self.selected)
                    .map(|choice| choice.target);
                self.hold_choices.clear();
                let Some(hold) = hold else {
                    return Effect::None;
                };
                if self.role == AccessRole::Spectator {
                    self.status = "Spectator access is read-only.".into();
                    return Effect::None;
                }
                if self.state.as_ref().is_some_and(|state| {
                    state
                        .state()
                        .observation
                        .linked_holds
                        .iter()
                        .any(|offer| offer.target == hold)
                }) {
                    return self.act(Action::Escape { hold });
                }
                self.status = "That hold is no longer available.".into();
            }
            _ => {}
        }
        Effect::None
    }

    fn select_targets(&mut self, ability: Option<AbilityView>) {
        self.selected_ability = ability;
        self.attack_targets = self
            .state
            .as_ref()
            .map(|state| {
                let view = &state.state().observation;
                let mut targets: Vec<_> = view
                    .visible_actors
                    .iter()
                    .filter(|actor| actor.id != view.self_target)
                    .filter(|actor| {
                        self.selected_ability.as_ref().is_none_or(|ability| {
                            tor_client_common::abilities::target_eligible(view, ability, actor.id)
                        })
                    })
                    .cloned()
                    .collect();
                if self.selected_ability.as_ref().is_some_and(|ability| {
                    tor_client_common::abilities::target_eligible(view, ability, view.self_target)
                }) {
                    targets.insert(
                        0,
                        ActorView {
                            appearance_seed: None,
                            worn: Vec::new(),
                            asset: None,
                            id: view.self_target,
                            name: "yourself".into(),
                            description: String::new(),
                            position: view.position,
                        },
                    );
                }
                targets
            })
            .unwrap_or_default();
        let mut seen = std::collections::BTreeSet::new();
        self.attack_targets.retain(|actor| seen.insert(actor.id));
        self.selected = 0;
        self.status = self.target_prompt();
        if self.attack_targets.is_empty() {
            self.selected_ability = None;
            self.supply_selection = None;
            self.weapon_selection = None;
            self.selected_weapon = WeaponSelection::Default;
        }
    }

    fn target_prompt(&self) -> String {
        let Some(target) = self.attack_targets.get(self.selected) else {
            return "No target is visible.".into();
        };
        let operation = self.selected_ability.as_ref().map_or_else(
            || "Attack".to_owned(),
            |ability| format!("Use {} on", ability.name),
        );
        format!(
            "{operation} {}{}? Up/Down select, {}Enter confirms, Esc cancels.",
            target.name,
            self.selected_weapon_label(),
            if self.weapon_selection_available() {
                "Tab chooses a weapon, "
            } else {
                ""
            }
        )
    }

    fn selected_weapon_label(&self) -> String {
        let WeaponSelection::Equipped { item, socket } = self.selected_weapon else {
            return String::new();
        };
        self.state
            .as_ref()
            .and_then(|state| {
                tor_client_common::weapons::choices(&state.state().observation)
                    .into_iter()
                    .find(|row| row.item == item && row.socket == socket)
            })
            .map_or_else(
                || " with a stale weapon selection".into(),
                |row| format!(" with {}", row.name),
            )
    }

    pub(crate) fn weapon_selection_available(&self) -> bool {
        self.ability_choices
            .get(self.selected)
            .or(self.selected_ability.as_ref())
            .map_or_else(
                || {
                    !self.attack_targets.is_empty()
                        && self.state.as_ref().is_some_and(|state| {
                            tor_client_common::abilities::available(&state.state().observation)
                                .iter()
                                .any(|ability| ability.basic_attack && ability.weapon_selection)
                        })
                },
                |ability| ability.weapon_selection,
            )
    }

    fn open_weapon_picker(&mut self) -> Effect {
        if !self.weapon_selection_available() {
            self.status = "That ability does not select a weapon.".into();
            return Effect::None;
        }
        let Some(state) = self.state.as_ref().filter(|state| state.has_control()) else {
            return Effect::None;
        };
        let choices = tor_client_common::weapons::choices(&state.state().observation);
        let selected = choices
            .iter()
            .position(|row| {
                self.selected_weapon
                    == WeaponSelection::Equipped {
                        item: row.item,
                        socket: row.socket,
                    }
            })
            .map_or(0, |index| index + 1);
        self.weapon_selection = Some(WeaponDraft {
            choices,
            previous_selected: self.selected,
            previous_status: self.status.clone(),
        });
        self.selected = selected;
        self.status =
            "Choose one weapon for this action. Enter selects; Esc keeps the previous choice."
                .into();
        Effect::None
    }
    pub fn weapon_rows(&self) -> Vec<String> {
        let Some(draft) = &self.weapon_selection else {
            return Vec::new();
        };
        let mut rows = vec![format!(
            "{} Default primary weapon or natural attack",
            if self.selected == 0 { ">" } else { " " }
        )];
        rows.extend(draft.choices.iter().enumerate().map(|(index, row)| {
            format!(
                "{} {} ({}, socket {})",
                if self.selected == index + 1 { ">" } else { " " },
                row.name,
                row.equipped,
                row.socket
            )
        }));
        rows
    }
    fn weapon_input(&mut self, input: Input) -> Effect {
        let Some(draft) = self.weapon_selection.take() else {
            return Effect::None;
        };
        match input {
            Input::Key { key: Key::Escape } => {
                self.selected = draft.previous_selected;
                self.status = draft.previous_status;
                return Effect::None;
            }
            Input::Key { key: Key::Up } => self.selected = self.selected.saturating_sub(1),
            Input::Key { key: Key::Down } => {
                self.selected = (self.selected + 1).min(draft.choices.len())
            }
            Input::Key { key: Key::Enter } => {
                let selected = if self.selected == 0 {
                    WeaponSelection::Default
                } else {
                    let row = &draft.choices[self.selected - 1];
                    WeaponSelection::Equipped {
                        item: row.item,
                        socket: row.socket,
                    }
                };
                if let Some(state) = &self.state {
                    match tor_client_common::weapons::validate(&state.state().observation, selected)
                    {
                        Ok(()) => {
                            self.selected_weapon = selected;
                            self.selected = draft.previous_selected;
                            self.status = if self.attack_targets.is_empty() {
                                format!("Weapon selected. {}", draft.previous_status)
                            } else {
                                self.target_prompt()
                            };
                            return Effect::None;
                        }
                        Err(reason) => self.status = reason,
                    }
                }
            }
            _ => {}
        }
        self.weapon_selection = Some(draft);
        Effect::None
    }

    fn choose_ability_target(&mut self, ability: AbilityView, target: ActorTarget) -> Effect {
        self.choose_ability_invocation(
            ability,
            tor_protocol::AbilityInvocation {
                aim: tor_protocol::AbilityAim::ActorTarget { target },
                weapon: tor_protocol::WeaponSelection::Default,
            },
        )
    }
    fn choose_ability_invocation(
        &mut self,
        ability: AbilityView,
        mut invocation: AbilityInvocation,
    ) -> Effect {
        invocation.weapon = std::mem::replace(&mut self.selected_weapon, WeaponSelection::Default);
        let Some(state) = self.state.as_ref() else {
            return Effect::None;
        };
        if !ability.supply_costs.is_empty() {
            let candidates = tor_client_common::abilities::supply_candidates(
                &state.state().observation,
                &ability,
            );
            self.supply_selection = Some(SupplyDraft {
                label: tor_client_common::abilities::ability_label(&ability),
                costs: ability.supply_costs.clone(),
                purpose: SupplyPurpose::Ability {
                    ability: Box::new(ability),
                    invocation,
                },
                candidates,
                checked: Default::default(),
            });
            self.selected = 0;
            self.status = "Choose carried inventory automatically, or exact supplies. Space toggles; Enter confirms; Esc cancels.".into();
            return Effect::None;
        }
        match tor_client_common::abilities::invocation_action(
            &state.state().observation,
            &ability.id,
            invocation,
            SupplySelection::Carried,
        ) {
            Ok(action) => self.act(action),
            Err(reason) => {
                self.status = reason;
                Effect::None
            }
        }
    }

    pub fn supply_rows(&self) -> Vec<String> {
        let Some(draft) = &self.supply_selection else {
            return Vec::new();
        };
        let mut rows = vec![
            draft.label.clone(),
            format!(
                "{} Automatic carried inventory",
                if self.selected == 0 { ">" } else { " " }
            ),
        ];
        for (index, row) in draft.candidates.iter().enumerate() {
            rows.push(format!(
                "{} [{}] {} x{} ({})",
                if self.selected == index + 1 { ">" } else { " " },
                if draft.checked.contains(&row.item) {
                    "x"
                } else {
                    " "
                },
                row.name,
                row.quantity,
                if row.carried { "carried" } else { "ground" }
            ));
        }
        rows
    }

    fn supply_input(&mut self, input: Input) -> Effect {
        let Some(mut draft) = self.supply_selection.take() else {
            return Effect::None;
        };
        match input {
            Input::Key { key: Key::Escape } => {
                self.status = "Never mind.".into();
                return Effect::None;
            }
            Input::Key { key: Key::Up } => self.selected = self.selected.saturating_sub(1),
            Input::Key { key: Key::Down } => {
                self.selected = (self.selected + 1).min(draft.candidates.len())
            }
            Input::Key { key: Key::Wait } => {
                if self.selected == 0 {
                    draft.checked.clear();
                } else if let Some(row) = draft.candidates.get(self.selected - 1) {
                    if !draft.checked.remove(&row.item) {
                        if draft.checked.len() < tor_protocol::MAX_SUPPLY_FRAGMENTS {
                            draft.checked.insert(row.item);
                        } else {
                            self.status = "At most 32 exact supply stacks can be selected.".into();
                        }
                    }
                }
            }
            Input::Key { key: Key::Enter } => {
                if let Some(state) = self.state.as_ref() {
                    let view = &state.state().observation;
                    let selection = if self.selected == 0 {
                        Ok(SupplySelection::Carried)
                    } else {
                        if draft.checked.is_empty() {
                            if let Some(row) = draft.candidates.get(self.selected - 1) {
                                draft.checked.insert(row.item);
                            }
                        }
                        tor_client_common::abilities::selection_from_items_for_costs(
                            view,
                            &draft.costs,
                            &draft.checked.iter().copied().collect::<Vec<_>>(),
                        )
                    };
                    let command = selection.and_then(|selection| match &draft.purpose {
                        SupplyPurpose::Ability {
                            ability,
                            invocation,
                        } => tor_client_common::abilities::invocation_action(
                            view,
                            &ability.id,
                            *invocation,
                            selection,
                        )
                        .map(|action| Command::Act { action }),
                        SupplyPurpose::Respec => {
                            self.advancement_pages
                                .confirm(&state.reply_context(), view, selection)
                        }
                    });
                    match command {
                        Ok(command) => {
                            self.last_move = false;
                            return match command {
                                Command::Act { action, .. } => self.act(action),
                                other => self.command(other),
                            };
                        }
                        Err(reason) => self.status = reason,
                    }
                }
            }
            _ => {}
        }
        self.supply_selection = Some(draft);
        Effect::None
    }

    fn close_advancement(&mut self) {
        self.advancement_open = false;
        self.advancement_command.clear();
        self.advancement_scroll = 0;
        self.advancement_pending = None;
        self.advancement_pages.clear();
        if self
            .supply_selection
            .as_ref()
            .is_some_and(|draft| matches!(draft.purpose, SupplyPurpose::Respec))
        {
            self.supply_selection = None;
            self.weapon_selection = None;
            self.selected_weapon = WeaponSelection::Default;
        }
    }

    pub fn advancement_rows(&self) -> Vec<String> {
        let rows = self.state.as_ref().map_or_else(Vec::new, |state| {
            self.advancement_pages.display(&state.state().observation)
        });
        rows.iter()
            .flat_map(|line| crate::messages::word_wrap(line, STATS_WIDTH))
            .collect()
    }

    pub fn prepare_overview(&mut self, revision: u64) -> Result<(), String> {
        let state = self.state.as_ref().ok_or("No current attachment.")?;
        if !self.advancement_open || !state.has_control() || state.state().revision != revision {
            return Err("Advancement is no longer open on this picture.".into());
        }
        self.advancement_pending = Some((revision, AdvancementQuery::Overview {}));
        Ok(())
    }

    pub fn bind_advancement_request(&mut self, id: &str) -> Result<(), String> {
        let Some((revision, query)) = self.advancement_pending.take() else {
            return Err("Advancement request is no longer open.".into());
        };
        let context = self
            .state
            .as_ref()
            .ok_or("No current attachment.")?
            .reply_context();
        self.advancement_pages.track(&context, id, revision, &query)
    }

    pub fn show_advancement_page(
        &mut self,
        context: &ReplyContext,
        id: &str,
        page: &AdvancementPage,
    ) -> Result<(), String> {
        let state = self.state.as_ref().ok_or("No current attachment.")?;
        if !self.advancement_open
            || !state.has_control()
            || self.role == AccessRole::Spectator
            || state.reply_context() != *context
        {
            self.close_advancement();
            return Err("Advancement page is no longer current.".into());
        }
        // The network worker has already enforced its negotiated frame limit.
        self.advancement_pages
            .accept(context, id, page, MAX_ADVANCEMENT_PAGE_BYTES)?;
        self.advancement_scroll = 0;
        self.status = "Advancement loaded. Type a command below; Enter submits, Esc closes.".into();
        Ok(())
    }

    fn advancement_action(
        &mut self,
        input: tor_client_common::advancement::AdvancementInput,
    ) -> Effect {
        use tor_client_common::advancement::AdvancementInput;
        self.last_move = false;
        let Some(state) = &self.state else {
            return Effect::None;
        };
        if matches!(input, AdvancementInput::Overview) {
            return match tor_client_common::advancement::OpenOverview::new(state) {
                Ok(plan) => {
                    self.advancement_pages.clear();
                    self.advancement_pending = None;
                    self.busy = true;
                    self.status = "Refreshing advancement...".into();
                    Effect::OpenOverview(Box::new(plan))
                }
                Err(error) => {
                    self.status = error;
                    Effect::None
                }
            };
        }
        let choose_ration = matches!(input, AdvancementInput::Confirm(None));
        match self.advancement_pages.request(input, state) {
            Ok(Some(request)) => {
                if choose_ration
                    && matches!(
                        &request,
                        Request::Command {
                            command: Command::Act {
                                action: Action::Respec { .. },
                                ..
                            },
                            ..
                        }
                    )
                {
                    match self.advancement_pages.supply_costs() {
                        Ok(costs) => {
                            self.supply_selection = Some(SupplyDraft {
                                purpose: SupplyPurpose::Respec,
                                label: "Confirm this respec with one ration. Choose carried or one exact source.".into(),
                                candidates: self.advancement_pages.supply_candidates(&state.state().observation),
                                costs,
                                checked: Default::default(),
                            });
                            self.selected = 0;
                            self.status = "Choose the ration source; Enter confirms the exact retained respec.".into();
                        }
                        Err(error) => self.status = error,
                    }
                    return Effect::None;
                }
                if let Request::Command {
                    command:
                        Command::Advancement {
                            expected_revision,
                            query,
                        },
                    ..
                } = &request
                {
                    self.advancement_pending = Some((*expected_revision, query.clone()));
                }
                self.request(request)
            }
            Ok(None) => {
                self.advancement_scroll = 0;
                Effect::None
            }
            Err(error) => {
                self.status = error;
                Effect::None
            }
        }
    }

    fn advancement_input(&mut self, input: Input) -> Effect {
        use tor_client_common::advancement::{parse, AdvancementInput};
        if matches!(
            input,
            Input::Key {
                key: Key::Escape | Key::Advancement
            }
        ) {
            self.close_advancement();
            return Effect::None;
        }
        if self.busy || !self.connected {
            return Effect::None;
        }
        match input {
            Input::Text { text } => {
                for ch in text.chars().filter(|ch| !ch.is_control()) {
                    if self.advancement_command.len() + ch.len_utf8() <= 512 {
                        self.advancement_command.push(ch);
                    }
                }
            }
            Input::Key {
                key: Key::Backspace,
            } => {
                self.advancement_command.pop();
            }
            Input::Key { key: Key::Enter } => {
                let text = std::mem::take(&mut self.advancement_command);
                match parse(&text) {
                    Ok(input) => return self.advancement_action(input),
                    Err(error) => self.status = error,
                }
            }
            Input::Key { key: Key::Up } => {
                self.advancement_scroll = self.advancement_scroll.saturating_sub(1)
            }
            Input::Key { key: Key::Down } => {
                self.advancement_scroll = (self.advancement_scroll + 1).min(
                    self.advancement_rows()
                        .len()
                        .saturating_sub(ADVANCEMENT_ROWS),
                )
            }
            Input::Key {
                key: Key::OlderHistory,
            } => return self.advancement_action(AdvancementInput::Previous),
            Input::Key {
                key: Key::RecentHistory,
            } => return self.advancement_action(AdvancementInput::Next),
            Input::Key {
                key: Key::ResumeIntention,
            } => return self.advancement_action(AdvancementInput::Resume),
            Input::Key {
                key: Key::CancelIntention,
            } => return self.advancement_action(AdvancementInput::Cancel),
            _ => {}
        }
        Effect::None
    }

    /// The owner-only paid choice currently disclosed by the server.
    pub fn identification_choice(&self) -> Option<&IdentificationChoiceView> {
        let state = self.state.as_ref()?;
        if !self.connected || self.role == AccessRole::Spectator || !state.has_control() {
            return None;
        }
        state
            .state()
            .observation
            .interactions
            .as_ref()?
            .identification
            .as_ref()
    }

    pub fn identification_empty_message(&self) -> Option<String> {
        let choice = self.identification_choice()?;
        if !choice.items.is_empty() {
            return None;
        }
        let categories = choice
            .classes
            .iter()
            .map(|class| match class {
                ItemClass::Armor => "armor".into(),
                ItemClass::Food => "food".into(),
                ItemClass::Misc => "miscellaneous items".into(),
                class => format!("{}s", format!("{class:?}").to_ascii_lowercase()),
            })
            .collect::<Vec<String>>()
            .join(" or ");
        Some(format!("No {categories} in your pack can be identified."))
    }

    /// Exact inventory entries in the server's category-specific offer order.
    fn identification_entries(&self) -> Vec<InventoryEntry> {
        let Some(choice) = self.identification_choice() else {
            return vec![];
        };
        let view = &self
            .state
            .as_ref()
            .expect("current choice")
            .state()
            .observation;
        let entries = inventory_entries(self, view);
        choice
            .items
            .iter()
            .filter_map(|target| {
                view.inventory
                    .iter()
                    .position(|item| item.id == *target)
                    .and_then(|index| entries.get(index).cloned())
            })
            .collect()
    }

    pub fn identification_rows(&self) -> Vec<String> {
        self.identification_entries()
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                format!(
                    "{} {}",
                    if index == self.identification_selected {
                        ">"
                    } else {
                        " "
                    },
                    entry.long()
                )
            })
            .collect()
    }

    /// Aiming never outlives the exact observation where the device was selected.
    pub fn zap_item(&self) -> Option<ItemTarget> {
        let (item, context) = self.zap_selection.as_ref()?;
        let state = self.state.as_ref()?;
        (self.connected
            && self.role != AccessRole::Spectator
            && state.has_control()
            && state.reply_context() == *context)
            .then_some(*item)
    }

    fn identification_input(&mut self, input: Input) -> Effect {
        if self.busy || !self.connected {
            return Effect::None;
        }
        let targets: Vec<_> = self
            .identification_choice()
            .map(|choice| choice.items.clone())
            .unwrap_or_default();
        let selected = match input {
            Input::Key { key: Key::Escape } => Some(None),
            Input::Key { key: Key::Up } => {
                self.identification_selected = self.identification_selected.saturating_sub(1);
                None
            }
            Input::Key { key: Key::Down } => {
                self.identification_selected =
                    (self.identification_selected + 1).min(targets.len().saturating_sub(1));
                None
            }
            Input::Key { key: Key::Enter } => {
                targets.get(self.identification_selected).copied().map(Some)
            }
            Input::Text { text } => {
                text.chars()
                    .find(char::is_ascii_alphabetic)
                    .and_then(|letter| {
                        let item = targets
                            .iter()
                            .copied()
                            .find(|target| self.letters.get(target) == Some(&letter));
                        if item.is_none() {
                            self.status = format!("No offered item is lettered {letter}.");
                        }
                        item.map(Some)
                    })
            }
            _ => None,
        };
        let Some(item) = selected else {
            return Effect::None;
        };
        match identification_action(
            &self
                .state
                .as_ref()
                .expect("current choice")
                .state()
                .observation,
            item,
        ) {
            Ok(action) => self.act(action),
            Err(reason) => {
                self.status = reason;
                Effect::None
            }
        }
    }

    fn zap_input(&mut self, input: Input) -> Effect {
        if self.busy || !self.connected {
            return Effect::None;
        }
        if matches!(input, Input::Key { key: Key::Escape }) {
            self.zap_selection = None;
            self.status = "Never mind.".into();
            return Effect::None;
        }
        if let Input::Click { x, y } = input {
            let action = self.state.as_ref().and_then(|state| {
                let position = crate::render::known_cell_at(state, x, y)?;
                let item = self.zap_item()?;
                Some(tor_client_common::items::zap_cell_action(
                    &state.state().observation,
                    item,
                    [position.x, position.y, position.z],
                ))
            });
            if let Some(action) = action {
                self.zap_selection = None;
                return match action {
                    Ok(action) => self.act(action),
                    Err(reason) => {
                        self.status = reason;
                        Effect::None
                    }
                };
            }
            return Effect::None;
        }
        let Input::Key { key } = input else {
            return Effect::None;
        };
        let direction = key.step_direction();
        if direction.is_none() && key != Key::Wait {
            return Effect::None;
        }
        let Some(item) = self.zap_item() else {
            self.zap_selection = None;
            self.status = "That device selection is no longer current.".into();
            return Effect::None;
        };
        let view = &self
            .state
            .as_ref()
            .expect("current device")
            .state()
            .observation;
        let result = if let Some(direction) = direction {
            zap_action(view, item, direction)
        } else {
            tor_client_common::items::zap_self_action(view, item)
        };
        self.zap_selection = None;
        match result {
            Ok(action) => self.act(action),
            Err(reason) => {
                self.status = reason;
                Effect::None
            }
        }
    }

    /// A delayed local timeout cannot cancel another view or selected sample.
    pub fn chart_cancelled(&mut self, query: &chart_pages::ChartQuery) {
        let Some(state) = &self.state else {
            return;
        };
        if query.context != *state.context()
            || query.branch != *state.branch()
            || query.base != state.observation_base()
            || !state.state().observation.charts.iter().any(|summary| {
                Some(summary.kind) == self.charts.selected()
                    && summary.chart.as_ref() == Some(&query.chart)
            })
        {
            return;
        }
        self.charts.clear();
        self.chart_selected = 0;
        self.chart_scroll = 0;
        self.status = "Chart page timed out; select the chart again to retry.".into();
    }

    pub fn chart_lines(&self) -> Vec<String> {
        let Some(state) = &self.state else {
            return vec!["No chart view is attached.".into()];
        };
        let mut lines = Vec::new();
        if self.charts.selected().is_none() {
            for (index, summary) in state.state().observation.charts.iter().enumerate() {
                lines.push(format!(
                    "{} {}",
                    if index == self.chart_selected {
                        ">"
                    } else {
                        " "
                    },
                    tor_client_common::charts::kind_label(summary.kind)
                ));
            }
        }
        lines.extend(self.charts.lines(state));
        lines
            .into_iter()
            .flat_map(|line| crate::wrap(&line, 96))
            .collect()
    }
    fn chart_input(&mut self, input: Input) -> Effect {
        use tor_client_common::charts::ChartInput as Chart;
        let Input::Key { key } = input else {
            return Effect::None;
        };
        if matches!(key, Key::Escape | Key::Charts) {
            self.charts_open = false;
            if let Some(state) = &self.state {
                let _ = self.charts.request(Chart::Cancel, state);
            }
            return Effect::CancelCharts;
        }
        let Some(state) = &self.state else {
            return Effect::None;
        };
        let command = match key {
            Key::Tab => {
                self.chart_scroll = 0;
                Some(Chart::Overview)
            }
            Key::Up if self.charts.selected().is_none() => {
                self.chart_selected = self.chart_selected.saturating_sub(1);
                None
            }
            Key::Down if self.charts.selected().is_none() => {
                self.chart_selected = (self.chart_selected + 1)
                    .min(state.state().observation.charts.len().saturating_sub(1));
                None
            }
            Key::Up => {
                self.chart_scroll = self.chart_scroll.saturating_sub(1);
                None
            }
            Key::Down => {
                self.chart_scroll =
                    (self.chart_scroll + 1).min(self.chart_lines().len().saturating_sub(24));
                None
            }
            Key::OlderHistory => {
                self.chart_scroll = 0;
                Some(Chart::Previous)
            }
            Key::RecentHistory => {
                self.chart_scroll = 0;
                Some(Chart::Next)
            }
            Key::Enter if self.charts.selected().is_none() => state
                .state()
                .observation
                .charts
                .get(self.chart_selected)
                .map(|s| Chart::Select(s.kind)),
            _ => None,
        };
        if let Some(command) = command {
            match self.charts.request(command, state) {
                Ok(Some(request)) => return Effect::Request(Box::new(request)),
                Ok(None) => {}
                Err(reason) => self.status = reason,
            }
        }
        // Every unhandled key/click is swallowed here, never navigation or an action.
        Effect::None
    }

    /// Reduce input through the active modal screen before ordinary map controls.
    /// Returns an effect for the shell; it neither sends nor executes a game action.
    fn travel_to(&mut self, position: Position) -> Effect {
        let Some(state) = self.state.as_ref() else {
            return Effect::None;
        };
        if self.role == AccessRole::Spectator
            || !state.has_control()
            || !state.can_admit_intention()
        {
            self.status = "Travel requires permission to admit an action.".into();
            return Effect::None;
        }
        let Some(cell) = state
            .map_cell(position)
            .filter(|c| !c.wall && c.door.as_ref().is_none_or(|d| d.open))
        else {
            self.status = "Select a known floor cell.".into();
            return Effect::None;
        };
        let command = Command::Travel {
            expected_revision: state.state().revision,
            destination: cell.key,
        };
        self.travel_cursor = None;
        self.command(command)
    }

    pub(crate) fn secondary_weapon_hint(&self) -> String {
        let Some(item) = self.secondary_weapon else {
            return "Esc or i closes; no secondary weapon chosen".into();
        };
        if let Some(name) = self
            .state
            .as_ref()
            .and_then(|state| {
                state
                    .state()
                    .observation
                    .inventory
                    .iter()
                    .find(|row| row.id == item)
            })
            .map(|row| row.name.as_str())
        {
            format!("Esc or i closes; secondary: {name} (x)")
        } else {
            "Esc or i closes; secondary weapon unavailable".into()
        }
    }

    fn close_secondary_weapon_picker(&mut self) {
        if std::mem::take(&mut self.choosing_secondary_weapon) {
            self.item_operation = None;
            self.pickup.clear();
        }
    }

    fn swap_weapon(&mut self) -> Effect {
        let Some(incoming) = self.secondary_weapon else {
            self.status = "No secondary weapon chosen. Use #secondary.".into();
            return Effect::None;
        };
        self.wield_weapon(incoming, true)
    }

    fn wield_weapon(&mut self, incoming: ItemTarget, update_secondary: bool) -> Effect {
        let Some(state) = self.state.as_ref() else {
            return Effect::None;
        };
        if !state.observation_current() || !state.character_ready() {
            self.status = "Wait until your character is ready to wield a weapon.".into();
            return Effect::None;
        }
        let view = &state.state().observation;
        let action = match item_action(view, incoming, ItemOperation::Wield) {
            Ok(action @ Action::Equip { .. }) => action,
            Err(reason) => {
                self.status = reason;
                return Effect::None;
            }
            _ => unreachable!("Wield submits an equip"),
        };
        let Action::Equip { slot, .. } = action else {
            unreachable!()
        };
        let Some(outgoing) = weapon_in_socket(view, slot) else {
            if update_secondary {
                self.status = "Nothing is wielded to swap. Wield a primary weapon first.".into();
                return Effect::None;
            }
            return self.act(action);
        };
        let effect = self.act(Action::Unequip { item: outgoing });
        if matches!(effect, Effect::Request(_)) {
            self.weapon_swap = Some(PendingWeaponSwap {
                incoming,
                removing: true,
                update_secondary,
                outgoing,
                slot,
                intention: None,
                resolved: false,
            });
        }
        effect
    }

    fn observe_weapon_swap(&mut self, phase: Option<IntentionPhase>) {
        let Some(swap) = self.weapon_swap.as_mut() else {
            return;
        };
        match phase {
            Some(
                IntentionPhase::Cancelled
                | IntentionPhase::Failed
                | IntentionPhase::Paused
                | IntentionPhase::Suspended,
            ) => {
                // Never resume the client shortcut automatically. The ordinary
                // server action can still be resumed at the player's direction.
                self.weapon_swap = None;
            }
            Some(IntentionPhase::Resolved) => swap.resolved = true,
            _ => {}
        }
    }

    pub fn continue_weapon_swap(&mut self) -> Effect {
        let Some(state) = self.state.as_ref() else {
            return Effect::None;
        };
        if self.busy
            || !state.has_control()
            || !state.observation_current()
            || !state.character_ready()
            || !state.can_admit_intention()
            || !self.weapon_swap.as_ref().is_some_and(|swap| swap.resolved)
        {
            return Effect::None;
        }
        let Some(mut swap) = self.weapon_swap.take() else {
            return Effect::None;
        };
        let view = &state.state().observation;
        let held = weapon_in_socket(view, swap.slot);
        if !swap.removing {
            // The matching completion may have minted a singleton for a stack.
            if held.is_some() && swap.update_secondary {
                self.secondary_weapon = view
                    .inventory
                    .iter()
                    .any(|item| item.id == swap.outgoing)
                    .then_some(swap.outgoing);
            }
            return Effect::None;
        }
        if held.is_some() {
            self.status = "Wield stopped: the weapon socket is still occupied.".into();
            return Effect::None;
        }
        let action = Action::Equip {
            item: swap.incoming,
            slot: swap.slot,
        };
        if item_action(view, swap.incoming, ItemOperation::Wield).as_ref() != Ok(&action) {
            self.status =
                "Old weapon removed. The new weapon is no longer available for that socket.".into();
            return Effect::None;
        }
        swap.removing = false;
        swap.resolved = false;
        swap.intention = None;
        let effect = self.act(action);
        if matches!(effect, Effect::Request(_)) {
            self.weapon_swap = Some(swap);
        }
        effect
    }

    fn open_projectile_shortcut(&mut self, shortcut: ProjectileShortcut) -> Effect {
        let Some(state) = &self.state else {
            return Effect::None;
        };
        let observation = &state.state().observation;
        let ability = |name| {
            tor_client_common::abilities::match_command(observation, name)
                .ok()
                .flatten()
                .filter(|(ability, rest)| {
                    rest.is_empty()
                        && ability.target_kind == AbilityTargetKind::DirectionalProjectile
                })
                .map(|(ability, _)| ability.clone())
        };
        let launch = ability("launch");
        let throw = (shortcut == ProjectileShortcut::Fire)
            .then(|| ability("throw"))
            .flatten();
        if let Some(launch) = launch {
            let effect = self.choose_ability(launch);
            if effect != Effect::None {
                self.projectile_shortcut = shortcut;
                self.scan_launchers = true;
                self.fire_alternate = throw.map(|ability| ability.id);
            }
            effect
        } else if let Some(throw) = throw {
            self.choose_ability(throw)
        } else {
            self.named_ability("launch", AbilityTargetKind::DirectionalProjectile)
        }
    }

    /// Follow accepted server pages, never inferring launcher or ammo eligibility.
    pub fn continue_fire(&mut self) -> Effect {
        use tor_client_common::projectiles::Step;
        if self.busy || self.projectile_shortcut == ProjectileShortcut::Manual {
            return Effect::None;
        }
        let (Some(state), Some(menu)) = (&self.state, &mut self.projectile_selection) else {
            return Effect::None;
        };
        if menu.step() != Step::Choices {
            return Effect::None;
        }
        let command = match menu.query().map(|query| query.phase) {
            Some(ProjectileChoicePhase::Sources {}) if self.scan_launchers => {
                for choice in menu.choices() {
                    if let ProjectileChoice::Launcher { launcher } = choice {
                        if self.launcher_candidate.replace(*launcher).is_some() {
                            self.scan_launchers = false;
                            return Effect::None; // Multiple launchers retain the explicit picker.
                        }
                    }
                }
                if menu.has_next() {
                    menu.next(state)
                } else {
                    self.scan_launchers = false;
                    if let Some(launcher) = self.launcher_candidate.take() {
                        menu.launcher_ammunition(state, launcher)
                    } else if self.fire_alternate.is_some() {
                        return self.switch_fire_source();
                    } else {
                        return Effect::None;
                    }
                }
            }
            Some(ProjectileChoicePhase::Ammunition { .. }) => {
                if let Some(preferred) = self
                    .quiver
                    .filter(|_| self.projectile_shortcut == ProjectileShortcut::Fire)
                {
                    if let Some(index) = menu
                        .choices()
                        .iter()
                        .position(|row| row.item() == preferred)
                    {
                        return self.select_projectile_choice(index);
                    }
                    if menu.has_next() {
                        menu.next(state)
                    } else {
                        self.quiver = None;
                        if menu.has_previous() {
                            menu.first(state)
                        } else {
                            return Effect::None;
                        }
                    }
                } else if menu.choices().is_empty() && menu.has_next() {
                    menu.next(state)
                } else {
                    return Effect::None;
                }
            }
            _ => return Effect::None,
        };
        match command {
            Ok(command) => {
                self.selected = 0;
                self.command(command)
            }
            Err(reason) => {
                self.close_projectile_picker();
                self.status = reason;
                Effect::None
            }
        }
    }

    fn switch_fire_source(&mut self) -> Effect {
        let Some(alternate) = self.fire_alternate.take() else {
            return Effect::None;
        };
        let current = self
            .projectile_selection
            .as_ref()
            .and_then(|menu| menu.query())
            .map(|query| query.ability.clone());
        let effect = self.open_projectile_picker(&alternate);
        if effect != Effect::None {
            self.fire_alternate = current;
        }
        effect
    }

    fn close_projectile_picker(&mut self) {
        self.projectile_selection = None;
        self.projectile_cursor = None;
    }

    fn open_projectile_picker(&mut self, ability: &str) -> Effect {
        self.projectile_cursor = None;
        self.fire_alternate = None;
        self.projectile_shortcut = ProjectileShortcut::Manual;
        self.scan_launchers = false;
        self.launcher_candidate = None;
        let Some(state) = &self.state else {
            return Effect::None;
        };
        match tor_client_common::projectiles::ProjectileMenu::new(state, ability) {
            Ok(menu) => {
                let command = menu.command().expect("new source query");
                self.projectile_selection = Some(menu);
                self.selected = 0;
                self.command(command)
            }
            Err(reason) => {
                self.status = reason;
                Effect::None
            }
        }
    }
    pub fn bind_projectile_request(&mut self, id: &str) -> Result<(), String> {
        if let Some(menu) = self.projectile_selection.as_mut() {
            menu.bind_request(id)?;
        }
        Ok(())
    }
    pub fn show_projectile_choices(
        &mut self,
        context: &ReplyContext,
        id: &str,
        page: &ProjectileChoicePage,
    ) -> Result<(), String> {
        let Some(menu) = self.projectile_selection.as_mut() else {
            return Ok(());
        };
        menu.accept(
            self.state.as_ref().ok_or("No current observation.")?,
            context,
            id,
            page,
        )?;
        self.selected = 0;
        self.status = if menu.choices().is_empty() && !menu.has_previous() && !menu.has_next() {
            "No eligible projectile choices. Enter or Esc closes."
        } else if matches!(page.query.phase, ProjectileChoicePhase::Ammunition { .. }) {
            if self.projectile_shortcut == ProjectileShortcut::Quiver {
                "Choose ammunition to quiver. Left/Right pages; Esc cancels."
            } else {
                "Choose compatible ammunition. Left/Right pages; Esc cancels."
            }
        } else {
            "Choose a source. Left/Right pages; Esc cancels."
        }
        .into();
        Ok(())
    }
    pub fn projectile_rows(&self) -> Vec<String> {
        let Some(menu) = &self.projectile_selection else {
            return vec![];
        };
        let mut rows: Vec<_> = menu
            .rows()
            .into_iter()
            .enumerate()
            .map(|(i, row)| format!("{} {row}", if i == self.selected { ">" } else { " " }))
            .collect();
        if menu.has_previous() {
            rows.push("Left: previous choices".into());
        }
        if menu.has_next() {
            rows.push("Right: more choices".into());
        }
        if self.fire_alternate.is_some()
            && menu.step() == tor_client_common::projectiles::Step::Choices
            && menu
                .query()
                .is_some_and(|query| matches!(query.phase, ProjectileChoicePhase::Sources {}))
        {
            rows.push("Tab: switch between launcher and hand throw".into());
        }
        rows
    }
    fn select_projectile_choice(&mut self, index: usize) -> Effect {
        let state = self
            .state
            .as_ref()
            .expect("projectile choice has current state");
        let menu = self
            .projectile_selection
            .as_mut()
            .expect("projectile picker");
        match menu.choose(state, index) {
            Ok(Some(command)) => {
                self.selected = 0;
                self.command(command)
            }
            Ok(None) => {
                if self.projectile_shortcut != ProjectileShortcut::Manual {
                    if let Some(ProjectileDeliverySelection::Launch { ammunition, .. }) =
                        menu.delivery()
                    {
                        self.quiver = Some(ammunition);
                        if self.projectile_shortcut == ProjectileShortcut::Quiver {
                            self.close_projectile_picker();
                            self.status = "Ammunition quivered.".into();
                            return Effect::None;
                        }
                    }
                }
                self.status = "Choose a direction. Movement keys select; Esc cancels.".into();
                Effect::None
            }
            Err(reason) => {
                self.status = reason;
                Effect::None
            }
        }
    }

    fn submit_aimed_projectile(&mut self, action: Result<Action, String>, quick: bool) -> Effect {
        self.close_projectile_picker();
        match action {
            Ok(action) if quick => self.act_confirmed(action),
            Ok(action) => self.act(action),
            Err(reason) => {
                self.status = reason;
                Effect::None
            }
        }
    }

    pub fn projectile_cursor(&self) -> Option<(Position, AimBand)> {
        let cursor = self.projectile_cursor.as_ref()?;
        Some((cursor.position, cursor.band()))
    }

    fn open_projectile_cursor(&mut self) {
        let (Some(state), Some(menu)) = (&self.state, &self.projectile_selection) else {
            return;
        };
        let Some(range) = menu.range(state) else {
            self.status = "This projectile's cell range is not disclosed. Use a direction.".into();
            return;
        };
        let view = &state.state().observation;
        let origin = view.position;
        self.projectile_cursor = Some(ProjectileCursor {
            origin,
            position: origin,
            range,
            visible: view
                .visible_cells
                .iter()
                .filter_map(|cell| (cell.position.z == origin.z).then_some(cell.position))
                .collect(),
        });
    }

    fn projectile_cursor_input(&mut self, input: Input) -> Effect {
        let cursor = self.projectile_cursor.as_mut().expect("aiming cursor");
        let state = self
            .state
            .as_ref()
            .expect("picker has a current observation");
        match input {
            Input::Key { key: Key::Tab } => {
                self.projectile_cursor = None;
            }
            Input::Key { key: Key::Enter } => {
                if !cursor.band().permits_shot() {
                    return Effect::None;
                }
                let menu = self.projectile_selection.as_ref().expect("aiming picker");
                let action = menu.cell_action(state, cursor.offset());
                return self.submit_aimed_projectile(action, true);
            }
            Input::Key { key } => {
                if let Some(moved) = nudge(cursor.position, key) {
                    cursor.position = crate::map::clamp(&state.state().observation, moved);
                }
            }
            Input::Click { x, y } => {
                if let Some((dx, dy)) = crate::map::column_at(x, y) {
                    cursor.position.x = cursor.origin.x.saturating_add(dx);
                    cursor.position.y = cursor.origin.y.saturating_add(dy);
                }
            }
            Input::Text { .. } => {}
        }
        Effect::None
    }

    fn projectile_input(&mut self, input: Input) -> Effect {
        use tor_client_common::projectiles::Step;
        if matches!(input, Input::Key { key: Key::Escape }) {
            self.close_projectile_picker();
            self.status = "Never mind.".into();
            return Effect::None;
        }
        let Some(state) = self.state.as_ref() else {
            self.close_projectile_picker();
            return Effect::None;
        };
        let menu = self
            .projectile_selection
            .as_mut()
            .expect("projectile picker");
        if !menu.current(state) {
            self.close_projectile_picker();
            self.status = "Projectile choices changed; choose the ability again.".into();
            return Effect::None;
        }
        if self.busy {
            return Effect::None;
        }
        if self.projectile_cursor.is_some() {
            return self.projectile_cursor_input(input);
        }
        if menu.step() == Step::Direction && matches!(input, Input::Key { key: Key::Tab }) {
            self.open_projectile_cursor();
            return Effect::None;
        }
        if menu.step() == Step::Direction {
            if let Input::Click { x, y } = input {
                if let Some(position) = crate::render::known_cell_at(state, x, y) {
                    let action = menu.cell_action(state, [position.x, position.y, position.z]);
                    return self.submit_aimed_projectile(action, false);
                }
                return Effect::None;
            }
        }
        let Input::Key { key } = input else {
            return Effect::None;
        };
        if menu.step() == Step::Direction {
            if let Some(direction) = key.step_direction() {
                let action = menu.action(state, direction);
                let quick = self.projectile_shortcut == ProjectileShortcut::Fire;
                return self.submit_aimed_projectile(action, quick);
            }
            return Effect::None;
        }
        let command = match key {
            Key::Up => {
                self.selected = self.selected.saturating_sub(1);
                return Effect::None;
            }
            Key::Down => {
                self.selected = (self.selected + 1).min(menu.choices().len().saturating_sub(1));
                return Effect::None;
            }
            Key::Left => menu.previous(state).map(Some),
            Key::Right => menu.next(state).map(Some),
            Key::Tab
                if menu.query().is_some_and(|query| {
                    matches!(query.phase, ProjectileChoicePhase::Sources {})
                }) =>
            {
                return self.switch_fire_source();
            }
            Key::Enter if menu.choices().is_empty() && !menu.has_previous() && !menu.has_next() => {
                self.close_projectile_picker();
                self.status = "Never mind.".into();
                return Effect::None;
            }
            Key::Enter => return self.select_projectile_choice(self.selected),
            _ => return Effect::None,
        };
        match command {
            Ok(Some(command)) => {
                self.selected = 0;
                self.command(command)
            }
            Ok(None) => Effect::None,
            Err(reason) => {
                self.status = reason;
                Effect::None
            }
        }
    }

    pub fn bind_weapon_preview(&mut self, id: &str) -> Result<(), String> {
        self.weapon_confirmation
            .as_mut()
            .ok_or("Weapon preview was cancelled.")?
            .bind_request(id)
    }
    pub fn show_weapon_preview(
        &mut self,
        context: &ReplyContext,
        id: &str,
        preview: &AbilityWeaponPreview,
    ) -> Result<(), String> {
        let Some(pending) = self.weapon_confirmation.as_mut() else {
            return Ok(());
        };
        let state = self.state.as_ref().ok_or("No current observation.")?;
        pending.accept(state, context, id, preview)?;
        self.status =
            "Review the exact weapon terms. Enter commits this action; Esc cancels.".into();
        Ok(())
    }
    pub fn projectile_prompt(&self) -> Option<String> {
        let inventory = &self.state.as_ref()?.state().observation.inventory;
        let name = |target| {
            inventory
                .iter()
                .find(|item| item.id == target)
                .map_or("projectile", |item| item.name.as_str())
        };
        let action = |delivery| match delivery {
            ProjectileDeliverySelection::Throw {
                source:
                    ThrownSourceSelection::Equipped { item, .. }
                    | ThrownSourceSelection::Carried { item, .. },
            } => format!("Throw the {}", name(item)),
            ProjectileDeliverySelection::Launch {
                launcher,
                ammunition,
            } => format!(
                "Fire {} with the {}",
                item_phrase(name(ammunition), 1),
                name(launcher.item)
            ),
        };
        if let Some(preview) = &self.weapon_confirmation {
            if let Some((offset, delivery)) = preview.projectile_cell_aim() {
                return Some(format!(
                    "{} toward cell {}, {}, {}? {} Esc cancels.",
                    action(delivery),
                    offset[0],
                    offset[1],
                    offset[2],
                    if preview.ready() {
                        "Enter confirms."
                    } else {
                        "Checking shot."
                    }
                ));
            }
            let (direction, delivery) = preview.projectile_aim()?;
            let action = action(delivery);
            return Some(if preview.ready() {
                format!(
                    "{action} {}? Enter confirms. Esc cancels.",
                    direction_name(direction)
                )
            } else {
                format!(
                    "{action} {}? Checking shot. Esc cancels.",
                    direction_name(direction)
                )
            });
        }
        let delivery = self.projectile_selection.as_ref()?.delivery()?;
        if let Some(cursor) = &self.projectile_cursor {
            let band = cursor.band();
            let confirm = if band.permits_shot() {
                " Enter fires."
            } else {
                ""
            };
            return Some(format!(
                "{}: {}. Move cursor;{confirm} Tab switches to direction. Esc cancels.",
                action(delivery),
                band.label()
            ));
        }
        let hint = if self.status.starts_with("This projectile's cell range") {
            self.status.as_str()
        } else {
            "Choose a direction; Tab aims at a square."
        };
        Some(format!("{}. {hint}", action(delivery)))
    }

    pub fn weapon_preview_rows(&self) -> Vec<String> {
        self.weapon_confirmation
            .as_ref()
            .map_or_else(Vec::new, |preview| preview.lines())
    }
    fn weapon_confirmation_input(&mut self, input: Input) -> Effect {
        match input {
            Input::Key { key: Key::Escape } => {
                self.weapon_confirmation = None;
                self.status = "Never mind.".into();
            }
            Input::Key { key: Key::Enter }
                if !self.busy
                    && self
                        .weapon_confirmation
                        .as_ref()
                        .is_some_and(|preview| preview.ready()) =>
            {
                let preview = self.weapon_confirmation.take().expect("ready preview");
                let confirmed = self
                    .state
                    .as_ref()
                    .ok_or("No current observation.".into())
                    .and_then(|state| preview.confirm(state));
                match confirmed {
                    Ok(action) => return self.act_confirmed(action),
                    Err(error) => self.status = error,
                }
            }
            _ => {}
        }
        Effect::None
    }
    fn act(&mut self, action: Action) -> Effect {
        if tor_client_common::weapon_preview::explicit_action(&action) {
            let Some(state) = self.state.as_ref() else {
                return Effect::None;
            };
            match tor_client_common::weapon_preview::WeaponPreview::new(state, action) {
                Ok(preview) => {
                    let command = preview.command();
                    self.weapon_confirmation = Some(preview);
                    return self.command(command);
                }
                Err(error) => {
                    self.status = error;
                    return Effect::None;
                }
            }
        }
        self.act_confirmed(action)
    }
    fn act_confirmed(&mut self, mut action: Action) -> Effect {
        let Some(state) = &self.state else {
            return Effect::None;
        };
        if !state.has_control() {
            self.attack_targets.clear();
            self.place_name = None;
            self.door_direction = None;
            self.status = "You are observing. Press F3 to request control.".into();
            return Effect::None;
        }
        // Ordered terminal facts can precede both the next picture and readiness.
        if ended(state) {
            self.status = "This run has ended.".into();
            return Effect::None;
        }
        let bear_trapped = bear_trapped(&state.state().observation);
        if let Action::Move { direction } = action {
            if let Some(request) = (!bear_trapped)
                .then(|| state.cancel_intention_with_step_request(direction))
                .flatten()
            {
                self.messages.begin_turn();
                self.running = None;
                self.last_move = true;
                return self.request(request);
            }
        }
        if !state.can_admit_intention() {
            if matches!(action, Action::Wait) && !ended(state) {
                self.messages.begin_turn();
                return self.request(Request::Continue);
            }
            self.status = if ended(state) {
                "This run has ended."
            } else {
                "You can't act right now."
            }
            .into();
            return Effect::None;
        }
        if let Action::Move { direction } = action {
            let (x, y, z) = match direction {
                Direction::North => (0, -1, 0),
                Direction::South => (0, 1, 0),
                Direction::East => (1, 0, 0),
                Direction::West => (-1, 0, 0),
                Direction::NorthEast => (1, -1, 0),
                Direction::SouthEast => (1, 1, 0),
                Direction::SouthWest => (-1, 1, 0),
                Direction::NorthWest => (-1, -1, 0),
                Direction::Up => (0, 0, 1),
                Direction::Down => (0, 0, -1),
            };
            let view = &state.state().observation;
            let columns = crate::map::CreatureColumns::new(view);
            let destination = match (
                view.position.x.checked_add(x),
                view.position.y.checked_add(y),
                view.position.z.checked_add(z),
            ) {
                (Some(x), Some(y), Some(z)) => Some(Position { x, y, z }),
                _ => None,
            };
            let target = destination.and_then(|at| {
                if z == 0 {
                    columns
                        .actor(at.x, at.y)
                        .filter(|actor| columns.near_body(actor.position))
                } else {
                    view.visible_actors
                        .iter()
                        .find(|actor| actor.id != view.self_target && actor.position == at)
                }
            });
            if let Some(target) =
                target
                    .filter(|_| !bear_trapped)
                    .filter(|actor| match self.bump_attacks {
                        BumpAttacks::Any => true,
                        BumpAttacks::Off => false,
                        BumpAttacks::Hostile => view.combat.as_ref().is_some_and(|c| {
                            c.actors
                                .iter()
                                .any(|other| other.actor == actor.id && other.hostile)
                        }),
                    })
            {
                match tor_client_common::abilities::basic_action(view, target.id) {
                    Ok(attack) => action = attack,
                    Err(reason) => {
                        self.status = reason;
                        return Effect::None;
                    }
                }
            }
        }
        let effect = self.command(Command::Act {
            action: action.clone(),
        });
        self.last_move = matches!(action, Action::Move { .. });
        effect
    }

    fn command(&mut self, command: Command) -> Effect {
        let Some(state) = &self.state else {
            return Effect::None;
        };
        if matches!(command, Command::Act { .. } | Command::Travel { .. }) {
            if ended(state) || !state.can_admit_intention() {
                self.status = if ended(state) {
                    "This run has ended."
                } else {
                    "You can't act right now."
                }
                .into();
                return Effect::None;
            }
            self.messages.begin_turn();
        }
        let request = state.command_request(command);
        self.request(request)
    }

    fn request(&mut self, request: Request) -> Effect {
        self.busy = true;
        self.status = "Waiting for server...".into();
        Effect::Request(Box::new(request))
    }
}

/// Rows the history screen shows at once.
pub const HISTORY_ROWS: usize = 22;
pub const DISCOVERY_ROWS: usize = 20;
pub const STATS_ROWS: usize = 24;
pub const STATS_WIDTH: usize = 65;
pub const ADVANCEMENT_ROWS: usize = 16;

/// Rows the message log screen shows at once.
pub const MESSAGE_LOG_ROWS: usize = 24;

/// A request's progress in plain words; a finished action needs none.
pub fn phase_status(phase: IntentionPhase) -> &'static str {
    match phase {
        IntentionPhase::Queued => "",
        IntentionPhase::Suspended => "Action suspended. F8 resumes, F9 cancels.",
        IntentionPhase::Paused => "Attack paused.",
        IntentionPhase::Started => "Action under way.",
        IntentionPhase::Resolved => "",
        IntentionPhase::Failed => "That didn't work.",
        IntentionPhase::Cancelled => "Action cancelled.",
    }
}

/// A server refusal in the player's words; unknown reasons keep the server's text.
pub fn plain_error(code: ErrorCode, message: &str) -> String {
    match (code, message) {
        (ErrorCode::InvalidAction, "Intention is unavailable") => "You can't do that now.".into(),
        (ErrorCode::InvalidAction, "Travel destination or known route is unavailable") => {
            "You don't know a way there.".into()
        }
        (ErrorCode::ActorBusy, _) => "You can't act yet.".into(),
        (ErrorCode::ControlTaken, _) => {
            "Another player has control; you can watch until they release it.".into()
        }
        (ErrorCode::NotController, _) => "You don't have control. Press F3 to request it.".into(),
        (ErrorCode::StaleRevision | ErrorCode::StaleContext | ErrorCode::WrongBranch, _) => {
            "The game moved on before that arrived; look again and retry.".into()
        }
        (ErrorCode::StorageFailure, _) => format!("The game couldn't be saved: {message}"),
        _ => {
            let mut text = message.trim().to_owned();
            if !text.ends_with(['.', '!', '?']) {
                text.push('.');
            }
            text
        }
    }
}

/// "a dawn seal", "an ember", "3 x arrow".
pub fn item_phrase(name: &str, quantity: u64) -> String {
    if quantity == 1 {
        let article = if name.to_lowercase().starts_with(['a', 'e', 'i', 'o', 'u']) {
            "an"
        } else {
            "a"
        };
        format!("{article} {name}")
    } else {
        format!("{quantity} x {name}")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BumpAttacks {
    #[default]
    Hostile,
    Any,
    Off,
}

/// The glyph the map draws for a column of the observer's scene, or a space
/// when nothing there is disclosed.
pub fn glyph_at(o: &Observation, palette: &tor_client_common::Palette, x: i32, y: i32) -> char {
    crate::map::tiles(o, palette, &|_| true)
        .into_iter()
        .find(|t| (t.position.x, t.position.y) == (x, y))
        .map_or(' ', |t| t.glyph)
}

/// One history entry as a line, in the shared client wording.
pub fn history_text(entry: &HistoryEntry) -> String {
    tor_client_common::history::text(entry)
}

pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let chars: Vec<char> = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    chars
        .chunks(width.max(1))
        .map(|line| line.iter().collect())
        .collect()
}

/// History entries for the history screen, newest last, one per turn time.
pub fn history_lines(entries: &[HistoryEntry]) -> Vec<String> {
    entries
        .iter()
        .flat_map(|e| crate::messages::word_wrap(&format!("T:{}  {}", e.tick, history_text(e)), 66))
        .collect()
}

/// The `[x, y, z]` cell offset of one step in `direction`.
fn step_offset(direction: Direction) -> [i32; 3] {
    match direction {
        Direction::North => [0, -1, 0],
        Direction::South => [0, 1, 0],
        Direction::East => [1, 0, 0],
        Direction::West => [-1, 0, 0],
        Direction::NorthEast => [1, -1, 0],
        Direction::SouthEast => [1, 1, 0],
        Direction::SouthWest => [-1, 1, 0],
        Direction::NorthWest => [-1, -1, 0],
        Direction::Up => [0, 0, 1],
        Direction::Down => [0, 0, -1],
    }
}

/// The `(x, y)` offset of a horizontal step; `None` for up and down.
fn horizontal_step(direction: Direction) -> Option<(i32, i32)> {
    let [dx, dy, dz] = step_offset(direction);
    (dz == 0).then_some((dx, dy))
}

/// Move a map cursor one horizontal step for a movement key.
fn nudge(position: Position, key: Key) -> Option<Position> {
    let (dx, dy) = key.step_direction().and_then(horizontal_step)?;
    Some(Position {
        x: position.x.saturating_add(dx),
        y: position.y.saturating_add(dy),
        ..position
    })
}

pub use tor_client_common::direction_name;

pub fn item_glyph(class: ItemClass) -> char {
    match class {
        ItemClass::Weapon => ')',
        ItemClass::Armor => '[',
        ItemClass::Potion => '!',
        ItemClass::Food | ItemClass::Corpse => '%',
        ItemClass::Misc | ItemClass::Tool => '(',
        ItemClass::Amulet => '"',
        ItemClass::Ring => '=',
        ItemClass::Scroll => '?',
        ItemClass::Spellbook => '+',
        ItemClass::Wand => '/',
        ItemClass::Coin => '$',
        ItemClass::Gem => '*',
    }
}

/// Why a journey stopped short, as a message; arriving needs none.
pub fn travel_stop(phase: TravelPhase) -> Option<&'static str> {
    Some(match phase {
        TravelPhase::Active | TravelPhase::Arrived => return None,
        TravelPhase::Blocked => "Your way is blocked.",
        TravelPhase::Hazard => "You stop. There may be danger in sight.",
        TravelPhase::DecisionRequired => "You are thrown off course.",
        TravelPhase::ControlLost => "You stop travelling; control was released.",
        TravelPhase::WorldChanged => "You stop. Something changed along the way.",
        TravelPhase::Failed => "You couldn't travel there.",
    })
}

/// One carried item as the inventory shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventoryEntry {
    pub letter: char,
    pub glyph: char,
    pub color: u32,
    pub name: String,
    pub quantity: u64,
    /// The slot it's worn or held in, when equipped.
    pub equipped: Option<String>,
    /// Known equipment numbers, when the server discloses them.
    pub stats: Option<String>,
    /// Exact charge count only when disclosed for this carried instance.
    pub charges: Option<u16>,
}

impl InventoryEntry {
    pub fn short(&self) -> String {
        format!(
            "{} - {}",
            self.letter,
            item_phrase(&self.name, self.quantity)
        )
    }
    pub fn long(&self) -> String {
        let mut text = format!(
            "{} - {} {}",
            self.letter,
            self.glyph,
            item_phrase(&self.name, self.quantity)
        );
        if let Some(slot) = &self.equipped {
            text.push_str(&format!(" ({})", slot));
        }
        if let Some(stats) = &self.stats {
            text.push_str(&format!("  [{stats}]"));
        }
        if let Some(charges) = self.charges {
            text.push_str(&format!("  [charges: {charges}]"));
        }
        text
    }
}

fn equipment_reduction(step: &CombatTraceStep) -> Option<String> {
    if !matches!(step.id.as_str(), "reduction" | "signed_reduction") {
        return None;
    }
    let field = |id| {
        step.fields
            .iter()
            .find(|field| field.id == id)
            .map(|field| &field.value)
    };
    let DiagnosticValue::Signed { value } = field("amount")? else {
        return None;
    };
    let DiagnosticValue::Content { value: kind } =
        field("category").or_else(|| field("descriptor"))?
    else {
        return None;
    };
    let condition = match field("scope")? {
        DiagnosticValue::Text { value } if value == "always" => "",
        DiagnosticValue::Text { value } if value == "versus_source" => " from the source",
        DiagnosticValue::Text { value } if value == "while_no_sensed_hostile" => {
            " when no enemies are sensed"
        }
        _ => return None,
    };
    let change = if *value < 0 { '+' } else { '-' };
    Some(format!(
        "{} damage {change}{}{condition}",
        kind.name,
        value.unsigned_abs()
    ))
}

fn weapon_in_socket(view: &Observation, socket: u16) -> Option<ItemTarget> {
    view.interactions
        .as_ref()?
        .inventory
        .iter()
        .find(|row| row.equipped_slot == Some(socket))
        .map(|row| row.item)
}

/// Everything carried, lettered and in disclosed order.
pub fn inventory_entries(app: &App, o: &Observation) -> Vec<InventoryEntry> {
    o.inventory
        .iter()
        .map(|item| {
            let affordance = o
                .interactions
                .as_ref()
                .and_then(|i| i.inventory.iter().find(|a| a.item == item.id));
            let equipped = affordance.and_then(|a| a.equipped_slot).map(|index| {
                o.interactions
                    .as_ref()
                    .and_then(|i| i.slots.get(index as usize))
                    .map_or_else(|| "in use".into(), |slot| slot.equipped_phrase.clone())
            });
            let stats = affordance
                .and_then(|a| a.known_equipment.as_ref())
                .map(|known| {
                    let mut parts = Vec::new();
                    if let Some(attack) = &known.attack {
                        parts.push(format!(
                            "attack {:+}; damage enchantment {:+}",
                            attack.bonus, attack.damage_enchantment
                        ));
                    }
                    for modifier in &known.modifiers {
                        if let Some(reduction) = equipment_reduction(modifier) {
                            parts.push(reduction);
                        } else {
                            parts.extend(tor_client_common::combat_diagnostics::trace_lines(
                                modifier,
                            ));
                        }
                    }
                    parts.join(", ")
                })
                .filter(|text| !text.is_empty());
            let mut parts: Vec<String> = stats.into_iter().collect();
            if app.secondary_weapon == Some(item.id) {
                parts.push("secondary weapon (x)".into());
            }
            if let Some(light) = tor_client_common::items::portable_light_details(o, item.id) {
                parts.push(light);
            }
            if let Some(cursed) = affordance.and_then(|value| value.known_cursed) {
                parts.push(if cursed { "cursed" } else { "uncursed" }.into());
            }
            if let Some(protected) = affordance.and_then(|value| value.known_protected) {
                parts.push(
                    if protected {
                        "rust protected"
                    } else {
                        "not rust protected"
                    }
                    .into(),
                );
            }
            let stats = (!parts.is_empty()).then(|| parts.join(", "));
            let appearance = crate::map::item(&app.palette, item);
            InventoryEntry {
                letter: app.letters.get(&item.id).copied().unwrap_or('?'),
                glyph: appearance.glyph,
                color: appearance.color,
                name: item.name.clone(),
                quantity: item.quantity,
                equipped,
                stats,
                charges: affordance.and_then(|affordance| affordance.charges),
            }
        })
        .collect()
}

// A current cell's explicit null is factual; it must not fall back to old art.
fn stair_presentation<'a>(
    current: &'a [CellView],
    memory: impl Iterator<Item = tor_client_common::MapCell<'a>>,
    position: Position,
) -> Option<&'a ObjectPresentation> {
    if let Some(cell) = current.iter().find(|cell| cell.position == position) {
        return cell.object_presentation.as_ref();
    }
    memory
        .into_iter()
        .find(|cell| cell.position == position)
        .and_then(|cell| cell.cell.appearance.object_presentation.as_ref())
}

/// What the look command says about a column.
pub fn describe(
    state: &ClientState,
    palette: &tor_client_common::Palette,
    x: i32,
    y: i32,
) -> String {
    let o = &state.state().observation;
    if (x, y) == (o.position.x, o.position.y) {
        return o
            .visible_cells
            .iter()
            .filter(|cell| cell.position == o.position)
            .find_map(|cell| cell.trap.as_ref())
            .map_or_else(
                || "That's you.".into(),
                |trap| {
                    format!(
                        "That's you. A {} lies underfoot.",
                        trap.family.map_or("trap", TrapFamily::name)
                    )
                },
            );
    }
    if let Some(actor) = crate::map::CreatureColumns::new(o).actor(x, y) {
        let phrase = if actor.name.trim().is_empty() {
            tor_client_common::presentation::resolve(palette, actor.asset.as_deref())
                .map_or("a figure", |entry| entry.noun.indefinite)
                .to_owned()
        } else {
            item_phrase(&actor.name, 1)
        };
        let mut text = format!(
            "{} - {}",
            crate::map::creature(palette, actor.asset.as_deref()).glyph,
            phrase
        );
        if let Some(combat) = o
            .combat
            .as_ref()
            .and_then(|c| c.actors.iter().find(|c| c.actor == actor.id))
        {
            text.push_str(&format!(
                " ({}, {})",
                if combat.hostile {
                    "hostile"
                } else {
                    "peaceful"
                },
                tor_client_common::narration::injury(combat.injury)
            ));
        }
        let attire = tor_client_common::narration::Attire::of(&actor.worn);
        let held: Vec<String> = attire
            .holds
            .iter()
            .map(|name| item_phrase(name, 1))
            .collect();
        for (verb, names) in [("wearing", &attire.wears), ("holding", &held)] {
            if !names.is_empty() {
                text.push_str(&format!(", {verb} {}", names.join(" and ")));
            }
        }
        if !actor.description.trim().is_empty() {
            text.push_str(&format!(". {}", actor.description.trim()));
        }
        return text + ".";
    }
    let tiles = crate::render::map_tiles(state, palette);
    let Some(tile) = tiles
        .iter()
        .find(|t| (t.position.x, t.position.y) == (x, y))
    else {
        return "You don't know what's there.".into();
    };
    if matches!(
        tile.kind,
        crate::map::Kind::StairsUp | crate::map::Kind::StairsDown
    ) {
        if let Some(presentation) =
            stair_presentation(&o.visible_cells, state.map_memory(), tile.position)
        {
            return format!(
                "{} - {}.",
                tile.glyph,
                item_phrase(tor_client_common::items::object_name(presentation), 1)
            );
        }
    }
    let what = match tile.kind {
        crate::map::Kind::Item => {
            let live = o
                .ground_items
                .iter()
                .find(|i| {
                    (i.position.x, i.position.y) == (x, y)
                        && (-2..=0).contains(&(i.position.z - o.position.z))
                })
                .map(|i| (tor_client_common::items::ground_name(i), i.item.quantity));
            let remembered = state
                .map_memory()
                .filter(|c| {
                    (c.position.x, c.position.y) == (x, y)
                        && (-2..=0).contains(&(c.position.z - o.position.z))
                })
                .find_map(|c| c.items().first())
                .map(|i| (tor_client_common::items::map_item_name(i), i.quantity));
            match live.or(remembered) {
                Some((name, quantity)) => item_phrase(name, quantity),
                None => "an object".into(),
            }
        }
        crate::map::Kind::Trap => {
            let current = o
                .visible_cells
                .iter()
                .filter(|cell| (cell.position.x, cell.position.y) == (x, y))
                .find_map(|cell| cell.trap.as_ref().map(|trap| trap.family));
            let remembered = state
                .map_memory()
                .filter(|cell| (cell.position.x, cell.position.y) == (x, y))
                .find_map(|cell| cell.trap.as_ref().map(|trap| trap.family));
            format!(
                "a {}",
                current
                    .or(remembered)
                    .flatten()
                    .map_or("trap", TrapFamily::name)
            )
        }
        crate::map::Kind::Wall => "a wall".into(),
        crate::map::Kind::LowWall => "a low wall".into(),
        crate::map::Kind::Drop => "a drop".into(),
        crate::map::Kind::Door => {
            if tile.glyph == '/' {
                "an open door".into()
            } else {
                "a closed door".into()
            }
        }
        crate::map::Kind::StairsUp => "a staircase up".into(),
        crate::map::Kind::StairsDown => "a staircase down".into(),
        crate::map::Kind::Floor => "the floor".into(),
        crate::map::Kind::Player => "you".into(),
        crate::map::Kind::Creature => "a creature".into(),
    };
    format!(
        "{} - {}{}.",
        tile.glyph,
        what,
        if tile.remembered { " (remembered)" } else { "" }
    )
}

/// Compact disclosed attributes and location, then health, resource balances,
/// reserved amounts, identified gold, time and conditions.
pub fn status_lines(app: &App, state: &ClientState) -> [String; 2] {
    let running = app.running.is_some().then_some("Running");
    tor_client_common::status::lines(state, running.as_slice())
}

/// Readable, bounded status rows. Whole fields wrap before optional details are
/// omitted; `+` marks omissions, never an abbreviated numeric value.
pub fn status_lines_at_width(app: &App, state: &ClientState, columns: usize) -> [String; 3] {
    let [first, second] = status_lines(app, state);
    let mut fields = Vec::new();
    for field in second.split("  ") {
        if field.starts_with("Sequence") {
            // Preserve the state even when the three exact counters are long.
            if let Some((label, detail)) = field.split_once(':') {
                fields.push(label.to_owned());
                let mut details = detail.split_whitespace();
                if let Some(progress) = details.next() {
                    fields.push(format!("Seq:{progress}"));
                }
                fields.extend(details.map(str::to_owned));
            }
        } else {
            fields.push(field.to_owned());
        }
    }
    fields.sort_by_key(|field| {
        if field.starts_with("HP:") || matches!(field.as_str(), "Starving" | "Fainting") {
            0
        } else if matches!(
            field.as_str(),
            "Dead" | "Victorious" | "hungry" | "weak" | "empty" | "Recovering"
        ) || field.starts_with("Held:")
            || field.starts_with("Sequence")
            || field.starts_with("Attack")
        {
            1
        } else if field.starts_with("Gold:") {
            2
        } else if field.starts_with("Stm:")
            || field.starts_with("Foc:")
            || field.starts_with("Mana:")
        {
            3
        } else if field.starts_with("T:") {
            5
        } else {
            4
        }
    });
    fn fit(fields: impl IntoIterator<Item = String>, columns: usize, rows: &mut [String]) {
        let mut packed: Vec<Vec<String>> = vec![Vec::new(); rows.len()];
        let mut row = 0;
        let mut omitted = false;
        for field in fields {
            if field.chars().count() > columns {
                omitted = true;
                continue;
            }
            while row < packed.len()
                && packed[row].join(" ").chars().count()
                    + usize::from(!packed[row].is_empty())
                    + field.chars().count()
                    > columns
            {
                row += 1;
            }
            if row == packed.len() {
                omitted = true;
                continue;
            }
            packed[row].push(field);
        }
        if omitted && columns > 0 {
            let last = packed.last_mut().expect("at least one status row");
            while last.join(" ").chars().count() + usize::from(!last.is_empty()) + 1 > columns {
                last.pop();
            }
            last.push("+".into());
        }
        for (row, fields) in rows.iter_mut().zip(packed) {
            *row = fields.join(" ");
        }
    }

    let mut rows = [String::new(), String::new(), String::new()];
    if let Some(label) = state.state().observation.level_name.as_deref() {
        let length = label.chars().count();
        if columns > 0 && length > columns {
            rows[0] = label.chars().take(columns - 1).chain(['~']).collect();
        } else if columns > 0 {
            rows[0].push_str(label);
            let rest = first.strip_prefix(label).unwrap_or(&first);
            if let Some(available) = columns.checked_sub(length + 1).filter(|n| *n > 0) {
                let mut details = [String::new()];
                fit(
                    rest.split_whitespace().map(str::to_owned),
                    available,
                    &mut details,
                );
                if !details[0].is_empty() {
                    rows[0].push(' ');
                    rows[0].push_str(&details[0]);
                }
            }
        }
    } else {
        fit(
            first.split_whitespace().map(str::to_owned),
            columns,
            &mut rows[..1],
        );
    }
    fit(fields, columns, &mut rows[1..]);
    rows
}

/// Who controls the character, and whether a request is out.
pub fn control_label(app: &App) -> String {
    let control = if !app.connected {
        "DISCONNECTED - relaunch to reconnect"
    } else if app.role == AccessRole::Spectator {
        "SPECTATOR"
    } else if app.state.as_ref().is_some_and(|s| s.has_control()) {
        "IN CONTROL"
    } else {
        "OBSERVING - F3 to take control"
    };
    if app.busy && app.connected {
        format!("{control}   waiting for the server...")
    } else {
        control.into()
    }
}

/// The end-of-run screen's lines.
pub fn end_summary(state: &ClientState) -> Vec<String> {
    let o = &state.state().observation;
    let mut lines = Vec::new();
    if let Some(combat) = &o.combat {
        lines.push(format!("Hit points {} of {}", combat.hp, combat.max_hp));
    }
    lines.push(format!("Time {}   Places known {}", o.tick, o.places.len()));
    let carried: Vec<_> = o
        .inventory
        .iter()
        .map(|item| item_phrase(&item.name, item.quantity))
        .collect();
    lines.push(if carried.is_empty() {
        "Carrying nothing".into()
    } else {
        format!("Carrying {}", carried.join(", "))
    });
    lines
}

/// The help screen opens on everyday controls, with reference pages one key away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelpPage {
    Play,
    CombatAndItems,
    MapSymbols,
    HistoryAndReports,
}

impl HelpPage {
    pub fn title(self) -> &'static str {
        match self {
            Self::Play => "Play",
            Self::CombatAndItems => "Combat and items",
            Self::MapSymbols => "Map symbols",
            Self::HistoryAndReports => "History and reports",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Play => Self::CombatAndItems,
            Self::CombatAndItems => Self::MapSymbols,
            Self::MapSymbols => Self::HistoryAndReports,
            Self::HistoryAndReports => Self::Play,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Play => Self::HistoryAndReports,
            Self::CombatAndItems => Self::Play,
            Self::MapSymbols => Self::CombatAndItems,
            Self::HistoryAndReports => Self::MapSymbols,
        }
    }

    pub fn rows(self) -> &'static [&'static str] {
        match self {
            Self::Play => &[
                "MOVE AND EXPLORE",
                "Arrows       Move one step",
                "y k u        Letter keys also move, including diagonals",
                "h @ l        Hold Shift with a direction to run",
                "b j n        Press m, then a direction, to step without attacking",
                "< or >       Go up or down stairs",
                "_ or click   Travel to a known square",
                "",
                "LOOK AND INTERACT",
                "; or /       Look around without taking a turn",
                "s            Search nearby for hidden features",
                ",            Pick up items underfoot",
                "i            Open your inventory",
                "o or c       Open or close a door, then choose a direction",
                "Space or .   Wait",
                "",
                "#            Find a command by name",
                "S            Save and close",
            ],
            Self::CombatAndItems => &[
                "FIGHT",
                "Direction    Move toward an adjacent enemy to attack",
                "t            Throw: choose a dart, then choose a direction",
                "f            Fire: aim an equipped launcher",
                "#quiver      Choose preferred ammunition",
                "Tab          Switch between launchers and hand throws",
                "#attack      Choose an attack target",
                "F8 or F9     Resume or abandon paused work",
                "F11          Try to escape a hold",
                "",
                "USE YOUR INVENTORY",
                "q / e / r    Drink a potion, eat food, or read a scroll",
                "z / Z        Zap a device or choose an ability",
                "F6 / F12     Alternate keys for reading or zapping",
                "x            Swap with the secondary weapon (#secondary)",
                "w / W / P    Wield a weapon, wear armor, or put on an accessory",
                "T / R        Take off armor or remove an accessory",
                "a / d        Apply a portable light or drop an item",
                "Ctrl-X       View creature stats",
                "F10          View advancement",
                "",
                "Letter case matters. Esc backs out of a choice.",
            ],
            Self::MapSymbols => &[
                "@            You",
                "Letters      Creatures",
                "#            Wall (tan for low walls)",
                ".            Floor",
                "+ or /       Closed or open door",
                "< or >       Stairs up or down",
                "^            Drop or pit",
                "Gray         Remembered terrain outside your current sight",
                "",
                "; or /       Look to see what a square contains",
                "#charts      View detection charts",
                "#discoveries See identified items",
            ],
            Self::HistoryAndReports => &[
                "Ctrl-P       Read earlier messages",
                "F2           Browse game history",
                "F5           Browse remembered places",
                "F4           Attach a note to the current game state",
                "#report      Write a playtest report",
                "F1           Save a screenshot in this game",
                "",
                "F3           Take control of the current character",
                "#release     Release control",
                "F7           Wizard tools, including the creature list",
                "             Enter help or where without the wizard prefix",
            ],
        }
    }
}

fn active_bear_trap(observation: &Observation) -> Option<&tor_protocol::PersonalCondition> {
    observation
        .combat
        .as_ref()?
        .own_stats
        .as_ref()?
        .conditions
        .iter()
        .find(|condition| {
            condition.condition.id == "rogue.bear_trap" && condition.phase == ConditionPhase::Active
        })
}

fn bear_trapped(observation: &Observation) -> bool {
    active_bear_trap(observation).is_some()
}

fn ended(state: &ClientState) -> bool {
    state
        .outcome_status()
        .is_some_and(|status| status.terminal || status.dead)
}

impl Key {
    fn step_direction(self) -> Option<Direction> {
        Some(match self {
            Self::Up => Direction::North,
            Self::Down => Direction::South,
            Self::Left => Direction::West,
            Self::Right => Direction::East,
            Self::NorthEast => Direction::NorthEast,
            Self::SouthEast => Direction::SouthEast,
            Self::SouthWest => Direction::SouthWest,
            Self::NorthWest => Direction::NorthWest,
            Self::Ascend => Direction::Up,
            Self::Descend => Direction::Down,
            _ => return None,
        })
    }

    /// The direction a run key keeps moving in.
    fn run_direction(self) -> Option<Direction> {
        RUN_KEYS
            .iter()
            .find(|(key, _)| *key == self)
            .map(|&(_, direction)| direction)
    }

    /// The run key for a horizontal direction; `None` for up and down.
    fn run_toward(direction: Direction) -> Option<Self> {
        RUN_KEYS
            .iter()
            .find(|(_, toward)| *toward == direction)
            .map(|&(key, _)| key)
    }

    /// The item operation an item key opens a picker for, and the key that
    /// filters equipment choices (`None` shows every eligible item).
    fn item_picker(self) -> Option<(ItemOperation, Option<Self>)> {
        let operation = match self {
            Self::Wield => ItemOperation::Wield,
            Self::Wear | Self::PutOn => ItemOperation::Equip,
            Self::TakeOff | Self::RemoveAccessory => ItemOperation::Unequip,
            Self::Equip => return Some((ItemOperation::Equip, None)),
            Self::Unequip => return Some((ItemOperation::Unequip, None)),
            Self::Drink => return Some((ItemOperation::Drink, None)),
            Self::Eat => return Some((ItemOperation::Eat, None)),
            Self::Read => return Some((ItemOperation::Read, None)),
            Self::Zap => return Some((ItemOperation::Zap, None)),
            Self::Ignite => return Some((ItemOperation::Ignite, None)),
            Self::Extinguish => return Some((ItemOperation::Extinguish, None)),
            _ => return None,
        };
        Some((operation, Some(self)))
    }
}

/// Run keys and the horizontal direction each one runs toward.
const RUN_KEYS: [(Key, Direction); 8] = [
    (Key::RunUp, Direction::North),
    (Key::RunDown, Direction::South),
    (Key::RunLeft, Direction::West),
    (Key::RunRight, Direction::East),
    (Key::RunNorthEast, Direction::NorthEast),
    (Key::RunSouthEast, Direction::SouthEast),
    (Key::RunSouthWest, Direction::SouthWest),
    (Key::RunNorthWest, Direction::NorthWest),
];

#[cfg(test)]
mod cosmetic_tests {
    use super::*;

    #[test]
    fn current_factual_stairs_do_not_resurrect_remembered_cosmetics() {
        let position = Position { x: 1, y: 0, z: 0 };
        let current = CellView {
            object_presentation: None,
            lit: true,
            door: None,
            trap: None,
            material: String::new(),
            key: "stairs".into(),
            stairs_up: true,
            stairs_down: false,
            position,
            wall: false,
            place_hint: false,
            asset: None,
        };
        let false_art = ObjectPresentation {
            name: "silver flask".into(),
            asset: None,
            kind: ObjectPresentationKind::Item {
                class: ItemClass::Potion,
            },
        };
        let old = tor_client_common::RememberedCell {
            appearance: RememberedMapCell {
                object_presentation: Some(false_art.clone()),
                key: "stairs".into(),
                position,
                wall: false,
                material: String::new(),
                asset: None,
                place_hint: false,
                stairs_up: true,
                stairs_down: false,
                door: None,
                trap: None,
                ground_items: vec![],
            },
            last_seen_tick: Some(1),
            last_seen_revision: Some(1),
        };
        let memory = || {
            std::iter::once(tor_client_common::MapCell {
                position,
                cell: &old,
            })
        };
        assert_eq!(
            stair_presentation(&[], memory(), position),
            Some(&false_art)
        );
        assert_eq!(stair_presentation(&[current], memory(), position), None);
    }
}
