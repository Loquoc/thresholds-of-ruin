//! Renderer-neutral composition of what happens between player decisions.
//!
//! A *settle* is one server input wait for one sent request ([`SettleBoundary`]).
//! A *beat* is what a client presents as one unit; it can span several settles
//! and include actions the client supplies itself.
mod admission;
mod beat;
mod settle;
mod timeline;

pub use admission::AdmissionBoundary;
pub use beat::{BeatEnd, BeatPolicy, Caution, Compositor, PlanDriver, Pushed};
pub use settle::{SettleBoundary, Settled, Silence, Until};
pub use timeline::{
    ActorSpan, Flight, Frame, Reaction, ReactionKind, SpanEnd, SpanKind, StrikeMark, Timeline,
};
