use super::*;

fn wearer(model: CreatureModel, woman: bool, build: u8) -> (Look, HumanoidBody) {
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
    (look, body)
}

#[test]
fn native_shoulder_straps_seat_on_the_actual_shoulder_instead_of_hovering() {
    for model in [CreatureModel::Hobgoblin, CreatureModel::Troll] {
        for woman in [false, true] {
            for build in 0..3 {
                let (look, body) = wearer(model, woman, build);
                let skin = torso_shape(body, None, true);
                let garment = torso_garment(body, TorsoMaterial::Cloth, false);
                let support = if model == CreatureModel::Troll {
                    &skin
                } else {
                    &garment
                };
                let mut parts = Parts::default();
                ornament_parts::emit(
                    &mut parts,
                    Outfit::worn(&look).torso_ornament.unwrap(),
                    body,
                    &garment,
                    &skin,
                    None,
                );
                // Independent spatial sweep, not coordinates returned by the fitter.
                // Anatomy-relative straps may move laterally from the old tube's
                // literal x; require a real finite-width shoulder surface instead.
                for side in [-1.0, 1.0] {
                    let mut hits = 0;
                    for z in [-0.03, -0.015, 0.0] {
                        for i in 0..=105 {
                            let ray = Vec3::new(side * (0.06 + i as f32 * 0.002), 0.3, z);
                            if let Some(strap) = parts
                                .satin
                                .outer_surface(ray, Vec3::Y)
                                .filter(|p| p.y > 0.47)
                            {
                                let shoulder = surface_ray_hit(support, ray, Vec3::Y, 0.0, 0.4)
                                    .expect("actual shoulder support");
                                let gap = strap.y - shoulder.y;
                                assert!((0.004..=0.025).contains(&gap), "{model:?} woman {woman} build {build} side {side}: shoulder gap {gap}");
                                hits += 1;
                            }
                        }
                    }
                    assert!(hits >= 12, "{model:?} woman {woman} build {build} side {side}: actual finite-width shoulder strap must remain present, {hits} rays");
                }
            }
        }
    }
}

fn profiles() -> [ShoulderStrapProfile; 2] {
    [
        ShoulderStrapProfile {
            chest_fraction: 11.0 / 18.0,
            shoulder_fraction: 0.52,
            chest_drop: 0.10,
            half_width: 0.030,
        },
        ShoulderStrapProfile {
            chest_fraction: 5.0 / 9.0,
            shoulder_fraction: 0.48,
            chest_drop: 0.09,
            half_width: 0.032,
        },
    ]
}

#[test]
fn shoulder_recipe_tracks_separate_anatomical_anchors_and_preserves_style_width() {
    let (_, body) = wearer(CreatureModel::Human, true, 1);
    let core = super::super::anatomy::TorsoCoreProfile::biped(body);
    for recipe in profiles() {
        let original = recipe.path(core, 1.0);
        let mut moved = core;
        moved.shoulder += Vec3::new(0.07, 0.04, 0.0);
        moved.chest.upper.0.y += 0.03;
        moved.chest.upper.1.x *= 1.25;
        let shifted = recipe.path(moved, 1.0);
        assert!((shifted[0].0.x - original[0].0.x * 1.25).abs() < 1e-6);
        assert!((shifted[0].0.y - original[0].0.y - 0.03).abs() < 1e-6);
        assert!((shifted[1].0.x - original[1].0.x - 0.07 * recipe.shoulder_fraction).abs() < 1e-6);
        assert!((shifted[1].0.y - original[1].0.y - 0.04).abs() < 1e-6);
        for ((a, w), (b, z)) in shifted.into_iter().zip(recipe.path(moved, -1.0)) {
            assert_eq!(w, recipe.half_width);
            assert_eq!(w, z);
            assert_eq!(a, b * Vec2::new(-1.0, 1.0));
        }
    }
}

#[test]
fn fitted_shoulder_assembly_retains_finite_width_and_sampled_complete_surface_clearance() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Nymph,
        CreatureModel::Hobgoblin,
        CreatureModel::Troll,
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                let (_, body) = wearer(model, woman, build);
                let core = super::super::anatomy::TorsoCoreProfile::biped(body);
                for (recipe, layered) in profiles().into_iter().zip([true, false]) {
                    let mut support = torso_shape(body, None, true);
                    if layered {
                        support.inflate(0.025);
                    }
                    for side in [-1.0, 1.0] {
                        let path = recipe.path(core, side);
                        let fitted = FittedShoulderStrap::fit(&support, &path, 0.014);
                        let bridge = fitted
                            .bridge
                            .as_ref()
                            .expect("native/transferred shoulder has continuous support");
                        assert_eq!(fitted.front.strips.len(), 1);
                        assert_eq!(fitted.back.strips.len(), 1);
                        assert_eq!(
                            bridge.first().unwrap(),
                            fitted.front.strips[0].last().unwrap()
                        );
                        assert_eq!(
                            bridge.last().unwrap(),
                            fitted.back.strips[0].last().unwrap()
                        );
                        for rows in [&fitted.front.strips[0], &fitted.back.strips[0]] {
                            for row in rows {
                                let a = row.first().unwrap().0.truncate();
                                let b = row.last().unwrap().0.truncate();
                                assert!(
                                    (a.distance(b) - 2.0 * recipe.half_width).abs() < 1e-5,
                                    "fit cannot pinch the style width"
                                );
                            }
                        }
                        let mut p = Parts::default();
                        fitted.emit(&mut p.satin, Vec3::ONE);
                        assert!(
                            p.satin.surface_triangles().count() > 100,
                            "actual complete cloth witness"
                        );
                        for t in p.satin.surface_triangles() {
                            for point in [t[0], t[1], t[2], (t[0] + t[1] + t[2]) / 3.0] {
                                let d = support.distance(point);
                                assert!((0.002..=0.017).contains(&d),"{model:?} {woman} {build} {side} layered {layered}: sampled clearance {d} at {point}");
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn fitted_shoulder_assembly_is_closed_without_internal_seam_caps() {
    use std::collections::BTreeMap;
    let (_, body) = wearer(CreatureModel::Human, true, 2);
    let core = super::super::anatomy::TorsoCoreProfile::biped(body);
    let support = torso_shape(body, None, true);
    for recipe in profiles() {
        for side in [-1.0, 1.0] {
            let fit = FittedShoulderStrap::fit(&support, &recipe.path(core, side), 0.014);
            assert!(fit.bridge.is_some());
            let mut p = Parts::default();
            fit.emit(&mut p.satin, Vec3::ONE);
            let mut edges = BTreeMap::<([i32; 3], [i32; 3]), (usize, i32)>::new();
            let mut volume = 0.0;
            for t in p.satin.surface_triangles() {
                assert!(
                    (t[1] - t[0]).cross(t[2] - t[0]).length() > 1e-10,
                    "nondegenerate cloth triangle"
                );
                volume += t[0].dot(t[1].cross(t[2])) / 6.0;
                let p = t.map(|v| v.to_array().map(|x| (x * 1_000_000.0).round() as i32));
                for (a, b) in [(p[0], p[1]), (p[1], p[2]), (p[2], p[0])] {
                    let (key, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                    let e = edges.entry(key).or_default();
                    e.0 += 1;
                    e.1 += sign;
                }
            }
            assert!(
                edges.values().all(|e| *e == (2, 0)),
                "cloth edges must close with opposite winding, including shared inner seams"
            );
            assert!(volume > 0.0, "outward closed shell");
        }
    }
}

#[test]
fn missing_shoulder_terminal_keeps_strips_capped_instead_of_bridging_earlier_rows() {
    let (_, body) = wearer(CreatureModel::Human, false, 1);
    let core = super::super::anatomy::TorsoCoreProfile::biped(body);
    let path = profiles()[0].path(core, 1.0);
    let mut support = torso_shape(body, None, true);
    support.carve(crate::sdf::Shape::Beyond(Vec3::Y, 0.445), 0.0);
    let fit = FittedShoulderStrap::fit(&support, &path, 0.014);
    assert!(!fit.front.strips.is_empty() && !fit.back.strips.is_empty());
    assert!(
        fit.bridge.is_none(),
        "earlier row is not the shoulder endpoint"
    );
    let mut actual = Parts::default();
    fit.emit(&mut actual.satin, Vec3::ONE);
    let mut capped = Parts::default();
    fit.front.emit(&mut capped.satin, Vec3::ONE, true);
    fit.back.emit(&mut capped.satin, Vec3::ONE, true);
    assert_eq!(
        actual.satin.surface_triangles().collect::<Vec<_>>(),
        capped.satin.surface_triangles().collect::<Vec<_>>()
    );
    let empty = FittedShoulderStrap::fit(&crate::sdf::Sculpt::default(), &path, 0.014);
    let mut p = Parts::default();
    empty.emit(&mut p.satin, Vec3::ONE);
    assert!(empty.bridge.is_none() && p.satin.is_empty());
}

#[test]
fn harness_shoulder_option_preserves_the_original_diagonal_and_ring_geometry() {
    for model in [CreatureModel::Human, CreatureModel::Troll] {
        let (_, body) = wearer(model, true, 2);
        let skin = torso_shape(body, None, true);
        let garment = torso_garment(body, TorsoMaterial::Cloth, false);
        let mut bare = Parts::default();
        let mut strapped = Parts::default();
        for (p, shoulder_straps) in [
            (&mut bare, None),
            (&mut strapped, Some(Vec3::new(0.3, 0.05, 0.1))),
        ] {
            ornament_parts::emit(
                p,
                TorsoOrnament::Harness {
                    strap: Vec3::ONE,
                    ring: Vec3::ONE,
                    shoulder_straps,
                },
                body,
                &garment,
                &skin,
                None,
            );
        }
        let diagonal = bare.satin.surface_triangles().collect::<Vec<_>>();
        let full = strapped.satin.surface_triangles().collect::<Vec<_>>();
        assert!(!diagonal.is_empty());
        assert!(full.len() > diagonal.len());
        assert!(full.starts_with(&diagonal));
        assert_eq!(
            bare.metal.surface_triangles().collect::<Vec<_>>(),
            strapped.metal.surface_triangles().collect::<Vec<_>>()
        );
    }
}

#[test]
fn riveted_vest_keeps_its_metal_hardware_while_both_recipes_transfer_to_other_bodies() {
    let (hobgoblin, _) = wearer(CreatureModel::Hobgoblin, false, 1);
    let (troll, _) = wearer(CreatureModel::Troll, false, 1);
    for model in [CreatureModel::Human, CreatureModel::Nymph] {
        for woman in [false, true] {
            for build in 0..3 {
                let (_, body) = wearer(model, woman, build);
                let skin = torso_shape(body, None, true);
                let garment = torso_garment(body, TorsoMaterial::Cloth, false);
                for donor in [&hobgoblin, &troll] {
                    let recipe = Outfit::worn(donor).torso_ornament.unwrap();
                    let mut p = Parts::default();
                    ornament_parts::emit(&mut p, recipe, body, &garment, &skin, None);
                    for side in [-1.0, 1.0] {
                        let origin = Vec3::new(side * 0.12 * body.build, 0.3, -0.015);
                        let point = p
                            .satin
                            .outer_surface(origin, Vec3::Y)
                            .expect("actual transferred shoulder ribbon");
                        let support = if matches!(recipe, TorsoOrnament::RivetedVest { .. }) {
                            &garment
                        } else {
                            &skin
                        };
                        assert!(
                            (0.004..0.017).contains(&support.distance(point)),
                            "transferred shoulder stays fitted"
                        );
                    }
                    if let TorsoOrnament::RivetedVest { rivets, .. } = recipe {
                        let mut expected = Parts::default();
                        for k in 0..3 {
                            expected.metal.blob(
                                torso_surface(&garment, -0.1 + k as f32 * 0.1, 0.16, true),
                                Vec3::splat(0.012),
                                0,
                                0.0,
                                rivets,
                            );
                        }
                        assert_eq!(
                            p.metal.surface_triangles().collect::<Vec<_>>(),
                            expected.metal.surface_triangles().collect::<Vec<_>>()
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn slim_male_troll_preserves_the_original_first_red_shoulder_contact_witness() {
    let (look, body) = wearer(CreatureModel::Troll, false, 0);
    let skin = torso_shape(body, None, true);
    let garment = torso_garment(body, TorsoMaterial::Cloth, false);
    let mut parts = Parts::default();
    ornament_parts::emit(
        &mut parts,
        Outfit::worn(&look).torso_ornament.unwrap(),
        body,
        &garment,
        &skin,
        None,
    );
    for side in [-1.0, 1.0] {
        let ray = Vec3::new(side * 0.12 * body.build, 0.3, -0.015);
        let strap = parts
            .satin
            .outer_surface(ray, Vec3::Y)
            .expect("original first-red ray still intersects actual strap");
        let shoulder = surface_ray_hit(&skin, ray, Vec3::Y, 0.0, 0.4).unwrap();
        assert!(
            (0.004..=0.025).contains(&(strap.y - shoulder.y)),
            "original baseline gap was 33.014 mm"
        );
    }
}

#[test]
fn harness_ring_seats_on_emitted_ribbon_without_penetrating_it() {
    let mut reports = Vec::new();
    let mut failures = Vec::new();
    for model in [
        CreatureModel::Troll,
        CreatureModel::Orc,
        CreatureModel::Human,
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                let (_, body) = wearer(model, woman, build);
                let skin = torso_shape(body, None, true);
                let garment = torso_garment(body, TorsoMaterial::Cloth, false);
                let mut parts = Parts::default();
                ornament_parts::emit(
                    &mut parts,
                    TorsoOrnament::Harness {
                        strap: Vec3::ONE,
                        ring: Vec3::ONE,
                        shoulder_straps: None,
                    },
                    body,
                    &garment,
                    &skin,
                    None,
                );
                let metal: Vec<_> = parts.metal.surface_triangles().collect();
                let mut original = Builder::default();
                original.torus(Vec3::ZERO, Vec3::Z, 0.03, 0.008, 14, 4, Vec3::ONE);
                let canonical: Vec<_> = original.surface_triangles().flatten().collect();
                let actual: Vec<_> = metal.iter().flatten().copied().collect();
                assert_eq!(actual.len(), canonical.len(), "unchanged ring topology");
                for (i, point) in actual.iter().enumerate() {
                    assert!(point.is_finite(), "finite fitted hardware");
                    for j in i + 1..actual.len() {
                        assert!(
                            (point.distance(actual[j]) - canonical[i].distance(canonical[j])).abs()
                                < 0.00001,
                            "ring retains its rigid dimensions, not a warped projection"
                        );
                    }
                }
                let (low, high) = metal
                    .iter()
                    .flatten()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), p| {
                        (low.min(p.y), high.max(p.y))
                    });
                let middle = (low + high) * 0.5;
                let mut minimum = [f32::INFINITY; 2];
                let mut hits = [0usize; 2];
                let mut worst_point = Vec3::ZERO;
                let mut worst_gap = f32::INFINITY;
                let mut body_gap = f32::INFINITY;
                // Sample actual triangle interiors as well as vertices. The
                // support oracle is emitted cloth, not the fitted attachment
                // frame, sculpt, nominal clearance, or duplicated ring formula.
                // A +Z ray from the torso interior excludes the rear ribbon.
                for tri in &metal {
                    for i in 0..=8 {
                        for j in 0..=8 - i {
                            let point = tri[0]
                                + (tri[1] - tri[0]) * (i as f32 / 8.0)
                                + (tri[2] - tri[0]) * (j as f32 / 8.0);
                            body_gap = body_gap.min(garment.distance(point));
                            let origin = Vec3::new(point.x, point.y, 0.0);
                            let Some(cloth) = parts.satin.outer_surface(origin, Vec3::Z) else {
                                continue;
                            };
                            let gap = point.z - cloth.z;
                            let half = usize::from(point.y >= middle);
                            minimum[half] = minimum[half].min(gap);
                            hits[half] += 1;
                            if gap < worst_gap {
                                worst_gap = gap;
                                worst_point = point;
                            }
                        }
                    }
                }
                // Both annulus/strap crossing regions must have support and
                // near contact. The central hole is deliberately not an oracle.
                // Only 10 micrometres of numerical ray tolerance is allowed;
                // millimetre-scale penetration cannot count as attachment.
                let seated = (0..2)
                    .all(|half| hits[half] >= 3 && (-0.00001..=0.003).contains(&minimum[half]));
                let report = format!(
                    "{model:?} woman {woman} build {build}: lower/upper contact gaps {:?} mm, samples {hits:?}, garment gap {body_gap} m, worst point {worst_point:?}",
                    minimum.map(|gap| gap * 1000.0),
                );
                if !seated {
                    failures.push(report.clone());
                }
                reports.push(report);
            }
        }
    }
    eprintln!(
        "Emitted harness ring contact (all 18 fixtures):\n{}",
        reports.join("\n")
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn harness_ring_attachment_preserves_the_authored_diagonal_ribbons() {
    for model in [
        CreatureModel::Troll,
        CreatureModel::Orc,
        CreatureModel::Human,
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                let (_, body) = wearer(model, woman, build);
                let skin = torso_shape(body, None, true);
                let garment = torso_garment(body, TorsoMaterial::Cloth, false);
                let mut actual = Parts::default();
                ornament_parts::emit(
                    &mut actual,
                    TorsoOrnament::Harness {
                        strap: Vec3::ONE,
                        ring: Vec3::ONE,
                        shoulder_straps: None,
                    },
                    body,
                    &garment,
                    &skin,
                    None,
                );
                // Frozen pre-migration cloth recipe, not an attachment oracle.
                let b = body.build;
                let sh = body.shoulders * b;
                let w = body.waist * b;
                let path = [
                    (Vec2::new(0.15 * sh, 0.45), 0.026 * b),
                    (Vec2::new(0.05 * b, 0.33), 0.026 * b),
                    (Vec2::new(-0.06 * b, 0.21), 0.026 * b),
                    (Vec2::new(-0.13 * w, 0.05), 0.022 * b),
                ];
                let mut expected = Parts::default();
                for front in [true, false] {
                    fitted_ribbon(
                        &mut expected.satin,
                        &garment,
                        &path,
                        front,
                        0.016,
                        Vec3::ONE,
                    );
                }
                actual.satin.assert_same_recipe(&expected.satin);
            }
        }
    }
}

#[test]
fn harness_ring_is_absent_when_its_ribbon_has_no_support() {
    let (_, body) = wearer(CreatureModel::Human, false, 1);
    let empty = crate::sdf::Sculpt::default();
    let mut parts = Parts::default();
    ornament_parts::emit(
        &mut parts,
        TorsoOrnament::Harness {
            strap: Vec3::ONE,
            ring: Vec3::ONE,
            shoulder_straps: None,
        },
        body,
        &empty,
        &empty,
        None,
    );
    assert!(
        parts.satin.is_empty(),
        "unsupported ribbon fixture must be empty"
    );
    assert!(
        parts.metal.is_empty(),
        "no ring may float without its supporting ribbon"
    );
}

// Independent bisection avoids the production ray's early-exit tolerance.
fn precise_front(form: &crate::sdf::Sculpt, x: f32, y: f32) -> Vec3 {
    let (mut inside, mut outside) = (0.0, 0.5);
    assert!(form.distance(Vec3::new(x, y, inside)) < 0.0);
    for _ in 0..24 {
        let middle = (inside + outside) * 0.5;
        if form.distance(Vec3::new(x, y, middle)) <= 0.0 {
            inside = middle;
        } else {
            outside = middle;
        }
    }
    Vec3::new(x, y, (inside + outside) * 0.5)
}

fn fine_normal(form: &crate::sdf::Sculpt, at: Vec3) -> Vec3 {
    let e = 0.00001;
    Vec3::new(
        form.distance(at + Vec3::X * e) - form.distance(at - Vec3::X * e),
        form.distance(at + Vec3::Y * e) - form.distance(at - Vec3::Y * e),
        form.distance(at + Vec3::Z * e) - form.distance(at - Vec3::Z * e),
    )
    .normalize()
}

#[test]
fn vest_support_has_no_normal_jump_where_the_bib_meets_the_underlayer() {
    for build in 0..3 {
        let (look, body) = wearer(CreatureModel::Hobgoblin, false, build);
        let skin = torso_shape(body, None, false);
        let shell = outfit_shell(body, &Outfit::worn(&look), false).unwrap();
        let mut hard = torso_layer_support(&skin, &shell);
        hard.inflate(0.014);
        let mut support = torso_layer_transition_support(&skin, &shell);
        support.inflate(0.014);
        for x in [0.095, 0.11, 0.125].map(|x| x * body.build) {
            let difference = |y| {
                let p = precise_front(&hard, x, y);
                skin.distance(p) - shell.distance(p)
            };
            let (mut a, mut b) = (0..120)
                .find_map(|i| {
                    let a = 0.37 + i as f32 * 0.001;
                    let b = a + 0.001;
                    (difference(a) * difference(b) < 0.0).then_some((a, b))
                })
                .expect("real bib/underlayer crossing must be covered");
            for _ in 0..18 {
                let m = (a + b) * 0.5;
                if difference(a) * difference(m) <= 0.0 {
                    b = m;
                } else {
                    a = m;
                }
            }
            let y = (a + b) * 0.5;
            let jump = |e| {
                let a = fine_normal(&support, precise_front(&support, x, y - e));
                let b = fine_normal(&support, precise_front(&support, x, y + e));
                a.distance(b)
            };
            let coarse = jump(0.0004);
            let fine = jump(0.0001);
            // Continuous normals contract with sample interval. A crease does not.
            assert!(
                fine < coarse * 0.5 + 0.003,
                "build {build} x {x} y {y}: normal jump does not contract: {coarse} -> {fine}"
            );
        }
    }
}

#[test]
fn vest_transition_is_conservative_and_keeps_complete_strap_clearance() {
    let mut worst = (f32::INFINITY, f32::NEG_INFINITY);
    for model in [
        CreatureModel::Hobgoblin,
        CreatureModel::Human,
        CreatureModel::Nymph,
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                let (_, body) = wearer(model, woman, build);
                let (native, _) = wearer(CreatureModel::Hobgoblin, woman, build);
                let outfit = Outfit::worn(&native);
                let skin = torso_shape(body, None, false);
                let shell = outfit_shell(body, &outfit, false).unwrap();
                let hard = torso_layer_support(&skin, &shell);
                let smooth = torso_layer_transition_support(&skin, &shell);
                let TorsoOrnament::RivetedVest { strap, rivets } = outfit.torso_ornament.unwrap()
                else {
                    unreachable!()
                };
                let mut actual = Parts::default();
                ornament_parts::emit_riveted_vest(&mut actual, body, &hard, &smooth, strap, rivets);
                let mut original = Parts::default();
                ornament_parts::emit(
                    &mut original,
                    outfit.torso_ornament.unwrap(),
                    body,
                    &hard,
                    &skin,
                    None,
                );
                actual.metal.assert_same_recipe(&original.metal);
                assert_eq!(
                    actual.satin.surface_triangles().count(),
                    original.satin.surface_triangles().count(),
                    "unchanged finite-width strap topology"
                );
                for t in actual.satin.surface_triangles() {
                    for point in [t[0], t[1], t[2], (t[0] + t[1] + t[2]) / 3.0] {
                        let old_distance = hard.distance(point);
                        assert!(
                            smooth.distance(point) <= old_distance + 1e-6,
                            "transition cannot shrink supporting layers"
                        );
                        worst.0 = worst.0.min(old_distance);
                        worst.1 = worst.1.max(old_distance);
                        assert!((0.002..=0.025).contains(&old_distance), "{model:?} woman {woman} build {build}: complete shoulder clearance {old_distance} at {point}");
                    }
                }
            }
        }
    }
    eprintln!("Complete vest transition clearance across 18 body fixtures: {worst:?}");
}

#[test]
fn vest_transition_preserves_missing_outer_layer_exactly() {
    let (_, body) = wearer(CreatureModel::Hobgoblin, false, 1);
    let skin = torso_shape(body, None, false);
    let no_layer = torso_layer_transition_support(&skin, &crate::sdf::Sculpt::default());
    let core = super::super::anatomy::TorsoCoreProfile::biped(body);
    let path = profiles()[0].path(core, 1.0);
    let mut a = Builder::default();
    let mut b = Builder::default();
    FittedShoulderStrap::fit(&skin, &path, 0.014).emit(&mut a, Vec3::ONE);
    FittedShoulderStrap::fit(&no_layer, &path, 0.014).emit(&mut b, Vec3::ONE);
    a.assert_same_recipe(&b);
}

#[test]
fn vest_lower_attachment_tabs_are_seated_without_changing_upper_clearance() {
    for model in [
        CreatureModel::Hobgoblin,
        CreatureModel::Human,
        CreatureModel::Nymph,
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                let (_, body) = wearer(model, woman, build);
                let (native, _) = wearer(CreatureModel::Hobgoblin, woman, build);
                let outfit = Outfit::worn(&native);
                let skin = torso_shape(body, None, false);
                let shell = outfit_shell(body, &outfit, false).unwrap();
                let hard = torso_layer_support(&skin, &shell);
                let smooth = torso_layer_transition_support(&skin, &shell);
                let TorsoOrnament::RivetedVest { strap, rivets } = outfit.torso_ornament.unwrap()
                else {
                    unreachable!()
                };
                let mut actual = Parts::default();
                ornament_parts::emit_riveted_vest(&mut actual, body, &hard, &smooth, strap, rivets);
                let core = super::super::anatomy::TorsoCoreProfile::biped(body);
                for side in [-1.0, 1.0] {
                    let y = profiles()[0].path(core, side)[0].0.y;
                    for front in [false, true] {
                        let mut nearest = f32::INFINITY;
                        for t in actual.satin.surface_triangles() {
                            for p in t {
                                if p.x * side > 0.0
                                    && (p.z > 0.0) == front
                                    && (p.y - y).abs() < 0.001
                                {
                                    nearest = nearest.min(hard.distance(p));
                                }
                            }
                        }
                        assert!((0.002..=0.0045).contains(&nearest), "{model:?} {woman} {build} side {side} front {front}: lower attachment underside gap {nearest}");
                    }
                }
            }
        }
    }
}

#[test]
fn lower_seating_preserves_upper_rows_bridge_topology_and_empty_support() {
    for woman in [false, true] {
        for build in 0..3 {
            let (look, body) = wearer(CreatureModel::Hobgoblin, woman, build);
            let outfit = Outfit::worn(&look);
            let skin = torso_shape(body, None, false);
            let shell = outfit_shell(body, &outfit, false).unwrap();
            let hard = torso_layer_support(&skin, &shell);
            let smooth = torso_layer_transition_support(&skin, &shell);
            let core = super::super::anatomy::TorsoCoreProfile::biped(body);
            for side in [-1.0, 1.0] {
                let path = profiles()[0].path(core, side);
                let original = FittedShoulderStrap::fit(&smooth, &path, 0.014);
                let mut seated = FittedShoulderStrap::fit(&smooth, &path, 0.014);
                let span = profiles()[0].chest_drop / 3.0;
                seated.seat_lower(&smooth, &hard, path[0].0.y, span);
                assert_eq!(original.bridge, seated.bridge);
                for (a, b) in [
                    (&original.front, &seated.front),
                    (&original.back, &seated.back),
                ] {
                    assert_eq!(a.strips.len(), b.strips.len());
                    for (a, b) in a.strips.iter().zip(&b.strips) {
                        assert_eq!(a.len(), b.len());
                        for (a, b) in a.iter().zip(b) {
                            assert_eq!(a.len(), b.len());
                            for (a, b) in a.iter().zip(b) {
                                if a.0.y >= path[0].0.y + span {
                                    assert_eq!(a, b);
                                }
                            }
                        }
                    }
                }
                let mut before = Builder::default();
                let mut after = Builder::default();
                original.emit(&mut before, Vec3::ONE);
                seated.emit(&mut after, Vec3::ONE);
                assert_eq!(
                    before.surface_triangles().count(),
                    after.surface_triangles().count()
                );
                let empty = crate::sdf::Sculpt::default();
                let mut missing = FittedShoulderStrap::fit(&empty, &path, 0.014);
                missing.seat_lower(&empty, &empty, path[0].0.y, span);
                assert!(
                    missing.front.strips.is_empty()
                        && missing.back.strips.is_empty()
                        && missing.bridge.is_none()
                );
            }
        }
    }
}
