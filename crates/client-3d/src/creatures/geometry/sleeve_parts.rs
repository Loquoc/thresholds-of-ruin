//! Gathered sleeve panels over continuous lining, independent of outfit identity.
use super::*;

const PANELS: usize = 8;
const ROWS: usize = 12;
const COLUMNS: usize = 6;

/// Longer upper-arm puff: gathered at each end, full between the gathers.
fn point(t: f32, angle: f32, side: f32, build: f32, length: f32, lift: f32) -> Vec3 {
    let fullness = (PI * t).sin().max(0.0);
    let radius = (0.050 + 0.042 * fullness + lift) * build;
    Vec3::new(
        side * 0.008 * build + angle.sin() * radius,
        (0.018 - 0.208 * t) * length,
        angle.cos() * radius,
    )
}

pub(super) fn slashed_puff(
    b: &mut Builder,
    side: f32,
    build: f32,
    length: f32,
    color: Vec3,
    lining: Vec3,
) {
    let center = |y| Vec3::new(side * 0.008 * build, y, 0.0);
    // Continuous lining keeps every opening covered, including at oblique views.
    for row in 0..ROWS {
        let t0 = row as f32 / ROWS as f32;
        let t1 = (row + 1) as f32 / ROWS as f32;
        for k in 0..PANELS * COLUMNS {
            let a0 = k as f32 * TAU / (PANELS * COLUMNS) as f32;
            let a1 = (k + 1) as f32 * TAU / (PANELS * COLUMNS) as f32;
            let q = [
                point(t0, a0, side, build, length, 0.0),
                point(t0, a1, side, build, length, 0.0),
                point(t1, a1, side, build, length, 0.0),
                point(t1, a0, side, build, length, 0.0),
            ];
            b.quad_out(q, center(q[0].y), lining);
        }
    }
    // Each strip narrows between its gathered ends to expose the contrasting
    // lining. Raised centers give real folds rather than painted stripes.
    for panel in 0..PANELS {
        let strip = |t: f32, x: f32, inner: bool| {
            let half_width = 0.5 - 0.21 * (PI * t).sin().max(0.0);
            let angle = (panel as f32 + x * half_width) * TAU / PANELS as f32;
            let lift = 0.003 + 0.005 * (PI * t).sin() * (x * PI * 0.5).cos()
                - if inner { 0.002 } else { 0.0 };
            point(t, angle, side, build, length, lift)
        };
        for row in 0..ROWS {
            let t0 = row as f32 / ROWS as f32;
            let t1 = (row + 1) as f32 / ROWS as f32;
            for col in 0..COLUMNS {
                let x0 = 2.0 * col as f32 / COLUMNS as f32 - 1.0;
                let x1 = 2.0 * (col + 1) as f32 / COLUMNS as f32 - 1.0;
                let q = [
                    strip(t0, x0, false),
                    strip(t0, x1, false),
                    strip(t1, x1, false),
                    strip(t1, x0, false),
                ];
                b.quad_out(q, center(q[0].y), color);
            }
            for x in [-1.0, 1.0] {
                let q = [
                    strip(t0, x, false),
                    strip(t1, x, false),
                    strip(t1, x, true),
                    strip(t0, x, true),
                ];
                let middle = strip((t0 + t1) * 0.5, 0.0, false);
                b.quad_out(q, middle, color * 0.85);
            }
        }
    }
    // The gathered top continues over the shoulder as a rounded cloth cap.
    // Leaving this ring open exposes the background above the arm's own mass.
    let cap = |row: usize, column: usize| {
        let theta = row as f32 * PI / 12.0;
        let angle = column as f32 * TAU / 48.0;
        Vec3::new(
            (side * 0.008 + angle.sin() * 0.053 * theta.sin()) * build,
            (0.018 + 0.035 * theta.cos()) * length,
            angle.cos() * 0.053 * theta.sin() * build,
        )
    };
    let inside = Vec3::new(side * 0.008 * build, 0.018 * length, 0.0);
    for column in 0..48 {
        b.tri_out(
            [cap(0, column), cap(1, column), cap(1, column + 1)],
            inside,
            color,
        );
        for row in 1..6 {
            b.quad_out(
                [
                    cap(row, column),
                    cap(row, column + 1),
                    cap(row + 1, column + 1),
                    cap(row + 1, column),
                ],
                inside,
                color,
            );
        }
    }
    // Narrow gathered cuff closes the lower end without a bulky torus.
    {
        let t = 1.0;
        let path: Vec<_> = (0..=48)
            .map(|k| point(t, k as f32 * TAU / 48.0, side, build, length, 0.003))
            .collect();
        tube(
            b,
            &path,
            &vec![(0.003 * build, 0.003 * build); path.len()],
            color,
        );
    }
}

/// A thin cloth shell with an open lumen and folds growing toward the wrist.
pub(super) fn bell(
    b: &mut Builder,
    profile: &[(f32, Vec2); 5],
    build: f32,
    length: f32,
    color: Vec3,
    trim: Vec3,
) {
    const ROWS: usize = 24;
    const SIDES: usize = 48;
    let point = |row: usize, column: usize, inner: bool| {
        let t = row as f32 / ROWS as f32;
        let height = profile[0].0 + t * (profile[4].0 - profile[0].0);
        let station = (0..4).find(|&i| height <= profile[i + 1].0).unwrap_or(3);
        let (h0, r0) = profile[station];
        let (h1, r1) = profile[station + 1];
        let radius = r0.lerp(r1, ease((height - h0) / (h1 - h0)));
        let angle = column as f32 * TAU / SIDES as f32;
        let wave = (8.0 * angle).cos();
        let fold = 0.008 * build * ease(t) * (0.5 + 0.5 * wave);
        let radius = radius + Vec2::splat(fold - if inner { 0.0025 * build } else { 0.0 });
        Vec3::new(
            angle.sin() * radius.x,
            (-height + 0.006 * t.powi(3) * wave) * length,
            angle.cos() * radius.y,
        )
    };
    for row in 0..ROWS {
        for column in 0..SIDES {
            for inner in [false, true] {
                let q = [
                    point(row, column, inner),
                    point(row, column + 1, inner),
                    point(row + 1, column + 1, inner),
                    point(row + 1, column, inner),
                ];
                let middle = q.iter().copied().sum::<Vec3>() * 0.25;
                let center = Vec3::Y * middle.y;
                let inside = if inner {
                    center + (middle - center) * 2.0
                } else {
                    center
                };
                let tint = if row == ROWS - 1 {
                    trim
                } else {
                    color * crate::sdf::Sculpt::weave(middle)
                };
                b.quad_out(q, inside, tint * if inner { 0.85 } else { 1.0 });
            }
        }
    }
    // Annular edges join the two surfaces without filling the sleeve opening.
    for row in [0, ROWS] {
        for column in 0..SIDES {
            let q = [
                point(row, column, false),
                point(row, column + 1, false),
                point(row, column + 1, true),
                point(row, column, true),
            ];
            let middle = q.iter().copied().sum::<Vec3>() * 0.25;
            let inside = middle + Vec3::Y * if row == 0 { -0.01 } else { 0.01 };
            b.quad_out(q, inside, trim);
        }
    }
}

/// Compression folds run across the inner crook, leaving the outer elbow smooth.
/// Each fold follows the eased support rather than swelling the whole section.
pub(super) fn gathers(
    mut form: crate::sdf::Sculpt,
    build: f32,
    length: f32,
    heights: &[f32],
) -> crate::sdf::Sculpt {
    use crate::sdf::Shape::Capsule;
    let support = form.clone();
    for (row, &height) in heights.iter().enumerate() {
        let points: [Vec3; 5] = std::array::from_fn(|i| {
            let u = i as f32 / 2.0 - 1.0;
            let y = height * length + u * 0.005 * build * if row % 2 == 0 { 1.0 } else { -1.0 };
            let mut surface =
                support.onto(Vec3::new(u * 0.028 * build, y, 0.2 * build), Vec3::NEG_Z);
            // The general attachment tolerance is large beside a shallow fold.
            // Refine on the same ray so its seating error does not become relief.
            for _ in 0..16 {
                let distance = support.distance(surface);
                if distance.abs() < 0.00001 * build {
                    break;
                }
                surface -= Vec3::Z * distance.clamp(-0.002 * build, 0.002 * build);
            }
            surface - Vec3::Z * 0.005 * build
        });
        let mut fold = crate::sdf::Sculpt::default();
        for i in 0..4 {
            let radius = |k: usize| {
                if k == 0 || k == 4 {
                    0.0035 * build
                } else {
                    0.007 * build
                }
            };
            // A tiny join rounds the path's internal seams without accumulating
            // the sleeve blend at every shared endpoint (which makes beads).
            fold.add(
                Capsule(points[i], points[i + 1], radius(i), radius(i + 1)),
                0.0005 * build,
            );
        }
        form.union(fold, 0.012 * build);
    }
    form
}

#[cfg(test)]
mod gather_tests {
    use super::*;
    use crate::sdf::{Sculpt, Shape};

    // Independent ray intersection, rather than the seating helper under test.
    fn front_surface(form: &Sculpt, x: f32, y: f32, build: f32) -> f32 {
        let (mut lo, mut hi) = (-0.02 * build, 0.1 * build);
        assert!(form.distance(Vec3::new(x, y, lo)) < 0.0);
        assert!(form.distance(Vec3::new(x, y, hi)) > 0.0);
        for _ in 0..24 {
            let z = (lo + hi) * 0.5;
            if form.distance(Vec3::new(x, y, z)) > 0.0 {
                hi = z;
            } else {
                lo = z;
            }
        }
        (lo + hi) * 0.5
    }

    #[test]
    fn compression_fold_crest_has_relief_without_segment_join_beads() {
        for build in [0.65, 1.0, 1.4] {
            let mut support = Sculpt::default();
            support.add(Shape::Beyond(Vec3::NEG_Z, 0.0), 0.0);
            let folded = gathers(support, build, 1.0, &[-0.025]);
            // The broad center should read as one ridge, not pearls at the
            // capsule joins. 4 mm relief leaves a small smoothing allowance.
            for sample in -14..=14 {
                let x = sample as f32 * 0.001 * build;
                let y = -0.025 + x * (0.005 / 0.028);
                let relief = front_surface(&folded, x, y, build) / build;
                assert!(
                    (0.0035..=0.0044).contains(&relief),
                    "build={build} x={x}: crest relief {relief}"
                );
            }
        }
    }

    #[test]
    fn compression_folds_keep_curved_support_and_outer_elbow() {
        for build in [0.65, 1.0, 1.4] {
            for length in [0.8, 1.2] {
                let mut support = Sculpt::default();
                support.add(
                    Shape::Capsule(
                        Vec3::new(0.0, -0.3 * length, 0.0),
                        Vec3::new(0.0, 0.05 * length, 0.0),
                        0.045 * build,
                        0.045 * build,
                    ),
                    0.0,
                );
                let folded = gathers(support.clone(), build, length, &[-0.025, -0.075]);
                for height in [-0.025, -0.075] {
                    let y = height * length;
                    let relief = front_surface(&folded, 0.0, y, build) / build - 0.045;
                    assert!((0.0035..=0.0044).contains(&relief), "relief={relief}");
                    for offset in [-0.002, 0.0, 0.002] {
                        let outer = Vec3::new(0.0, y, (-0.045 + offset) * build);
                        assert!(
                            (folded.distance(outer) - support.distance(outer)).abs() < 1e-6,
                            "outer elbow changed: build={build}, length={length}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn compression_fold_has_a_broad_flank_without_a_wide_pad() {
        for build in [0.65, 1.0, 1.4] {
            let mut support = Sculpt::default();
            support.add(Shape::Beyond(Vec3::NEG_Z, 0.0), 0.0);
            let folded = gathers(support, build, 1.0, &[-0.025]);
            // Between internal joins, measure the transverse cloth profile.
            // A 4 mm crest needs a broad flank, not a cord-like narrow ridge:
            // retain half its height at 7 mm either side, but no raised pad
            // 25 mm away. These dimensions scale with the wearer's build.
            let x = 0.007 * build;
            let y = -0.025 + x * (0.005 / 0.028);
            let crest = front_surface(&folded, x, y, build);
            assert!((0.0035..=0.0044).contains(&(crest / build)));
            for side in [-1.0, 1.0] {
                let flank = front_surface(&folded, x, y + side * 0.007 * build, build);
                assert!(
                    flank >= crest * 0.5,
                    "build={build} side={side}: 7 mm flank={} mm, crest={} mm",
                    flank / build * 1000.0,
                    crest / build * 1000.0,
                );
                let distant = front_surface(&folded, x, y + side * 0.025 * build, build);
                assert!(
                    distant.abs() <= 0.00001 * build,
                    "build={build} side={side}: fold extends into distant cloth"
                );
            }
        }
    }
}
