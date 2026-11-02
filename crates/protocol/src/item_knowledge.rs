//! Remembered item facts disclosed to an observer, not authoritative item state.
//! Unknown is explicit: every field is required, and null is distinct from known
//! zero or false. Appearance and true identity are separate contracts.
use serde::{de, Deserialize, Deserializer, Serialize};

/// Known blessing/curse classification. No blessed behavior is defined here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum BucStatus {
    #[default]
    #[serde(rename = "item.buc.unknown")]
    Unknown,
    #[serde(rename = "item.buc.uncursed")]
    Uncursed,
    #[serde(rename = "item.buc.cursed")]
    Cursed,
}
impl<'de> Deserialize<'de> for BucStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "item.buc.unknown" => Ok(Self::Unknown),
            "item.buc.uncursed" => Ok(Self::Uncursed),
            "item.buc.cursed" => Ok(Self::Cursed),
            _ => Err(de::Error::custom("invalid item BUC knowledge key")),
        }
    }
}

/// Enchantments the observer has learned, not the item's undisclosed modifiers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct KnownEnchantments {
    pub hit: Option<i32>,
    pub damage: Option<i32>,
    pub armor: Option<u32>,
    pub potency: Option<i32>,
}

/// Condition facts last learned by this observer; not authoritative condition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct KnownItemCondition {
    pub corrosion: Option<u32>,
    pub armor_deterioration: Option<u32>,
    pub protected: Option<bool>,
}

/// Observer knowledge only. Construct this from the existing disclosure boundary,
/// never by copying unknown fields from the authoritative item record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ItemKnowledge {
    pub buc: BucStatus,
    pub enchantments: KnownEnchantments,
    pub condition: KnownItemCondition,
}

impl ItemKnowledge {
    /// Validate in-memory DTOs as well as their strict wire representation.
    pub fn valid(&self) -> bool {
        [
            self.enchantments.hit,
            self.enchantments.damage,
            self.enchantments.potency,
        ]
        .into_iter()
        .flatten()
        .all(|n| (-1_000_000..=1_000_000).contains(&n))
            && [
                self.enchantments.armor,
                self.condition.corrosion,
                self.condition.armor_deterioration,
            ]
            .into_iter()
            .flatten()
            .all(|n| n <= 1_000_000)
    }
}

const MAX_KNOWN_VALUE: u32 = 1_000_000;
fn required_option<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}

impl<'de> Deserialize<'de> for KnownEnchantments {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "required_option")]
            hit: Option<i32>,
            #[serde(deserialize_with = "required_option")]
            damage: Option<i32>,
            #[serde(deserialize_with = "required_option")]
            armor: Option<u32>,
            #[serde(deserialize_with = "required_option")]
            potency: Option<i32>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        let signed = [fields.hit, fields.damage, fields.potency];
        if signed
            .into_iter()
            .flatten()
            .any(|n| !(-1_000_000..=1_000_000).contains(&n))
            || fields.armor.is_some_and(|n| n > MAX_KNOWN_VALUE)
        {
            return Err(de::Error::custom(
                "item enchantment knowledge exceeds bound",
            ));
        }
        Ok(Self {
            hit: fields.hit,
            damage: fields.damage,
            armor: fields.armor,
            potency: fields.potency,
        })
    }
}
impl<'de> Deserialize<'de> for KnownItemCondition {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "required_option")]
            corrosion: Option<u32>,
            #[serde(deserialize_with = "required_option")]
            armor_deterioration: Option<u32>,
            #[serde(deserialize_with = "required_option")]
            protected: Option<bool>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        if [fields.corrosion, fields.armor_deterioration]
            .into_iter()
            .flatten()
            .any(|n| n > MAX_KNOWN_VALUE)
        {
            return Err(de::Error::custom("item condition knowledge exceeds bound"));
        }
        Ok(Self {
            corrosion: fields.corrosion,
            armor_deterioration: fields.armor_deterioration,
            protected: fields.protected,
        })
    }
}
impl<'de> Deserialize<'de> for ItemKnowledge {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            buc: BucStatus,
            enchantments: KnownEnchantments,
            condition: KnownItemCondition,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            buc: fields.buc,
            enchantments: fields.enchantments,
            condition: fields.condition,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    fn unknown() -> Value {
        json!({"buc":"item.buc.unknown","enchantments":{"hit":null,"damage":null,"armor":null,"potency":null},
            "condition":{"corrosion":null,"armor_deterioration":null,"protected":null}})
    }
    fn parse(value: Value) -> Result<ItemKnowledge, serde_json::Error> {
        serde_json::from_value(value)
    }

    #[test]
    fn protocol_and_simulation_knowledge_contracts_stay_in_lockstep() {
        assert_eq!(
            include_str!("../../protocol/src/item_knowledge.rs"),
            include_str!("../../simulation/src/item_knowledge.rs")
        );
    }

    #[test]
    fn default_is_explicit_all_unknown_and_roundtrips() {
        assert_eq!(
            serde_json::to_value(ItemKnowledge::default()).unwrap(),
            unknown()
        );
        assert_eq!(parse(unknown()).unwrap(), ItemKnowledge::default());
    }

    #[test]
    fn known_zero_and_false_are_not_unknown() {
        let known = ItemKnowledge {
            buc: BucStatus::Uncursed,
            enchantments: KnownEnchantments {
                hit: Some(0),
                damage: Some(0),
                armor: Some(0),
                potency: Some(0),
            },
            condition: KnownItemCondition {
                corrosion: Some(0),
                armor_deterioration: Some(0),
                protected: Some(false),
            },
        };
        let encoded = serde_json::to_value(known).unwrap();
        assert_eq!(
            encoded,
            json!({"buc":"item.buc.uncursed","enchantments":{"hit":0,"damage":0,"armor":0,"potency":0},
            "condition":{"corrosion":0,"armor_deterioration":0,"protected":false}})
        );
        assert_eq!(parse(encoded).unwrap(), known);
        assert_ne!(known, ItemKnowledge::default());
    }

    #[test]
    fn missing_fields_are_rejected_but_explicit_null_is_valid() {
        assert!(parse(unknown()).is_ok());
        for key in ["buc", "enchantments", "condition"] {
            let mut value = unknown();
            value.as_object_mut().unwrap().remove(key);
            assert!(parse(value).is_err(), "missing {key}");
        }
        for (section, keys) in [
            ("enchantments", &["hit", "damage", "armor", "potency"][..]),
            (
                "condition",
                &["corrosion", "armor_deterioration", "protected"][..],
            ),
        ] {
            for key in keys {
                let mut value = unknown();
                value[section].as_object_mut().unwrap().remove(*key);
                assert!(parse(value).is_err(), "missing {section}.{key}");
            }
            let mut value = unknown();
            value[section] = Value::Null;
            assert!(parse(value).is_err(), "null section {section}");
        }
    }

    #[test]
    fn unknown_fields_at_every_level_are_rejected() {
        let mut value = unknown();
        value["actual_curse"] = json!(true);
        assert!(parse(value).is_err());
        for section in ["enchantments", "condition"] {
            let mut value = unknown();
            value[section]["secret"] = json!(7);
            assert!(parse(value).is_err(), "{section}");
        }
    }

    #[test]
    fn every_numeric_leaf_enforces_its_signed_or_unsigned_bounds() {
        for key in ["hit", "damage", "potency"] {
            for valid in [-1_000_000, 0, 1_000_000] {
                let mut value = unknown();
                value["enchantments"][key] = json!(valid);
                assert!(parse(value).is_ok());
            }
            for invalid in [
                json!(-1_000_001),
                json!(1_000_001),
                json!(i64::MAX),
                json!(i64::MIN),
                json!(1.5),
                json!(true),
                json!("0"),
            ] {
                let mut value = unknown();
                value["enchantments"][key] = invalid;
                assert!(parse(value).is_err(), "{key}");
            }
        }
        for (section, key) in [
            ("enchantments", "armor"),
            ("condition", "corrosion"),
            ("condition", "armor_deterioration"),
        ] {
            for valid in [0, 1_000_000] {
                let mut value = unknown();
                value[section][key] = json!(valid);
                assert!(parse(value).is_ok());
            }
            for invalid in [
                json!(-1),
                json!(1_000_001),
                json!(u64::MAX),
                json!(0.5),
                json!(false),
                json!("1"),
            ] {
                let mut value = unknown();
                value[section][key] = invalid;
                assert!(parse(value).is_err(), "{section}.{key}");
            }
        }
        for invalid in [json!(0), json!(1), json!("false")] {
            let mut value = unknown();
            value["condition"]["protected"] = invalid;
            assert!(parse(value).is_err());
        }
    }

    #[test]
    fn buc_uses_only_approved_hierarchical_string_keys() {
        for (key, status) in [
            ("item.buc.unknown", BucStatus::Unknown),
            ("item.buc.uncursed", BucStatus::Uncursed),
            ("item.buc.cursed", BucStatus::Cursed),
        ] {
            assert_eq!(serde_json::to_value(status).unwrap(), json!(key));
            assert_eq!(
                serde_json::from_value::<BucStatus>(json!(key)).unwrap(),
                status
            );
        }
        for invalid in [
            json!("unknown"),
            json!("item.buc.blessed"),
            json!("item.buc.Cursed"),
            json!("item.buc.cursed.extra"),
            json!("item.buc.cursed\n"),
            json!(null),
            json!(false),
            json!({"item.buc.unknown":null}),
        ] {
            let mut value = unknown();
            value["buc"] = invalid;
            assert!(parse(value).is_err());
        }
    }

    #[test]
    fn named_records_reject_positional_sequences_and_duplicate_fields() {
        assert!(parse(json!([
            "item.buc.unknown",
            unknown()["enchantments"],
            unknown()["condition"]
        ]))
        .is_err());
        for (section, sequence) in [
            ("enchantments", json!([null, null, null, null])),
            ("condition", json!([null, null, null])),
        ] {
            let mut value = unknown();
            value[section] = sequence;
            assert!(parse(value).is_err());
        }
        assert!(serde_json::from_str::<KnownEnchantments>(
            r#"{"hit":null,"hit":0,"damage":null,"armor":null,"potency":null}"#
        )
        .is_err());
        assert!(serde_json::from_str::<KnownItemCondition>(
            r#"{"corrosion":null,"armor_deterioration":null,"protected":null,"protected":false}"#
        )
        .is_err());
    }
}
