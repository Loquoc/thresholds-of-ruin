//! Past-tense activity facts about other visible actors: strict wire shape,
//! category-only disclosure and checked event-time validity.
use serde_json::json;
use tor_protocol::{
    ActivityFollowThrough, ActivityKind, ActivityPosture, ActorTarget, ItemWorkKind, OutcomeFact,
    OutcomeFacts, Position,
};

const SELF: [u8; 32] = [1; 32];
const OTHER: [u8; 32] = [2; 32];

fn facts(fact: serde_json::Value) -> serde_json::Value {
    json!({
        "self_target": ActorTarget::from_digest(SELF),
        "status": null,
        "actor_names": [],
        "facts": [fact],
    })
}

fn activity(actor: [u8; 32], started_at: &str, activity: serde_json::Value) -> serde_json::Value {
    facts(json!({
        "kind": "activity",
        "actor": ActorTarget::from_digest(actor),
        "started_at": started_at,
        "activity": activity,
    }))
}

fn decode(wire: serde_json::Value) -> Result<OutcomeFacts, serde_json::Error> {
    serde_json::from_value(wire)
}

fn position(x: i32, y: i32, z: i32) -> Position {
    Position { x, y, z }
}

#[test]
fn activity_kinds_round_trip_with_exact_wire_spellings() {
    let cases = [
        (
            json!({"type": "move", "from": {"x": 1, "y": 2, "z": 0}, "to": {"x": 2, "y": 2, "z": 0}}),
            ActivityKind::Move {
                from: Some(position(1, 2, 0)),
                to: Some(position(2, 2, 0)),
            },
        ),
        (
            json!({"type": "move", "from": {"x": 4, "y": 0, "z": 0}, "to": null}),
            ActivityKind::Move {
                from: Some(position(4, 0, 0)),
                to: None,
            },
        ),
        (
            json!({"type": "wind_up", "posture": "melee"}),
            ActivityKind::WindUp {
                posture: ActivityPosture::Melee,
            },
        ),
        (
            json!({"type": "wind_up", "posture": "ranged"}),
            ActivityKind::WindUp {
                posture: ActivityPosture::Ranged,
            },
        ),
        (
            json!({"type": "wind_up", "posture": "casting"}),
            ActivityKind::WindUp {
                posture: ActivityPosture::Casting,
            },
        ),
        (
            json!({"type": "wind_up", "posture": "other"}),
            ActivityKind::WindUp {
                posture: ActivityPosture::Other,
            },
        ),
        (
            json!({"type": "follow_through", "after": "strike"}),
            ActivityKind::FollowThrough {
                after: ActivityFollowThrough::Strike,
            },
        ),
        (
            json!({"type": "follow_through", "after": "cast"}),
            ActivityKind::FollowThrough {
                after: ActivityFollowThrough::Cast,
            },
        ),
        (
            json!({"type": "item_handling", "work": "drink", "name": "bubbly potion"}),
            ActivityKind::ItemHandling {
                work: ItemWorkKind::Drink,
                name: Some("bubbly potion".into()),
            },
        ),
        (
            json!({"type": "item_handling", "work": "zap", "name": null}),
            ActivityKind::ItemHandling {
                work: ItemWorkKind::Zap,
                name: None,
            },
        ),
    ];
    for (shape, expected) in cases {
        let wire = activity(OTHER, "40", shape);
        let decoded = decode(wire.clone()).unwrap();
        decoded.validate_at(40).unwrap();
        assert_eq!(
            decoded.facts,
            [OutcomeFact::Activity {
                actor: ActorTarget::from_digest(OTHER),
                started_at: 40,
                activity: expected,
            }]
        );
        assert_eq!(serde_json::to_value(&decoded).unwrap(), wire);
    }
}

#[test]
fn malformed_activity_shapes_are_rejected_at_decode() {
    let other = ActorTarget::from_digest(OTHER);
    for (case, fact) in [
        (
            "missing started_at",
            json!({"kind": "activity", "actor": other, "activity": {"type": "wind_up", "posture": "melee"}}),
        ),
        (
            "numeric started_at",
            json!({"kind": "activity", "actor": other, "started_at": 4, "activity": {"type": "wind_up", "posture": "melee"}}),
        ),
        (
            "unknown activity field",
            json!({"kind": "activity", "actor": other, "started_at": "4", "ready_at": "9", "activity": {"type": "wind_up", "posture": "melee"}}),
        ),
        (
            "unknown kind",
            json!({"kind": "activity", "actor": other, "started_at": "4", "activity": {"type": "wait"}}),
        ),
        (
            "unknown posture",
            json!({"kind": "activity", "actor": other, "started_at": "4", "activity": {"type": "wind_up", "posture": "fireball"}}),
        ),
        (
            "posture carries an ability",
            json!({"kind": "activity", "actor": other, "started_at": "4", "activity": {"type": "wind_up", "posture": "melee", "ability": "bite"}}),
        ),
        (
            "follow-through carries an end",
            json!({"kind": "activity", "actor": other, "started_at": "4", "activity": {"type": "follow_through", "after": "strike", "ends_at": "9"}}),
        ),
        (
            "move omits a nullable endpoint",
            json!({"kind": "activity", "actor": other, "started_at": "4", "activity": {"type": "move", "from": {"x": 1, "y": 0, "z": 0}}}),
        ),
        (
            "item handling omits its nullable name",
            json!({"kind": "activity", "actor": other, "started_at": "4", "activity": {"type": "item_handling", "work": "drink"}}),
        ),
    ] {
        assert!(decode(facts(fact)).is_err(), "{case}");
    }
}

#[test]
fn activity_validation_rejects_own_future_and_empty_facts() {
    let valid = |actor, started_at: &str, tick, shape| {
        decode(activity(actor, started_at, shape))
            .unwrap()
            .validate_at(tick)
            .is_ok()
    };
    let posture = json!({"type": "wind_up", "posture": "ranged"});
    // Independent boundary witness: the start may equal, never follow, the update tick.
    assert!(valid(OTHER, "12", 12, posture.clone()));
    assert!(valid(OTHER, "11", 12, posture.clone()));
    assert!(!valid(OTHER, "13", 12, posture.clone()), "future start");
    // Own actions keep their own-action events and status facts.
    assert!(!valid(SELF, "12", 12, posture), "own activity");
    assert!(
        !valid(
            OTHER,
            "12",
            12,
            json!({"type": "move", "from": null, "to": null})
        ),
        "move without any disclosed endpoint"
    );
    assert!(
        !valid(
            OTHER,
            "12",
            12,
            json!({"type": "move", "from": {"x": 1, "y": 1, "z": 0}, "to": {"x": 1, "y": 1, "z": 0}})
        ),
        "move to its own cell"
    );
    for name in ["", "  ", "bad\nname"] {
        assert!(
            !valid(
                OTHER,
                "12",
                12,
                json!({"type": "item_handling", "work": "read", "name": name})
            ),
            "{name:?}"
        );
    }
    assert!(!valid(
        OTHER,
        "12",
        12,
        json!({"type": "item_handling", "work": "read", "name": "x".repeat(257)})
    ));
}
