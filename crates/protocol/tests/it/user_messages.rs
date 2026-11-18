//! Server-authored user messages: closed control vocabulary, strict wire
//! shape and bounded, presentable text.
use serde_json::json;
use tor_protocol::{
    ActorTarget, ControlHint, OutcomeFact, OutcomeFacts, UserMessageSegment, UserMessageStyle,
    MAX_USER_MESSAGE_SEGMENTS, MAX_USER_MESSAGE_SEGMENT_BYTES, MAX_USER_MESSAGE_TEXT_BYTES,
};

fn facts(fact: serde_json::Value) -> serde_json::Value {
    json!({
        "self_target": ActorTarget::from_digest([1; 32]),
        "status": null,
        "actor_names": [],
        "facts": [fact],
    })
}

fn decode(segments: serde_json::Value) -> Result<OutcomeFacts, serde_json::Error> {
    serde_json::from_value(facts(json!({
        "kind": "user_message", "style": "tutorial", "segments": segments,
    })))
}

#[test]
fn control_ids_are_the_closed_snake_case_wire_spellings() {
    // Independent oracle: the vocabulary the package validator and every
    // client binding table rely on, spelled out rather than derived.
    let expected = [
        "move",
        "wait",
        "search",
        "pick_up",
        "inventory",
        "equip",
        "drink",
        "read",
        "zap",
        "throw",
        "travel",
        "look",
        "open_door",
        "rest",
        "save",
        "help",
    ];
    let ids: Vec<_> = ControlHint::ALL
        .iter()
        .map(|control| control.id())
        .collect();
    assert_eq!(ids, expected);
    for &control in ControlHint::ALL {
        assert_eq!(ControlHint::from_id(control.id()), Some(control));
        assert_eq!(serde_json::to_value(control).unwrap(), json!(control.id()));
        assert_eq!(
            serde_json::from_value::<ControlHint>(json!(control.id())).unwrap(),
            control
        );
    }
    for unknown in ["", "Move", "pickup", "pick-up", " move", "attack", "quit"] {
        assert_eq!(ControlHint::from_id(unknown), None, "{unknown:?}");
        assert!(serde_json::from_value::<ControlHint>(json!(unknown)).is_err());
    }
}

#[test]
fn user_message_round_trips_in_outcome_facts() {
    let wire = facts(json!({
        "kind": "user_message",
        "style": "hint",
        "segments": [{"text": "Press "}, {"control": "pick_up"}, {"text": " to take it."}],
    }));
    let decoded: OutcomeFacts = serde_json::from_value(wire.clone()).unwrap();
    decoded.validate().unwrap();
    assert_eq!(
        decoded.facts,
        [OutcomeFact::UserMessage {
            style: UserMessageStyle::Hint,
            segments: vec![
                UserMessageSegment::Text("Press ".into()),
                UserMessageSegment::Control(ControlHint::PickUp),
                UserMessageSegment::Text(" to take it.".into()),
            ],
        }]
    );
    assert_eq!(serde_json::to_value(&decoded).unwrap(), wire);
    for (style, id) in [
        (UserMessageStyle::Tutorial, "tutorial"),
        (UserMessageStyle::Hint, "hint"),
        (UserMessageStyle::Narration, "narration"),
    ] {
        assert_eq!(style.id(), id);
        assert_eq!(serde_json::to_value(style).unwrap(), json!(id));
    }
}

#[test]
fn malformed_user_message_shapes_are_rejected_at_decode() {
    for (case, fact) in [
        (
            "unknown style",
            json!({"kind":"user_message","style":"warning","segments":[{"text":"x"}]}),
        ),
        (
            "unknown control",
            json!({"kind":"user_message","style":"hint","segments":[{"control":"fly"}]}),
        ),
        (
            "unknown segment kind",
            json!({"kind":"user_message","style":"hint","segments":[{"key":"g"}]}),
        ),
        (
            "two-key segment",
            json!({"kind":"user_message","style":"hint","segments":[{"text":"a","control":"wait"}]}),
        ),
        (
            "bare string segment",
            json!({"kind":"user_message","style":"hint","segments":["text"]}),
        ),
        (
            "extra field",
            json!({"kind":"user_message","style":"hint","segments":[{"text":"x"}],"icon":"!"}),
        ),
        (
            "missing segments",
            json!({"kind":"user_message","style":"hint"}),
        ),
        (
            "missing style",
            json!({"kind":"user_message","segments":[{"text":"x"}]}),
        ),
    ] {
        assert!(
            serde_json::from_value::<OutcomeFacts>(facts(fact)).is_err(),
            "{case}"
        );
    }
    let over: Vec<_> = (0..=MAX_USER_MESSAGE_SEGMENTS)
        .map(|_| json!({"control": "wait"}))
        .collect();
    assert!(decode(json!(over)).is_err(), "segment count bound");
    let at: Vec<_> = (0..MAX_USER_MESSAGE_SEGMENTS)
        .map(|_| json!({"control": "wait"}))
        .collect();
    decode(json!(at)).unwrap().validate().unwrap();
}

#[test]
fn unpresentable_or_oversized_text_fails_validation() {
    let long = "a".repeat(MAX_USER_MESSAGE_SEGMENT_BYTES);
    decode(json!([{"text": long}])).unwrap().validate().unwrap();
    let quarter = "a".repeat(MAX_USER_MESSAGE_TEXT_BYTES / 4);
    decode(json!([{"text": quarter}, {"text": quarter}, {"text": quarter}, {"text": quarter}]))
        .unwrap()
        .validate()
        .unwrap();
    for (case, segments) in [
        ("no segments", json!([])),
        ("empty text", json!([{"text": ""}])),
        ("only whitespace", json!([{"text": "   "}])),
        ("newline", json!([{"text": "one\ntwo"}])),
        ("escape sequence", json!([{"text": "\u{1b}[31mred"}])),
        (
            "segment too long",
            json!([{"text": "a".repeat(MAX_USER_MESSAGE_SEGMENT_BYTES + 1)}]),
        ),
        (
            "total too long",
            json!([{"text": quarter}, {"text": quarter}, {"text": quarter}, {"text": quarter}, {"text": "a"}]),
        ),
    ] {
        let decoded = decode(segments).unwrap();
        assert!(decoded.validate().is_err(), "{case}");
    }
    // Whitespace between two controls is presentable.
    decode(json!([{"control": "move"}, {"text": " "}, {"control": "travel"}]))
        .unwrap()
        .validate()
        .unwrap();
}
