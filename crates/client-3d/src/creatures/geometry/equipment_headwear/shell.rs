//! Reusable head-local shell recipes. Assembly owns silhouettes and palettes;
//! this module owns periodic rim topology and open-bottom lined crown emission.
//! Coordinates retain the authored cranium frame; fitting stays with the caller.
use super::{Builder, Vec3, BROW, TAU};

pub(super) fn ring_point(angle: f32, y: f32, radius: f32) -> Vec3 {
    Vec3::new(angle.sin() * radius, y, angle.cos() * radius * 1.04 - 0.012)
}

pub(super) fn rim(b: &mut Builder, y: f32, radius: f32, width: f32, color: Vec3) {
    // This is a closed ellipse, not an open tube with two coincident caps.
    // Periodic frames share the exact seam positions and normals, avoiding the
    // visible front notch caused by mismatched endpoint tangents and cap disks.
    const AROUND: usize = 64;
    const SIDES: usize = 12;
    let vertex = |i: usize, j: usize| {
        let a = (i % AROUND) as f32 * TAU / AROUND as f32;
        let cross = (j % SIDES) as f32 * TAU / SIDES as f32;
        let outward = Vec3::new(a.sin() * 1.04, 0.0, a.cos()).normalize();
        let normal = outward * cross.cos() + Vec3::Y * cross.sin();
        (ring_point(a, y, radius) + normal * width, normal)
    };
    // Each periodic vertex belongs to both adjacent quads. Resolve one ring
    // at a time and reuse the first ring at the seam, keeping the original
    // arithmetic and triangle order without a heap-backed mesh cache.
    let first: [_; SIDES] = std::array::from_fn(|j| vertex(0, j));
    let mut current = first;
    for i in 0..AROUND {
        let next = if i + 1 == AROUND {
            first
        } else {
            std::array::from_fn(|j| vertex(i + 1, j))
        };
        for j in 0..SIDES {
            let q = [
                current[j],
                next[j],
                next[(j + 1) % SIDES],
                current[(j + 1) % SIDES],
            ];
            for ids in [[0, 1, 2], [0, 2, 3]] {
                b.tri_smooth(ids.map(|k| q[k].0), ids.map(|k| q[k].1), [color; 3]);
            }
        }
        current = next;
    }
}

/// Open-bottom, lined crown: no solid sphere occupies the wearer's forehead.
/// Alternating triangular planes deliberately expose crystal's faceting.
/// Named authored parameters shared by dome helmets and the dented pot.
/// Profile values are head-local (height, radius), before seating/fitting.
/// `faceted` retains the existing unlined, alternating-pigment shell option.
#[derive(Clone, Copy)]
pub(super) struct CrownRecipe<'a> {
    pub(super) profile: &'a [(f32, f32)],
    pub(super) facets: usize,
    pub(super) color: Vec3,
    pub(super) faceted: bool,
    pub(super) dented: bool,
}
impl CrownRecipe<'_> {
    pub(super) fn emit(self, b: &mut Builder) {
        let Self {
            profile,
            facets,
            color,
            faceted: crystal,
            dented: dent,
        } = self;
        let point = |row: usize, column: usize, inner: bool| {
            let (y, radius) = profile[row];
            let a = column as f32 * TAU / facets as f32;
            let damage = if dent && row > 0 {
                0.018 * ((a - 0.8).cos().max(0.0)).powi(12)
            } else {
                0.0
            };
            ring_point(
                a,
                y - damage * 0.5,
                radius - damage - if inner { 0.006 } else { 0.0 },
            )
        };
        for row in 0..profile.len() - 1 {
            for col in 0..facets {
                for inner in [false, true] {
                    if crystal && inner {
                        continue;
                    }
                    let q = [
                        point(row, col, inner),
                        point(row, col + 1, inner),
                        point(row + 1, col + 1, inner),
                        point(row + 1, col, inner),
                    ];
                    let middle = (q[0] + q[2]) * 0.5;
                    let reference = if inner {
                        middle + Vec3::new(middle.x, 0.0, middle.z + 0.012)
                    } else {
                        Vec3::new(0.0, middle.y, -0.012)
                    };
                    let pigment = color
                        * if inner {
                            0.68
                        } else if crystal {
                            0.75 + ((col * 3 + row) % 5) as f32 * 0.09
                        } else {
                            1.0
                        };
                    b.quad_out(q, reference, pigment);
                }
            }
        }
        for col in 0..facets {
            b.quad_out(
                [
                    point(0, col, false),
                    point(0, col, true),
                    point(0, col + 1, true),
                    point(0, col + 1, false),
                ],
                Vec3::Y * (BROW + 0.02),
                color,
            );
            let row = profile.len() - 1;
            let center = Vec3::new(0.0, profile[row].0, -0.012);
            b.tri_out(
                [point(row, col, false), point(row, col + 1, false), center],
                center - Vec3::Y,
                color,
            );
            if !crystal {
                b.tri_out(
                    [point(row, col, true), center, point(row, col + 1, true)],
                    center + Vec3::Y,
                    color * 0.68,
                );
            }
        }
    }
}
