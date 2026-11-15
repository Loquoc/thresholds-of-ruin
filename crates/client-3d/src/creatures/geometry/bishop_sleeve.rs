//! Detached gathered sleeves: one translucent sheet, fitted in each arm frame.
//! Authored fullness is independent of anatomy; cuffs and cloth use one grid.
use super::*;
use crate::sdf::Sculpt;

const AROUND: usize = 32;
const ROWS: usize = 12;

#[derive(Clone, Copy)]
struct GatheredSegment {
    start: f32,
    end: f32,
    start_radius: f32,
    end_radius: f32,
    billow: f32,
}

impl GatheredSegment {
    fn upper() -> Self {
        Self {
            start: -0.085,
            end: -0.30,
            start_radius: 0.064,
            end_radius: 0.060,
            billow: 0.021,
        }
    }

    fn lower() -> Self {
        Self {
            start: 0.0,
            end: -0.25,
            start_radius: 0.060,
            end_radius: 0.039,
            billow: 0.043,
        }
    }

    fn rings(self, support: &Sculpt, build: f32, length: f32) -> Vec<Vec<Vec3>> {
        (0..=ROWS)
            .map(|row| {
                let t = row as f32 / ROWS as f32;
                let y = (self.start + (self.end - self.start) * t) * length;
                (0..=AROUND)
                    .map(|k| {
                        let a = k as f32 * TAU / AROUND as f32;
                        let out = Vec3::new(a.sin(), 0.0, a.cos());
                        let fullness = (PI * t).sin().max(0.0);
                        let gathered = (a * 8.0).cos() * 0.004 * fullness;
                        let radius = (self.start_radius
                            + (self.end_radius - self.start_radius) * t
                            + self.billow * fullness
                            + gathered)
                            * build;
                        let origin = Vec3::Y * y;
                        let fitted =
                            surface_ray_hit(support, origin, out, 0.004 * build, 0.3 * build)
                                .map_or(radius, |at| (at - origin).dot(out).max(radius));
                        origin + out * fitted
                    })
                    .collect()
            })
            .collect()
    }
}

fn emit(p: &mut Parts, rings: &[Vec<Vec3>], color: Vec3) {
    for pair in rings.windows(2) {
        for k in 0..AROUND {
            let q = [pair[0][k], pair[0][k + 1], pair[1][k + 1], pair[1][k]];
            // The sheer material is already double-sided. No coincident back face.
            p.sheer.quad_out(q, Vec3::Y * q[0].y, color);
        }
    }
}

fn cuff(p: &mut Parts, ring: &[Vec3], color: Vec3, trim: Vec3, build: f32) {
    let radii = vec![(0.004 * build, 0.004 * build); ring.len()];
    tube(&mut p.satin, ring, &radii, color);
    for offset in [-0.004, 0.004] {
        let path: Vec<_> = ring
            .iter()
            .map(|at| *at + Vec3::Y * offset * build)
            .collect();
        tube(
            &mut p.metal,
            &path,
            &vec![(0.0015 * build, 0.0015 * build); path.len()],
            trim,
        );
    }
}

pub(super) fn upper(p: &mut Parts, body: HumanoidBody, side: f32, color: Vec3, trim: Vec3) {
    let support = upper_arm_form(side, body.build, body.arm_length, body.profile.muscle, 0.94);
    let rings = GatheredSegment::upper().rings(&support, body.build, body.arm_length);
    emit(p, &rings, color);
    cuff(p, &rings[0], color, trim, body.build);
}

pub(super) fn lower(p: &mut Parts, body: HumanoidBody, _side: f32, color: Vec3, trim: Vec3) {
    let support = forearm_form(body.build, body.arm_length, body.profile.muscle);
    let rings = GatheredSegment::lower().rings(&support, body.build, body.arm_length);
    emit(p, &rings, color);
    cuff(p, &rings[ROWS], color, trim, body.build);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gathered_sheets_clear_support_and_retain_detached_and_gathered_ends() {
        for build in [0.8, 1.0, 1.25] {
            for muscle in [0.94, 1.08] {
                for side in [-1.0, 1.0] {
                    let upper_support = upper_arm_form(side, build, 1.0, muscle, 0.94);
                    let lower_support = forearm_form(build, 1.0, muscle);
                    let upper = GatheredSegment::upper().rings(&upper_support, build, 1.0);
                    let lower = GatheredSegment::lower().rings(&lower_support, build, 1.0);
                    assert!((upper[0][0].y + 0.085).abs() < 1e-6);
                    for (grid, support) in [(&upper, &upper_support), (&lower, &lower_support)] {
                        assert!(grid
                            .iter()
                            .flatten()
                            .all(|p| p.is_finite() && support.distance(*p) > 0.002 * build));
                    }
                    for k in 0..=AROUND {
                        assert!((upper[ROWS][k] + Vec3::Y * 0.30).distance(lower[0][k]) < 1e-5);
                    }
                    assert!(lower[ROWS / 2][0].z > lower[ROWS][0].z * 1.5);
                }
            }
        }
    }

    #[test]
    fn translucent_sheet_is_single_surface_not_overlaid_reverse_triangles() {
        let support = forearm_form(1.0, 1.0, 1.0);
        let rings = GatheredSegment::lower().rings(&support, 1.0, 1.0);
        let mut p = Parts::default();
        emit(&mut p, &rings, Vec3::ONE);
        let mut unique = std::collections::BTreeSet::new();
        for triangle in p.sheer.surface_triangles() {
            let mut key = triangle.map(|v| v.to_array().map(f32::to_bits));
            key.sort();
            assert!(unique.insert(key));
        }
        assert_eq!(unique.len(), ROWS * AROUND * 2);
        assert_eq!(
            p.matte.surface_triangles().count(),
            0,
            "skin remains caller-owned"
        );
    }
}
