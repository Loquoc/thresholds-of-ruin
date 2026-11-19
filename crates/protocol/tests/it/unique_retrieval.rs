use tor_protocol::{Event, OutcomeFact, UserMessageSegment};

#[test]
fn taking_the_scenario_exit_is_an_event_without_an_item_and_goals_are_text() {
    let event = serde_json::json!({"type": "escaped"});
    let decoded: Event = serde_json::from_value(event.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), event);
    // The exit no longer names the item the old built-in victory required.
    assert!(
        serde_json::from_value::<Event>(serde_json::json!({"type": "escaped", "item": "x"}))
            .is_err()
    );

    let goal = serde_json::json!({"kind": "goal", "segments": [{"text": "Climb out with "}, {"control": "inventory"}]});
    let decoded: OutcomeFact = serde_json::from_value(goal.clone()).unwrap();
    assert_eq!(
        decoded,
        OutcomeFact::Goal {
            segments: vec![
                UserMessageSegment::Text("Climb out with ".into()),
                UserMessageSegment::Control(tor_protocol::ControlHint::Inventory),
            ]
        }
    );
    assert_eq!(serde_json::to_value(decoded).unwrap(), goal);
    // The removed objective fact is refused.
    assert!(serde_json::from_value::<OutcomeFact>(
        serde_json::json!({"kind": "objective", "objective": "reach_exit"})
    )
    .is_err());
}
