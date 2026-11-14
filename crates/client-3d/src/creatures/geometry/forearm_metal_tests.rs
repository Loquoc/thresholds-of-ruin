use super::*;

#[test]
fn fitted_forearm_cuff_has_an_open_bore_and_clears_its_sleeve() {
    let mut minimum = f32::INFINITY;
    let mut maximum = f32::NEG_INFINITY;
    for build in [0.7, 1.0, 1.4] {
        for length in [0.8, 1.2] {
            for muscle in [0.8, 1.2] {
                let support = forearm_sleeve_form(build, length, muscle);
                let mut cuff = Builder::default();
                biped_cuff(&mut cuff, build, length, muscle, false, STEEL, GILT);
                let tris: Vec<_> = cuff.surface_triangles().collect();
                assert_eq!(tris.len(), 576, "complete cuff required for every fixture");
                closed_oriented(&tris);
                assert!(cuff
                    .outer_surface(Vec3::Y * -0.18 * length, Vec3::X)
                    .is_some());
                for tri in cuff.surface_triangles() {
                    for at in [
                        tri[0],
                        tri[1],
                        tri[2],
                        (tri[0] + tri[1]) * 0.5,
                        (tri[1] + tri[2]) * 0.5,
                        (tri[2] + tri[0]) * 0.5,
                        (tri[0] + tri[1] + tri[2]) / 3.0,
                    ] {
                        let gap = support.distance(at);
                        minimum = minimum.min(gap);
                        maximum = maximum.max(gap);
                    }
                }
                assert!(
                    cuff.outer_surface(Vec3::Y * -0.4 * length, Vec3::Y)
                        .is_none(),
                    "a rigid cuff must leave an open bore, not cap across the wearer"
                );
            }
        }
    }
    assert!(
        minimum >= 0.001,
        "actual cuff intersects its sleeve: minimum={minimum}, maximum={maximum}"
    );
    assert!(
        maximum <= 0.020,
        "actual cuff floats above sleeve: minimum={minimum}, maximum={maximum}"
    );
}

fn samples(tri: [Vec3; 3]) -> [Vec3; 7] {
    [
        tri[0],
        tri[1],
        tri[2],
        (tri[0] + tri[1]) * 0.5,
        (tri[1] + tri[2]) * 0.5,
        (tri[2] + tri[0]) * 0.5,
        (tri[0] + tri[1] + tri[2]) / 3.0,
    ]
}

fn closed_oriented(tris: &[[Vec3; 3]]) {
    use std::collections::BTreeMap;
    let key = |p: Vec3| p.to_array().map(|v| (v * 1_000_000.0).round() as i32);
    let mut edges = BTreeMap::<([i32; 3], [i32; 3]), (usize, i32)>::new();
    for tri in tris {
        assert!(tri.iter().all(|p| p.is_finite()));
        assert!((tri[1] - tri[0]).cross(tri[2] - tri[0]).length() > 1e-10);
        for i in 0..3 {
            let a = key(tri[i]);
            let b = key(tri[(i + 1) % 3]);
            assert_ne!(a, b);
            let (edge, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
            let entry = edges.entry(edge).or_default();
            entry.0 += 1;
            entry.1 += sign;
        }
    }
    assert!(
        edges
            .values()
            .all(|&(count, orientation)| count == 2 && orientation == 0),
        "closed oppositely oriented edges required"
    );
}

#[test]
fn limb_shell_retains_span_open_bore_and_closed_oriented_walls() {
    for heights in [Vec2::new(-0.2, 0.1), Vec2::new(0.1, -0.2)] {
        let profile = radial_parts::LimbShellProfile {
            heights,
            clearance: 0.002,
            thickness: 0.003,
            rows: 8,
            sides: 16,
        };
        let shell = profile
            .resolve(&|_, _| Some(0.06), STEEL, GILT, true)
            .unwrap();
        let tris: Vec<_> = shell.surface_triangles().collect();
        assert_eq!(tris.len(), 576);
        closed_oriented(&tris);
        for axis in [Vec3::Y, Vec3::NEG_Y] {
            assert!(shell.outer_surface(-axis, axis).is_none());
        }
        for y in [-0.2, -0.05, 0.1] {
            let r = shell.inner_surface(Vec3::Y * y, Vec3::X).unwrap().x;
            assert!((0.061..0.064).contains(&r));
        }
        let (min, max) = shell.bounds();
        assert!((min.y + 0.2).abs() < 1e-6 && (max.y - 0.1).abs() < 1e-6);
    }
}

#[test]
fn limb_shell_rejects_missing_nonfinite_and_unbounded_support() {
    let p = radial_parts::LimbShellProfile {
        heights: Vec2::new(-0.2, -0.1),
        clearance: 0.002,
        thickness: 0.003,
        rows: 8,
        sides: 16,
    };
    for invalid in [None, Some(f32::NAN), Some(-0.1), Some(0.31)] {
        assert!(p
            .resolve(
                &|y, a| if y < -0.17 && a < 0.01 {
                    invalid
                } else {
                    Some(0.06)
                },
                STEEL,
                GILT,
                true
            )
            .is_none());
    }
    let zero = radial_parts::LimbShellProfile {
        heights: Vec2::ZERO,
        ..p
    };
    assert!(zero
        .resolve(&|_, _| Some(0.06), STEEL, GILT, true)
        .is_none());
}

#[test]
fn metal_forearm_and_layered_cuff_follow_actual_anatomy_and_mesh() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Orc,
        CreatureModel::Nymph,
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                let body = HumanoidBody::resolve(&Look::with_choice(
                    model,
                    22,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                ));
                let b = body.build;
                let length = body.arm_length;
                let muscle = body.profile.muscle;
                let anatomy = forearm_form(b, length, muscle);
                for smooth in [false, true] {
                    let mut support = Builder::default();
                    support.smooth = smooth;
                    metal_forearm(&mut support, b, length, muscle, STEEL);
                    assert_eq!(support.smooth, smooth);
                    let prior: Vec<_> = support.surface_triangles().collect();
                    assert_eq!(prior.len(), 320);
                    closed_oriented(&prior);
                    assert!(support.outer_surface(Vec3::Y * 0.1, Vec3::NEG_Y).is_some());
                    assert!(support
                        .outer_surface(Vec3::Y * (-0.4 * length), Vec3::Y)
                        .is_some());
                    for tri in &prior {
                        if (tri[1].y - tri[0].y).abs() < 1e-7 && (tri[2].y - tri[0].y).abs() < 1e-7
                        {
                            continue;
                        }
                        for at in samples(*tri) {
                            let gap = anatomy.distance(at);
                            assert!(
                                (0.001..0.023).contains(&gap),
                                "base {model:?} woman={woman} build={build} at={at:?} gap={gap}"
                            );
                        }
                    }
                    // Independent retained mesh oracle, never the fitter's own radius.
                    let mut oracle = Builder::default();
                    for tri in &prior {
                        oracle.tri(*tri, STEEL);
                    }
                    metal_forearm_cuff(&mut support, b, length, GILT);
                    assert_eq!(support.smooth, smooth);
                    let result: Vec<_> = support.surface_triangles().collect();
                    assert_eq!(&result[..prior.len()], prior.as_slice());
                    let cuff = &result[prior.len()..];
                    assert_eq!(cuff.len(), 448);
                    closed_oriented(cuff);
                    let mut cuff_mesh = Builder::default();
                    for tri in cuff {
                        cuff_mesh.tri(*tri, GILT);
                    }
                    assert!(cuff_mesh
                        .outer_surface(Vec3::Y * (-0.4 * length), Vec3::Y)
                        .is_none());
                    for tri in cuff {
                        for at in samples(*tri) {
                            let origin = Vec3::Y * at.y;
                            let ray = (at - origin).normalize();
                            let hit = oracle.outer_surface(origin, ray).unwrap();
                            let gap = (at - hit).dot(ray);
                            assert!(
                                (0.001..0.020).contains(&gap),
                                "layer {model:?} woman={woman} build={build} at={at:?} gap={gap}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn fitted_cuffs_retain_authored_bell_geometry_exactly() {
    for build in [0.7, 1.0, 1.4] {
        for length in [0.8, 1.2] {
            for muscle in [0.8, 1.2] {
                let heights = [0.055, 0.11, 0.19, 0.25, 0.263];
                let radii = [
                    Vec2::new(0.052, 0.047),
                    Vec2::new(0.057, 0.05),
                    Vec2::new(0.072, 0.063),
                    Vec2::new(0.084, 0.075),
                    Vec2::new(0.083, 0.074),
                ];
                let profile = std::array::from_fn::<_, 5, _>(|i| {
                    (
                        heights[i],
                        radii[i]
                            * build
                            * (1.0
                                + (muscle - 1.0)
                                    * (1.0 - ease((heights[i] - 0.145) / (0.25 - 0.145))))
                            + Vec2::splat(SLEEVE_EASE * build),
                    )
                });
                let mut reference = Builder::default();
                sleeve_parts::bell(&mut reference, &profile, build, length, STEEL, GILT);
                let mut actual = Builder::default();
                biped_cuff(&mut actual, build, length, muscle, true, STEEL, GILT);
                assert_eq!(
                    actual.surface_triangles().collect::<Vec<_>>(),
                    reference.surface_triangles().collect::<Vec<_>>()
                );
            }
        }
    }
}

#[test]
fn metal_forearm_attachment_preserves_transformed_recipe_frame_and_prior_cop() {
    use bevy::math::Affine3A;
    let mut reference = Parts::default();
    let mut transformed = Parts::default();
    transformed.set_transform(Affine3A::from_scale_rotation_translation(
        Vec3::splat(1.1),
        Quat::from_rotation_z(0.4),
        Vec3::new(0.2, 0.7, -0.3),
    ));
    for parts in [&mut reference, &mut transformed] {
        parts
            .metal
            .blob(Vec3::ZERO, Vec3::splat(0.058), 2, 0.0, STEEL);
        parts.metal.blob(
            Vec3::new(0.055, 0.0, -0.01),
            Vec3::new(0.012, 0.05, 0.045),
            2,
            0.0,
            STEEL,
        );
        metal_forearm(&mut parts.metal, 1.0, 1.0, 1.0, STEEL);
        let before: Vec<_> = parts.metal.surface_triangles().collect();
        metal_forearm_cuff(&mut parts.metal, 1.0, 1.0, GILT);
        let after: Vec<_> = parts.metal.surface_triangles().collect();
        assert_eq!(&after[..before.len()], before.as_slice());
        assert_eq!(after.len() - before.len(), 448);
    }
    for (a, b) in reference
        .metal
        .surface_triangles()
        .zip(transformed.metal.surface_triangles())
    {
        for (x, y) in a.into_iter().zip(b) {
            assert!(
                x.distance(y) < 0.00001,
                "transformed local surface: {x:?}, {y:?}"
            );
        }
    }
}

#[test]
fn incomplete_forearm_support_does_not_modify_existing_geometry() {
    let mut b = Builder::default();
    b.blob(Vec3::new(0.1, 0.2, 0.3), Vec3::splat(0.01), 1, 0.0, GILT);
    let before: Vec<_> = b.surface_triangles().collect();
    metal_forearm_cuff(&mut b, 1.0, 1.0, STEEL);
    assert_eq!(b.surface_triangles().collect::<Vec<_>>(), before);
    let p = radial_parts::LimbShellProfile {
        heights: Vec2::new(-0.2, -0.1),
        clearance: 0.002,
        thickness: 0.003,
        rows: 8,
        sides: 16,
    };
    assert!(p
        .resolve(
            &|y, a| if y > -0.11 && a > 5.5 {
                None
            } else {
                Some(0.06)
            },
            STEEL,
            GILT,
            true
        )
        .is_none());
}
