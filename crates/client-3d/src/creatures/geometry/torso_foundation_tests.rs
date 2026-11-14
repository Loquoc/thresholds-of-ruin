//! Freeze the pre-extraction foundation and the distinct consumer policies.
use super::*;
use crate::sdf::{Sculpt, Shape, Step};

fn legacy_body(figure: &Figure) -> Sculpt {
    let mut body = torso_shape(figure.body, None, figure.trim_neck);
    if let (None, Some(bust), Some(share)) = (
        figure.look.equipment.shirt,
        figure.body.bust,
        figure.outfit.bodice.support.share(),
    ) {
        bust_underbridge(&mut body, &bust, share, 0.0, 0.0);
    }
    body
}

// Exact operation trees, not sparse distance samples. New operation variants
// require a conscious extension of this oracle.
fn assert_shape(a: Shape, b: Shape) {
    match (a, b) {
        (Shape::Ellipsoid(ac, ar), Shape::Ellipsoid(bc, br)) => assert_eq!((ac, ar), (bc, br)),
        (Shape::Capsule(aa, ab, ar, az), Shape::Capsule(ba, bb, br, bz)) => {
            assert_eq!((aa, ab, ar, az), (ba, bb, br, bz));
        }
        (Shape::Torus(ac, ar, az), Shape::Torus(bc, br, bz)) => {
            assert_eq!((ac, ar, az), (bc, br, bz))
        }
        (Shape::Beyond(an, ad), Shape::Beyond(bn, bd)) => assert_eq!((an, ad), (bn, bd)),
        _ => panic!("shape variant changed"),
    }
}
fn assert_sculpt(a: &Sculpt, b: &Sculpt) {
    assert_eq!(a.steps.len(), b.steps.len(), "operation count");
    for (a, b) in a.steps.iter().zip(&b.steps) {
        match a {
            Step::Add(shape, k) => match b {
                Step::Add(other, q) => {
                    assert_eq!(k, q);
                    assert_shape(*shape, *other);
                }
                _ => panic!("addition changed"),
            },
            Step::Carve(shape, k) => match b {
                Step::Carve(other, q) => {
                    assert_eq!(k, q);
                    assert_shape(*shape, *other);
                }
                _ => panic!("carving changed"),
            },
            Step::Inflate(k) => match b {
                Step::Inflate(q) => assert_eq!(k, q),
                _ => panic!("inflation changed"),
            },
            Step::Union(layer, k) => match b {
                Step::Union(other, q) => {
                    assert_eq!(k, q);
                    assert_sculpt(layer, other);
                }
                _ => panic!("union changed"),
            },
        }
    }
}
fn look(woman: bool, build: u8) -> Look {
    Look::with_choice(
        CreatureModel::Human,
        17,
        Some(Variant {
            woman,
            build,
            hair: 0,
            ..default()
        }),
    )
}

#[test]
fn shared_foundation_preserves_every_native_support_and_shirt_choice() {
    let mut bridged = 0;
    let mut replaced = 0;
    for woman in [false, true] {
        for build in 0..3 {
            for shirt in [
                None,
                Some(outfit::shirts::ShirtAppearance::TShirt),
                Some(outfit::shirts::ShirtAppearance::HawaiianShirt),
            ] {
                let mut look = look(woman, build);
                look.equipment.shirt = shirt;
                let mut figure = Figure::resolve(HumanoidBody::resolve(&look), &look);
                for trim_neck in [false, true] {
                    figure.trim_neck = trim_neck;
                    for support in [
                        outfit::Support::None,
                        outfit::Support::Loose,
                        outfit::Support::Structured,
                    ] {
                        figure.outfit.bodice.support = support;
                        let expected = legacy_body(&figure);
                        assert_sculpt(&figure.body_support(), &expected);
                        assert_sculpt(
                            &figure.inner_torso_support(),
                            figure.shirt_shape.as_ref().unwrap_or(&expected),
                        );
                        let skin = torso_shape(figure.body, None, trim_neck);
                        if shirt.is_none() && woman && support.share().is_some() {
                            assert!(
                                expected.steps.len() > skin.steps.len(),
                                "native bodice fixture must add support"
                            );
                            bridged += 1;
                        } else {
                            assert_sculpt(&expected, &skin);
                        }
                        if shirt.is_some() {
                            assert!(figure.shirt_shape.is_some());
                            replaced += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(bridged, 12);
    assert_eq!(replaced, 72);
}

#[test]
fn shared_foundation_preserves_distinct_outerwear_and_baldric_recipes() {
    for woman in [false, true] {
        for build in [0, 2] {
            for shirt in [None, Some(outfit::shirts::ShirtAppearance::HawaiianShirt)] {
                let mut look = look(woman, build);
                look.equipment.shirt = shirt;
                look.equipment.select("plate-mail").unwrap();
                let mut figure = Figure::resolve(HumanoidBody::resolve(&look), &look);
                // Explicit coexistence witness: outerwear retains every shell,
                // while baldric keeps native plate precedence over the shell.
                let mut shell = Sculpt::default();
                shell.add(
                    Shape::Ellipsoid(Vec3::new(0.0, 0.26, -0.12), Vec3::new(0.24, 0.23, 0.14)),
                    0.0,
                );
                figure.shell_shape = Some(shell);
                figure.native_plate = Some(torso_garment(
                    figure.body,
                    TorsoMaterial::Plate,
                    figure.trim_neck,
                ));
                assert!(figure.equipped_plate.is_some());
                assert!(figure.outfit.baldric.is_some());
                let skin = figure
                    .shirt_shape
                    .clone()
                    .unwrap_or_else(|| legacy_body(&figure));
                let mut outer = skin.clone();
                for layer in [
                    &figure.shell_shape,
                    &figure.native_plate,
                    &figure.equipped_plate,
                ] {
                    outer.union(layer.as_ref().unwrap().clone(), 0.0);
                }
                assert_sculpt(&figure.outerwear_support(), &outer);
                assert!(
                    figure.baldric.get().is_none(),
                    "outerwear must not force strap fitting"
                );
                let mut strap_support =
                    torso_layer_support(&skin, figure.native_plate.as_ref().unwrap());
                strap_support.union(figure.equipped_plate.as_ref().unwrap().clone(), 0.0);
                let expected = fitting::FittedBaldric::resolve(
                    &strap_support,
                    figure.body,
                    figure.outfit.baldric.unwrap(),
                    EMBLEM_FIELD_OFFSET,
                );
                let actual = figure.baldric().unwrap();
                assert_sculpt(&actual.support, &expected.support);
                let mut a = Parts::default();
                let mut b = Parts::default();
                actual.emit_covered(&mut a, figure.equipped_plate.as_ref());
                expected.emit_covered(&mut b, figure.equipped_plate.as_ref());
                for (a, b) in [&a.matte, &a.metal, &a.satin, &a.sheer, &a.ghost, &a.glow]
                    .into_iter()
                    .zip([&b.matte, &b.metal, &b.satin, &b.sheer, &b.ghost, &b.glow])
                {
                    a.assert_same_recipe(b);
                }
            }
        }
    }
}

#[test]
fn native_vest_rounding_reaches_the_actual_torso_without_moving_rivets() {
    for woman in [false, true] {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Hobgoblin,
                22,
                Some(Variant {
                    woman,
                    build,
                    ..default()
                }),
            );
            let body = HumanoidBody::resolve(&look);
            let figure = Figure::resolve(body, &look);
            let inner = figure.inner_torso_support();
            let shell = figure.shell_shape.as_ref().expect("native vest shell");
            // Freeze the original consumer policy as a comparison, not the new
            // smooth union or new fitting helper as an expected-value oracle.
            let hard = torso_layer_support(&inner, shell);
            let mut old = Parts::default();
            ornament_parts::emit(
                &mut old,
                figure.outfit.torso_ornament.unwrap(),
                body,
                &hard,
                &inner,
                None,
            );
            let mut actual = Parts::default();
            torso_parts(&mut actual, &figure);
            for side in [-1.0, 1.0] {
                let x = side * 0.11 * body.build;
                let ray = Vec3::new(x, 0.444, 0.0);
                let a = actual
                    .satin
                    .outer_surface(ray, Vec3::Z)
                    .expect("actual shoulder over bib edge");
                let b = old
                    .satin
                    .outer_surface(ray, Vec3::Z)
                    .expect("original shoulder witness");
                assert!(a.z > b.z + 1e-5, "woman {woman} build {build}: actual torso still contains the hard layer crease ({a:?} versus {b:?})");
            }
            // Select genuinely inactive support by field separation, not a
            // guessed height. The extra 3 mm covers both sides of the normal
            // estimator's 1.5 mm stencil. Their surface equations are unchanged,
            // but the ray marcher can take different steps before reaching them.
            let triangles: Vec<_> = actual.satin.surface_triangles().collect();
            let stable: Vec<_> = old
                .satin
                .surface_triangles()
                .filter(|tri| {
                    tri.iter()
                        .all(|&point| (inner.distance(point) - shell.distance(point)).abs() > 0.024)
                })
                .collect();
            assert!(
                !stable.is_empty(),
                "positive unchanged-field shoulder fixture"
            );
            // Use the project's existing one-millimeter physical geometry
            // allowance, not bit equality or a field residual mislabelled as
            // physical distance. Measured inactive-region differences are
            // at most 0.458 mm across these six figures.
            for tri in stable {
                assert!(triangles.iter().any(|candidate| {
                    tri.iter().all(|point| candidate.iter().any(|other| point.distance(*other) <= 0.001))
                }), "woman {woman} build {build}: shoulder face outside the blend changed by more than one millimeter: {tri:?}");
            }
            let metal: Vec<_> = actual.metal.surface_triangles().collect();
            for tri in old.metal.surface_triangles() {
                assert!(
                    metal.contains(&tri),
                    "rivets must retain original layer support"
                );
            }
        }
    }
}

#[test]
fn sleeveless_vest_covers_native_lower_rib_foundation() {
    for woman in [false, true] {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Hobgoblin,
                22,
                Some(Variant {
                    woman,
                    build,
                    ..default()
                }),
            );
            let body = HumanoidBody::resolve(&look);
            let figure = Figure::resolve(body, &look);
            let foundation = legacy_body(&figure);
            let shell = outfit_shell(body, &figure.outfit, false).unwrap();
            for y in 0..21 {
                for k in 0..21 {
                    let a = (60.0 + k as f32 * 1.5).to_radians();
                    for side in [-1.0, 1.0] {
                        let dir = Vec3::new(side * a.sin(), 0.0, a.cos());
                        if let Some(p) = surface_ray_hit(
                            &foundation,
                            Vec3::new(0.0, 0.26 + y as f32 * 0.006, 0.0),
                            dir,
                            0.0,
                            0.45,
                        ) {
                            assert!(
                                shell.distance(p) < -0.002,
                                "{woman} {build}: lower rib foundation outside shell at {p:?}: {}",
                                shell.distance(p)
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn shell_side_clipping_begins_only_above_the_shared_armhole_landmark() {
    for woman in [false, true] {
        for build in 0..3 {
            for model in [
                CreatureModel::Hobgoblin,
                CreatureModel::Orc,
                CreatureModel::Scout,
                CreatureModel::Human,
            ] {
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
                let outfit = Outfit::worn(&look);
                let Some(Outer::Shell { top, underbust, .. }) = outfit.outer else {
                    continue;
                };
                let actual = outfit_shell(body, &outfit, false).unwrap();
                let low = shell_top(body, top, underbust) <= ARMPIT;
                let side_cuts = actual
                    .steps
                    .iter()
                    .filter(|s| matches!(s,Step::Carve(Shape::Beyond(n,_),_) if n.x.abs()>0.99))
                    .count();
                assert_eq!(
                    side_cuts,
                    if low { 0 } else { 2 },
                    "{model:?} {woman} {build}"
                );
                if low {
                    let mut full = outfit;
                    if let Some(Outer::Shell {
                        ref mut sleeveless, ..
                    }) = full.outer
                    {
                        *sleeveless = false;
                    }
                    assert_sculpt(&actual, &outfit_shell(body, &full, false).unwrap());
                }
            }
        }
    }
}
