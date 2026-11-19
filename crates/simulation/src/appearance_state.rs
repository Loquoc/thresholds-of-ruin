//! Pure observer-presentation selection. No world queries, clocks, RNG mutation,
//! identity disclosure or saved-state integration live in this primitive.

pub const MAX_PRESENTATION_CHOICES: usize = crate::limits::CATALOG_ENTRIES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum AppearanceChannel {
    Creature = 1,
    Item = 2,
    Terrain = 3,
    MonsterSense = 4,
}

/// Private stable inputs, never a public target handle or a source of sight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppearanceKey {
    pub seed: u64,
    pub observer: u64,
    pub subject: u64,
    pub channel: AppearanceChannel,
    pub phase: u64,
}

/// Select in a caller-provided canonical catalog. The version-one recipe folds
/// seed, observer, subject, channel and phase in that order from domain
/// 0x6170706561727631. For each operand v, replace x with mix(x XOR v), where
/// mix adds 0x9e3779b97f4a7c15, multiplies (x XOR (x >> 30)) by
/// 0xbf58476d1ce4e5b9, multiplies (x XOR (x >> 27)) by 0x94d049bb133111eb,
/// then returns x XOR (x >> 31). Addition/multiplication wrap at 64 bits.
/// Reduce the final unsigned value modulo count. Catalog order is authority;
/// this deterministic presentation choice is not a cryptographic primitive.
pub fn presentation_index(key: AppearanceKey, count: usize) -> Option<usize> {
    if !(1..=MAX_PRESENTATION_CHOICES).contains(&count) {
        return None;
    }
    presentation_index_in_validated_palette(key, count)
}

/// Internal selection after immutable palette construction has charged its
/// complete canonical rows against the retained source-envelope budget. This
/// does not change the public primitive's independent 4096-choice contract.
/// No allocation, query RNG, source scan or hierarchical probability change.
pub(crate) fn presentation_index_in_validated_palette(
    key: AppearanceKey,
    count: usize,
) -> Option<usize> {
    let count = u64::try_from(count).ok().filter(|count| *count != 0)?;
    usize::try_from(appearance_hash(key) % count).ok()
}

fn appearance_hash(key: AppearanceKey) -> u64 {
    let mut mixed = 0x6170706561727631;
    for operand in [
        key.seed,
        key.observer,
        key.subject,
        key.channel as u64,
        key.phase,
    ] {
        mixed = mix(mixed ^ operand);
    }
    mixed
}

fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

/// Every duration is exactly two independent d10 draws in fixed game ticks.
pub(crate) const HALLUCINATION_TICK_UNIT: u64 = 100;
const HALLUCINATION_DURATION_RING: usize = 1024;

/// Pure random-access disguise interval. key.phase is the condition generation
/// for this duration stream, NOT the resulting catalog selector phase.
/// The private ring repeats durations only after at least204800 ticks; selector
/// interval numbers keep increasing, so catalog appearances do not loop with it.
/// Typical reads stop at the requested interval, without generating the ring.
/// No game RNG, per-entity state, world membership or query order is involved.
pub(crate) fn hallucination_interval(key: AppearanceKey, elapsed_ticks: u64) -> u64 {
    if elapsed_ticks < 2 * HALLUCINATION_TICK_UNIT {
        return 0;
    }
    let mut state = appearance_hash(key) ^ 0x68616c6c5f326431;
    let mut remaining = elapsed_ticks;
    let mut durations = [0u16; HALLUCINATION_DURATION_RING];
    let mut total = 0u64;
    for (index, duration) in durations.iter_mut().enumerate() {
        *duration = crate::dice::die(&mut state, 10) + crate::dice::die(&mut state, 10);
        let ticks = u64::from(*duration) * HALLUCINATION_TICK_UNIT;
        total += ticks;
        if remaining < ticks {
            return index as u64;
        }
        remaining -= ticks;
    }
    // One complete ring has already been consumed. Both products are bounded
    // by elapsed/min_duration, including u64::MAX; no per-elapsed-time loop.
    let completed = 1 + remaining / total;
    remaining %= total;
    for (index, duration) in durations.into_iter().enumerate() {
        let ticks = u64::from(duration) * HALLUCINATION_TICK_UNIT;
        if remaining < ticks {
            return completed * HALLUCINATION_DURATION_RING as u64 + index as u64;
        }
        remaining -= ticks;
    }
    unreachable!("remainder is strictly shorter than the duration ring")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> AppearanceKey {
        AppearanceKey {
            seed: 42,
            observer: 1,
            subject: 2,
            channel: AppearanceChannel::Creature,
            phase: 0,
        }
    }

    #[test]
    fn exact_two_d10_intervals_preserve_boundaries_and_random_access() {
        let k = key();
        let mut local = appearance_hash(k) ^ 0x68616c6c5f326431;
        let mut boundary = 0;
        let mut durations = Vec::new();
        for interval in 0..1024u64 {
            let a = crate::dice::die(&mut local, 10);
            let b = crate::dice::die(&mut local, 10);
            assert!((1..=10).contains(&a) && (1..=10).contains(&b));
            let duration = u64::from(a + b) * 100;
            assert!((200..=2000).contains(&duration));
            assert_eq!(hallucination_interval(k, boundary), interval);
            assert_eq!(hallucination_interval(k, boundary + duration - 1), interval);
            boundary += duration;
            durations.push(duration);
        }
        assert_eq!(hallucination_interval(k, boundary), 1024);
        assert_eq!(hallucination_interval(k, boundary + durations[0] - 1), 1024);
        assert_eq!(hallucination_interval(k, boundary + durations[0]), 1025);
        assert_eq!(
            hallucination_interval(k, u64::MAX),
            hallucination_interval(k, u64::MAX)
        );
        let span = boundary;
        let cycles = u64::MAX / span;
        assert_eq!(hallucination_interval(k, cycles * span), cycles * 1024);
    }

    #[test]
    fn disguise_timing_is_private_staggered_and_first_interval_has_full_minimum() {
        for channel in [
            AppearanceChannel::Creature,
            AppearanceChannel::Item,
            AppearanceChannel::Terrain,
            AppearanceChannel::MonsterSense,
        ] {
            let starts: std::collections::BTreeSet<_> = (1..33)
                .map(|subject| {
                    let k = AppearanceKey {
                        subject,
                        channel,
                        ..key()
                    };
                    assert_eq!(hallucination_interval(k, 0), 0);
                    assert_eq!(hallucination_interval(k, 199), 0);
                    (200..=2000)
                        .step_by(100)
                        .find(|t| hallucination_interval(k, *t) == 1)
                        .unwrap()
                })
                .collect();
            assert!(starts.len() > 5, "subjects must not shift in lockstep");
        }
        let schedules = |k| {
            (0..80)
                .map(|i| hallucination_interval(k, i * 100))
                .collect::<Vec<_>>()
        };
        assert_ne!(
            schedules(key()),
            schedules(AppearanceKey {
                observer: 9,
                ..key()
            })
        );
        assert_ne!(
            schedules(key()),
            schedules(AppearanceKey { phase: 9, ..key() })
        );
    }

    #[test]
    fn normal_disguise_lookup_does_not_eagerly_generate_duration_ring() {
        let before = crate::dice::random_word_draws_for_test();
        assert_eq!(hallucination_interval(key(), 199), 0);
        assert_eq!(crate::dice::random_word_draws_for_test(), before);
        hallucination_interval(key(), 85000);
        // Every interval is >=200 ticks. At most426 intervals plus vanishingly
        // rare unbiased-rejection draws; exact fixture requires far fewer.
        assert!(crate::dice::random_word_draws_for_test() - before < 852);
    }

    #[test]
    fn duration_stream_matches_independent_two_die_vectors() {
        // Independently evaluated pairs: (2,9),(1,1),(9,10),(7,7),(9,7),(2,4),(7,9),(7,3).
        let mut boundary = 0;
        for (index, duration) in [1100, 200, 1900, 1400, 1600, 600, 1600, 1000]
            .into_iter()
            .enumerate()
        {
            assert_eq!(hallucination_interval(key(), boundary), index as u64);
            assert_eq!(
                hallucination_interval(key(), boundary + duration - 1),
                index as u64
            );
            boundary += duration;
        }
        assert_eq!(hallucination_interval(key(), boundary), 8);
    }
    #[test]
    fn validated_palette_selector_preserves_old_recipe_without_old_public_cap() {
        for count in [1, 2, 26, 75, MAX_PRESENTATION_CHOICES] {
            assert_eq!(
                presentation_index_in_validated_palette(key(), count),
                presentation_index(key(), count),
            );
        }
        assert_eq!(presentation_index_in_validated_palette(key(), 0), None);
        for count in [MAX_PRESENTATION_CHOICES + 1, 8192, 100_000] {
            assert_eq!(presentation_index(key(), count), None);
            let index = presentation_index_in_validated_palette(key(), count).unwrap();
            assert!(index < count);
            assert_eq!(
                Some(index),
                presentation_index_in_validated_palette(key(), count)
            );
        }
    }

    #[test]
    fn selection_rejects_empty_and_oversized_catalogs_without_clamping() {
        assert_eq!(presentation_index(key(), 0), None);
        assert_eq!(
            presentation_index(key(), MAX_PRESENTATION_CHOICES + 1),
            None
        );
        assert_eq!(presentation_index(key(), usize::MAX), None);
        assert_eq!(presentation_index(key(), 1), Some(0));
    }

    #[test]
    fn appearance_selection_matches_frozen_unsigned_domain_vectors() {
        // Literal values independently evaluated from the documented integer
        // recipe, not recomputed by the production selector under test.
        let cases = [
            (AppearanceKey { seed: 0, ..key() }, 257, 165),
            (key(), 257, 183),
            (
                AppearanceKey {
                    observer: 2,
                    ..key()
                },
                257,
                173,
            ),
            (
                AppearanceKey {
                    subject: 3,
                    ..key()
                },
                257,
                94,
            ),
            (
                AppearanceKey {
                    channel: AppearanceChannel::Item,
                    ..key()
                },
                257,
                63,
            ),
            (
                AppearanceKey {
                    channel: AppearanceChannel::Terrain,
                    ..key()
                },
                257,
                192,
            ),
            (
                AppearanceKey {
                    channel: AppearanceChannel::MonsterSense,
                    ..key()
                },
                257,
                28,
            ),
            (AppearanceKey { phase: 1, ..key() }, 257, 21),
            (
                AppearanceKey {
                    seed: u64::MAX,
                    observer: u64::MAX,
                    subject: u64::MAX,
                    channel: AppearanceChannel::MonsterSense,
                    phase: u64::MAX,
                },
                4096,
                1668,
            ),
        ];
        for (key, count, expected) in cases {
            assert_eq!(presentation_index(key, count), Some(expected), "{key:?}");
        }
    }

    #[test]
    fn repeated_selection_is_pure_and_within_the_declared_catalog() {
        let key = key();
        let before = key;
        for count in [1, 2, 257, MAX_PRESENTATION_CHOICES] {
            let first = presentation_index(key, count).unwrap();
            assert!(first < count);
            for _ in 0..16 {
                assert_eq!(presentation_index(key, count), Some(first));
            }
        }
        assert_eq!(key, before);
    }
}
