use tor_client_common::{test_support, Palette};
use tor_client_text::engine::{
    chronicle::Chronicler,
    narrate::{compose, Entry, Record},
};
use tor_protocol::*;

#[test]
fn watched_item_handling_is_told_once_and_presentation_timing_is_silent() {
    let before = test_support::observation();
    let row = before.visible_actors[0].clone();
    let after = before.clone();
    let mut facts = OutcomeFacts::capture(&before, &after, None);
    let activity = |activity| OutcomeFact::Activity {
        actor: row.id,
        started_at: after.tick,
        activity,
    };
    facts.extend_activities(
        vec![
            activity(ActivityKind::Move {
                from: Some(Position { x: 1, y: 0, z: 0 }),
                to: None,
            }),
            activity(ActivityKind::WindUp {
                posture: ActivityPosture::Ranged,
            }),
            activity(ActivityKind::ItemHandling {
                work: ItemWorkKind::Read,
                name: Some("scroll labelled ZELGO MER".into()),
            }),
        ],
        &OutcomeObservation::from(&before),
        &OutcomeObservation::from(&after),
    );
    facts.validate_at(after.tick).unwrap();
    let text = compose(&Record {
        entries: vec![Entry::Beats(
            Chronicler::default().outcome(&facts, &Palette::default()),
        )],
    });
    let line = format!("{} reads the scroll labelled ZELGO MER.", row.name);
    assert_eq!(text.matches(&line).count(), 1, "{text}");
    for silent in ["ranged", "wind", "steps", "moves"] {
        assert!(!text.to_lowercase().contains(silent), "{silent}: {text}");
    }
}
