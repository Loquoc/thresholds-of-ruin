//! Fixed sampling topology shared by fitting and riding-shaft emission.
//! These are numerical coverage settings, not independent artistic knobs.
use std::f32::consts::TAU;

pub(super) const AXIAL_INTERVALS: usize = 48;
pub(super) const ROWS: usize = AXIAL_INTERVALS + 1;
pub(super) const SIDES: usize = 48;

pub(super) fn row_fraction(row: usize) -> f32 {
    row as f32 / AXIAL_INTERVALS as f32
}
pub(super) fn column_angle(column: usize) -> f32 {
    column as f32 * TAU / SIDES as f32
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct CalfGrid([[f32; SIDES]; ROWS]);
impl CalfGrid {
    pub(super) fn from_rows(sample: impl FnMut(usize) -> [f32; SIDES]) -> Self {
        Self(std::array::from_fn(sample))
    }

    pub(super) fn from_fn(mut sample: impl FnMut(usize, usize) -> f32) -> Self {
        Self(std::array::from_fn(|row| {
            std::array::from_fn(|column| sample(row, column))
        }))
    }

    pub(super) fn radius(&self, t: f32, angle: f32) -> f32 {
        let row = t.clamp(0.0, 1.0) * AXIAL_INTERVALS as f32;
        let i = (row.floor() as usize).min(AXIAL_INTERVALS - 1);
        let u = row - i as f32;
        let column = angle.rem_euclid(TAU) / TAU * SIDES as f32;
        let j = (column.floor() as usize).min(SIDES - 1);
        let v = column - j as f32;
        let next = (j + 1) % SIDES;
        let a = self[i][j] + (self[i][next] - self[i][j]) * v;
        let b = self[i + 1][j] + (self[i + 1][next] - self[i + 1][j]) * v;
        a + (b - a) * u
    }

    #[cfg(test)]
    pub(super) fn rows(&self) -> &[[f32; SIDES]; ROWS] {
        &self.0
    }
}
impl std::ops::Index<usize> for CalfGrid {
    type Output = [f32; SIDES];
    fn index(&self, row: usize) -> &Self::Output {
        &self.0[row]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Frozen pre-refactor interpolation, independent of the named topology.
    fn original(grid: &[[f32; 48]; 49], t: f32, angle: f32) -> f32 {
        let row = t.clamp(0.0, 1.0) * 48.0;
        let i = (row.floor() as usize).min(47);
        let u = row - i as f32;
        let column = angle.rem_euclid(TAU) / TAU * 48.0;
        let j = (column.floor() as usize).min(47);
        let v = column - j as f32;
        let next = (j + 1) % 48;
        let a = grid[i][j] + (grid[i][next] - grid[i][j]) * v;
        let b = grid[i + 1][j] + (grid[i + 1][next] - grid[i + 1][j]) * v;
        a + (b - a) * u
    }
    #[test]
    fn topology_and_interpolation_preserve_original_bits() {
        assert_eq!((AXIAL_INTERVALS, ROWS, SIDES), (48, 49, 48));
        assert_eq!(
            std::mem::size_of::<CalfGrid>(),
            std::mem::size_of::<[[f32; 48]; 49]>()
        );
        let grid = CalfGrid::from_fn(|r, c| ((r * 37 + c * 19) as f32 * 0.001).sin());
        for row in 0..49 {
            assert_eq!(row_fraction(row).to_bits(), (row as f32 / 48.0).to_bits());
            for col in 0..48 {
                assert_eq!(
                    column_angle(col).to_bits(),
                    (col as f32 * TAU / 48.0).to_bits()
                );
                for offset in [-0.000001, 0.0, 0.000001, 0.37] {
                    let t = (row as f32 + offset) / 48.0;
                    let a = (col as f32 + offset) * TAU / 48.0;
                    for angle in [a, a - TAU, a + TAU] {
                        assert_eq!(
                            grid.radius(t, angle).to_bits(),
                            original(grid.rows(), t, angle).to_bits()
                        );
                    }
                }
            }
        }
        for t in [-1.0, -0.0, 0.0, 1.0, 2.0] {
            for a in [-TAU, -0.0, 0.0, TAU, 2.0 * TAU] {
                assert_eq!(
                    grid.radius(t, a).to_bits(),
                    original(grid.rows(), t, a).to_bits()
                );
            }
        }
    }
}
