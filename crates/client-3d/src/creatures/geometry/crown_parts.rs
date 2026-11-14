//! Shared open-bottom, lined crown in the anatomical head frame.
//! This is a construction profile, not a hair or arbitrary-face collision solver.
use super::super::anatomy::HumanoidCranium;
use super::*;

pub(super) const VEILED_RIM_RADIUS: f32 = 0.006;

#[derive(Clone, Copy)]
pub(super) struct CrownProfile {
    pub cranium: HumanoidCranium,
    pub ease: Vec3,
    pub wall: f32,
    pub opening: f32,
    pub around: usize,
    pub rows: usize,
}

impl CrownProfile {
    pub fn winged() -> Self {
        let cranium = HumanoidCranium::STANDARD;
        Self {
            cranium,
            ease: Vec3::new(0.009, 0.016, 0.009),
            wall: 0.005,
            opening: cranium.brow_height(),
            around: 32,
            rows: 6,
        }
    }

    /// A shallow cloth cap above the brow; its opening and volume also own
    /// the hair envelope and veil mount.
    pub fn veiled() -> Self {
        Self {
            cranium: HumanoidCranium::STANDARD,
            ease: Vec3::new(0.018, 0.024, 0.018),
            wall: 0.004,
            opening: 0.240,
            around: 32,
            rows: 6,
        }
    }

    /// Rounded tricorn crown. Its authored roof height is retained while the
    /// opening, lining and hair volume share the anatomical skull datum.
    pub fn tricorn() -> Self {
        Self {
            cranium: HumanoidCranium::STANDARD,
            ease: Vec3::new(0.040, 0.080, 0.040),
            wall: 0.005,
            opening: 0.255,
            around: 32,
            rows: 6,
        }
    }

    /// Conservative upper-hair volume inside the bent witch cone. The same
    /// anatomical opening seats its cone mouth and annular brim; the taller
    /// authored point is a style feature, not room to stretch the hairstyle.
    pub fn witch_hat() -> Self {
        Self {
            cranium: HumanoidCranium::STANDARD,
            ease: Vec3::new(0.050, 0.070, 0.050),
            wall: 0.005,
            opening: 0.255,
            around: 32,
            rows: 6,
        }
    }

    pub fn opening_radii(self) -> Vec2 {
        let radii = self.cranium.radii + self.ease;
        let section = (1.0 - ((self.opening - self.cranium.center.y) / radii.y).powi(2)).sqrt();
        Vec2::new(radii.x, radii.z) * section
    }

    fn valid(self) -> bool {
        self.cranium.center.is_finite()
            && self.cranium.radii.is_finite()
            && self.cranium.radii.min_element() > 0.0
            && self.ease.is_finite()
            && self.ease.min_element() >= 0.0
            && self.wall.is_finite()
            && self.wall > 0.0
            && self.opening.is_finite()
            && self.opening > self.cranium.center.y
            && self.opening < self.cranium.center.y + self.cranium.radii.y
            && (8..=128).contains(&self.around)
            && (2..=32).contains(&self.rows)
    }

    fn point(self, angle: f32, t: f32, outer: bool) -> (Vec3, Vec3) {
        let radii =
            self.cranium.radii + self.ease + Vec3::splat(if outer { self.wall } else { 0.0 });
        let phi = ((self.opening - self.cranium.center.y) / radii.y).acos() * (1.0 - t);
        let unit = if t == 1.0 {
            Vec3::Y
        } else {
            Vec3::new(phi.sin() * angle.sin(), phi.cos(), phi.sin() * angle.cos())
        };
        let mut at = self.cranium.center + radii * unit;
        if t == 0.0 {
            at.y = self.opening;
        }
        (at, (unit / radii).normalize())
    }

    pub fn outer_point(self, angle: f32, t: f32) -> Vec3 {
        self.point(angle, t, true).0
    }

    pub fn rim_point(self, angle: f32) -> Vec3 {
        self.point(angle, 0.0, false)
            .0
            .lerp(self.outer_point(angle, 0.0), 0.5)
    }

    pub fn emit(self, b: &mut Builder, color: Vec3) {
        if !self.valid() {
            return;
        }
        let at = |row: usize, col: usize, outer: bool| {
            self.point(
                (col % self.around) as f32 * TAU / self.around as f32,
                row as f32 / self.rows as f32,
                outer,
            )
        };
        for outer in [true, false] {
            let sign = if outer { 1.0 } else { -1.0 };
            let shade = if outer { color } else { color * 0.68 };
            for row in 0..self.rows {
                for col in 0..self.around {
                    let q = [
                        at(row, col, outer),
                        at(row, col + 1, outer),
                        at(row + 1, col + 1, outer),
                        at(row + 1, col, outer),
                    ];
                    let triangles: &[[usize; 3]] = if row + 1 == self.rows {
                        &[[0, 1, 2]]
                    } else {
                        &[[0, 1, 2], [0, 2, 3]]
                    };
                    for ids in triangles {
                        let mut points = ids.map(|k| q[k].0);
                        let mut normals = ids.map(|k| q[k].1 * sign);
                        if (points[1] - points[0])
                            .cross(points[2] - points[0])
                            .dot(normals[0] + normals[1] + normals[2])
                            < 0.0
                        {
                            points.swap(1, 2);
                            normals.swap(1, 2);
                        }
                        b.tri_smooth(points, normals, [shade; 3]);
                    }
                }
            }
        }
        for col in 0..self.around {
            b.quad_out(
                [
                    at(0, col, false).0,
                    at(0, col + 1, false).0,
                    at(0, col + 1, true).0,
                    at(0, col, true).0,
                ],
                self.cranium.center + Vec3::Y * self.cranium.radii.y,
                color,
            );
        }
    }

    pub fn emit_rim(self, b: &mut Builder, radius: f32, color: Vec3) {
        if !self.valid() || !radius.is_finite() || radius <= 0.0 {
            return;
        }
        const CROSS: usize = 6;
        let vertex = |i: usize, j: usize| {
            let angle = (i % self.around) as f32 * TAU / self.around as f32;
            let cross = (j % CROSS) as f32 * TAU / CROSS as f32;
            let center = self.rim_point(angle);
            let radial = (center - self.cranium.center).with_y(0.0).normalize();
            let normal = radial * cross.cos() + Vec3::Y * cross.sin();
            (center + normal * radius, normal)
        };
        for i in 0..self.around {
            for j in 0..CROSS {
                let q = [
                    vertex(i, j),
                    vertex(i + 1, j),
                    vertex(i + 1, j + 1),
                    vertex(i, j + 1),
                ];
                for ids in [[0, 1, 2], [0, 2, 3]] {
                    b.tri_smooth(ids.map(|k| q[k].0), ids.map(|k| q[k].1), [color; 3]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangles(profile: CrownProfile) -> Vec<[Vec3; 3]> {
        let mut b = Builder::default();
        profile.emit(&mut b, STEEL);
        b.surface_triangles().collect()
    }

    #[test]
    fn crown_profile_tracks_anatomy_translation_scale_and_keeps_opening() {
        let base = CrownProfile::winged();
        for scale in [0.7, 1.6] {
            let delta = Vec3::new(0.08, -0.11, 0.21);
            let mut shifted = base;
            shifted.cranium.center = base.cranium.center * scale + delta;
            shifted.cranium.radii *= scale;
            shifted.ease *= scale;
            shifted.wall *= scale;
            shifted.opening = base.opening * scale + delta.y;
            assert_eq!(triangles(base).len(), triangles(shifted).len());
            for (a, b) in triangles(base)
                .iter()
                .flatten()
                .zip(triangles(shifted).iter().flatten())
            {
                assert!(b.distance(*a * scale + delta) < 1e-6);
            }
            assert!(
                shifted
                    .rim_point(0.3)
                    .distance(base.rim_point(0.3) * scale + delta)
                    < 1e-6
            );
        }
        let mut wider = base;
        wider.cranium.radii.x *= 1.4;
        assert!(wider.rim_point(PI * 0.5).x > base.rim_point(PI * 0.5).x + 0.03);
        assert_eq!(wider.rim_point(0.0), base.rim_point(0.0));
        assert_eq!(wider.outer_point(0.0, 1.0), base.outer_point(0.0, 1.0));
    }

    #[test]
    fn lined_crown_is_closed_oriented_and_has_no_forehead_disk() {
        use std::collections::BTreeMap;
        let profile = CrownProfile::winged();
        let tris = triangles(profile);
        assert_eq!(tris.len(), 768);
        let quantize = |v: Vec3| v.to_array().map(|c| (c * 1e6).round() as i32);
        let mut edges = BTreeMap::new();
        for tri in &tris {
            assert!(tri.iter().all(|p| p.is_finite()));
            assert!((tri[1] - tri[0]).cross(tri[2] - tri[0]).length() > 1e-9);
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                let a = quantize(tri[a]);
                let b = quantize(tri[b]);
                let (key, direction) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                let entry = edges.entry(key).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += direction;
            }
        }
        assert!(edges
            .values()
            .all(|&(count, orientation)| count == 2 && orientation == 0));
        let mut b = Builder::default();
        profile.emit(&mut b, STEEL);
        assert!(
            b.outer_surface(Vec3::new(0.0, profile.opening - 0.01, -0.012), Vec3::Y)
                .unwrap()
                .y
                > 0.29
        );
        assert!(b
            .outer_surface(Vec3::new(0.0, profile.opening - 0.001, -0.012), Vec3::Z)
            .is_none());
    }

    #[test]
    fn invalid_crowns_reject_before_emitting_any_surface() {
        let base = CrownProfile::winged();
        let mut cases = [base; 9];
        cases[0].cranium.center.x = f32::NAN;
        cases[1].cranium.radii.y = 0.0;
        cases[2].ease.x = -0.01;
        cases[3].wall = f32::INFINITY;
        cases[4].wall = 0.0;
        cases[5].opening = base.cranium.center.y;
        cases[6].opening = base.cranium.center.y + base.cranium.radii.y;
        cases[7].around = 0;
        cases[8].rows = 1;
        for profile in cases {
            let mut b = Builder::default();
            b.tri([Vec3::ZERO, Vec3::X, Vec3::Y], STEEL);
            let before: Vec<_> = b.surface_triangles().collect();
            profile.emit(&mut b, STEEL);
            profile.emit_rim(&mut b, 0.006, GILT);
            assert_eq!(before, b.surface_triangles().collect::<Vec<_>>());
        }
    }

    #[test]
    fn crown_and_rim_clear_actual_face_across_humanoid_variants() {
        let profile = CrownProfile::winged();
        let mut b = Builder::default();
        profile.emit(&mut b, STEEL);
        profile.emit_rim(&mut b, 0.006, GILT);
        let samples: Vec<_> = b
            .surface_triangles()
            .flat_map(|tri| {
                [
                    tri[0],
                    tri[1],
                    tri[2],
                    (tri[0] + tri[1]) * 0.5,
                    (tri[1] + tri[2]) * 0.5,
                    (tri[2] + tri[0]) * 0.5,
                    (tri[0] + tri[1] + tri[2]) / 3.0,
                ]
            })
            .collect();
        for model in [
            CreatureModel::Human,
            CreatureModel::Nymph,
            CreatureModel::Orc,
            CreatureModel::Hobgoblin,
            CreatureModel::Troll,
            CreatureModel::Centaur,
        ] {
            for woman in [false, true] {
                for build in 0..3 {
                    let look = Look::with_choice(
                        model,
                        22,
                        Some(Variant {
                            woman,
                            build,
                            ..default()
                        }),
                    );
                    let face = face_shape(
                        &look,
                        !matches!(look.style, Style::Hobgoblin | Style::Troll),
                    );
                    let (mut min, mut max) = (f32::INFINITY, f32::NEG_INFINITY);
                    for at in &samples {
                        let gap = face.form.distance(*at);
                        min = min.min(gap);
                        max = max.max(gap);
                    }
                    eprintln!("winged cap {model:?} woman={woman} build={build}: min {min:.6}, max {max:.6}");
                    assert!(
                        min >= 0.001 && max <= 0.030,
                        "{model:?} woman={woman} build={build}: min={min}, max={max}"
                    );
                }
            }
        }
    }

    #[test]
    fn winged_attachment_roots_contact_own_cap_without_other_support() {
        let profile = CrownProfile::winged();
        let mut b = Builder::default();
        profile.emit(&mut b, STEEL);
        for side in [-1.0, 1.0] {
            for k in 0..6 {
                let angle = side * (PI * 0.5 - k as f32 / 5.0 * 0.42);
                let root = profile.outer_point(angle, 0.2);
                let origin = profile.cranium.center.with_y(root.y);
                let ray = (root - origin).normalize();
                let hit = b.outer_surface(origin, ray).unwrap();
                assert!(
                    root.distance(hit) < 0.002,
                    "feather root must touch actual shell"
                );
            }
        }
        let root = headwear_parts::winged_helm_hair_root();
        let rim = profile.rim_point(PI);
        assert!(
            root.x.abs() < 1e-6
                && (root.y - rim.y + hair::BRAID_ROOT_RADIUS - 0.002).abs() < 1e-6
                && (root.z - rim.z).abs() < 1e-6
        );
    }
    #[test]
    fn winged_braid_emerges_below_opening_without_piercing_rear_cap() {
        let profile = CrownProfile::winged();
        for smooth in [false, true] {
            let mut hair = Parts::default();
            if smooth {
                hair.soften();
            }
            headwear_parts::winged_helm_braid(&mut hair, Vec3::ONE);
            let points: Vec<_> = hair.matte.surface_triangles().flatten().collect();
            let top = points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
            assert!(
                top <= profile.opening + 0.003,
                "emitted braid bulb pierces the rear cap: top={top}, opening={}",
                profile.opening
            );
            assert!(
                top >= profile.opening - 0.002,
                "braid must still reach the rim"
            );
            let bottom = points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
            assert!(
                top - bottom >= 0.32,
                "retain the full plait rather than shrinking its hairstyle"
            );
        }
    }

    #[test]
    fn decorative_crown_rim_is_closed_oriented_without_end_caps() {
        use std::collections::BTreeMap;
        let profile = CrownProfile::winged();
        let mut b = Builder::default();
        profile.emit_rim(&mut b, 0.006, GILT);
        let triangles: Vec<_> = b.surface_triangles().collect();
        assert_eq!(triangles.len(), 384);
        let quantize = |v: Vec3| v.to_array().map(|c| (c * 1e6).round() as i32);
        let mut edges = BTreeMap::new();
        for tri in triangles {
            assert!((tri[1] - tri[0]).cross(tri[2] - tri[0]).length() > 1e-9);
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                let (a, b) = (quantize(tri[a]), quantize(tri[b]));
                let (key, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                let e = edges.entry(key).or_insert((0, 0));
                e.0 += 1;
                e.1 += sign;
            }
        }
        assert!(edges.values().all(|&(n, sign)| n == 2 && sign == 0));
    }

    #[test]
    fn actual_winged_fan_insertions_reach_the_emitted_crown() {
        let profile = CrownProfile::winged();
        let mut cap = Builder::default();
        profile.emit(&mut cap, STEEL);
        let mut p = Parts::default();
        headwear_parts::winged_helm(&mut p, STEEL, Vec3::Z);
        let points: Vec<_> = p.matte.surface_triangles().flatten().collect();
        for side in [-1.0, 1.0] {
            for k in 0..6 {
                let angle = side * (PI * 0.5 - k as f32 / 5.0 * 0.42);
                let root = profile.outer_point(angle, 0.2);
                assert!(
                    points.iter().any(|v| v.distance(root) < 1e-6),
                    "production vane must contain its fitted insertion"
                );
                let origin = profile.cranium.center.with_y(root.y);
                let hit = cap
                    .outer_surface(origin, (root - origin).normalize())
                    .unwrap();
                assert!(
                    root.distance(hit) < 0.002,
                    "actual insertion must touch emitted crown"
                );
            }
        }
    }
    #[test]
    fn winged_braid_preserves_authored_lower_tuft_after_root_fitting() {
        for smooth in [false, true] {
            let mut actual = Parts::default();
            let mut expected = Parts::default();
            if smooth {
                actual.soften();
                expected.soften();
            }
            headwear_parts::winged_helm_braid(&mut actual, Vec3::ONE);
            // Independent retained lower hairstyle landmarks, from the previous
            // native braid. Moving its upper insertion must not lower its tail
            // into the shoulders/cloak or invent a new lower route.
            lock(
                &mut expected.matte,
                &[
                    Vec3::new(0.0, -0.055, -0.155),
                    Vec3::new(0.0, -0.105, -0.160),
                ],
                0.02,
                0.014,
                Vec3::ONE,
            );
            let wanted: Vec<_> = expected.matte.surface_triangles().collect();
            let whole: Vec<_> = actual.matte.surface_triangles().collect();
            let tail = &whole[whole.len() - wanted.len()..];
            let maximum = tail
                .iter()
                .flatten()
                .zip(wanted.iter().flatten())
                .map(|(a, b)| a.distance(*b))
                .fold(0.0f32, f32::max);
            assert!(
                maximum < 1e-6,
                "root fit displaced the emitted lower tuft by {maximum} m"
            );
        }
    }
    #[test]
    fn shared_braid_profile_retains_ordinary_authored_geometry() {
        for smooth in [false, true] {
            let top = Vec3::new(0.012, 0.234, -0.137);
            let mut actual = Parts::default();
            let mut expected = Parts::default();
            if smooth {
                actual.soften();
                expected.soften();
            }
            braid(&mut actual, top, Vec3::ONE);
            for k in 0..8 {
                let t = k as f32 / 7.0;
                let at = top
                    + Vec3::new(
                        if k % 2 == 0 { 0.008 } else { -0.008 },
                        -t * 0.26,
                        -t * 0.03,
                    );
                expected.matte.blob(
                    at,
                    Vec3::splat(0.032 - 0.012 * t),
                    1,
                    0.0,
                    Vec3::ONE * (if k % 2 == 0 { 1.0 } else { 0.92 }),
                );
            }
            lock(
                &mut expected.matte,
                &[
                    top + Vec3::new(0.0, -0.27, -0.03),
                    top + Vec3::new(0.0, -0.32, -0.035),
                ],
                0.02,
                0.014,
                Vec3::ONE,
            );
            let a: Vec<_> = actual.matte.surface_triangles().collect();
            let b: Vec<_> = expected.matte.surface_triangles().collect();
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().flatten().zip(b.iter().flatten()) {
                assert!(a.distance(*b) < 1e-6);
            }
        }
    }
}
