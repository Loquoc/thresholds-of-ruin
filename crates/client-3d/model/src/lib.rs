//! Presentation and input policy for the independently maintained 3D client.
//! No Bevy, native window, filesystem or simulation dependency belongs here.
//!
//! # From disclosure to input
//!
//! 1. Project validated shared state and its palette with [`view`]. The resulting
//!    [`View`] contains observer-frame geometry and the context of the picture
//!    shown to the player, not a second authoritative simulation.
//! 2. Feed visible cell identities to [`SceneFrame`] to keep presentation stable
//!    across translated views. A discontinuity requires resetting interpolation
//!    and other state that assumes the previous scene is still the same place.
//! 3. Use [`InventoryBrowser`], [`Equipment`], [`MapSelection`] and
//!    [`AdvancementBrowser`] for local presentation. Their choices do not grant
//!    permission to act; the shared client and server remain authoritative.
//! 4. Send an [`Input`] carrying the displayed [`View::shown`] context to the
//!    connection owner. [`request_from`] checks that context against the owner's
//!    current state before constructing a protocol request.
//!
//! [`Cmd::Projectile`], [`Cmd::Advancement`] and [`Cmd::Chart`] need their shared
//! session reducers rather than direct conversion by [`request_from`]. Rendering
//! recipes selected by [`creature_model`] and [`item_model`] are cosmetic, never
//! a source of creature capabilities, item identity or action permissions.
mod advancement;
mod appearance;
mod commands;
mod equipment;
mod frame;
mod interaction;
mod inventory;
mod map;
mod observation;
mod routing;
mod wearables;
pub use wearables::WearableLook;

pub use advancement::{
    catalog_command, AdvancementAction, AdvancementBrowser, AdvancementOption, AdvancementView,
};
pub use appearance::{
    actor_model, creature_model, item_model, item_surface, object_model, wearing, CreatureModel,
    ItemModel, ItemSurface, Wearing,
};
pub use commands::{
    offset, request_from, AbilityTarget, Cmd, Input, ProjectileStep, MAX_PLACE_NAME,
};
pub use equipment::Equipment;
pub use frame::{FrameChange, SceneFrame};
pub use interaction::{
    describe, describe_focus, direction, known_cells, near_actor, Focus, Intent,
};
pub use inventory::{InventoryBrowser, InventoryFilter, MAX_INVENTORY_SEARCH};
pub use map::{map_destinations, MapDestination, MapSelection};
pub use observation::{
    happenings, observed_harm, view, Ability, Actor, Carried, GroundItem, Happening, Terrain, View,
};

pub use routing::{approach_cells, attack_at_hand, step_toward};
