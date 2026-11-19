//! Server-authored user messages: tutorial, hint and narration text that can
//! name player controls. A message names a control only by its closed
//! [`ControlHint`]; each client decides which key or command it shows, so the
//! server never claims a binding it does not own.
use serde::{Deserialize, Serialize};

/// Most segments one message may carry.
pub const MAX_USER_MESSAGE_SEGMENTS: usize = 32;
/// Most UTF-8 bytes in one text segment.
pub const MAX_USER_MESSAGE_SEGMENT_BYTES: usize = 512;
/// Most UTF-8 bytes across all text segments of one message.
pub const MAX_USER_MESSAGE_TEXT_BYTES: usize = 2048;

/// How a client should present a message. Clients without a distinct
/// presentation for a style show its text plainly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMessageStyle {
    /// Teaching text, usually naming a control.
    Tutorial,
    /// An optional suggestion.
    Hint,
    /// Scenario prose with no instructional intent.
    Narration,
}

impl UserMessageStyle {
    pub const ALL: &'static [Self] = &[Self::Tutorial, Self::Hint, Self::Narration];

    pub fn id(self) -> &'static str {
        match self {
            Self::Tutorial => "tutorial",
            Self::Hint => "hint",
            Self::Narration => "narration",
        }
    }
}

/// A player control a message can refer to. Closed: a message can't name a
/// control no client knows, and the package validator checks
/// `{control:<id>}` placeholders against [`ControlHint::from_id`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlHint {
    Move,
    Wait,
    Search,
    PickUp,
    Inventory,
    Equip,
    Drink,
    Read,
    Zap,
    Throw,
    Travel,
    Look,
    OpenDoor,
    Rest,
    Save,
    Help,
}

impl ControlHint {
    pub const ALL: &'static [Self] = &[
        Self::Move,
        Self::Wait,
        Self::Search,
        Self::PickUp,
        Self::Inventory,
        Self::Equip,
        Self::Drink,
        Self::Read,
        Self::Zap,
        Self::Throw,
        Self::Travel,
        Self::Look,
        Self::OpenDoor,
        Self::Rest,
        Self::Save,
        Self::Help,
    ];

    /// The stable snake_case identifier, identical to the wire spelling.
    pub fn id(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::Wait => "wait",
            Self::Search => "search",
            Self::PickUp => "pick_up",
            Self::Inventory => "inventory",
            Self::Equip => "equip",
            Self::Drink => "drink",
            Self::Read => "read",
            Self::Zap => "zap",
            Self::Throw => "throw",
            Self::Travel => "travel",
            Self::Look => "look",
            Self::OpenDoor => "open_door",
            Self::Rest => "rest",
            Self::Save => "save",
            Self::Help => "help",
        }
    }

    /// The control with exactly this identifier; no aliases or case folding.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|control| control.id() == id)
    }
}

/// One piece of a message: literal text, or a control the client names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMessageSegment {
    Text(String),
    Control(ControlHint),
}

pub(crate) fn read_segments<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<UserMessageSegment>, D::Error> {
    crate::bounded::bounded::<D, _, MAX_USER_MESSAGE_SEGMENTS>(d)
}

/// Whether segments form a presentable bounded message: at least one segment,
/// some visible text or control, no control characters, and within limits.
pub fn valid_user_message(segments: &[UserMessageSegment]) -> bool {
    let mut total = 0usize;
    let mut visible = false;
    if segments.is_empty() || segments.len() > MAX_USER_MESSAGE_SEGMENTS {
        return false;
    }
    for segment in segments {
        match segment {
            UserMessageSegment::Text(text) => {
                if text.is_empty()
                    || text.len() > MAX_USER_MESSAGE_SEGMENT_BYTES
                    || text.chars().any(char::is_control)
                {
                    return false;
                }
                total += text.len();
                visible |= !text.trim().is_empty();
            }
            UserMessageSegment::Control(_) => visible = true,
        }
    }
    visible && total <= MAX_USER_MESSAGE_TEXT_BYTES
}
