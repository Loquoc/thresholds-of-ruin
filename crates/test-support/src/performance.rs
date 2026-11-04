//! Shared performance-workload inputs and disclosed-state assertions.
//!
//! This module neither runs a server nor measures time. Drivers own setup,
//! action submission, settlement and timing; these helpers select fixture steps
//! and check their observable effects. The same assertions serve direct Engine
//! workloads and published protocol pictures, with different clock checks.
//!
//! [`Trace`] reads only the workload portion of [`SPEC`]. Geometry and other
//! fixture fields are consumed by separate owners; deserializing a trace is not
//! validation of the entire performance fixture.

use serde::Deserialize;
use tor_protocol::{Action, Direction, StateView};

/// Embedded JSON fixture shared with the server's performance scenario.
///
/// The text includes geometry as well as workload sequences. [`Trace::load`]
/// parses the workload fields without performing I/O or constructing a world.
pub const SPEC: &str = include_str!("../../server/fixtures/performance-v2.json");

/// Ordered workload sequences and their reproducibility metadata.
///
/// Deserialization checks field types and known outcome labels, but does not
/// reject an unfamiliar `version` or validate action/outcome pairings. Callers
/// reading custom traces must impose any additional compatibility policy.
#[derive(Deserialize)]
pub struct Trace {
    /// Fixture schema identity, not the game protocol or save version.
    pub version: u32,
    /// Seed supplied to workload setup; loading a trace does not seed an engine.
    pub seed: u64,
    steps: Vec<Step>,
    /// Secondary-actor sequence, unfiltered; the driver chooses when to run it.
    pub secondary: Vec<Step>,
    /// Reusable traversal sequence, unfiltered; the driver owns repetition.
    pub traversal: Vec<Step>,
}
/// One action and its expected observable effect in a controlled workload.
///
/// `expected` accepts `blocked`, `waited`, `moved`, or `door_changed`.
/// Only `blocked` selects a different assertion branch; successful-effect checks
/// follow [`Self::action`], rather than comparing its kind to the outcome label.
/// These fixture assertions are not a general gameplay-success oracle.
///
/// ```
/// use tor_test_support::performance::{Step, TraceAction};
///
/// let step: Step = serde_json::from_str(r#"{
///     "label": "idle", "action": {"type": "wait"}, "expected": "waited"
/// }"#)?;
/// assert_eq!(step.min_regions, 1);
/// assert!(matches!(step.action, TraceAction::Wait));
/// let typo = r#"{
///     "label": "idle", "action": {"type": "wait"}, "expected": "waied"
/// }"#;
/// assert!(serde_json::from_str::<Step>(typo).is_err());
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Clone, Deserialize)]
pub struct Step {
    /// Diagnostic/measurement category; labels need not uniquely identify steps.
    pub label: String,
    /// Actor-relative operation resolved against the driver's current picture.
    pub action: TraceAction,
    #[serde(deserialize_with = "expected_outcome")]
    /// Outcome label checked on deserialization and again during verification.
    pub expected: String,
    #[serde(default = "one")]
    /// Minimum region count for [`Trace::steps`]; defaults to one in JSON.
    pub min_regions: u64,
}
// Keep the reporting API's string labels while rejecting typos at both input
// boundaries. These are the unchanged labels in performance-v1.
fn known_expected(value: &str) -> bool {
    matches!(value, "blocked" | "waited" | "moved" | "door_changed")
}
fn expected_outcome<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    if known_expected(&value) {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("unknown expected outcome"))
    }
}
fn one() -> u64 {
    1
}
/// Fixture operation, encoded as a snake-case JSON `type` tag.
///
/// Door IDs are deliberately absent: [`Step::resolve`] discovers them only
/// through the actor's current disclosed view.
#[derive(Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TraceAction {
    /// Move in a disclosed actor-relative direction.
    Move {
        /// Direction passed unchanged to the protocol action.
        direction: Direction,
    },
    /// Change the single eligible disclosed, reachable door.
    Door {
        /// Desired state; doors already in this state are not candidates.
        open: bool,
    },
    /// Wait without requiring a target identity.
    Wait,
}

impl Trace {
    /// Parse the embedded workload, preserving sequence order.
    ///
    /// # Panics
    ///
    /// Panics if the compiled-in fixture cannot deserialize as a [`Trace`].
    pub fn load() -> Self {
        serde_json::from_str(SPEC).expect("versioned trace")
    }
    /// Borrow primary steps whose minimum region count fits this workload.
    ///
    /// Filtering preserves fixture order, does not mutate the trace, and does
    /// not include [`Self::secondary`] or [`Self::traversal`]. It does not check
    /// that a world with the requested count actually exists.
    ///
    /// ```
    /// use tor_test_support::performance::Trace;
    ///
    /// let trace = Trace::load();
    /// let single = trace.steps(1).collect::<Vec<_>>();
    /// assert!(!single.is_empty());
    /// assert!(single.iter().all(|step| step.min_regions <= 1));
    /// assert!(trace.steps(2).count() > single.len());
    /// assert_eq!(trace.steps(0).count(), 0);
    /// ```
    pub fn steps(&self, regions: u64) -> impl Iterator<Item = &Step> {
        self.steps.iter().filter(move |s| s.min_regions <= regions)
    }
}
impl Step {
    /// Resolve an action exclusively through the actor's current disclosure.
    ///
    /// Move and wait preserve their fixture operands. Doors are filtered by
    /// reachability and desired state, then deduplicated by opaque identity:
    /// repeated visible occurrences of one door are not an ambiguity.
    /// This does not authorize or execute the resulting action.
    ///
    /// # Panics
    ///
    /// Panics if the actor is not ready, or a door operation has zero or more
    /// than one distinct eligible door in the supplied view.
    pub fn resolve(&self, view: &StateView) -> Action {
        assert!(view.observation.ready, "{}: actor not ready", self.label);
        match self.action {
            TraceAction::Move { direction } => Action::Move { direction },
            TraceAction::Wait => Action::Wait,
            TraceAction::Door { open } => {
                let mut doors = view
                    .observation
                    .visible_cells
                    .iter()
                    .filter_map(|c| c.door.as_ref())
                    .filter(|d| d.reachable && d.open != open)
                    .map(|d| d.id)
                    .collect::<Vec<_>>();
                doors.sort();
                doors.dedup();
                assert_eq!(
                    doors.len(),
                    1,
                    "{}: expected one disclosed reachable door",
                    self.label
                );
                Action::SetDoor {
                    door: doors[0],
                    open,
                }
            }
        }
    }
    /// Assert an effect and, for accepted operations, picture revision progress.
    ///
    /// Use this for pictures with issued revisions. The driver must supply the
    /// matching before/after pair and completed effect, not just an admission
    /// receipt. Internal region coordinates are not used by this oracle.
    /// See [`Self::verify_effect`] for its deliberately narrow effect checks.
    ///
    /// # Panics
    ///
    /// Panics when an accepted operation does not increase `revision`, or any
    /// effect assertion in [`Self::verify_effect`] fails.
    pub fn verify(&self, before: &StateView, after: &StateView, accepted: bool) {
        if accepted {
            assert!(
                after.revision > before.revision,
                "{} failed to advance",
                self.label
            );
        }
        self.verify_effect(before, after, accepted);
    }
    /// Assert fixture effects without requiring picture revision progress.
    ///
    /// Direct Engine workloads use this when their pictures have no issued
    /// generation; the driver must separately check its authoritative causal
    /// boundary and settle the operation before comparing pictures.
    ///
    /// A blocked step requires rejection and exact equality of both views.
    /// Otherwise acceptance is required, followed by action-specific checks:
    /// movement must change the center key to the previously disclosed adjacent
    /// destination; wait must preserve visible cells; door changes must expose
    /// the resolved door in the desired state and change visible cells. Wait and
    /// door checks do not compare every field of the picture.
    ///
    /// # Panics
    ///
    /// Panics on an unknown outcome label, an acceptance/effect mismatch, missing
    /// movement center/destination cells, or the door-resolution preconditions
    /// of [`Self::resolve`]. This assertion helper is for controlled workloads,
    /// not fallible validation of arbitrary client input.
    pub fn verify_effect(&self, before: &StateView, after: &StateView, accepted: bool) {
        assert!(
            known_expected(&self.expected),
            "{}: unknown expected outcome",
            self.label
        );
        if self.expected == "blocked" {
            assert!(!accepted, "{} unexpectedly accepted", self.label);
            assert_eq!(before, after, "{} advanced state", self.label);
            return;
        }
        assert!(accepted, "{} unexpectedly rejected", self.label);
        let center = |s: &StateView| {
            s.observation
                .visible_cells
                .iter()
                .find(|c| c.position.x == 0 && c.position.y == 0 && c.position.z == 0)
                .unwrap()
                .key
                .clone()
        };
        match self.action {
            TraceAction::Move { direction } => {
                assert_ne!(center(before), center(after), "{} did not move", self.label);
                let (x, y, z) = match direction {
                    Direction::North => (0, -1, 0),
                    Direction::East => (1, 0, 0),
                    Direction::South => (0, 1, 0),
                    Direction::West => (-1, 0, 0),
                    Direction::NorthEast => (1, -1, 0),
                    Direction::SouthEast => (1, 1, 0),
                    Direction::SouthWest => (-1, 1, 0),
                    Direction::NorthWest => (-1, -1, 0),
                    Direction::Up => (0, 0, 1),
                    Direction::Down => (0, 0, -1),
                };
                let destination = before
                    .observation
                    .visible_cells
                    .iter()
                    .find(|c| c.position == tor_protocol::Position { x, y, z })
                    .expect("trace destination must be disclosed");
                assert_eq!(
                    destination.key,
                    center(after),
                    "{} disclosed destination offset",
                    self.label
                );
            }
            TraceAction::Wait => assert_eq!(
                before.observation.visible_cells,
                after.observation.visible_cells
            ),
            TraceAction::Door { open } => {
                let Action::SetDoor { door, .. } = self.resolve(before) else {
                    unreachable!()
                };
                assert!(after
                    .observation
                    .visible_cells
                    .iter()
                    .filter_map(|c| c.door.as_ref())
                    .any(|d| d.id == door && d.open == open));
                assert_ne!(
                    before.observation.visible_cells,
                    after.observation.visible_cells
                );
            }
        }
    }
}
