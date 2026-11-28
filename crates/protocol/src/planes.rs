//! What an observer perceives of coexisting planes.
//!
//! Games without planes send no [`PlaneView`]. Plane keys are opaque and
//! per-observer, derived like cell keys: they reveal no internal identity, and
//! one plane keeps its key for one observer across saves.

use serde::{Deserialize, Serialize};

use crate::Position;

/// Most planes one view can name besides the observer's own.
pub const MAX_ECHO_PLANES: usize = 32;
/// Most cells one echo can carry: a range-16 Manhattan ball.
pub const MAX_ECHO_CELLS: usize = 6017;
/// Most actor rows one echo can carry.
pub const MAX_ECHO_ACTORS: usize = 4096;

/// The observer's plane and what it sees of others.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaneView {
    /// The observer's current plane.
    pub plane: PlaneRef,
    /// Other planes seen through perception channels, nearest first.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "crate::bounded::bounded::<_, PlaneEcho, MAX_ECHO_PLANES>"
    )]
    pub echoes: Vec<PlaneEcho>,
}

/// A plane as one observer knows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaneRef {
    /// Opaque per-observer key.
    pub key: String,
    /// The revealed description when the observer knows the plane, otherwise
    /// the mysterious one.
    pub description: String,
    pub known: bool,
}

/// Another plane's occurrences seen through a channel. Positions are relative
/// to the observer's reference cell in its own frame, like visible cells, so a
/// client can layer an echo over the observer's map. Echo cells are never part
/// of the observer's own plane and never replace its cells.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaneEcho {
    pub plane: PlaneRef,
    #[serde(deserialize_with = "crate::bounded::bounded::<_, EchoCell, MAX_ECHO_CELLS>")]
    pub cells: Vec<EchoCell>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "crate::bounded::bounded::<_, EchoActor, MAX_ECHO_ACTORS>"
    )]
    pub actors: Vec<EchoActor>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EchoCell {
    /// Stable opaque identity of the other plane's cell, as a cell key.
    pub key: String,
    pub position: Position,
    pub wall: bool,
    /// Cosmetic material of a solid cell.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub material: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EchoActor {
    pub position: Position,
    /// The public name of what is seen, never an identity.
    pub name: String,
}

/// Whether a description fits the wire contract.
pub fn valid_plane_description(text: &str) -> bool {
    !text.is_empty() && text.len() <= 240 && !text.chars().any(char::is_control)
}
