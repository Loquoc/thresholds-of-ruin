//! Physical Rogue weapon silhouettes, selected solely by disclosed appearance.
//! Recipes share a +X blade/shaft convention; grip metadata uses authored handles.
use super::*;

/// Append a supported recipe. Unknown/generic weapons stay with the caller.
pub(crate) fn append(kind: Kind, p: &mut Parts) -> bool {
    match kind {
        Kind::LongSword => blade(p, false),
        Kind::Dagger => blade(p, true),
        Kind::Shortbow => shortbow(p),
        Kind::Arrow => projectile(p, false),
        Kind::Spear => spear(p),
        Kind::Dart => projectile(p, true),
        Kind::Shuriken => shuriken(p),
        _ => return false,
    }
    true
}

/// Local origin of the actual grasp, not the model bounds' center.
/// The bow's limbs lie along Z so the shared hand rotation makes them upright.
pub(crate) fn grip_frame(kind: Kind) -> Option<Transform> {
    let at = match kind {
        Kind::LongSword => Vec3::new(-0.309, 0.035, 0.0),
        Kind::Dagger => Vec3::new(-0.164, 0.035, 0.0),
        Kind::Shortbow => Vec3::new(0.12, 0.035, 0.0),
        Kind::Spear => Vec3::new(-0.12, 0.035, 0.0),
        Kind::Arrow => Vec3::new(-0.08, 0.024, 0.0),
        Kind::Dart => Vec3::new(-0.055, 0.024, 0.0),
        Kind::Shuriken => Vec3::new(0.0, 0.025, 0.0),
        _ => return None,
    };
    Some(Transform::from_translation(at))
}

fn blade(p: &mut Parts, dagger: bool) {
    let (guard, heel, shoulder, tip, width, pommel) = if dagger {
        (-0.09, -0.075, 0.13, 0.29, 0.038, -0.245)
    } else {
        (-0.21, -0.19, 0.37, 0.52, 0.041, -0.415)
    };
    p.metal.extrude(
        &[
            Vec2::new(heel, -width),
            Vec2::new(shoulder, -width * 0.78),
            Vec2::new(tip, 0.0),
            Vec2::new(shoulder, width * 0.78),
            Vec2::new(heel, width),
        ],
        Vec3::new(0.0, 0.027, 0.0),
        Vec3::X,
        Vec3::NEG_Z,
        0.014,
        STEEL,
    );
    // Shallow raised center facets catch light even without animated glints.
    let ridge_a = Vec3::new(heel, 0.042, 0.0);
    let ridge_b = Vec3::new(shoulder, 0.042, 0.0);
    for sign in [-1.0, 1.0] {
        let edge_a = Vec3::new(heel, 0.035, width * sign);
        let edge_b = Vec3::new(shoulder, 0.035, width * 0.78 * sign);
        p.metal
            .tri_out([ridge_a, edge_a, edge_b], Vec3::ZERO, STEEL * 0.92);
        p.metal
            .tri_out([ridge_a, edge_b, ridge_b], Vec3::ZERO, STEEL * 1.08);
        p.metal.tri_out(
            [ridge_b, edge_b, Vec3::new(tip, 0.034, 0.0)],
            Vec3::ZERO,
            STEEL,
        );
    }
    let half_guard = if dagger { 0.074 } else { 0.105 };
    p.metal.cuboid(
        Vec3::new(guard, 0.036, 0.0),
        Vec3::new(0.016, 0.023, half_guard),
        Quat::IDENTITY,
        IRON,
    );
    for sign in [-1.0, 1.0] {
        p.metal.prism(
            Vec3::new(guard, 0.036, half_guard * sign),
            Vec3::new(guard + 0.025, 0.036, (half_guard + 0.025) * sign),
            0.018,
            0.011,
            6,
            STEEL,
            0.0,
        );
    }
    p.matte.prism(
        Vec3::new(pommel + 0.025, 0.035, 0.0),
        Vec3::new(guard - 0.018, 0.035, 0.0),
        0.024,
        0.023,
        8,
        LEATHER * 0.8,
        0.0,
    );
    for i in 0..4 {
        let x = (pommel + 0.04) + (guard - pommel - 0.075) * i as f32 / 3.0;
        p.matte.prism(
            Vec3::new(x, 0.035, 0.0),
            Vec3::new(x + 0.009, 0.035, 0.0),
            0.026,
            0.026,
            8,
            WOOD_DARK,
            0.0,
        );
    }
    p.metal.prism(
        Vec3::new(pommel - 0.012, 0.035, 0.0),
        Vec3::new(pommel + 0.025, 0.035, 0.0),
        0.032,
        0.03,
        8,
        BRONZE,
        0.0,
    );
}

fn shortbow(p: &mut Parts) {
    let limb = [
        Vec3::new(-0.08, 0.035, -0.43),
        Vec3::new(-0.005, 0.035, -0.35),
        Vec3::new(0.085, 0.035, -0.22),
        Vec3::new(0.12, 0.035, -0.08),
        Vec3::new(0.12, 0.035, 0.08),
        Vec3::new(0.085, 0.035, 0.22),
        Vec3::new(-0.005, 0.035, 0.35),
        Vec3::new(-0.08, 0.035, 0.43),
    ];
    for (i, pair) in limb.windows(2).enumerate() {
        let radius = if i == 0 || i == 6 { 0.016 } else { 0.025 };
        p.matte
            .prism(pair[0], pair[1], radius, radius, 7, WOOD * 1.18, 0.0);
        // A pale laminated belly separates the curve from dark floors.
        p.matte.prism(
            pair[0] - Vec3::X * 0.017,
            pair[1] - Vec3::X * 0.017,
            radius * 0.28,
            radius * 0.28,
            5,
            Vec3::new(0.66, 0.48, 0.27),
            0.0,
        );
    }
    p.matte.prism(
        limb[0],
        limb[7],
        0.0035,
        0.0035,
        5,
        Vec3::new(0.73, 0.67, 0.52),
        0.0,
    );
    p.matte.prism(
        Vec3::new(0.12, 0.035, -0.075),
        Vec3::new(0.12, 0.035, 0.075),
        0.032,
        0.032,
        8,
        LEATHER * 0.7,
        0.0,
    );
    for z in [-0.06, -0.03, 0.0, 0.03, 0.06] {
        p.matte.prism(
            Vec3::new(0.12, 0.035, z),
            Vec3::new(0.12, 0.035, z + 0.007),
            0.034,
            0.034,
            8,
            WOOD_DARK,
            0.0,
        );
    }
    for tip in [limb[0], limb[7]] {
        p.matte.blob(tip, Vec3::splat(0.019), 0, 0.0, BONE * 0.8);
    }
}

fn projectile(p: &mut Parts, dart: bool) {
    let (tail, head, point, radius, feather) = if dart {
        (-0.19, 0.10, 0.20, 0.011, 0.036)
    } else {
        (-0.38, 0.29, 0.40, 0.009, 0.044)
    };
    p.matte.prism(
        Vec3::new(tail, 0.024, 0.0),
        Vec3::new(head, 0.024, 0.0),
        radius,
        radius,
        6,
        WOOD * 1.35,
        0.0,
    );
    let outline = [
        Vec2::new(head - 0.025, -0.025),
        Vec2::new(point, 0.0),
        Vec2::new(head - 0.025, 0.025),
        Vec2::new(head, 0.0),
    ];
    // Separate triangular faces avoid a concave fan across the socket notch.
    p.metal.extrude(
        &outline[..3],
        Vec3::new(0.0, 0.024, 0.0),
        Vec3::X,
        Vec3::NEG_Z,
        0.012,
        STEEL,
    );
    p.metal.prism(
        Vec3::new(head - 0.04, 0.024, 0.0),
        Vec3::new(head + 0.005, 0.024, 0.0),
        radius * 1.5,
        radius * 1.2,
        6,
        IRON,
        0.0,
    );
    for sign in [-1.0, 1.0] {
        let fin = [
            Vec2::new(tail + 0.015, 0.0),
            Vec2::new(tail + 0.01, feather * sign),
            Vec2::new(tail + 0.085, feather * 0.68 * sign),
            Vec2::new(tail + 0.12, 0.0),
        ];
        p.matte.extrude(
            &fin,
            Vec3::new(0.0, 0.025, 0.0),
            Vec3::X,
            Vec3::NEG_Z,
            0.004,
            if dart {
                Vec3::new(0.54, 0.25, 0.18)
            } else {
                Vec3::new(0.80, 0.75, 0.59)
            },
        );
    }
    p.matte.prism(
        Vec3::new(tail + 0.10, 0.024, 0.0),
        Vec3::new(tail + 0.13, 0.024, 0.0),
        radius * 1.35,
        radius * 1.35,
        6,
        LEATHER,
        0.0,
    );
}

fn spear(p: &mut Parts) {
    p.matte.prism(
        Vec3::new(-0.66, 0.035, 0.0),
        Vec3::new(0.43, 0.035, 0.0),
        0.019,
        0.016,
        8,
        WOOD * 1.1,
        0.0,
    );
    p.metal.prism(
        Vec3::new(-0.69, 0.035, 0.0),
        Vec3::new(-0.62, 0.035, 0.0),
        0.024,
        0.020,
        8,
        IRON,
        0.0,
    );
    p.matte.prism(
        Vec3::new(-0.23, 0.035, 0.0),
        Vec3::new(-0.01, 0.035, 0.0),
        0.025,
        0.025,
        8,
        LEATHER,
        0.0,
    );
    for x in [-0.20, -0.15, -0.10, -0.05] {
        p.matte.prism(
            Vec3::new(x, 0.035, 0.0),
            Vec3::new(x + 0.008, 0.035, 0.0),
            0.027,
            0.027,
            8,
            WOOD_DARK,
            0.0,
        );
    }
    p.metal.prism(
        Vec3::new(0.34, 0.035, 0.0),
        Vec3::new(0.46, 0.035, 0.0),
        0.025,
        0.022,
        8,
        IRON,
        0.0,
    );
    p.metal.extrude(
        &[
            Vec2::new(0.41, 0.0),
            Vec2::new(0.48, -0.052),
            Vec2::new(0.70, 0.0),
            Vec2::new(0.48, 0.052),
        ],
        Vec3::new(0.0, 0.035, 0.0),
        Vec3::X,
        Vec3::NEG_Z,
        0.018,
        STEEL,
    );
    p.metal.prism(
        Vec3::new(0.44, 0.046, 0.0),
        Vec3::new(0.66, 0.046, 0.0),
        0.007,
        0.001,
        4,
        STEEL * 1.1,
        0.0,
    );
}

fn shuriken(p: &mut Parts) {
    let outline: Vec<_> = (0..8)
        .map(|i| {
            let angle = i as f32 * TAU / 8.0;
            let radius = if i % 2 == 0 { 0.15 } else { 0.042 };
            Vec2::new(angle.cos() * radius, angle.sin() * radius)
        })
        .collect();
    p.metal.extrude(
        &outline,
        Vec3::new(0.0, 0.025, 0.0),
        Vec3::X,
        Vec3::NEG_Z,
        0.014,
        STEEL,
    );
    for i in 0..4 {
        let a = i as f32 * TAU / 4.0;
        let dir = Vec3::new(a.cos(), 0.0, a.sin());
        p.metal.tri_out(
            [
                Vec3::new(0.0, 0.039, 0.0),
                dir * 0.145 + Vec3::Y * 0.032,
                Vec3::new((a + 0.7).cos(), 0.0, (a + 0.7).sin()) * 0.038 + Vec3::Y * 0.032,
            ],
            Vec3::ZERO,
            STEEL * 0.78,
        );
    }
    p.metal.torus(
        Vec3::new(0.0, 0.038, 0.0),
        Vec3::Y,
        0.018,
        0.005,
        10,
        4,
        IRON,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    const KINDS: [Kind; 7] = [
        Kind::LongSword,
        Kind::Dagger,
        Kind::Shortbow,
        Kind::Arrow,
        Kind::Spear,
        Kind::Dart,
        Kind::Shuriken,
    ];

    #[test]
    fn rogue_weapon_geometry_is_finite_grounded_distinct_and_nonmagical() {
        let mut signatures = Vec::new();
        for kind in KINDS {
            let mut p = Parts::default();
            assert!(append(kind, &mut p));
            assert!(p.accent.is_none());
            assert!(p.glow.positions.is_empty());
            let points: Vec<_> = p
                .matte
                .positions
                .iter()
                .chain(&p.metal.positions)
                .copied()
                .collect();
            assert!(points.len() > 50, "{kind:?}");
            assert!(points.iter().flatten().all(|f| f.is_finite()));
            assert!(
                points.iter().all(|v| v[0].abs() <= 0.72
                    && v[1] >= -0.002
                    && v[1] <= 0.10
                    && v[2].abs() <= 0.46),
                "{kind:?}"
            );
            for b in [&p.matte, &p.metal] {
                for tri in b.positions.as_chunks::<3>().0 {
                    let [a, b, c] = tri;
                    assert!(
                        (Vec3::from_array(*b) - Vec3::from_array(*a))
                            .cross(Vec3::from_array(*c) - Vec3::from_array(*a))
                            .length_squared()
                            > 1e-18,
                        "{kind:?}: degenerate triangle"
                    );
                }
            }
            assert!(
                !signatures.contains(&points),
                "{kind:?}: reused another weapon's silhouette"
            );
            signatures.push(points);
        }
    }

    #[test]
    fn rogue_weapon_grasps_match_authored_handle_centers() {
        let expected = [
            (-0.309, 0.035, 0.0),
            (-0.164, 0.035, 0.0),
            (0.12, 0.035, 0.0),
            (-0.08, 0.024, 0.0),
            (-0.12, 0.035, 0.0),
            (-0.055, 0.024, 0.0),
            (0.0, 0.025, 0.0),
        ];
        for (kind, (x, y, z)) in KINDS.into_iter().zip(expected) {
            let frame = grip_frame(kind).unwrap();
            assert_eq!(frame.translation, Vec3::new(x, y, z));
            assert_eq!(frame.rotation, Quat::IDENTITY);
        }
        for kind in [Kind::LongSword, Kind::Dagger] {
            let mut parts = Parts::default();
            assert!(append(kind, &mut parts));
            let (low, high) = parts.matte.bounds();
            assert!(
                ((low + high) * 0.5).distance(grip_frame(kind).unwrap().translation) < 0.00001,
                "{kind:?}: grip must match the emitted wrapped handle, not model center"
            );
        }
        assert!(grip_frame(Kind::Potion).is_none());
        assert!(!append(Kind::Potion, &mut Parts::default()));
    }
}
