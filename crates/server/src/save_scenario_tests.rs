#[test]
fn stored_seeded_item_pairs_and_appearance_entries_round_trip_independent_schema() {
    use super::{Borrowed, Owned};
    let item: source::Item =
        serde_json::from_value(serde_json::json!({"id":99,"at":[1,1,0],"seed_names":[
            {"name":"arbitrary label","identity":"item.misc.token"}
        ]}))
        .unwrap();
    let bytes = serde_json::to_vec(&Borrowed(&item)).unwrap();
    let Owned(restored) = crate::storage::codec::strict::<Owned<source::Item>>(&bytes).unwrap();
    assert_eq!(restored.seed_names[0].name, "arbitrary label");
    assert_eq!(restored.seed_names[0].identity.as_str(), "item.misc.token");
    let mut old = serde_json::to_value(Borrowed(&item)).unwrap();
    old["seed_names"] = serde_json::json!(["arbitrary label"]);
    assert!(serde_json::from_value::<Owned<source::Item>>(old).is_err());
    for asset in [Some("item.potion.amber".to_owned()), None] {
        let entry = source::AppearanceEntry {
            name: "public label".into(),
            asset,
        };
        let value = serde_json::to_value(Borrowed(&entry)).unwrap();
        let Owned(decoded) =
            serde_json::from_value::<Owned<source::AppearanceEntry>>(value.clone()).unwrap();
        assert_eq!(decoded, entry);
        let mut invalid = value;
        invalid["identity"] = serde_json::json!("item.potion.healing");
        assert!(serde_json::from_value::<Owned<source::AppearanceEntry>>(invalid).is_err());
    }
}

#[test]
fn stored_action_duration_modifier_roundtrips_exact_named_vocabulary() {
    use super::{Borrowed, Owned};
    for rate in ["normal", "haste", "slow"] {
        let value = serde_json::json!({"cancellable":false,"scope":"always","op":{"kind":"action_duration","value":rate}});
        let decoded: Owned<crate::creature_authoring::Modifier> =
            serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(Borrowed(&decoded.0)).unwrap(), value);
        assert!(decoded.0.compile().is_ok());
        for op in [
            serde_json::json!({"kind":"action_duration"}),
            serde_json::json!({"kind":"action_duration","value":"unknown"}),
            serde_json::json!({"kind":"action_duration","value":{(rate):null}}),
            serde_json::json!({"kind":"action_duration","value":[rate]}),
            serde_json::json!({"kind":"action_duration","value":null}),
            serde_json::json!({"kind":"action_duration","value":true}),
            serde_json::json!({"kind":"action_duration","value":1}),
            serde_json::json!({"kind":"action_duration","value":rate,"extra":true}),
        ] {
            let mut invalid = value.clone();
            invalid["op"] = op;
            assert!(
                serde_json::from_value::<Owned<crate::creature_authoring::Modifier>>(invalid)
                    .is_err()
            );
        }
    }
}

#[test]
fn stored_locomotion_requires_nullable_chance_and_bounded_named_fraction() {
    use super::{Borrowed, Owned};
    use serde_json::json;
    type Value = crate::creature_authoring::Locomotion;
    for value in [
        json!({"kind":"confused","chance":null}),
        json!({"kind":"confused","chance":{"numerator":4,"denominator":5}}),
        json!({"kind":"erratic","chance":null}),
        json!({"kind":"erratic","chance":{"numerator":1,"denominator":5}}),
    ] {
        let decoded: Owned<Value> = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(Borrowed(&decoded.0)).unwrap(), value);
    }
    for value in [
        json!({"kind":"confused"}),
        json!({"kind":"confused","chance":[4,5]}),
        json!({"kind":"confused","chance":{"numerator":5,"denominator":4}}),
        json!({"kind":"confused","chance":null,"enabled":true}),
        json!({"kind":"erratic"}),
        json!({"kind":"erratic","chance":[1,5]}),
        json!({"kind":"erratic","chance":{"numerator":2,"denominator":1}}),
        json!({"kind":"erratic","chance":{"numerator":0,"denominator":0}}),
        json!({"kind":"erratic","chance":{"numerator":1,"denominator":65536}}),
        json!({"kind":"erratic","chance":null,"enabled":true}),
        json!(["flying", true]),
    ] {
        assert!(serde_json::from_value::<Owned<Value>>(value).is_err());
    }
}

#[test]
fn stored_mutual_sight_requires_exact_named_range() {
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Record {
        #[serde(with = "super::AbilityReach")]
        reach: tor_simulation::rules::Reach,
    }
    for reach in [
        serde_json::json!({"kind":"melee"}),
        serde_json::json!({"kind":"sight","range":6}),
        serde_json::json!({"kind":"mutual_sight","range":4}),
        serde_json::json!({"kind":"self_only"}),
        serde_json::json!({"kind":"self_or_ally","range":{"kind":"touch"}}),
        serde_json::json!({"kind":"self_or_ally","range":{"kind":"sight","range":3}}),
    ] {
        let good = serde_json::json!({"reach":reach});
        let decoded: Record = serde_json::from_value(good.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), good);
    }
    for reach in [
        serde_json::json!({"kind":"mutual_sight"}),
        serde_json::json!({"kind":"mutual_sight","range":4,"awareness":true}),
        serde_json::json!(["mutual_sight", 4]),
        serde_json::json!(["melee"]),
        serde_json::json!(["sight", 6]),
        serde_json::json!(["self_only"]),
        serde_json::json!(["self_or_ally", {"kind":"touch"}]),
        serde_json::json!({"kind":"melee","range":1}),
        serde_json::json!({"kind":"self_only","range":0}),
        serde_json::json!({"kind":"sight"}),
        serde_json::json!({"kind":"self_or_ally"}),
        serde_json::json!({"kind":"self_or_ally","range":["touch"]}),
        serde_json::json!({"kind":"self_or_ally","range":["sight",3]}),
        serde_json::json!({"kind":"self_or_ally","range":{"kind":"touch","range":1}}),
        serde_json::json!({"kind":"self_or_ally","range":{"kind":"sight"}}),
    ] {
        assert!(
            serde_json::from_value::<Record>(serde_json::json!({"reach":reach})).is_err(),
            "malformed stored reach accepted: {reach}"
        );
    }
}

#[test]
fn sequence_rider_stored_source_and_rules_require_independent_bounded_conditions() {
    use super::{Borrowed, Owned};
    use serde_json::json;
    let good = json!({"cancellable":false,"trigger":"success",
            "condition":{"kind":"sequence_successful_hits","minimum":2},
            "check":null,"effects":[],"descriptors":[]});
    for value in [good.clone(), {
        let mut ordinary = good.clone();
        ordinary["condition"] = json!(null);
        ordinary
    }] {
        let source: Owned<crate::creature_authoring::Rider> =
            serde_json::from_value(value.clone()).unwrap();
        let rules: Owned<tor_simulation::rules::RiderDef> =
            serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(Borrowed(&source.0)).unwrap(), value);
        assert_eq!(serde_json::to_value(Borrowed(&rules.0)).unwrap(), value);
    }
    let mut malformed = Vec::new();
    let mut omitted = good.clone();
    omitted.as_object_mut().unwrap().remove("condition");
    malformed.push(omitted);
    for condition in [
        json!({"kind":"sequence_successful_hits"}),
        json!({"kind":"sequence_successful_hits","minimum":2,"group":"arbitrary"}),
        json!({"kind":"any_effect","minimum":2}),
        json!(["sequence_successful_hits", 2]),
    ]
    .into_iter()
    .chain(
        [0, 17, 65536].map(|minimum| json!({"kind":"sequence_successful_hits","minimum":minimum})),
    ) {
        let mut bad = good.clone();
        bad["condition"] = condition;
        malformed.push(bad);
    }
    for bad in malformed {
        assert!(
            serde_json::from_value::<Owned<crate::creature_authoring::Rider>>(bad.clone()).is_err()
        );
        assert!(serde_json::from_value::<Owned<tor_simulation::rules::RiderDef>>(bad).is_err());
    }
    let mut failed_gate = good;
    failed_gate["trigger"] = json!("failure");
    let source = json!({"kind":"natural_melee","additional_riders":[failed_gate]});
    assert!(serde_json::from_value::<Owned<tor_simulation::rules::StrikeSource>>(source).is_err());
}

#[test]
fn sequence_rider_stored_conditions_reject_duplicate_and_positional_records() {
    use super::Owned;
    let good = r#"{"cancellable":false,"trigger":"success","condition":{"kind":"sequence_successful_hits","minimum":2},"check":null,"effects":[],"descriptors":[]}"#;
    assert!(serde_json::from_str::<Owned<crate::creature_authoring::Rider>>(good).is_ok());
    assert!(serde_json::from_str::<Owned<tor_simulation::rules::RiderDef>>(good).is_ok());
    let duplicates = [
        good.replacen("\"condition\":", "\"condition\":null,\"condition\":", 1),
        good.replace("\"minimum\":2", "\"minimum\":2,\"minimum\":3"),
        good.replace(
            "\"kind\":\"sequence_successful_hits\"",
            "\"kind\":\"sequence_successful_hits\",\"kind\":\"sequence_successful_hits\"",
        ),
    ];
    for bad in duplicates {
        assert!(serde_json::from_str::<Owned<crate::creature_authoring::Rider>>(&bad).is_err());
        assert!(serde_json::from_str::<Owned<tor_simulation::rules::RiderDef>>(&bad).is_err());
    }
    let positional =
        r#"[false,"success",{"kind":"sequence_successful_hits","minimum":2},null,[],[]]"#;
    assert!(serde_json::from_str::<Owned<crate::creature_authoring::Rider>>(positional).is_err());
    assert!(serde_json::from_str::<Owned<tor_simulation::rules::RiderDef>>(positional).is_err());
}

#[test]
fn stored_generation_operations_reject_positional_outer_aliases() {
    use super::{Borrowed, Owned};
    type Operation = crate::generation_recipe::Operation;
    // Unit, ordinary field-bearing and new nested-config variants all use
    // the same independent save-owned tagged-map boundary.
    for (good, positional) in [
        (r#"{"kind":"stone_fill"}"#, r#"["stone_fill"]"#),
        (
            r#"{"kind":"rooms","width":[4,6],"height":[3,5]}"#,
            r#"["rooms",[4,6],[3,5]]"#,
        ),
        (
            r#"{"kind":"room_omissions","config":{"count":[0,3]}}"#,
            r#"["room_omissions",{"count":[0,3]}]"#,
        ),
    ] {
        let Owned(operation) =
            crate::storage::codec::strict::<Owned<Operation>>(good.as_bytes()).unwrap();
        assert_eq!(
            serde_json::to_value(Borrowed(&operation)).unwrap(),
            serde_json::from_str::<serde_json::Value>(good).unwrap()
        );
        assert!(serde_json::from_str::<Owned<Operation>>(positional).is_err());
        assert!(crate::storage::codec::strict::<Owned<Operation>>(positional.as_bytes()).is_err());
    }
}

#[test]
fn stored_room_omissions_require_exact_named_config() {
    use super::{Borrowed, Owned};
    type Operation = crate::generation_recipe::Operation;
    let good = r#"{"kind":"room_omissions","config":{"count":[0,3]}}"#;
    let Owned(operation) = serde_json::from_str::<Owned<Operation>>(good).unwrap();
    assert_eq!(
        serde_json::to_value(Borrowed(&operation)).unwrap(),
        serde_json::from_str::<serde_json::Value>(good).unwrap()
    );
    // Each negative starts from the independently authored accepted shape.
    for bad in [
        r#"{"kind":"room_omissions"}"#,
        r#"{"kind":"room_omissions","config":null}"#,
        r#"{"kind":"room_omissions","config":{}}"#,
        r#"{"kind":"room_omissions","config":{"count":[0,3],"surprise":0}}"#,
        r#"{"kind":"room_omissions","config":{"count":[0,3]},"surprise":0}"#,
        r#"{"kind":"room_omissions","config":[[0,3]]}"#,
        r#"["room_omissions",{"count":[0,3]}]"#,
        r#"{"kind":"room_omissions","config":{"count":[0]}}"#,
        r#"{"kind":"room_omissions","config":{"count":[0,3,4]}}"#,
        r#"{"kind":"room_omissions","config":{"count":[false,3]}}"#,
        r#"{"kind":"room_omissions","config":{"count":[-1,3]}}"#,
        r#"{"kind":"room_omissions","config":{"count":[0,4294967296]}}"#,
        r#"{"kind":"room_omissions","config":{"count":[0,3],"count":[0,3]}}"#,
        r#"{"kind":"room_omissions","config":{"count":[0,3]},"config":{"count":[0,3]}}"#,
        r#"{"kind":"room_omissions","kind":"room_omissions","config":{"count":[0,3]}}"#,
    ] {
        assert!(
            serde_json::from_str::<Owned<Operation>>(bad).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn stored_room_omission_recipe_preserves_bounds_and_stage_identity() {
    use super::{Borrowed, Owned};
    use crate::generation_recipe::{check, Recipe};
    let good = serde_json::json!({"version":4,"stages":[
        {"id":"stone","version":1,"operation":{"kind":"stone_fill"}},
        {"id":"slots","version":1,"operation":{"kind":"grid_partition"}},
        {"id":"rooms","version":1,"operation":{"kind":"rooms","width":[4,6],"height":[3,5]}},
        {"id":"connections","version":1,"operation":{"kind":"connected_graph","extra":[0,1]}},
        {"id":"corridors","version":1,"operation":{"kind":"corridors"}},
        {"id":"stairs","version":1,"operation":{"kind":"stairs"}},
        {"id":"room_omissions","version":1,"operation":{"kind":"room_omissions","config":{"count":[0,3]}}}
    ]});
    let bytes = serde_json::to_vec(&good).unwrap();
    let Owned(recipe) = crate::storage::codec::strict::<Owned<Recipe>>(&bytes).unwrap();
    check(&recipe).unwrap();
    assert_eq!(serde_json::to_value(Borrowed(&recipe)).unwrap(), good);
    for (path, value) in [
        ("/version", serde_json::json!(3)),
        ("/stages/6/version", serde_json::json!(2)),
        ("/stages/6/id", serde_json::json!("stairs")),
        (
            "/stages/6/operation/config/count",
            serde_json::json!([3, 2]),
        ),
        (
            "/stages/6/operation/config/count",
            serde_json::json!([0, 64]),
        ),
    ] {
        let mut invalid = good.clone();
        *invalid.pointer_mut(path).unwrap() = value;
        let Owned(recipe) = serde_json::from_value::<Owned<Recipe>>(invalid).unwrap();
        assert!(check(&recipe).is_err(), "{path}");
    }
}

#[test]
fn stored_maze_rooms_require_exact_named_config() {
    use super::{Borrowed, Owned};
    type Operation = crate::generation_recipe::Operation;
    let good = r#"{"kind":"maze_rooms","config":{"one_in":1}}"#;
    let Owned(operation) =
        crate::storage::codec::strict::<Owned<Operation>>(good.as_bytes()).unwrap();
    assert_eq!(
        serde_json::to_value(Borrowed(&operation)).unwrap(),
        serde_json::from_str::<serde_json::Value>(good).unwrap()
    );
    for bad in [
        r#"{"kind":"maze_rooms"}"#,
        r#"{"kind":"maze_rooms","config":null}"#,
        r#"{"kind":"maze_rooms","config":{}}"#,
        r#"{"kind":"maze_rooms","config":{"one_in":1,"surprise":0}}"#,
        r#"{"kind":"maze_rooms","config":{"one_in":1},"surprise":0}"#,
        r#"{"kind":"maze_rooms","config":[1]}"#,
        r#"["maze_rooms",{"one_in":1}]"#,
        r#"{"kind":"maze_rooms","config":{"one_in":false}}"#,
        r#"{"kind":"maze_rooms","config":{"one_in":-1}}"#,
        r#"{"kind":"maze_rooms","config":{"one_in":4294967296}}"#,
        r#"{"kind":"maze_rooms","config":{"one_in":1.0}}"#,
        r#"{"kind":"maze_rooms","config":{"one_in":1,"one_in":1}}"#,
        r#"{"kind":"maze_rooms","config":{"one_in":1},"config":{"one_in":1}}"#,
        r#"{"kind":"maze_rooms","kind":"maze_rooms","config":{"one_in":1}}"#,
    ] {
        assert!(
            serde_json::from_str::<Owned<Operation>>(bad).is_err(),
            "{bad}"
        );
        assert!(
            crate::storage::codec::strict::<Owned<Operation>>(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn stored_maze_recipe_preserves_lighting_order_and_version_ownership() {
    use super::{Borrowed, Owned};
    use crate::generation_recipe::{check, Recipe};
    let good = serde_json::json!({"version":5,"stages":[
        {"id":"stone","version":1,"operation":{"kind":"stone_fill"}},
        {"id":"slots","version":1,"operation":{"kind":"grid_partition"}},
        {"id":"rooms","version":1,"operation":{"kind":"rooms","width":[4,6],"height":[3,5]}},
        {"id":"connections","version":1,"operation":{"kind":"connected_graph","extra":[0,1]}},
        {"id":"corridors","version":1,"operation":{"kind":"corridors"}},
        {"id":"stairs","version":1,"operation":{"kind":"stairs"}},
        {"id":"maze_rooms","version":1,"operation":{"kind":"maze_rooms","config":{"one_in":1}}},
        {"id":"lighting","version":1,"operation":{"kind":"room_lighting","darkness_roll":10,"darkness_start":1}}
    ]});
    let Owned(recipe) =
        crate::storage::codec::strict::<Owned<Recipe>>(&serde_json::to_vec(&good).unwrap())
            .unwrap();
    check(&recipe).unwrap();
    assert_eq!(serde_json::to_value(Borrowed(&recipe)).unwrap(), good);
    for (path, value) in [
        ("/version", serde_json::json!(4)),
        ("/version", serde_json::json!(6)),
        ("/stages/6/version", serde_json::json!(2)),
        ("/stages/6/id", serde_json::json!("stairs")),
        ("/stages/6/operation/config/one_in", serde_json::json!(0)),
        (
            "/stages/6/operation/config/one_in",
            serde_json::json!(10001),
        ),
    ] {
        let mut invalid = good.clone();
        *invalid.pointer_mut(path).unwrap() = value;
        let Owned(recipe) = serde_json::from_value::<Owned<Recipe>>(invalid).unwrap();
        assert!(check(&recipe).is_err(), "{path}");
    }
    for mutation in 0..4 {
        let mut invalid = good.clone();
        let stages = invalid["stages"].as_array_mut().unwrap();
        match mutation {
            0 => {
                stages.remove(7);
            }
            1 => {
                stages.remove(6);
            }
            2 => stages.swap(6, 7),
            3 => {
                let mut duplicate = stages[6].clone();
                duplicate["id"] = serde_json::json!("other_maze");
                stages.insert(7, duplicate);
            }
            _ => unreachable!(),
        }
        let Owned(recipe) = serde_json::from_value::<Owned<Recipe>>(invalid).unwrap();
        assert!(check(&recipe).is_err(), "mutation {mutation}");
    }
}

#[test]
fn discovery_formats_match_independent_payload_boundaries() {
    assert_eq!(
        (
            crate::engine::ARCHIVE_VERSION,
            crate::scenario_package::FORMAT,
            crate::scenario_package::RULESET,
            super::RULES_INPUT_FORMAT,
            super::REGION_INPUT_FORMAT,
            super::CONTENT_PACKAGE_FORMAT,
            crate::arena_evaluation::FORMAT,
        ),
        (75, 38, "interactions-v73", 27, 10, 35, "tor-arena-run-v6")
    );
}

#[test]
fn discovery_formats_preserve_physical_envelopes() {
    assert_eq!(tor_protocol::PROTOCOL_VERSION, 78);
    assert_eq!(super::RULES_INPUT_FORMAT, 27);
    assert_eq!(super::REGION_INPUT_FORMAT, 10);
    // Semantic rule, region and package payload versions advance independently
    // of their physical framing, which remains unchanged.
    let base = crate::storage::codec::frame(0, 0, &serde_json::json!({})).unwrap();
    assert_eq!(&base[..6], b"TORB\x06\x00");
    let journal = crate::storage::codec::frame(1, 0, &serde_json::json!({})).unwrap();
    assert_eq!(&journal[..6], b"TORJ\x06\x00");
    let content = crate::Scenario::two_room(42).content().unwrap();
    let record = tor_simulation::Game::two_room_in_stone(42, content)
        .into_region_record(tor_world::RegionId(2))
        .unwrap();
    let region = crate::storage::encode_region(1, &record).unwrap();
    assert_eq!(&region[..6], b"TORR\x06\x00");
    // The bounded zlib envelope still wraps an independently validated JSON
    // payload. Its physical representation has not changed.
    let checkpoint =
        crate::storage::checkpoint_codec::write_json(&serde_json::json!({}), Vec::new()).unwrap();
    assert_eq!(&checkpoint[..8], b"TORC\x01\x00\x00\x00");
}

#[test]
fn natural_melee_compiled_rules_keep_explicit_riders_and_reject_positional_records() {
    let mut document: tor_simulation::rules::RulesetDocument =
        toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
    let ability = document
        .abilities
        .iter_mut()
        .find(|ability| ability.basic_attack)
        .unwrap();
    ability.strikes = tor_simulation::rules::StrikeSource::NaturalMelee {
        additional_riders: vec![tor_simulation::rules::RiderDef {
            cancellable: false,
            condition: None,
            trigger: tor_simulation::rules::RiderTrigger::Success,
            check: None,
            effects: vec![tor_simulation::rules::Effect::ApplyCondition {
                condition: tor_simulation::content_ids::ConditionId::new("rattled").unwrap(),
                duration: 100,
            }],
            descriptors: vec![],
        }],
    };
    let encoded = encode_rules_document(&document).unwrap();
    let restored = decode_rules_document(&encoded).unwrap();
    assert_eq!(encode_rules_document(&restored).unwrap(), encoded);
    assert_eq!(
        restored
            .abilities
            .iter()
            .find(|ability| ability.basic_attack)
            .unwrap()
            .strikes,
        document
            .abilities
            .iter()
            .find(|ability| ability.basic_attack)
            .unwrap()
            .strikes,
    );
    let mut value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    let source = value["document"]["abilities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|ability| ability["basic_attack"] == true)
        .unwrap();
    source["strikes"]["additional_riders"][0] =
        serde_json::json!([false, "success", null, null, [], []]);
    assert!(decode_rules_document(&serde_json::to_string(&value).unwrap()).is_err());
    let mut obsolete: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    obsolete["format"] = serde_json::json!(11);
    assert!(decode_rules_document(&serde_json::to_string(&obsolete).unwrap()).is_err());
}

#[test]
fn phase5_stored_touch_range_rejects_surplus_fields() {
    #[derive(Deserialize)]
    struct Range {
        #[serde(with = "AllyRange")]
        range: tor_simulation::rules::AllyRange,
    }
    let touch =
        serde_json::from_value::<Range>(serde_json::json!({"range":{"kind":"touch"}})).unwrap();
    assert_eq!(touch.range, tor_simulation::rules::AllyRange::Touch);
    assert!(serde_json::from_value::<Range>(
        serde_json::json!({"range":{"kind":"touch","range":1}})
    )
    .is_err());
}
#[test]
fn phase5_stored_supply_cost_requires_a_named_object() {
    use tor_simulation::rules::SupplyCost;
    assert!(serde_json::from_str::<super::Owned<SupplyCost>>(
        r#"{"supply":"bandage","quantity":1,"access":"carried_only","spend_at":"first_resolution"}"#
    )
    .is_ok());
    assert!(serde_json::from_str::<super::Owned<SupplyCost>>(
        r#"["bandage",1,"carried_only","first_resolution"]"#
    )
    .is_err());
}
#[test]
fn phase5_stored_supply_definition_requires_a_named_object() {
    use tor_simulation::rules::SupplyDef;
    assert!(serde_json::from_str::<super::Owned<SupplyDef>>(
        r#"{"id":"bandage","name":"Bandage"}"#
    )
    .is_ok());
    assert!(serde_json::from_str::<super::Owned<SupplyDef>>(r#"["bandage","Bandage"]"#).is_err());
}

#[test]
fn stored_behavior_names_every_disposition_and_compiles_it() {
    use tor_simulation::behavior::Disposition;
    let decode = |name: &str| {
        let value = serde_json::json!({"disposition":name,"idle":"wait","memory_ticks":1000,"doors":false,"care":false,"equipment":false,"loot":false,"gold":true,"flee_percent":25});
        serde_json::from_slice::<super::Owned<crate::creature_authoring::BehaviorPolicy>>(
            &serde_json::to_vec(&value).unwrap(),
        )
    };
    for (name, compiled) in [
        ("passive", Disposition::Passive),
        ("mean", Disposition::Mean),
        ("greedy", Disposition::Greedy),
    ] {
        let policy = decode(name).unwrap().0.compile();
        assert_eq!(policy.disposition, compiled, "{name}");
        assert!(policy.gold);
    }
    assert!(decode("greed").is_err());
}

#[test]
fn stored_behavior_and_interaction_have_independent_required_shapes() {
    let behavior = serde_json::json!({"disposition":"passive","idle":"wait","memory_ticks":1000,"doors":false,"care":false,"equipment":false,"loot":false,"gold":false,"flee_percent":25});
    let decode = |value: &serde_json::Value| {
        serde_json::from_slice::<super::Owned<crate::creature_authoring::BehaviorPolicy>>(
            &serde_json::to_vec(value).unwrap(),
        )
    };
    assert!(decode(&behavior).is_ok());
    for percent in [0, 100, 101, u32::MAX] {
        let mut threshold = behavior.clone();
        threshold["flee_percent"] = serde_json::json!(percent);
        assert_eq!(decode(&threshold).is_ok(), percent <= 100);
    }
    for field in [
        "disposition",
        "idle",
        "memory_ticks",
        "doors",
        "care",
        "equipment",
        "loot",
        "gold",
        "flee_percent",
    ] {
        let mut missing = behavior.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(decode(&missing).is_err());
    }
    for malformed in [
        serde_json::json!(["passive", "wait", 1000]),
        serde_json::json!({"disposition":{"passive":null},"idle":"wait","memory_ticks":1000}),
    ] {
        assert!(decode(&malformed).is_err());
    }
    for encoded in [r#""non_hostile""#, r#""hostile_to_target""#] {
        assert!(
            serde_json::from_str::<super::Owned<tor_simulation::rules::Interaction>>(encoded)
                .is_ok()
        );
    }
    for encoded in [
        r#"{"non_hostile":null}"#,
        r#"{"kind":"hostile_to_target"}"#,
        r#""unknown""#,
    ] {
        assert!(
            serde_json::from_str::<super::Owned<tor_simulation::rules::Interaction>>(encoded)
                .is_err()
        );
    }
    let document: tor_simulation::rules::RulesetDocument =
        toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
    let ability = document.abilities[0].clone();
    let mut stored = serde_json::to_value(super::Borrowed(&ability)).unwrap();
    stored.as_object_mut().unwrap().remove("interaction");
    assert!(
        serde_json::from_value::<super::Owned<tor_simulation::rules::AbilityDef>>(stored).is_err()
    );
}

#[test]
fn stored_partial_ai_profiles_require_nullable_fields_and_bounded_values() {
    let empty = serde_json::json!({"idle":null,"doors":null,"care":null,"equipment":null,"loot":null,"gold":null,"flee_percent":null});
    let decode = |value: &serde_json::Value| {
        crate::storage::codec::strict::<super::Owned<crate::scenario_package::AiProfile>>(
            &serde_json::to_vec(value).unwrap(),
        )
    };
    assert!(decode(&empty).is_ok());
    for field in [
        "idle",
        "doors",
        "care",
        "equipment",
        "loot",
        "gold",
        "flee_percent",
    ] {
        let mut missing = empty.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            decode(&missing).is_err(),
            "stored {field} must not use source defaults"
        );
    }
    let explicit = serde_json::json!({"idle":"wait","doors":false,"care":false,"equipment":false,"loot":false,"gold":false,"flee_percent":0});
    let profile = decode(&explicit).unwrap().0;
    assert_eq!(
        serde_json::to_value(super::Borrowed(&profile)).unwrap(),
        explicit
    );
    let mut invalid = explicit;
    invalid["flee_percent"] = serde_json::json!(101);
    assert!(decode(&invalid).is_err());
    let mut invalid = empty;
    invalid["extra"] = serde_json::json!(false);
    assert!(decode(&invalid).is_err());
}

#[test]
fn independent_stored_supply_adapters_reject_missing_unknown_and_unbounded_fields() {
    use tor_simulation::rules::{AbilityDef, RulesetDocument, SupplyCost};
    let document: RulesetDocument =
        toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
    let ability = &document.abilities[0];
    let original = serde_json::to_value(super::Borrowed(ability)).unwrap();
    let decoded: super::Owned<AbilityDef> = serde_json::from_value(original.clone()).unwrap();
    assert_eq!(&decoded.0, ability);
    assert!(original.get("cooldown").unwrap().is_null());
    for field in ["cooldown", "supply_costs"] {
        let mut value = original.clone();
        value.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<super::Owned<AbilityDef>>(value).is_err());
    }
    let row = serde_json::json!({"supply":"bandage", "quantity":1, "access":"carried_only", "spend_at":"first_resolution"});
    assert!(serde_json::from_value::<super::Owned<SupplyCost>>(row.clone()).is_ok());
    for field in ["supply", "quantity", "access", "spend_at"] {
        let mut value = row.clone();
        value.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<super::Owned<SupplyCost>>(value).is_err());
    }
    let mut value = row.clone();
    value["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<super::Owned<SupplyCost>>(value).is_err());
    let mut value = original;
    value["supply_costs"] = serde_json::json!(vec![row; 9]);
    assert!(serde_json::from_value::<super::Owned<AbilityDef>>(value).is_err());
}
#[test]
fn outgoing_edge_policy_is_required_and_strict_in_independent_stored_rules() {
    let document: tor_simulation::rules::RulesetDocument =
        toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
    let encoded = super::encode_rules_document(&document).unwrap();
    let original: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    for kind in ["ordinary", "ignore_negative"] {
        let mut document = document.clone();
        document.abilities[0].outgoing_edge_policy =
            serde_json::from_value(serde_json::json!({"kind":kind})).unwrap();
        let encoded = super::encode_rules_document(&document).unwrap();
        let decoded = super::decode_rules_document(&encoded).unwrap();
        assert_eq!(super::encode_rules_document(&decoded).unwrap(), encoded);
    }
    let mut missing = original.clone();
    missing["document"]["abilities"][0]
        .as_object_mut()
        .unwrap()
        .remove("outgoing_edge_policy");
    assert!(super::decode_rules_document(&serde_json::to_string(&missing).unwrap()).is_err());
    assert!(
        serde_json::from_value::<super::Owned<tor_simulation::rules::AbilityDef>>(
            missing["document"]["abilities"][0].clone()
        )
        .is_err()
    );
    for invalid in [
        serde_json::json!(null),
        serde_json::json!("ordinary"),
        serde_json::json!(["ordinary"]),
        serde_json::json!({}),
        serde_json::json!({"kind":"unknown"}),
        serde_json::json!({"kind":"ordinary","extra":true}),
        serde_json::json!({"kind":"ignore_negative","extra":true}),
    ] {
        let mut value = original.clone();
        value["document"]["abilities"][0]["outgoing_edge_policy"] = invalid;
        assert!(super::decode_rules_document(&serde_json::to_string(&value).unwrap()).is_err());
        assert!(
            serde_json::from_value::<super::Owned<tor_simulation::rules::AbilityDef>>(
                value["document"]["abilities"][0].clone()
            )
            .is_err(),
            "the independent decoder rejects shape before canonical byte comparison"
        );
    }
}
#[test]
fn compiled_supply_rules_require_explicit_owned_fields() {
    let document = toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
    let encoded = super::encode_rules_document(&document).unwrap();
    let restored = super::decode_rules_document(&encoded).unwrap();
    assert_eq!(super::encode_rules_document(&restored).unwrap(), encoded);
    let original: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    for field in ["cooldown", "supply_costs"] {
        let mut value = original.clone();
        value["document"]["abilities"][0]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(super::decode_rules_document(&serde_json::to_string(&value).unwrap()).is_err());
    }
    let mut value = original.clone();
    value["document"]
        .as_object_mut()
        .unwrap()
        .remove("supplies");
    assert!(super::decode_rules_document(&serde_json::to_string(&value).unwrap()).is_err());
    let mut value = original.clone();
    value["document"]["abilities"][0]["cooldown"] = serde_json::json!(0);
    assert!(super::decode_rules_document(&serde_json::to_string(&value).unwrap()).is_err());
    let mut value = original;
    value["document"]["abilities"][0]["supply_costs"] = serde_json::json!(vec![
        serde_json::json!({"supply":"bandage", "quantity":1, "access":"carried_only", "spend_at":"first_resolution"});
        9
    ]);
    assert!(super::decode_rules_document(&serde_json::to_string(&value).unwrap()).is_err());
}
#[test]
fn stored_check_only_edge_stats_round_trip_strictly() {
    use crate::creature_authoring::Stat;
    for stat in [
        Stat::CheckEdge,
        Stat::IncomingEdge { defense: None },
        Stat::IncomingEdge {
            defense: Some("physical".into()),
        },
    ] {
        let bytes = serde_json::to_vec(&super::Borrowed(&stat)).unwrap();
        let decoded = strict::<super::Owned<Stat>>(&bytes).unwrap();
        assert_eq!(decoded.0, stat);
        let mut forged = serde_json::to_value(super::Borrowed(&stat)).unwrap();
        forged["private_owner"] = serde_json::json!(1);
        assert!(strict::<super::Owned<Stat>>(&serde_json::to_vec(&forged).unwrap()).is_err());
    }
}

use super::*;
use crate::storage::codec::strict;
use serde_json::Value;

#[test]
fn saved_talent_protection_uses_the_independent_named_category_schema() {
    use crate::creature_authoring::{Modifier, Op, Scope};
    let modifier = Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: Op::TalentReduction {
            category: "impact".into(),
            amount: 2,
        },
    };
    let bytes = serde_json::to_vec(&Borrowed(&modifier)).unwrap();
    let Owned(restored) = strict::<Owned<Modifier>>(&bytes).unwrap();
    assert_eq!(restored, modifier);
    assert!(restored.compile().is_ok());
    for malformed in [
        r#"["always",{"kind":"talent_reduction","category":"impact","amount":2}]"#,
        r#"{"scope":"always","op":{"kind":"talent_reduction","category":"impact"}}"#,
        r#"{"scope":"always","op":{"kind":"talent_reduction","category":"impact","category":"keen","amount":2}}"#,
        r#"{"scope":"always","op":{"kind":"talent_reduction","category":"impact","amount":2,"unknown":true}}"#,
        r#"{"scope":"always","op":{"kind":"talent_reduction","selector":{"kind":"descriptor","descriptor":"fire"},"amount":2}}"#,
    ] {
        assert!(
            strict::<Owned<Modifier>>(malformed.as_bytes()).is_err(),
            "{malformed}"
        );
    }
    for (category, amount) in [("unknown", 2), ("impact", 1_000_001)] {
        let invalid = Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: Op::TalentReduction {
                category: category.into(),
                amount,
            },
        };
        assert!(invalid.compile().is_err());
    }
}

#[test]
fn saved_recovery_stat_modifiers_round_trip_through_the_independent_schema() {
    use crate::creature_authoring::{Modifier, Op, Scope, Stat};
    for (stat, amount, kind) in [
        (Stat::Health, 10, "health"),
        (Stat::HealthRecoveryPeriod, 40, "health_recovery_period"),
        (
            Stat::HealthRecoveryPeriodPercent,
            -25,
            "health_recovery_period_percent",
        ),
        (
            Stat::HealthRecoveryResetPercent,
            -50,
            "health_recovery_reset_percent",
        ),
    ] {
        let modifier = Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: Op::Add {
                stat,
                value: amount,
            },
        };
        let value = serde_json::to_value(Borrowed(&modifier)).unwrap();
        assert_eq!(value["op"]["stat"], serde_json::json!({"kind": kind}));
        let Owned(restored) =
            strict::<Owned<Modifier>>(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(restored, modifier);
        let text = serde_json::to_string(&value).unwrap();
        let tag = format!("\"kind\":\"{kind}\"");
        assert_eq!(text.matches(&tag).count(), 1);
        let duplicate = text.replacen(&tag, &format!("{tag},{tag}"), 1);
        assert_ne!(duplicate, text);
        assert!(strict::<Owned<Modifier>>(duplicate.as_bytes()).is_err());
        let mut unknown = value.clone();
        unknown["op"]["stat"]["private_owner"] = 999.into();
        assert!(strict::<Owned<Modifier>>(&serde_json::to_vec(&unknown).unwrap()).is_err());
        let mut missing = value;
        missing["op"]["stat"]
            .as_object_mut()
            .unwrap()
            .remove("kind");
        assert!(strict::<Owned<Modifier>>(&serde_json::to_vec(&missing).unwrap()).is_err());
    }
}

#[test]
fn procedural_family_save_golden_and_strict_boundaries() {
    use crate::procedural_world::{Domain, FamilyKind, WorldFamily};
    let expected: BTreeMap<String, Value> =
        serde_json::from_str(include_str!("../fixtures/saved-world-families.json")).unwrap();
    for (name, domain) in [
        ("finite", Domain::Finite { count: 7 }),
        ("seeded_finite", Domain::SeededFinite { min: 2, max: 9 }),
        (
            "open",
            Domain::Open {
                max_depth: 1_000_000,
            },
        ),
    ] {
        let family = WorldFamily {
            gravity: None,
            kind: FamilyKind::GridChain,
            version: 1,
            base_region: 100,
            domain,
            grid: [2, 3],
            size: [10, 8, 2],
            recipe: "rooms".into(),
            zone: None,
        };
        let value = serde_json::to_value(Borrowed(&family)).unwrap();
        assert_eq!(value, expected[name]);
        let bytes = serde_json::to_vec(&value).unwrap();
        let Owned(restored) = strict::<Owned<WorldFamily>>(&bytes).unwrap();
        assert_eq!(restored, family);
        for pointer in ["", "/domain"] {
            let mut unknown = value.clone();
            unknown
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unexpected".into(), Value::Bool(true));
            assert!(strict::<Owned<WorldFamily>>(&serde_json::to_vec(&unknown).unwrap()).is_err());
            for field in value.pointer(pointer).unwrap().as_object().unwrap().keys() {
                let mut missing = value.clone();
                missing
                    .pointer_mut(pointer)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
                assert!(
                    strict::<Owned<WorldFamily>>(&serde_json::to_vec(&missing).unwrap()).is_err(),
                    "{name}: accepted missing {pointer}/{field}"
                );
            }
        }
        let text = serde_json::to_string(&value).unwrap();
        let duplicate = text.replacen("\"version\":1", "\"version\":1,\"version\":1", 1);
        assert_ne!(duplicate, text);
        assert!(strict::<Owned<WorldFamily>>(duplicate.as_bytes()).is_err());
        let domain_kind = format!("\"kind\":\"{name}\"");
        let duplicate = text.replacen(&domain_kind, &format!("{domain_kind},{domain_kind}"), 1);
        assert_ne!(duplicate, text);
        assert!(strict::<Owned<WorldFamily>>(duplicate.as_bytes()).is_err());
    }
}

#[test]
fn compiled_rules_catalog_order_is_irrelevant_but_effect_order_is_preserved() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let package = source::read_package(&root).unwrap();
    let mut document = package.rules().unwrap().document().clone();
    let second_pool = tor_simulation::content_ids::PointPoolId::new("secondary").unwrap();
    document
        .point_pools
        .push(tor_simulation::rules::PointPoolDef {
            id: second_pool.clone(),
            name: "Secondary".into(),
            allowed: vec![tor_simulation::rules::PurchaseKind::SkillRank],
        });
    document
        .constants
        .racial_point_grants
        .push(tor_simulation::rules::PointGrant {
            pool: second_pool,
            points: 1,
        });
    let package = source::Package::from_parts(
        package.manifest.clone(),
        package.region_defs().unwrap(),
        &toml::to_string(&document).unwrap(),
    )
    .unwrap();
    let mut document = package.rules().unwrap().document().clone();
    let original = encode_rules_document(&document).unwrap();
    document.point_pools.reverse();
    document.supplies.reverse();
    document.abilities.reverse();
    document.ability_tags.reverse();
    document.talents.reverse();
    document.creature_types.reverse();
    document.subtypes.reverse();
    document.classes.reverse();
    document.conditions.reverse();
    document.descriptors.reverse();
    document.flags.reverse();
    document.resources.reverse();
    document.equipment_slots.reverse();
    assert_eq!(encode_rules_document(&document).unwrap(), original);
    let reordered = source::Package::from_parts(
        package.manifest.clone(),
        package.region_defs().unwrap(),
        &toml::to_string(&document).unwrap(),
    )
    .unwrap();
    assert_eq!(
        package.certificate.ruleset_hash,
        reordered.certificate.ruleset_hash
    );
    assert_eq!(
        package.rules().unwrap().document(),
        reordered.rules().unwrap().document()
    );
    assert_ne!(
        package.certificate.files[&package.manifest.ruleset],
        reordered.certificate.files[&package.manifest.ruleset]
    );
    let first = document
        .abilities
        .iter()
        .flat_map(|ability| match &ability.strikes {
            tor_simulation::rules::StrikeSource::Authored { strikes } => strikes.as_slice(),
            tor_simulation::rules::StrikeSource::SelectedMelee
            | tor_simulation::rules::StrikeSource::Projectile { .. }
            | tor_simulation::rules::StrikeSource::NaturalMelee { .. } => &[],
        })
        .flat_map(|strike| &strike.on_success)
        .next()
        .unwrap()
        .clone();
    let second = document
        .abilities
        .iter()
        .flat_map(|ability| match &ability.strikes {
            tor_simulation::rules::StrikeSource::Authored { strikes } => strikes.as_slice(),
            tor_simulation::rules::StrikeSource::SelectedMelee
            | tor_simulation::rules::StrikeSource::Projectile { .. }
            | tor_simulation::rules::StrikeSource::NaturalMelee { .. } => &[],
        })
        .flat_map(|strike| &strike.on_success)
        .find(|effect| **effect != first)
        .unwrap()
        .clone();
    {
        let tor_simulation::rules::StrikeSource::Authored { strikes } =
            &mut document.abilities[0].strikes
        else {
            panic!("expected authored fixture");
        };
        strikes[0].on_success = vec![first, second];
    }
    let ordered = encode_rules_document(&document).unwrap();
    {
        let tor_simulation::rules::StrikeSource::Authored { strikes } =
            &mut document.abilities[0].strikes
        else {
            panic!("expected authored fixture");
        };
        strikes[0].on_success.reverse();
    }
    assert_ne!(encode_rules_document(&document).unwrap(), ordered);
    let ordered_grants = encode_rules_document(&document).unwrap();
    document.constants.racial_point_grants.reverse();
    assert_ne!(
        encode_rules_document(&document).unwrap(),
        ordered_grants,
        "grant order selects the default spending pool"
    );
}

#[test]
fn compiled_regions_normalize_explicit_defaults_and_source_comments() {
    let authored = "id = 1\nname = \"Room\"\nsize = [4, 4, 1]\n";
    let explicit = format!(
            "{authored}# authored presentation only\nlit = true\nchamber = false\nlighting = []\nwalls = []\n"
        );
    let first: source::RegionDef = toml::from_str(authored).unwrap();
    let second: source::RegionDef = toml::from_str(&explicit).unwrap();
    assert_ne!(
        source::source_digest(authored),
        source::source_digest(&explicit)
    );
    assert_eq!(
        encode_region_input(&first).unwrap(),
        encode_region_input(&second).unwrap()
    );
}

#[test]
fn compiled_package_rejects_tampered_index_and_dependency_provenance() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let package = source::read_package(&root).unwrap();
    let original: Value =
        serde_json::from_slice(&encode_content_package(&package).unwrap()).unwrap();
    for pointer in [
        "/index/regions/0/file",
        "/index/regions/0/name",
        "/package/certificate/content_hash",
    ] {
        let mut bad = original.clone();
        *bad.pointer_mut(pointer).unwrap() = Value::String("tampered".into());
        assert!(
            decode_content_package(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "{pointer}"
        );
    }
    let mut bad = original;
    bad["package"]["certificate"]["ruleset_dependencies"][source::RULESET] =
        Value::String("0".repeat(64));
    assert!(decode_content_package(&serde_json::to_vec(&bad).unwrap()).is_err());
}

#[test]
fn compiled_metadata_capacity_fails_before_a_save_is_admitted() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let mut package = source::read_package(&root).unwrap();
    package.manifest.version = "x".repeat(MAX_SAVE_METADATA_BYTES);
    let error = check_metadata_capacity(&package).unwrap_err();
    assert!(error.message.contains("save metadata limit"));
    assert!(encode_content_package(&package).is_err());
}

fn reject_obsolete_compiled_headers(input: &str, current: u32, accepts: impl Fn(&str) -> bool) {
    assert!(accepts(input));
    assert_eq!(
        serde_json::from_str::<Value>(input).unwrap()["format"],
        current
    );
    let header = format!("\"format\":{current}");
    let mut replacements: Vec<_> = [0, current - 1, current + 1]
        .into_iter()
        .map(|version| format!("\"format\":{version}"))
        .collect();
    replacements.extend([
        format!("{header},{header}"),
        format!("{header},\"unknown\":true"),
    ]);
    for replacement in replacements {
        let invalid = input.replacen(&header, &replacement, 1);
        assert_ne!(invalid, input);
        assert!(!accepts(&invalid), "accepted {replacement}");
    }
    let missing = input.replacen(&format!("{header},"), "", 1);
    assert_ne!(missing, input);
    assert!(!accepts(&missing));
}

#[test]
fn compiled_rules_current_header_rejects_previous_and_malformed_envelopes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let package = source::read_package(&root).unwrap();
    let input = package.rules_document;
    reject_obsolete_compiled_headers(&input, super::RULES_INPUT_FORMAT, |text| {
        decode_rules_document(text).is_ok()
    });
}

#[test]
fn compiled_region_current_header_rejects_previous_and_malformed_envelopes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let package = source::read_package(&root).unwrap();
    let input = encode_region_input(&package.region_def(1).unwrap()).unwrap();
    reject_obsolete_compiled_headers(&input, super::REGION_INPUT_FORMAT, |text| {
        decode_region_input(text).is_ok()
    });
}

#[test]
fn compiled_package_current_header_rejects_previous_and_malformed_envelopes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let package = source::read_package(&root).unwrap();
    let input = String::from_utf8(encode_content_package(&package).unwrap()).unwrap();
    reject_obsolete_compiled_headers(&input, super::CONTENT_PACKAGE_FORMAT, |text| {
        decode_content_package(text.as_bytes()).is_ok()
    });
}

#[test]
fn compiled_rules_reject_unknown_version_duplicates_and_changed_document() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let package = source::read_package(&root).unwrap();
    let original = &package.rules_document;
    assert_eq!(
        encode_rules_document(&decode_rules_document(original).unwrap()).unwrap(),
        *original
    );
    let header = format!("\"format\":{}", super::RULES_INPUT_FORMAT);
    for replacement in ["\"format\":0".to_owned(), format!("{header},{header}")] {
        let invalid = original.replacen(&header, &replacement, 1);
        assert_ne!(invalid, *original);
        assert!(decode_rules_document(&invalid).is_err());
    }
    let mut corrupted = package.clone();
    let mut document = decode_rules_document(original).unwrap();
    document.constants.base_defense += 1;
    corrupted.rules_document = encode_rules_document(&document).unwrap();
    assert!(corrupted.check_identity().is_err());
}

#[test]
fn compiled_region_round_trip_rejects_unknown_duplicate_and_missing_fields() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/tests/generated-filler");
    let package = source::read_package(&root).unwrap();
    let def = package.region_def(2).unwrap();
    let input = encode_region_input(&def).unwrap();
    assert_eq!(
        encode_region_input(&decode_region_input(&input).unwrap()).unwrap(),
        input
    );
    let value: Value = serde_json::from_str(&input).unwrap();
    for path in ["/region", "/region/generate"] {
        let mut corrupted = value.clone();
        corrupted
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("surprise".into(), Value::Bool(true));
        assert!(decode_region_input(&serde_json::to_string(&corrupted).unwrap()).is_err());
    }
    let mut missing = value.clone();
    missing["region"]
        .as_object_mut()
        .unwrap()
        .remove("lighting");
    assert!(decode_region_input(&serde_json::to_string(&missing).unwrap()).is_err());
    assert_eq!(value["format"], 10);
    let header = "\"format\":10";
    for replacement in ["\"format\":0".to_owned(), format!("{header},{header}")] {
        let invalid = input.replacen(header, &replacement, 1);
        assert_ne!(
            invalid, input,
            "the strict mutation must change the current input"
        );
        assert!(decode_region_input(&invalid).is_err());
    }
    assert!(decode_region_input("id = 1").is_err());
}

#[test]
fn compiled_discovery_fields_round_trip_and_are_required_strictly() {
    let region: source::RegionDef = serde_json::from_value(serde_json::json!({
            "id":1,"name":"Hidden features","size":[5,5,2],"chamber":true,
            "doors":[{"id":1,"at":[3,2,0],"open":false,"height":2,"concealment":{"dc":12}}],
            "traps":[{"id":1,"at":[2,2,0],"dc":12,"kind":{"kind":"rust","slot":"body_armor","amount":1}},
                     {"id":2,"at":[1,2,0],"dc":13,"kind":{"kind":"sleeping_gas","condition":"rogue.sleep","duration":200}}]
        })).unwrap();
    let encoded = encode_region_input(&region).unwrap();
    assert_eq!(
        encode_region_input(&decode_region_input(&encoded).unwrap()).unwrap(),
        encoded
    );
    let value: Value = serde_json::from_str(&encoded).unwrap();
    for (path, field) in [
        ("/region", "traps"),
        ("/region/doors/0", "concealment"),
        ("/region/traps/0", "dc"),
        ("/region/traps/0/kind", "slot"),
        ("/region/traps/1/kind", "condition"),
    ] {
        let mut missing = value.clone();
        missing
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            decode_region_input(&serde_json::to_string(&missing).unwrap()).is_err(),
            "missing {path}/{field}"
        );
    }
    for path in [
        "/region/doors/0/concealment",
        "/region/traps/0",
        "/region/traps/1/kind",
    ] {
        let mut extra = value.clone();
        extra
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("surprise".into(), Value::Bool(true));
        assert!(
            decode_region_input(&serde_json::to_string(&extra).unwrap()).is_err(),
            "extra {path}"
        );
    }
    for (path, fields) in [
        (
            "/region",
            &[
                "trapdoor_destination",
                "lit",
                "lighting",
                "id",
                "name",
                "size",
                "chamber",
                "zone",
                "gravity",
                "gravity_overrides",
                "anchors",
                "walls",
                "openings",
                "places",
                "portals",
                "doors",
                "traps",
                "items",
                "actors",
                "generate",
            ][..],
        ),
        (
            "/region/doors/0",
            &["concealment", "id", "at", "open", "height"][..],
        ),
        ("/region/doors/0/concealment", &["dc"][..]),
        (
            "/region/traps/0",
            &["avoidance_dc", "id", "at", "dc", "kind"][..],
        ),
    ] {
        let mut positional = value.clone();
        let record = positional.pointer_mut(path).unwrap();
        *record = Value::Array(fields.iter().map(|field| record[*field].clone()).collect());
        assert!(
            decode_region_input(&serde_json::to_string(&positional).unwrap()).is_err(),
            "positional {path}"
        );
    }
    assert!(
        serde_json::from_value::<Owned<crate::generation_recipe::SecretEntrances>>(
            serde_json::json!([250, 2, 12])
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<Owned<crate::generation_recipe::HiddenTraps>>(serde_json::json!(
            [[1, 2], 12, "body_armor", 1, "rogue.sleep", 200]
        ))
        .is_err()
    );
    assert!(
        serde_json::from_value::<Owned<source::IndexedRegion>>(serde_json::json!([
            [],
            1,
            "regions/1.toml",
            "hash",
            "compiled_hash",
            "Hidden features",
            [5, 5, 2],
            true,
            null,
            {},
            [],
            [],
            [],
            [],
            [1],
            [1, 2],
            false
        ]))
        .is_err()
    );
}

#[test]
fn current_region_input_pins_supply_placement_bytes() {
    // Supply capability belongs to the separately stored manifest archetype.
    // Placement still carries only its name. The region envelope also owns
    // actor recipes with explicit nutrition requirements.
    let region: source::RegionDef = serde_json::from_value(serde_json::json!({
        "id":1,"name":"Supply room","size":[4,4,1],
        "items":[{"id":10,"at":[1,1,0],"archetype":"clean_bandage","quantity":2}]
    }))
    .unwrap();
    let expected = concat!(
            "{\"format\":10,\"region\":{\"trapdoor_destination\":null,\"lit\":true,\"lighting\":[],\"id\":1,",
            "\"name\":\"Supply room\",\"size\":[4,4,1],\"chamber\":false,\"zone\":null,",
            "\"gravity\":null,\"gravity_overrides\":[],\"anchors\":{},\"walls\":[],",
            "\"openings\":[],\"places\":[],\"portals\":[],\"doors\":[],\"traps\":[],\"items\":[{",
            "\"equipped_slot\":null,\"class\":null,\"quantity\":2,\"stackable\":null,",
            "\"properties\":{},\"id\":10,\"at\":[1,1,0],\"archetype\":\"clean_bandage\",",
            "\"name\":null,\"identity\":null,\"carried_by\":null,\"seed_names\":[]}],\"actors\":[],\"generate\":null}}"
        );
    assert_eq!(encode_region_input(&region).unwrap(), expected);
    let restored = decode_region_input(expected).unwrap();
    assert_eq!(encode_region_input(&restored).unwrap(), expected);
}

#[test]
fn current_region_recipe_requires_growth_and_nutrition_with_current_header() {
    let region: source::RegionDef = serde_json::from_value(serde_json::json!({
            "id":1,"name":"Growth room","size":[4,4,1],
            "actors":[{"id":2,"at":[1,1,0],"controller":"external",
                "creature":{"species":"human","name":"owner","corpse_identity":"item.corpse.test.owner","faction":"neutral",
                    "binding":"intellect","restoration":"authored", "nutrition": {"kind":"exempt"},"starting_skills":["athletics","heavy_weaponry"],
                    "hit_dice":[{"source":"racial"}],"future_growth":{"kind":"selected","source":{"kind":"racial"}}}}]
        })).unwrap();
    let encoded = encode_region_input(&region).unwrap();
    let value: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(value["format"], 10);
    assert_eq!(
        value["region"]["actors"][0]["creature"]["future_growth"],
        serde_json::json!({"kind":"selected","source":{"kind":"racial"}})
    );
    assert_eq!(
        value["region"]["actors"][0]["creature"]["nutrition"],
        serde_json::json!({"kind":"exempt"})
    );
    assert_eq!(
        encode_region_input(&decode_region_input(&encoded).unwrap()).unwrap(),
        encoded
    );
    let mut previous = value.clone();
    previous["format"] = (super::REGION_INPUT_FORMAT - 1).into();
    assert!(
        decode_region_input(&serde_json::to_string(&previous).unwrap()).is_err(),
        "current actor fields cannot upgrade the previous region envelope"
    );
    for field in ["future_growth", "nutrition"] {
        let mut missing = value.clone();
        missing["region"]["actors"][0]["creature"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(decode_region_input(&serde_json::to_string(&missing).unwrap()).is_err());
    }
}

#[test]
fn compiled_room_omission_recipe_roundtrips_without_authoring_sources() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let template = source::read_package(&root).unwrap();
    let preset: crate::generation_recipe::GridRoomsPreset = serde_json::from_value(
        serde_json::json!({"version":1,"room_width":[4,6],"room_height":[3,5],
                "extra_connections":[0,1],"room_omissions":{"count":[0,3]}}),
    )
    .unwrap();
    let mut manifest = template.manifest.clone();
    manifest
        .generation_recipes
        .insert("optional".into(), preset.to_recipe().unwrap());
    let mut package = source::Package::from_parts_scripted(
        manifest,
        template.region_defs().unwrap(),
        &template.rules_source,
        template.scripts.clone(),
    )
    .unwrap();
    package.check().unwrap();
    let encoded = encode_content_package(&package).unwrap();
    let restored = decode_content_package(&encoded).unwrap();
    assert!(restored.directory.is_none());
    assert_eq!(restored.sources.files_read(), 0);
    assert_eq!(encode_content_package(&restored).unwrap(), encoded);
    let value: Value = serde_json::from_slice(&encoded).unwrap();
    let path = "/package/manifest/generation_recipes/optional/stages/6/operation/config";
    assert_eq!(
        value.pointer(path).unwrap(),
        &serde_json::json!({"count":[0,3]})
    );
    package.rules_source = "not authoring TOML".into();
    package.resolved_rules_source = Some("also not authoring TOML".into());
    assert_eq!(encode_content_package(&package).unwrap(), encoded);
    for invalid in [
        serde_json::json!({}),
        serde_json::json!([[0, 3]]),
        serde_json::json!({"count":[0,3],"surprise":true}),
    ] {
        let mut bad = value.clone();
        *bad.pointer_mut(path).unwrap() = invalid;
        assert!(decode_content_package(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    let text = String::from_utf8(encoded).unwrap();
    assert!(text.contains("\"count\":[0,3]"));
    let duplicate = text.replacen("\"count\":[0,3]", "\"count\":[0,3],\"count\":[0,3]", 1);
    assert!(decode_content_package(duplicate.as_bytes()).is_err());
}

#[test]
fn compiled_metadata_has_no_authoring_sources_or_machine_path() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let mut package = source::read_package(&root).unwrap();
    let encoded = encode_content_package(&package).unwrap();
    let value: Value = serde_json::from_slice(&encoded).unwrap();
    assert!(value["package"].get("rules_source").is_none());
    assert!(value["package"].get("resolved_rules_source").is_none());
    assert_eq!(value["package"]["directory"], Value::Null);
    let restored = decode_content_package(&encoded).unwrap();
    assert_eq!(
        restored.rules().unwrap().document(),
        package.rules().unwrap().document()
    );
    package.rules_source = "this is not TOML".into();
    package.resolved_rules_source = Some("also not TOML".into());
    assert_eq!(encode_content_package(&package).unwrap(), encoded);
    let mut bad = value;
    bad["package"]["rules_document"] = Value::String("id = 'source-is-not-a-snapshot'".into());
    assert!(decode_content_package(&serde_json::to_vec(&bad).unwrap()).is_err());
}

fn scenario_fixture_name(archive_version: u32, ruleset: &str) -> String {
    match (archive_version, ruleset) {
        // This exact published pair predates rules-aware filenames. It is
        // a name mapping, not fallback to an older fixture or save reader.
        (70, "interactions-v60") => "saved-scenarios-v70.json".into(),
        _ => format!("saved-scenarios-v{archive_version}-{ruleset}.json"),
    }
}

#[test]
fn scenario_golden_identity_tracks_archive_and_rules_independently() {
    assert_eq!(
        scenario_fixture_name(70, "interactions-v60"),
        "saved-scenarios-v70.json",
        "the exact published pair keeps its original filename"
    );
    assert_eq!(
        scenario_fixture_name(70, "interactions-v59"),
        "saved-scenarios-v70-interactions-v59.json",
        "a different rules identity never reuses the legacy file"
    );
    assert_eq!(
        scenario_fixture_name(69, "interactions-v60"),
        "saved-scenarios-v69-interactions-v60.json",
        "a different archive never reuses archive-only naming"
    );
    assert_eq!(
        scenario_fixture_name(71, "fixture.future-rules"),
        "saved-scenarios-v71-fixture.future-rules.json"
    );
}

fn current_scenarios() -> BTreeMap<String, Value> {
    // Certificates embed rules identity even when the physical archive
    // format is unchanged. Keep every published archive/rules golden intact.
    // Only machine-specific package directories are normalized to null;
    // this is not an old-save reader and has no historical fallback.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(scenario_fixture_name(
            crate::engine::ARCHIVE_VERSION,
            crate::scenario_package::RULESET,
        ));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{}: {error}; record the current writer with TOR_RECORD_SCENARIO_FIXTURES",
            path.display()
        )
    });
    serde_json::from_str(&text).unwrap()
}

#[test]
fn current_writer_matches_golden_scenario_fixture() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios");
    let mut actual = BTreeMap::new();
    for (name, path) in [
        ("two-room", "two-room"),
        ("items", "tests/items"),
        ("first-dungeon", "first-dungeon"),
        ("interactions", "tests/interactions"),
        ("ai-interactions", "tests/ai-interactions"),
    ] {
        let mut scenario = source::load(&root.join(path), 42, None, false).unwrap();
        let mut package = (**scenario.package.as_ref().unwrap()).clone();
        package.directory = None;
        scenario.package = Some(Arc::new(package));
        actual.insert(
            name.to_owned(),
            serde_json::to_value(Borrowed(&scenario)).unwrap(),
        );
    }
    let diagnostic = crate::Scenario::performance(42, 8, 2).unwrap();
    actual.insert(
        "diagnostic".into(),
        serde_json::to_value(Borrowed(&diagnostic)).unwrap(),
    );
    if let Some(output) = std::env::var_os("TOR_RECORD_SCENARIO_FIXTURES") {
        let mut bytes = serde_json::to_vec_pretty(&actual).unwrap();
        bytes.push(b'\n');
        std::fs::write(output, bytes).unwrap();
        return;
    }
    assert_eq!(actual, current_scenarios());
}

#[test]
fn stored_package_metadata_requires_current_owned_fields() {
    let fixtures = current_scenarios();
    let original = &fixtures["first-dungeon"];
    assert_saved_nested_fields::<crate::Scenario>(
        original,
        "/package",
        &[
            "manifest",
            "rules_document",
            "certificate",
            "validated",
            "selected",
        ],
    );
    assert_saved_nested_fields::<crate::Scenario>(
        original,
        "/package/certificate",
        &[
            "validator",
            "ruleset",
            "ruleset_hash",
            "ruleset_dependencies",
            "content_hash",
            "files",
            "regions",
            "model_hash",
            "coverage",
        ],
    );
}

#[test]
fn compiled_package_requires_current_certificate_identities() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let package = source::read_package(&root).unwrap();
    let encoded = encode_content_package(&package).unwrap();
    decode_content_package(&encoded).unwrap();
    let original: Value = serde_json::from_slice(&encoded).unwrap();
    for field in ["ruleset", "validator"] {
        let mut unsupported = original.clone();
        unsupported["package"]["certificate"][field] =
            Value::String(format!("unsupported-{field}"));
        assert!(
            decode_content_package(&serde_json::to_vec(&unsupported).unwrap()).is_err(),
            "accepted unsupported {field} identity"
        );
    }
}

fn assert_saved_nested_fields<T: Stored>(original: &Value, path: &str, fields: &[&str]) {
    strict::<Owned<T>>(&serde_json::to_vec(original).unwrap()).unwrap();
    let object = original
        .pointer(path)
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("missing saved object {path}"));
    let mut unknown = original.clone();
    unknown
        .pointer_mut(path)
        .unwrap()
        .as_object_mut()
        .unwrap()
        .insert("unexpected".into(), Value::Bool(true));
    assert!(
        strict::<Owned<T>>(&serde_json::to_vec(&unknown).unwrap()).is_err(),
        "{path} accepted an unknown field"
    );
    for field in fields {
        let expected = object
            .get(*field)
            .unwrap_or_else(|| panic!("missing saved field {path}/{field}"));
        let mut missing = original.clone();
        assert!(missing
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(*field)
            .is_some());
        assert!(
            strict::<Owned<T>>(&serde_json::to_vec(&missing).unwrap()).is_err(),
            "{path} accepted missing field {field}"
        );
        // Replace the selected object with a raw duplicate-member object;
        // serde_json::Value itself cannot retain duplicate fields.
        let mut marked = original.clone();
        *marked.pointer_mut(path).unwrap() = Value::String("duplicate_saved_object".into());
        let text = serde_json::to_string(&marked).unwrap();
        let marker = "\"duplicate_saved_object\"";
        assert_eq!(text.matches(marker).count(), 1);
        let encoded = serde_json::to_string(object).unwrap();
        let duplicate = format!(
            "{{{}:{},{}",
            serde_json::to_string(field).unwrap(),
            serde_json::to_string(expected).unwrap(),
            &encoded[1..]
        );
        let text = text.replacen(marker, &duplicate, 1);
        assert!(
            strict::<Owned<T>>(text.as_bytes()).is_err(),
            "{path} accepted duplicate field {field}"
        );
    }
}

#[test]
fn saved_builds_require_creation_and_nested_purchase_fields() {
    let fixtures = current_scenarios();
    let original = &fixtures["first-dungeon"];
    for (build, kind, choice) in [
        (
            "/package/manifest/characters/0/creature",
            "talent",
            "talent",
        ),
        (
            "/package/manifest/archetypes/dungeon.guardian/creature",
            "skill_rank",
            "skill",
        ),
    ] {
        assert_saved_nested_fields::<crate::Scenario>(
            original,
            build,
            &["starting_skills", "restoration", "nutrition", "hit_dice"],
        );
        let advancement = format!("{build}/hit_dice/0");
        assert_saved_nested_fields::<crate::Scenario>(
            original,
            &advancement,
            &["source", "purchases"],
        );
        let purchase = format!("{advancement}/purchases/0");
        assert_saved_nested_fields::<crate::Scenario>(original, &purchase, &["pool", "selection"]);
        let selection = format!("{purchase}/selection");
        assert_eq!(original.pointer(&selection).unwrap()["kind"], kind);
        assert_saved_nested_fields::<crate::Scenario>(original, &selection, &["kind", choice]);
    }
}

#[test]
fn saved_skill_rank_purchase_fields_are_required_and_strict() {
    use crate::creature_authoring::{Purchase, PurchaseSelection};
    let purchase = Purchase {
        pool: "advancement".into(),
        selection: PurchaseSelection::SkillRank {
            skill: "athletics".into(),
        },
    };
    // Exercise the independent save writer/reader, not the authoring or
    // mutable-simulation serde implementations.
    let original = serde_json::to_value(Borrowed(&purchase)).unwrap();
    assert_saved_nested_fields::<Purchase>(&original, "", &["pool", "selection"]);
    assert_saved_nested_fields::<Purchase>(&original, "/selection", &["kind", "skill"]);
}

#[test]
fn current_scenarios_round_trip_through_the_explicit_saved_schema() {
    for (name, expected) in current_scenarios() {
        let bytes = serde_json::to_vec(&expected).unwrap();
        let Owned(actual) = strict::<Owned<crate::Scenario>>(&bytes).unwrap();
        assert_eq!(
            serde_json::to_value(Borrowed(&actual)).unwrap(),
            expected,
            "{name}"
        );
        if let Some(package) = actual.package {
            assert!(
                package.index.regions.is_empty(),
                "index attachment stays separate"
            );
            assert_eq!(
                package.sources.files_read(),
                0,
                "decode must not acquire regions"
            );
        }
    }
}

#[test]
fn stored_scenario_coordinates_require_numbers_and_reject_unknown_fields() {
    let original = &current_scenarios()["diagnostic"];
    for (field, bad) in [
        ("region", Value::String("1".into())),
        ("x", Value::from(i64::from(i32::MAX) + 1)),
        ("unexpected", Value::Bool(true)),
    ] {
        let mut value = original.clone();
        value["actors"][0]["position"][field] = bad;
        assert!(
            strict::<Owned<crate::Scenario>>(&serde_json::to_vec(&value).unwrap()).is_err(),
            "{field}"
        );
    }
}

#[test]
fn stored_metadata_rejects_unknown_nested_fields_at_each_owned_boundary() {
    let fixtures = current_scenarios();
    for (fixture, paths) in [(
            "first-dungeon",
            &[
                "/streaming",
                "/package",
                "/package/certificate",
                "/package/manifest",
                "/package/manifest/archetypes/dungeon.guardian",
                "/package/manifest/archetypes/dungeon.guardian/creature",
                "/package/manifest/archetypes/dungeon.guardian/creature/hit_dice/0",
                "/package/manifest/characters/0",
                "/package/manifest/characters/0/creature/hit_dice/3",
                "/package/manifest/creatures",
                "/package/manifest/creatures/species/stone_construct",
                "/package/manifest/creatures/species/stone_construct/attributes",
                "/package/manifest/creatures/species/stone_construct/melee",
                "/package/manifest/creatures/species/stone_construct/melee/strikes/0/on_success/0/damage",
                "/package/manifest/creatures/species/stone_construct/melee/strikes/0/on_success/0/damage/components/0/amount",
                "/package/manifest/archetypes/dungeon.delver_blade/equipment",
                "/package/manifest/archetypes/dungeon.delver_blade/equipment/attack",
                "/package/manifest/archetypes/dungeon.delver_blade/equipment/attack/damage",
                "/package/manifest/archetypes/dungeon.delver_blade/equipment/attack/damage/primary",
                "/package/manifest/archetypes/dungeon.delver_blade/equipment/attack/damage/components/0/amount",
            ][..],
        )] {
            for path in paths {
                let mut value = fixtures[fixture].clone();
                value
                    .pointer_mut(path)
                    .and_then(Value::as_object_mut)
                    .unwrap_or_else(|| panic!("missing {fixture} fixture boundary {path}"))
                    .insert("unexpected".into(), Value::Bool(true));
                assert!(
                    strict::<Owned<crate::Scenario>>(&serde_json::to_vec(&value).unwrap()).is_err(),
                    "{fixture}: {path} accepted an unknown field"
                );
            }
        }
}

#[test]
fn stored_actor_definitions_reject_removed_combat_fields() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let package = source::read_package(&root).unwrap();
    macro_rules! reject {
        ($definition:expr, $kind:ty) => {{
            let mut value = serde_json::to_value(Borrowed($definition)).unwrap();
            strict::<Owned<$kind>>(&serde_json::to_vec(&value).unwrap()).unwrap();
            value
                .as_object_mut()
                .unwrap()
                .insert("combat".into(), serde_json::Value::Null);
            // Storage deliberately sanitizes decoding diagnostics. Validate
            // both the strict boundary and the underlying unknown-field cause.
            assert!(strict::<Owned<$kind>>(&serde_json::to_vec(&value).unwrap()).is_err());
            let error = serde_json::from_value::<Owned<$kind>>(value)
                .err()
                .expect("removed combat field must fail");
            assert!(
                error.to_string().contains("unknown field `combat`"),
                "{error}"
            );
        }};
    }
    reject!(&package.manifest.characters[0], source::Character);
    reject!(&source::Archetype::default(), source::Archetype);
}

#[test]
fn all_stored_damage_names_preserve_their_pre_refactor_spellings() {
    for (value, name) in [
        (source::DamageType::Energy, "energy"),
        (source::DamageType::Impact, "impact"),
        (source::DamageType::Keen, "keen"),
        (source::DamageType::Spirit, "spirit"),
        (source::DamageType::Vital, "vital"),
        (source::DamageType::Psychic, "psychic"),
    ] {
        let bytes = serde_json::to_vec(&Borrowed(&value)).unwrap();
        assert_eq!(bytes, format!("\"{name}\"").as_bytes());
        assert_eq!(
            strict::<Owned<source::DamageType>>(&bytes).unwrap().0,
            value
        );
    }
}

#[test]
fn stored_damage_collections_reject_duplicate_and_unknown_names() {
    for bytes in [
        br#"{"impact":1,"impact":2}"#.as_slice(),
        br#"{"unknown":1}"#.as_slice(),
    ] {
        assert!(strict::<Owned<BTreeMap<source::DamageType, u32>>>(bytes).is_err());
    }
    for bytes in [
        br#"["impact","impact"]"#.as_slice(),
        br#"["unknown"]"#.as_slice(),
    ] {
        assert!(strict::<Owned<BTreeSet<source::DamageType>>>(bytes).is_err());
    }
}

#[test]
fn saved_stair_pair_metadata_and_generated_names_round_trip_strictly() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/tests/paired-stairs");
    let scenario = source::load(&root, 42, None, true).unwrap();
    let bytes = serde_json::to_vec(&Borrowed(&scenario)).unwrap();
    let restored = strict::<Owned<crate::Scenario>>(&bytes).unwrap().0;
    assert_eq!(serde_json::to_vec(&Borrowed(&restored)).unwrap(), bytes);
    let package = restored.package.unwrap();
    assert_eq!(package.manifest.stair_pairs["first"].lower, "2/up");
    // The index is stored separately from the scenario's manifest metadata.
    let index = &scenario.package.as_ref().unwrap().index;
    let restored_index = decode_index(&encode_index(index).unwrap()).unwrap();
    assert_eq!(restored_index.regions[1].stair_anchors, ["up", "down"]);
    for bad in [
        br#"{"upper":"1/stair","lower":"2/up","lower":"2/down"}"#.as_slice(),
        br#"{"upper":"1/stair","lower":"2/up","gate":true}"#.as_slice(),
    ] {
        assert!(strict::<Owned<source::StairPair>>(bad).is_err());
    }
}

#[test]
fn stored_indexes_preserve_existing_bytes_and_generated_flag_omission() {
    for bytes in [
        include_bytes!("../../../scenarios/two-room/index.json").as_slice(),
        include_bytes!("../../../scenarios/tests/generated-filler/index.json").as_slice(),
        include_bytes!("../../../scenarios/first-dungeon/index.json").as_slice(),
    ] {
        let index = decode_index(bytes).unwrap();
        assert_eq!(encode_index(&index).unwrap(), bytes);
        assert_eq!(index.to_bytes().unwrap(), bytes);
    }
}

#[test]
fn stored_index_rejects_nested_duplicates_and_noncanonical_optional_fields() {
    let text = include_str!("../../../scenarios/two-room/index.json");
    for bad in [
        text.replacen(
            "\"start\":[1,1,0]",
            "\"start\":[0,0,0],\"start\":[1,1,0]",
            1,
        ),
        text.replacen("\"zone\":null,", "", 1),
        text.replacen(
            "\"chamber\":true",
            "\"chamber\":true,\"generated\":false",
            1,
        ),
        text.replacen("\"items\":", "\"unexpected\":true,\"items\":", 1),
    ] {
        assert_ne!(bad, text);
        assert!(decode_index(bad.as_bytes()).is_err());
    }
}

#[test]
fn stored_index_requires_unique_increasing_region_identities() {
    let bytes = include_bytes!("../../../scenarios/two-room/index.json");
    let original: Value = serde_json::from_slice(bytes).unwrap();
    let mut duplicate = original.clone();
    duplicate["regions"][1]["id"] = Value::from(1);
    assert!(decode_index(&serde_json::to_vec(&duplicate).unwrap()).is_err());
    let mut descending = original;
    descending["regions"].as_array_mut().unwrap().reverse();
    assert!(decode_index(&serde_json::to_vec(&descending).unwrap()).is_err());
}
