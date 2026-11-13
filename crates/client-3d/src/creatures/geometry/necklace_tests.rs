use super::*;

#[test]
fn torso_necklace_emitted_triangles_clear_actual_body() {
    let mut violations = Vec::new();
    for model in [
        CreatureModel::Human,
        CreatureModel::Orc,
        CreatureModel::Nymph,
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
                let body = HumanoidBody::resolve(&look);
                let support = torso_shape(body, None, true);
                let face = face_shape(&look, true).form;
                let composed_support =
                    super::super::necklace_parts::NecklaceSupport::new(body, &support, &face);
                for neck in [
                    Neck::Pendant {
                        metal: Vec3::ONE,
                        gem: Vec3::X,
                    },
                    Neck::Teeth { cord: Vec3::Y },
                ] {
                    for smooth in [false, true] {
                        let mut parts = Parts::default();
                        parts.matte.smooth = smooth;
                        parts.metal.smooth = smooth;
                        parts.satin.smooth = smooth;
                        neck_collar(&mut parts, neck, body, &support, &look);
                        let mut minimum = f32::INFINITY;
                        let mut maximum = f32::NEG_INFINITY;
                        let mut count = 0;
                        for mesh in [&parts.metal, &parts.satin, &parts.matte] {
                            for tri in mesh.surface_triangles() {
                                for p in [
                                    tri[0],
                                    tri[1],
                                    tri[2],
                                    (tri[0] + tri[1]) * 0.5,
                                    (tri[1] + tri[2]) * 0.5,
                                    (tri[2] + tri[0]) * 0.5,
                                    (tri[0] + tri[1] + tri[2]) / 3.0,
                                ] {
                                    assert!(p.is_finite() && support.distance(p).is_finite());
                                    let head = (p * storybook_part(Plan::Biped, J::Torso)
                                        - body.head)
                                        / storybook_part(Plan::Biped, J::Head);
                                    assert!(
                                        face.distance(head) >= 0.001,
                                        "necklace intersects actual head neck: {head:?}"
                                    );
                                    minimum = minimum.min(composed_support.distance(p));
                                    maximum = maximum.max(composed_support.distance(p));
                                    count += 1;
                                }
                            }
                        }
                        eprintln!("necklace {model:?} woman={woman} build={build} {neck:?}: minimum {minimum:.6}, maximum {maximum:.6}, samples {count}");
                        if count == 0 || minimum < 0.001 || maximum > 0.025 {
                            violations.push(format!("{model:?} woman={woman} build={build} smooth={smooth} {neck:?}: samples {count}, min {minimum:.6}, max {maximum:.6}"));
                        }
                    }
                }
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[test]
fn necklace_layout_tracks_anatomy_and_retains_exact_closed_mounts() {
    use super::super::necklace_parts::NecklaceProfile;
    use crate::sdf::{Sculpt, Shape};
    let look = Look::with_choice(CreatureModel::Human, 22, None);
    let mut core =
        super::super::super::anatomy::TorsoCoreProfile::biped(HumanoidBody::resolve(&look));
    let offset = Vec3::new(0.026, 0.13, -0.04);
    let initial_anchor = core.upper_chest_anchor();
    core.chest.upper.0 += offset;
    assert_eq!(core.upper_chest_anchor(), initial_anchor + offset);
    let base = NecklaceProfile {
        anchor: Vec3::new(0.0, 0.09, 0.0),
        front_drop: 0.06,
        clearance: 0.004,
        radius: 0.004,
        stations: 64,
        sides: 6,
    };
    let mut support = Sculpt::default();
    support.add(
        Shape::Ellipsoid(Vec3::ZERO, Vec3::new(0.16, 0.18, 0.1)),
        0.0,
    );
    let a = base.resolve(&support).unwrap();
    let mut moved = Sculpt::default();
    moved.add(Shape::Ellipsoid(offset, Vec3::new(0.16, 0.18, 0.1)), 0.0);
    let b = NecklaceProfile {
        anchor: base.anchor + offset,
        ..base
    }
    .resolve(&moved)
    .unwrap();
    for (a, b) in a.path.iter().zip(&b.path) {
        assert!((*a + offset - *b).length() < 0.00001);
    }
    for (i, p) in a.path.iter().enumerate() {
        assert!((a.mount(i as f32 * TAU / 64.0) - *p).length() < 0.00001);
    }
    assert!(a.mount(0.0).distance(a.mount(TAU)) < 1e-6);
    assert!(a.mount(-0.32).distance(a.mount(TAU - 0.32)) < 1e-6);
    let mut mesh = Builder::default();
    a.emit(&mut mesh, Vec3::ONE);
    let triangles: Vec<_> = mesh.surface_triangles().collect();
    assert_eq!(triangles.len(), 768);
    let key = |p: Vec3| {
        [
            (p.x * 1e7).round() as i32,
            (p.y * 1e7).round() as i32,
            (p.z * 1e7).round() as i32,
        ]
    };
    let mut edges = std::collections::BTreeMap::new();
    for t in &triangles {
        assert!((t[1] - t[0]).cross(t[2] - t[0]).length() > 1e-9);
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *edges.entry((key(a), key(b))).or_insert(0usize) += 1;
        }
    }
    for ((a, b), count) in &edges {
        assert_eq!(*count, 1);
        assert_eq!(edges.get(&(*b, *a)), Some(&1));
    }
    for tri in triangles {
        for p in [tri[0], tri[1], tri[2], (tri[0] + tri[1] + tri[2]) / 3.0] {
            let gap = support.distance(p);
            assert!((0.002..=0.014).contains(&gap), "gap {gap}");
        }
    }
    assert!(base.resolve(&Sculpt::default()).is_none());
    let mut partial = support.clone();
    partial.carve(Shape::Beyond(Vec3::X, -0.08), 0.0);
    assert!(base.resolve(&partial).is_none());
    for bad in [
        NecklaceProfile {
            radius: f32::NAN,
            ..base
        },
        NecklaceProfile {
            anchor: Vec3::splat(f32::NAN),
            ..base
        },
        NecklaceProfile {
            stations: 0,
            ..base
        },
        NecklaceProfile {
            front_drop: -0.1,
            ..base
        },
    ] {
        assert!(bad.resolve(&support).is_none());
    }
}

#[test]
fn necklaces_keep_style_mounts_independent_of_other_accessories() {
    let look = Look::with_choice(
        CreatureModel::Human,
        22,
        Some(Variant {
            woman: true,
            ..default()
        }),
    );
    let body = HumanoidBody::resolve(&look);
    let support = torso_shape(body, None, true);
    let core = super::super::super::anatomy::TorsoCoreProfile::biped(body);
    let face = face_shape(&look, true).form;
    let anatomy_support = super::super::necklace_parts::NecklaceSupport::new(body, &support, &face);
    let mut moved_body = body;
    let movement = Vec3::new(0.02, 0.03, -0.01);
    moved_body.head += movement;
    let moved_support =
        super::super::necklace_parts::NecklaceSupport::new(moved_body, &support, &face);
    assert!(
        (moved_support.anchor()
            - anatomy_support.anchor()
            - movement / storybook_part(Plan::Biped, J::Torso))
        .length()
            < 1e-6
    );

    for neck in [
        Neck::Pendant {
            metal: Vec3::ONE,
            gem: Vec3::X,
        },
        Neck::Teeth { cord: Vec3::Y },
    ] {
        let mut isolated = Parts::default();
        super::super::necklace_parts::emit(&mut isolated, neck, core, &anatomy_support);
        let mut composed = Parts::default();
        composed
            .metal
            .blob(Vec3::new(0.0, 0.7, 0.3), Vec3::splat(0.08), 1, 0.0, Vec3::Z);
        composed
            .satin
            .blob(Vec3::new(0.0, 0.7, 0.3), Vec3::splat(0.08), 1, 0.0, Vec3::Z);
        composed
            .matte
            .blob(Vec3::new(0.0, 0.7, 0.3), Vec3::splat(0.08), 1, 0.0, Vec3::Z);
        let prior: [Vec<_>; 3] = [&composed.metal, &composed.satin, &composed.matte]
            .map(|b| b.surface_triangles().collect());
        super::super::necklace_parts::emit(&mut composed, neck, core, &anatomy_support);
        for (i, (a, b)) in [&isolated.metal, &isolated.satin, &isolated.matte]
            .into_iter()
            .zip([&composed.metal, &composed.satin, &composed.matte])
            .enumerate()
        {
            let before = &prior[i];
            let after: Vec<_> = b.surface_triangles().collect();
            assert_eq!(&after[..before.len()], before);
            assert_eq!(
                after[before.len()..],
                a.surface_triangles().collect::<Vec<_>>()
            );
        }
        match neck {
            Neck::Teeth { .. } => {
                assert_eq!(isolated.metal.surface_triangles().count(), 0);
                // Seven distinct teeth remain, one central tip lower than all
                // others. The material source is independent of the cord.
                let mut tips: Vec<_> = isolated
                    .matte
                    .surface_triangles()
                    .flatten()
                    .filter(|p| p.y < core.upper_chest_anchor().y - 0.041)
                    .collect();
                tips.sort_by(|a, b| a.y.total_cmp(&b.y));
                assert!(!tips.is_empty());
                assert!(tips[0].x.abs() < 0.015);
            }
            Neck::Pendant { .. } => {
                assert_eq!(isolated.matte.surface_triangles().count(), 0);
                assert!(isolated.satin.surface_triangles().count() > 0);
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn hanging_teeth_keep_authored_lateral_splay_over_layered_support() {
    use super::super::necklace_parts::{NecklaceProfile, NecklaceSupport};
    let look = Look::with_choice(
        CreatureModel::Human,
        22,
        Some(Variant {
            woman: true,
            ..default()
        }),
    );
    let body = HumanoidBody::resolve(&look);
    let core = super::super::super::anatomy::TorsoCoreProfile::biped(body);
    let mut torso = torso_shape(body, Some(TorsoMaterial::Plate), true);
    // A positive layered fixture with clearly more depth than the small neck.
    torso.union(torso_shape(body, None, true), 0.0);
    let face = face_shape(&look, true).form;
    let support = NecklaceSupport::new(body, &torso, &face);
    let anchor = support.anchor();
    let ring = NecklaceProfile {
        anchor,
        front_drop: anchor.y - core.upper_chest_anchor().y,
        clearance: 0.004,
        radius: 0.005,
        stations: 64,
        sides: 6,
    }
    .resolve_field(&|at| support.distance(at))
    .unwrap();
    let mut parts = Parts::default();
    neck_collar(
        &mut parts,
        Neck::Teeth { cord: Vec3::Y },
        body,
        &torso,
        &look,
    );
    let points: Vec<_> = parts.matte.surface_triangles().flatten().collect();
    assert!(!points.is_empty());
    for k in -3i32..=3 {
        let angle = k as f32 * 0.32;
        let root = ring.mount(angle);
        let length = if k == 0 { 0.05 } else { 0.036 };
        let tip_x = root.x + angle.sin() * 0.008;
        let tip_y = root.y - length;
        assert!(
            points
                .iter()
                .any(|p| (p.x - tip_x).abs() < 1e-6 && (p.y - tip_y).abs() < 1e-6),
            "tooth {k} must retain authored tip x={tip_x}, y={tip_y}"
        );
    }
    // Independent negative control for the rejected rule: constant radial
    // angle turns growing chest depth into unintended lateral displacement.
    let angle = 0.96f32;
    let root = ring.mount(angle);
    let old = surface_field_ray_hit(
        &|at| support.distance(at),
        Vec3::new(anchor.x, root.y - 0.036, anchor.z),
        Vec3::new(angle.sin(), 0.0, angle.cos()),
        0.004,
        0.45,
    )
    .unwrap();
    assert!(
        (old.x - (root.x + angle.sin() * 0.008)).abs() > 0.005,
        "fixture must expose the rejected radial tooth rule"
    );
}
