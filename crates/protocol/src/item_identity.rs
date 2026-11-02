//! Canonical item identity hierarchy, separate from public appearance and names.
//! This validates syntax only. Authoring/observation owners decide which known
//! identity may be disclosed and reject reserved unknown leaves as true IDs.
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, str::FromStr};

/// A bounded canonical hierarchy such as `item.potion.healing`.
/// `item` is the root; an `unknown` sentinel can appear only at the final leaf.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemIdentity(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidItemIdentity;

impl fmt::Display for InvalidItemIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid canonical item identity")
    }
}
impl std::error::Error for InvalidItemIdentity {}

impl ItemIdentity {
    pub const MAX_BYTES: usize = 80;

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The immediate parent. The root itself has no parent.
    pub fn parent(&self) -> Option<Self> {
        self.0
            .rsplit_once('.')
            .map(|(parent, _)| Self(parent.to_owned()))
    }

    /// Strict ancestry, comparing complete segments rather than text prefixes.
    /// An identity is not its own descendant.
    pub fn is_descendant_of(&self, ancestor: &Self) -> bool {
        self.0
            .strip_prefix(ancestor.as_str())
            .is_some_and(|rest| rest.starts_with('.'))
    }

    pub fn is_unknown(&self) -> bool {
        self.0.rsplit('.').next() == Some("unknown")
    }
}

fn valid(value: &str) -> bool {
    if value.is_empty() || value.len() > ItemIdentity::MAX_BYTES {
        return false;
    }
    let mut segments = value.split('.').peekable();
    if segments.next() != Some("item") {
        return false;
    }
    while let Some(segment) = segments.next() {
        if segment.is_empty()
            || !segment.as_bytes()[0].is_ascii_lowercase()
            || segment.ends_with('-')
            || segment.contains("--")
            || !segment
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || (segment == "unknown" && segments.peek().is_some())
        {
            return false;
        }
    }
    true
}

impl FromStr for ItemIdentity {
    type Err = InvalidItemIdentity;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if valid(value) {
            Ok(Self(value.to_owned()))
        } else {
            Err(InvalidItemIdentity)
        }
    }
}
impl TryFrom<String> for ItemIdentity {
    type Error = InvalidItemIdentity;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if valid(&value) {
            Ok(Self(value))
        } else {
            Err(InvalidItemIdentity)
        }
    }
}
impl TryFrom<&str> for ItemIdentity {
    type Error = InvalidItemIdentity;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl AsRef<str> for ItemIdentity {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl fmt::Display for ItemIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}
impl Serialize for ItemIdentity {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
impl<'de> Deserialize<'de> for ItemIdentity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl de::Visitor<'_> for Visitor {
            type Value = ItemIdentity;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a bounded canonical item identity hierarchy")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                value.parse().map_err(E::custom)
            }
            fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                ItemIdentity::try_from(value).map_err(E::custom)
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(value: &str) -> ItemIdentity {
        value.parse().unwrap()
    }

    #[test]
    fn protocol_and_simulation_identity_syntax_stay_in_lockstep() {
        // Separate crate boundaries intentionally avoid a simulation -> protocol
        // dependency. This test prevents the two validation contracts drifting.
        assert_eq!(
            include_str!("../../protocol/src/item_identity.rs"),
            include_str!("../../simulation/src/item_identity.rs")
        );
    }

    #[test]
    fn canonical_identity_is_typed_bounded_and_not_normalized() {
        for value in [
            "item",
            "item.potion",
            "item.potion.restore-strength",
            "item.weapon.long-sword",
            "item.scroll.identify-armor",
            "item.v2.category-3",
            "item.potion.unknown",
            "item.unknown",
        ] {
            let parsed = id(value);
            assert_eq!(parsed.as_str(), value);
            assert_eq!(parsed.to_string(), value);
            assert_eq!(ItemIdentity::try_from(value.to_owned()).unwrap(), parsed);
            assert_eq!(ItemIdentity::try_from(value).unwrap(), parsed);
        }
        let limit = format!("item.{}", "a".repeat(75));
        assert_eq!(limit.len(), 80);
        assert!(limit.parse::<ItemIdentity>().is_ok());
        assert!(format!("{limit}a").parse::<ItemIdentity>().is_err());
    }

    #[test]
    fn invalid_tokens_and_injection_are_rejected_without_echoing_input() {
        for value in [
            "",
            "potion.healing",
            "items.potion",
            "itemx.potion",
            ".item",
            "item.",
            "item..potion",
            "Item.potion",
            "item.Potion",
            " item.potion",
            "item.potion ",
            "item.potion\n",
            "item.potion\t",
            "item.potion\0",
            "item.potión",
            "item.药水",
            "item.potion_healing",
            "item.potion/healing",
            "item.potion\\healing",
            "item.potion:healing",
            "item.potion;drop",
            "item.potion\"",
            "item.<script>",
            "item.%2epotion",
            "item.-potion",
            "item.potion-",
            "item.potion--healing",
            "item.2potion",
            "item.unknown.healing",
            "item.potion.unknown.healing",
        ] {
            let error = value.parse::<ItemIdentity>().expect_err(value);
            assert_eq!(error.to_string(), "invalid canonical item identity");
        }
    }

    #[test]
    fn hierarchy_uses_segment_boundaries_and_has_a_root() {
        let root = id("item");
        let category = id("item.potion");
        let leaf = id("item.potion.healing");
        assert_eq!(root.parent(), None);
        assert_eq!(category.parent(), Some(root.clone()));
        assert_eq!(leaf.parent(), Some(category.clone()));
        assert!(leaf.is_descendant_of(&category));
        assert!(leaf.is_descendant_of(&root));
        assert!(!leaf.is_descendant_of(&leaf));
        assert!(!category.is_descendant_of(&leaf));
        assert!(!id("item.potionx.healing").is_descendant_of(&category));
        assert!(!id("item.potion-healing").is_descendant_of(&category));
        assert!(!id("item.scroll.healing").is_descendant_of(&category));
        assert_eq!(id("item.potion.healing.extra").parent(), Some(leaf));
    }

    #[test]
    fn unknown_is_a_leaf_sentinel_not_a_substring_guess() {
        assert!(id("item.potion.unknown").is_unknown());
        assert!(id("item.unknown").is_unknown());
        for known in [
            "item",
            "item.potion",
            "item.potion.unknown-like",
            "item.potion.known-unknown",
        ] {
            assert!(!id(known).is_unknown());
        }
        assert_eq!(id("item.potion.unknown").parent(), Some(id("item.potion")));
    }

    #[test]
    fn serde_remains_a_string_and_enforces_validation_on_input() {
        let identity = id("item.potion.healing");
        assert_eq!(
            serde_json::to_string(&identity).unwrap(),
            "\"item.potion.healing\""
        );
        assert_eq!(
            serde_json::from_str::<ItemIdentity>("\"item.potion.healing\"").unwrap(),
            identity
        );
        for json in [
            "null",
            "0",
            "true",
            "[]",
            "{}",
            "\"\"",
            "\"Potion.Healing\"",
            "\"item.potion\\u0000\"",
            "\"item..potion\"",
            "\"item.potion/../healing\"",
        ] {
            assert!(
                serde_json::from_str::<ItemIdentity>(json).is_err(),
                "{json}"
            );
        }
    }
}
