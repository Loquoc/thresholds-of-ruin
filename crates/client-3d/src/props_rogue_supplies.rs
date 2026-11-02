//! Readable supply silhouettes selected only by public appearance recipes.
//! No name, effect, curse, enchantment or identification state enters this module.
use super::*;

pub(super) fn ground(kind: Kind) -> Option<Parts> {
    let mut p = Parts::default();
    match kind {
        Kind::Potion => flask(&mut p),
        Kind::Wand => device(&mut p, false),
        Kind::Staff => device(&mut p, true),
        Kind::Amulet => pendant(&mut p),
        Kind::Bandage => dressing(&mut p),
        Kind::Ration => provisions(&mut p),
        Kind::Fruit => fruit(&mut p),
        Kind::Torch => torch(&mut p),
        _ => return None,
    }
    Some(p)
}

/// Authored shaft centers; parent applies the same hand alignment as swords.
/// The recipes remain unlit even when equipped: light state belongs to the
/// observation-driven lighting path, not an appearance key.
pub(super) fn grip_frame(kind: Kind) -> Option<Transform> {
    let center = match kind {
        Kind::Wand => Vec3::new(-0.14, 0.028, 0.0),
        Kind::Staff => Vec3::new(-0.10, 0.035, 0.0),
        Kind::Torch => Vec3::new(-0.17, 0.045, 0.0),
        _ => return None,
    };
    Some(Transform::from_translation(center))
}

fn flask(p: &mut Parts) {
    // Opaque smoky glass avoids transparency sorting and promises no liquid
    // colour that the generic recipe has not disclosed. The shoulder, heavy
    // foot and cork remain legible when emissive rendering is disabled.
    let glass = Vec3::new(0.32, 0.49, 0.49);
    for (y0, y1, r0, r1, colour) in [
        (0.008, 0.028, 0.066, 0.086, glass * 0.62),
        (0.028, 0.125, 0.086, 0.092, glass),
        (0.125, 0.185, 0.092, 0.037, glass * 1.12),
        (0.185, 0.227, 0.037, 0.032, glass * 0.83),
    ] {
        p.satin
            .prism(Vec3::Y * y0, Vec3::Y * y1, r0, r1, 10, colour, 0.0);
    }
    p.satin
        .torus(Vec3::Y * 0.22, Vec3::Y, 0.034, 0.008, 10, 4, glass * 1.4);
    p.matte.prism(
        Vec3::Y * 0.218,
        Vec3::Y * 0.266,
        0.027,
        0.031,
        8,
        WOOD * 1.5,
        0.0,
    );
    p.matte.prism(
        Vec3::Y * 0.258,
        Vec3::Y * 0.269,
        0.031,
        0.030,
        8,
        WOOD * 1.1,
        0.0,
    );
    // Narrow pale glass facets, not a luminous liquid or potion-effect symbol.
    p.satin.cuboid(
        Vec3::new(-0.055, 0.085, 0.066),
        Vec3::new(0.007, 0.035, 0.003),
        Quat::from_rotation_y(-0.65),
        Vec3::new(0.63, 0.77, 0.72),
    );
    p.metal
        .torus(Vec3::Y * 0.19, Vec3::Y, 0.039, 0.004, 10, 4, BRONZE * 0.8);
}

fn device(p: &mut Parts, staff: bool) {
    if staff {
        let path = [
            Vec3::new(-0.65, 0.035, 0.0),
            Vec3::new(-0.22, 0.035, 0.0),
            Vec3::new(0.24, 0.04, 0.0),
            Vec3::new(0.48, 0.055, 0.015),
            Vec3::new(0.57, 0.055, 0.075),
            Vec3::new(0.54, 0.055, 0.13),
        ];
        p.matte.tube(
            &path,
            &[
                Vec2::splat(0.022),
                Vec2::splat(0.027),
                Vec2::splat(0.026),
                Vec2::splat(0.035),
                Vec2::splat(0.035),
                Vec2::splat(0.023),
            ],
            8,
            &|_, _, _| WOOD * 1.12,
        );
        p.metal.prism(
            path[0],
            path[0] + Vec3::X * 0.065,
            0.025,
            0.024,
            8,
            IRON * 0.75,
            0.0,
        );
        for x in [-0.23, 0.04] {
            p.metal.torus(
                Vec3::new(x, 0.035, 0.0),
                Vec3::X,
                0.028,
                0.005,
                8,
                4,
                BRONZE,
            );
        }
        for x in [-0.20, -0.16, -0.12, -0.08, -0.04, 0.0] {
            p.matte.torus(
                Vec3::new(x, 0.035, 0.0),
                Vec3::X,
                0.027,
                0.005,
                8,
                4,
                LEATHER,
            );
        }
    } else {
        p.matte.prism(
            Vec3::new(-0.26, 0.028, 0.0),
            Vec3::new(0.22, 0.028, 0.0),
            0.027,
            0.017,
            8,
            WOOD_DARK,
            0.0,
        );
        p.matte.prism(
            Vec3::new(-0.25, 0.028, 0.0),
            Vec3::new(-0.04, 0.028, 0.0),
            0.029,
            0.026,
            8,
            WOOD * 1.28,
            0.0,
        );
        for x in [-0.24, -0.04, 0.17] {
            p.metal.torus(
                Vec3::new(x, 0.028, 0.0),
                Vec3::X,
                if x < 0.0 { 0.029 } else { 0.019 },
                0.005,
                8,
                4,
                BRONZE,
            );
        }
        p.matte.prism(
            Vec3::new(0.18, 0.028, 0.0),
            Vec3::new(0.27, 0.028, 0.0),
            0.021,
            0.010,
            8,
            BONE,
            0.0,
        );
    }
}

fn pendant(p: &mut Parts) {
    // Laid-out linked chain and substantial medallion, not a ring or a gem
    // colour that implies the unique amulet's true identity.
    for i in 0..16 {
        let a = i as f32 * TAU / 16.0;
        let center = Vec3::new(-0.04 + 0.12 * a.cos(), 0.015, 0.10 * a.sin());
        let normal = if i % 2 == 0 {
            Vec3::Y
        } else {
            Vec3::new(a.cos(), 0.7, a.sin()).normalize()
        };
        p.metal
            .torus(center, normal, 0.019, 0.004, 8, 4, GOLD * 0.88);
    }
    let at = Vec3::new(0.13, 0.024, 0.0);
    p.metal.prism(
        at - Vec3::Y * 0.017,
        at + Vec3::Y * 0.017,
        0.073,
        0.069,
        10,
        GOLD * 0.72,
        0.0,
    );
    p.metal.prism(
        at + Vec3::Y * 0.017,
        at + Vec3::Y * 0.023,
        0.056,
        0.056,
        10,
        GOLD * 1.06,
        0.0,
    );
    p.metal.torus(
        at + Vec3::Y * 0.025,
        Vec3::Y,
        0.052,
        0.006,
        10,
        4,
        GOLD * 0.75,
    );
    p.metal.prism(
        at + Vec3::Y * 0.024,
        at + Vec3::Y * 0.029,
        0.021,
        0.021,
        6,
        BRONZE,
        0.0,
    );
}

fn fruit(p: &mut Parts) {
    let apple = Vec3::new(0.64, 0.17, 0.08);
    p.matte.blob(
        Vec3::new(-0.026, 0.089, 0.0),
        Vec3::new(0.081, 0.085, 0.086),
        13,
        0.0,
        apple,
    );
    p.matte.blob(
        Vec3::new(0.031, 0.085, 0.0),
        Vec3::new(0.075, 0.080, 0.081),
        29,
        0.0,
        apple * 1.1,
    );
    p.matte.prism(
        Vec3::new(0.0, 0.153, 0.0),
        Vec3::new(0.012, 0.21, -0.004),
        0.010,
        0.006,
        6,
        WOOD_DARK,
        0.0,
    );
    p.matte.extrude(
        &[
            Vec2::new(0.0, 0.0),
            Vec2::new(0.041, -0.025),
            Vec2::new(0.081, 0.0),
            Vec2::new(0.041, 0.02),
        ],
        Vec3::new(0.003, 0.181, 0.0),
        Vec3::X,
        Vec3::NEG_Z,
        0.004,
        Vec3::new(0.23, 0.39, 0.12),
    );
}

fn torch(p: &mut Parts) {
    p.matte.prism(
        Vec3::new(-0.35, 0.045, 0.0),
        Vec3::new(0.24, 0.045, 0.0),
        0.026,
        0.038,
        8,
        WOOD * 0.95,
        0.0,
    );
    p.matte.prism(
        Vec3::new(0.15, 0.055, 0.0),
        Vec3::new(0.33, 0.055, 0.0),
        0.055,
        0.06,
        8,
        LINEN * 0.72,
        0.0,
    );
    for x in [0.17, 0.21, 0.25, 0.29] {
        p.matte
            .torus(Vec3::new(x, 0.055, 0.0), Vec3::X, 0.057, 0.007, 8, 4, TWINE);
    }
    p.matte.prism(
        Vec3::new(0.31, 0.055, 0.0),
        Vec3::new(0.35, 0.055, 0.0),
        0.058,
        0.047,
        8,
        Vec3::new(0.15, 0.12, 0.095),
        0.0,
    );
}

fn dressing(p: &mut Parts) {
    super::bandage(p);
    // Visible nested turns at the roll end, plus the subtle folded edge of
    // the loose strip. No medical cross or claim beyond clean fabric.
    for (radius, tint) in [(0.052, CLOTH * 0.82), (0.038, LINEN * 0.9)] {
        p.matte.torus(
            Vec3::new(0.0, 0.06, 0.069),
            Vec3::Z,
            radius,
            0.004,
            12,
            4,
            tint,
        );
    }
    p.matte.cuboid(
        Vec3::new(0.12, 0.018, -0.049),
        Vec3::new(0.075, 0.004, 0.006),
        Quat::from_rotation_y(0.12),
        CLOTH * 1.07,
    );
}

fn provisions(p: &mut Parts) {
    super::ration(p);
    // A biscuit scored into quarters gives the secondary food shape a clear
    // hard surface distinct from the tied soft packet.
    let face = Quat::from_rotation_y(-0.4) * Quat::from_rotation_x(0.35);
    for turn in [0.0, std::f32::consts::FRAC_PI_2] {
        p.matte.cuboid(
            Vec3::new(0.05, 0.03, 0.16) + face * (Vec3::Y * 0.018),
            Vec3::new(0.056, 0.002, 0.003),
            face * Quat::from_rotation_y(turn),
            WOOD * 0.85,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn points(p: &Parts) -> impl Iterator<Item = &[f32; 3]> {
        p.matte
            .positions
            .iter()
            .chain(&p.satin.positions)
            .chain(&p.metal.positions)
    }

    #[test]
    fn supplies_have_bounded_static_shapes_without_identity_or_light_emission() {
        for kind in [
            Kind::Potion,
            Kind::Wand,
            Kind::Staff,
            Kind::Amulet,
            Kind::Bandage,
            Kind::Ration,
            Kind::Fruit,
            Kind::Torch,
        ] {
            let p = ground(kind).unwrap();
            assert!(
                p.accent.is_none(),
                "{kind:?} must not imply magic or a lit torch"
            );
            assert!(p.glow.positions.is_empty());
            let positions: Vec<_> = points(&p).collect();
            assert!(positions.len() > 60);
            for at in positions {
                assert!(at.iter().all(|v| v.is_finite()));
                assert!(
                    at[0].abs() < 0.7 && at[1] >= -0.02 && at[1] < 0.30 && at[2].abs() < 0.3,
                    "{kind:?}: {at:?}"
                );
            }
        }
        assert!(
            ground(Kind::Coins).is_none(),
            "preserve the existing readable coin recipe"
        );
    }

    #[test]
    fn public_shape_categories_are_distinct_not_magic_variants() {
        let fruit = ground(Kind::Fruit).unwrap();
        let food = ground(Kind::Ration).unwrap();
        assert_ne!(fruit.matte.positions, food.matte.positions);
        let wand = ground(Kind::Wand).unwrap();
        let staff = ground(Kind::Staff).unwrap();
        let span = |p: &Parts| {
            points(p)
                .map(|v| v[0])
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), x| {
                    (lo.min(x), hi.max(x))
                })
        };
        let (wl, wh) = span(&wand);
        let (sl, sh) = span(&staff);
        assert!(wh - wl < 0.6 && sh - sl > 1.1);
        assert!(
            ground(Kind::Potion).unwrap().satin.positions.len() > 60,
            "the flask is shaped reflective glass, not a glowing sphere"
        );
    }

    #[test]
    fn device_grips_are_authored_inside_their_shaft_not_guessed_from_tips() {
        for (kind, expected) in [
            (Kind::Wand, Vec3::new(-0.14, 0.028, 0.0)),
            (Kind::Staff, Vec3::new(-0.10, 0.035, 0.0)),
            (Kind::Torch, Vec3::new(-0.17, 0.045, 0.0)),
        ] {
            assert_eq!(grip_frame(kind).unwrap().translation, expected);
            let p = ground(kind).unwrap();
            // A long prism need not have a vertex at the grasp: intersect the
            // actual shaft triangles through the authored grip instead.
            for direction in [Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z] {
                let surface = p
                    .matte
                    .outer_surface(expected, direction)
                    .unwrap_or_else(|| panic!("{kind:?}: grip is outside shaft"));
                let radius = surface.distance(expected);
                assert!((0.015..0.05).contains(&radius), "{kind:?}: {radius}");
            }
        }
        assert!(grip_frame(Kind::Fruit).is_none());
    }
}
