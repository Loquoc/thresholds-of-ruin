//! Stable, typed content identities. Runtime loading interns strings; persistence uses text.
//!
//! Construction proves syntax only. A valid [`AbilityId`] still needs an
//! entry in the attached [`crate::rules::Ruleset`]. These IDs are not the
//! hierarchical [`crate::ItemIdentity`] grammar and are not normalized.
//!
//! Each generated type is a distinct namespace even when the text matches.
//! Converting an ID to text does not authorize a cross-category reference:
//!
//! ```compile_fail,E0308
//! use tor_simulation::content_ids::{AbilityId, TalentId};
//! let ability = AbilityId::new("fixture.shared").unwrap();
//! let talent: TalentId = ability; // A matching spelling is not a type conversion.
//! ```
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

/// Check content-ID syntax, not membership in any loaded catalog.
///
/// IDs are nonempty, at most [`crate::limits::ID_BYTES`] UTF-8 bytes, and contain
/// only ASCII lowercase letters, digits, underscores, hyphens and periods.
///
/// ```
/// use tor_simulation::content_ids::{AbilityId, TalentId, valid_id};
/// assert!(valid_id("fixture.basic-melee"));
/// assert!(!valid_id("Fixture basic melee"));
/// let ability = AbilityId::new("fixture.shared").unwrap();
/// let talent = TalentId::new("fixture.shared").unwrap();
/// assert_eq!(ability.as_str(), talent.as_str()); // Separate typed namespaces.
/// assert_eq!(serde_json::to_string(&ability).unwrap(), "\"fixture.shared\"");
/// assert!(AbilityId::new("").is_err());
/// // Successful construction does not establish that this ability is authored.
/// ```
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= crate::limits::ID_BYTES
        && value.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'.')
        })
}
macro_rules! ids {
    ($($name:ident),+ $(,)?) => {$ (
        #[doc = concat!("A syntactically validated `", stringify!($name), "` content identity.")]
        ///
        /// Uses the grammar checked by [`valid_id`] and serializes as text.
        /// This typed value does not prove membership in a particular catalog.
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(Arc<str>);
        impl $name {
            /// Construct an identity after checking [`valid_id`].
            ///
            /// # Errors
            ///
            /// Returns [`InvalidContentId`] with the rejected text for invalid
            /// syntax or length. Catalog-reference checks are separate.
            pub fn new(value: impl AsRef<str>) -> Result<Self, InvalidContentId> {
                let value = value.as_ref();
                if !valid_id(value) { return Err(InvalidContentId(value.into())); }
                Ok(Self(Arc::from(value)))
            }
            /// Borrow the stable text used for comparison and serialization.
            pub fn as_str(&self) -> &str { &self.0 }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(|e| serde::de::Error::custom(format!("invalid content id: {}", e.0)))
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { self.as_str().fmt(f) }
        }
    )+};
}
// Capability identities can use the same validated representation without
// claiming an interning owner. Existing catalog identities retain interning.
macro_rules! interned_ids {
    ($($name:ident),+ $(,)?) => {
        ids!($($name),+);
        $(impl $name {
            pub(crate) fn intern(&mut self, pool: &mut Interner) {
                self.0 = pool.intern(&self.0);
            }
        })+
    };
}
ids!(LauncherFamilyId, BodyFormId);
interned_ids!(
    RulesetId,
    AbilityId,
    AbilityTagId,
    FormId,
    TalentId,
    ExpertiseId,
    TypeId,
    SubtypeId,
    ClassId,
    ConditionId,
    ConditionFamilyId,
    DescriptorId,
    FlagId,
    SpeciesId,
    TemplateId,
    EquipmentSlot,
    EquipmentCapacityId,
    PointPoolId,
    SupplyId
);
#[derive(Clone, Debug, PartialEq, Eq)]
/// Rejected text from a content identity's syntax/length check.
pub struct InvalidContentId(pub String);
#[derive(Default)]
pub(crate) struct Interner(BTreeMap<String, Arc<str>>);
impl Interner {
    fn intern(&mut self, value: &str) -> Arc<str> {
        self.0
            .entry(value.into())
            .or_insert_with(|| Arc::from(value))
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_serialize_as_stable_strings() {
        let id = TalentId::new("custom_talent").unwrap();
        assert_eq!(serde_json::to_string(&id).unwrap(), "\"custom_talent\"");
        assert_eq!(
            serde_json::from_str::<TalentId>("\"custom_talent\"").unwrap(),
            id
        );
        assert!(TalentId::new("").is_err());
        assert!(serde_json::from_str::<TalentId>("\"bad id\"").is_err());
        assert!(TalentId::new("wrong space").is_err());
        assert!(TalentId::new("x".repeat(crate::limits::ID_BYTES + 1)).is_err());
    }
    #[test]
    fn interning_reuses_one_allocation_even_across_content_categories() {
        let mut ability = AbilityId::new("shared").unwrap();
        let mut talent = TalentId::new("shared").unwrap();
        let mut pool = Interner::default();
        ability.intern(&mut pool);
        talent.intern(&mut pool);
        assert!(Arc::ptr_eq(&ability.0, &talent.0));
    }
}
