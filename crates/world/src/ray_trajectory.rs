//! Integer cell-centre rasterization for bounded, arbitrary-slope flights.

use crate::rotate_vector;

/// A ray's direction and sub-cell phase, independent of world geometry.
///
/// Steps use nearest-cell rounding (half ties away from the starting centre).
/// A caller probes a clone, committing it only when the cell is accepted. A
/// reflection changes the original direction, not the unaccepted probe's phase.
/// Geometry remains responsible for determining actual blocking face normals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RayTrajectory {
    vector: [i64; 3],
    phase: [i64; 3],
    denominator: i64,
}

impl RayTrajectory {
    /// Construct a nonzero integer bearing. Its length is not a flight range.
    pub fn new(offset: [i32; 3]) -> Option<Self> {
        let vector = offset.map(i64::from);
        let denominator = vector.iter().map(|n| n.abs()).max()?;
        (denominator != 0).then_some(Self {
            vector,
            phase: [denominator / 2; 3],
            denominator,
        })
    }

    /// Advance one major-axis cell. Every component is -1, 0 or 1 and at least
    /// one component is nonzero. Range accounting belongs to the flight owner.
    pub fn advance(&mut self) -> [i32; 3] {
        std::array::from_fn(|axis| {
            self.phase[axis] += self.vector[axis].abs();
            if self.phase[axis] >= self.denominator {
                self.phase[axis] -= self.denominator;
                self.vector[axis].signum() as i32
            } else {
                0
            }
        })
    }

    /// Reflect only components normal to known blocking axis-aligned faces.
    /// Multiple faces at an exact corner reflect their union deterministically.
    /// An all-false mask does nothing; missing geometry must not invent a face.
    pub fn reflect(&mut self, normal_axes: [bool; 3]) {
        for (component, normal) in self.vector.iter_mut().zip(normal_axes) {
            if normal {
                *component = -*component;
            }
        }
    }

    /// Carry the direction and raster phase through a physical cube-frame join.
    pub fn rotate(&mut self, rotation: u8) {
        self.vector = rotate_vector(rotation, self.vector);
        self.phase = rotate_vector(rotation, self.phase).map(i64::abs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_cell_is_reached_in_major_axis_steps() {
        for x in -7i32..=7 {
            for y in -7i32..=7 {
                for z in -7i32..=7 {
                    let offset = [x, y, z];
                    let Some(mut ray) = RayTrajectory::new(offset) else {
                        assert_eq!(offset, [0; 3]);
                        continue;
                    };
                    let mut at = [0; 3];
                    for _ in 0..x.abs().max(y.abs()).max(z.abs()) {
                        let step = ray.advance();
                        assert_ne!(step, [0; 3]);
                        for axis in 0..3 {
                            assert!((-1..=1).contains(&step[axis]));
                            at[axis] += step[axis];
                        }
                    }
                    assert_eq!(at, offset);
                }
            }
        }
    }

    #[test]
    fn wall_and_ceiling_reflection_preserve_tangents_and_phase() {
        let mut ray = RayTrajectory::new([4, 2, 1]).unwrap();
        assert_eq!(ray.advance(), [1, 1, 0]);
        ray.reflect([true, false, true]);
        assert_eq!(ray.advance(), [-1, 0, -1]);
        assert_eq!(ray.advance(), [-1, 1, 0]);
        assert_eq!(ray.advance(), [-1, 0, 0]);
    }

    #[test]
    fn each_cube_rotation_commutes_with_every_future_step() {
        for rotation in 0..24 {
            let mut original = RayTrajectory::new([7, -4, 2]).unwrap();
            original.advance();
            original.advance();
            let mut rotated = original;
            rotated.rotate(rotation);
            for _ in 0..30 {
                let expected = rotate_vector(rotation, original.advance().map(i64::from));
                assert_eq!(rotated.advance().map(i64::from), expected);
            }
        }
    }

    #[test]
    fn sign_reversal_mirrors_the_whole_path_including_half_ties() {
        for offset in [[4, 2, 1], [7, 4, 2], [1, 1, 1], [0, 3, 2]] {
            let mut forward = RayTrajectory::new(offset).unwrap();
            let mut reverse = RayTrajectory::new(offset.map(|n| -n)).unwrap();
            for _ in 0..30 {
                assert_eq!(forward.advance().map(|n| -n), reverse.advance());
            }
        }
    }

    #[test]
    fn rejecting_a_probe_and_reflecting_does_not_spend_raster_phase() {
        let mut ray = RayTrajectory::new([4, 2, 1]).unwrap();
        assert_eq!(ray.advance(), [1, 1, 0]);
        let mut probe = ray;
        assert_eq!(probe.advance(), [1, 0, 1]);
        ray.reflect([true, false, false]);
        assert_eq!(ray.advance(), [-1, 0, 1]);
    }

    #[test]
    fn extreme_offsets_and_reflection_do_not_overflow() {
        let mut ray = RayTrajectory::new([i32::MIN, i32::MAX, 1]).unwrap();
        for _ in 0..100 {
            assert_eq!(ray.advance(), [-1, 1, 0]);
        }
        ray.reflect([true; 3]);
        assert_eq!(ray.advance(), [1, -1, 0]);
    }
}
