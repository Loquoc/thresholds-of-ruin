//! Graphical ASCII input and presentation derived only from disclosed protocol state.
//!
//! [`App`] is the presentation-thread reducer. Native keyboard/mouse input and
//! the opt-in process driver share [`Input`]; [`App::input`] returns an [`Effect`]
//! for the shell/connection worker rather than performing transport itself.
//! Apply every ordered server update, even when several arrive before one frame.
//!
//! [`map`] projects disclosed heights into one logical map; [`render`] paints
//! that map and modal screens on a fixed-size [`render::Canvas`]. Hit testing must
//! use the same logical coordinates after native-window scaling. [`messages`]
//! retains player-visible turns independently of display refreshes.
pub mod map;
pub mod messages;
mod model;
pub mod render;
pub use model::*;
