//! Validated disclosure, transport and presentation shared by the frontends.
//!
//! # Entry points
//!
//! - [`Connection`] owns the socket, applies messages and manages recovery and
//!   background queries. Poll it before interpreting already-ready local input;
//!   [`server_before_local`] supplies that ordering.
//! - [`ClientState`] validates and retains live authority alongside an immutable
//!   displayed picture. [`ClientSessionState`] is not an exportable reset snapshot.
//! - [`PendingRequest`] correlates request completion after messages have been
//!   applied. An admitted request, a completed query and finished gameplay are
//!   different events; an unknown outcome must not trigger automatic replay.
//! - [`advancement`], [`charts`], [`inspection_pages`], [`projectiles`] and
//!   [`weapon_preview`] own bounded, context-pinned browsing and confirmation.
//! - [`items`], [`abilities`], [`weapons`] and [`holds`] select from disclosed
//!   affordances. [`Palette`], [`presentation`], [`narration`] and [`status`]
//!   turn disclosed facts into frontend-neutral presentation.
//! - [`annotations`] formats notes/reports; [`captures`] uploads bytes already
//!   captured explicitly by a frontend. Neither invents a new screenshot.
//!
//! Remembered appearances and cosmetic recipes never grant action authority.
//! Frontends must preserve exact targets and selection contexts rather than
//! reconstruct them from names, list indices or a newer picture.

#[cfg(test)]
#[path = "../../protocol/tests/support/fixture.rs"]
mod fixture;

pub mod abilities;
pub mod advancement;
pub mod annotations;
pub mod captures;
pub mod charts;
mod connection;
pub mod history;
pub mod holds;
pub mod inspection;
pub mod inspection_pages;
pub mod items;
mod map_hydration;
mod map_memory;
pub mod projectiles;
pub mod weapon_preview;
pub mod weapons;
pub use map_hydration::{MapHydrationStatus, MapMemoryUpdate};
pub use map_memory::MapCell;
pub mod narration;
mod palette;
mod pending_request;
pub mod presentation;
pub mod public_plan;
mod state;
pub mod stats;
pub mod status;
pub mod surfaces;
pub use connection::{Connection, ConnectionError};
pub use palette::{observation_assets, AssetTable, Palette};
pub use pending_request::{ConfirmedReply, PendingRequest, RequestCompletion};
pub use state::{ClientSessionState, ClientState, RememberedCell};

use tor_protocol::{ActorId, StreamCursor};

/// Validation failure while applying a disclosed stream or reset boundary.
/// Callers must recover through a fresh authoritative boundary, not patch a
/// failed update into the previously valid state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamError {
    WrongStreamContext,
    WrongObservationBase,
    WrongActor,
    SequenceMismatch,
    TimeReversed,
    WrongBranch,
    InconsistentState,
}

/// Tracks one attachment after its authoritative snapshot has been received.
///
/// This only validates ordering. `ClientState` applies payloads and `Connection`
/// supplies the transport. Reconnects establish a fresh snapshot.
///
/// ```
/// use tor_client_common::{ObservationStream, StreamError};
/// use tor_protocol::{ActorId, StreamCursor};
/// let actor = ActorId(1);
/// let mut stream = ObservationStream::from_snapshot(
///     actor, StreamCursor { sequence: 7, tick: 10 },
/// );
/// stream.accept(actor, StreamCursor { sequence: 8, tick: 10 }).unwrap();
/// let accepted = stream.cursor();
/// assert_eq!(stream.accept(actor, StreamCursor { sequence: 10, tick: 11 }),
///            Err(StreamError::SequenceMismatch));
/// assert_eq!(stream.cursor(), accepted);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservationStream {
    actor: ActorId,
    cursor: StreamCursor,
}

impl ObservationStream {
    /// Establish the ordering cursor of an already validated snapshot.
    /// This constructor does not validate the snapshot payload itself.
    pub fn from_snapshot(actor: ActorId, cursor: StreamCursor) -> Self {
        Self { actor, cursor }
    }

    pub fn cursor(&self) -> StreamCursor {
        self.cursor
    }

    /// Accept an update atomically, leaving the cursor unchanged on failure.
    /// A sequence gap requires resynchronization before applying more updates.
    ///
    /// # Errors
    ///
    /// Rejects a different actor, a sequence other than the immediate successor
    /// (including exhaustion of `u64`), or a decreasing tick. Equal ticks are valid.
    pub fn accept(&mut self, actor: ActorId, next: StreamCursor) -> Result<(), StreamError> {
        if actor != self.actor {
            return Err(StreamError::WrongActor);
        }
        if self.cursor.sequence.checked_add(1) != Some(next.sequence) {
            return Err(StreamError::SequenceMismatch);
        }
        if next.tick < self.cursor.tick {
            return Err(StreamError::TimeReversed);
        }
        self.cursor = next;
        Ok(())
    }
}

use std::future::Future;

/// The first of a server message and a local input to act on.
#[derive(Debug, PartialEq, Eq)]
pub enum FirstReady<S, L> {
    Server(S),
    Local(L),
}

/// A revision-checked command is built from local state at the moment input
/// is taken. A server message that has already arrived has to be applied
/// before that, or the command names a revision the client has already been
/// told is old. `tokio::select!` without `biased` can take the input when
/// both are ready and leave the message unread.
pub async fn server_before_local<S, L>(
    server: impl Future<Output = S>,
    local: impl Future<Output = L>,
) -> FirstReady<S, L> {
    tokio::select! {
        biased;
        message = server => FirstReady::Server(message),
        value = local => FirstReady::Local(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_delivered_message_is_taken_before_input_that_is_also_ready() {
        // Fair select picks either branch. Sixty-four trials makes that fail
        // instead of passing by chance.
        for _ in 0..64 {
            let ready = server_before_local(std::future::ready(1), std::future::ready(2)).await;
            assert_eq!(ready, FirstReady::Server(1));
        }
    }

    #[tokio::test]
    async fn input_proceeds_when_no_server_message_is_waiting() {
        let ready = server_before_local(std::future::pending::<i32>(), std::future::ready(2)).await;
        assert_eq!(ready, FirstReady::Local(2));
    }
}

pub mod combat_diagnostics;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub mod creature_catalog;
