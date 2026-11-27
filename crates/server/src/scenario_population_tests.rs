use super::*;
use serde_json::{json, Value};

fn fixture() -> Package {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/rogue-geometry");
    let mut source = serde_json::to_value(resolve_source(&root).unwrap().manifest).unwrap();
    source["archetypes"] = json!({"marker":{"name":"marker","identity":"item.misc.marker"}});
    source["floor_activation"] = json!({"version":1,"protagonist":"selected_character",
            "families":{"dungeon":"plain"},"profiles":{"plain":{"version":1,"stages":[
            {"kind":"items","id":"stock","version":1,"count":[1,1],
            "bands":[{"depth":{"kind":"open","minimum":1},"choices":[{"archetype":"marker","weight":1}]}]}]}}});
    Package::from_parts(
        serde_json::from_value(source).unwrap(),
        vec![],
        tor_simulation::rules::DEFAULT_RULESET_TOML,
    )
    .unwrap()
}

fn changed(package: &Package, edit: impl FnOnce(&mut Value)) -> Result<Package, Failure> {
    let mut value = serde_json::to_value(&package.manifest).unwrap();
    edit(&mut value);
    Package::from_parts_scripted(
        serde_json::from_value(value).map_err(|error| fail(error.to_string()))?,
        vec![],
        &package.rules_source,
        package.scripts.clone(),
    )
}

fn carried_fixture() -> Package {
    changed(&fixture(), |value| {
        value["archetypes"]["stock"] =
            json!({"name":"finite stock","identity":"item.misc.stock","stackable":true});
        value["floor_activation"]["carried_item_pools"] = json!({
            "supplies":{"version":1,"choices":[
                {"archetype":"marker","weight":2,"quantity":1},
                {"archetype":"stock","weight":3,"quantity":7}
            ]}
        });
        value["floor_activation"]["profiles"]["plain"]["stages"] = json!([{
            "kind":"actors","id":"inhabitants","version":1,"count":[1,1],
            "bands":[{"depth":{"kind":"open","minimum":1},"choices":[{
                "archetype":"marker","weight":1,
                "carried_loot":{"version":1,"pool":"supplies",
                    "chance":{"numerator":1,"denominator":5}}
            }]}]
        }]);
    })
    .unwrap()
}

#[test]
fn wandering_authoring_binds_actor_stage_and_rejects_unknown_exclusions() {
    let good = changed(&carried_fixture(), |source| {
        source["floor_activation"]["wandering"] = json!({
            "stage_id":"inhabitants", "excluded_wandering_content":[],
            "initial_delay":[6700,7300], "retry_ticks":400,
            "chance":{"numerator":1,"denominator":6}, "living_cap":8
        });
    })
    .unwrap();
    // from_parts assembles source; semantic references are checked separately.
    good.check().unwrap();
    assert_eq!(
        good.manifest
            .floor_activation
            .as_ref()
            .unwrap()
            .wandering
            .as_ref()
            .unwrap()
            .living_cap,
        8
    );
    assert!(
        changed(&good, |source| source["floor_activation"]["wandering"]
            ["stage_id"] = json!("missing"))
        .and_then(|package| package.check())
        .is_err()
    );
    assert!(
        changed(&good, |source| source["floor_activation"]["wandering"]
            ["excluded_wandering_content"] =
            json!(["missing"]))
        .and_then(|package| package.check())
        .is_err()
    );
    assert!(
        changed(&good, |source| source["floor_activation"]["wandering"]
            ["retry_ticks"] = json!(0))
        .and_then(|package| package.check())
        .is_err()
    );
    assert!(
        changed(&good, |source| source["floor_activation"]["wandering"]
            ["chance"]["denominator"] = json!(0))
        .and_then(|package| package.check())
        .is_err()
    );
}

fn repeated_population_package() -> Package {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let mut package = read_package(&root).unwrap();
    let activation = package.manifest.floor_activation.as_mut().unwrap();
    // The fixture narrows the catalog to one recipe to test memoization;
    // live population cadence and its excluded-species set are unrelated.
    activation.wandering = None;
    let (name, mut profile) = activation
        .profiles
        .iter()
        .next()
        .map(|(id, p)| (id.clone(), p.clone()))
        .unwrap();
    profile
        .stages
        .retain(|stage| matches!(stage, PopulationStage::Actors { .. }));
    profile.stages.truncate(1);
    let PopulationStage::Actors { bands, .. } = &mut profile.stages[0] else {
        unreachable!()
    };
    let mut first = bands[0].clone();
    first.choices.truncate(1);
    *bands = (1..=16)
        .map(|depth| {
            let mut band = first.clone();
            band.depth = PopulationDepth::Finite {
                minimum: depth,
                maximum: depth,
            };
            band.choices[0].weight = depth;
            band
        })
        .collect();
    activation.profiles = BTreeMap::from([(name, profile)]);
    package
}

fn first_prepared_population_choice(
    plan: &runtime::PopulationPlan,
) -> &runtime::PreparedActorChoice {
    let runtime::PopulationContent::Actors { bands, .. } =
        &plan.profiles.values().next().unwrap().stages[0].population
    else {
        unreachable!()
    };
    &bands[0].choices[0]
}

#[test]
fn population_birth_validation_counts_once_without_sharing_character_or_seed_witnesses() {
    let mut package = repeated_population_package();
    let characters: Vec<_> = package.manifest.characters.iter().map(|c| c.id).collect();
    let mut witnesses = 0;
    for selected in &characters {
        package.selected = *selected;
        for seed in [0, 1, 42] {
            let index = package.index(seed).unwrap();
            for _ in 0..2 {
                let mut births = PopulationBirthValidation::new(&index);
                let plan = package
                    .population_plan_with_births(&mut births)
                    .unwrap()
                    .unwrap();
                assert_eq!(births.checks, 1);
                assert_eq!(plan.protagonist, tor_simulation::ActorId(*selected));
                let runtime::PopulationContent::Actors { bands, .. } =
                    &plan.profiles.values().next().unwrap().stages[0].population
                else {
                    unreachable!()
                };
                assert_eq!(bands.len(), 16);
                for (n, band) in bands.iter().enumerate() {
                    assert_eq!(band.choices[0].weight, n as u32 + 1);
                }
                witnesses += 1;
            }
        }
    }
    assert_eq!(witnesses, characters.len() * 6);
}

#[test]
fn population_birth_validation_changed_recipe_and_failed_result_never_reuse_success() {
    let package = repeated_population_package();
    let index = package.index(42).unwrap();
    let plan = package.population_plan(&index).unwrap().unwrap();
    let original = first_prepared_population_choice(&plan);
    assert!(original.actor.build.is_some());
    for case in 0..3 {
        let mut births = PopulationBirthValidation::new(&index);
        births.validate(original).unwrap();
        let mut changed = original.clone();
        match case {
            0 => {
                let mut build =
                    serde_json::to_value(changed.actor.build.as_ref().unwrap()).unwrap();
                build["species"] = json!("missing-species");
                changed.actor.build = Some(serde_json::from_value(build).unwrap());
            }
            1 => {
                changed.depth_growth =
                    tor_simulation::prepared_spawn::DepthGrowth::RacialAfter { depth: 0 }
            }
            2 => changed
                .advancement
                .push(tor_simulation::prepared_spawn::OrdinalAdvancement {
                    ordinal: tor_simulation::HdCount(1),
                    choices: Default::default(),
                }),
            _ => unreachable!(),
        }
        for expected in [2, 3] {
            assert!(births.validate(&changed).is_err(), "case {case}");
            assert_eq!(births.checks, expected);
        }
        births.validate(original).unwrap();
        assert_eq!(births.checks, 3);
        assert_eq!(births.successful.len(), 1);
    }
}

#[test]
fn population_birth_validation_is_bounded_and_content_local() {
    let package = repeated_population_package();
    let index = package.index(42).unwrap();
    let plan = package.population_plan(&index).unwrap().unwrap();
    let original = first_prepared_population_choice(&plan);
    let mut births = PopulationBirthValidation::new(&index);
    for n in 0..=MAX_CHOICES {
        let mut choice = original.clone();
        choice.content = format!("recipe-{n}");
        births.validate(&choice).unwrap();
    }
    assert_eq!(births.successful.len(), MAX_CHOICES);
    assert_eq!(births.checks, MAX_CHOICES + 1);
    let other = fixture();
    let other_index = other.index(42).unwrap();
    let mut other_births = PopulationBirthValidation::new(&other_index);
    assert!(other_births.validate(original).is_err());
    assert_eq!(other_births.checks, 1);
    assert!(other_births.successful.is_empty());
}

#[test]
fn activation_assets_require_effective_family_themes_even_without_a_spawn() {
    let package = changed(&carried_fixture(), |value| {
        value["themes"] = json!(["elsewhere"]);
        value["assets"] = json!({"elsewhere":["creature.marker","item.stock"], "floor":[]});
        value["zones"] = json!({"floor":{"themes":["floor"]}});
        value["procedural_worlds"]["dungeon"]["zone"] = json!("floor");
        value["archetypes"]["marker"]["asset"] = json!("creature.marker");
        value["archetypes"]["stock"]["asset"] = json!("item.stock");
        let stage = &mut value["floor_activation"]["profiles"]["plain"]["stages"][0];
        stage["count"] = json!([0, 0]);
        stage["bands"][0]["depth"]["minimum"] = json!(100);
        stage["bands"][0]["choices"][0]["carried_loot"]["chance"]["numerator"] = json!(0);
    })
    .unwrap();
    let error = package.check().unwrap_err();
    assert!(
        error
            .message
            .contains("Activation family dungeon: asset creature.marker"),
        "{error}"
    );
    let only_actor = changed(&package, |value| {
        value["assets"]["floor"] = json!(["creature.marker"]);
    })
    .unwrap();
    let error = only_actor.check().unwrap_err();
    assert!(
        error
            .message
            .contains("Activation family dungeon: asset item.stock"),
        "{error}"
    );
    let complete = changed(&package, |value| {
        value["assets"]["floor"] = json!(["creature.marker", "item.stock"]);
    })
    .unwrap();
    complete.check().unwrap();
}

#[test]
fn activation_items_validate_the_concealed_pool_asset_only() {
    let package = changed(&fixture(), |value| {
        value["themes"] = json!(["floor"]);
        value["assets"] = json!({"floor":["item.secret"]});
        value["archetypes"]["marker"]["asset"] = json!("item.secret");
        value["archetypes"]["marker"]["appearance_pool"] = json!("vials");
        value["appearance_pools"] =
            json!({"vials":{"appearances":[{"name":"cloudy vial","asset":"item.potion"}]}});
    })
    .unwrap();
    let error = package.check().unwrap_err();
    assert!(error.message.contains("asset item.potion"), "{error}");
    let complete = changed(&package, |value| {
        value["assets"]["floor"] = json!(["item.potion"]);
    })
    .unwrap();
    complete.check().unwrap();
    let palette = complete.palette(complete.manifest.themes.iter());
    assert!(palette.contains("item.potion"));
    assert!(!palette.contains("item.secret"));
}

#[test]
fn future_floor_and_carried_pools_require_every_physical_form_asset() {
    for carried in [false, true] {
        let source = if carried {
            carried_fixture()
        } else {
            fixture()
        };
        let complete = changed(&source, |value| {
                value["themes"] = json!(["floor"]);
                value["assets"] = json!({"floor":["item.metal", "item.wood"]});
                value["appearance_pools"] = json!({
                    "metal":{"appearances":[{"name":"copper scepter","asset":"item.metal"}]},
                    "wood":{"appearances":[{"name":"oak scepter","asset":"item.wood"}]}
                });
                let equipment = json!({"slot":"weapon", "modifiers":[], "attack":{
                    "skill":"heavy_weaponry", "bonus":0, "wind_up":60, "recovery":40,
                    "damage":{"primary":{"category":"impact"}, "components":[
                        {"category":"impact", "amount":{"type":"fixed", "value":1}}
                    ]}
                }});
                let key = if carried { "stock" } else { "marker" };
                value["archetypes"][key] = json!({
                    "name":"scepter", "identity":if carried { "item.weapon.stock" } else { "item.weapon.marker" }, "class":"weapon", "appearance_pool":"metal",
                    "forms":[
                        {"name":"metal scepter", "appearance_pool":"metal", "equipment":equipment},
                        {"name":"wooden scepter", "appearance_pool":"wood", "equipment":equipment}
                    ]
                });
                let stage = &mut value["floor_activation"]["profiles"]["plain"]["stages"][0];
                stage["count"] = json!([0, 0]);
                stage["bands"][0]["depth"]["minimum"] = json!(100);
                if carried {
                    stage["bands"][0]["choices"][0]["carried_loot"]["chance"]["numerator"] =
                        json!(0);
                    value["floor_activation"]["carried_item_pools"]["supplies"]["choices"] =
                        json!([{"archetype":"stock", "weight":1, "quantity":1}]);
                }
            })
            .unwrap();
        // A real all-alternatives positive precedes every negative. Neither
        // late depth, zero births nor zero carried chance may hide an asset.
        complete.check().unwrap();
        complete.appearance_mapping(42).unwrap();
        let missing_alternate = changed(&complete, |value| {
            value["assets"]["floor"] = json!(["item.metal"]);
        })
        .unwrap();
        let error = missing_alternate.check().unwrap_err();
        assert!(
            error
                .message
                .contains("Activation family dungeon: asset item.wood"),
            "carried={carried}: {error}"
        );
    }
}

#[test]
fn carried_stock_compiles_every_pool_with_the_ordinary_item_adapter() {
    let package = carried_fixture();
    for (numerator, denominator) in [(0, 1), (1, 1), (1, 5), (5, 5)] {
        let package = changed(&package, |value| {
            value["floor_activation"]["profiles"]["plain"]["stages"][0]["bands"][0]["choices"][0]
                ["carried_loot"]["chance"]["numerator"] = json!(numerator);
            value["floor_activation"]["profiles"]["plain"]["stages"][0]["bands"][0]["choices"][0]
                ["carried_loot"]["chance"]["denominator"] = json!(denominator);
        })
        .unwrap();
        package.check().unwrap();
        let index = package.index(42).unwrap();
        let plan = package.population_plan(&index).unwrap().unwrap();
        let encoded = serde_json::to_value(&plan).unwrap();
        let pool = &encoded["carried_item_pools"]["supplies"];
        assert_eq!(pool["version"], 1);
        for (position, archetype, quantity) in [(0, "marker", 1), (1, "stock", 7)] {
            let expected = index
                .definitions
                .population_item(archetype, quantity, index.seed, &index.appearances)
                .unwrap();
            assert_eq!(
                pool["choices"][position]["quantity"],
                json!({"kind":"fixed","quantity":quantity})
            );
            assert_eq!(
                pool["choices"][position]["spec"],
                serde_json::to_value(expected).unwrap()
            );
        }
        let runtime::PopulationContent::Actors { bands, .. } =
            &plan.profiles["plain"].stages[0].population
        else {
            unreachable!()
        };
        let choice = serde_json::to_value(&bands[0].choices[0]).unwrap();
        assert_eq!(choice["carried_loot"]["chance"]["numerator"], numerator);
        assert_eq!(choice["carried_loot"]["chance"]["denominator"], denominator);
    }
    let package = changed(&package, |value| {
        value["floor_activation"]["profiles"]["plain"]["stages"][0]["bands"][0]["choices"][0]
            .as_object_mut()
            .unwrap()
            .remove("carried_loot");
    })
    .unwrap();
    let plan = package
        .population_plan(&package.index(42).unwrap())
        .unwrap()
        .unwrap();
    let runtime::PopulationContent::Actors { bands, .. } =
        &plan.profiles["plain"].stages[0].population
    else {
        unreachable!()
    };
    assert_eq!(
        serde_json::to_value(&bands[0].choices[0]).unwrap()["carried_loot"],
        Value::Null
    );
}

#[test]
fn carried_stock_pins_concealed_ordinary_items_and_maximum_unused_pools() {
    let package = changed(&carried_fixture(), |value| {
        value["appearance_pools"] = json!({"hidden":{
            "appearances":[{"name":"unmarked parcel","asset":"item.hidden"}], "confounding":false
        }});
        value["archetypes"]["stock"]["appearance_pool"] = json!("hidden");
        value["archetypes"]["stock"]["asset"] = json!("item.secret");
        value["archetypes"]["stock"]["properties"] = json!({"quality":"fine"});
    })
    .unwrap();
    let index = package.index(42).unwrap();
    let plan = package.population_plan(&index).unwrap().unwrap();
    let spec = &plan.carried_item_pools["supplies"].choices[1].spec;
    assert!(spec.concealed && spec.stackable);
    assert_eq!(spec.appearance, "unmarked parcel");
    assert_eq!(spec.asset.as_deref(), Some("item.hidden"));
    assert_eq!(spec.properties["quality"], "fine");

    let package = changed(&fixture(), |value| {
        let choices: Vec<_> = (0..64)
            .map(|n| {
                let name = format!("stock{n}");
                value["archetypes"][&name] =
                    json!({"name":name,"identity":format!("item.misc.stock{n}"),"stackable":true});
                json!({"archetype":name,"weight":1,"quantity":7})
            })
            .collect();
        value["floor_activation"]["carried_item_pools"] = Value::Object(
            (0..32)
                .map(|n| (format!("pool{n}"), json!({"version":1,"choices":choices})))
                .collect(),
        );
    })
    .unwrap();
    package.check().unwrap();
    let plan = package
        .population_plan(&package.index(42).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(plan.carried_item_pools.len(), 32);
    assert!(plan
        .carried_item_pools
        .values()
        .all(|pool| pool.choices.len() == 64));
}

#[test]
fn carried_pool_capacity_is_independent_of_population_band_capacity() {
    for count in [79, 128, 129] {
        let package = changed(&fixture(), |value| {
            let choices: Vec<_> = (0..count)
                .map(|n| {
                    let name = format!("stock{n}");
                    value["archetypes"][&name] = json!({
                        "name":name, "identity":format!("item.misc.stock{n}")
                    });
                    json!({"archetype":name,"weight":1,"quantity":1})
                })
                .collect();
            value["floor_activation"]["carried_item_pools"] =
                json!({"supplies":{"version":1,"choices":choices}});
        })
        .unwrap();
        assert_eq!(
            package.check().is_ok(),
            count <= 128,
            "carried count {count}"
        );
    }
    let package = changed(&fixture(), |value| {
        let choices: Vec<_> = (0..=runtime::MAX_ITEM_CHOICES)
            .map(|n| {
                let name = format!("marker{n}");
                value["archetypes"][&name] =
                    json!({"name":name,"identity":format!("item.misc.marker{n}")});
                json!({"archetype":name,"weight":1})
            })
            .collect();
        value["floor_activation"]["profiles"]["plain"]["stages"][0]["bands"][0]["choices"] =
            json!(choices);
    })
    .unwrap();
    assert!(
        package.check().is_err(),
        "ordinary item bands remain bounded"
    );
}

#[test]
fn carried_stock_rejects_invalid_even_unreachable_and_zero_chance_declarations() {
    let package = carried_fixture();
    for case in 0..19 {
        let invalid = changed(&package, |value| {
            // Neither this depth band nor a carry roll is reached at initial entry.
            value["floor_activation"]["profiles"]["plain"]["stages"][0]["bands"][0]["depth"]
                ["minimum"] = json!(100);
            value["floor_activation"]["profiles"]["plain"]["stages"][0]["bands"][0]["choices"][0]
                ["carried_loot"]["chance"]["numerator"] = json!(0);
            let plan = &mut value["floor_activation"];
            match case {
                0 => plan["carried_item_pools"]["supplies"]["version"] = json!(2),
                1 => plan["carried_item_pools"]["supplies"]["choices"] = json!([]),
                2 => plan["carried_item_pools"]["supplies"]["choices"][0]["quantity"] = json!(0),
                3 => plan["carried_item_pools"]["supplies"]["choices"][0]["quantity"] = json!(2),
                4 => plan["carried_item_pools"]["supplies"]["choices"][0]["weight"] = json!(0),
                5 => {
                    plan["carried_item_pools"]["supplies"]["choices"][0]["weight"] = json!(u32::MAX)
                }
                6 => {
                    plan["carried_item_pools"]["supplies"]["choices"][1]["archetype"] =
                        json!("marker")
                }
                7 => {
                    plan["carried_item_pools"]["supplies"]["choices"][0]["archetype"] =
                        json!("missing")
                }
                8 => {
                    plan["carried_item_pools"]["supplies"]["choices"] = json!(vec![
                        json!({"archetype":"stock","quantity":1,"weight":1});
                        65
                    ])
                }
                9 => {
                    plan["carried_item_pools"] = Value::Object(
                        (0..33)
                            .map(|n| {
                                (
                                    format!("pool{n}"),
                                    json!({"version":1,"choices":[
                            {"archetype":"stock","quantity":1,"weight":1}]}),
                                )
                            })
                            .collect(),
                    )
                }
                10 => {
                    plan["profiles"]["plain"]["stages"][0]["bands"][0]["choices"][0]
                        ["carried_loot"]["pool"] = json!("missing")
                }
                11 => {
                    plan["profiles"]["plain"]["stages"][0]["bands"][0]["choices"][0]
                        ["carried_loot"]["version"] = json!(2)
                }
                12 => {
                    plan["profiles"]["plain"]["stages"][0]["bands"][0]["choices"][0]
                        ["carried_loot"]["chance"]["denominator"] = json!(0)
                }
                13 => {
                    plan["profiles"]["plain"]["stages"][0]["bands"][0]["choices"][0]
                        ["carried_loot"]["chance"]["numerator"] = json!(6)
                }
                14 => {
                    let choice =
                        plan["profiles"]["plain"]["stages"][0]["bands"][0]["choices"][0].clone();
                    plan["profiles"]["plain"]["stages"][0]["bands"][0]["choices"] =
                        json!([choice.clone(), choice]);
                }
                15 => {
                    let pool = plan["carried_item_pools"]["supplies"].clone();
                    plan["carried_item_pools"][""] = pool;
                }
                16 => value["archetypes"]["stock"]["properties"] = json!({"invalid":"\n"}),
                17 => value["archetypes"]["stock"]["supply"] = json!("missing"),
                18 => {
                    plan["carried_item_pools"]["unused"] = json!({"version":1,
                            "choices":[{"archetype":"marker","weight":1,"quantity":2}]});
                }
                _ => unreachable!(),
            }
        });
        assert!(
            invalid.is_err() || invalid.unwrap().build(42, false).is_err(),
            "invalid carried stock case {case}"
        );
    }
}

#[test]
fn carried_stock_source_pool_map_rejects_duplicates_and_over_limit_before_compiling() {
    let pool = r#"{"version":1,"choices":[{"archetype":"marker","weight":1,"quantity":1}]}"#;
    let duplicate = format!(
        r#"{{"version":1,"protagonist":"selected_character","profiles":{{}},"carried_item_pools":{{"supplies":{pool},"supplies":{pool}}}}}"#
    );
    assert!(serde_json::from_str::<FloorActivation>(&duplicate).is_err());
    let too_many = json!({"version":1,"protagonist":"selected_character","profiles":{},
            "carried_item_pools":(0..33).map(|n| (format!("pool{n}"),
                json!({"version":1,"choices":[{"archetype":"marker","weight":1,"quantity":1}]})))
                .collect::<serde_json::Map<String, Value>>()});
    assert!(serde_json::from_value::<FloorActivation>(too_many).is_err());
    let duplicate_toml = r#"
version = 1
protagonist = "selected_character"
profiles = {}
[carried_item_pools.supplies]
version = 1
choices = [{ archetype = "marker", weight = 1, quantity = 1 }]
[carried_item_pools.supplies]
version = 1
choices = [{ archetype = "marker", weight = 1, quantity = 1 }]
"#;
    assert!(toml::from_str::<FloorActivation>(duplicate_toml).is_err());
}

#[test]
fn carried_stock_source_fields_are_actor_choice_only_and_strict() {
    let actor = json!({"kind":"actors","id":"inhabitants","version":1,"count":[1,1],
    "bands":[{"depth":{"kind":"open","minimum":1},"choices":[{
        "archetype":"marker","weight":1,
        "carried_loot":{"version":1,"pool":"supplies","chance":{"numerator":1,"denominator":1}}
    }]}]});
    assert!(serde_json::from_value::<PopulationStage>(actor.clone()).is_ok());
    for case in 0..5 {
        let mut invalid = actor.clone();
        match case {
            0 => invalid["kind"] = json!("items"),
            1 => invalid["bands"][0]["choices"][0]["carried_loot"]["equipment_slot"] = json!(0),
            2 => {
                invalid["bands"][0]["choices"][0]["carried_loot"]["chance"]["reroll"] = json!(true)
            }
            3 => invalid["bands"][0]["choices"][0]["quantity"] = json!(1),
            4 => invalid["carried_loot"] = json!({"version":1}),
            _ => unreachable!(),
        }
        assert!(
            serde_json::from_value::<PopulationStage>(invalid).is_err(),
            "case {case}"
        );
    }
}

#[test]
fn activation_authoring_rejects_unknown_mismatched_duplicate_and_unbounded_inputs() {
    let package = fixture();
    package.check().unwrap();
    for case in 0..12 {
        let package = changed(&package, |value| match case {
            0 => value["floor_activation"]["version"] = json!(2),
            1 => value["floor_activation"]["protagonist"] = json!(2),
            2 => value["floor_activation"]["protagonist"] = json!("controller"),
            3 => value["floor_activation"]["families"] = json!({"unknown":"plain"}),
            4 => value["floor_activation"]["families"] = json!({"dungeon":"unknown"}),
            5 => value["floor_activation"]["profiles"]["plain"]["version"] = json!(2),
            6 => {
                value["floor_activation"]["profiles"]["plain"]["stages"][0]["count"] = json!([2, 1])
            }
            7 => {
                value["floor_activation"]["profiles"]["plain"]["stages"][0]["count"] =
                    json!([257, 257])
            }
            8 => {
                value["floor_activation"]["profiles"]["plain"]["stages"][0]["bands"][0]["choices"] =
                    json!([{"archetype":"marker","weight":1}, {"archetype":"marker","weight":1}])
            }
            9 => {
                value["floor_activation"]["profiles"]["plain"]["stages"][0]["bands"][0]["choices"] =
                    json!([{"archetype":"unknown","weight":1}])
            }
            10 => {
                let stage = value["floor_activation"]["profiles"]["plain"]["stages"][0].clone();
                value["floor_activation"]["profiles"]["plain"]["stages"]
                    .as_array_mut()
                    .unwrap()
                    .push(stage);
            }
            11 => {
                value["floor_activation"]["profiles"]["plain"]["stages"][0]["count"] =
                    json!([129, 129]);
                let mut stage = value["floor_activation"]["profiles"]["plain"]["stages"][0].clone();
                stage["id"] = json!("second");
                value["floor_activation"]["profiles"]["plain"]["stages"]
                    .as_array_mut()
                    .unwrap()
                    .push(stage);
            }
            _ => unreachable!(),
        })
        .unwrap();
        assert!(package.check().is_err(), "invalid population case {case}");
    }
}

#[test]
fn population_stage_schema_is_typed_instead_of_an_expression_language() {
    let base = json!({"kind":"items","id":"stock","version":1,"count":[1,1],
            "bands":[{"depth":{"kind":"open","minimum":1},
            "choices":[{"archetype":"marker","weight":1}]}]});
    for case in 0..4 {
        let mut stage = base.clone();
        match case {
            0 => stage["kind"] = json!("script"),
            1 => stage["ai"] = json!("wander"),
            2 => {
                stage["kind"] = json!("actors");
                stage["quantity"] = json!(1);
            }
            3 => stage["when"] = json!("inventory.ration == 0"),
            _ => unreachable!(),
        }
        assert!(serde_json::from_value::<PopulationStage>(stage).is_err());
    }
}

#[test]
fn room_gold_source_free_package_preserves_activated_and_unentered_scope_metadata() {
    room_gold_source_free_metadata(None, false);
}

#[test]
fn omitted_rooms_have_no_gold_receipts_or_metadata_after_source_free_restore() {
    room_gold_source_free_metadata(Some(3), false);
}

#[test]
fn maze_rooms_keep_actual_gold_cells_in_source_free_future_floors() {
    room_gold_source_free_metadata(Some(3), true);
}

fn room_gold_source_free_metadata(omissions: Option<u32>, mazes: bool) {
    let baseline = fixture();
    baseline.check().unwrap();
    let package = changed(&baseline, |value| {
            if let Some(count) = omissions {
                let recipe = &mut value["generation_recipes"]["exploration"];
                recipe["version"] = json!(4);
                recipe["stages"].as_array_mut().unwrap().insert(6, json!({
                    "id":"room_omissions", "version":1,
                    "operation":{"kind":"room_omissions", "config":{"count":[count,count]}}
                }));
            }
            if mazes {
                let recipe = &mut value["generation_recipes"]["exploration"];
                recipe["version"] = json!(5);
                let stages = recipe["stages"].as_array_mut().unwrap();
                let position = 6 + usize::from(omissions.is_some());
                assert_eq!(stages[position]["operation"]["kind"], json!("room_lighting"));
                stages[position]["operation"]["darkness_roll"] = json!(1);
                stages[position]["operation"]["darkness_start"] = json!(0);
                stages.insert(position, json!({"id":"maze_rooms", "version":1,
                    "operation":{"kind":"maze_rooms", "config":{"one_in":1}}}));
            }
            value["archetypes"]["marker"]["stackable"] = json!(true);
            value["floor_activation"]["profiles"]["plain"]["stages"] = json!([{
                "kind":"room_items","id":"room-gold","version":1,
                "quantity":{"kind":"depth_scaled_uniform","minimum":2,"maximum_base":51,"per_depth":10},
                "chance":{"numerator":1,"denominator":2},
                "bands":[{"depth":{"kind":"open","minimum":1},"choices":[{"archetype":"marker","weight":1}]}]
            }]);
        }).unwrap();
    let bytes = crate::storage::schema::scenario::encode_content_package(&package).unwrap();
    let restored = crate::storage::schema::scenario::decode_content_package(&bytes).unwrap();
    assert!(restored.directory.is_none());
    assert!(restored.rules_source.is_empty());
    let mut warm = package.build(42, false).unwrap();
    let cold = restored.build(42, false).unwrap();
    assert_eq!(warm.activations_since(0), cold.activations_since(0));
    assert_eq!(
        warm.activation_scopes().collect::<Vec<_>>(),
        cold.activation_scopes().collect::<Vec<_>>()
    );
    let generated = package.generated_group("dungeon/2", 42).unwrap();
    let source_free_future = restored.generated_group("dungeon/2", 42).unwrap();
    assert_eq!(
        serde_json::to_value(&generated.regions).unwrap(),
        serde_json::to_value(&source_free_future.regions).unwrap()
    );
    assert_eq!(generated.rooms, source_free_future.rooms);
    if mazes {
        let baseline = baseline.generated_group("dungeon/2", 42).unwrap();
        assert!(
            generated.rooms.iter().any(|room| {
                room.cells.len()
                    < baseline
                        .rooms
                        .iter()
                        .find(|old| old.id == room.id)
                        .unwrap()
                        .cells
                        .len()
            }),
            "source-free witness must actually contain carved maze walls"
        );
        for room in &generated.rooms {
            for cell in &room.cells {
                let region = &generated.regions[&cell.region.0];
                assert!((0..2).all(|z| !region.walls.contains(&[
                    cell.position.x,
                    cell.position.y,
                    z
                ])));
            }
        }
    }

    assert_eq!(
        generated.regions.len(),
        9,
        "omissions never remove region identities"
    );
    assert_eq!(generated.rooms.len(), 9 - omissions.unwrap_or(0) as usize);
    let definitions: Vec<_> = generated.regions.values().collect();
    let unentered = package
        .activation_scope("dungeon/2", 2, &definitions, &generated.rooms)
        .unwrap()
        .unwrap();
    warm.register_activation_scope(unentered).unwrap();
    assert_eq!(warm.activation_count(), 1);
    let configuration = restored.population_configuration(42).unwrap();
    assert!(configuration.valid_game(&warm));
    for original in warm.activation_scopes() {
        assert!(configuration.valid_scope(original));
        let mut corrupt = original.clone();
        corrupt.rooms.remove(0);
        assert!(!configuration.valid_scope(&corrupt));
        let mut corrupt = original.clone();
        corrupt.rooms[0].id = 64;
        assert!(!configuration.valid_scope(&corrupt));
        let mut corrupt = original.clone();
        corrupt.rooms[0].cells.pop();
        assert!(!configuration.valid_scope(&corrupt));
    }
    let record = warm.activations_since(0).unwrap().remove(0);
    let initial = warm
        .activation_scopes()
        .find(|scope| scope.scope == "dungeon/1")
        .unwrap();
    assert_eq!(initial.members.len(), 9);
    assert_eq!(initial.rooms.len(), 9 - omissions.unwrap_or(0) as usize);
    let runtime::StageResult::RoomsEvaluated { rooms } = &record.decisions[0].result else {
        panic!("room Gold must decide once for each real room");
    };
    assert_eq!(
        rooms.iter().map(|room| room.room).collect::<Vec<_>>(),
        initial.rooms.iter().map(|room| room.id).collect::<Vec<_>>()
    );
    assert!(initial.rooms.windows(2).all(|pair| pair[0].id < pair[1].id));
    assert!(initial.rooms.iter().all(|room| !room.cells.is_empty()));
    assert!(configuration.valid_record(&record));
    let mut missing_room = record;
    let runtime::StageResult::RoomsEvaluated { rooms } = &mut missing_room.decisions[0].result
    else {
        panic!()
    };
    rooms.remove(0);
    assert!(!configuration.valid_record(&missing_room));
    assert_eq!(
        crate::storage::schema::scenario::encode_content_package(&restored).unwrap(),
        bytes
    );
}

#[test]
fn quantity_expressions_check_depth_overflow_and_stackability_before_population() {
    for quantity in [
        json!({"kind":"uniform","minimum":0,"maximum":10}),
        json!({"kind":"uniform","minimum":3,"maximum":2}),
        json!({"kind":"depth_scaled_uniform","minimum":2,"maximum_base":u64::MAX,"per_depth":1}),
        json!({"kind":"depth_scaled_uniform","minimum":2,"maximum_base":1,"per_depth":u64::MAX}),
    ] {
        let invalid = changed(&fixture(), |value| {
            value["floor_activation"]["profiles"]["plain"]["stages"][0]["quantity"] = quantity;
        })
        .unwrap();
        assert!(invalid.check().is_err());
    }
    let nonstackable = changed(&fixture(), |value| {
        value["floor_activation"]["profiles"]["plain"]["stages"][0]["quantity"] =
            json!({"kind":"uniform","minimum":1,"maximum":2});
    })
    .unwrap();
    assert!(nonstackable
        .population_plan(&nonstackable.index(42).unwrap())
        .is_err());
}

#[test]
fn group_preparation_contains_only_canonical_immutable_activation_geometry() {
    let package = fixture();
    let index = package.index(42).unwrap();
    let first = package.build_group(42, &index, RegionId(1)).unwrap();
    let second = package.build_group(42, &index, RegionId(1)).unwrap();
    assert_eq!(first.records, second.records);
    assert_eq!(first.activation, second.activation);
    let scope = first.activation.unwrap();
    assert_eq!(scope.scope, "dungeon/1");
    assert_eq!(scope.profile, "plain");
    assert_eq!(scope.depth, 1);
    assert_eq!(scope.members.len(), 9);
    assert!(!scope.cells.is_empty());
    assert!(scope.cells.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(!scope.protected.is_empty());
    // A plan has an initial marker; scratch records have no such allocation.
    let mut plain = package.manifest.clone();
    plain.floor_activation = None;
    let plain = Package::from_parts_scripted(
        plain,
        vec![],
        &package.rules_source,
        package.scripts.clone(),
    )
    .unwrap();
    assert_eq!(
        first.records,
        plain
            .build_group(42, &plain.index(42).unwrap(), RegionId(1))
            .unwrap()
            .records
    );
}

#[test]
fn omitted_character_namespace_is_reserved_before_dynamic_population() {
    let package = fixture();
    let mut package = changed(&package, |value| {
        value["characters"].as_array_mut().unwrap().push(json!({
            "id":900,"anchor":"dungeon/1/start","unselected":"omit"
        }));
    })
    .unwrap();
    let first = package.index(42).unwrap();
    assert!(first.ceilings.0 > 900);
    assert!(!first.homes.contains_key(&900));
    package.selected = 900;
    let alternate = package.index(42).unwrap();
    assert_eq!(first.ceilings.0, alternate.ceilings.0);
    assert!(!alternate.homes.contains_key(&1));
}

fn item_reservation_package(item_id: u64) -> Package {
    let package = changed(&fixture(), |value| {
        value["characters"][0]["loadout"] = json!([
            {"archetype":"marker","quantity":1},
            {"archetype":"marker","quantity":1}
        ]);
    })
    .unwrap();
    // Open families reserve their full address domain, independently of
    // the smaller materialization depth budget used by this fixture.
    let authored_region = package.manifest.procedural_worlds["dungeon"]
        .last_region()
        .unwrap()
        .checked_add(1)
        .unwrap();
    let region = serde_json::from_value(json!({
        "id":authored_region,"name":"Store room","size":[3,3,1],
        "items":[{"id":item_id,"at":[1,1,0],"archetype":"marker"}]
    }))
    .unwrap();
    Package::from_parts_scripted(
        package.manifest,
        vec![region],
        &package.rules_source,
        package.scripts.clone(),
    )
    .unwrap()
}

#[test]
fn population_item_floor_includes_authored_reservations_and_selected_kit() {
    let package = item_reservation_package(100);
    let game = package.build(42, false).unwrap();
    let mut record = game.activations_since(0).unwrap().remove(0);
    let configuration = package.population_configuration(42).unwrap();
    assert!(configuration.valid_record(&record));
    let runtime::StageResult::Evaluated { attempts, .. } = &record.decisions[0].result else {
        panic!("fixture must evaluate its marker stage");
    };
    let runtime::PlacementDecision::Placed {
        entity: runtime::PopulationEntity::Item { id, .. },
        ..
    } = &attempts[0]
    else {
        panic!("fixture must place a population item");
    };
    assert_eq!(
        id.0, 103,
        "population starts immediately after both kit rows"
    );
    for reserved in [0, 100, 101, 102] {
        let runtime::StageResult::Evaluated { attempts, .. } = &mut record.decisions[0].result
        else {
            unreachable!();
        };
        let runtime::PlacementDecision::Placed { entity, .. } = &mut attempts[0] else {
            unreachable!();
        };
        *entity = runtime::PopulationEntity::Item {
            id: tor_simulation::ItemId(reserved),
            quantity: 1,
        };
        assert!(
            !configuration.valid_record(&record),
            "authored or selected-kit identity {reserved} is not a population origin"
        );
    }
}

#[test]
fn carried_stock_nested_birth_rejects_authored_and_selected_kit_identity_aliases() {
    let package = item_reservation_package(100);
    let game = package.build(42, false).unwrap();
    let mut record = game.activations_since(0).unwrap().remove(0);
    let configuration = package.population_configuration(42).unwrap();
    for item in [103, 0, 100, 101, 102] {
        let runtime::StageResult::Evaluated { attempts, .. } = &mut record.decisions[0].result
        else {
            unreachable!()
        };
        let runtime::PlacementDecision::Placed { entity, .. } = &mut attempts[0] else {
            unreachable!()
        };
        *entity = runtime::PopulationEntity::Actor {
            id: tor_simulation::ActorId(configuration.actor_floor),
            carried_loot: Some(runtime::CarriedLootDecision::Carried {
                content: "marker".into(),
                quantity: 1,
                item: tor_simulation::ItemId(item),
            }),
        };
        assert_eq!(
            configuration.valid_record(&record),
            item == 103,
            "nested item {item} must obey the same historical reservation floor"
        );
    }
    let runtime::StageResult::Evaluated { attempts, .. } = &mut record.decisions[0].result else {
        unreachable!()
    };
    let runtime::PlacementDecision::Placed { entity, .. } = &mut attempts[0] else {
        unreachable!()
    };
    *entity = runtime::PopulationEntity::Actor {
        id: tor_simulation::ActorId(configuration.actor_floor - 1),
        carried_loot: None,
    };
    assert!(
        !configuration.valid_record(&record),
        "ordinary actor floor remains enforced"
    );
}

#[test]
fn population_item_floor_overflow_rejects_recovery_configuration() {
    let package = item_reservation_package(u64::MAX - 1);
    assert_eq!(package.index(42).unwrap().ceilings.1, u64::MAX);
    assert!(package.population_configuration(42).is_err());
}

#[test]
fn initial_loadout_is_selected_only_and_uses_ordinary_quantity_validation() {
    let package = fixture();
    let package = changed(&package, |value| {
        value["characters"][0]["loadout"] = json!([{"archetype":"marker","quantity":2}]);
    })
    .unwrap();
    assert!(
        package.build(42, false).is_err(),
        "non-stackable loadout quantity rejected"
    );
    let package = changed(&package, |value| {
        value["characters"][0]["loadout"][0]["quantity"] = json!(1);
        value["characters"][0]["loadout"][0]["equipped_slot"] = json!(0);
    })
    .unwrap();
    assert!(
        package.build(42, false).is_err(),
        "non-equipment loadout cannot equip"
    );
}
// Insert inside scenario_population::tests. Independent Python fixture owns
// exact product weights; this checks the real compiler + source-free boundary.
#[test]
fn actual_rogue_birth_pools_compile_and_round_trip_without_source_files() {
    for name in ["rogue", "rogue-exploration"] {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios")
            .join(name);
        let package = read_package(&root).unwrap();
        package.check().unwrap();
        let index = package.index(42).unwrap();
        let original = package.population_plan(&index).unwrap().unwrap();
        let pool = &original.carried_item_pools["rogue.vampire_supplies"];
        assert_eq!(pool.choices.len(), 79, "{name}");
        assert!(pool.choices.iter().all(|choice| {
            !choice.spec.identity.is_unknown()
                && choice.spec.identity.as_str().starts_with("item.")
                && choice.quantity.resolve(Some(1)).is_ok()
        }));
        let dagger = pool
            .choices
            .iter()
            .find(|choice| choice.content == "rogue.dagger")
            .unwrap();
        assert_eq!(
            dagger.quantity,
            tor_simulation::quantity::QuantityExpr::Uniform {
                minimum: 2,
                maximum: 5
            }
        );
        assert!(dagger.spec.stackable);
        let arrow = pool
            .choices
            .iter()
            .find(|choice| choice.content == "rogue.generated_arrow")
            .unwrap();
        assert_eq!(
            arrow.quantity,
            tor_simulation::quantity::QuantityExpr::Uniform {
                minimum: 8,
                maximum: 15
            }
        );
        assert_eq!(
            arrow.spec.equipment_generation,
            Some(tor_simulation::EquipmentGeneration::Weapon {
                curse_percent: 10,
                enchant_percent: 5,
                minimum: 1,
                maximum: 3,
            })
        );
        let bytes = crate::storage::schema::scenario::encode_content_package(&package).unwrap();
        let restored = crate::storage::schema::scenario::decode_content_package(&bytes).unwrap();
        assert!(restored.directory.is_none());
        assert!(restored.rules_source.is_empty());
        let restored_index = restored.index(42).unwrap();
        let decoded = restored.population_plan(&restored_index).unwrap().unwrap();
        assert_eq!(
            decoded.carried_item_pools, original.carried_item_pools,
            "{name}"
        );
        assert_eq!(
            crate::storage::schema::scenario::encode_content_package(&restored).unwrap(),
            bytes
        );
        let encoded: Value = serde_json::from_slice(&bytes).unwrap();
        let pool_shape = encoded["package"]["manifest"]["floor_activation"]["carried_item_pools"]
            ["rogue.vampire_supplies"]
            .as_object()
            .unwrap();
        assert_eq!(
            pool_shape.keys().map(String::as_str).collect::<Vec<_>>(),
            ["choices", "version"]
        );
        for choice in pool_shape["choices"].as_array().unwrap() {
            assert_eq!(
                choice
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                ["archetype", "quantity", "weight"]
            );
        }
    }
}
