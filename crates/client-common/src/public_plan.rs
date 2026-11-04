//! Presentation of already-paid historical facts; no target resolution or clock.
//!
//! The server's paid observation is a historical record, not a live countdown.
//! Its subject name and aim coordinates remain in the original observer frame.
//! Feed these helpers records from validated disclosed state. They format supplied
//! values without calling [`PublicPlanObservationView::valid_at`] or resolving
//! identities, checking current visibility, or granting interaction authority.
use std::fmt::Write;
use tor_protocol::{ObservedAimView, PublicPlanObservationView, PublicPlanStateView};

/// Format one historical observation as a sentence with its original tick.
///
/// Missing ability and aim disclosure stay explicitly unknown. Remaining work
/// is printed as observed, never decremented using a local clock. The subject
/// label and any disclosed ability name are used verbatim, so this formatter is
/// not a substitute for protocol/state validation or frontend escaping.
///
/// ```
/// use tor_client_common::public_plan::line;
/// use tor_protocol::{ObservedAimView, PublicPlanObservationView, PublicPlanStateView};
/// let record = PublicPlanObservationView {
///     observed_at: 12,
///     subject_name: "a creature".into(),
///     state: PublicPlanStateView::Preparing {
///         ability: None, aim: ObservedAimView::Unavailable, remaining_work: 4,
///     },
/// };
/// assert_eq!(line(&record), "Observed at tick 12: a creature was preparing; ability undisclosed; aim unavailable; 4 work remaining.");
/// ```
///
/// A disclosed direction is a historical vector, not a current target cell:
///
/// ```
/// use tor_client_common::public_plan::line;
/// use tor_protocol::{ContentRef, ObservedAimView, Position, PublicPlanObservationView, PublicPlanStateView};
/// let record = PublicPlanObservationView {
///     observed_at: 12,
///     subject_name: "a creature".into(),
///     state: PublicPlanStateView::Preparing {
///         ability: Some(ContentRef { id: "example-strike".into(), name: "Strike".into() }),
///         aim: ObservedAimView::Direction { direction: Position { x: 1, y: 0, z: 0 } },
///         remaining_work: 4,
///     },
/// };
/// assert_eq!(line(&record), "Observed at tick 12: a creature was preparing Strike; aim direction (1,0,0) in your frame at that time; 4 work remaining.");
/// ```
pub fn line(record: &PublicPlanObservationView) -> String {
    let mut text = format!(
        "Observed at tick {}: {} ",
        record.observed_at, record.subject_name
    );
    match &record.state {
        PublicPlanStateView::NoPublicPlan => text.push_str("had no public plan available"),
        PublicPlanStateView::Preparing {
            ability,
            aim,
            remaining_work,
        } => {
            text.push_str("was preparing");
            if let Some(ability) = ability {
                text.push(' ');
                text.push_str(&ability.name);
            } else {
                text.push_str("; ability undisclosed");
            }
            text.push_str("; aim ");
            match aim {
                ObservedAimView::Unavailable => text.push_str("unavailable"),
                ObservedAimView::Direction { direction } => {
                    write!(
                        &mut text,
                        "direction ({},{},{}) in your frame at that time",
                        direction.x, direction.y, direction.z
                    )
                    .expect("String writes are infallible");
                }

                ObservedAimView::Disclosed { occurrences, .. } => {
                    for (index, at) in occurrences.iter().enumerate() {
                        if index > 0 {
                            text.push_str(" / ");
                        }
                        write!(&mut text, "({},{},{})", at.x, at.y, at.z)
                            .expect("String writes are infallible");
                    }
                    text.push_str(" in your frame at that time");
                }
            }
            write!(&mut text, "; {remaining_work} work remaining")
                .expect("String writes are infallible");
        }
    }
    text.push('.');
    text
}

/// Format `after` only when it is present and differs from `before` by value.
///
/// This compares the complete record, including the observation tick. `None`
/// after a known record is silent: losing disclosure does not imply that a
/// preparation ended. An explicit [`PublicPlanStateView::NoPublicPlan`] record
/// is different from absence and can produce a new line. Callers retain their
/// own prior record; this function stores nothing and never reads a clock.
///
/// ```
/// use tor_client_common::public_plan::{changed_line, line};
/// use tor_protocol::{PublicPlanObservationView, PublicPlanStateView};
/// let record = PublicPlanObservationView {
///     observed_at: 12, subject_name: "a creature".into(),
///     state: PublicPlanStateView::NoPublicPlan,
/// };
/// assert_eq!(changed_line(None, Some(&record)), Some(line(&record)));
/// assert_eq!(line(&record), "Observed at tick 12: a creature had no public plan available.");
/// assert_eq!(changed_line(Some(&record), Some(&record.clone())), None);
/// assert_eq!(changed_line(Some(&record), None), None);
/// assert_eq!(changed_line(None, None), None);
/// let later = PublicPlanObservationView { observed_at: 20, ..record.clone() };
/// assert_eq!(changed_line(Some(&record), Some(&later)), Some(line(&later)));
/// ```
pub fn changed_line(
    before: Option<&PublicPlanObservationView>,
    after: Option<&PublicPlanObservationView>,
) -> Option<String> {
    after.filter(|record| before != Some(*record)).map(line)
}
