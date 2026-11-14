use super::*;

#[test]
fn pouch_loops_have_two_sides_instead_of_zero_thickness_sheets() {
    let body = HumanoidBody::resolve(&Look::with_choice(
        CreatureModel::Human,
        22,
        Some(Variant::default()),
    ));
    let support = torso_shape(body, Some(TorsoMaterial::Leather), false);
    let mut pouch = Builder::default();
    waist_parts::pouch(&mut pouch, body, &support, Vec3::X);
    let tris: Vec<_> = pouch.surface_triangles().collect();
    let hash = tris[..732]
        .iter()
        .flatten()
        .flat_map(|p| p.to_array())
        .flat_map(|v| v.to_bits().to_le_bytes())
        .fold(0xcbf29ce484222325_u64, |h, byte| {
            (h ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
    assert_eq!(
        tris.len(),
        940,
        "unchanged 732-triangle bag/flap plus two 104-triangle attachments"
    );
    // A diagnostic for local before/after reconstruction evidence, not a
    // cross-platform oracle for library trigonometric implementations.
    eprintln!("POUCH prefix=732 fingerprint={hash:016x}");
    let face = pouch
        .surface_triangles()
        .find(|tri| {
            tri.iter().all(|p| p.y > 0.055 && p.y < 0.09)
                && (tri[1] - tri[0])
                    .cross(tri[2] - tri[0])
                    .dot(tri[0].with_y(0.0))
                    > 0.0
        })
        .expect("positive fixture: the actual pouch emits its rising loop face");
    let center = face.iter().copied().sum::<Vec3>() / 3.0;
    let origin = Vec3::Y * center.y;
    let out = center.with_y(0.0).normalize();
    let inner = pouch.inner_surface(origin, out).unwrap();
    let outer = pouch.outer_surface(origin, out).unwrap();
    let thickness = (outer - inner).dot(out);
    assert!(
        thickness > 0.0015 && thickness < 0.0025,
        "pouch attachment must have physical leather thickness, not a one-sided sheet: {thickness}"
    );
}

fn closed_oriented(tris: &[[Vec3; 3]]) {
    use std::collections::BTreeMap;
    let key = |p: Vec3| p.to_array().map(|v| (v * 10_000_000.0).round() as i32);
    let mut edges = BTreeMap::<([i32; 3], [i32; 3]), (usize, i32)>::new();
    for tri in tris {
        assert!(tri.iter().all(|p| p.is_finite()));
        assert!((tri[1] - tri[0]).cross(tri[2] - tri[0]).length() > 1e-10);
        for i in 0..3 {
            let (a, b) = (key(tri[i]), key(tri[(i + 1) % 3]));
            assert_ne!(a, b);
            let (edge, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
            let entry = edges.entry(edge).or_default();
            entry.0 += 1;
            entry.1 += sign;
        }
    }
    assert!(!edges.is_empty());
    assert!(
        edges
            .values()
            .all(|&(count, orientation)| count == 2 && orientation == 0),
        "every exposed edge must close with opposite winding"
    );
}

fn profile() -> radial_parts::RadialAttachmentProfile {
    radial_parts::RadialAttachmentProfile {
        heights: Vec2::new(-0.1, 0.2),
        angles: Vec2::new(0.6, 0.8),
        clearance: 0.012,
        thickness: 0.002,
        rows: 8,
        columns: 2,
    }
}

#[test]
fn radial_attachment_is_closed_oriented_and_preserves_both_span_directions() {
    for reverse_y in [false, true] {
        for reverse_a in [false, true] {
            let mut p = profile();
            if reverse_y {
                p.heights = Vec2::new(p.heights.y, p.heights.x);
            }
            if reverse_a {
                p.angles = Vec2::new(p.angles.y, p.angles.x);
            }
            let b = p.resolve(&|_, _, gap| Some(0.2 + gap), Vec3::X).unwrap();
            let tris: Vec<_> = b.surface_triangles().collect();
            assert_eq!(tris.len(), 104, "explicit 8 by 2 finite patch budget");
            closed_oriented(&tris);
            let (min, max) = b.bounds();
            assert!((min.y + 0.1).abs() < 1e-6 && (max.y - 0.2).abs() < 1e-6);
            let out = Vec3::new(0.7_f32.sin(), 0.0, 0.7_f32.cos());
            let inner = b.inner_surface(Vec3::ZERO, out).unwrap();
            let outer = b.outer_surface(Vec3::ZERO, out).unwrap();
            assert!((inner.length() - 0.212).abs() < 1e-6);
            assert!((outer.length() - 0.214).abs() < 1e-6);
            assert!(
                b.outer_surface(Vec3::ZERO, -out).is_none(),
                "do not wrap a patch around its wearer"
            );
            for tri in tris {
                let n = (tri[1] - tri[0]).cross(tri[2] - tri[0]);
                let radial = tri.iter().copied().sum::<Vec3>().with_y(0.0).normalize();
                let radii = tri.map(|v| v.with_y(0.0).length());
                if radii.iter().all(|r| (*r - 0.212).abs() < 1e-6) {
                    assert!(n.dot(radial) < 0.0);
                }
                if radii.iter().all(|r| (*r - 0.214).abs() < 1e-6) {
                    assert!(n.dot(radial) > 0.0);
                }
            }
        }
    }
}

#[test]
fn radial_attachment_rejects_invalid_profiles_and_missing_sampled_support() {
    let p = profile();
    let support = |_: f32, _: f32, gap: f32| Some(0.2 + gap);
    assert!(
        p.resolve(&support, Vec3::X).is_some(),
        "valid positive fixture first"
    );
    for invalid in [
        None,
        Some(0.0),
        Some(-0.1),
        Some(f32::NAN),
        Some(f32::INFINITY),
        Some(f32::MAX),
    ] {
        for (y, a) in [(-0.1, 0.6), (0.2, 0.8), (0.05, 0.65), (0.03125, 0.75)] {
            assert!(
                p.resolve(
                    &|h, angle, gap| {
                        if (h - y).abs() < 1e-6 && (angle - a).abs() < 1e-6 {
                            invalid
                        } else {
                            support(h, angle, gap)
                        }
                    },
                    Vec3::X
                )
                .is_none(),
                "missing endpoint or half-cell support must reject the whole attachment"
            );
        }
    }
    let mut invalid = vec![
        radial_parts::RadialAttachmentProfile { rows: 0, ..p },
        radial_parts::RadialAttachmentProfile { columns: 0, ..p },
        radial_parts::RadialAttachmentProfile {
            heights: Vec2::ONE,
            ..p
        },
        radial_parts::RadialAttachmentProfile {
            heights: Vec2::splat(f32::NAN),
            ..p
        },
        radial_parts::RadialAttachmentProfile {
            angles: Vec2::ONE,
            ..p
        },
        radial_parts::RadialAttachmentProfile {
            angles: Vec2::new(0.0, PI),
            ..p
        },
        radial_parts::RadialAttachmentProfile {
            angles: Vec2::splat(f32::INFINITY),
            ..p
        },
    ];
    for v in [-1.0, f32::NAN, f32::INFINITY] {
        invalid.push(radial_parts::RadialAttachmentProfile { clearance: v, ..p });
        invalid.push(radial_parts::RadialAttachmentProfile { thickness: v, ..p });
    }
    invalid.push(radial_parts::RadialAttachmentProfile {
        thickness: 0.0,
        ..p
    });
    for bad in invalid {
        assert!(bad.resolve(&support, Vec3::X).is_none());
    }
}

#[test]
fn radial_attachment_parameters_follow_asymmetric_support_without_recentering() {
    let p = profile();
    let support = |y: f32, a: f32, gap: f32| Some(0.18 + 0.1 * y + 0.02 * a.sin() + gap);
    let mesh = p.resolve(&support, Vec3::X).unwrap();
    closed_oriented(&mesh.surface_triangles().collect::<Vec<_>>());
    for at in mesh.surface_triangles().flatten() {
        let angle = at.x.atan2(at.z);
        let expected = support(at.y, angle, p.clearance).unwrap();
        let radial = at.with_y(0.0).length();
        assert!(
            (radial - expected).abs() < 1e-6 || (radial - expected - p.thickness).abs() < 1e-6,
            "vertices must follow the caller's asymmetric frame, not a nominal cylinder"
        );
    }
}

#[test]
fn both_production_pouch_attachments_are_closed_and_follow_body_and_clothed_support() {
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
                for material in [
                    None,
                    Some(TorsoMaterial::Leather),
                    Some(TorsoMaterial::Plate),
                ] {
                    let support = torso_shape(body, material, false);
                    let pelvis = pelvis_shape(body);
                    let mut mesh = Builder::default();
                    waist_parts::pouch(&mut mesh, body, &support, Vec3::X);
                    let tris: Vec<_> = mesh.surface_triangles().collect();
                    assert_eq!(
                        tris.len(),
                        940,
                        "complete unchanged bag/flap and both finite straps required"
                    );
                    let loops = &tris[732..];
                    for (i, loop_tris) in loops.as_chunks::<104>().0.iter().enumerate() {
                        closed_oriented(loop_tris);
                        let vertices: Vec<_> = loop_tris.iter().flatten().copied().collect();
                        let low = vertices.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
                        let high = vertices
                            .iter()
                            .map(|p| p.y)
                            .fold(f32::NEG_INFINITY, f32::max);
                        assert!((low - 0.032 * body.build).abs() < 1e-6);
                        assert!((high - (BELT_TOP_Y + 0.005)).abs() < 1e-6);
                        let angles: Vec<_> = vertices.iter().map(|p| p.x.atan2(p.z)).collect();
                        if i == 0 {
                            assert!(angles.iter().all(|a| *a < 0.9));
                        } else {
                            assert!(angles.iter().all(|a| *a > 0.9));
                        }
                        for tri in loop_tris {
                            for at in [
                                tri[0],
                                tri[1],
                                tri[2],
                                (tri[0] + tri[1]) * 0.5,
                                (tri[1] + tri[2]) * 0.5,
                                (tri[2] + tri[0]) * 0.5,
                                (tri[0] + tri[1] + tri[2]) / 3.0,
                            ] {
                                let gap =
                                    support.distance(at - body.torso).min(pelvis.distance(at));
                                assert!((0.002..0.018).contains(&gap),
                                    "{model:?} woman={woman} build={build}: loop {i} clearance {gap} at {at:?}");
                            }
                        }
                    }
                }
            }
        }
    }
}
