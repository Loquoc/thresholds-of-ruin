fn identity_contract_definitions() -> compiler::PreparedDefinitions {
    let manifest: Manifest =
        toml::from_str(include_str!("../../../scenarios/tests/items/scenario.toml")).unwrap();
    compiler::PreparedDefinitions::new(&manifest, 1).unwrap()
}

#[test]
fn concrete_authored_identity_is_required_known_leaf_and_not_derived_from_name() {
    let definitions = identity_contract_definitions();
    let mut value =
        serde_json::json!({"id":99,"at":[1,1,0],"class":"potion","name":"potion of healing"});
    let missing: Item = serde_json::from_value(value.clone()).unwrap();
    assert!(definitions
        .item(&missing, 0, &BTreeMap::new())
        .unwrap_err()
        .message
        .contains("explicit canonical identity"));
    for key in ["item", "item.potion", "item.potion.unknown"] {
        value["identity"] = serde_json::json!(key);
        let invalid: Item = serde_json::from_value(value.clone()).unwrap();
        assert!(
            definitions
                .item(&invalid, 0, &BTreeMap::new())
                .unwrap_err()
                .message
                .contains("known leaf"),
            "{key}"
        );
    }
    value["identity"] = serde_json::json!("item.potion.healing");
    let valid: Item = serde_json::from_value(value.clone()).unwrap();
    let first = definitions.item(&valid, 0, &BTreeMap::new()).unwrap();
    value["name"] = serde_json::json!("arbitrary translated label");
    let renamed: Item = serde_json::from_value(value).unwrap();
    let second = definitions.item(&renamed, 0, &BTreeMap::new()).unwrap();
    assert_eq!(first.identity, second.identity);
    assert_ne!(first.name, second.name);
}

#[test]
fn wizard_item_resolves_pinned_identity_appearance_and_equivalent_alias() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/items");
    let mut package = super::read_package(&root).unwrap();
    let arrow = package.manifest.archetypes.get_mut("arrow").unwrap();
    arrow.asset = Some("item.weapon.arrow".into());
    let alias = arrow.clone();
    package.manifest.archetypes.insert("a-arrow".into(), alias);
    let pool = package
        .manifest
        .appearance_pools
        .get_mut("potions")
        .unwrap();
    pool.appearances[0].asset = Some("item.potion.red".into());

    let index = package.index(42).unwrap();
    let arrow_identity = "item.weapon.arrow";
    let healing = "item.potion.healing";
    let red = "item.potion.red";
    let catalog = index.wizard_item_catalog(None).unwrap();
    let aliases = catalog
        .rows
        .iter()
        .filter(|row| row.identity.as_str() == arrow_identity)
        .map(|row| row.recipe.as_str())
        .collect::<Vec<_>>();
    assert_eq!(aliases, ["a-arrow", "arrow"]);

    let by_identity = index.wizard_item(Some(arrow_identity), None, None).unwrap();
    assert_eq!(by_identity.archetype, "a-arrow");
    assert_eq!(by_identity.identity.as_str(), arrow_identity);
    assert!(by_identity.stackable);
    assert_eq!(
        index.wizard_item(None, Some(arrow_identity), None).unwrap(),
        by_identity
    );

    // Both potions use the one confounding pool entry. Match the already
    // allocated name and asset together, never invent a pairing.
    let allocated = index.wizard_item(Some(healing), Some(red), None).unwrap();
    assert_eq!(
        (
            allocated.identity.as_str(),
            allocated.appearance.as_str(),
            allocated.asset.as_deref()
        ),
        (healing, "red potion", Some(red))
    );
    assert!(index
        .wizard_item(None, Some(red), None)
        .unwrap_err()
        .message
        .contains("Ambiguous"));
    assert!(index
        .wizard_item(Some(arrow_identity), Some(red), None)
        .is_err());
    assert!(index
        .wizard_item(Some("item.weapon.unknown"), None, None)
        .is_err());
    assert!(index
        .wizard_item(Some("item.weapon.missing"), None, None)
        .is_err());
    assert!(index.wizard_item(None, Some("red potion"), None).is_err());
    assert!(index
        .wizard_item(None, Some("item.potion.absent"), None)
        .is_err());
    assert!(index.wizard_item(None, None, None).is_err());

    // Same item identity is not enough to prove stackability equivalence.
    package
        .manifest
        .archetypes
        .get_mut("a-arrow")
        .unwrap()
        .stackable = false;
    let index = package.index(42).unwrap();
    assert!(index
        .wizard_item(Some(arrow_identity), None, None)
        .unwrap_err()
        .message
        .contains("Ambiguous"));
}

#[test]
fn wizard_item_catalog_pages_all_rogue_recipes_with_seeded_assets() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let package = super::read_package(&root).unwrap();
    let index = package.index(42).unwrap();
    let first = index.wizard_item_catalog(None).unwrap();
    assert_eq!(first.rows.len(), 16);
    assert_eq!(
        first.next.as_deref(),
        first.rows.last().map(|row| row.recipe.as_str())
    );
    let second = index.wizard_item_catalog(first.next.as_deref()).unwrap();
    assert_eq!(second.rows.len(), 16);
    assert!(first.rows.last().unwrap().recipe < second.rows[0].recipe);
    assert_eq!(index.wizard_item_catalog(None).unwrap(), first);
    let mut rows = first.rows;
    let mut next = first.next;
    while let Some(cursor) = next {
        let page = index.wizard_item_catalog(Some(&cursor)).unwrap();
        assert!((1..=16).contains(&page.rows.len()));
        rows.extend(page.rows);
        next = page.next;
        assert!(rows.len() <= package.manifest.archetypes.len());
    }
    let expected: Vec<_> = package
        .manifest
        .archetypes
        .iter()
        .filter_map(|(key, author)| {
            (author.identity.is_some() && (author.name.is_some() || !author.forms.is_empty()))
                .then_some(key.as_str())
        })
        .collect();
    assert_eq!(
        rows.iter()
            .map(|row| row.recipe.as_str())
            .collect::<Vec<_>>(),
        expected
    );
    let find = |key| rows.iter().find(|row| row.recipe == key).unwrap();
    assert_eq!(
        find("rogue.hallucination").identity.as_str(),
        "item.potion.hallucination"
    );
    assert_eq!(
        find("rogue.hallucination").appearance.as_deref(),
        Some("item.potion.saffron")
    );
    for (base, generated) in [
        ("rogue.mace", "rogue.generated_mace"),
        ("rogue.arrow", "rogue.generated_arrow"),
        ("rogue.ring_mail", "rogue.generated_ring_mail"),
    ] {
        assert_eq!(find(base).identity, find(generated).identity);
        assert_eq!(find(base).appearance, find(generated).appearance);
    }
    assert!(index
        .wizard_item_catalog(Some("rogue.unknown-recipe"))
        .is_err());
    let (creature, _) = index.creature_archetypes().next().unwrap();
    assert!(index.wizard_item_catalog(Some(creature)).is_err());
    let exhausted = index
        .wizard_item_catalog(Some(&rows.last().unwrap().recipe))
        .unwrap();
    assert!(exhausted.rows.is_empty() && exhausted.next.is_none());
    assert_eq!(
        index.lookups.get(),
        0,
        "catalog must not materialize regions"
    );
}

#[test]
fn wizard_item_exact_rogue_recipe_resolves_distinct_birth_policies() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let package = super::read_package(&root).unwrap();
    let index = package.index(42).unwrap();
    for (identity, appearance, ordinary, generated) in [
        (
            "item.weapon.mace",
            "item.weapon.mace",
            "rogue.mace",
            "rogue.generated_mace",
        ),
        (
            "item.weapon.arrow",
            "item.weapon.arrow",
            "rogue.arrow",
            "rogue.generated_arrow",
        ),
        (
            "item.armor.ring-mail",
            "item.armor.mail.ring",
            "rogue.ring_mail",
            "rogue.generated_ring_mail",
        ),
    ] {
        for selectors in [
            (Some(identity), None),
            (None, Some(appearance)),
            (Some(identity), Some(appearance)),
        ] {
            assert!(index
                .wizard_item(selectors.0, selectors.1, None)
                .unwrap_err()
                .message
                .contains("Ambiguous"));
        }
        let base = index.wizard_item(None, None, Some(ordinary)).unwrap();
        assert_eq!(base.archetype, ordinary);
        assert_eq!(base.identity.as_str(), identity);
        assert_eq!(base.asset.as_deref(), Some(appearance));
        assert_eq!(
            base.equipment_generation.is_none(),
            ordinary == "rogue.arrow"
        );
        let loot = index
            .wizard_item(Some(identity), Some(appearance), Some(generated))
            .unwrap();
        assert_eq!(loot.archetype, generated);
        assert!(loot.equipment_generation.is_some());
        assert_ne!(loot.equipment_generation, base.equipment_generation);
        assert!(index
            .wizard_item(Some("item.potion.hallucination"), None, Some(ordinary))
            .is_err());
        assert!(index
            .wizard_item(None, Some("item.potion.saffron"), Some(ordinary))
            .is_err());
    }
    for invalid in ["", "rogue missing", "rogue.no-such-recipe"] {
        assert!(index.wizard_item(None, None, Some(invalid)).is_err());
    }
    let (creature, _) = index.creature_archetypes().next().unwrap();
    assert!(index.wizard_item(None, None, Some(creature)).is_err());
}

#[test]
fn seeded_item_names_keep_same_index_explicit_identity_and_reject_legacy_strings() {
    let definitions = identity_contract_definitions();
    let value = serde_json::json!({"id":99,"at":[1,1,0],"seed_names":[
        {"name":"copper token","identity":"item.misc.copper-token"},
        {"name":"silver token","identity":"item.misc.silver-token"},
        {"name":"iron token","identity":"item.misc.iron-token"}
    ]});
    let valid: Item = serde_json::from_value(value.clone()).unwrap();
    let expected = [
        ("copper token", "item.misc.copper-token"),
        ("silver token", "item.misc.silver-token"),
        ("iron token", "item.misc.iron-token"),
    ];
    for seed in 0..9 {
        let item = definitions.item(&valid, seed, &BTreeMap::new()).unwrap();
        let (name, identity) = expected[(seed % 3) as usize];
        assert_eq!(item.name, name);
        assert_eq!(item.identity.as_str(), identity);
    }
    let mut legacy = value;
    legacy["seed_names"] = serde_json::json!(["copper token"]);
    assert!(serde_json::from_value::<Item>(legacy).is_err());
}

#[test]
fn unselected_seeded_identity_is_also_validated_before_item_creation() {
    let definitions = identity_contract_definitions();
    for bad in ["item", "item.misc", "item.misc.unknown"] {
        let value = serde_json::json!({"id":99,"at":[1,1,0],"seed_names":[
            {"name":"good","identity":"item.misc.good"},
            {"name":"invalid later choice","identity":bad}
        ]});
        let invalid: Item = serde_json::from_value(value).unwrap();
        assert!(
            definitions.item(&invalid, 0, &BTreeMap::new()).is_err(),
            "unselected {bad} must reject, not wait until another seed"
        );
    }
}

#[test]
fn unselected_seeded_identity_cannot_override_inherited_identity() {
    let definitions = identity_contract_definitions();
    let mut value = serde_json::json!({"id":99,"at":[1,1,0],"archetype":"arrow","seed_names":[
        {"name":"first public arrow label","identity":"item.weapon.arrow"},
        {"name":"later public arrow label","identity":"item.weapon.arrow"}
    ]});
    let valid: Item = serde_json::from_value(value.clone()).unwrap();
    for seed in [0, 1] {
        assert_eq!(
            definitions
                .item(&valid, seed, &BTreeMap::new())
                .unwrap()
                .identity
                .as_str(),
            "item.weapon.arrow"
        );
    }
    value["seed_names"][1]["identity"] = serde_json::json!("item.weapon.dagger");
    let invalid: Item = serde_json::from_value(value).unwrap();
    assert!(
        definitions.item(&invalid, 0, &BTreeMap::new()).is_err(),
        "a currently unselected known key cannot override the inherited identity"
    );
}

fn care_alias_package(replacement: &str) -> Result<Package, Failure> {
    let manifest: Manifest =
        toml::from_str(include_str!("../../../scenarios/two-room/scenario.toml")).unwrap();
    let regions: Vec<RegionDef> = [
        include_str!("../../../scenarios/two-room/regions/1.toml"),
        include_str!("../../../scenarios/two-room/regions/2.toml"),
    ]
    .into_iter()
    .map(|source| toml::from_str(source).unwrap())
    .collect();
    let original = tor_simulation::rules::DEFAULT_RULESET_TOML;
    let named = "reach = { kind = \"self_or_ally\", range = { kind = \"touch\" } }";
    assert_eq!(original.matches(named).count(), 1);
    Package::from_parts(manifest.clone(), regions.clone(), original).unwrap();
    let source = original.replacen(named, replacement, 1);
    assert!(toml::from_str::<toml::Value>(&source).is_ok());
    Package::from_parts(manifest, regions, &source)
}

#[test]
fn palette_budget_reserves_full_and_delta_envelopes_at_the_boundary() {
    let assets = BTreeSet::from(["creature.equine.quagga".into(), "item.potion".into()]);
    let encoded = serde_json::to_vec(&assets).unwrap().len();
    assert!(check_palette_budget(&assets, PALETTE_ENVELOPE_RESERVE + encoded).is_ok());
    assert!(check_palette_budget(&assets, PALETTE_ENVELOPE_RESERVE + encoded - 1).is_err());
    assert!(check_palette_budget(&assets, PALETTE_ENVELOPE_RESERVE - 1).is_err());
    // Exercise real envelopes too, including escaped context/request IDs
    // and the longest numeric revisions. Delta sets partition the same
    // package-wide union; they never double its serialized asset payload.
    let context: tor_protocol::ReplyContext = serde_json::from_value(serde_json::json!({
        "snapshot":"\\".repeat(128),
        "input": {"stream": {"stream": "\\".repeat(128), "epoch": u64::MAX.to_string()},
                  "readiness_revision": u64::MAX.to_string()},
        "actor": u64::MAX.to_string(), "branch": "\\".repeat(128),
        "cursor": {"sequence": u64::MAX.to_string(), "tick": u64::MAX.to_string()},
        "revision": u64::MAX.to_string(),
    }))
    .unwrap();
    for body in [
        tor_protocol::PaletteBody::Full {
            assets: assets.clone(),
        },
        tor_protocol::PaletteBody::Delta {
            base: u64::MAX - 1,
            added: assets.iter().take(1).cloned().collect(),
            removed: assets.iter().skip(1).cloned().collect(),
        },
    ] {
        let message = tor_protocol::ServerMessage::Palette {
            context: context.clone(),
            request_id: Some("\\".repeat(128)),
            palette: tor_protocol::PaletteUpdate {
                revision: u64::MAX,
                body,
            },
        };
        assert!(
            tor_protocol::encode_bounded_json(&message, PALETTE_ENVELOPE_RESERVE + encoded).is_ok()
        );
    }
}

#[test]
fn phase5_authoring_package_rejects_positional_reach_alias() {
    assert!(care_alias_package("reach = [\"self_or_ally\", { kind = \"touch\" }]").is_err());
}

#[test]
fn phase5_authoring_package_rejects_positional_ally_range_alias() {
    assert!(
        care_alias_package("reach = { kind = \"self_or_ally\", range = [\"touch\"] }").is_err()
    );
}

#[test]
fn phase5_container_archetype_rejects_positional_fields() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let package = read_package(&root).unwrap();
    let mut manifest = package.manifest.clone();
    manifest.archetypes.insert(
        "bandage".into(),
        Archetype {
            supply: Some("bandage".into()),
            name: Some("Bandage".into()),
            stackable: true,
            ..Default::default()
        },
    );
    // Ordinary compiler validation proves this is a legal supply archetype.
    PreparedDefinitions::new(&manifest, 1).unwrap();
    let mut value = serde_json::to_value(&manifest.archetypes["bandage"]).unwrap();
    let decoded: Archetype = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    // Creature is the only omitted optional serialized field.
    value
        .as_object_mut()
        .unwrap()
        .entry("creature")
        .or_insert(serde_json::Value::Null);
    let object = value.as_object().unwrap();
    let fields = [
        "creature",
        "anatomy",
        "equipment",
        "projectile",
        "equipment_generation",
        "consumable",
        "readable",
        "ground_scare",
        "device",
        "held_light",
        "edible",
        "supply",
        "magical",
        "currency",
        "class",
        "body",
        "identity",
        "appearance_pool",
        "forms",
        "stackable",
        "properties",
        "name",
        "turn_ticks",
        "asset",
    ];
    assert_eq!(object.len(), fields.len());
    let positional: Vec<_> = fields
        .iter()
        .map(|field| object.get(*field).unwrap().clone())
        .collect();
    assert!(serde_json::from_value::<Archetype>(serde_json::json!(positional)).is_err());
}

#[test]
fn compiled_inputs_reject_unsupported_source_headers_even_with_current_identity() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let original = read_package(&root).unwrap();
    let encode = crate::storage::schema::scenario::encode_content_package;
    let decode = crate::storage::schema::scenario::decode_content_package;
    decode(&encode(&original).unwrap()).unwrap();
    for format in [0, FORMAT - 1, FORMAT + 1] {
        let mut package = original.clone();
        package.manifest.format = format;
        // Source-format metadata is deliberately outside semantic identity.
        // Current hashes and validator evidence must not bypass admission.
        assert_eq!(
            package.certificate.model_hash,
            model_hash(
                &package.manifest,
                &package.index,
                &package.certificate.ruleset_hash,
                &package.certificate.ruleset_dependencies,
            )
            .unwrap()
        );
        assert!(
            decode(&encode(&package).unwrap()).is_err(),
            "format {format}"
        );
    }
    let mut package = original;
    package.manifest.version = "not-a-version".into();
    assert!(decode(&encode(&package).unwrap()).is_err());
}

#[test]
fn previous_draft_generator_validator_cannot_reuse_compiled_inputs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let mut package = read_package(&root).unwrap();
    package.check_identity().unwrap();
    let current_artifact = artifact::relative_root(&package);
    package.certificate.validator = "tor-scenario-13".into();
    let encoded = crate::storage::schema::scenario::encode_content_package(&package).unwrap();
    assert!(
        crate::storage::schema::scenario::decode_content_package(&encoded).is_err(),
        "Old generator evidence must not silently admit changed entry placement"
    );
    assert!(package.check_identity().is_err());
    assert_ne!(artifact::relative_root(&package), current_artifact);
}

#[test]
fn previous_campaign_validator_cannot_admit_changed_health_and_ledger_inputs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let mut package = read_package(&root).unwrap();
    package.check_identity().unwrap();
    for validator in ["tor-scenario-15", "tor-scenario-16"] {
        package.certificate.validator = validator.into();
        let encoded = crate::storage::schema::scenario::encode_content_package(&package).unwrap();
        assert!(crate::storage::schema::scenario::decode_content_package(&encoded).is_err());
        assert!(package.check_identity().is_err());
    }
}

#[test]
fn current_package_rejects_previous_validator_and_rules_identity() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let original = read_package(&root).unwrap();
    original.check_identity().unwrap();
    assert_eq!(original.certificate.validator, "tor-scenario-52");
    assert_eq!(original.certificate.ruleset, "interactions-v73");
    let current_artifact = artifact::relative_root(&original);
    for (boundary, identity) in [
        ("validator", "tor-scenario-27"),
        ("validator", "tor-scenario-28"),
        ("validator", "tor-scenario-29"),
        ("validator", "tor-scenario-30"),
        ("validator", "tor-scenario-31"),
        ("validator", "tor-scenario-32"),
        ("validator", "tor-scenario-47"),
        ("validator", "tor-scenario-51"),
        ("ruleset", "interactions-v38"),
        ("ruleset", "interactions-v39"),
        ("ruleset", "interactions-v40"),
        ("ruleset", "interactions-v41"),
        ("ruleset", "interactions-v42"),
        ("ruleset", "interactions-v56"),
        ("ruleset", "interactions-v58"),
        ("ruleset", "interactions-v60"),
    ] {
        let mut package = original.clone();
        if boundary == "validator" {
            package.certificate.validator = identity.into();
            assert_ne!(artifact::relative_root(&package), current_artifact);
        } else {
            package.certificate.ruleset = identity.into();
        }
        let encoded = crate::storage::schema::scenario::encode_content_package(&package).unwrap();
        assert!(crate::storage::schema::scenario::decode_content_package(&encoded).is_err());
        assert!(package.check_identity().is_err(), "{boundary}");
    }
    for previous in [
        "interactions-v35",
        "interactions-v56",
        "interactions-v58",
        "interactions-v60",
    ] {
        assert!(resolve_rules_source(&format!("import = \"{previous}\"\n"), "rules.toml").is_err());
    }
}

fn compact_package(domain: crate::procedural_world::Domain) -> Package {
    let template = read_package(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/explicit-grid-groups"),
    )
    .unwrap();
    let mut manifest = template.manifest.clone();
    manifest.generation_groups.clear();
    manifest.stair_pairs.clear();
    manifest.characters[0].anchor = "caves/1/start".into();
    manifest.procedural_worlds.clear();
    manifest.procedural_worlds.insert(
        "caves".into(),
        crate::procedural_world::WorldFamily {
            kind: crate::procedural_world::FamilyKind::GridChain,
            version: 1,
            base_region: 1,
            domain,
            grid: [2, 2],
            size: [26, 7, 2],
            recipe: "exploration".into(),
            gravity: None,
            zone: None,
        },
    );
    Package::from_parts_scripted(
        manifest,
        vec![],
        &template.rules_source,
        template.scripts.clone(),
    )
    .unwrap()
}

#[test]
fn rogue_magic_catalog_uses_explicit_capabilities_and_appends_finite_stock() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let package = read_package(&root).unwrap();
    let healing = &package.manifest.archetypes["rogue.healing"];
    assert!(healing.magical && healing.stackable);
    assert_eq!(healing.class, ItemClass::Potion);
    assert_eq!(healing.appearance_pool.as_deref(), Some("rogue.potions"));
    let dose = healing.consumable.as_ref().unwrap();
    assert_eq!(dose.discovery, ItemDiscovery::OnUse);
    assert_eq!(
        dose.effects,
        vec![
            Effect::HealPerHitDie {
                sides: 4,
                bonus: 0,
                overflow: HealingOverflow::ImproveEarliestHitDie
            },
            Effect::RemoveCondition {
                condition: "rogue.blindness".into()
            },
        ]
    );
    assert!(dose.compile(&package.rules().unwrap()).is_ok());
    for (id, condition, sides, bonus) in [
        ("rogue.confusion", "rogue.potion_confusion", 2, 18),
        ("rogue.blindness", "rogue.blindness", 85, 807),
    ] {
        let item = &package.manifest.archetypes[id];
        assert!(item.magical && item.stackable);
        assert_eq!(item.appearance_pool.as_deref(), Some("rogue.potions"));
        let dose = item.consumable.as_ref().unwrap();
        assert_eq!(dose.discovery, ItemDiscovery::OnUse);
        assert_eq!(
            dose.effects,
            vec![Effect::ApplyConditionForTurns {
                condition: condition.into(),
                duration: AmountExpr::Rolled {
                    dice: DiceExpr {
                        count: 1,
                        sides,
                        bonus
                    }
                },
            }]
        );
    }
    for (id, class) in [
        ("rogue.identify_potion", ItemClass::Potion),
        ("rogue.identify_scroll", ItemClass::Scroll),
        ("rogue.identify_wand", ItemClass::Wand),
    ] {
        let item = &package.manifest.archetypes[id];
        let readable = item.readable.as_ref().unwrap();
        assert_eq!(
            readable.identify,
            Some(if class == ItemClass::Wand {
                vec![ItemClass::Ring, ItemClass::Wand]
            } else {
                vec![class]
            })
        );
        assert!(readable.effects.is_empty());
        assert_eq!(readable.discovery, ItemDiscovery::OnUse);
    }
    let sleep = package.manifest.archetypes["rogue.sleep"]
        .readable
        .as_ref()
        .unwrap();
    assert_eq!(sleep.discovery, ItemDiscovery::OnUse);
    assert_eq!(
        sleep.effects,
        vec![Effect::ApplyConditionForTurns {
            condition: "rogue.sleep".into(),
            duration: AmountExpr::Rolled {
                dice: DiceExpr {
                    count: 1,
                    sides: 5,
                    bonus: 3
                }
            },
        }]
    );
    assert_eq!(
        package.manifest.archetypes["rogue.restore_strength"]
            .consumable
            .as_ref()
            .unwrap()
            .discovery,
        ItemDiscovery::SelfEffect
    );
    let activation =
        serde_json::to_value(package.manifest.floor_activation.as_ref().unwrap()).unwrap();
    let stages = activation["profiles"]["rogue.low_depth"]["stages"]
        .as_array()
        .unwrap();
    let ids: Vec<_> = stages
        .iter()
        .map(|stage| stage["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        &ids[..5],
        &[
            "rogue.eight_species_foes",
            "rogue.bandages",
            "rogue.rations",
            "rogue.restore_strength",
            "rogue.room_gold"
        ]
    );
    assert_eq!(ids[5], "rogue.magic_potions");
    assert_eq!(
        stages[5]["bands"][0]["choices"],
        serde_json::json!([
            {"archetype":"rogue.healing","weight":1},
            {"archetype":"rogue.confusion","weight":1},
            {"archetype":"rogue.blindness","weight":1},
            {"archetype":"rogue.poison","weight":1},
            {"archetype":"rogue.see_invisible","weight":1},
            {"archetype":"rogue.extra_healing","weight":1},
            {"archetype":"rogue.levitation","weight":1},
        ])
    );
    assert_eq!(ids[6], "rogue.magic_scrolls");
    assert_eq!(ids[7], "rogue.magic_devices");
    assert_eq!(stages.len(), 15);
    assert_eq!(
        &ids[8..],
        &[
            "rogue.equipment",
            "rogue.equipment_scrolls",
            "rogue.ammunition",
            "rogue.rings",
            "rogue.control_scrolls",
            "rogue.expanded_stock",
            "rogue.treasure_room",
        ]
    );
    for stage in &stages[5..13] {
        assert_eq!(stage["kind"], "items");
        assert_eq!(stage["count"], serde_json::json!([1, 1]));
    }
    let supplemental_choices: Vec<_> = [
        "haste",
        "gain_strength",
        "raise_level",
        "hallucination",
        "monster_detection",
        "magic_detection",
        "food_detection",
        "magic_mapping",
        "wand_nothing",
        "wand_light",
        "wand_drain_life",
        "wand_cancellation",
        "wand_magic_missile",
        "wand_polymorph",
        "fruit",
        "dart",
        "shuriken",
    ]
    .into_iter()
    .map(|name| {
        serde_json::json!({
            "archetype":format!("rogue.{name}"), "weight":1
        })
    })
    .collect();
    assert_eq!(
        stages[13],
        serde_json::json!({
            "id":"rogue.expanded_stock", "version":1, "kind":"items", "count":[0,1],
            "quantity":1, "bands":[{"depth":{"kind":"open","minimum":1},
                "choices":supplemental_choices}]
        })
    );
    let treasure = &stages[14];
    assert_eq!(treasure["version"], 1);
    assert_eq!(treasure["kind"], "room_cluster");
    assert_eq!(
        treasure["chance"],
        serde_json::json!({"numerator":1,"denominator":20})
    );
    assert_eq!(treasure["item_count"], serde_json::json!([2, 4]));
    assert_eq!(treasure["actor_count"], serde_json::json!([1, 3]));
    assert_eq!(treasure["quantity"], 1);
    assert_eq!(treasure["actor_depth_offset"], 1);
    assert_eq!(treasure["ai"], "rogue_hostile");
    assert_eq!(treasure["avoid_protagonist_sight"], false);
    assert_eq!(
        treasure["depth_growth"],
        serde_json::json!({"kind":"racial_after","depth":26})
    );
    assert_eq!(treasure["actor_bands"], stages[0]["bands"]);
    assert_eq!(
        treasure["item_bands"],
        serde_json::json!([
            {"depth":{"kind":"open","minimum":1}, "choices":[
                {"archetype":"rogue.gold","weight":1},
                {"archetype":"rogue.healing","weight":1},
                {"archetype":"rogue.fruit","weight":1},
                {"archetype":"rogue.dart","weight":1},
                {"archetype":"rogue.shuriken","weight":1},
            ]}
        ])
    );
    assert_eq!(
        activation["carried_item_pools"].as_object().unwrap().len(),
        2
    );
}

#[test]
fn rogue_devices_compile_lore_rays_without_grants_or_non_charge_costs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let package = read_package(&root).unwrap();
    let rules = package.rules().unwrap();
    for element in ["fire", "cold", "lightning", "invisibility", "haste", "slow"] {
        let utility = matches!(element, "invisibility" | "haste" | "slow");
        let id = format!("rogue.wand_{element}");
        let item = &package.manifest.archetypes[&id];
        assert!(item.magical && !item.stackable);
        assert_eq!(item.class, ItemClass::Wand);
        assert_eq!(item.appearance_pool.as_deref(), Some("rogue.wands"));
        let device = item.device.as_ref().unwrap();
        assert_eq!(
            device.initial_charges,
            ChargeRange {
                minimum: 3,
                maximum: 7
            }
        );
        assert_eq!(
            device.discovery,
            if utility {
                ItemDiscovery::Never
            } else {
                ItemDiscovery::OnUse
            }
        );
        let compiled = device.compile(&rules).unwrap();
        let ability = rules.ability(&compiled.ability).unwrap();
        assert!(
            ability.cost.is_none() && ability.cooldown.is_none() && ability.supply_costs.is_empty()
        );
        assert_eq!(
            ability.invocation,
            if utility {
                tor_simulation::abilities::InvocationKind::DirectionalContactRay
            } else {
                tor_simulation::abilities::InvocationKind::DirectionalRay
            }
        );
        let value = serde_json::to_value(ability).unwrap();
        assert_eq!(
            value["reach"],
            serde_json::json!({"kind":"sight","range":6})
        );
        let strike = &value["strikes"]["strikes"][0];
        assert_eq!(
            strike["check"]["skill"],
            serde_json::json!({"kind":"named","skill":"agility"})
        );
        assert_eq!(
            strike["check"]["against"]["skill"],
            serde_json::json!({"kind":"named","skill":"lore"})
        );
        if utility {
            let (condition, cancels) = match element {
                "invisibility" => ("rogue.invisible", None),
                "haste" => ("rogue.hasted", Some("rogue.slowed")),
                "slow" => ("rogue.slowed", Some("rogue.hasted")),
                _ => unreachable!("utility variants are selected explicitly above"),
            };
            assert_eq!(
                strike["on_failure"],
                serde_json::json!([{"kind":"apply_persistent_condition", "condition":condition, "cancels":cancels}])
            );
        } else {
            let damage = &strike["on_failure"][0]["damage"]["components"][0];
            assert_eq!(damage["descriptor"], element);
            assert_eq!(
                damage["amount"]["dice"],
                serde_json::json!({"count":6,"sides":6,"bonus":0})
            );
        }
        // The catalog authors an invocation reference, never a character or
        // talent purchase that permanently grants the device's ability.
        assert!(!rules
            .document()
            .constants
            .default_abilities
            .contains(&compiled.ability));
        assert!(!rules.document().talents.iter().any(|talent| {
                talent.modifiers.iter().any(|modifier| matches!(
                    &modifier.op,
                    tor_simulation::rules::ModifierOp::GrantAbility { ability } if ability == &compiled.ability
                ))
            }));
    }
}

#[test]
fn device_catalog_is_class_independent_but_rejects_stackable_or_mixed_capabilities() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let mut package = read_package(&root).unwrap();
    // Device capability is class-independent, but every concealed alternative
    // in one appearance group must retain the same physical class.
    let ids = [
        "rogue.wand_fire",
        "rogue.wand_cold",
        "rogue.wand_lightning",
        "rogue.wand_invisibility",
        "rogue.wand_haste",
        "rogue.wand_slow",
        "rogue.wand_teleport_away",
        "rogue.wand_teleport_to",
        "rogue.wand_nothing",
        "rogue.wand_light",
        "rogue.wand_drain_life",
        "rogue.wand_cancellation",
        "rogue.wand_magic_missile",
        "rogue.wand_polymorph",
    ];
    assert_eq!(ids.len(), 14);
    let pool = package.manifest.archetypes["rogue.wand_fire"]
        .appearance_pool
        .clone();
    assert!(pool.is_some());
    let actual: BTreeSet<_> = package
        .manifest
        .archetypes
        .iter()
        .filter(|(_, item)| item.appearance_pool == pool)
        .map(|(id, _)| id.as_str())
        .collect();
    assert_eq!(
        actual,
        ids.iter().copied().collect(),
        "class conversion must cover the exact complete appearance group"
    );
    for id in ids {
        let item = package.manifest.archetypes.get_mut(id).unwrap();
        // This owner isolates class-independent charged use, not the new
        // Wand-class physical equipment overlap; keep the former pure-device shape.
        item.forms.clear();
        item.class = ItemClass::Tool;
    }
    package.appearance_mapping(42).unwrap();
    let mut stackable = package.clone();
    stackable
        .manifest
        .archetypes
        .get_mut("rogue.wand_fire")
        .unwrap()
        .stackable = true;
    assert!(stackable.appearance_mapping(42).is_err());
    let mut mixed = package.clone();
    let device = mixed
        .manifest
        .archetypes
        .get_mut("rogue.wand_fire")
        .unwrap();
    device.readable = Some(ReadableSpec {
        control: None,
        descriptors: vec![],
        effects: vec![],
        identify: Some(vec![ItemClass::Wand]),
        discovery: ItemDiscovery::OnUse,
    });
    assert!(mixed.appearance_mapping(42).is_err());
}

#[test]
fn bundled_rogue_uses_compact_open_family_and_preserves_room_recipe() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios");
    let package = read_package(&root.join("rogue-exploration")).unwrap();
    let geometry = read_package(&root.join("tests/rogue-geometry")).unwrap();
    assert!(geometry.manifest.floor_activation.is_none());
    assert!(geometry
        .manifest
        .characters
        .iter()
        .all(|character| character.creature.is_none() && character.loadout.is_empty()));
    assert_eq!(
        package.manifest.procedural_worlds["dungeon"].gravity,
        Some([0, 0, -1])
    );
    assert_eq!(
        geometry.manifest.procedural_worlds["dungeon"].gravity,
        Some([0, 0, -1])
    );
    assert_eq!(
        serde_json::to_value(&package.manifest.procedural_worlds).unwrap(),
        serde_json::to_value(&geometry.manifest.procedural_worlds).unwrap()
    );
    let live_recipe = &package.manifest.generation_recipes["exploration"];
    assert_eq!(live_recipe.version, 5);
    assert_eq!(
        live_recipe
            .stages
            .iter()
            .map(|stage| stage.id.as_str())
            .collect::<Vec<_>>(),
        [
            "stone",
            "slots",
            "rooms",
            "connections",
            "corridors",
            "stairs",
            "room_omissions",
            "maze_rooms",
            "lighting",
            "secret_entrances",
            "hidden_traps"
        ]
    );
    assert_eq!(
        serde_json::to_value(&live_recipe.stages[6]).unwrap(),
        serde_json::json!({
            "id":"room_omissions", "version":1,
            "operation":{"kind":"room_omissions","config":{"count":[0,1]}}
        })
    );
    assert_eq!(
        serde_json::to_value(&live_recipe.stages[7]).unwrap(),
        serde_json::json!({
            "id":"maze_rooms", "version":1,
            "operation":{"kind":"maze_rooms","config":{"one_in":15}}
        })
    );
    // Preserve the historical discovery-isolation witness with a validated
    // control removing only the two new, independently pinned layout ops.
    // The live package intentionally does not promise seed-identical geometry.
    let mut discovery_manifest = package.manifest.clone();
    let recipe = discovery_manifest
        .generation_recipes
        .get_mut("exploration")
        .unwrap();
    recipe.stages.drain(6..8);
    recipe.version = 3;
    let discovery = Package::from_parts_scripted(
        discovery_manifest,
        vec![],
        &package.rules_source,
        package.scripts.clone(),
    )
    .unwrap();
    discovery.check().unwrap();
    let production_recipe = &discovery.manifest.generation_recipes["exploration"];
    let geometry_recipe = &geometry.manifest.generation_recipes["exploration"];
    assert_eq!(production_recipe.version, 3);
    assert_eq!(geometry_recipe.version, 2);
    assert_eq!(geometry_recipe.stages.len(), 7);
    assert_eq!(production_recipe.stages.len(), 9);
    assert_eq!(
        serde_json::to_value(&production_recipe.stages[..7]).unwrap(),
        serde_json::to_value(&geometry_recipe.stages).unwrap(),
        "discovery appends stages without changing any existing stream identity or operand"
    );
    assert_eq!(production_recipe.stages[7].id, "secret_entrances");
    assert_eq!(production_recipe.stages[8].id, "hidden_traps");
    assert!(matches!(
        production_recipe.stages[7].operation,
        crate::generation_recipe::Operation::SecretEntrances { .. }
    ));
    assert!(matches!(
        production_recipe.stages[8].operation,
        crate::generation_recipe::Operation::HiddenTraps { .. }
    ));
    let explicit = read_package(&root.join("tests/explicit-grid-groups")).unwrap();
    assert!(package.index.regions.is_empty());
    assert!(package.manifest.generation_groups.is_empty());
    assert!(package.manifest.stair_pairs.is_empty());
    assert_eq!(package.manifest.characters[0].anchor, "dungeon/1/up");
    assert_eq!(package.manifest.procedural_worlds.len(), 1);
    let family = &package.manifest.procedural_worlds["dungeon"];
    assert_eq!(family.grid, [3, 3]);
    assert_eq!(family.size, [26, 7, 2]);
    assert_eq!(
        family.domain,
        crate::procedural_world::Domain::Open {
            max_depth: 1_000_000
        }
    );
    assert_eq!(
        serde_json::to_value(geometry_recipe).unwrap(),
        serde_json::to_value(&explicit.manifest.generation_recipes["exploration"]).unwrap()
    );
    let mut entrances = 0;
    let mut live_entrances = 0;
    let mut changed_live_geometry = false;
    for seed in [0, 1, 42] {
        for depth in [26, 27, 101, 1_000_000] {
            let group = family.group("dungeon", seed, depth).unwrap();
            let identity = format!("dungeon/{depth}");
            let live = package.group_definitions(&identity, seed).unwrap();
            let mut discovered = discovery.group_definitions(&identity, seed).unwrap();
            assert_eq!(
                live.len(),
                discovered.len(),
                "omissions retain every grid slot"
            );
            changed_live_geometry |= live
                .iter()
                .any(|(id, region)| region.walls != discovered[id].walls);
            let baseline = geometry.group_definitions(&identity, seed).unwrap();
            for region in baseline.values() {
                assert_eq!(region.gravity, Some([0, 0, -1]));
            }
            assert!(baseline
                .values()
                .all(|region| region.doors.is_empty() && region.traps.is_empty()));
            entrances += discovered
                .values()
                .map(|region| region.doors.len())
                .sum::<usize>();
            live_entrances += live
                .values()
                .map(|region| region.doors.len())
                .sum::<usize>();
            for generated in [&live, &discovered] {
                let trap_count: usize = generated.values().map(|region| region.traps.len()).sum();
                assert!(
                    (1..=2).contains(&trap_count),
                    "live and discovery-control floors retain finite trap placement"
                );
                for region in generated.values() {
                    assert!(region
                        .doors
                        .iter()
                        .all(|door| !door.open && door.height == 2 && door.concealment.is_some()));
                    // New ceiling anchors are independent physical endpoints, not
                    // rerolled carving/stairs. Assert their clearance before removing
                    // only these deliberate additions from the geometry comparison.
                    let landings: Vec<_> = region
                        .anchors
                        .iter()
                        .filter(|(name, _)| name.starts_with("trapdoor_landing_"))
                        .collect();
                    assert_eq!(landings.len(), 2);
                    for (_, landing) in landings {
                        assert_eq!(landing[2], region.size[2] - 1);
                        assert!((0..region.size[2])
                            .all(|z| !region.walls.contains(&[landing[0], landing[1], z])));
                        assert!(!region.traps.iter().any(|trap| trap.at[..2] == landing[..2]));
                    }
                    assert_eq!(region.gravity, Some([0, 0, -1]));
                }
            }
            for region in discovered.values_mut() {
                region
                    .anchors
                    .retain(|name, _| !name.starts_with("trapdoor_landing_"));
                region.doors.clear();
                region.traps.clear();
            }
            assert_eq!(
                serde_json::to_value(discovered).unwrap(),
                serde_json::to_value(baseline).unwrap(),
                "population and discovery must not reroll geometry at seed {seed}, depth {depth}"
            );
            assert_eq!(group.members.len(), 9);
            assert!(group
                .members
                .iter()
                .flat_map(|region| family.connections("dungeon", seed, *region))
                .any(|exit| exit.direction == Direction::Down));
        }
    }
    assert!(
        changed_live_geometry,
        "the live seed matrix must exercise its enabled layout changes"
    );
    assert!(
        entrances > 0,
        "the discovery control seed matrix must exercise actual secret closures"
    );
    assert!(
        live_entrances > 0,
        "the live seed matrix must exercise actual secret closures"
    );
}

#[test]
fn rogue_spatial_restraint_uses_valid_protected_condition_lifecycle() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let package = read_package(&root).unwrap();
    let rules = package
        .rules()
        .expect("the actual authored rules are a valid baseline");
    let id = tor_simulation::content_ids::ConditionId::new("rogue.bear_trap").unwrap();
    let definition = rules.condition(&id).unwrap();
    assert!(matches!(
        definition.timing,
        tor_simulation::rules::ConditionTiming::UntilRemoved
    ));
    assert_eq!(definition.stacking, tor_simulation::rules::Stacking::Keep);
    assert_eq!(
        definition.preparation,
        tor_simulation::rules::ConditionPreparation::Preserve
    );
    assert!(!definition
        .restrictions
        .contains(&tor_simulation::rules::ConditionRestriction::VoluntaryActions));
    let mut invalid = rules.document().clone();
    invalid
        .conditions
        .iter_mut()
        .find(|condition| condition.id == id)
        .unwrap()
        .stacking = tor_simulation::rules::Stacking::Refresh;
    assert!(matches!(tor_simulation::rules::Ruleset::validate(invalid),
            Err(tor_simulation::rules::RulesetError::Bound { field }) if field == "persistent_condition_shape"));
}

#[test]
fn generated_trap_effects_require_declared_slots_and_sleep_conditions() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let template = read_package(&root).unwrap();
    Package::from_parts_scripted(
        template.manifest.clone(),
        vec![],
        &template.rules_source,
        template.scripts.clone(),
    )
    .unwrap()
    .check()
    .expect("valid production discovery fixture");
    for missing_slot in [true, false] {
        let mut manifest = template.manifest.clone();
        let recipe = manifest.generation_recipes.get_mut("exploration").unwrap();
        let config = recipe
            .stages
            .iter_mut()
            .find_map(|stage| match &mut stage.operation {
                crate::generation_recipe::Operation::HiddenTraps { config } => Some(config),
                _ => None,
            })
            .expect("production fixture must exercise configured hidden traps");
        if missing_slot {
            config.rust_slot = "missing_trap_slot".into();
        } else {
            config.sleep_condition = "missing_trap_sleep".into();
        }
        // Rebuild the authored input and its identity together: rejection
        // must reach effect validation, not a stale certificate mismatch.
        let package = Package::from_parts_scripted(
            manifest,
            vec![],
            &template.rules_source,
            template.scripts.clone(),
        )
        .unwrap();
        let error = package.check().unwrap_err();
        assert!(
            error.message.contains(
                "Trap effect refers to an invalid equipment slot, condition, or descriptor"
            ),
            "{error:?}"
        );
    }
}

#[test]
fn compact_initial_entries_are_safe_distinct_and_match_seeded_character_spawns() {
    let package = compact_package(crate::procedural_world::Domain::Open { max_depth: 100 });
    for seed in [0, 1, 2, 42, u64::MAX] {
        let endpoint = package.resolve_endpoint(seed, "caves/1/start").unwrap();
        let owner: u64 = endpoint.split_once('/').unwrap().0.parse().unwrap();
        let definitions = package.group_definitions("caves/1", seed).unwrap();
        let definition = &definitions[&owner];
        let at = definition.anchors["start"];
        assert!(!definition.walls.contains(&at));
        assert!(!definition.walls.contains(&[at[0], at[1], 1]));
        assert!(definition
            .anchors
            .iter()
            .filter(|(name, _)| name.as_str() == "up" || name.as_str() == "down")
            .all(|(_, stair)| stair != &at));
        let group = package.generation_group("caves/1", seed).unwrap();
        let chart = |region: u64, point: [i32; 3]| {
            let slot = group.members.iter().position(|id| *id == region).unwrap() as i32;
            (
                point[0] + slot % group.grid[0] as i32 * 26,
                point[1] + slot / group.grid[0] as i32 * 7,
            )
        };
        let mut open = BTreeSet::new();
        let mut targets = BTreeSet::new();
        for (id, definition) in &definitions {
            for x in 0..definition.size[0] {
                for y in 0..definition.size[1] {
                    if !definition.walls.contains(&[x, y, 0]) {
                        open.insert(chart(*id, [x, y, 0]));
                    }
                }
            }
            for (name, point) in &definition.anchors {
                if matches!(name.as_str(), "up" | "down") {
                    targets.insert(chart(*id, *point));
                }
            }
        }
        let mut pending = vec![chart(owner, at)];
        let mut seen = BTreeSet::new();
        while let Some(point) = pending.pop() {
            if !seen.insert(point) {
                continue;
            }
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let next = (point.0 + dx, point.1 + dy);
                if open.contains(&next) && !seen.contains(&next) {
                    pending.push(next);
                }
            }
        }
        assert!(
            targets.is_subset(&seen),
            "all stair endpoints reachable from initial entry"
        );
        let index = package.index(seed).unwrap();
        assert_eq!(
            index.spawns[&owner]
                .iter()
                .find(|(actor, _, _)| *actor == 1)
                .unwrap()
                .1,
            loc(owner, at)
        );
        let game = package.build(seed, false).unwrap();
        assert_eq!(
            game.observe(tor_simulation::ActorId(1)).unwrap().location,
            loc(owner, at)
        );
    }
}

#[test]
fn declared_deep_starts_resolve_generated_ports_without_intermediate_floors() {
    for depth in [26, 999_999] {
        let template = compact_package(crate::procedural_world::Domain::Open {
            max_depth: 1_000_000,
        });
        let mut manifest = template.manifest.clone();
        manifest.characters[0].anchor = format!("caves/{depth}/up");
        let package = Package::from_parts_scripted(
            manifest,
            vec![],
            &template.rules_source,
            template.scripts.clone(),
        )
        .unwrap();
        package.check().unwrap();
        for seed in [0, 42] {
            let index = package.index(seed).unwrap();
            let endpoint = package
                .resolve_endpoint(seed, &package.manifest.characters[0].anchor)
                .unwrap();
            let definitions = package
                .group_definitions(&format!("caves/{depth}"), seed)
                .unwrap();
            let expected = resolved_anchors(&definitions.values().collect::<Vec<_>>())[&endpoint];
            assert_eq!(index.homes[&1], expected);
            assert!(index.groups.is_empty());
            assert!(index.group_owners.is_empty());
            assert!(index.anchors.len() <= 32);
            let game = package.build(seed, false).unwrap();
            assert_eq!(
                game.observe(tor_simulation::ActorId(1)).unwrap().location,
                expected
            );
            assert_eq!(game.loaded_regions().count(), 4);
            assert!(game.loaded_regions().all(|id| {
                let (floor, _) = package.manifest.procedural_worlds["caves"]
                    .address(id.0)
                    .unwrap();
                floor == depth
            }));
        }
    }
}

#[test]
fn declared_starts_reject_invalid_ports_boundaries_and_out_of_domain_roots() {
    let template = compact_package(crate::procedural_world::Domain::Open {
        max_depth: 1_000_000,
    });
    let mut beyond = template.manifest.procedural_worlds["caves"].clone();
    beyond.domain = crate::procedural_world::Domain::Open {
        max_depth: 1_000_001,
    };
    let numeric_beyond = beyond.resolve_port("caves", 0, "1000001", "up").unwrap();
    for anchor in [
        "caves/0/up",
        "caves/1000001/up",
        "caves/26/start",
        "caves/last/up",
        "missing/26/up",
        "caves/26/west",
        "1/west",
        "01/start",
        "1/missing",
        &numeric_beyond,
    ] {
        let mut manifest = template.manifest.clone();
        manifest.characters[0].anchor = anchor.into();
        let package = Package::from_parts_scripted(
            manifest,
            vec![],
            &template.rules_source,
            template.scripts.clone(),
        )
        .unwrap();
        let error = package.check().unwrap_err();
        assert!(
            error.message.contains("character 1"),
            "{anchor}: {}",
            error.message
        );
        assert!(package.index(0).is_err(), "{anchor}");
    }
    let mut manifest = template.manifest.clone();
    manifest.procedural_worlds.get_mut("caves").unwrap().domain =
        crate::procedural_world::Domain::Finite { count: 26 };
    manifest.characters[0].anchor = "caves/last/down".into();
    let finite = Package::from_parts_scripted(
        manifest.clone(),
        vec![],
        &template.rules_source,
        template.scripts.clone(),
    )
    .unwrap();
    finite.check().unwrap();
    assert_eq!(finite.build(42, false).unwrap().loaded_regions().count(), 4);
    manifest.characters[0].anchor = "caves/27/up".into();
    let outside = Package::from_parts_scripted(
        manifest,
        vec![],
        &template.rules_source,
        template.scripts.clone(),
    )
    .unwrap();
    assert!(outside.check().is_err());
}

fn give_compact_character_owned_build(manifest: &mut Manifest) {
    let definitions: Manifest = toml::from_str(include_str!(
        "../../../scenarios/tests/interactions/scenario.toml"
    ))
    .unwrap();
    manifest.creatures = definitions.creatures;
    manifest.characters[0].creature = definitions.characters[0].creature.clone();
}

#[test]
fn declared_start_closures_are_included_in_preflight_validation_budgets() {
    let template = compact_package(crate::procedural_world::Domain::Open {
        max_depth: 1_000_000,
    });
    let mut manifest = template.manifest.clone();
    give_compact_character_owned_build(&mut manifest);
    manifest.ai_profiles.insert(
        "careful".into(),
        AiProfile {
            care: Some(true),
            equipment: Some(true),
            loot: Some(true),
            flee_percent: Some(19),
            ..Default::default()
        },
    );
    manifest.characters = (1..=16)
        .map(|id| {
            let mut character = manifest.characters[0].clone();
            character.id = id;
            character.anchor = format!("caves/{id}/up");
            character.unselected = "ai".into();
            character.ai = Some("careful".into());
            character
        })
        .collect();
    let directory = tempfile::tempdir().unwrap();
    write_package_scripted(
        directory.path(),
        &manifest,
        &[],
        &template.rules_source,
        &template.scripts,
    )
    .unwrap();
    let error = checked_package(directory.path()).unwrap_err();
    assert!(
        error.message.contains("sampled-cell budget"),
        "{}",
        error.message
    );
    assert!(!directory.path().join("validation.json").exists());
    assert!(!directory.path().join("index.json").exists());
}

#[test]
fn declared_spawned_roots_deduplicate_groups_and_omit_unselected_floors() {
    let template = compact_package(crate::procedural_world::Domain::Open {
        max_depth: 1_000_000,
    });
    for companion_depth in [26, 27] {
        let mut manifest = template.manifest.clone();
        give_compact_character_owned_build(&mut manifest);
        manifest.characters[0].anchor = "caves/26/up".into();
        manifest.ai_profiles.insert(
            "careful".into(),
            AiProfile {
                care: Some(true),
                equipment: Some(true),
                loot: Some(true),
                flee_percent: Some(19),
                ..Default::default()
            },
        );
        let mut companion = manifest.characters[0].clone();
        companion.id = 2;
        companion.anchor = format!("caves/{companion_depth}/down");
        companion.unselected = "ai".into();
        companion.ai = Some("careful".into());
        manifest.characters.push(companion);
        let mut omitted = manifest.characters[0].clone();
        omitted.id = 3;
        omitted.anchor = "caves/999999/up".into();
        manifest.characters.push(omitted);
        let package = Package::from_parts_scripted(
            manifest,
            vec![],
            &template.rules_source,
            template.scripts.clone(),
        )
        .unwrap();
        package.check().unwrap();
        let index = package.index(42).unwrap();
        assert_eq!(index.homes.keys().copied().collect::<Vec<_>>(), vec![1, 2]);
        let floors = BTreeSet::from([26, companion_depth]);
        assert_eq!(
            package
                .initial_procedural_groups(index.homes.values().map(|at| at.region.0))
                .len(),
            floors.len()
        );
        let family = &package.manifest.procedural_worlds["caves"];
        assert!(index.anchors.values().all(|at| {
            let (depth, _) = family.address(at.region.0).unwrap();
            depth == 1 || floors.contains(&depth)
        }));
        assert!(index.anchors.len() <= 1 + 18 * floors.len());
        let game = package.build(42, false).unwrap();
        assert_eq!(game.loaded_regions().count(), 4 * floors.len());
        assert_eq!(
            game.loaded_regions()
                .map(|id| family.address(id.0).unwrap().0)
                .collect::<BTreeSet<_>>(),
            floors
        );
        for actor in [1, 2] {
            assert_eq!(
                game.observe(tor_simulation::ActorId(actor))
                    .unwrap()
                    .location,
                index.homes[&actor]
            );
        }
        assert!(game.observe(tor_simulation::ActorId(3)).is_err());
    }
}

#[test]
fn compact_procedural_world_keeps_descriptor_and_queries_bounded_horizon() {
    let package = compact_package(crate::procedural_world::Domain::Open {
        max_depth: 1_000_000,
    });
    package.check().unwrap();
    assert!(package.index.regions.is_empty());
    let index = package.index(42).unwrap();
    assert!(index.group_owners.is_empty());
    assert!(index.groups.is_empty());
    assert!(index.stairs.is_empty());
    let catalog = crate::region_streaming::RegionCatalog::from_package_seed(&package, 42).unwrap();
    let plan = catalog
        .plan(&BTreeSet::from([RegionId(2_000_001)]), 2, &BTreeSet::new())
        .unwrap();
    assert!(plan.required.len() < 32);
    assert!(plan.expanded_regions < 16);
    let first = package
        .build_group(42, &index, RegionId(2_000_001))
        .unwrap();
    let second = package
        .build_group(42, &index, RegionId(2_000_001))
        .unwrap();
    assert_eq!(first.records, second.records);
    assert_eq!(first.records.len(), 4);
    assert!(first.files.is_empty());
}

#[test]
fn authored_portals_to_procedural_families_require_named_stair_pairs() {
    let template = compact_package(crate::procedural_world::Domain::Finite { count: 2 });
    for target in ["caves/2/up", "1/start"] {
        let mut room = (*template.structural_definition(0, 1).unwrap()).clone();
        room.id = 1000;
        room.generate = None;
        room.portals = vec![Portal {
            rotation: None,
            kind: None,
            at: [2, 2, 0],
            direction: "down".into(),
            to: target.into(),
            turns: 0,
            width: 1,
            height: 1,
        }];
        let package = Package::from_parts_scripted(
            template.manifest.clone(),
            vec![room],
            &template.rules_source,
            template.scripts.clone(),
        )
        .unwrap();
        let error = package.check().unwrap_err();
        assert!(
            error.message.contains("use stair_pairs"),
            "{}",
            error.message
        );
    }
}

#[test]
fn authored_hub_and_chamber_compose_with_seeded_procedural_ports() {
    let template = compact_package(crate::procedural_world::Domain::Finite { count: 1 });
    let mut manifest = template.manifest.clone();
    manifest.characters[0].anchor = "1000/stair".into();
    manifest.stair_pairs.insert(
        "entry".into(),
        StairPair {
            upper: "1000/stair".into(),
            lower: "caves/1/up".into(),
        },
    );
    manifest.stair_pairs.insert(
        "exit".into(),
        StairPair {
            upper: "caves/last/down".into(),
            lower: "1001/stair".into(),
        },
    );
    let mut room = (*template.structural_definition(0, 1).unwrap()).clone();
    room.id = 1000;
    room.name = "Hub".into();
    room.size = [5, 5, 2];
    room.generate = None;
    room.portals.clear();
    room.anchors = BTreeMap::from([("stair".into(), [2, 2, 0])]);
    let mut chamber = room.clone();
    chamber.id = 1001;
    chamber.name = "Chamber".into();
    let package = Package::from_parts_scripted(
        manifest,
        vec![room, chamber],
        &template.rules_source,
        template.scripts.clone(),
    )
    .unwrap();
    for seed in [0, 1, 42] {
        let catalog =
            crate::region_streaming::RegionCatalog::from_package_seed(&package, seed).unwrap();
        let entry = catalog.anchor_region("caves/1/up").unwrap();
        assert!(catalog
            .region(RegionId(1000))
            .unwrap()
            .outgoing
            .contains(&entry));
        let mut game = package.build(seed, false).unwrap();
        let actor = tor_simulation::ActorId(1);
        game.act(actor, tor_simulation::Action::Move(Direction::Down))
            .unwrap();
        assert_eq!(game.observe(actor).unwrap().location.region, entry);
        let defs = package.group_definitions("caves/1", seed).unwrap();
        let down = package.resolve_endpoint(seed, "caves/last/down").unwrap();
        let anchors = resolved_anchors(&defs.values().collect::<Vec<_>>());
        game.teleport(actor, anchors[&down]).unwrap();
        game.act(actor, tor_simulation::Action::Move(Direction::Down))
            .unwrap();
        assert_eq!(game.observe(actor).unwrap().location, loc(1001, [2, 2, 0]));
        game.act(actor, tor_simulation::Action::Move(Direction::Up))
            .unwrap();
        assert_eq!(game.observe(actor).unwrap().location, anchors[&down]);
    }
}

#[test]
fn open_world_cap_keeps_structural_continuation_and_bounds_loading() {
    let package = compact_package(crate::procedural_world::Domain::Open { max_depth: 2 });
    let index = package.index(42).unwrap();
    assert!(package.build_group(42, &index, RegionId(5)).is_ok());
    assert!(package.build_group(42, &index, RegionId(9)).is_err());
    let catalog = crate::region_streaming::RegionCatalog::from_package_seed(&package, 42).unwrap();
    for radius in [0, 1, 2, 10] {
        let plan = catalog
            .plan(&BTreeSet::from([RegionId(5)]), radius, &BTreeSet::new())
            .unwrap();
        assert!(plan.required.iter().all(|region| region.0 <= 8));
    }
    let outgoing: BTreeSet<_> = (5..=8)
        .flat_map(|id| catalog.region(RegionId(id)).unwrap().outgoing.clone())
        .collect();
    assert!(outgoing.iter().any(|id| id.0 >= 9));
    assert!(catalog.region(RegionId(9)).is_some());
}

#[test]
fn streaming_engine_respects_open_cap_without_prefetch_failure() {
    for max_depth in [1, 2] {
        for radius in [0, 1, 2] {
            let package = compact_package(crate::procedural_world::Domain::Open { max_depth });
            let scenario = Scenario {
                seed: 42,
                actors: vec![],
                regions: 0,
                workload_version: None,
                package: Some(Arc::new(package)),
                streaming: Some(crate::regions::Streaming {
                    active_radius: radius,
                    load_radius: radius,
                }),
            };
            let engine = crate::Engine::memory(scenario).unwrap();
            assert!(engine.region_counts().unwrap().active <= (max_depth * 4) as usize);
            assert!(engine.observation(tor_protocol::ActorId(1)).is_ok());
        }
    }
}

#[test]
fn procedural_records_are_independent_of_depth_and_member_visit_order() {
    let package = compact_package(crate::procedural_world::Domain::SeededFinite { min: 4, max: 7 });
    for seed in [0, 1, 42] {
        let index = package.index(seed).unwrap();
        let count = package.manifest.procedural_worlds["caves"].depth_limit("caves", seed);
        let forward: Vec<_> = (1..=count).collect();
        let reverse: Vec<_> = forward.iter().copied().rev().collect();
        let mut rotated = forward.clone();
        rotated.rotate_left(2);
        let build = |order: &[u32]| -> BTreeMap<RegionId, tor_simulation::RegionRecord> {
            order
                .iter()
                .flat_map(|depth| {
                    let owner = 1 + u64::from(*depth - 1) * 4;
                    let mut records = package
                        .build_group(seed, &index, RegionId(owner))
                        .unwrap()
                        .records;
                    if depth % 2 == 0 {
                        records.reverse();
                    }
                    records
                        .into_iter()
                        .map(|(declaration, record)| (declaration.region.id, record))
                })
                .collect()
        };
        assert_eq!(build(&forward), build(&reverse));
        assert_eq!(build(&forward), build(&rotated));
    }
}

#[test]
fn seeded_finite_boundaries_and_multiple_families_remain_independent() {
    let mut package =
        compact_package(crate::procedural_world::Domain::SeededFinite { min: 2, max: 7 });
    let mut second = package.manifest.procedural_worlds["caves"].clone();
    second.base_region = 1000;
    package
        .manifest
        .procedural_worlds
        .insert("tower".into(), second);
    let package = Package::from_parts_scripted(
        package.manifest.clone(),
        vec![],
        &package.rules_source,
        package.scripts.clone(),
    )
    .unwrap();
    package.check().unwrap();
    for seed in [0, 1, 42] {
        for (id, family) in &package.manifest.procedural_worlds {
            let limit = family.depth_limit(id, seed);
            let first: Vec<_> = family
                .group(id, seed, 1)
                .unwrap()
                .members
                .iter()
                .flat_map(|region| family.connections(id, seed, *region))
                .collect();
            assert!(first.iter().all(|exit| exit.direction == Direction::Down));
            let last = family.group(id, seed, limit).unwrap();
            let exits: Vec<_> = last
                .members
                .iter()
                .flat_map(|region| family.connections(id, seed, *region))
                .collect();
            assert_eq!(exits.len(), 1);
            assert_eq!(exits[0].direction, Direction::Up);
            let down = family.resolve_port(id, seed, "last", "down").unwrap();
            assert!(last
                .members
                .iter()
                .any(|region| down.starts_with(&format!("{region}/"))));
            assert!(family.group(id, seed, limit + 1).is_err());
        }
    }
}

#[test]
fn attempted_traversal_beyond_open_budget_keeps_state_unchanged() {
    let package = compact_package(crate::procedural_world::Domain::Open { max_depth: 1 });
    let mut game = package.build(42, false).unwrap();
    let defs = package.group_definitions("caves/1", 42).unwrap();
    let anchors = resolved_anchors(&defs.values().collect::<Vec<_>>());
    let down = package.resolve_endpoint(42, "caves/1/down").unwrap();
    let actor = tor_simulation::ActorId(1);
    game.teleport(actor, anchors[&down]).unwrap();
    let before = game.clone();
    assert!(game
        .act(actor, tor_simulation::Action::Move(Direction::Down))
        .is_err());
    assert_eq!(game, before);
}

#[test]
fn explicit_connections_cannot_replace_a_procedural_direction() {
    let mut package = compact_package(crate::procedural_world::Domain::Finite { count: 3 });
    package.manifest.stair_pairs.insert(
        "conflict".into(),
        StairPair {
            upper: "caves/1/down".into(),
            lower: "caves/3/up".into(),
        },
    );
    assert!(package.index(42).is_err());
}

#[test]
fn validation_preflights_repeated_authored_volume_before_construction() {
    let template = compact_package(crate::procedural_world::Domain::Finite { count: 1 });
    let mut manifest = template.manifest.clone();
    manifest.characters = (1..=64)
        .map(|id| {
            let mut c = manifest.characters[0].clone();
            c.id = id;
            c
        })
        .collect();
    let mut room = (*template.structural_definition(0, 1).unwrap()).clone();
    room.generate = None;
    room.portals.clear();
    room.anchors.clear();
    room.size = [32, 32, 8];
    let definitions: Vec<_> = (1000..1032)
        .map(|id| {
            let mut def = room.clone();
            def.id = id;
            def
        })
        .collect();
    let temp = tempfile::tempdir().unwrap();
    write_package_scripted(
        temp.path(),
        &manifest,
        &definitions,
        &template.rules_source,
        &template.scripts,
    )
    .unwrap();
    let error = checked_package(temp.path()).unwrap_err();
    assert!(
        error.message.contains("aggregate cell-work budget"),
        "{}",
        error.message
    );
    assert!(!temp.path().join("validation.json").exists());
}

#[test]
fn compact_validation_still_checks_authored_geometry() {
    let template = compact_package(crate::procedural_world::Domain::Finite { count: 3 });
    let mut authored = (*template.structural_definition(0, 1).unwrap()).clone();
    authored.id = 1000;
    authored.generate = None;
    authored.portals.clear();
    authored.anchors.clear();
    authored.walls = vec![[i32::MAX, 0, 0]];
    let temp = tempfile::tempdir().unwrap();
    write_package_scripted(
        temp.path(),
        &template.manifest,
        &[authored],
        &template.rules_source,
        &template.scripts,
    )
    .unwrap();
    assert!(checked_package(temp.path()).is_err());
    assert!(!temp.path().join("validation.json").exists());
}

#[test]
fn compact_world_starts_streaming_and_persists_without_source_files() {
    let package =
        compact_package(crate::procedural_world::Domain::SeededFinite { min: 50, max: 200 });
    let scenario = Scenario {
        seed: 42,
        actors: vec![],
        regions: 0,
        workload_version: None,
        package: Some(Arc::new(package)),
        streaming: Some(crate::regions::Streaming {
            active_radius: 0,
            load_radius: 0,
        }),
    };
    let temp = tempfile::tempdir().unwrap();
    let save = temp.path().join("compact.db");
    let engine = crate::Engine::open(&save, scenario).unwrap();
    let counts = engine.region_counts().unwrap();
    assert!(counts.active <= 4);
    assert!(counts.active + counts.frozen <= 4);
    drop(engine);
    let restored = crate::Engine::open(&save, Scenario::two_room(0)).unwrap();
    assert_eq!(restored.region_counts().unwrap(), counts);
}

fn stairs_package() -> Package {
    read_package(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/paired-stairs"))
        .unwrap()
}

#[test]
fn paired_stairs_resolve_identically_in_either_build_order() {
    let package = stairs_package();
    for seed in [0, 1, 42, u64::MAX] {
        let forward = package.index(seed).unwrap();
        let (upper, files) = package.build_region(seed, &forward, 1).unwrap();
        assert_eq!(files, BTreeSet::from([1, 2]));
        let (lower, _) = package.build_region(seed, &forward, 2).unwrap();
        let reverse = package.index(seed).unwrap();
        assert_eq!(package.build_region(seed, &reverse, 2).unwrap().0, lower);
        assert_eq!(package.build_region(seed, &reverse, 1).unwrap().0, upper);
    }
}

#[test]
fn paired_stairs_allow_same_region_and_both_directions_on_one_cell() {
    let package = stairs_package();
    let mut game = package.build(42, false).unwrap();
    let actor = tor_simulation::ActorId(1);
    game.teleport(actor, loc(3, [1, 1, 0])).unwrap();
    let before = game.observe(actor).unwrap();
    assert!(before.exits.iter().any(|e| e.direction == Direction::Up));
    assert!(before.exits.iter().any(|e| e.direction == Direction::Down));
    game.act(actor, tor_simulation::Action::Move(Direction::Down))
        .unwrap();
    assert_eq!(game.observe(actor).unwrap().location, loc(3, [3, 3, 0]));
    game.act(actor, tor_simulation::Action::Move(Direction::Up))
        .unwrap();
    assert_eq!(game.observe(actor).unwrap().location, before.location);
    assert_eq!(game.tick(), 200);
}

#[test]
fn paired_stairs_preserve_rotated_facing_and_existing_momentum() {
    let mut game = stairs_package().build(42, false).unwrap();
    let actor = tor_simulation::ActorId(1);
    // Enter the stair from an existing rotated link. The pair must keep
    // this facing rather than reset it to the destination region's axes.
    game.connect(
        Passage {
            from: loc(1, [1, 1, 0]),
            direction: Direction::Up,
            to: loc(3, [1, 1, 0]),
        },
        1,
    )
    .unwrap();
    game.act(actor, tor_simulation::Action::Move(Direction::Up))
        .unwrap();
    let frame = |game: &Game| {
        let observation = game.observe(actor).unwrap();
        observation
            .visible_cells
            .iter()
            .find(|cell| cell.location == observation.location)
            .unwrap()
            .frame
    };
    assert_eq!(frame(&game), 1);
    game.set_actor_velocity(actor, [1, 2, 0]).unwrap();
    game.act(actor, tor_simulation::Action::Move(Direction::Down))
        .unwrap();
    assert_eq!(game.observe(actor).unwrap().location, loc(3, [3, 3, 0]));
    assert_eq!(frame(&game), 1);
    assert_eq!(game.actor_motion(actor).unwrap().velocity, [1, 2, 0]);
    game.act(actor, tor_simulation::Action::Move(Direction::East))
        .unwrap();
    assert_eq!(game.observe(actor).unwrap().location, loc(3, [3, 4, 0]));
}

#[test]
fn paired_stair_blocked_arrival_checks_entire_body_without_consuming_time() {
    let mut game = stairs_package().build(42, false).unwrap();
    let actor = tor_simulation::ActorId(1);
    game.teleport(actor, loc(3, [1, 1, 0])).unwrap();
    game.set_body(
        actor,
        tor_simulation::BodySpec {
            cells: vec![[0, 0, 0], [1, 0, 0]],
            eye: [0; 3],
            mass: 80,
        },
    )
    .unwrap();
    game.set_wall(loc(3, [4, 3, 0]), true).unwrap();
    let before = game.clone();
    assert!(game
        .act(actor, tor_simulation::Action::Move(Direction::Down))
        .is_err());
    assert_eq!(game, before);
    game.set_wall(loc(3, [4, 3, 0]), false).unwrap();
    game.act(actor, tor_simulation::Action::Move(Direction::Down))
        .unwrap();
    assert_eq!(game.observe(actor).unwrap().location, loc(3, [3, 3, 0]));
}

#[test]
fn stair_pairs_reject_conflicting_exits_at_either_endpoint() {
    let template = stairs_package();
    for (upper, lower) in [("1/stair", "3/return"), ("3/return", "2/up")] {
        let mut manifest = template.manifest.clone();
        manifest.stair_pairs.insert(
            "conflict".into(),
            StairPair {
                upper: upper.into(),
                lower: lower.into(),
            },
        );
        let temp = tempfile::tempdir().unwrap();
        let definitions = template.region_defs().unwrap();
        write_package_scripted(
            temp.path(),
            &manifest,
            &definitions,
            &template.rules_source,
            &template.scripts,
        )
        .unwrap();
        let package = read_package(temp.path()).unwrap();
        assert!(package.build(42, false).is_err());
    }
}

#[test]
fn paired_stair_chooses_among_nearest_open_landings() {
    let actor = tor_simulation::ActorId(1);
    let anchor = loc(3, [3, 3, 0]);
    let nearest = [loc(3, [2, 3, 0]), loc(3, [3, 4, 0])];
    let mut selected = std::collections::BTreeSet::new();
    for seed in 0..16 {
        let mut game = stairs_package().build(seed, false).unwrap();
        game.teleport(actor, loc(3, [1, 1, 0])).unwrap();
        let blocker = game
            .spawn_actor(anchor, NonZeroU64::new(100).unwrap())
            .unwrap();
        for wall in [[3, 2, 0], [4, 3, 0]] {
            game.set_wall(loc(3, wall), true).unwrap();
        }
        let mut replay = game.clone();
        let action = tor_simulation::Action::Move(Direction::Down);
        let outcome = game.act(actor, action.clone()).unwrap();
        assert_eq!(replay.act(actor, action).unwrap(), outcome);
        assert_eq!(game, replay);
        let landed = game.observe(actor).unwrap().location;
        assert!(nearest.contains(&landed), "{landed:?}");
        assert_eq!(
            outcome.kind,
            tor_simulation::OutcomeKind::Moved {
                from: loc(3, [1, 1, 0]),
                to: landed,
            },
        );
        assert_eq!(game.observe(blocker).unwrap().location, anchor);
        selected.insert(landed);
    }
    assert_eq!(selected.len(), nearest.len());
}

#[test]
fn paired_stair_with_no_open_landing_rejects_atomically() {
    let mut game = stairs_package().build(42, false).unwrap();
    let actor = tor_simulation::ActorId(1);
    game.teleport(actor, loc(3, [1, 1, 0])).unwrap();
    game.spawn_actor(loc(3, [3, 3, 0]), NonZeroU64::new(100).unwrap())
        .unwrap();
    for wall in [[2, 3, 0], [3, 2, 0], [4, 3, 0], [3, 4, 0]] {
        game.set_wall(loc(3, wall), true).unwrap();
    }
    let before = game.clone();
    assert!(game
        .act(actor, tor_simulation::Action::Move(Direction::Down))
        .is_err());
    assert_eq!(game, before);
}

#[test]
fn stair_pairs_reject_missing_endpoints_with_source_coordinates() {
    let mut package = stairs_package();
    package.manifest.stair_pairs.get_mut("first").unwrap().lower = "2/missing".into();
    let error = package.index(42).unwrap_err();
    assert!(error.message.contains("2/missing"), "{error}");
    // Changed source must not invent coordinates for the replacement name.
    assert!(error.message.contains("scenario.toml"), "{error}");
    assert!(error.message.contains("stair"), "{error}");
}

#[test]
fn stair_pair_horizon_uses_generated_names_without_coordinates() {
    let package = stairs_package();
    let catalog = crate::region_streaming::RegionCatalog::from_package(&package).unwrap();
    assert_eq!(catalog.anchor_region("2/up").unwrap(), RegionId(2));
    assert!(catalog.resolve_anchor("2/up").is_err());
    let ids = |values: &[u64]| values.iter().copied().map(RegionId).collect();
    assert_eq!(
        catalog.plan(&ids(&[1]), 1, &ids(&[])).unwrap().required,
        ids(&[1, 2])
    );
    assert_eq!(
        catalog.plan(&ids(&[3]), 1, &ids(&[])).unwrap().required,
        ids(&[2, 3])
    );
}
#[test]
fn shared_item_identity_requires_one_projectile_profile_in_source_and_restore() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let template = read_package(&root).unwrap();
    let projectile = serde_json::json!({
        "family":"fixture.bow", "recovery":"recoverable",
        "damage":{"components":[{"category":"keen","descriptor":null,"amount":{"type":"fixed","value":3}}],"primary":{"category":"keen","descriptor":null}}
    });
    let archetype = serde_json::json!({
        "name":"shared arrow", "identity":"item.weapon.shared-arrow", "class":"weapon", "stackable":true,
        "projectile":projectile
    });
    let mut manifest = template.manifest.clone();
    for key in ["fixture.arrow_a", "fixture.arrow_b"] {
        manifest.archetypes.insert(
            key.into(),
            serde_json::from_value(archetype.clone()).unwrap(),
        );
    }
    let make = |manifest| {
        Package::from_parts_scripted(
            manifest,
            template.region_defs().unwrap(),
            &template.rules_source,
            template.scripts.clone(),
        )
        .unwrap()
    };
    let valid = make(manifest.clone());
    valid.appearance_mapping(42).unwrap();
    valid.check_identity().unwrap();
    let encode = crate::storage::schema::scenario::encode_content_package;
    let decode = crate::storage::schema::scenario::decode_content_package;
    let restored = decode(&encode(&valid).unwrap()).unwrap();
    assert!(restored.directory.is_none());
    restored.check_identity().unwrap();
    for changed in ["family", "damage", "recovery", "presence"] {
        let mut manifest = manifest.clone();
        let item = manifest.archetypes.get_mut("fixture.arrow_b").unwrap();
        let mut profile = projectile.clone();
        match changed {
            "family" => profile["family"] = serde_json::json!("fixture.sling"),
            "damage" => {
                profile["damage"]["components"][0]["amount"]["value"] = serde_json::json!(4)
            }
            "recovery" => profile["recovery"] = serde_json::json!("consumed_on_hit"),
            "presence" => {}
            _ => unreachable!(),
        }
        item.projectile = (changed != "presence").then(|| serde_json::from_value(profile).unwrap());
        // Rebuild the honest hash/certificate around the mismatch. Rejection
        // must come from the shared semantic invariant, not stale hashes.
        let invalid = make(manifest);
        for failure in [
            invalid.appearance_mapping(42).unwrap_err(),
            invalid.check_identity().unwrap_err(),
            decode(&encode(&invalid).unwrap()).unwrap_err(),
        ] {
            assert!(
                failure.message.contains("One identity"),
                "{changed}: {failure}"
            );
            assert!(
                failure.message.contains("projectile profile"),
                "{changed}: {failure}"
            );
        }
    }
}

#[test]
fn appearance_pools_cannot_encode_hidden_identity_through_physical_class() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/items");
    let mut package = super::read_package(&path).unwrap();
    package.manifest.archetypes.get_mut("poison").unwrap().class = super::ItemClass::Weapon;
    assert!(package.appearance_mapping(42).is_err());
    package.manifest.archetypes.get_mut("poison").unwrap().class = super::ItemClass::Potion;
    assert!(package.appearance_mapping(42).is_ok());
}

#[test]
fn readable_capability_is_class_independent_and_concealed_affordances_must_match() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/items");
    let mut package = read_package(&path).unwrap();
    let readable = ReadableSpec {
        control: None,
        descriptors: vec![],
        effects: vec![],
        identify: Some(vec![ItemClass::Potion]),
        discovery: ItemDiscovery::OnUse,
    };
    package
        .manifest
        .archetypes
        .get_mut("healing")
        .unwrap()
        .readable = Some(readable.clone());
    // Both identities look alike, so one must not expose a unique Read action.
    assert!(package.appearance_mapping(42).is_err());
    package
        .manifest
        .archetypes
        .get_mut("poison")
        .unwrap()
        .readable = Some(readable);
    assert!(package.appearance_mapping(42).is_ok());
    // Potion is a physical label here; the explicit capability grants Read.
    package.manifest.archetypes.get_mut("poison").unwrap().class = ItemClass::Tool;
    package
        .manifest
        .archetypes
        .get_mut("healing")
        .unwrap()
        .class = ItemClass::Tool;
    assert!(package.appearance_mapping(42).is_ok());
    package
        .manifest
        .archetypes
        .get_mut("healing")
        .unwrap()
        .readable
        .as_mut()
        .unwrap()
        .identify = None;
    assert!(package.appearance_mapping(42).is_err());
}

#[test]
fn appearance_pools_share_affordances_while_effects_remain_distinct() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/items");
    let mut package = read_package(&path).unwrap();
    package
        .manifest
        .archetypes
        .get_mut("healing")
        .unwrap()
        .consumable = Some(ConsumableSpec {
        descriptors: vec![],
        discovery: ItemDiscovery::SelfEffect,
        effects: vec![Effect::Heal {
            amount: AmountExpr::Fixed { amount: 5 },
        }],
    });
    assert!(package.appearance_mapping(42).is_err());
    package
        .manifest
        .archetypes
        .get_mut("poison")
        .unwrap()
        .consumable = Some(ConsumableSpec {
        descriptors: vec![],
        discovery: ItemDiscovery::SelfEffect,
        effects: vec![Effect::Damage {
            damage: DamageExpr::Components {
                components: vec![DamageComponentExpr {
                    category: DamageType::Vital,
                    descriptor: None,
                    amount: AmountExpr::Fixed { amount: 8 },
                }],
            },
        }],
    });
    assert!(package.appearance_mapping(42).is_ok());
}
use super::*;

#[test]
fn creature_diagnostic_source_is_acquired_only_for_catalog_failure() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/generated-filler");
    let package = read_package(&root).unwrap();
    let mut manifest = package.manifest.clone();
    PreparedDefinitions::new_with_source(&manifest, 1, package.rules().unwrap(), || {
        panic!("valid build must not acquire source")
    })
    .unwrap();
    manifest.characters[0].creature.as_mut().unwrap().faction = "missing".into();
    let mut invalid = manifest.clone();
    invalid.characters[0]
        .creature
        .as_mut()
        .unwrap()
        .hit_dice
        .clear();
    let failure =
        PreparedDefinitions::new_with_source(&invalid, 1, package.rules().unwrap(), || {
            panic!("choice failure precedes source acquisition")
        })
        .unwrap_err();
    assert!(failure.message.contains("require 1 to 256 hit dice"));
    let reads = std::cell::Cell::new(0);
    let failure =
        PreparedDefinitions::new_with_source(&manifest, 1, package.rules().unwrap(), || {
            reads.set(reads.get() + 1);
            None
        })
        .unwrap_err();
    assert_eq!(reads.get(), 1);
    assert!(failure.message.contains("scenario.toml: character 1"));
    assert!(failure.message.contains("Unknown creature faction"));
    manifest.factions.clear();
    PreparedDefinitions::new_with_source(&manifest, 1, package.rules().unwrap(), || {
        panic!("empty faction catalog permits valid builds")
    })
    .unwrap();
}

#[test]
fn stored_scenario_encoding_borrows_definitions_without_region_acquisition() {
    #[derive(Serialize)]
    struct StoredScenario<'a>(#[serde(with = "crate::storage::schema::scenario")] &'a Scenario);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/first-dungeon");
    let scenario = load(&root, 42, None, false).unwrap();
    let package = scenario.package.as_ref().unwrap();
    let reads = package.sources.files_read();
    ARCHETYPE_DEFINITION_COPIES.with(|copies| copies.set(0));
    let saved = serde_json::to_value(StoredScenario(&scenario)).unwrap();
    assert_eq!(ARCHETYPE_DEFINITION_COPIES.with(|copies| copies.get()), 0);
    assert_eq!(package.sources.files_read(), reads);
    let metadata = saved["package"].as_object().unwrap();
    assert_eq!(
        metadata.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "certificate",
            "directory",
            "manifest",
            "rules_document",
            "scripts",
            "selected",
            "validated"
        ]
    );
}

#[test]
fn missing_identity_name_does_not_blame_a_valid_appearance_pool_reference() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/items");
    let mut package = read_package(&root).unwrap();
    package.manifest.archetypes.get_mut("healing").unwrap().name = None;
    let error = package.appearance_mapping(0).unwrap_err();
    assert_eq!(
        error.message,
        "scenario.toml: archetype \"healing\": Missing identity name for appearance pool"
    );
}
#[test]
fn generated_regions_declare_authored_character_identities_before_lazy_start() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/generated-filler");
    let template = read_package(&root).unwrap();
    let mut manifest = template.manifest.clone();
    manifest
        .characters
        .iter_mut()
        .find(|c| c.id == 1)
        .unwrap()
        .anchor = "2/west".into();
    let package = Package::from_parts_scripted(
        manifest,
        template.region_defs().unwrap(),
        &template.rules_source,
        template.scripts.clone(),
    )
    .unwrap();
    let index = package.index(42).unwrap();
    let declared = package.unbuilt_region(&index, 2).unwrap();
    assert!(declared
        .identities
        .actors
        .contains(&tor_simulation::ActorId(1)));
    package.start(42).unwrap();
}

#[test]
fn streamed_save_uses_pinned_rules_when_general_import_changes() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/streaming-corridor");
    let mut scenario = load(&root, 42, None, true).unwrap();
    scenario.streaming = Some(crate::Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    let package = Arc::make_mut(scenario.package.as_mut().unwrap());
    // Simulate a saved prior general-rules revision, while authored imports,
    // manifest and lazy region sources still match the directory exactly.
    let mut prior = package.rules().unwrap().document().clone();
    prior.constants.base_defense += 1;
    package.rules_document =
        crate::storage::schema::scenario::encode_rules_document(&prior).unwrap();
    package.certificate.ruleset_hash = digest(package.rules_document.as_bytes());
    package
        .certificate
        .ruleset_dependencies
        .insert(RULESET.into(), source_digest("prior shared rules revision"));
    package.certificate.model_hash = scripted_model_hash(
        model_hash(
            &package.manifest,
            &package.index,
            &package.certificate.ruleset_hash,
            &package.certificate.ruleset_dependencies,
        )
        .unwrap(),
        &scripts::digests(&package.scripts),
    )
    .unwrap();
    package.certificate.content_hash = digest(
        &serde_json::to_vec(&(
            &package.certificate.files,
            &package.certificate.ruleset_dependencies,
        ))
        .unwrap(),
    );
    let directory = tempfile::tempdir().unwrap();
    artifact::publish(directory.path(), package).unwrap();
    package.directory = Some(directory.path().display().to_string());
    let save = directory.path().join("streamed.db");
    let engine = crate::Engine::open(&save, scenario).unwrap();
    assert!(engine.region_counts().unwrap().unbuilt > 0);
    engine.flush().unwrap();
    drop(engine);
    let mut restored = crate::Engine::open(&save, Scenario::two_room(0)).unwrap();
    // Continue far enough to demand a source that had never been copied.
    for step in 0..76 {
        let actor = tor_protocol::ActorId(1);
        let revision = restored.causal_boundary(actor).unwrap();
        let branch = restored.branch().clone();
        restored
            .command(
                "player",
                "test",
                actor,
                &format!("pinned-{step}"),
                &branch,
                crate::journal::Command::Act {
                    expected_boundary: revision,
                    action: crate::journal::Action::Move {
                        direction: crate::journal::Direction::East,
                    },
                },
            )
            .unwrap();
    }
    assert!(restored.region_counts().unwrap().detached > 0);
}

#[test]
fn shared_rules_import_accepts_validated_scenario_override() {
    let source = r#"import = "interactions-v73"
[[overrides]]
op = "set"
path = ["constants", "base_defense"]
value = 13
"#;
    let rules = parse_rules(source, "rules.toml").unwrap();
    assert_eq!(rules.constants().base_defense, 13);
}

#[test]
fn shared_rules_and_authored_overrides_are_pinned_by_the_certificate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let template = read_package(&root).unwrap();
    let directory = tempfile::tempdir().unwrap();
    write_package_scripted(
        directory.path(),
        &template.manifest,
        &template.region_defs().unwrap(),
        &template.rules_source,
        &template.scripts,
    )
    .unwrap();
    let certificate = validate(directory.path()).unwrap();
    assert_eq!(certificate.ruleset, RULESET);
    assert_eq!(certificate.ruleset_dependencies.len(), 1);
    assert_eq!(
        certificate.ruleset_dependencies[RULESET],
        source_digest(tor_simulation::rules::DEFAULT_RULESET_TOML)
    );
    assert_ne!(
        certificate.files[&template.manifest.ruleset],
        certificate.ruleset_hash
    );
    let path = directory.path().join(&template.manifest.ruleset);
    std::fs::write(
        &path,
        format!("{}\n# tuning source changed\n", template.rules_source),
    )
    .unwrap();
    assert!(load(directory.path(), 42, None, false).is_err());
    let changed = read_package(directory.path()).unwrap();
    // Comments alter authored content identity, not the effective rules.
    assert_eq!(changed.certificate.ruleset_hash, certificate.ruleset_hash);
    assert_eq!(changed.certificate.model_hash, certificate.model_hash);
    assert_ne!(changed.certificate.content_hash, certificate.content_hash);
    std::fs::remove_file(path).unwrap();
    assert!(read_package(directory.path()).is_err());
}

#[test]
fn bundled_rules_paths_cannot_escape_or_replace_scenario_inputs() {
    for path in [
        "../rules.toml",
        "/rules.toml",
        "scenario.toml",
        "regions/1.toml",
        "rules.json",
    ] {
        assert!(!valid_rules_path(path), "{path}");
    }
    assert!(valid_rules_path("rules.toml"));
    assert!(valid_rules_path("rules/combat.toml"));
}

#[test]
fn validation_and_loading_ignore_crlf_source_line_endings() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let template = read_package(&root).unwrap();
    let directory = tempfile::tempdir().unwrap();
    write_package_scripted(
        directory.path(),
        &template.manifest,
        &template.region_defs().unwrap(),
        &template.rules_source,
        &template.scripts,
    )
    .unwrap();
    let paths: Vec<_> = std::iter::once(directory.path().join("scenario.toml"))
        .chain(std::iter::once(
            directory.path().join(&template.manifest.ruleset),
        ))
        .chain(
            template
                .index
                .regions
                .iter()
                .map(|region| directory.path().join(&region.file)),
        )
        .collect();
    let lf: Vec<_> = paths
        .iter()
        .map(|path| {
            let text = std::fs::read_to_string(path).unwrap();
            assert!(!text.contains('\r'), "generated package source must use LF");
            text
        })
        .collect();
    for (path, text) in paths.iter().zip(&lf) {
        std::fs::write(path, text.replace('\n', "\r\n")).unwrap();
    }
    let crlf_certificate = validate(directory.path()).unwrap();
    assert_eq!(
        crlf_certificate.files["scenario.toml"],
        digest(lf[0].as_bytes())
    );
    for (path, text) in paths.iter().zip(&lf) {
        std::fs::write(path, text).unwrap();
    }
    let lf_certificate = validate(directory.path()).unwrap();
    assert_eq!(crlf_certificate, lf_certificate);
    let expected = load(directory.path(), 42, None, false)
        .unwrap()
        .package
        .unwrap()
        .build(42, false)
        .unwrap();
    for (path, text) in paths.iter().zip(&lf) {
        std::fs::write(path, text.replace('\n', "\r\n")).unwrap();
    }
    assert_eq!(
        load(directory.path(), 42, None, false)
            .unwrap()
            .package
            .unwrap()
            .build(42, false)
            .unwrap(),
        expected
    );
}

#[test]
fn generated_region_comments_change_integrity_hash_without_changing_content() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/generated-filler");
    let template = read_package(&root).unwrap();
    let directory = tempfile::tempdir().unwrap();
    write_package_scripted(
        directory.path(),
        &template.manifest,
        &template.region_defs().unwrap(),
        &template.rules_source,
        &template.scripts,
    )
    .unwrap();
    let before = read_package(directory.path()).unwrap();
    let file = directory.path().join("regions/2.toml");
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(
        file,
        format!("# Author notes must not reroll the cave.\n{text}\n"),
    )
    .unwrap();
    let after = read_package(directory.path()).unwrap();
    assert_ne!(
        before.index.region(2).unwrap().hash,
        after.index.region(2).unwrap().hash
    );
    for seed in [0, 1, 42, u64::MAX] {
        let original = before.index(seed).unwrap().region(&before, 2).unwrap();
        let annotated = after.index(seed).unwrap().region(&after, 2).unwrap();
        assert_eq!(
            serde_json::to_value(&*original).unwrap(),
            serde_json::to_value(&*annotated).unwrap(),
            "seed {seed}"
        );
    }
}

#[test]
fn construction_reference_errors_identify_source_without_reordering_failures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let template = read_package(&root).unwrap();
    let mut manifest = template.manifest.clone();
    let mut regions = template.region_defs().unwrap();
    let definitions: Manifest = toml::from_str(include_str!(
        "../../../scenarios/tests/interactions/scenario.toml"
    ))
    .unwrap();
    manifest.creatures = definitions.creatures;
    manifest
        .creatures
        .species
        .get_mut("figure")
        .unwrap()
        .modifiers = vec![crate::creature_authoring::Modifier {
        cancellable: false,
        scope: crate::creature_authoring::Scope::Always,
        op: crate::creature_authoring::Op::Add {
            stat: crate::creature_authoring::Stat::Health,
            value: 33,
        },
    }];
    let creature = definitions.characters[0].creature.clone().unwrap();
    let mut actor: Actor = serde_json::from_value(serde_json::json!({
        "id": 2, "at": [3, 1, 0], "controller": "ai", "ai": "missing",
        "creature": creature,
        "body": {"cells": [[0, 0, 0]], "eye": [1, 0, 0], "mass": 91}
    }))
    .unwrap();
    regions[0].actors.push(actor.clone());
    let package = Package::from_parts_scripted(
        manifest.clone(),
        regions.clone(),
        &template.rules_source,
        template.scripts.clone(),
    )
    .unwrap();
    let failure = package.build(42, true).unwrap_err();
    assert_eq!(failure.code, tor_protocol::ErrorCode::InvalidAction);
    assert!(failure.message.starts_with("regions/1.toml:"));
    assert!(failure
        .message
        .ends_with(": region 1, actor 2: Unknown actor AI profile \"missing\""));
    manifest
        .ai_profiles
        .insert("careful".into(), AiProfile::general());
    actor.ai = Some("careful".into());
    *regions[0].actors.last_mut().unwrap() = actor;
    let package = Package::from_parts_scripted(
        manifest.clone(),
        regions.clone(),
        &template.rules_source,
        template.scripts.clone(),
    )
    .unwrap();
    let failure = package.build(42, true).unwrap_err();
    assert_eq!(failure.code, tor_protocol::ErrorCode::InvalidAction);
    assert_eq!(
        failure.message,
        "regions/1.toml: region 1, actor 2: Actor body eye must be one of its cells"
    );

    // A valid non-default body must precede capability acquisition, not
    // retire the actor's one-lifetime readied preparation during setup.
    let body = BodySpec {
        cells: vec![[0, 0, 0], [0, 0, 1]],
        eye: [0, 0, 1],
        mass: 91,
    };
    regions[0].actors.last_mut().unwrap().body = Some(body.clone());
    for value in [
        crate::creature_authoring::Perception::ObjectGuise {
            appearance: Some(crate::creature_authoring::GuiseAppearance {
                name: "fixture object".into(),
                description: String::new(),
                class: ItemClass::Tool,
                asset: None,
            }),
            skill: "stealth".into(),
            edge: 0,
        },
        crate::creature_authoring::Perception::ReadiedTouchAmbush { enabled: true },
    ] {
        manifest
            .creatures
            .species
            .get_mut("figure")
            .unwrap()
            .modifiers
            .push(crate::creature_authoring::Modifier {
                cancellable: false,
                scope: crate::creature_authoring::Scope::Always,
                op: crate::creature_authoring::Op::Perception { value },
            });
    }
    let package = Package::from_parts_scripted(
        manifest,
        regions,
        &template.rules_source,
        template.scripts.clone(),
    )
    .unwrap();
    let game = package.build(42, true).unwrap();
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let pool = serde_json::to_value(&snapshot).unwrap()["actors"]
        .as_u64()
        .unwrap() as usize;
    let recorded = serde_json::to_value(&shared).unwrap();
    let ready = &crate::checkpoint_fixture::actor(&recorded, pool, "2")["readied_ambush"];
    assert_eq!(ready["state"], "ready");
    assert_eq!(ready["body"], serde_json::to_value(body).unwrap());
    assert_eq!(
        Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap(),
        game
    );
}

#[test]
fn declaration_diagnostics_identify_source_and_preserve_precedence() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let mut package = read_package(&root).unwrap();
    package
        .manifest
        .factions
        .insert("guard".into(), BTreeSet::from(["missing".into()]));
    package.manifest.ai_profiles.insert(
        "guard".into(),
        AiProfile {
            flee_percent: Some(101),
            ..Default::default()
        },
    );
    for expected in [
        r#"scenario.toml: faction "guard": Invalid faction relationship"#,
        r#"scenario.toml: AI profile "guard": Invalid AI profile"#,
    ] {
        let failure = package.supported().unwrap_err();
        assert_eq!(failure.code, tor_protocol::ErrorCode::InvalidAction);
        assert_eq!(failure.message, expected);
        if expected.contains("faction relationship") {
            package.manifest.factions.clear();
        } else {
            package.manifest.ai_profiles.clear();
        }
    }
    package.supported().unwrap();
    let definitions: Manifest = toml::from_str(include_str!(
        "../../../scenarios/tests/interactions/scenario.toml"
    ))
    .unwrap();
    package.manifest.creatures = definitions.creatures;
    let mut invalid = definitions.characters[0].creature.clone().unwrap();
    invalid.hit_dice.clear();
    package.manifest.characters[0].creature = Some(invalid.clone());
    package
        .manifest
        .archetypes
        .get_mut("token")
        .unwrap()
        .creature = Some(invalid.clone());
    for origin in ["character 1", r#"archetype "token""#] {
        let failure = package.index(42).unwrap_err();
        assert!(failure.message.contains(origin), "{}", failure.message);
        assert!(failure.message.contains("require 1 to 256 hit dice"));
        assert_eq!(failure.code, tor_protocol::ErrorCode::InvalidAction);
        if origin == "character 1" {
            package.manifest.characters[0].creature = None;
        } else {
            package
                .manifest
                .archetypes
                .get_mut("token")
                .unwrap()
                .creature = None;
        }
    }
    let mut regions = package.region_defs().unwrap();
    let mut actor: Actor = serde_json::from_value(serde_json::json!({
        "id": 9, "at": [2, 1, 0], "controller": "invalid", "creature": invalid
    }))
    .unwrap();
    regions[0].actors.push(actor.clone());
    let failure = package.check_region(&regions[0]).unwrap_err();
    assert_eq!(
        failure.message,
        "regions/1.toml: region 1, actor 9: Invalid actor controller"
    );
    actor.controller = "external".into();
    *regions[0].actors.last_mut().unwrap() = actor;
    let failure = Package::from_parts_scripted(
        package.manifest.clone(),
        regions,
        &package.rules_source,
        package.scripts.clone(),
    )
    .and_then(|package| package.build(42, true))
    .unwrap_err();
    assert!(
        failure
            .message
            .contains("regions/1.toml: region 1, actor 9"),
        "{}",
        failure.message
    );
    assert!(failure.message.contains("require 1 to 256 hit dice"));
    assert_eq!(failure.code, tor_protocol::ErrorCode::InvalidAction);
}

#[test]
fn compiled_instances_preserve_inheritance_and_explicit_overrides() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let template = read_package(&root).unwrap();
    let mut manifest = template.manifest.clone();
    let mut regions = template.region_defs().unwrap();
    let definitions: Manifest = toml::from_str(include_str!(
        "../../../scenarios/tests/interactions/scenario.toml"
    ))
    .unwrap();
    let mut guard_species = definitions.creatures.species["figure"].clone();
    guard_species.melee = serde_json::from_value(serde_json::json!({"skill":"heavy_weaponry","bonus":-7,"wind_up":19,"recovery":23,"strikes":[{"offset":0,"check":{"actor":"user","skill":{"kind":"selected_weapon"},"against":{"kind":"defense","defense":"physical"}},"on_success":[{"kind":"damage","damage":{"kind":"components","components":[{"category":"impact","amount":{"kind":"fixed","amount":2}},{"category":"energy","amount":{"kind":"fixed","amount":1}},{"category":"keen","amount":{"kind":"fixed","amount":3}},{"category":"spirit","amount":{"kind":"fixed","amount":4}},{"category":"vital","amount":{"kind":"fixed","amount":5}}]}}],"on_failure":[],"descriptors":[],"riders":[]}]}))
        .unwrap();
    guard_species.modifiers = serde_json::from_value(serde_json::json!([
            {"scope": "always", "op": {"kind": "add", "stat": {"kind": "health"}, "value": 33}}, {"scope": "always", "op": {"kind": "add", "stat": {"kind": "defense", "defense": "physical"}, "value": -13}},
            {"scope": "always", "op": {"kind": "immunity", "selector": {"kind": "category", "category": "energy"}}},
            {"scope": "always", "op": {"kind": "immunity", "selector": {"kind": "category", "category": "vital"}}},
            {"scope": "always", "op": {"kind": "reduction", "selector": {"kind": "category", "category": "impact"}, "amount": 6}},
            {"scope": "always", "op": {"kind": "reduction", "selector": {"kind": "category", "category": "keen"}, "amount": 7}},
            {"scope": "always", "op": {"kind": "reduction", "selector": {"kind": "category", "category": "spirit"}, "amount": 8}}
        ])).unwrap();
    let mut override_species = definitions.creatures.species["figure"].clone();
    override_species.modifiers = vec![crate::creature_authoring::Modifier {
        cancellable: false,
        scope: crate::creature_authoring::Scope::Always,
        op: crate::creature_authoring::Op::Add {
            stat: crate::creature_authoring::Stat::Health,
            value: 49,
        },
    }];
    manifest
        .creatures
        .species
        .insert("guard".into(), guard_species);
    manifest
        .creatures
        .species
        .insert("override".into(), override_species);
    let inherited_creature: crate::creature_authoring::BuildSpec = serde_json::from_value(serde_json::json!({
            "species": "guard", "name": "guard", "corpse_identity": "item.corpse.test.guard", "faction": "neutral", "binding": "intellect", "starting_skills": ["deception", "persuasion"], "restoration": "choose_again", "nutrition": {"kind":"exempt"}, "hit_dice": [{"source": "racial"}]
        })).unwrap();
    let override_creature: crate::creature_authoring::BuildSpec = serde_json::from_value(serde_json::json!({
            "species": "override", "name": "override", "corpse_identity": "item.corpse.test.override", "faction": "neutral", "binding": "intellect", "starting_skills": ["deception", "persuasion"], "restoration": "choose_again", "nutrition": {"kind":"exempt"}, "hit_dice": [{"source": "racial"}]
        })).unwrap();
    let inherited_body = BodySpec {
        cells: vec![[0, 0, 0]],
        eye: [0, 0, 0],
        mass: 91,
    };
    let override_body = BodySpec {
        mass: 137,
        ..inherited_body.clone()
    };
    manifest.archetypes.insert(
        "guard".into(),
        Archetype {
            creature: Some(inherited_creature.clone()),
            body: Some(inherited_body.clone()),
            identity: Some("item.misc.guard-item".parse().unwrap()),
            name: Some("guard token".into()),
            stackable: true,
            properties: BTreeMap::from([
                ("quality".into(), "fine".into()),
                ("origin".into(), "authored".into()),
            ]),
            turn_ticks: Some(73),
            asset: Some("creature.guard".into()),
            ..Default::default()
        },
    );
    manifest
        .assets
        .insert("stone".into(), vec!["creature.guard".into()]);
    manifest.ai_profiles.insert(
        "careful".into(),
        AiProfile {
            care: Some(true),
            equipment: Some(true),
            loot: Some(true),
            flee_percent: Some(19),
            ..Default::default()
        },
    );
    regions[0].actors.extend([
        Actor {
            creature: None,
            anatomy: None,
            known_identities: vec![],
            id: 2,
            at: [3, 1, 0],
            archetype: Some("guard".into()),
            body: None,
            turn_ticks: None,
            controller: "ai".into(),
            ai: Some("careful".into()),
            velocity: None,
        },
        Actor {
            creature: Some(override_creature.clone()),
            anatomy: None,
            known_identities: vec![],
            id: 3,
            at: [4, 1, 0],
            archetype: Some("guard".into()),
            body: Some(override_body.clone()),
            turn_ticks: Some(89),
            controller: "external".into(),
            ai: None,
            velocity: Some([1, 0, 0]),
        },
    ]);
    regions[0].items.extend([
        Item {
            identity: None,
            equipped_slot: None,
            class: None,
            id: 3,
            at: [1, 1, 0],
            archetype: Some("guard".into()),
            name: None,
            quantity: 3,
            stackable: None,
            properties: BTreeMap::new(),
            carried_by: None,
            seed_names: vec![],
        },
        Item {
            identity: None,
            equipped_slot: None,
            class: None,
            id: 4,
            at: [1, 1, 0],
            archetype: Some("guard".into()),
            name: Some("named gift".into()),
            quantity: 1,
            stackable: Some(false),
            properties: BTreeMap::from([("quality".into(), "ordinary".into())]),
            carried_by: Some(1),
            seed_names: vec![],
        },
    ]);
    let catalog = manifest
        .creatures
        .compile(template.rules().unwrap())
        .unwrap();
    let package = Package::from_parts_scripted(
        manifest,
        regions,
        &template.rules_source,
        template.scripts.clone(),
    )
    .unwrap();
    let game = package.build(42, true).unwrap();
    // Expected explicit recipes share the actual immutable package palette.
    // Keep the full build comparison, including Content; a native neutral
    // catalog is not this authored package's presentation catalog.
    let catalog = catalog.with_presentation_catalog(game.content().presentation_catalog().clone());
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let shared = serde_json::to_value(shared).unwrap();
    for (id, recipe, body, ticks) in [
        ("2", inherited_creature, inherited_body, 73),
        ("3", override_creature, override_body, 89),
    ] {
        let actor = crate::checkpoint_fixture::actor(
            &shared,
            snapshot["actors"].as_u64().unwrap() as usize,
            id,
        );
        let id_number = id.parse::<u64>().unwrap();
        let expected = catalog
            .prepare(&recipe)
            .unwrap()
            .instantiate(crate::creature_authoring::actor_health_seed(42, id_number))
            .unwrap();
        let actual = game.creature(tor_simulation::ActorId(id_number)).unwrap();
        assert_eq!(actual.build(), &expected);
        assert_eq!(actual.derived(), &expected.derive().unwrap());
        assert_eq!(
            game.inspect_creature(tor_simulation::ActorId(id_number))
                .unwrap()
                .identity
                .name,
            recipe.name
        );
        assert_eq!(actor["body"], serde_json::to_value(body).unwrap());
        assert_eq!(actor["turn_ticks"], ticks);
        assert_eq!(actor["asset"], "creature.guard");
    }
    assert_eq!(
        shared["ai_records"][snapshot["combat"]["ai"]["2"].as_u64().unwrap() as usize]["profile"],
        serde_json::json!({"idle": "explore", "doors": false,
                "care": true, "equipment": true, "loot": true, "gold": false, "flee_percent": 19})
    );
    assert_eq!(
        crate::checkpoint_fixture::actor(
            &shared,
            snapshot["actors"].as_u64().unwrap() as usize,
            "3"
        )["motion"]["velocity"],
        serde_json::json!([1, 0, 0])
    );
    let items = &shared["items"][snapshot["items"].as_u64().unwrap() as usize];
    assert_eq!(items["3"]["quantity"], 3);
    assert_eq!(items["3"]["spec"]["name"], "guard token");
    assert_eq!(items["3"]["spec"]["stackable"], true);
    assert_eq!(items["4"]["spec"]["name"], "named gift");
    assert_eq!(items["4"]["spec"]["stackable"], false);
    assert_eq!(
        items["4"]["spec"]["properties"],
        serde_json::json!({"quality": "ordinary", "origin": "authored"})
    );
    for id in ["3", "4"] {
        assert_eq!(items[id]["spec"]["identity"], "item.misc.guard-item");
        assert_eq!(items[id]["spec"]["asset"], "creature.guard");
    }
}

#[test]
fn prepared_definitions_are_shared_and_isolated_from_author_edits() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let mut package = read_package(&root).unwrap();
    let prepared = package.index(42).unwrap();
    let copy = prepared.clone();
    assert!(Arc::ptr_eq(&prepared.definitions, &copy.definitions));
    let before: Vec<_> = [1, 2]
        .into_iter()
        .map(|id| package.build_region(42, &prepared, id).unwrap())
        .collect();
    package.manifest.characters[0].body.as_mut().unwrap().mass = 91;
    for archetype in package.manifest.archetypes.values_mut() {
        archetype.name = Some("renamed author definition".into());
    }
    for (offset, id) in [1, 2].into_iter().enumerate() {
        assert_eq!(package.build_region(42, &copy, id).unwrap(), before[offset]);
    }
    let updated = package.index(42).unwrap();
    assert!(!Arc::ptr_eq(&prepared.definitions, &updated.definitions));
    for (offset, id) in [1, 2].into_iter().enumerate() {
        assert_ne!(
            package.build_region(42, &updated, id).unwrap(),
            before[offset]
        );
    }
}

#[test]
fn prepared_region_avoids_recopying_archetypes_at_every_scale() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let mut results = Vec::new();
    for count in [16, 256, 4096] {
        let mut package = read_package(&root).unwrap();
        for id in 0..count {
            package.manifest.archetypes.insert(
                format!("unrelated-{id}"),
                Archetype {
                    name: Some(format!("unrelated-{id}")),
                    body: Some(BodySpec {
                        cells: vec![[0, 0, 0]],
                        eye: [0, 0, 0],
                        mass: 80,
                    }),
                    ..Default::default()
                },
            );
        }
        let index = package.index(42).unwrap();
        let expected = package.build_region(42, &index, 2).unwrap();
        ARCHETYPE_DEFINITION_COPIES.with(|copies| copies.set(0));
        let actual = package.build_region(42, &index, 2).unwrap();
        assert_eq!(actual, expected);
        results.push((
            count,
            ARCHETYPE_DEFINITION_COPIES.with(|copies| copies.get()),
        ));
    }
    assert_eq!(results, vec![(16, 0), (256, 0), (4096, 0)]);
}

#[test]
fn authoring_rejects_removed_profiles_and_preserves_nested_body_requirements() {
    let baseline = serde_json::json!({"id": 7, "anchor": "1/start"});
    assert!(serde_json::from_value::<Character>(baseline.clone()).is_ok());
    for profile in [
        serde_json::json!({}),
        serde_json::Value::Null,
        serde_json::json!({"max_hp": 30}),
    ] {
        let mut character = baseline.clone();
        character["combat"] = profile.clone();
        assert!(serde_json::from_value::<Character>(character)
            .unwrap_err()
            .to_string()
            .contains("unknown field `combat`"));
        assert!(
            serde_json::from_value::<Archetype>(serde_json::json!({"combat": profile}))
                .unwrap_err()
                .to_string()
                .contains("unknown field `combat`")
        );
        assert!(serde_json::from_value::<Actor>(
            serde_json::json!({"id": 9, "at": [1, 2, 3], "combat": profile})
        )
        .unwrap_err()
        .to_string()
        .contains("unknown field `combat`"));
    }
    for body in [
        serde_json::json!({"cells": [[0, 0, 0]], "mass": 80}),
        serde_json::json!({"cells": [[0, 0, 0]], "eye": [0, 0, 0]}),
        serde_json::json!({"eye": [0, 0, 0], "mass": 80}),
    ] {
        assert!(serde_json::from_value::<Character>(serde_json::json!({
            "id": 7, "anchor": "1/start", "body": body
        }))
        .is_err());
    }
}

#[test]
fn authoring_explicit_actor_values_preserve_canonical_shape() {
    let value = serde_json::json!({
        "id": 9, "at": [1, 2, 3], "archetype": "guard", "turn_ticks": 73,
        "controller": "ai", "ai": "cautious", "velocity": [-1, 0, 2],
        "anatomy": null,
        "known_identities": ["item.potion.healing"],
        "body": {"cells": [[0, 0, 0], [0, 0, 1]], "eye": [0, 0, 1], "mass": 91},
        "creature": {
            "species": "guard", "name": "guard", "corpse_identity": "item.corpse.test.guard", "faction": "guards", "binding": "intellect",
            "attributes": null, "templates": [], "future_growth": {"kind": "disabled"},
            "body_package": null, "personal_body_grants": [],
            "starting_skills": ["deception", "persuasion"], "restoration": "choose_again", "nutrition": {"kind":"exempt"}, "hit_dice": [{"source": "racial", "purchases": [], "attribute": null}],
            "growth_schedule": []
        }
    });
    let actor: Actor = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(&actor).unwrap(), value);
    assert_eq!(actor.creature.as_ref().unwrap().species, "guard");
    let body = actor.body.unwrap();
    let author_bytes = serde_json::to_vec(&body).unwrap();
    let compiled = tor_simulation::BodySpec::from(body);
    assert_eq!(serde_json::to_value(&compiled).unwrap(), value["body"]);
    assert_eq!(serde_json::to_vec(&compiled).unwrap(), author_bytes);
}

#[test]
fn authoring_ai_overrides_inherit_without_losing_explicit_false_or_zero() {
    let species = tor_simulation::behavior::BehaviorPolicy {
        doors: true,
        care: true,
        equipment: true,
        loot: true,
        gold: true,
        ..Default::default()
    };
    let author: AiProfile = serde_json::from_value(serde_json::json!({
        "idle":"wait","doors":false,"care":false,"gold":false,"flee_percent":0
    }))
    .unwrap();
    let resolved = author.resolve(species);
    assert_eq!(resolved.idle, tor_simulation::behavior::IdleBehavior::Wait);
    assert!(!resolved.doors && !resolved.care && !resolved.gold);
    assert!(resolved.equipment && resolved.loot);
    assert_eq!(resolved.flee_percent, 0);
    assert_eq!(AiProfile::default().resolve(species), species.into());
    assert!(serde_json::from_value::<AiProfile>(serde_json::json!({"unknown":true})).is_err());
    assert!(
        !serde_json::from_value::<AiProfile>(serde_json::json!({"flee_percent":101}))
            .unwrap()
            .valid()
    );
}

#[test]
fn acquisition_file_growth_cannot_bypass_the_read_limit() {
    struct Counted<'a> {
        input: &'a [u8],
        consumed: &'a std::cell::Cell<usize>,
    }
    impl Read for Counted<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let size = self.input.read(buffer)?;
            self.consumed.set(self.consumed.get() + size);
            Ok(size)
        }
    }
    // The opened stream can grow after its metadata was checked.
    let consumed = std::cell::Cell::new(0);
    let result = read_text_limited(
        Counted {
            input: b"12345678901234567890",
            consumed: &consumed,
        },
        "growing.toml",
        8,
    );
    assert_eq!((result.is_err(), consumed.get()), (true, 9));
    assert_eq!(
        read_text_limited(&b"12345678"[..], "exact.toml", 8).unwrap(),
        "12345678"
    );
    assert_eq!(
        read_text_limited("éé".as_bytes(), "utf8.toml", 4).unwrap(),
        "éé"
    );
    assert!(read_text_limited(&[0xff][..], "invalid.toml", 4).is_err());
}

#[test]
fn saved_index_obeys_the_same_byte_limit_as_package_files() {
    let expected = RegionIndex::default();
    let mut bytes = expected.to_bytes().unwrap();
    bytes.resize(MAX_INDEX_BYTES as usize, b' ');
    assert_eq!(RegionIndex::from_bytes(&bytes).unwrap(), expected);
    bytes.push(b' ');
    assert!(
        RegionIndex::from_bytes(&bytes).is_err(),
        "saved indexes must reject bytes beyond the package-file limit"
    );
}

#[test]
fn authored_default_matches_existing_fixture_at_every_seed_variant() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let package = read_package(&root).unwrap();
    for seed in [0, 1, 2, 42] {
        let mut actual = package.build(seed, true).unwrap();
        let mut expected = Game::two_room_in_stone(seed, package.content().unwrap());
        expected
            .spawn_actor(loc(1, [1, 1, 0]), NonZeroU64::new(100).unwrap())
            .unwrap();
        // Packages now retain archetype identity; the legacy diagnostic
        // fixture has only ordinary names. Verify the complete disclosed
        // state and deterministic play rather than erasing that identity.
        let actor = tor_simulation::ActorId(1);
        expected
            .set_body(
                actor,
                tor_simulation::BodySpec {
                    cells: vec![[0, 0, 0], [0, 0, 1]],
                    eye: [0, 0, 1],
                    mass: 80,
                },
            )
            .unwrap();
        expected.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
        expected.set_gravity(RegionId(2), [0, 0, -1]).unwrap();
        assert_eq!(
            actual.observe(actor).unwrap(),
            expected.observe(actor).unwrap()
        );
        for action in [
            tor_simulation::Action::Take {
                item: tor_simulation::ItemId(1),
                quantity: None,
            },
            tor_simulation::Action::Drop {
                item: tor_simulation::ItemId(1),
                quantity: None,
            },
            tor_simulation::Action::Move(Direction::East),
            tor_simulation::Action::Wait,
        ] {
            assert_eq!(
                actual.act(actor, action.clone()),
                expected.act(actor, action)
            );
            assert_eq!(
                actual.observe(actor).unwrap(),
                expected.observe(actor).unwrap()
            );
        }
    }
}
