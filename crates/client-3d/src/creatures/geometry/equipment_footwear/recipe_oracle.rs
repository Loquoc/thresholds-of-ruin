use super::*;
fn original_heeled(appearance: FootwearAppearance) -> bool {
    matches!(
        appearance,
        FootwearAppearance::RidingBoots | FootwearAppearance::Jackboots
    )
}
fn original_color(appearance: FootwearAppearance) -> Vec3 {
    use FootwearAppearance::*;
    match appearance {
        WalkingShoes => Vec3::new(0.39, 0.23, 0.12),
        HardShoes => Vec3::new(0.25, 0.20, 0.15),
        Jackboots => Vec3::new(0.16, 0.13, 0.11),
        CombatBoots => Vec3::new(0.23, 0.17, 0.12),
        JungleBoots => Vec3::new(0.33, 0.32, 0.17),
        HikingBoots => Vec3::new(0.42, 0.29, 0.16),
        MudBoots => Vec3::new(0.20, 0.22, 0.19),
        BuckledBoots => Vec3::new(0.40, 0.22, 0.10),
        RidingBoots => Vec3::new(0.12, 0.13, 0.14),
        SnowBoots => Vec3::new(0.37, 0.27, 0.18),
    }
}
fn original_trim(appearance: FootwearAppearance) -> Vec3 {
    match appearance {
        FootwearAppearance::HardShoes => Vec3::new(0.64, 0.68, 0.72),
        FootwearAppearance::HikingBoots => Vec3::new(0.62, 0.09, 0.06),
        FootwearAppearance::SnowBoots => Vec3::new(0.85, 0.79, 0.63),
        FootwearAppearance::BuckledBoots => Vec3::new(0.70, 0.49, 0.20),
        _ => Vec3::new(0.52, 0.39, 0.22),
    }
}
fn original_fit_shape(
    appearance: FootwearAppearance,
    mut shape: BootShape,
    build: f32,
) -> BootShape {
    use FootwearAppearance::*;
    let fraction = match appearance {
        WalkingShoes => 0.07,
        HardShoes => 0.28,
        Jackboots | RidingBoots => 0.86,
        CombatBoots | JungleBoots => 0.55,
        HikingBoots => 0.39,
        MudBoots => 0.78,
        BuckledBoots => 0.61,
        SnowBoots => 0.58,
    };
    // The knee is the origin, even when the incoming native boot was ankle high.
    shape.top = shape.bottom * (1.0 - fraction);
    let original_top = shape.radius_top;
    shape.radius_top = shape.radius_bottom + (original_top - shape.radius_bottom) * fraction;
    if appearance == SnowBoots {
        shape.radius_top *= 1.12;
    }
    shape.cuff_radius = shape.radius_top + 0.002;
    shape.cuff_thickness = if appearance == SnowBoots {
        0.018
    } else {
        0.008
    };
    shape.sample_radius_top = shape.radius_top + shape.cuff_thickness + 0.004;
    if appearance == SnowBoots {
        let contact_length = shape.top - shape.bottom;
        fit_snow_cuff_contact(&mut shape, build, contact_length);
    }
    shape
}
fn original_emit_foot(
    p: &mut Parts,
    appearance: FootwearAppearance,
    last: BootLast,
    color: Vec3,
    trim: Vec3,
) {
    use FootwearAppearance::*;
    boot_parts::emit(p, last, color);
    let b = last.build;
    let sections = last.sections();
    if appearance == WalkingShoes {
        walking_upper(p, last, color);
    }
    let laced = matches!(
        appearance,
        CombatBoots | JungleBoots | HikingBoots | SnowBoots
    );
    if laced {
        // Sample the authored vamp, rather than suspending flat laces above it.
        for k in 0..4 {
            let t = k as f32 / 4.0;
            let s = sections[2];
            let n = sections[3];
            let y = (s.top + s.height) * (1.0 - t) + (n.top + n.height) * t + 0.001;
            let z = s.z * (1.0 - t) + n.z * t;
            for sign in [-1.0, 1.0] {
                cord(
                    &mut p.satin,
                    Vec3::new(-0.024 * b * sign, y - 0.003, z),
                    Vec3::new(0.024 * b * sign, y - 0.005, z + 0.012 * b),
                    0.0025 * b,
                    trim,
                );
            }
        }
    }
    if appearance == HardShoes {
        // Four overlapping curved sabaton plates, each following the last.
        for index in 2..6 {
            let s = sections[index];
            let n = sections[index + 1];
            for k in 0..8 {
                let point = |section: super::super::anatomy::BootSection, j: usize| {
                    let a = j as f32 * PI / 8.0;
                    Vec3::new(
                        a.cos() * (section.width * 0.96 + 0.003 * b),
                        section.top + a.sin() * (section.height + 0.007),
                        section.z,
                    )
                };
                let corners = [point(s, k), point(s, k + 1), point(n, k + 1), point(n, k)];
                p.metal.tri([corners[0], corners[1], corners[2]], trim);
                p.metal.tri([corners[0], corners[2], corners[3]], trim);
                cord(&mut p.metal, corners[0], corners[1], 0.0025 * b, trim * 0.7);
            }
        }
    }
    if matches!(
        appearance,
        CombatBoots | JungleBoots | HikingBoots | MudBoots | SnowBoots
    ) {
        // Raised edge lugs produce a serrated outsole without lowering the foot.
        for k in 0..7 {
            let t = k as f32 / 6.0;
            let z = (-0.045 + t * 0.20) * b;
            let w = (0.049 + 0.011 * (t * PI).sin()) * b;
            for side in [-1.0, 1.0] {
                p.matte.cuboid(
                    Vec3::new(side * w, BootLast::FLOOR + 0.009, z),
                    Vec3::new(0.009 * b, 0.009, 0.009 * b),
                    Quat::IDENTITY,
                    color * 0.38,
                );
            }
        }
    }
    if appearance == HikingBoots {
        hiking_bumper(p, last, color);
    }
    if appearance == SnowBoots {
        snow_vamp(p, last, trim);
    }
}
fn original_emit_shaft(
    p: &mut Parts,
    appearance: FootwearAppearance,
    shape: BootShape,
    build: f32,
    side: f32,
    color: Vec3,
    trim: Vec3,
) {
    use FootwearAppearance::*;
    let radius = |t: f32| shape.radius_bottom + (shape.radius_top - shape.radius_bottom) * t;
    let y = |t: f32| shape.bottom + (shape.top - shape.bottom) * t;
    p.satin.prism(
        Vec3::Y * shape.top,
        Vec3::Y * shape.bottom,
        shape.radius_top,
        shape.radius_bottom,
        12,
        color,
        0.0,
    );
    match appearance {
        WalkingShoes => {
            // Low dark leather topline, not a contrasting gold ankle bracelet.
            band(p, shape, 0.88, 0.002, color * 0.48);
        }
        HardShoes => {
            // Front greave and two separate ankle straps.
            p.metal.cuboid(
                Vec3::new(0.0, y(0.52), radius(0.52)),
                Vec3::new(radius(0.5) * 0.52, (shape.top - shape.bottom) * 0.42, 0.008),
                Quat::from_rotation_x(-0.06),
                trim,
            );
            for t in [0.2, 0.8] {
                band(p, shape, t, 0.006, color * 0.55);
            }
        }
        Jackboots => {
            // Wide turned-down leather cuff and soft stacked ankle folds.
            band(
                p,
                shape,
                0.88,
                (shape.top - shape.bottom) * 0.11,
                color * 1.15,
            );
            for t in [0.14, 0.25, 0.36] {
                band(p, shape, t, 0.005, color * 0.85);
            }
        }
        CombatBoots | JungleBoots | HikingBoots | SnowBoots => {
            let count = match appearance {
                HikingBoots => 5,
                SnowBoots => 6,
                _ => 8,
            };
            // Jungle boots have a broad, thin canvas gusset rather than the
            // combat boot's thick leather tongue. Keep that construction
            // visible in geometry even when both materials share a pigment.
            let (tongue_width, tongue_depth) = if appearance == JungleBoots {
                (0.66, 0.003)
            } else {
                (0.50, 0.006)
            };
            p.matte.cuboid(
                Vec3::new(0.0, y(0.5), radius(0.5)),
                Vec3::new(
                    radius(0.5) * tongue_width,
                    (shape.top - shape.bottom) * 0.47,
                    tongue_depth,
                ),
                Quat::IDENTITY,
                if appearance == JungleBoots {
                    trim * 0.62
                } else {
                    color * 0.75
                },
            );
            // Each crossing joins two real, consecutive eyelet rows. The
            // former short X marks ended in the facing between eyelets.
            for k in 0..count {
                let t = 0.10 + k as f32 / (count - 1) as f32 * 0.82;
                for sign in [-1.0, 1.0] {
                    let a = Vec3::new(sign * radius(t) * 0.40, y(t), radius(t) + 0.009);
                    p.metal
                        .torus(a, Vec3::Z, 0.004 * build, 0.0015 * build, 8, 3, trim * 0.7);
                    if k + 1 < count {
                        let next = 0.10 + (k + 1) as f32 / (count - 1) as f32 * 0.82;
                        let b =
                            Vec3::new(-sign * radius(next) * 0.40, y(next), radius(next) + 0.009);
                        cord(&mut p.satin, a, b, 0.0025 * build, trim);
                    }
                }
            }
            band(
                p,
                shape,
                0.98,
                if appearance == HikingBoots {
                    0.012
                } else {
                    0.005
                },
                color * 0.6,
            );
            if appearance == JungleBoots {
                for t in [0.13, 0.24] {
                    p.metal.torus(
                        Vec3::new(side * (radius(t) + 0.002), y(t), 0.0),
                        Vec3::X,
                        0.009 * build,
                        0.002 * build,
                        10,
                        4,
                        trim,
                    );
                }
            }
            if appearance == SnowBoots {
                snow_cuff(p, shape, build, trim);
            }
        }
        MudBoots => {
            // Molded, unbroken waterproof shaft with a rolled rim and ankle welt.
            band(p, shape, 0.99, 0.006, color * 1.12);
            band(p, shape, 0.12, 0.008, color * 0.65);
        }
        BuckledBoots => {
            for t in [0.18, 0.52, 0.86] {
                let mut leather = p.satin.empty_like();
                emit_band(&mut leather, shape, t, 0.014, color * 0.68);
                let mount = band_attachment(&leather, y(t), side);
                p.satin.absorb(leather);
                let Some(mount) = mount else {
                    continue;
                };
                let point = |x, y| mount.position + mount.rotation * Vec3::new(x, y, 0.0);
                let corners = [
                    point(-0.012, -0.015),
                    point(0.012, -0.015),
                    point(0.012, 0.015),
                    point(-0.012, 0.015),
                ];
                for k in 0..4 {
                    cord(&mut p.metal, corners[k], corners[(k + 1) % 4], 0.003, trim);
                }
                cord(
                    &mut p.metal,
                    point(0.0, -0.012),
                    point(0.0, 0.012),
                    0.002,
                    trim,
                );
            }
        }
        RidingBoots => {
            // Scalloped topline and a vertical rear seam distinguish the sleek
            // shaft from the folded jackboot, even with identical pigment.
            let path: Vec<_> = (0..=24)
                .map(|i| {
                    let a = i as f32 * TAU / 24.0;
                    Vec3::new(
                        a.sin() * shape.radius_top,
                        shape.top - 0.025 * a.cos().abs(),
                        a.cos() * shape.radius_top,
                    )
                })
                .collect();
            riding_ring(&mut p.satin, &path, 0.004, trim);
            cord(
                &mut p.satin,
                Vec3::new(0.0, y(0.05), -radius(0.05)),
                Vec3::new(0.0, y(0.94), -radius(0.94)),
                0.003,
                color * 1.2,
            );
            band(p, shape, 0.13, 0.004, color * 0.6);
        }
    }
}

#[test]
fn typed_recipes_preserve_decisions_and_complete_emission() {
    let bits = |s: BootShape| {
        [
            s.top,
            s.bottom,
            s.radius_top,
            s.radius_bottom,
            s.cuff_radius,
            s.cuff_thickness,
            s.sample_radius_top,
            s.sample_radius_bottom,
        ]
        .map(f32::to_bits)
    };
    let assert_parts = |a: &Parts, b: &Parts| {
        for (a, b) in [
            (&a.matte, &b.matte),
            (&a.satin, &b.satin),
            (&a.metal, &b.metal),
            (&a.ghost, &b.ghost),
        ] {
            a.assert_same_recipe(b);
            let bits = |b: &Builder| {
                b.surface_triangles()
                    .flatten()
                    .map(|v| v.to_array().map(f32::to_bits))
                    .collect::<Vec<_>>()
            };
            assert_eq!(bits(a), bits(b));
        }
    };
    for appearance in FootwearAppearance::ALL {
        assert_eq!(appearance.heeled(), original_heeled(appearance));
        assert_eq!(appearance.color(), original_color(appearance));
        assert_eq!(appearance.trim(), original_trim(appearance));
        for build in [0.75, 1.0, 1.4] {
            let input = BootShape::fitted(build, -0.24);
            let shape = fit_shape(appearance, input, build);
            let old_shape = original_fit_shape(appearance, input, build);
            assert_eq!(bits(shape), bits(old_shape));
            let last = BootLast {
                build,
                heeled: original_heeled(appearance),
            };
            for (color, trim) in [
                (original_color(appearance), original_trim(appearance)),
                (Vec3::ONE, Vec3::ONE),
            ] {
                let mut a = Parts::default();
                let mut b = Parts::default();
                emit_foot(&mut a, appearance, last, color, trim);
                original_emit_foot(&mut b, appearance, last, color, trim);
                assert_parts(&a, &b);
                for side in [-1.0, 1.0] {
                    let mut a = Parts::default();
                    let mut b = Parts::default();
                    emit_shaft(&mut a, appearance, shape, build, side, color, trim);
                    original_emit_shaft(&mut b, appearance, old_shape, build, side, color, trim);
                    assert_parts(&a, &b);
                }
            }
        }
    }
}
