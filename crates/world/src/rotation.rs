//! The 24 proper rotations of a cube. IDs 0..4 retain the planar convention.
//! Composition applies the first transform, then the second; reflections are excluded.
const AXES: [[i8; 3]; 24] = [
    [1, 2, 3],
    [-2, 1, 3],
    [-1, -2, 3],
    [2, -1, 3],
    [-1, 2, -3],
    [1, -2, -3],
    [-1, -3, -2],
    [-1, 3, 2],
    [1, -3, 2],
    [1, 3, -2],
    [-2, -1, -3],
    [2, 1, -3],
    [-2, -3, 1],
    [-2, 3, -1],
    [2, -3, -1],
    [2, 3, 1],
    [-3, -1, 2],
    [-3, 1, -2],
    [3, -1, -2],
    [3, 1, 2],
    [-3, -2, -1],
    [-3, 2, 1],
    [3, -2, 1],
    [3, 2, -1],
];
const COMPOSE: [[u8; 24]; 24] = [
    [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
    ],
    [
        1, 2, 3, 0, 11, 10, 14, 15, 12, 13, 4, 5, 6, 7, 8, 9, 21, 20, 23, 22, 16, 17, 18, 19,
    ],
    [
        2, 3, 0, 1, 5, 4, 8, 9, 6, 7, 11, 10, 14, 15, 12, 13, 17, 16, 19, 18, 21, 20, 23, 22,
    ],
    [
        3, 0, 1, 2, 10, 11, 12, 13, 14, 15, 5, 4, 8, 9, 6, 7, 20, 21, 22, 23, 17, 16, 19, 18,
    ],
    [
        4, 10, 5, 11, 0, 2, 9, 8, 7, 6, 1, 3, 13, 12, 15, 14, 19, 18, 17, 16, 22, 23, 20, 21,
    ],
    [
        5, 11, 4, 10, 2, 0, 7, 6, 9, 8, 3, 1, 15, 14, 13, 12, 18, 19, 16, 17, 23, 22, 21, 20,
    ],
    [
        6, 18, 9, 17, 8, 7, 0, 5, 4, 2, 19, 16, 23, 22, 21, 20, 11, 3, 1, 10, 15, 14, 13, 12,
    ],
    [
        7, 16, 8, 19, 9, 6, 5, 0, 2, 4, 17, 18, 20, 21, 22, 23, 1, 10, 11, 3, 12, 13, 14, 15,
    ],
    [
        8, 19, 7, 16, 6, 9, 2, 4, 5, 0, 18, 17, 22, 23, 20, 21, 10, 1, 3, 11, 13, 12, 15, 14,
    ],
    [
        9, 17, 6, 18, 7, 8, 4, 2, 0, 5, 16, 19, 21, 20, 23, 22, 3, 11, 10, 1, 14, 15, 12, 13,
    ],
    [
        10, 5, 11, 4, 3, 1, 15, 14, 13, 12, 0, 2, 9, 8, 7, 6, 23, 22, 21, 20, 19, 18, 17, 16,
    ],
    [
        11, 4, 10, 5, 1, 3, 13, 12, 15, 14, 2, 0, 7, 6, 9, 8, 22, 23, 20, 21, 18, 19, 16, 17,
    ],
    [
        12, 22, 15, 21, 14, 13, 3, 11, 10, 1, 23, 20, 18, 19, 16, 17, 4, 2, 0, 5, 7, 6, 9, 8,
    ],
    [
        13, 20, 14, 23, 15, 12, 11, 3, 1, 10, 21, 22, 17, 16, 19, 18, 0, 5, 4, 2, 8, 9, 6, 7,
    ],
    [
        14, 23, 13, 20, 12, 15, 1, 10, 11, 3, 22, 21, 19, 18, 17, 16, 5, 0, 2, 4, 9, 8, 7, 6,
    ],
    [
        15, 21, 12, 22, 13, 14, 10, 1, 3, 11, 20, 23, 16, 17, 18, 19, 2, 4, 5, 0, 6, 7, 8, 9,
    ],
    [
        16, 8, 19, 7, 18, 17, 22, 23, 20, 21, 9, 6, 5, 0, 2, 4, 13, 12, 15, 14, 1, 10, 11, 3,
    ],
    [
        17, 6, 18, 9, 19, 16, 23, 22, 21, 20, 7, 8, 4, 2, 0, 5, 15, 14, 13, 12, 3, 11, 10, 1,
    ],
    [
        18, 9, 17, 6, 16, 19, 21, 20, 23, 22, 8, 7, 0, 5, 4, 2, 14, 15, 12, 13, 11, 3, 1, 10,
    ],
    [
        19, 7, 16, 8, 17, 18, 20, 21, 22, 23, 6, 9, 2, 4, 5, 0, 12, 13, 14, 15, 10, 1, 3, 11,
    ],
    [
        20, 14, 23, 13, 22, 21, 19, 18, 17, 16, 15, 12, 11, 3, 1, 10, 9, 8, 7, 6, 0, 5, 4, 2,
    ],
    [
        21, 12, 22, 15, 23, 20, 18, 19, 16, 17, 13, 14, 10, 1, 3, 11, 7, 6, 9, 8, 2, 4, 5, 0,
    ],
    [
        22, 15, 21, 12, 20, 23, 16, 17, 18, 19, 14, 13, 3, 11, 10, 1, 6, 7, 8, 9, 4, 2, 0, 5,
    ],
    [
        23, 13, 20, 14, 21, 22, 17, 16, 19, 18, 12, 15, 1, 10, 11, 3, 8, 9, 6, 7, 5, 0, 2, 4,
    ],
];
const INVERSE: [u8; 24] = [
    0, 3, 2, 1, 4, 5, 6, 7, 9, 8, 10, 11, 18, 16, 17, 19, 13, 14, 12, 15, 20, 23, 22, 21,
];
/// Apply a proper cube rotation to a vector in region-local coordinates.
///
/// Rotation IDs are in `0..24`. Zero is the identity; IDs `0..4` retain the
/// planar quarter-turn convention. Other IDs can exchange vertical/horizontal
/// axes. Components must remain representable when their signs are reversed.
///
/// # Panics
///
/// Panics if `rotation >= 24`. Sign reversal of `i64::MIN` also panics when
/// overflow checks are enabled; callers must exclude that unrepresentable result.
///
/// ```
/// use tor_world::{inverse_rotation, rotate_vector};
/// let rotated = rotate_vector(1, [2, 3, 4]);
/// assert_eq!(rotated, [-3, 2, 4]);
/// assert_eq!(rotate_vector(inverse_rotation(1), rotated), [2, 3, 4]);
/// ```
#[inline]
pub fn rotate_vector(rotation: u8, v: [i64; 3]) -> [i64; 3] {
    if rotation == 0 {
        return v;
    }
    AXES[usize::from(rotation)]
        .map(|axis| v[usize::from(axis.unsigned_abs() - 1)] * i64::from(axis.signum()))
}
/// Compose two rotation IDs, applying `first` and then `second`.
///
/// Both inputs must be in `0..24`. Composition is generally not commutative.
///
/// # Panics
///
/// Invalid IDs may panic when the composition table is indexed. The identity
/// fast paths do not validate IDs and do not make out-of-range inputs supported.
///
/// ```
/// use tor_world::{compose_rotation, rotate_vector};
/// let vector = [2, 3, 4];
/// assert_eq!(rotate_vector(compose_rotation(1, 4), vector), [3, 2, -4]);
/// assert_eq!(
///     rotate_vector(compose_rotation(1, 4), vector),
///     rotate_vector(4, rotate_vector(1, vector)),
/// );
/// assert_ne!(compose_rotation(1, 4), compose_rotation(4, 1));
/// ```
#[inline]
pub fn compose_rotation(first: u8, second: u8) -> u8 {
    if second == 0 {
        return first;
    }
    if first == 0 {
        return second;
    }
    COMPOSE[usize::from(first)][usize::from(second)]
}
/// Return the inverse of a rotation ID in `0..24`.
///
/// # Panics
///
/// Panics if `rotation >= 24`.
#[inline]
pub fn inverse_rotation(rotation: u8) -> u8 {
    INVERSE[usize::from(rotation)]
}
