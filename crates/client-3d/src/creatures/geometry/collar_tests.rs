use super::*;

#[test]
fn pharaoh_collar_emitted_triangles_clear_actual_body() {
    let mut violations = Vec::new();
    for model in [
        CreatureModel::Human,
        CreatureModel::Orc,
        CreatureModel::Nymph,
        CreatureModel::Hobgoblin,
        CreatureModel::Troll,
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
                let face = face_shape(
                    &look,
                    !matches!(look.style, Style::Hobgoblin | Style::Troll),
                )
                .form;
                let composed_support =
                    super::super::necklace_parts::NecklaceSupport::new(body, &support, &face);
                for neck in [Neck::Collar {
                    metal: Vec3::ONE,
                    beads: Vec3::X,
                }] {
                    for smooth in [false, true] {
                        let mut parts = Parts::default();
                        if smooth {
                            parts.soften();
                        }
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
                                        "pharaoh collar intersects actual head neck: {head:?}, distance {}", face.distance(head)
                                    );
                                    minimum = minimum.min(composed_support.distance(p));
                                    maximum = maximum.max(composed_support.distance(p));
                                    count += 1;
                                }
                            }
                        }
                        eprintln!("pharaoh collar {model:?} woman={woman} build={build} {neck:?}: minimum {minimum:.6}, maximum {maximum:.6}, samples {count}");
                        if count == 0 || minimum < 0.001 || maximum > 0.030 {
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
fn coupled_collar_keeps_adjacent_tiers_fringe_and_closed_seams() {
    use super::super::necklace_parts::{CollarProfile, NecklaceProfile};
    use crate::sdf::{Sculpt, Shape};
    let mut support = Sculpt::default();
    support.add(
        Shape::Ellipsoid(Vec3::ZERO, Vec3::new(0.17, 0.22, 0.13)),
        0.0,
    );
    let profile = CollarProfile {
        inner: NecklaceProfile {
            anchor: Vec3::Y * 0.16,
            front_drop: 0.01,
            clearance: 0.004,
            radius: 0.011,
            stations: 64,
            sides: 6,
        },
        tier_spacing: 0.024,
    };
    let collar = profile
        .resolve(&|at| support.distance(at))
        .expect("positive supported collar");
    assert_eq!(collar.tiers.len(), 3);
    assert_eq!(collar.fringe.len(), 13);
    for tiers in collar.tiers.windows(2) {
        assert_eq!(tiers[0].path.len(), tiers[1].path.len());
        for i in 0..tiers[0].path.len() {
            let j = (i + 1) % tiers[0].path.len();
            for t in [0.0, 0.25, 0.5, 0.75] {
                let upper = tiers[0].path[i].lerp(tiers[0].path[j], t);
                let lower = tiers[1].path[i].lerp(tiers[1].path[j], t);
                assert!(lower.y < upper.y);
                assert!(
                    (0.0235..=0.0243).contains(&lower.distance(upper)),
                    "adjacent tier spacing {}",
                    lower.distance(upper)
                );
            }
        }
    }
    let mut outer_ring = Builder::default();
    collar.tiers[2].emit(&mut outer_ring, Vec3::ONE);
    for (i, &(root, center, tangent)) in collar.fringe.iter().enumerate() {
        let angle = (i as f32 - 6.0) * 0.2;
        assert_eq!(root, collar.tiers[2].mount(angle));
        assert!((center.distance(root) - 0.021).abs() < 0.0003);
        assert!(center.y < root.y && tangent.is_normalized());
        let down = (center - root).normalize();
        let ring_wall = outer_ring
            .outer_surface(root, down)
            .expect("own outer ring cross-section");
        assert!(
            ring_wall.distance(root) > center.distance(root) - 0.012 + 0.0001,
            "actual fringe pole must overlap the emitted outer ring"
        );
    }
    // Exact closed, directed edges on every actually emitted ring.
    for ring in &collar.tiers {
        let mut mesh = Builder::default();
        ring.emit(&mut mesh, Vec3::ONE);
        assert_eq!(mesh.surface_triangles().count(), ring.path.len() * 12);
        let key = |p: Vec3| p.to_array().map(|x| (x * 1e7).round() as i32);
        let mut edges = std::collections::BTreeMap::new();
        for tri in mesh.surface_triangles() {
            assert!((tri[1] - tri[0]).cross(tri[2] - tri[0]).length() > 1e-9);
            for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                *edges.entry((key(a), key(b))).or_insert(0) += 1;
            }
        }
        for ((a, b), count) in &edges {
            assert_eq!(*count, 1);
            assert_eq!(edges.get(&(*b, *a)), Some(&1));
        }
    }
    let offset = Vec3::new(0.031, 0.07, -0.04);
    let moved = CollarProfile {
        inner: NecklaceProfile {
            anchor: profile.inner.anchor + offset,
            ..profile.inner
        },
        ..profile
    }
    .resolve(&|at| support.distance(at - offset))
    .unwrap();
    let mut a = Parts::default();
    collar.emit(&mut a, Vec3::ONE, Vec3::X);
    let mut b = Parts::default();
    moved.emit(&mut b, Vec3::ONE, Vec3::X);
    for (original, translated) in [&a.metal, &a.satin].into_iter().zip([&b.metal, &b.satin]) {
        let x: Vec<_> = original.surface_triangles().flatten().collect();
        let y: Vec<_> = translated.surface_triangles().flatten().collect();
        assert_eq!(x.len(), y.len());
        for (x, y) in x.into_iter().zip(y) {
            assert!((x + offset - y).length() < 0.00002);
        }
    }
    // Positive fixture first, then independent rejection mutations.
    assert!(profile.resolve(&|_| f32::INFINITY).is_none());
    assert!(profile.resolve(&|_| f32::NAN).is_none());
    assert!(profile
        .resolve(&|at| if at.y < 0.14 {
            f32::NAN
        } else {
            support.distance(at)
        })
        .is_none());
    for bad in [-1.0, 0.01, 0.1, f32::NAN] {
        assert!(CollarProfile {
            tier_spacing: bad,
            ..profile
        }
        .resolve(&|at| support.distance(at))
        .is_none());
    }
}

#[test]
fn collar_mounts_ignore_prior_accessories_and_preserve_them() {
    let look = Look::with_choice(CreatureModel::Human, 22, None);
    let body = HumanoidBody::resolve(&look);
    let support = torso_shape(body, None, true);
    let neck = Neck::Collar {
        metal: Vec3::ONE,
        beads: Vec3::X,
    };
    let mut isolated = Parts::default();
    neck_collar(&mut isolated, neck, body, &support, &look);
    assert!(!isolated.metal.is_empty() && !isolated.satin.is_empty());
    let mut dressed = Parts::default();
    for b in [&mut dressed.metal, &mut dressed.satin] {
        b.blob(Vec3::new(0.0, 0.56, 0.1), Vec3::splat(0.1), 1, 0.0, Vec3::Y);
    }
    let prior = [&dressed.metal, &dressed.satin].map(|b| b.surface_triangles().collect::<Vec<_>>());
    neck_collar(&mut dressed, neck, body, &support, &look);
    for (i, (expected, actual)) in [&isolated.metal, &isolated.satin]
        .into_iter()
        .zip([&dressed.metal, &dressed.satin])
        .enumerate()
    {
        let actual: Vec<_> = actual.surface_triangles().collect();
        assert_eq!(&actual[..prior[i].len()], prior[i]);
        assert_eq!(
            actual[prior[i].len()..],
            expected.surface_triangles().collect::<Vec<_>>()
        );
    }
}

#[test]
fn coupled_collar_supports_vertical_sections_at_translated_heights() {
    use super::super::necklace_parts::{CollarProfile, NecklaceProfile};
    use crate::sdf::{Sculpt, Shape};
    for offset in [Vec3::ZERO, Vec3::new(0.023, 0.07, -0.04)] {
        let mut cylinder = Sculpt::default();
        cylinder.add(
            Shape::Capsule(offset, offset + Vec3::Y * 1.2, 0.10, 0.10),
            0.0,
        );
        for height in [0.3, 0.55, 0.56, 0.57, 0.58, 0.6] {
            let profile = CollarProfile {
                inner: NecklaceProfile {
                    anchor: offset + Vec3::Y * height,
                    front_drop: 0.0,
                    clearance: 0.004,
                    radius: 0.011,
                    stations: 64,
                    sides: 6,
                },
                tier_spacing: 0.024,
            };
            for scale in [1.0, 0.98] {
                let field = |p| cylinder.distance(p) * scale;
                // The single ring confirms complete support under the shared
                // ray helper's existing 0.2 mm convergence contract first.
                assert!(profile.inner.resolve_field(&field).is_some());
                let fitted = profile.resolve(&field).unwrap_or_else(|| panic!("supported cylindrical collar omitted at height {height}, offset {offset:?}, field scale {scale}"));
                assert_eq!(fitted.tiers.len(), 3);
                for (i, tier) in fitted.tiers.iter().enumerate() {
                    for &p in &tier.path {
                        assert!((p.y - (offset.y + height - i as f32 * 0.024)).abs() < 0.00002);
                        assert!(
                            (Vec2::new(p.x - offset.x, p.z - offset.z).length()
                                - (0.10 + 0.015 / scale))
                                .abs()
                                < 0.0003
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn coupled_collar_crosses_connected_shoulder_valley_without_jumping_tiers() {
    use super::super::necklace_parts::{CollarProfile, NecklaceProfile};
    use crate::sdf::{Sculpt, Shape};
    let profile = CollarProfile {
        inner: NecklaceProfile {
            anchor: Vec3::Y * 0.620,
            front_drop: 0.0,
            clearance: 0.004,
            radius: 0.011,
            stations: 64,
            sides: 6,
        },
        tier_spacing: 0.024,
    };
    let mut support = Sculpt::default();
    support.add(Shape::Capsule(Vec3::ZERO, Vec3::Y, 0.060, 0.060), 0.0);
    support.add(
        Shape::Torus(Vec3::Y * 0.520, Vec2::new(0.110, 0.070), 1.0),
        0.0,
    );
    assert!(profile.inner.resolve(&support).is_some());
    let collar = profile
        .resolve(&|p| support.distance(p))
        .expect("connected neck/shoulder collar");
    let a = collar.tiers[0].mount(0.0);
    let b = collar.tiers[1].mount(0.0);
    let c = collar.tiers[2].mount(0.0);
    assert!((a.z - 0.075).abs() < 0.0003 && (a.y - 0.620).abs() < 0.0003);
    // Analytic circle/torus-level intersection, independent of the resolver.
    assert!((b.z - 0.0920169).abs() < 0.0006 && (b.y - 0.6030759).abs() < 0.0006);
    assert!((a.distance(b) - 0.024).abs() < 0.0003);
    assert!(
        c.z > b.z && c.y > b.y,
        "outer tier must rise onto the shoulder"
    );
    assert!((b.distance(c) - 0.024).abs() < 0.0003);
    // A nearby detached lower ring must not be used to bypass a route gap.
    let mut detached = Sculpt::default();
    detached.add(
        Shape::Capsule(Vec3::Y * 0.590, Vec3::Y * 0.7, 0.060, 0.060),
        0.0,
    );
    detached.add(
        Shape::Torus(Vec3::Y * 0.550, Vec2::new(0.110, 0.008), 1.0),
        0.0,
    );
    assert!(profile.inner.resolve(&detached).is_some());
    assert!(profile.resolve(&|p| detached.distance(p)).is_none());
}

#[test]
fn collar_seat_moves_whole_profile_within_its_bounded_anatomical_window() {
    use super::super::necklace_parts::{CollarProfile, NecklaceProfile, NecklaceSupport};
    for woman in [false, true] {
        let look = Look::with_choice(
            CreatureModel::Human,
            22,
            Some(Variant {
                woman,
                build: 0,
                ..default()
            }),
        );
        let body = HumanoidBody::resolve(&look);
        let torso = torso_shape(body, None, true);
        let face = face_shape(&look, true).form;
        let support = NecklaceSupport::new(body, &torso, &face);
        let anchor = support.anchor() + Vec3::Y * 0.010;
        let core = super::super::super::anatomy::TorsoCoreProfile::biped(body);
        let profile = CollarProfile {
            inner: NecklaceProfile {
                anchor,
                front_drop: anchor.y - (core.upper_chest_anchor().y + 0.030),
                clearance: 0.004,
                radius: 0.011,
                stations: 64,
                sides: 6,
            },
            tier_spacing: 0.024,
        };
        let base = profile.resolve(&|p| support.distance(p));
        let (fitted, drop) = profile
            .resolve_seated(&|p| support.distance(p))
            .expect("bounded anatomical seat");
        eprintln!("Anatomical collar seat woman={woman}: downward adjustment {drop:.6} m");
        assert!((0.0..=0.012001).contains(&drop));
        assert!(
            (fitted.tiers[0].mount(0.0).y - (anchor.y - profile.inner.front_drop - drop)).abs()
                < 1e-6
        );
        assert!((fitted.tiers[0].mount(PI).y - (anchor.y - drop)).abs() < 1e-6);
        if let Some(base) = base {
            assert_eq!(drop, 0.0);
            for (a, b) in base.tiers.iter().zip(&fitted.tiers) {
                assert_eq!(a.path, b.path);
            }
            assert_eq!(base.fringe, fitted.fringe);
        } else {
            assert!(drop > 0.0);
        }
    }
    use crate::sdf::{Sculpt, Shape};
    let mut cylinder = Sculpt::default();
    cylinder.add(Shape::Capsule(Vec3::ZERO, Vec3::Y, 0.10, 0.10), 0.0);
    let profile = CollarProfile {
        inner: NecklaceProfile {
            anchor: Vec3::Y * 0.160,
            front_drop: 0.0,
            clearance: 0.004,
            radius: 0.011,
            stations: 64,
            sides: 6,
        },
        tier_spacing: 0.024,
    };
    let field = |p: Vec3| {
        if p.y > 0.140001 {
            f32::NAN
        } else {
            cylinder.distance(p)
        }
    };
    let lower = CollarProfile {
        inner: NecklaceProfile {
            anchor: Vec3::Y * 0.14,
            ..profile.inner
        },
        ..profile
    };
    assert!(
        lower.resolve(&field).is_some(),
        "positive supported lower seat"
    );
    assert!(
        profile.resolve_seated(&field).is_none(),
        "must not search beyond 12 mm"
    );
}
