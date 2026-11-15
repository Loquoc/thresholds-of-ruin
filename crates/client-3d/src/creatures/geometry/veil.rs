//! A bordered veil is an independent head attachment, not part of a crown.
//! One shared sampled sheet supplies cloth, scalloped edging and its ornaments.
use super::*;
use crate::creatures::outfit::Veil;

const ROWS: usize = 24;
const COLS: usize = 32;

/// Resolve the neutral head attachment envelope independently of the crown.
/// The trailing sheet is torso-local: turning the head cannot whip the train.
/// Its roots use the neutral head frame; this is not a simulated veil and the
/// small relative motion of crown/root attachment still needs visual review.
pub(super) fn emit(p: &mut Parts, body: HumanoidBody, recipe: Veil) {
    let roots = attachment_roots(body.head);
    let grid = sheet(&roots, body.shoulders, recipe);
    for row in 0..ROWS {
        for column in 0..COLS {
            // Sheer material is already double-sided. No coincident reverse
            // triangles or thickness faces: they make translucent seams dark.
            let q = [
                grid[row][column],
                grid[row][column + 1],
                grid[row + 1][column + 1],
                grid[row + 1][column],
            ];
            p.sheer.tri([q[0], q[1], q[2]], recipe.color);
            p.sheer.tri([q[0], q[2], q[3]], recipe.color);
        }
    }
    let mut border = Vec::with_capacity(ROWS * 2 + COLS + 1);
    border.extend((0..=ROWS).map(|row| grid[row][0]));
    border.extend((1..=COLS).map(|column| grid[ROWS][column]));
    border.extend((0..ROWS).rev().map(|row| grid[row][COLS]));
    tube(
        &mut p.metal,
        &border,
        &vec![(0.0017, 0.0017); border.len()],
        recipe.trim,
    );
    // Sparse flower settings follow the very same border, rather than a second
    // independent curve that could float away when length/spread changes.
    for index in (4..border.len() - 4).step_by(8) {
        let at = border[index];
        let tangent = (border[index + 1] - border[index - 1]).normalize_or(Vec3::Y);
        let across = tangent.cross(Vec3::Z).normalize_or(Vec3::X);
        for petal in 0..5 {
            let angle = petal as f32 * TAU / 5.0;
            let direction = tangent * angle.cos() + across * angle.sin();
            p.metal.blob(
                at + direction * 0.006,
                Vec3::splat(0.0038),
                1,
                0.0,
                recipe.trim,
            );
        }
        p.satin.blob(
            at - Vec3::Z * 0.002,
            Vec3::new(0.0035, 0.0045, 0.0025),
            1,
            0.0,
            recipe.gem,
        );
    }
}

/// Neutral head attachment frame, deliberately independent of rendered crown
/// geometry. The ordinary scalp envelope includes a 35 mm hair allowance;
/// ornate hairstyles still require visual review rather than an expensive
/// second head mesh construction for every torso.
fn attachment_roots(head_origin: Vec3) -> Vec<Vec3> {
    let head_scale = storybook_part(Plan::Biped, J::Head);
    let torso_scale = storybook_part(Plan::Biped, J::Torso);
    (0..=COLS)
        .map(|column| {
            let u = column as f32 / COLS as f32 * 2.0 - 1.0;
            let angle = PI - u * PI * 0.42;
            let local = Vec3::new(
                angle.sin() * 0.145,
                0.247 - 0.006 * angle.cos(),
                -0.012 + angle.cos() * 0.155,
            );
            (local * head_scale + head_origin) / torso_scale
        })
        .collect()
}

fn sheet(roots: &[Vec3], shoulders: f32, recipe: Veil) -> Vec<Vec<Vec3>> {
    (0..=ROWS)
        .map(|row| {
            let t = row as f32 / ROWS as f32;
            (0..=COLS)
                .map(|column| {
                    let u = column as f32 / COLS as f32 * 2.0 - 1.0;
                    let root = roots[column];
                    let release = (t * 5.0).min(1.0);
                    let flare = shoulders * recipe.spread * t.powf(0.8);
                    let folds = 0.012 * (u * PI * 6.0).cos() * t;
                    let scallop = 0.007 * (t * PI * 8.0).sin().powi(2) * u.abs().powi(8);
                    Vec3::new(
                        root.x + u * (flare + scallop),
                        root.y - recipe.length * t + 0.018 * (u * PI * 8.0).cos() * t.powi(12),
                        root.z - release * 0.10 - t * recipe.length * 0.20 + folds,
                    )
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn recipe() -> Veil {
        Veil {
            color: Vec3::ONE,
            trim: GILT,
            gem: Vec3::Z,
            length: 0.85,
            spread: 1.0,
        }
    }
    fn roots() -> Vec<Vec3> {
        (0..=COLS)
            .map(|k| Vec3::new((k as f32 / COLS as f32 - 0.5) * 0.2, 0.25, -0.12))
            .collect()
    }
    #[test]
    fn veil_roots_translate_through_torso_presentation_frame() {
        let a = attachment_roots(Vec3::ZERO);
        let offset = Vec3::new(0.0, 0.4, 0.02);
        let b = attachment_roots(offset);
        let expected = offset / storybook_part(Plan::Biped, J::Torso);
        for (a, b) in a.iter().zip(b) {
            assert!((b - *a - expected).length() < 1e-6);
            assert!(b.is_finite());
        }
    }
    #[test]
    fn veil_preserves_attachment_row_and_parameterized_drape() {
        let roots = roots();
        for length in [0.85, 1.55, 1.80] {
            for shoulders in [0.85, 1.0, 1.12] {
                let grid = sheet(&roots, shoulders, Veil { length, ..recipe() });
                assert_eq!(grid[0], roots);
                assert!(grid.iter().flatten().all(|v| v.is_finite()));
                assert!(grid[ROWS][COLS].x > roots[COLS].x + shoulders * 0.9);
                assert!((grid[ROWS][COLS / 2].y - (0.25 - length)).abs() <= 0.019);
                assert!(grid[ROWS][COLS / 2].z < -0.20);
            }
        }
    }
    #[test]
    fn veil_cells_are_non_degenerate_single_surface() {
        let grid = sheet(&roots(), 0.2, recipe());
        for row in 0..ROWS {
            for col in 0..COLS {
                let a = grid[row][col];
                let b = grid[row][col + 1];
                let c = grid[row + 1][col];
                assert!((b - a).cross(c - a).length() > 1e-7);
            }
        }
    }
}
