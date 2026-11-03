//! Exact Hit Die counts and zero-based owners, independent of row admission.
//!
//! Both scalars retain the full `u64` domain. Neither constructing a value nor
//! converting it to a host index admits a creature build or proves that a row
//! exists. The separate [`crate::limits::HIT_DICE`] guard bounds resident rows.
use serde::{Deserialize, Serialize};

/// A number of Hit Dice, including zero; also used for one-based HD ordinals.
///
/// Serialization is an unsigned integer, not a row collection. Checked arithmetic
/// refuses overflow/underflow rather than clamping to the resident-row limit.
///
/// ```
/// use tor_simulation::HdCount;
/// let count = HdCount(65_536);
/// assert_eq!(count.checked_add(1), Some(HdCount(65_537)));
/// assert_eq!(count.checked_sub(HdCount(1)), Some(HdCount(65_535)));
/// assert_eq!(HdCount::ZERO.checked_sub(HdCount(1)), None);
/// assert_eq!(HdCount(u64::MAX).checked_add(1), None);
/// assert_eq!(serde_json::to_string(&count).unwrap(), "65536");
/// assert!(serde_json::from_str::<HdCount>("-1").is_err());
/// ```
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct HdCount(pub u64);
impl HdCount {
    /// The empty count; it has no corresponding one-based owner ordinal.
    pub const ZERO: Self = Self(0);
    /// Return the exact scalar without applying a storage limit.
    pub fn get(self) -> u64 {
        self.0
    }
    /// Convert a host-sized count, returning [`crate::progression::HdError::Arithmetic`]
    /// if it cannot be represented by `u64`.
    pub fn from_usize(value: usize) -> Result<Self, crate::progression::HdError> {
        u64::try_from(value)
            .map(Self)
            .map_err(|_| crate::progression::HdError::Arithmetic)
    }
    /// Convert to the host's addressable count domain, without admitting rows.
    ///
    /// Returns [`crate::progression::HdError::Arithmetic`] if `usize` is too narrow.
    pub fn to_usize(self) -> Result<usize, crate::progression::HdError> {
        usize::try_from(self.0).map_err(|_| crate::progression::HdError::Arithmetic)
    }
    /// Add an exact count, returning `None` on `u64` overflow.
    pub fn checked_add(self, value: u64) -> Option<Self> {
        self.0.checked_add(value).map(Self)
    }
    /// Subtract a count, returning `None` when the result would be negative.
    pub fn checked_sub(self, value: Self) -> Option<Self> {
        self.0.checked_sub(value.0).map(Self)
    }
}
impl From<HdCount> for u64 {
    fn from(value: HdCount) -> Self {
        value.0
    }
}
impl std::fmt::Display for HdCount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A zero-based HD owner identity, independent of whether its row is resident.
///
/// `HdIndex(0)` names the first HD, whose ordinal is `HdCount(1)`. The maximum
/// index can be represented, but its one-based ordinal cannot fit in `u64`.
///
/// ```
/// use tor_simulation::{HdCount, HdIndex};
/// use tor_simulation::progression::HdError;
/// assert_eq!(HdIndex(0).ordinal(), Ok(HdCount(1)));
/// assert_eq!(HdIndex::from_ordinal(HdCount(65_536)), Some(HdIndex(65_535)));
/// assert_eq!(HdIndex::from_ordinal(HdCount::ZERO), None);
/// assert_eq!(HdIndex(u64::MAX).ordinal(), Err(HdError::Arithmetic));
/// assert_eq!(HdIndex::from_usize(3).unwrap().to_usize(), Ok(3));
/// ```
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct HdIndex(pub u64);
impl HdIndex {
    /// Return the exact zero-based owner value.
    pub fn get(self) -> u64 {
        self.0
    }
    /// Convert a host index, returning [`crate::progression::HdError::Arithmetic`]
    /// if it cannot be represented by `u64`.
    pub fn from_usize(value: usize) -> Result<Self, crate::progression::HdError> {
        u64::try_from(value)
            .map(Self)
            .map_err(|_| crate::progression::HdError::Arithmetic)
    }
    /// Convert to a host index; this does not check whether the row exists.
    ///
    /// Returns [`crate::progression::HdError::Arithmetic`] if `usize` is too narrow.
    pub fn to_usize(self) -> Result<usize, crate::progression::HdError> {
        usize::try_from(self.0).map_err(|_| crate::progression::HdError::Arithmetic)
    }
    /// Return the one-based ordinal, or [`crate::progression::HdError::Arithmetic`]
    /// when incrementing the maximum `u64` index would overflow.
    pub fn ordinal(self) -> Result<HdCount, crate::progression::HdError> {
        self.0
            .checked_add(1)
            .map(HdCount)
            .ok_or(crate::progression::HdError::Arithmetic)
    }
    /// Convert a one-based ordinal to its owner; zero has no owner and returns `None`.
    pub fn from_ordinal(value: HdCount) -> Option<Self> {
        value.0.checked_sub(1).map(Self)
    }
}
impl From<HdIndex> for u64 {
    fn from(value: HdIndex) -> Self {
        value.0
    }
}
impl std::fmt::Display for HdIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wide_scalar_count_and_owner_boundaries_remain_distinct() {
        for value in [65_535, 65_536, 65_537, (1_u64 << 53) + 1, u64::MAX] {
            let count = HdCount(value);
            let owner = HdIndex::from_ordinal(count).unwrap();
            assert_eq!(owner.get(), value - 1);
            assert_eq!(owner.ordinal(), Ok(count));
            assert_eq!(count.to_string(), value.to_string());
            assert_eq!(
                serde_json::from_str::<HdCount>(&value.to_string()).unwrap(),
                count
            );
        }
        assert_eq!(HdIndex::from_ordinal(HdCount::ZERO), None);
        assert_eq!(
            HdIndex(u64::MAX).ordinal(),
            Err(crate::progression::HdError::Arithmetic)
        );
        // A host index is checked at the point a resident row is addressed.
        for value in [u64::from(u32::MAX), u64::from(u32::MAX) + 1, u64::MAX] {
            assert_eq!(HdIndex(value).to_usize().ok(), usize::try_from(value).ok());
            assert_eq!(HdCount(value).to_usize().ok(), usize::try_from(value).ok());
        }
    }
}
