//! Checkpoint-local complete records. Hashes accelerate lookup only; equality
//! resolves collisions and canonical indices follow first capture order.
//! serde_json writes only to an in-memory deterministic hash sink here; this
//! does not read or write files, use the network, or introduce simulation I/O.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    hash::{DefaultHasher, Hasher},
    io,
};

const MAX_RECORDS: usize = super::ARCHIVE_BYTE_LIMIT / 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SizeError {
    Missing,
    Serialization,
}

pub(super) struct Pool<T, const LIMIT: usize = MAX_RECORDS> {
    records: Vec<T>,
    encoded_bytes: Vec<Option<usize>>,
    buckets: BTreeMap<u64, Vec<usize>>,
    #[cfg(test)]
    capture_fingerprint_work: (usize, usize),
}
impl<T, const LIMIT: usize> Default for Pool<T, LIMIT> {
    fn default() -> Self {
        Self {
            records: Vec::new(),
            encoded_bytes: Vec::new(),
            buckets: BTreeMap::new(),
            #[cfg(test)]
            capture_fingerprint_work: (0, 0),
        }
    }
}
impl<T: Serialize, const LIMIT: usize> Serialize for Pool<T, LIMIT> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.records.serialize(serializer)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FingerprintError {
    Capacity,
    Serialization,
}
impl std::fmt::Display for FingerprintError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Capacity => "checkpoint record exceeds capacity",
            Self::Serialization => "checkpoint record cannot be serialized",
        })
    }
}
struct Fingerprint {
    hash: DefaultHasher,
    bytes: usize,
    limit: usize,
    capacity: bool,
}
impl io::Write for Fingerprint {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes) {
            self.capacity = true;
            return Err(io::Error::other("checkpoint record exceeds capacity"));
        }
        self.hash.write(bytes);
        self.bytes += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn fingerprint(value: &impl Serialize) -> Result<(u64, usize), FingerprintError> {
    fingerprint_with_limit(value, super::ARCHIVE_BYTE_LIMIT)
}
fn fingerprint_with_limit(
    value: &impl Serialize,
    limit: usize,
) -> Result<(u64, usize), FingerprintError> {
    let mut writer = Fingerprint {
        hash: DefaultHasher::new(),
        bytes: 0,
        limit,
        capacity: false,
    };
    let encoded = serde_json::to_writer(&mut writer, value);
    if writer.capacity {
        return Err(FingerprintError::Capacity);
    }
    encoded.map_err(|_| FingerprintError::Serialization)?;
    Ok((writer.hash.finish(), writer.bytes))
}
impl<T: Serialize + Eq, const LIMIT: usize> Pool<T, LIMIT> {
    pub(super) fn insert(&mut self, record: T) -> usize {
        self.insert_with_limit(record, super::ARCHIVE_BYTE_LIMIT)
    }

    // The production call uses the archive ceiling. A reduced limit exercises
    // the same sink and classification without allocating an oversized record.
    fn insert_with_limit(&mut self, record: T, limit: usize) -> usize {
        // Capture is infallible, but a future record serializer need not be.
        // Retain the value with a distinct capacity or serialization marker.
        // Only proven capacity permits journal-only deferral. Equality still
        // resolves collisions in the fallback bucket.
        #[cfg(test)]
        {
            self.capture_fingerprint_work.0 += 1;
        }
        let (key, bytes) = match fingerprint_with_limit(&record, limit) {
            Ok((key, bytes)) => {
                #[cfg(test)]
                {
                    self.capture_fingerprint_work.1 += bytes;
                }
                (key, Some(bytes))
            }
            Err(FingerprintError::Capacity) => (0, Some(super::ARCHIVE_BYTE_LIMIT + 1)),
            Err(FingerprintError::Serialization) => (0, None),
        };
        if let Some(index) = self
            .buckets
            .get(&key)
            .and_then(|indices| indices.iter().copied().find(|&i| self.records[i] == record))
        {
            return index;
        }
        let index = self.records.len();
        self.records.push(record);
        self.encoded_bytes.push(bytes);
        self.buckets.entry(key).or_default().push(index);
        index
    }
}
impl<T, const LIMIT: usize> Pool<T, LIMIT> {
    /// Capture calls and successfully fingerprinted bytes, excluding decoder work.
    #[cfg(test)]
    pub(super) fn capture_fingerprint_work(&self) -> (usize, usize) {
        self.capture_fingerprint_work
    }
    pub(super) fn get(&self, index: usize) -> Option<&T> {
        self.records.get(index)
    }
    pub(super) fn encoded_bytes(&self, index: usize) -> Result<usize, SizeError> {
        self.encoded_bytes
            .get(index)
            .ok_or(SizeError::Missing)?
            .ok_or(SizeError::Serialization)
    }
    pub(super) fn len(&self) -> usize {
        self.records.len()
    }
}
impl<'de, T: Deserialize<'de> + Serialize + Eq, const LIMIT: usize> Deserialize<'de>
    for Pool<T, LIMIT>
{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Both production record types have mandatory named fields exceeding
        // 64 bytes even before their payloads. This allocation ceiling cannot
        // reject a legal 64 MiB archive; the outer byte cap remains independent.
        let values = crate::bounded::Bounded::<T, LIMIT>::deserialize(deserializer)?.0;
        let mut pool = Self::default();
        for record in values {
            let (key, bytes) = fingerprint(&record).map_err(serde::de::Error::custom)?;
            if pool
                .buckets
                .get(&key)
                .is_some_and(|indices| indices.iter().any(|&i| pool.records[i] == record))
            {
                return Err(serde::de::Error::custom("duplicate checkpoint record"));
            }
            let index = pool.records.len();
            pool.records.push(record);
            pool.encoded_bytes.push(Some(bytes));
            pool.buckets.entry(key).or_default().push(index);
        }
        Ok(pool)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_fingerprint_is_retained_without_panicking_or_claiming_capacity() {
        #[derive(PartialEq, Eq)]
        struct Broken;
        impl Serialize for Broken {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("intentional serializer failure"))
            }
        }
        assert_eq!(
            fingerprint_with_limit(&Broken, 0),
            Err(FingerprintError::Serialization)
        );
        let mut pool = Pool::<Broken>::default();
        assert_eq!(pool.insert(Broken), 0);
        assert_eq!(pool.insert(Broken), 0);
        assert_eq!(pool.len(), 1);
        assert_eq!(pool.encoded_bytes(0), Err(SizeError::Serialization));
        assert_eq!(pool.encoded_bytes(1), Err(SizeError::Missing));
        assert!(serde_json::to_vec(&pool).is_err());
    }

    #[test]
    fn fingerprint_limit_preserves_capacity_classification() {
        assert_eq!(fingerprint_with_limit(&12345_u64, 5).unwrap().1, 5);
        assert_eq!(
            fingerprint_with_limit(&12345_u64, 4),
            Err(FingerprintError::Capacity)
        );
        let mut pool = Pool::<u64>::default();
        let index = pool.insert_with_limit(12345, 4);
        assert_eq!(pool.get(index), Some(&12345));
        assert_eq!(
            pool.encoded_bytes(index),
            Ok(super::super::ARCHIVE_BYTE_LIMIT + 1)
        );
    }

    #[test]
    fn record_reader_enforces_its_bound_and_rejects_duplicates() {
        assert!(serde_json::from_str::<Pool<u64, 2>>("[1,2]").is_ok());
        assert!(serde_json::from_str::<Pool<u64, 2>>("[1,2,3]").is_err());
        assert!(serde_json::from_str::<Pool<u64, 2>>("[1,1]").is_err());
    }
    #[test]
    fn hash_collisions_do_not_alias_distinct_records() {
        let mut pool = Pool::<u64>::default();
        let first = pool.insert(1);
        // Force a candidate into the next value's bucket; full equality must
        // still distinguish it instead of trusting the fingerprint.
        pool.buckets
            .insert(fingerprint(&2_u64).unwrap().0, vec![first]);
        let second = pool.insert(2);
        assert_ne!(first, second);
        assert_eq!(pool.get(first), Some(&1));
        assert_eq!(pool.get(second), Some(&2));
        assert_eq!(pool.insert(2), second);
    }
}
