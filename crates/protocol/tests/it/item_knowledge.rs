use serde_json::{json, Value};
use tor_protocol::*;

fn item(identified: bool) -> ItemView {
    ItemView {
        identity: if identified {
            "item.potion.healing".parse().unwrap()
        } else {
            ItemClass::Potion.unknown_identity()
        },
        knowledge: ItemKnowledge::default(),
        corpse: None,
        class: ItemClass::Potion,
        quantity: 1,
        appearance: "amber potion".into(),
        identified,
        description: String::new(),
        id: ItemTarget::from_digest([71; 32]),
        name: if identified {
            "healing potion"
        } else {
            "amber potion"
        }
        .into(),
        asset: Some("item.potion.amber".into()),
        known_supply: None,
        known_currency: None,
    }
}
fn decode(v: Value) -> Result<ItemView, serde_json::Error> {
    serde_json::from_value(v)
}

#[test]
fn identity_buc_and_numeric_knowledge_are_independent_strict_machine_fields() {
    let known = item(true);
    let mut unknown = item(false);
    unknown.knowledge.buc = BucStatus::Cursed;
    unknown.knowledge.enchantments.hit = Some(0);
    unknown.knowledge.condition.protected = Some(false);
    for original in [known, unknown] {
        let encoded = serde_json::to_value(&original).unwrap();
        assert_eq!(decode(encoded.clone()).unwrap(), original);
        for field in ["identity", "knowledge"] {
            let mut missing = encoded.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(decode(missing).is_err(), "missing {field}");
        }
    }
    let mut forged = serde_json::to_value(item(false)).unwrap();
    forged["identity"] = json!("item.potion.healing");
    assert!(
        decode(forged).is_err(),
        "unknown type cannot disclose its true identity"
    );
    let mut ambiguous = serde_json::to_value(item(false)).unwrap();
    ambiguous["identity"] = json!("item.weapon.unknown");
    assert!(
        decode(ambiguous).is_err(),
        "ordinary category sentinel must match public class"
    );
}

#[test]
fn sampled_ground_items_redact_new_facts_without_revoking_existing_affordances() {
    let mut underlying = item(true);
    underlying.class = ItemClass::Coin;
    underlying.known_currency = Some(CurrencyKind::Gold);
    underlying.name = "object".into();
    underlying.appearance = "object".into();
    underlying.asset = None;
    underlying.identity = ItemClass::Misc.unknown_identity();
    let ground = GroundItemView {
        presentation: Some(ObjectPresentation {
            name: "scarlet scroll".into(),
            asset: Some("item.scroll".into()),
            kind: ObjectPresentationKind::Item {
                class: ItemClass::Scroll,
            },
        }),
        item: underlying,
        position: Position { x: 0, y: 0, z: 0 },
        reachable: true,
        supply_reachable: false,
    };
    let encoded = serde_json::to_value(&ground).unwrap();
    let decoded: GroundItemView = serde_json::from_value(encoded.clone()).unwrap();
    assert!(decoded.item.identified);
    assert_eq!(decoded.item.known_currency, Some(CurrencyKind::Gold));
    assert_eq!(decoded.item.identity.as_str(), "item.unknown");
    for (pointer, value) in [
        ("/item/identity", json!("item.coin.gold")),
        ("/item/knowledge/buc", json!("item.buc.uncursed")),
        ("/item/knowledge/enchantments/hit", json!(0)),
    ] {
        let mut leaked = encoded.clone();
        *leaked.pointer_mut(pointer).unwrap() = value;
        assert!(
            serde_json::from_value::<GroundItemView>(leaked).is_err(),
            "leaked {pointer}"
        );
    }
}
