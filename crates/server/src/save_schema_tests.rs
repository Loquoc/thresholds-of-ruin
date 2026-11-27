use super::*;
use crate::actions::{Action, Direction};
use tor_simulation::ActorId;
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Stored {
    #[serde(with = "super::Action")]
    action: Action,
}
#[test]
fn saved_projectile_operands_are_numeric_exact_and_independent_of_live_items() {
    use serde_json::json;
    for delivery in [
        json!({"kind":"throw","source":{"kind":"equipped","item":9_007_199_254_740_993_u64,"socket":0}}),
        json!({"kind":"throw","source":{"kind":"carried","item":9_007_199_254_740_995_u64,"socket":63}}),
        json!({"kind":"launch","launcher":{"item":9_007_199_254_740_993_u64,"socket":0},"ammunition":9_007_199_254_740_995_u64}),
    ] {
        for direction in [
            "north",
            "east",
            "south",
            "west",
            "north_east",
            "south_east",
            "south_west",
            "north_west",
            "up",
            "down",
        ] {
            let value = json!({"action":{"type":"use_ability","id":"fixture_projectile",
                    "invocation":{"aim":{"kind":"directional_projectile","direction":direction,"delivery":delivery},"weapon":{"kind":"default"}},"supplies":{"kind":"carried"}}});
            let stored: Stored =
                crate::storage::codec::strict(&serde_json::to_vec(&value).unwrap()).unwrap();
            assert_eq!(serde_json::to_value(&stored).unwrap(), value);
            assert_eq!(
                crate::adapt::recorded_action(crate::adapt::action(&stored.action)),
                Some(stored.action)
            );
        }
    }
}

#[test]
fn saved_projectile_selector_codec_rejects_missing_extra_positional_and_conflicting_operands() {
    use serde_json::json;
    let good = json!({"aim":{"kind":"directional_projectile","direction":"north","delivery":{"kind":"launch","launcher":{"item":1,"socket":0},"ammunition":2}},"weapon":{"kind":"default"}});
    assert!(serde_json::from_value::<AbilityInvocation>(good.clone()).is_ok());
    let mut bad = Vec::new();
    for path in ["", "/aim", "/aim/delivery", "/aim/delivery/launcher"] {
        let original = good.pointer(path).unwrap().as_object().unwrap();
        for key in original.keys() {
            let mut value = good.clone();
            value
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            bad.push(value);
        }
        let mut value = good.clone();
        value.pointer_mut(path).unwrap()["extra"] = json!(null);
        bad.push(value);
        let mut value = good.clone();
        *value.pointer_mut(path).unwrap() = json!(original.values().collect::<Vec<_>>());
        bad.push(value);
    }
    for path in ["/aim/delivery/launcher/item", "/aim/delivery/ammunition"] {
        for item in [
            json!(0),
            json!(u64::MAX),
            json!("1"),
            json!(1.0),
            json!(-1),
            json!(null),
        ] {
            let mut value = good.clone();
            *value.pointer_mut(path).unwrap() = item;
            bad.push(value);
        }
    }
    for socket in [json!(64), json!(65536), json!(-1), json!(0.0), json!("0")] {
        let mut value = good.clone();
        value["aim"]["delivery"]["launcher"]["socket"] = socket;
        bad.push(value);
    }
    let mut value = good.clone();
    value["aim"]["delivery"]["ammunition"] = json!(1);
    bad.push(value);
    let mut value = good.clone();
    value["weapon"] = json!({"kind":"equipped","item":1,"socket":0});
    bad.push(value);
    for direction in [
        json!({"north":null}),
        json!(["north"]),
        json!("north_up"),
        json!(null),
    ] {
        let mut value = good.clone();
        value["aim"]["direction"] = direction;
        bad.push(value);
    }
    for value in bad {
        assert!(
            serde_json::from_value::<AbilityInvocation>(value.clone()).is_err(),
            "{value}"
        );
    }
    let raw = serde_json::to_string(&good).unwrap();
    for (from, to) in [
        ("\"item\":1", "\"item\":1,\"item\":2"),
        ("\"socket\":0", "\"socket\":0,\"socket\":1"),
        ("\"ammunition\":2", "\"ammunition\":2,\"ammunition\":3"),
        (
            "\"kind\":\"launch\"",
            "\"kind\":\"launch\",\"kind\":\"throw\"",
        ),
    ] {
        assert!(serde_json::from_str::<AbilityInvocation>(&raw.replace(from, to)).is_err());
    }
    for kind in ["equipped", "carried"] {
        let source = json!({"kind":kind,"item":1,"socket":0});
        assert!(serde_json::from_value::<ThrownSourceSelection>(source.clone()).is_ok());
        for key in ["kind", "item", "socket"] {
            let mut value = source.clone();
            value.as_object_mut().unwrap().remove(key);
            assert!(serde_json::from_value::<ThrownSourceSelection>(value).is_err());
        }
        for item in [json!(0), json!(u64::MAX), json!("1")] {
            let mut value = source.clone();
            value["item"] = item;
            assert!(serde_json::from_value::<ThrownSourceSelection>(value).is_err());
        }
        let mut value = source.clone();
        value["socket"] = json!(64);
        assert!(serde_json::from_value::<ThrownSourceSelection>(value).is_err());
        let mut value = source.clone();
        value["extra"] = json!(null);
        assert!(serde_json::from_value::<ThrownSourceSelection>(value).is_err());
        assert!(serde_json::from_value::<ThrownSourceSelection>(json!([kind, 1, 0])).is_err());
        let raw = serde_json::to_string(&source).unwrap();
        assert!(serde_json::from_str::<ThrownSourceSelection>(
            &raw.replace("\"item\":1", "\"item\":1,\"item\":2")
        )
        .is_err());
    }
}

#[test]
fn apparent_touch_saved_input_requires_exact_numeric_subject_and_nullable_quantity() {
    for quantity in [None, Some(1), Some(0), Some(2), Some(u64::MAX)] {
        let value = Stored {
            action: Action::TouchApparentObject {
                subject: ActorId(u64::MAX),
                quantity,
            },
        };
        let saved = serde_json::json!({"action": {
            "type": "touch_apparent_object", "subject": u64::MAX, "quantity": quantity
        }});
        assert_eq!(serde_json::to_value(&value).unwrap(), saved);
        assert_eq!(
            crate::storage::codec::strict::<Stored>(&serde_json::to_vec(&saved).unwrap()).unwrap(),
            value
        );
    }
    for invalid in [
        r#"{"action":{"type":"touch_apparent_object","subject":7}}"#,
        r#"{"action":{"type":"touch_apparent_object","subject":"7","quantity":null}}"#,
        r#"{"action":{"type":"touch_apparent_object","subject":7.0,"quantity":null}}"#,
        r#"{"action":{"type":"touch_apparent_object","subject":7,"quantity":"1"}}"#,
        r#"{"action":{"type":"touch_apparent_object","subject":7,"quantity":null,"item":7}}"#,
        r#"{"action":{"type":"touch_apparent_object","subject":7,"quantity":null,"reference":"creature"}}"#,
    ] {
        assert!(
            crate::storage::codec::strict::<Stored>(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn saved_crumbled_pickup_keeps_internal_identity_and_no_result_transfer() {
    let saved = serde_json::json!({"type":"crumbled", "item":7, "quantity":2});
    let restored: crate::journal::Event =
        crate::storage::codec::strict(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), saved);
    for invalid in [
        serde_json::json!({"type":"crumbled", "item":"7", "quantity":2}),
        serde_json::json!({"type":"crumbled", "item":7}),
        serde_json::json!({"type":"crumbled", "item":7, "quantity":2, "result":8}),
    ] {
        assert!(crate::storage::codec::strict::<crate::journal::Event>(
            &serde_json::to_vec(&invalid).unwrap()
        )
        .is_err());
    }
}

#[test]
fn saved_ability_started_reference_is_required_and_save_owned() {
    for reference in ["creature", "contact"] {
        let saved = serde_json::json!({"type":"ability_started", "reference":reference,
                "id":"basic_melee", "name":"Basic melee", "target":u64::MAX});
        let restored: crate::journal::Event =
            crate::storage::codec::strict(&serde_json::to_vec(&saved).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), saved);
    }
    for reference in [
        serde_json::Value::Null,
        serde_json::json!("apparent_object"),
        serde_json::json!({"contact":null}),
        serde_json::json!(0),
    ] {
        let invalid = serde_json::json!({"type":"ability_started", "reference":reference,
                "id":"basic_melee", "name":"Basic melee", "target":7});
        assert!(crate::storage::codec::strict::<crate::journal::Event>(
            &serde_json::to_vec(&invalid).unwrap()
        )
        .is_err());
    }
    assert!(crate::storage::codec::strict::<crate::journal::Event>(
        br#"{"type":"ability_started","id":"basic_melee","name":"Basic melee","target":7}"#
    )
    .is_err());
}

#[test]
fn saved_escape_uses_exact_internal_numeric_identity_not_wire_tokens() {
    let value = Stored {
        action: Action::Escape {
            hold: tor_simulation::HoldId(7),
        },
    };
    assert_eq!(
        serde_json::to_value(&value).unwrap(),
        serde_json::json!({"action":{"type":"escape","hold":7}})
    );
    assert_eq!(
        serde_json::from_value::<Stored>(serde_json::to_value(&value).unwrap()).unwrap(),
        value
    );
    for hold in [
        serde_json::json!(0),
        serde_json::json!(u64::MAX),
        serde_json::json!("7"),
        serde_json::json!(format!("h_{}", "09".repeat(32))),
    ] {
        assert!(serde_json::from_value::<Stored>(
            serde_json::json!({"action":{"type":"escape","hold":hold}})
        )
        .is_err());
    }
}
#[test]
fn saved_directional_ray_has_no_actor_or_predicted_path_operand() {
    for direction in [
        "north",
        "east",
        "south",
        "west",
        "north_east",
        "south_east",
        "south_west",
        "north_west",
        "up",
        "down",
    ] {
        let value = format!(
            r#"{{"action":{{"type":"use_ability","id":"fixture_ray","invocation":{{"aim":{{"kind":"directional_ray","direction":"{direction}"}},"weapon":{{"kind":"default"}}}},"supplies":{{"kind":"carried"}}}}}}"#
        );
        check_stored::<Stored>(&value);
        for extra in [
            r#", "target":null"#,
            r#", "path":[]"#,
            r#", "destination":null"#,
        ] {
            let forged = value.replace(
                &format!(r#""direction":"{direction}""#),
                &format!(r#""direction":"{direction}"{extra}"#),
            );
            assert!(crate::storage::codec::strict::<Stored>(forged.as_bytes()).is_err());
        }
    }
}
#[test]
fn saved_invocations_keep_each_direction_and_numeric_actor_independently_of_wire() {
    let id = tor_simulation::content_ids::AbilityId::new("arbitrary_authored_name").unwrap();
    for (direction, spelling) in [
        (Direction::North, "north"),
        (Direction::East, "east"),
        (Direction::South, "south"),
        (Direction::West, "west"),
        (Direction::NorthEast, "north_east"),
        (Direction::SouthEast, "south_east"),
        (Direction::SouthWest, "south_west"),
        (Direction::NorthWest, "north_west"),
        (Direction::Up, "up"),
        (Direction::Down, "down"),
    ] {
        let value = Stored {
            action: Action::UseAbility {
                id: id.clone(),
                invocation: crate::actions::AbilityInvocation {
                    aim: crate::actions::AbilityAim::SelfStep { direction },
                    weapon: crate::actions::WeaponSelection::Default,
                },
                supplies: Default::default(),
            },
        };
        let encoded = serde_json::to_string(&value).unwrap();
        assert_eq!(
            encoded,
            format!(
                r#"{{"action":{{"type":"use_ability","id":"arbitrary_authored_name","invocation":{{"aim":{{"kind":"self_step","direction":"{spelling}"}},"weapon":{{"kind":"default"}}}},"supplies":{{"kind":"carried"}}}}}}"#
            )
        );
        assert_eq!(
            crate::storage::codec::strict::<Stored>(encoded.as_bytes()).unwrap(),
            value
        );
    }
    for target in [1, 9_007_199_254_740_993, u64::MAX] {
        let value = Stored {
            action: Action::UseAbility {
                id: id.clone(),
                invocation: crate::actions::AbilityInvocation {
                    aim: crate::actions::AbilityAim::ActorTarget {
                        target: ActorId(target),
                    },
                    weapon: crate::actions::WeaponSelection::Default,
                },
                supplies: Default::default(),
            },
        };
        let encoded = serde_json::to_string(&value).unwrap();
        assert!(encoded.contains(&format!(
                r#""invocation":{{"aim":{{"kind":"actor_target","target":{target}}},"weapon":{{"kind":"default"}}}}"#
            )));
        assert_eq!(
            crate::storage::codec::strict::<Stored>(encoded.as_bytes()).unwrap(),
            value
        );
    }
}

#[test]
fn saved_invocations_reject_legacy_positional_duplicate_and_cross_variant_operands() {
    for invocation in [
        r#"["self_step","north"]"#,
        r#"{"aim":{"kind":"self_step"},"weapon":{"kind":"default"}}"#,
        r#"{"aim":{"kind":"self_step","direction":"north_up"},"weapon":{"kind":"default"}}"#,
        r#"{"aim":{"kind":"self_step","direction":{"north":null}},"weapon":{"kind":"default"}}"#,
        r#"{"aim":{"kind":"self_step","direction":["north"]},"weapon":{"kind":"default"}}"#,
        r#"{"aim":{"kind":"self_step","direction":null},"weapon":{"kind":"default"}}"#,
        r#"{"aim":{"kind":"self_step","direction":0},"weapon":{"kind":"default"}}"#,
        r#"{"aim":{"kind":"self_step","direction":"north","direction":"south"},"weapon":{"kind":"default"}}"#,
        r#"{"aim":{"kind":"self_step","kind":"actor_target","direction":"north"},"weapon":{"kind":"default"}}"#,
        r#"{"aim":{"kind":"self_step","direction":"north","target":2},"weapon":{"kind":"default"}}"#,
        r#"{"aim":{"kind":"actor_target","target":2,"direction":"north"},"weapon":{"kind":"default"}}"#,
        r#"{"aim":{"kind":"actor_target","target":"2"},"weapon":{"kind":"default"}}"#,
        r#"{"aim":{"kind":"actor_target","target":2,"target":3},"weapon":{"kind":"default"}}"#,
        r#"{"aim":{"kind":"actor_target","target":{"type":"self_actor"}},"weapon":{"kind":"default"}}"#,
        r#"{"aim":{"kind":"self_step","direction":"north","destination":{"region":1,"position":{"x":1,"y":2,"z":0}}},"weapon":{"kind":"default"}}"#,
    ] {
        let encoded = format!(
            r#"{{"action":{{"type":"use_ability","id":"arbitrary_authored_name","invocation":{invocation},"supplies":{{"kind":"carried"}}}}}}"#
        );
        assert!(
            crate::storage::codec::strict::<Stored>(encoded.as_bytes()).is_err(),
            "{encoded}"
        );
    }
    for encoded in [
        r#"{"action":{"type":"use_ability","id":"arbitrary_authored_name","target":2,"supplies":{"kind":"carried"}}}"#,
        r#"{"action":{"type":"use_ability","id":"arbitrary_authored_name","invocation":{"aim":{"kind":"self_step","direction":"north"},"weapon":{"kind":"default"}},"invocation":{"aim":{"kind":"self_step","direction":"south"},"weapon":{"kind":"default"}},"supplies":{"kind":"carried"}}}"#,
    ] {
        assert!(crate::storage::codec::strict::<Stored>(encoded.as_bytes()).is_err());
    }
}
#[test]
fn stored_actions_keep_numeric_extremes_and_explicit_variant_shapes() {
    let cases = [
        (
            Action::Equip {
                item: u64::MAX,
                slot: u16::MAX,
            },
            r#"{"type":"equip","item":18446744073709551615,"slot":65535}"#,
        ),
        (
            Action::Unequip { item: u64::MAX },
            r#"{"type":"unequip","item":18446744073709551615}"#,
        ),
        (
            Action::Ignite { item: u64::MAX },
            r#"{"type":"ignite","item":18446744073709551615}"#,
        ),
        (
            Action::Extinguish { item: u64::MAX },
            r#"{"type":"extinguish","item":18446744073709551615}"#,
        ),
        (
            Action::Drink { item: u64::MAX },
            r#"{"type":"drink","item":18446744073709551615}"#,
        ),
        (
            Action::Eat { item: u64::MAX },
            r#"{"type":"eat","item":18446744073709551615}"#,
        ),
        (
            Action::UseAbility {
                id: tor_simulation::content_ids::AbilityId::new("basic_melee").unwrap(),
                invocation: crate::actions::AbilityInvocation {
                    aim: crate::actions::AbilityAim::ActorTarget {
                        target: ActorId(u64::MAX),
                    },
                    weapon: crate::actions::WeaponSelection::Default,
                },
                supplies: Default::default(),
            },
            r#"{"type":"use_ability","id":"basic_melee","invocation":{"aim":{"kind":"actor_target","target":18446744073709551615},"weapon":{"kind":"default"}},"supplies":{"kind":"carried"}}"#,
        ),
        (
            Action::SetDoor {
                door: u64::MAX,
                open: true,
            },
            r#"{"type":"set_door","door":18446744073709551615,"open":true}"#,
        ),
        (
            Action::Move {
                direction: Direction::NorthEast,
            },
            r#"{"type":"move","direction":"north_east"}"#,
        ),
        (
            Action::Take {
                item: u64::MAX,
                quantity: Some(u64::MAX),
            },
            r#"{"type":"take","item":18446744073709551615,"quantity":18446744073709551615}"#,
        ),
        (
            Action::Drop {
                item: 9007199254740993,
                quantity: None,
            },
            r#"{"type":"drop","item":9007199254740993,"quantity":null}"#,
        ),
        (Action::Wait, r#"{"type":"wait"}"#),
    ];
    for (action, expected) in cases {
        let value = Stored { action };
        let encoded = serde_json::to_string(&value).unwrap();
        assert_eq!(encoded, format!("{{\"action\":{expected}}}"));
        assert_eq!(
            super::super::codec::strict::<Stored>(encoded.as_bytes()).unwrap(),
            value
        );
    }
}
#[test]
fn stored_device_aim_is_closed_named_and_independent_of_transport() {
    use crate::actions::DeviceAim;
    for aim in [
        DeviceAim::Direction {
            direction: Direction::NorthWest,
        },
        DeviceAim::SelfTarget {},
    ] {
        let stored = Stored {
            action: Action::Zap { item: 71, aim },
        };
        let encoded = serde_json::to_vec(&stored).unwrap();
        assert_eq!(
            crate::storage::codec::strict::<Stored>(&encoded).unwrap(),
            stored
        );
        let value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        assert!(value["action"].get("direction").is_none());
        assert!(value["action"]["aim"].is_object());
    }
    for aim in [
        r#""self_target""#,
        r#"["self_target"]"#,
        r#"{"kind":"self_target","direction":"east"}"#,
        r#"{"kind":"self_target","target":1}"#,
        r#"{"kind":"direction"}"#,
        r#"{"kind":"direction","direction":"east","target":1}"#,
        r#"{"kind":"direction","direction":null}"#,
        r#"{"kind":"self_target","kind":"self_target"}"#,
    ] {
        let encoded = format!(r#"{{"action":{{"type":"zap","item":71,"aim":{aim}}}}}"#);
        assert!(
            crate::storage::codec::strict::<Stored>(encoded.as_bytes()).is_err(),
            "{encoded}"
        );
    }
    assert!(crate::storage::codec::strict::<Stored>(
        br#"{"action":{"type":"zap","item":71,"direction":"east"}}"#
    )
    .is_err());
}

#[test]
fn stored_action_rejects_unknown_fields_and_lossy_or_string_numbers() {
    for value in [
        r#"{"action":{"type":"use_ability","ability":"basic_melee","target":1}}"#,
        r#"{"action":{"type":"use_ability","id":"fear","target":"1"}}"#,
        r#"{"action":{"type":"wait","extra":1}}"#,
        r#"{"action":{"type":"attack","target":1}}"#,
        r#"{"action":{"type":"use_ability","id":"basic_melee","target":"18446744073709551615"}}"#,
        r#"{"action":{"type":"take","item":1.5,"quantity":null}}"#,
        r#"{"action":{"type":"take","item":18446744073709551616,"quantity":null}}"#,
    ] {
        assert!(
            super::super::codec::strict::<Stored>(value.as_bytes()).is_err(),
            "{value}"
        );
    }
}
#[test]
fn stored_ability_actions_preserve_numeric_targets_independently_of_wire_tokens() {
    for name in [
        "basic_melee",
        "power_strike",
        "magic_bolt",
        "fear",
        "fixture_ability",
    ] {
        let value = Stored {
            action: crate::actions::Action::UseAbility {
                id: tor_simulation::content_ids::AbilityId::new(name).unwrap(),
                invocation: crate::actions::AbilityInvocation {
                    aim: crate::actions::AbilityAim::ActorTarget {
                        target: tor_simulation::ActorId(u64::MAX),
                    },
                    weapon: crate::actions::WeaponSelection::Default,
                },
                supplies: Default::default(),
            },
        };
        let encoded = serde_json::to_string(&value).unwrap();
        assert_eq!(encoded, format!("{{\"action\":{{\"type\":\"use_ability\",\"id\":\"{name}\",\"invocation\":{{\"aim\":{{\"kind\":\"actor_target\",\"target\":18446744073709551615}},\"weapon\":{{\"kind\":\"default\"}}}},\"supplies\":{{\"kind\":\"carried\"}}}}}}"));
        assert_eq!(
            super::super::codec::strict::<Stored>(encoded.as_bytes()).unwrap(),
            value
        );
    }
}
#[test]
fn phase5_saved_selection_uses_required_bounded_numeric_source_rows() {
    let original = serde_json::json!({"action":{"type":"use_ability","id":"bind_wounds","invocation":{"aim":{"kind":"actor_target","target":1},"weapon":{"kind":"default"}},
            "supplies":{"kind":"explicit","items":[{"cost_ordinal":0,"item":10}]}}});
    let decoded: Stored = serde_json::from_value(original.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), original);
    for mutation in 0..7 {
        let mut value = original.clone();
        match mutation {
            0 => {
                value["action"].as_object_mut().unwrap().remove("supplies");
            }
            1 => value["action"]["supplies"]["items"][0]["item"] = serde_json::json!("10"),
            2 => value["action"]["supplies"]["items"][0]["cost_ordinal"] = serde_json::json!(8),
            3 => value["action"]["supplies"]["items"] = serde_json::json!([]),
            4 => {
                value["action"]["supplies"]["items"] =
                    serde_json::json!(vec![serde_json::json!({"cost_ordinal":0,"item":10}); 33])
            }
            5 => value["action"]["supplies"]["items"][0]["held"] = serde_json::json!(1),
            6 => value["action"]["supplies"]["items"][0] = serde_json::json!([0, 10]),
            _ => unreachable!(),
        }
        assert!(
            serde_json::from_value::<Stored>(value).is_err(),
            "accepted corruption {mutation}"
        );
    }
}

#[test]
fn phase5_saved_carried_selection_rejects_surplus_rows() {
    assert!(serde_json::from_value::<SupplySelection>(
        serde_json::json!({"kind":"carried","items":[]})
    )
    .is_err());
}
#[test]
fn phase5_saved_selection_cannot_reuse_one_exact_item_across_costs() {
    assert!(serde_json::from_value::<SupplySelection>(
        serde_json::json!({"kind":"explicit","items":[
            {"cost_ordinal":0,"item":10},{"cost_ordinal":1,"item":10}
        ]})
    )
    .is_err());
}

fn check_stored<T: serde::de::DeserializeOwned + Serialize>(json: &str) {
    let decoded: T = super::super::codec::strict(json.as_bytes()).unwrap();
    assert_eq!(
        serde_json::to_value(decoded).unwrap(),
        serde_json::from_str::<serde_json::Value>(json).unwrap()
    );
}

#[test]
fn saved_note_anchors_keep_current_string_shapes_and_private_boundary() {
    use crate::journal::{Command, RecordedAnchor};
    use serde_json::json;

    // Independent current-save fixtures, including the byte-length boundary
    // and strings requiring JSON escaping. No wire serializer is the oracle.
    for snapshot in [
        "issued-snapshot".to_owned(),
        "snapshot \\\"é".to_owned(),
        "a".repeat(128),
        "é".repeat(64),
    ] {
        let command = Command::Annotate {
            attachments: Vec::new(),
            anchor: wire::Anchor::State {
                snapshot: wire::SnapshotId(snapshot.clone()),
            },
            text: "note".into(),
            source: wire::ClientSource::User,
            audience: wire::Audience::Private,
            category: wire::AnnotationCategory::Note,
        };
        let expected = json!({
            "type":"annotate", "anchor":{"type":"state", "snapshot":snapshot},
            "text":"note", "source":"user", "audience":"private", "category":"note"
        });
        assert_eq!(serde_json::to_value(&command).unwrap(), expected);
        assert_eq!(
            super::super::codec::strict::<Command>(&serde_json::to_vec(&expected).unwrap())
                .unwrap(),
            command
        );

        for boundary in [None, Some("private-causal-entry"), Some("entry \\\"é")] {
            let anchor = RecordedAnchor::State {
                snapshot: wire::SnapshotId(snapshot.clone()),
                boundary: boundary.map(|id| wire::EntryId(id.into())),
            };
            let expected = json!({"type":"state", "snapshot":snapshot, "boundary":boundary});
            assert_eq!(serde_json::to_value(&anchor).unwrap(), expected);
            assert_eq!(
                super::super::codec::strict::<RecordedAnchor>(
                    &serde_json::to_vec(&expected).unwrap()
                )
                .unwrap(),
                anchor
            );
            assert_eq!(
                anchor.to_public(),
                wire::Anchor::State {
                    snapshot: wire::SnapshotId(snapshot.clone())
                }
            );
        }
    }
    // Entry anchors retain the same independent string identity shape.
    check_stored::<RecordedAnchor>(r#"{"type":"entry","id":"retained-entry"}"#);
}

#[test]
fn saved_note_anchors_preserve_identity_validation_and_required_fields() {
    use crate::journal::{Command, RecordedAnchor};
    use serde_json::json;

    let command = json!({
        "type":"annotate", "anchor":{"type":"state", "snapshot":"issued-snapshot"},
        "text":"note", "source":"user", "audience":"private", "category":"note"
    });
    let recorded = json!({"type":"state", "snapshot":"issued-snapshot", "boundary":null});
    let command_ok = |value: &serde_json::Value| {
        super::super::codec::strict::<Command>(&serde_json::to_vec(value).unwrap()).is_ok()
    };
    let recorded_ok = |value: &serde_json::Value| {
        super::super::codec::strict::<RecordedAnchor>(&serde_json::to_vec(value).unwrap()).is_ok()
    };
    assert!(command_ok(&command));
    assert!(recorded_ok(&recorded));
    for invalid in [
        json!(""),
        json!("a".repeat(129)),
        json!("é".repeat(65)),
        json!("snapshot\nidentity"),
        json!("snapshot\u{7f}identity"),
        json!("snapshot\u{85}identity"),
        json!(null),
        json!(1),
        json!([]),
        json!({"id":"snapshot"}),
    ] {
        let mut value = command.clone();
        value["anchor"]["snapshot"] = invalid.clone();
        assert!(!command_ok(&value), "accepted receipt snapshot {invalid}");
        let mut value = recorded.clone();
        value["snapshot"] = invalid.clone();
        assert!(!recorded_ok(&value), "accepted recorded snapshot {invalid}");
    }
    let mut missing = command;
    missing["anchor"]
        .as_object_mut()
        .unwrap()
        .remove("snapshot");
    assert!(!command_ok(&missing));
    for field in ["snapshot", "boundary"] {
        let mut missing = recorded.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(!recorded_ok(&missing), "accepted missing {field}");
    }
    for invalid in [json!(1), json!([]), json!({"id":"entry"})] {
        let mut value = recorded.clone();
        value["boundary"] = invalid.clone();
        assert!(!recorded_ok(&value), "accepted recorded boundary {invalid}");
    }
    let mut extra = recorded;
    extra["entry"] = json!("unexpected");
    assert!(!recorded_ok(&extra));
}

#[test]
fn nested_journal_receipts_and_annotations_keep_save_owned_numbers() {
    check_stored::<crate::engine::Record>(
        r#"{
            "before_tick": 18446744073709551615,
            "after_tick": 18446744073709551615,
            "activations": [],
            "retrievals": [],
            "retrieval_before": null,
            "retrieval_after": null,
            "action_resolution": {"kind":"unchanged"},
            "entry": {
                "intention_suspensions": [], "intention_ends": [],
                "id": "entry", "branch": "branch", "actor": 18446744073709551615,
                "tick": 18446744073709551615,
                "author": {"type":"user", "user":"owner"}, "audience":"actor",
                "content": {"type":"intention_admitted", "intention":1,
                    "human_operands":{"weapon":{"kind":"natural"},"items":[],"preparation":null},
                    "action":{"type":"use_ability", "id":"basic_melee", "invocation":{"aim":{"kind":"actor_target","target":18446744073709551615},"weapon":{"kind":"default"}},"supplies":{"kind":"carried"}}}
            },
            "receipt": {"user":"owner", "frontend":"test", "request_id":"request",
                "actor":18446744073709551615, "branch":"branch",
                "command":{"type":"admit_intention", "expected_boundary":18446744073709551615,
                    "action":{"type":"use_ability", "id":"basic_melee", "invocation":{"aim":{"kind":"actor_target","target":18446744073709551615},"weapon":{"kind":"default"}},"supplies":{"kind":"carried"}}}}
        }"#,
    );
    check_stored::<crate::journal::Command>(
        r#"{
            "type":"annotate", "anchor":{"type":"state", "snapshot":"issued-snapshot"},
            "text":"note", "source":"user", "audience":"actor", "category":"note"
        }"#,
    );
    check_stored::<crate::journal::IntentionEnd>(
        r#"{
            "actor":18446744073709551615, "intention":1, "kind":"resolved"
        }"#,
    );
    check_stored::<crate::journal::IntentionSuspension>(
        r#"{
            "actor":18446744073709551615, "intention":1
        }"#,
    );
}

#[test]
fn metadata_identities_and_all_direction_variants_keep_stored_names() {
    for direction in [
        "north",
        "east",
        "south",
        "west",
        "north_east",
        "south_east",
        "south_west",
        "north_west",
        "up",
        "down",
    ] {
        check_stored::<Stored>(&format!(
            r#"{{"action":{{"type":"move","direction":"{direction}"}}}}"#
        ));
    }
    for target in ["null", r#""entry\\escaped\"identity""#] {
        check_stored::<crate::journal::Command>(&format!(
            r#"{{"type":"wizard","expected_boundary":1,"display_basis":null,"operation":{{"type":"rewind","target":{target}}}}}"#
        ));
    }
    for source in ["user", "frontend"] {
        for audience in ["private", "actor"] {
            for category in ["note", "bookmark", "explanation"] {
                check_stored::<crate::journal::Command>(&format!(
                    r#"{{"type":"annotate","anchor":{{"type":"entry","id":"identity"}},"text":"note","source":"{source}","audience":"{audience}","category":"{category}"}}"#
                ));
            }
        }
    }
    for author in [
        r#"{"type":"user","user":"owner"}"#,
        r#"{"type":"frontend","user":"owner","component":"client"}"#,
        r#"{"type":"backend","component":"scheduler"}"#,
    ] {
        check_stored::<crate::journal::JournalEntry>(&format!(
            r#"{{"intention_suspensions":[],"intention_ends":[],"id":"entry","branch":"branch","actor":1,"tick":0,"author":{author},"audience":"actor","content":{{"type":"annotation","anchor":{{"type":"state","snapshot":"issued-snapshot","boundary":null}},"category":"note","text":"note"}}}}"#
        ));
    }
}
#[test]
fn saved_weapon_selection_is_exact_numeric_named_and_independent_of_wire() {
    use crate::actions as backend;
    for (item, socket) in [(1, 0), (9_007_199_254_740_993, 63), (u64::MAX - 1, 1)] {
        let stored = Stored {
            action: backend::Action::UseAbility {
                id: tor_simulation::content_ids::AbilityId::new("basic_melee").unwrap(),
                invocation: backend::AbilityInvocation {
                    aim: backend::AbilityAim::ContactTarget { target: ActorId(2) },
                    weapon: backend::WeaponSelection::Equipped { item, socket },
                },
                supplies: Default::default(),
            },
        };
        let value = serde_json::to_value(&stored).unwrap();
        assert_eq!(
            value["action"]["invocation"],
            serde_json::json!({
                "aim":{"kind":"contact_target","target":2},
                "weapon":{"kind":"equipped","item":item,"socket":socket}
            })
        );
        assert_eq!(
            crate::storage::codec::strict::<Stored>(&serde_json::to_vec(&value).unwrap()).unwrap(),
            stored
        );
    }
    let aim = r#"{"kind":"actor_target","target":2}"#;
    for weapon in [
        r#""default""#,
        r#"["equipped",7,0]"#,
        r#"{"kind":"default","item":7}"#,
        r#"{"kind":"default","socket":0}"#,
        r#"{"kind":"equipped","item":7}"#,
        r#"{"kind":"equipped","socket":0}"#,
        r#"{"kind":"equipped","item":0,"socket":0}"#,
        r#"{"kind":"equipped","item":18446744073709551615,"socket":0}"#,
        r#"{"kind":"equipped","item":7,"socket":64}"#,
        r#"{"kind":"equipped","item":7,"socket":-1}"#,
        r#"{"kind":"equipped","item":7,"socket":0.0}"#,
        r#"{"kind":"equipped","item":7,"socket":"0"}"#,
        r#"{"kind":"equipped","item":"7","socket":0}"#,
        r#"{"kind":"equipped","item":7,"socket":0,"target":2}"#,
        r#"{"kind":"equipped","item":7,"item":8,"socket":0}"#,
        r#"{"kind":"equipped","item":7,"socket":0,"socket":1}"#,
        r#"{"kind":"equipped","kind":"default","item":7,"socket":0}"#,
    ] {
        let encoded = format!(r#"{{"aim":{aim},"weapon":{weapon}}}"#);
        assert!(
            crate::storage::codec::strict::<super::AbilityInvocation>(encoded.as_bytes()).is_err(),
            "{encoded}"
        );
    }
    for encoded in [
        format!(r#"{{"aim":{aim}}}"#),
        r#"{"weapon":{"kind":"default"}}"#.into(),
        format!(r#"{{"aim":{aim},"aim":{aim},"weapon":{{"kind":"default"}}}}"#),
        format!(r#"{{"aim":{aim},"weapon":{{"kind":"default"}},"weapon":{{"kind":"default"}}}}"#),
        format!(r#"{{"aim":{aim},"weapon":{{"kind":"default"}},"kind":"actor_target"}}"#),
        format!(r#"[{aim},{{"kind":"default"}}]"#),
        aim.into(),
    ] {
        assert!(
            crate::storage::codec::strict::<super::AbilityInvocation>(encoded.as_bytes()).is_err(),
            "{encoded}"
        );
    }
}
