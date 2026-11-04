//! Deterministic test and diagnostic orchestration, outside the simulation.
//!
//! The public [`performance`] module shares workload inputs and observable
//! assertions across harnesses. It does not own process lifetimes, clocks,
//! server admission, or settlement; those remain responsibilities of each driver.
pub mod performance;
