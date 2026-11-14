use super::*;

fn support_fixture(body: HumanoidBody, layer: usize, smooth: bool) -> Parts {
    let mut p = Parts::default();
    if smooth {
        p.soften();
    }
    organic_forearm(
        &mut p.matte,
        body.build,
        body.arm_length,
        body.profile.muscle,
        layer == 0,
        BONE,
    );
    if layer >= 2 {
        let builder = if layer == 3 {
            &mut p.sheer
        } else {
            &mut p.matte
        };
        biped_cuff(
            builder,
            body.build,
            body.arm_length,
            body.profile.muscle,
            layer == 3,
            LEATHER,
            LEATHER,
        );
    }
    p
}

fn support_mesh(p: &Parts) -> Builder {
    let mut b = Builder::default();
    for part in [&p.matte, &p.metal, &p.satin, &p.sheer, &p.ghost, &p.glow] {
        for tri in part.surface_triangles() {
            b.tri(tri, BONE);
        }
    }
    b
}

#[test]
fn bracelet_clears_actual_forearm_and_prior_cuff_layers() {
    let mut violations = Vec::new();
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
                for smooth in [false, true] {
                    for layer in 0..3 {
                        let mut p = support_fixture(body, layer, smooth);
                        let support = support_mesh(&p);
                        let prior = p.metal.surface_triangles().count();
                        accessory_parts::bracelet(&mut p, GILT, body.build, body.arm_length);
                        let ring: Vec<_> = p.metal.surface_triangles().skip(prior).collect();
                        assert_eq!(ring.len(), 384, "complete ring required");
                        closed_ring(&ring);
                        let (mut minimum, mut maximum) = (f32::INFINITY, f32::NEG_INFINITY);
                        for tri in &ring {
                            for at in [
                                tri[0],
                                tri[1],
                                tri[2],
                                (tri[0] + tri[1]) * 0.5,
                                (tri[1] + tri[2]) * 0.5,
                                (tri[2] + tri[0]) * 0.5,
                                (tri[0] + tri[1] + tri[2]) / 3.0,
                            ] {
                                assert!(at.is_finite());
                                let center = Vec3::Y * at.y;
                                let out = (at - center).normalize();
                                let hit = support
                                    .outer_surface(center, out)
                                    .expect("complete independent support");
                                let gap = (at - hit).dot(out);
                                minimum = minimum.min(gap);
                                maximum = maximum.max(gap);
                            }
                        }
                        eprintln!("bracelet {model:?} woman={woman} build={build} smooth={smooth} layer={layer}: min {minimum:.6}, max {maximum:.6}");
                        if minimum < 0.001 || maximum > 0.018 {
                            violations.push(format!("{model:?} woman={woman} build={build} smooth={smooth} layer={layer}: min {minimum:.6}, max {maximum:.6}"));
                        }
                    }
                }
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

pub(super) fn closed_ring(triangles: &[[Vec3; 3]]) {
    use std::collections::BTreeMap;
    let key = |p: Vec3| {
        [
            (p.x * 1e7).round() as i64,
            (p.y * 1e7).round() as i64,
            (p.z * 1e7).round() as i64,
        ]
    };
    let mut edges = BTreeMap::new();
    for tri in triangles {
        assert!((tri[1] - tri[0]).cross(tri[2] - tri[0]).length() > 1e-9);
        for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            let (a, b) = (key(a), key(b));
            let edge = if a < b { (a, b) } else { (b, a) };
            let count = edges.entry(edge).or_insert((0, 0));
            count.0 += 1;
            count.1 += if a < b { 1 } else { -1 };
        }
    }
    assert!(edges
        .values()
        .all(|&(count, direction)| count == 2 && direction == 0));
}

#[test]
fn bracelet_retains_wrist_seat_fine_height_and_closed_surface() {
    for build in [0.8, 1.0, 1.3] {
        for length in [0.8, 1.0, 1.25] {
            let mut p = Parts::default();
            p.matte.tube(
                &[Vec3::Y * -0.4, Vec3::ZERO],
                &[Vec2::new(0.045, 0.035); 2],
                48,
                &|_, _, _| BONE,
            );
            accessory_parts::bracelet(&mut p, GILT, build, length);
            let ring: Vec<_> = p.metal.surface_triangles().collect();
            assert_eq!(ring.len(), 384);
            closed_ring(&ring);
            let (mut low, mut high) = (f32::INFINITY, f32::NEG_INFINITY);
            for point in ring.iter().flatten() {
                low = low.min(point.y);
                high = high.max(point.y);
            }
            assert!((low - (-0.255 * length - 0.004 * build)).abs() < 1e-6);
            assert!((high - (-0.255 * length + 0.004 * build)).abs() < 1e-6);
        }
    }
}

#[test]
fn bracelet_material_independent_support_preserves_existing_layers() {
    let mut expected = None;
    for material in 0..6 {
        let mut p = Parts::default();
        let b = match material {
            0 => &mut p.matte,
            1 => &mut p.metal,
            2 => &mut p.satin,
            3 => &mut p.sheer,
            4 => &mut p.ghost,
            _ => &mut p.glow,
        };
        b.tube(
            &[Vec3::Y * -0.3, Vec3::ZERO],
            &[Vec2::new(0.05, 0.04); 2],
            48,
            &|_, _, _| BONE,
        );
        let before: Vec<Vec<_>> = [&p.matte, &p.metal, &p.satin, &p.sheer, &p.ghost, &p.glow]
            .into_iter()
            .map(|b| b.surface_triangles().collect())
            .collect();
        accessory_parts::bracelet(&mut p, GILT, 1.0, 1.0);
        let after: Vec<Vec<_>> = [&p.matte, &p.metal, &p.satin, &p.sheer, &p.ghost, &p.glow]
            .into_iter()
            .map(|b| b.surface_triangles().collect())
            .collect();
        for i in 0..6 {
            assert_eq!(&after[i][..before[i].len()], before[i]);
        }
        let ring = after[1][before[1].len()..].to_vec();
        assert_eq!(ring.len(), 384);
        if let Some(ref prior) = expected {
            assert_eq!(&ring, prior);
        } else {
            expected = Some(ring);
        }
    }
}

#[test]
fn bracelet_rejects_missing_or_incomplete_support_atomically() {
    let mut p = Parts::default();
    accessory_parts::bracelet(&mut p, GILT, 1.0, 1.0);
    assert_eq!(p.metal.surface_triangles().count(), 0);
    p.matte.blob(
        Vec3::new(0.04, -0.255, 0.0),
        Vec3::splat(0.015),
        2,
        0.0,
        BONE,
    );
    accessory_parts::bracelet(&mut p, GILT, 1.0, 1.0);
    assert_eq!(p.metal.surface_triangles().count(), 0);
}

#[test]
fn bracelet_supports_metal_and_rejects_crossed_styled_hems() {
    for model in [CreatureModel::Human, CreatureModel::Orc] {
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
                for layer in 0..3 {
                    let mut p = Parts::default();
                    let b = body.build;
                    let length = body.arm_length;
                    let muscle = body.profile.muscle;
                    if layer == 0 {
                        // Match the production metal forearm and its optional
                        // vambrace. The elbow cop is outside the wrist footprint.
                        p.metal
                            .blob(Vec3::ZERO, Vec3::splat(0.058 * b), 2, 0.0, STEEL);
                        metal_forearm(&mut p.metal, b, length, muscle, STEEL);
                        metal_forearm_cuff(&mut p.metal, b, length, STEEL);
                    } else {
                        organic_forearm(&mut p.matte, b, length, muscle, false, BONE);
                        let cuff = if layer == 1 {
                            &mut p.matte
                        } else {
                            &mut p.sheer
                        };
                        // Bell/draped and sheer gauntlet use the authored 1.15
                        // multiplier, unlike ordinary robe cuffs.
                        biped_cuff(cuff, b * 1.15, length, muscle, true, LEATHER, LEATHER * 0.8);
                    }
                    let support = support_mesh(&p);
                    let prior: Vec<_> = p.metal.surface_triangles().collect();
                    let before = all_layers(&p);
                    accessory_parts::bracelet(&mut p, GILT, b, length);
                    if layer > 0 {
                        assert_eq!(
                            all_layers(&p),
                            before,
                            "crossed scalloped hem rejects the complete bracelet"
                        );
                        continue;
                    }
                    let after: Vec<_> = p.metal.surface_triangles().collect();
                    assert_eq!(&after[..prior.len()], prior.as_slice());
                    let ring = &after[prior.len()..];
                    assert_eq!(
                        ring.len(),
                        384,
                        "model={model:?} woman={woman} build={build} layer={layer}"
                    );
                    closed_ring(ring);
                    for tri in ring {
                        for at in [
                            tri[0],
                            tri[1],
                            tri[2],
                            (tri[0] + tri[1]) * 0.5,
                            (tri[1] + tri[2]) * 0.5,
                            (tri[2] + tri[0]) * 0.5,
                            (tri[0] + tri[1] + tri[2]) / 3.0,
                        ] {
                            let origin = Vec3::Y * at.y;
                            let out = (at - origin).normalize();
                            let hit = support
                                .outer_surface(origin, out)
                                .expect("complete independent wrist support");
                            let gap = (at - hit).dot(out);
                            assert!((0.001..=0.018).contains(&gap), "model={model:?} woman={woman} build={build} layer={layer} point={at:?} gap={gap}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn bracelet_production_caller_retains_forearm_layer_boundary() {
    // Preserve the production-frame and ordering witness after moving recipe
    // selection out of geometry. Emitted-geometry tests own actual clearance.
    let source = include_str!("biped.rs");
    let start = source
        .find("let lower = kit.joint(fore,")
        .expect("forearm joint");
    let lower = &source[start..];
    let lower = &lower[..lower.find("let grip = kit").expect("hand boundary")];
    let call = "forearm.emit(p, body, side);";
    assert_eq!(source.matches(call).count(), 1);
    assert!(lower.contains(call), "plan remains in forearm frame");
    let source = include_str!("forearm_plan.rs");
    assert_eq!(
        source
            .matches("bracelet: outfit.trimmings.bracelets")
            .count(),
        1
    );
    let emit = &source[source.find("pub fn emit(").expect("recipe emitter")..];
    let forearm = emit.rfind("organic_forearm(").expect("base first");
    let cuff = emit.rfind("biped_cuff(").expect("native cuff next");
    let early_support = emit
        .find("accessory_parts::resolve_bracelet(")
        .expect("base support choice");
    let cuff_support = emit
        .rfind("accessory_parts::resolve_bracelet(")
        .expect("cuff support choice");
    let bracelet = emit
        .find("accessory_parts::emit_bracelet(")
        .expect("whole bracelet");
    assert!(forearm < early_support && early_support < cuff);
    assert!(cuff < cuff_support && cuff_support < bracelet);
    let fur = emit
        .find("fur_parts::Coat::hanging_limb(")
        .expect("later fur");
    let outerwear = emit
        .find("equipment_outerwear::emit_robe_sleeve(")
        .expect("later outerwear");
    // The earlier cloth-bell glove branch explicitly excludes bracelets.
    // This witness concerns the later branch used by bracelet-bearing plans;
    // forearm_plan_tests checks the early branch's exclusion and geometry.
    let gloves = emit
        .rfind("equipment_gloves::emit_cuff(")
        .expect("later gloves");
    assert!(forearm < cuff && cuff < bracelet);
    assert!(bracelet < fur && fur < outerwear && outerwear < gloves);
}

#[test]
fn bracelet_fine_radial_thickness_matches_authored_metal_on_cylinder() {
    for build in [0.8, 1.0, 1.3] {
        let mut p = Parts::default();
        p.matte.tube(
            &[Vec3::Y * -0.3, Vec3::Y * -0.2],
            &[Vec2::splat(0.05); 2],
            96,
            &|_, _, _| BONE,
        );
        accessory_parts::bracelet(&mut p, GILT, build, 1.0);
        let ring: Vec<_> = p.metal.surface_triangles().collect();
        assert_eq!(ring.len(), 384);
        // At the authored center plane and +Z meridian, the rounded section
        // has exactly its inner and outer tips. Their distance measures the
        // metal itself, independent of support radius or fitting allowance.
        let tips: Vec<_> = ring
            .iter()
            .flatten()
            .filter(|v| v.x.abs() < 1e-6 && v.z > 0.0 && (v.y + 0.255).abs() < 1e-6)
            .collect();
        assert!(!tips.is_empty());
        let inner = tips.iter().map(|v| v.z).reduce(f32::min).unwrap();
        let outer = tips.iter().map(|v| v.z).reduce(f32::max).unwrap();
        assert!((outer - inner - 0.008 * build).abs() < 1e-6);
        assert!(tips
            .iter()
            .all(|v| (v.z - inner).abs() < 1e-6 || (v.z - outer).abs() < 1e-6));
    }
}

#[test]
fn bracelet_rejects_late_angular_and_axial_support_holes_without_mutation() {
    let mut cylinder = Builder::default();
    cylinder.tube(
        &[Vec3::Y * -0.28, Vec3::Y * -0.252, Vec3::Y * -0.23],
        &[Vec2::splat(0.05); 3],
        48,
        &|_, _, _| BONE,
    );
    let triangles: Vec<_> = cylinder.surface_triangles().collect();
    let snapshot = |p: &Parts| -> Vec<Vec<[Vec3; 3]>> {
        [&p.matte, &p.metal, &p.satin, &p.sheer, &p.ghost, &p.glow]
            .into_iter()
            .map(|b| b.surface_triangles().collect())
            .collect()
    };
    // Validate the unchanged source mesh before making either negative fixture.
    let mut positive = Parts::default();
    for &tri in &triangles {
        positive.matte.tri(tri, BONE);
    }
    accessory_parts::bracelet(&mut positive, GILT, 1.0, 1.0);
    let positive_ring: Vec<_> = positive.metal.surface_triangles().collect();
    assert_eq!(positive_ring.len(), 384);
    closed_ring(&positive_ring);

    for angular in [true, false] {
        let mut p = Parts::default();
        let mut removed = 0;
        for &tri in &triangles {
            let center = (tri[0] + tri[1] + tri[2]) / 3.0;
            // Leave earlier azimuths / lower height rows supported. A later
            // missing sample must reject the caller's entire proposed ring.
            let hole = if angular {
                center.x.atan2(center.z).rem_euclid(TAU) > 5.7
            } else {
                center.y > -0.252
            };
            if hole {
                removed += 1;
            } else {
                p.matte.tri(tri, BONE);
            }
        }
        assert!(removed > 0 && removed < triangles.len());
        let supported_y = -0.259;
        assert!(p
            .matte
            .outer_surface(Vec3::Y * supported_y, Vec3::Z)
            .is_some());
        let missing_y = if angular { -0.255 } else { -0.251 };
        let missing_angle: f32 = if angular { 6.0 } else { 0.0 };
        let missing_ray = Vec3::new(missing_angle.sin(), 0.0, missing_angle.cos());
        assert!(p
            .matte
            .outer_surface(Vec3::Y * missing_y, missing_ray)
            .is_none());
        // Existing unrelated metal cannot mask the wrist hole or be lost.
        p.metal
            .blob(Vec3::Y * 0.1, Vec3::splat(0.01), 1, 0.0, STEEL);
        let before = snapshot(&p);
        accessory_parts::bracelet(&mut p, GILT, 1.0, 1.0);
        assert_eq!(snapshot(&p), before, "angular hole={angular}");
    }
}

fn all_layers(p: &Parts) -> Vec<Vec<[Vec3; 3]>> {
    [&p.matte, &p.metal, &p.satin, &p.sheer, &p.ghost, &p.glow]
        .into_iter()
        .map(|b| b.surface_triangles().collect())
        .collect()
}

#[test]
fn royal_robe_crossed_cuff_omits_bracelet_without_mutating_layers() {
    use super::super::outfit::Equipment;
    for model in [
        CreatureModel::Human,
        CreatureModel::Orc,
        CreatureModel::Nymph,
    ] {
        for build in 0..3 {
            let look = Look::with_choice(
                model,
                22,
                Some(Variant {
                    woman: true,
                    build,
                    ..default()
                }),
            )
            .dressed(Wardrobe::RoyalPrincess);
            let body = HumanoidBody::resolve(&look);
            let original = Composition::resolve(&look);
            assert!(original.outfit.trimmings.bracelets.is_some());
            assert!(original.outfit.robe.is_none());
            let mut equipment = Equipment::default();
            equipment.select("robe").unwrap();
            let clothed = look.clone().equipped(equipment);
            let composed = Composition::resolve(&clothed);
            assert!(
                matches!(composed.outfit.arms, Arms::Styled(sleeve) if sleeve.cut == SleeveCut::OffShoulder)
            );
            let robe = composed
                .outfit
                .robe
                .expect("actual equipment introduces the prior bell cuff");
            let metal = composed
                .outfit
                .trimmings
                .bracelets
                .expect("wardrobe still requests bracelet");
            for smooth in [false, true] {
                let mut p = support_fixture(body, 0, smooth);
                biped_cuff(
                    &mut p.matte,
                    body.build,
                    body.arm_length,
                    body.profile.muscle,
                    true,
                    robe.color,
                    robe.trim,
                );
                let before = all_layers(&p);
                accessory_parts::bracelet(&mut p, metal, body.build, body.arm_length);
                assert_eq!(
                    all_layers(&p),
                    before,
                    "unsupported hem drops the complete ornament, not a partial ring"
                );
            }
        }
    }
}
