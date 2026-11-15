//! Court regalia assembled on explicit garment and joint-local supports.
//! The same rosette and fitted strokes can decorate other ceremonial recipes.
use super::*;
use crate::sdf::Sculpt;

/// A radial metal flower with a faceted jewel, in an arbitrary attachment frame.
pub(super) fn rosette(
    p: &mut Parts,
    center: Vec3,
    normal: Vec3,
    radius: f32,
    metal: Vec3,
    gem: Vec3,
) {
    let n = normal.normalize_or(Vec3::Z);
    let u = n.cross(Vec3::Y).normalize_or(Vec3::X);
    let v = u.cross(n).normalize();
    for k in 0..8 {
        let a = k as f32 * TAU / 8.0;
        let r = u * a.cos() + v * a.sin();
        let t = -u * a.sin() + v * a.cos();
        let path = [
            center + r * radius * 0.35,
            center + r * radius * 0.67 + t * radius * 0.16,
            center + r * radius,
            center + r * radius * 0.67 - t * radius * 0.16,
            center + r * radius * 0.35,
        ];
        tube(
            &mut p.metal,
            &path,
            &[(radius * 0.065, radius * 0.065); 5],
            metal,
        );
    }
    // Tube cross-sections orient to the supplied normal rather than world axes.
    tube(
        &mut p.metal,
        &[center - n * radius * 0.08, center + n * radius * 0.15],
        &[(radius * 0.46, radius * 0.57); 2],
        metal,
    );
    tube(
        &mut p.satin,
        &[
            center + n * radius * 0.12,
            center + n * radius * 0.35,
            center + n * radius * 0.45,
        ],
        &[
            (radius * 0.34, radius * 0.44),
            (radius * 0.24, radius * 0.32),
            (0.0005, 0.0005),
        ],
        gem,
    );
}

fn stroke(p: &mut Parts, support: &Sculpt, path: &[Vec2], radius: f32, metal: Vec3) {
    embroidery_parts::StitchProfile {
        radius,
        max_step: 0.008,
    }
    .emit(
        &mut p.metal,
        path,
        &|uv| torso_surface_hit_offset(support, uv.x, uv.y, true, radius + 0.001),
        metal,
    );
}

/// Lion relief and paired cape clasps, fitted onto the final chest layer.
pub(super) fn torso(p: &mut Parts, body: HumanoidBody, support: &Sculpt, metal: Vec3, gem: Vec3) {
    let scale = body.build;
    if torso_surface_hit(support, 0.0, 0.08, true).is_none() {
        return;
    }
    // Continuous crossing belts share the existing fitted radial-band profile.
    // Front-only ribbons leave unsupported visible ends on the cuirass.
    for (tilt, clearance) in [(-0.035, 0.008), (0.035, 0.013)] {
        radial_parts::RadialBandProfile {
            heights: Vec2::new(0.062, 0.087),
            support_to_output_y: 0.0,
            tilt,
            clearance,
            thickness: 0.003,
            rows: 4,
            sides: 48,
        }
        .emit(&mut p.matte, support, LEATHER * 0.65);
    }
    let center = Vec2::new(0.0, 0.31);
    let crest_scale = scale * 1.35;
    // Curled mane: each lock is independently fitted, not a floating flat badge.
    for k in 0..16 {
        let a = k as f32 * TAU / 16.0;
        let path: Vec<_> = (0..=12)
            .map(|i| {
                let t = i as f32 / 12.0;
                let angle = a + 0.30 * (PI * t).sin();
                center
                    + Vec2::new(angle.cos(), angle.sin() * 1.15) * (0.025 + 0.041 * t) * crest_scale
            })
            .collect();
        stroke(p, support, &path, 0.0035 * scale, metal);
    }
    // Face, muzzle and brow are fitted relief, clearly separate from the mane.
    for (layer, (offset, radii, shade)) in [
        (Vec2::ZERO, Vec3::new(0.026, 0.034, 0.010), 1.0),
        (
            Vec2::new(-0.012, -0.014),
            Vec3::new(0.014, 0.011, 0.009),
            1.10,
        ),
        (
            Vec2::new(0.012, -0.014),
            Vec3::new(0.014, 0.011, 0.009),
            1.10,
        ),
        (Vec2::new(0.0, -0.006), Vec3::new(0.009, 0.008, 0.009), 0.55),
        (
            Vec2::new(-0.011, 0.011),
            Vec3::new(0.006, 0.004, 0.006),
            0.35,
        ),
        (
            Vec2::new(0.011, 0.011),
            Vec3::new(0.006, 0.004, 0.006),
            0.35,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let uv = center + offset * crest_scale;
        if let Some(at) = torso_surface_hit(support, uv.x, uv.y, true) {
            p.metal.blob(
                at + Vec3::Z * ((if layer == 0 { 0.009 } else { 0.025 }) * crest_scale),
                radii * crest_scale,
                1,
                0.0,
                metal * shade,
            );
        }
    }
    for side in [-1.0, 1.0] {
        let uv = Vec2::new(side * body.shoulders * scale * 0.14, 0.445);
        if let Some(at) = torso_surface_hit_offset(support, uv.x, uv.y, true, 0.007 * scale) {
            rosette(p, at, Vec3::Z, 0.029 * scale, metal, gem);
        }
    }
    for row in 0..2 {
        let path: Vec<_> = (0..=32)
            .map(|i| {
                let x = i as f32 / 16.0 - 1.0;
                Vec2::new(
                    x * body.shoulders * scale * 0.14,
                    0.44 - (1.0 - x * x) * (0.025 + row as f32 * 0.018),
                )
            })
            .collect();
        stroke(p, support, &path, 0.0024 * scale, metal);
    }
}

/// Shoulder cap and bullion fringe in the upper-arm joint frame.
pub(super) fn epaulette(
    p: &mut Parts,
    side: f32,
    build: f32,
    arm_length: f32,
    muscle: f32,
    metal: Vec3,
) {
    let support = upper_sleeve_form(side, build, arm_length, muscle);
    let mut rim = Vec::new();
    for k in 0..=32 {
        let a = k as f32 * TAU / 32.0;
        let origin = Vec3::new(0.066 * build * a.sin(), 0.0, 0.058 * build * a.cos());
        let Some(at) = surface_ray_hit(&support, origin, Vec3::Y, 0.006 * build, 0.20 * build)
        else {
            continue;
        };
        rim.push(at);
        if k < 32 && k % 2 == 0 {
            let out = Vec3::new(a.sin(), 0.0, a.cos());
            let end = at + out * (0.018 * build) - Vec3::Y * (0.095 * arm_length);
            tube(
                &mut p.metal,
                &[at, end],
                &[(0.003 * build, 0.003 * build); 2],
                metal,
            );
        }
    }
    if rim.len() > 2 {
        tube(
            &mut p.metal,
            &rim,
            &vec![(0.004 * build, 0.004 * build); rim.len()],
            metal,
        );
        let center = surface_ray_hit(&support, Vec3::ZERO, Vec3::Y, 0.009 * build, 0.20 * build)
            .unwrap_or_else(|| rim.iter().copied().sum::<Vec3>() / rim.len() as f32);
        for edge in rim.windows(2) {
            p.metal
                .tri_out([center, edge[0], edge[1]], center - Vec3::Y, metal);
        }
    }
}

/// Fine scrolling metalwork projected onto a supplied forearm/bracer surface.
pub(super) fn bracer(p: &mut Parts, support: &Sculpt, build: f32, arm_length: f32, metal: Vec3) {
    for side in [-1.0, 1.0] {
        let path: Vec<_> = (0..=32)
            .map(|i| {
                let t = i as f32 / 32.0;
                Vec2::new(
                    side * (0.012 + 0.008 * (t * TAU * 2.0).sin()) * build,
                    -(0.035 + t * 0.18) * arm_length,
                )
            })
            .collect();
        stroke(p, support, &path, 0.0018 * build, metal);
    }
}

/// Open-front stand collar, derived from the same neck sections as anatomy.
/// Emitted in torso coordinates; the head's independent presentation scale is
/// explicitly accounted for rather than widening every body to one fixed ring.
pub(super) fn collar(p: &mut Parts, body: HumanoidBody, cloth: Vec3, metal: Vec3) {
    let neck = NeckAttachment::new(body.build, neck_slim(body.woman));
    let head_scale = storybook_part(Plan::Biped, J::Head);
    let torso_scale = storybook_part(Plan::Biped, J::Torso);
    let at = |t: f32, a: f32, inner: bool| {
        let height = -0.032 + t * 0.075;
        let ease = if inner { 0.008 } else { 0.012 } * body.build;
        (neck.surface(height, a, ease) * head_scale + body.head) / torso_scale
    };
    let opening = 0.35;
    for k in 0..32 {
        let a0 = opening + k as f32 * (TAU - 2.0 * opening) / 32.0;
        let a1 = opening + (k + 1) as f32 * (TAU - 2.0 * opening) / 32.0;
        let outer = [
            at(0.0, a0, false),
            at(0.0, a1, false),
            at(1.0, a1, false),
            at(1.0, a0, false),
        ];
        p.matte.quad_out(outer, body.head / torso_scale, cloth);
        let inner = [
            at(0.0, a0, true),
            at(0.0, a1, true),
            at(1.0, a1, true),
            at(1.0, a0, true),
        ];
        let radial = (outer[0] - Vec3::Y * outer[0].y).normalize_or(Vec3::Z);
        p.matte.quad_out(inner, inner[0] + radial, cloth * 0.75);
        for t in [0.0, 1.0] {
            p.matte.quad_out(
                [
                    at(t, a0, false),
                    at(t, a1, false),
                    at(t, a1, true),
                    at(t, a0, true),
                ],
                Vec3::Y * (body.head.y + 0.01),
                cloth,
            );
        }
    }
    for a in [opening, TAU - opening] {
        p.matte.quad_out(
            [
                at(0.0, a, false),
                at(1.0, a, false),
                at(1.0, a, true),
                at(0.0, a, true),
            ],
            body.head / torso_scale,
            cloth,
        );
    }
    for t in [0.0, 1.0] {
        let path: Vec<_> = (0..=32)
            .map(|k| at(t, opening + k as f32 * (TAU - 2.0 * opening) / 32.0, false))
            .collect();
        tube(
            &mut p.metal,
            &path,
            &vec![(0.002, 0.002); path.len()],
            metal,
        );
    }
    for a in [opening, TAU - opening] {
        tube(
            &mut p.metal,
            &[at(0.0, a, false), at(1.0, a, false)],
            &[(0.002, 0.002); 2],
            metal,
        );
    }
}

/// Ceremonial sheathed sword anchored at the hip, not the moving hand.
/// A compact accessory recipe shares its hilt jewel with the cape clasps.
pub(super) fn sword(p: &mut Parts, body: HumanoidBody, leather: Vec3, metal: Vec3, gem: Vec3) {
    let b = body.build;
    let guard = Vec3::new(-0.21 * body.hips_width * b, 0.045, 0.075 * b);
    let down = Vec3::new(-0.14, -1.0, -0.06).normalize();
    let tip = guard + down * 0.62;
    tube(
        &mut p.satin,
        &[guard, tip],
        &[(0.021 * b, 0.012 * b), (0.009 * b, 0.006 * b)],
        leather,
    );
    for (t, radius) in [(0.02, 0.022), (0.96, 0.010)] {
        let at = guard.lerp(tip, t);
        tube(
            &mut p.metal,
            &[at, at + down * 0.018],
            &[(radius * b, 0.013 * b); 2],
            metal,
        );
    }
    let grip = guard - down * 0.13;
    tube(
        &mut p.satin,
        &[guard, grip],
        &[(0.012 * b, 0.012 * b); 2],
        leather,
    );
    for k in 1..6 {
        let at = guard.lerp(grip, k as f32 / 6.0);
        p.metal.torus(at, down, 0.013 * b, 0.0018 * b, 10, 4, metal);
    }
    let cross = Vec3::new(1.0, -0.14, 0.0).normalize();
    tube(
        &mut p.metal,
        &[
            guard - cross * 0.065 * b - down * 0.012,
            guard,
            guard + cross * 0.065 * b - down * 0.012,
        ],
        &[(0.006 * b, 0.008 * b); 3],
        metal,
    );
    rosette(
        p,
        guard + Vec3::Z * 0.014 * b,
        Vec3::Z,
        0.019 * b,
        metal,
        gem,
    );
    p.metal.blob(grip, Vec3::splat(0.017 * b), 1, 0.0, metal);
    p.satin.blob(
        grip + Vec3::Z * 0.012 * b,
        Vec3::new(0.009, 0.012, 0.007) * b,
        1,
        0.0,
        gem,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collar_and_sheathed_sword_follow_wearer_proportions_without_nonfinite_geometry() {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Human,
                42,
                Some(Variant {
                    woman: false,
                    build,
                    ..default()
                }),
            );
            let body = HumanoidBody::resolve(&look);
            let mut p = Parts::default();
            collar(&mut p, body, Vec3::Z, Vec3::ONE);
            assert!(p.matte.surface_triangles().count() > 64);
            assert!(p
                .matte
                .surface_triangles()
                .flatten()
                .all(|v| v.is_finite() && v.y > 0.48 && v.y < 0.60));
            let mut p = Parts::default();
            sword(&mut p, body, Vec3::X, Vec3::ONE, Vec3::Z);
            assert!(p.satin.surface_triangles().count() > 0);
            assert!(p
                .satin
                .surface_triangles()
                .flatten()
                .all(|v| v.is_finite() && v.x < -0.10 && v.y > -0.65 && v.y < 0.23));
        }
    }

    #[test]
    fn rosettes_and_epaulettes_emit_finite_joint_local_geometry_at_all_builds() {
        for build in [0.8, 1.0, 1.25] {
            for side in [-1.0, 1.0] {
                let mut p = Parts::default();
                epaulette(&mut p, side, build, 1.0, 1.0, Vec3::ONE);
                assert!(p.metal.surface_triangles().count() > 32);
                assert!(p.metal.surface_triangles().flatten().all(|v| v.is_finite()));
            }
            let mut p = Parts::default();
            rosette(
                &mut p,
                Vec3::ZERO,
                Vec3::Z,
                0.03 * build,
                Vec3::ONE,
                Vec3::Z,
            );
            assert!(p.metal.surface_triangles().count() > 32);
            assert!(p.satin.surface_triangles().count() > 0);
            assert!(p.satin.surface_triangles().flatten().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn ornament_strokes_do_not_invent_support() {
        let mut p = Parts::default();
        bracer(&mut p, &Sculpt::default(), 1.0, 1.0, Vec3::ONE);
        assert_eq!(p.metal.surface_triangles().count(), 0);
    }
}
