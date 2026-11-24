//! Language-independent scenario hook contracts.
//!
//! Scenario scripts are the scenario's own authority. They react to *facts*
//! the simulation notes after effects are applied, and they answer with
//! [`ScriptEffect`]s. Those effects are checked against world invariants, not
//! against action legality. The script runtime lives outside this crate behind
//! [`HookHost`]; this crate never depends on a language implementation.
//!
//! Checking cost is the central design constraint:
//!
//! * Whether any binding for a [`HookKind`] exists is one AND against a
//!   derived `u64` mask held directly by `Game`. A game without bindings pays
//!   only that branch at each fact site and never builds a fact.
//! * With the bit set, the fact's candidate [`FilterKey`]s are looked up in a
//!   per-kind ordered index. Facts nobody listens to are dropped here and never
//!   reach a host.
//! * Matched facts are batched per handler and dispatched only at tick
//!   boundary settlement, next to the run policy check. No handler runs per
//!   tick, and idle timers cost one ordered-set minimum in the scheduler.

mod dispatch;
mod state;
mod view;

#[cfg(test)]
mod tests;

pub use dispatch::{HookCall, HookHost, HookOutput, HookPort, WorkBudget};
pub use state::{Binding, ScriptSetup, ScriptState, Timer, VariableSchema};
pub use view::{script_random, ScriptView};

use crate::{ActorId, ItemId, ItemIdentity};
use serde::{Deserialize, Serialize};
use tor_world::{Location, RegionId};

/// Provisional cascade limit: effects of a handler may cause facts that are
/// dispatched again, at most this many levels below the root invocation.
pub const MAX_CASCADE_DEPTH: u8 = 4;
/// Upper bounds on persisted script state. Exceeding one rejects the root
/// invocation that tried to grow it.
pub const MAX_BINDINGS: usize = 4096;
pub const MAX_TIMERS: usize = 4096;
pub const MAX_VARIABLES: usize = 4096;
pub const MAX_PENDING_FACTS: usize = 1024;
pub const MAX_EFFECTS_PER_CALL: usize = 256;
pub const MAX_TEXT_BYTES: usize = 512;
pub const MAX_MESSAGE_SEGMENTS: usize = 32;
pub const MAX_NAME_BYTES: usize = 64;
/// Actors that can hold a goal at once.
pub const MAX_GOALS: usize = 1024;

/// One documented boundary a binding can react to. The discriminant is the
/// mask bit, so the enum must stay below 64 variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum HookKind {
    RunStarted,
    Timer,
    Moved,
    DoorChanged,
    ItemTaken,
    ItemDropped,
    Waited,
    AttackResolved,
    Died,
    RegionFirstVisit,
    /// An actor took a scenario exit, such as Rogue's level-1 up-stairs.
    Escaped,
}

impl HookKind {
    pub const ALL: [HookKind; 11] = [
        Self::RunStarted,
        Self::Timer,
        Self::Moved,
        Self::DoorChanged,
        Self::ItemTaken,
        Self::ItemDropped,
        Self::Waited,
        Self::AttackResolved,
        Self::Died,
        Self::RegionFirstVisit,
        Self::Escaped,
    ];
    pub const fn bit(self) -> u64 {
        1 << self as u8
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::RunStarted => "run_started",
            Self::Timer => "timer",
            Self::Moved => "moved",
            Self::DoorChanged => "door_changed",
            Self::ItemTaken => "item_taken",
            Self::ItemDropped => "item_dropped",
            Self::Waited => "waited",
            Self::AttackResolved => "attack_resolved",
            Self::Died => "died",
            Self::RegionFirstVisit => "region_first_visit",
            Self::Escaped => "escaped",
        }
    }
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.id() == id)
    }
    /// Filter key families this kind can be narrowed by, besides `Any`.
    pub fn accepts(self, key: &FilterKey) -> bool {
        match key {
            // Timers call their own handler directly; they are not bindable.
            _ if self == Self::Timer => false,
            FilterKey::Any => true,
            FilterKey::Actor(_) => self != Self::RunStarted,
            FilterKey::Cell(_) | FilterKey::Region(_) => matches!(
                self,
                Self::Moved
                    | Self::DoorChanged
                    | Self::ItemTaken
                    | Self::ItemDropped
                    | Self::Waited
                    | Self::Died
                    | Self::RegionFirstVisit
                    | Self::Escaped
            ),
            FilterKey::Item(_) | FilterKey::ItemIdentity(_) => {
                matches!(self, Self::ItemTaken | Self::ItemDropped)
            }
        }
    }
}

/// A host-validated index into the package's bindable handlers. Saves store
/// this stable identity, never a closure or interpreter object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HandlerId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BindingId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TimerId(pub u64);

/// The single key a binding is indexed by. A fact offers a small fixed set of
/// keys, so matching is a handful of ordered-map lookups regardless of how
/// many bindings or how large the world is.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum FilterKey {
    Any,
    Actor(ActorId),
    Item(ItemId),
    ItemIdentity(ItemIdentity),
    Cell(Location),
    Region(RegionId),
}

/// Which actors a binding hears about. Facts without an actor always pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorClass {
    /// Player-role characters only; the tutorial default.
    #[default]
    Character,
    Creature,
    Any,
}

/// Something whose removal retires a binding or timer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Owner {
    Actor(ActorId),
    Item(ItemId),
    Region(RegionId),
}

/// What happened, as plain values. Facts never carry references into state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HookFact {
    pub kind: HookKind,
    pub tick: u64,
    pub actor: Option<ActorId>,
    pub subject: FactSubject,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum FactSubject {
    None,
    Moved {
        from: Location,
        to: Location,
    },
    Door {
        at: Location,
        open: bool,
    },
    Item {
        item: ItemId,
        identity: ItemIdentity,
        quantity: u64,
        at: Location,
    },
    Waited {
        at: Location,
    },
    Attack {
        target: ActorId,
    },
    Death {
        at: Option<Location>,
    },
    Region {
        region: RegionId,
    },
    Timer {
        timer: TimerId,
        arg: i64,
    },
    Escaped {
        at: Location,
    },
}

/// The filter keys one fact offers, borrowed from the game so noting a fact
/// allocates nothing. A binding sits in exactly one key class; matching looks
/// up only classes some binding of that kind uses.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FactKeys<'a> {
    pub actor: Option<ActorId>,
    pub item: Option<ItemId>,
    pub identity: Option<&'a ItemIdentity>,
    pub cell: Option<Location>,
    pub region: Option<RegionId>,
}

impl FactKeys<'_> {
    /// A cell and the region holding it.
    pub(crate) fn at(at: Location) -> Self {
        Self {
            cell: Some(at),
            region: Some(at.region),
            ..Self::default()
        }
    }
}

/// A typed persistent value. Text is bounded by [`MAX_TEXT_BYTES`].
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Text(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    Bool,
    Int,
    Text,
}

impl Value {
    pub fn value_type(&self) -> ValueType {
        match self {
            Self::Bool(_) => ValueType::Bool,
            Self::Int(_) => ValueType::Int,
            Self::Text(_) => ValueType::Text,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum VarScope {
    Scenario,
    Region(RegionId),
    Actor(ActorId),
    Item(ItemId),
}

impl VarScope {
    pub fn kind(self) -> ScopeKind {
        match self {
            Self::Scenario => ScopeKind::Scenario,
            Self::Region(_) => ScopeKind::Region,
            Self::Actor(_) => ScopeKind::Actor,
            Self::Item(_) => ScopeKind::Item,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind {
    Scenario,
    Region,
    Actor,
    Item,
}

/// Who a script message is for. Delivery is resolved by the host against its
/// own disclosure rules; the simulation never addresses sockets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Audience {
    /// Whoever controls this actor.
    Actor(ActorId),
    /// Every character.
    Characters,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageStyle {
    Tutorial,
    Hint,
    Narration,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum MessageSegment {
    Text(String),
    /// A semantic control identity such as `pick_up`; each client names its
    /// own binding. The package validator checks the vocabulary.
    Control(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptMessage {
    pub tick: u64,
    pub audience: Audience,
    pub style: MessageStyle,
    pub segments: Vec<MessageSegment>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunEnd {
    Victory,
    Defeat,
}

/// A change proposed by a handler. Identities a host hands back to its script
/// (bindings, timers, placed items) are predicted from the view's counters and
/// verified exactly when applied, so a script can bind to what it creates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ScriptEffect {
    Message {
        audience: Audience,
        style: MessageStyle,
        segments: Vec<MessageSegment>,
    },
    SetVar {
        scope: VarScope,
        name: String,
        value: Value,
    },
    ClearVar {
        scope: VarScope,
        name: String,
    },
    StartTimer {
        id: TimerId,
        handler: HandlerId,
        owner: Option<Owner>,
        after: u64,
        arg: i64,
    },
    CancelTimer {
        id: TimerId,
    },
    Bind {
        id: BindingId,
        binding: Binding,
    },
    Unbind {
        id: BindingId,
    },
    SetDoor {
        at: Location,
        open: bool,
    },
    PlaceItem {
        id: ItemId,
        identity: ItemIdentity,
        name: String,
        at: Location,
    },
    RemoveItem {
        item: ItemId,
    },
    Teleport {
        actor: ActorId,
        to: Location,
    },
    /// `terminal: false` records a victory and lets play continue.
    EndRun {
        end: RunEnd,
        actor: Option<ActorId>,
        terminal: bool,
    },
    /// End an arena; `None` means nobody won.
    StopArena {
        winner: Option<String>,
    },
    /// What the scenario asks of its audience, shown until changed. The cell
    /// is disclosed by identity and seen only when the observer sees it.
    SetGoal {
        audience: Audience,
        segments: Vec<MessageSegment>,
        cell: Option<Location>,
    },
    ClearGoal {
        audience: Audience,
    },
}

/// A character's current goal, declared by scripts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Goal {
    pub segments: Vec<MessageSegment>,
    pub cell: Option<Location>,
}

impl ScriptEffect {
    /// Whether applying this changes the world beyond script state and the
    /// report. Only these make a root invocation checkpoint the game.
    pub fn changes_world(&self) -> bool {
        match self {
            Self::Message { .. }
            | Self::SetVar { .. }
            | Self::ClearVar { .. }
            | Self::StartTimer { .. }
            | Self::CancelTimer { .. }
            | Self::Bind { .. }
            | Self::Unbind { .. }
            | Self::SetGoal { .. }
            | Self::ClearGoal { .. } => false,
            Self::SetDoor { .. }
            | Self::PlaceItem { .. }
            | Self::RemoveItem { .. }
            | Self::Teleport { .. }
            | Self::EndRun { .. }
            | Self::StopArena { .. } => true,
        }
    }
}

/// Why a root invocation was rejected. A rejected root changes nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum FailureReason {
    /// The deterministic work budget ran out.
    Budget,
    /// The runtime memory limit was reached.
    Memory,
    /// The handler raised an error. The text is the runtime's deterministic
    /// message, bounded by [`MAX_TEXT_BYTES`].
    Error(String),
    /// The effect at this index broke a world invariant.
    Invariant { effect: u32, detail: String },
    /// Effects kept causing facts past [`MAX_CASCADE_DEPTH`].
    Cascade,
    /// No host was installed although bindings exist.
    NoHost,
    /// More facts matched in one boundary than [`MAX_PENDING_FACTS`].
    Overflow,
}

/// One rejected root invocation, reported to the host for its journal and
/// author diagnostics. Never a client message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptFailure {
    pub tick: u64,
    pub handler: HandlerId,
    pub depth: u8,
    pub reason: FailureReason,
}

/// One handler call the simulation made, for the host's journal. Replay
/// recomputes these and requires exact equality.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchRecord {
    pub tick: u64,
    pub handler: HandlerId,
    pub depth: u8,
    pub facts: u32,
    pub work: u64,
    pub effects: Vec<ScriptEffect>,
}

/// Keep all disclosed built-in cues representable on the recipient's history row.
pub const MAX_GAMEPLAY_NARRATION_LINES: usize = 16;

/// Everything a boundary produced for the host: delivered messages, handler
/// calls and rejected roots. Drained by the host after each transaction.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScriptReport {
    pub messages: Vec<ScriptMessage>,
    /// Event-time built-in narration. Like script output, replay compares the
    /// exact emitted wording; unlike tutorials, this also belongs in history.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub gameplay_narration: Vec<ScriptMessage>,
    pub dispatches: Vec<DispatchRecord>,
    pub failures: Vec<ScriptFailure>,
}

/// Where a game holds its report until the host drains it. The report is
/// output, not game state, so game equality ignores it (as it ignores the
/// installed host); replay compares reports through the host's journal.
#[derive(Clone, Debug, Default)]
pub(crate) struct ReportSlot(pub(crate) ScriptReport);

impl PartialEq for ReportSlot {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for ReportSlot {}

impl std::ops::Deref for ReportSlot {
    type Target = ScriptReport;
    fn deref(&self) -> &ScriptReport {
        &self.0
    }
}
impl std::ops::DerefMut for ReportSlot {
    fn deref_mut(&mut self) -> &mut ScriptReport {
        &mut self.0
    }
}

impl ScriptReport {
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
            && self.gameplay_narration.is_empty()
            && self.dispatches.is_empty()
            && self.failures.is_empty()
    }
}

/// A presentable message: 1 to [`MAX_MESSAGE_SEGMENTS`] segments, no empty
/// or control-character text, some visible content, and at most
/// [`MAX_TEXT_BYTES`] of text in total. Hosts deliver only such messages.
pub fn valid_message(segments: &[MessageSegment]) -> bool {
    let mut total = 0usize;
    let mut visible = false;
    if segments.is_empty() || segments.len() > MAX_MESSAGE_SEGMENTS {
        return false;
    }
    for segment in segments {
        match segment {
            MessageSegment::Text(text) => {
                if text.is_empty() || text.chars().any(char::is_control) {
                    return false;
                }
                total += text.len();
                visible |= !text.trim().is_empty();
            }
            MessageSegment::Control(id) => {
                if !bounded_name(id) {
                    return false;
                }
                visible = true;
            }
        }
    }
    visible && total <= MAX_TEXT_BYTES
}

pub(crate) fn bounded_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_BYTES
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.')
}
