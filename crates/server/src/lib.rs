//! Authoritative sessions, durable action/annotation history, and WebSocket transport.

mod actions;
mod adapt;
mod advancement_adapter;
mod advancement_page;
#[cfg(test)]
mod advancement_test_support;
mod chart_view;
pub mod creature_authoring;
mod creature_view;
mod developer;
mod diagnostics;
mod engine;
pub mod generation_recipe;
pub mod generator;
mod history_index;
mod map_memory;
pub use map_memory::PreparedMapProjection;
mod outbound;
pub mod procedural_world;
mod wizard_creatures;
pub use outbound::OutboundLimits;
pub mod journal;
pub mod performance_fixture;
mod preload;
#[doc(hidden)]
pub mod process_diagnostics;
mod public_plan_view;
pub mod region_streaming;
mod regions;
mod runner;
pub use regions::{RegionAcquisitionProfile, Streaming};
pub mod scenario_package;
mod semantic_visual;
mod session;
mod storage;
pub use storage::{inspect_save, SavePolicy, SaveStatus};
mod transport;
pub mod wire_adapter;
pub use engine::{
    ActorSetup, BootstrapProfile, CommandProfile, CommandResult, Engine, Failure,
    PresentationBoundary, RecoveryProfile, RegionCounts, Scenario,
};
pub use runner::{Simulation, SimulationHandle};
#[cfg(feature = "service-contract-host")]
#[doc(hidden)]
pub use session::contract_host::run as run_service_contract_host;
pub use session::{Account, Service};
pub use transport::serve;

pub use engine::arena_evaluation;

#[cfg(test)]
#[path = "../test_support/packages.rs"]
mod test_packages;

#[cfg(test)]
use crate as host;
#[cfg(test)]
#[path = "../test_support/public_plan.rs"]
mod public_plan_test_fixture;

// Exercise the exact example parser without building a separate test executable.
#[cfg(test)]
#[path = "../example_support/physics_options.rs"]
mod physics_options;

// Exercise the exact offline benchmark inspector and CLI in the ordinary unit target.
#[cfg(test)]
#[path = "../example_support/active_mob_options.rs"]
mod active_mob_options;
#[cfg(test)]
#[path = "../example_support/active_mob_trace.rs"]
mod active_mob_trace;

/// Compare all disclosed fields across separately issued pictures. Identity-only
/// differences are expected; same-picture atomicity tests must use assert_eq!.
#[cfg(test)]
fn assert_disclosed_state_eq(actual: &tor_protocol::StateView, expected: &tor_protocol::StateView) {
    let mut payload = actual.clone();
    payload.snapshot = expected.snapshot.clone();
    payload.revision = expected.revision;
    assert_eq!(&payload, expected);
}

/// Independent spectator oracle: retain every legacy public field and compare
/// exactly, while explicitly replacing the observer-owned item disclosures.
/// Do not use this for same-role equality or reuse the production sanitizer.
#[cfg(test)]
fn assert_spectator_disclosed_state_eq(
    actual: &tor_protocol::StateView,
    owner: &tor_protocol::StateView,
) {
    let mut expected = owner.clone();
    for item in expected.observation.inventory.iter_mut().chain(
        expected
            .observation
            .ground_items
            .iter_mut()
            .map(|row| &mut row.item),
    ) {
        item.identity = "item.unknown".parse().unwrap();
        item.knowledge = tor_protocol::ItemKnowledge::default();
    }
    if let Some(interaction) = &mut expected.observation.interactions {
        interaction.identification = None;
        interaction.curse_blocked.clear();
        for item in &mut interaction.inventory {
            item.charges = None;
            item.known_equipment = None;
            item.known_projectile_bonus = None;
            item.known_cursed = None;
            item.known_protected = None;
        }
    }
    assert_disclosed_state_eq(actual, &expected);
}
