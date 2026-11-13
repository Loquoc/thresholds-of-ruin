use super::*;

fn witch(woman: bool, style: u8) -> Look {
    Look::with_choice(
        CreatureModel::Human,
        22,
        Some(Variant {
            woman,
            hair: style,
            ..default()
        }),
    )
    .dressed(Wardrobe::Witch)
}

#[test]
fn witch_hat_has_a_real_opening_without_a_scalp_disk() {
    let mut p = Parts::default();
    headwear_parts::witch_hat(&mut p, Vec3::ONE, Vec3::ONE);
    let roof = p
        .matte
        .inner_surface(Vec3::new(0.0, 0.23, -0.012), Vec3::Y)
        .unwrap();
    assert!(
        roof.y > 0.48,
        "solid brim or crown closes opening at {roof}"
    );
    for a in [0.0, PI / 2.0, PI, PI * 1.5] {
        let at = Vec3::new(a.sin() * 0.18, 0.20, a.cos() * 0.18 - 0.012);
        let brim = p
            .matte
            .inner_surface(at, Vec3::Y)
            .expect("brim outside opening");
        assert!((brim.y - 0.249).abs() < 1e-5, "annulus at {brim}");
    }
}

#[test]
fn witch_hat_covered_hair_clears_cavity_and_brim_opening() {
    let mut maximum = 0.0f32;
    for woman in [false, true] {
        for style in 0..5 {
            let l = witch(woman, style);
            let mut p = Parts::default();
            p.soften();
            covered_hair(&mut p, &l, true);
            for at in p
                .matte
                .surface_triangles()
                .flatten()
                .filter(|p| p.y >= 0.249)
            {
                let q = ((at - Vec3::new(0.0, 0.182, -0.012)) / Vec3::new(0.1415, 0.1675, 0.1475))
                    .length();
                maximum = maximum.max(q);
            }
        }
    }
    assert!(maximum < 1.0, "hair exceeds witch_hat cavity: {maximum}");
}
#[test]
fn witch_hat_envelope_preserves_exposed_hair_and_has_no_folds() {
    let l = witch(true, 0);
    let fit = |at| hair_envelope::fit_point(&l, at);
    for y in [-0.25, 0.0, 0.1, 0.215] {
        for x in [-0.17, 0.0, 0.17] {
            let at = Vec3::new(x, y, -0.16);
            assert_eq!(fit(at), at);
        }
    }
    for y in [
        0.2149, 0.2151, 0.239, 0.2489, 0.2491, 0.2799, 0.2801, 0.34, 0.40,
    ] {
        for x in [-0.17, -0.07, 0.0, 0.07, 0.17] {
            for z in [-0.20, -0.08, 0.02, 0.11] {
                let at = Vec3::new(x, y, z);
                let h = 0.0001;
                let derivative = |axis| (fit(at + axis * h) - fit(at - axis * h)) / (2.0 * h);
                let j = Mat3::from_cols(
                    derivative(Vec3::X),
                    derivative(Vec3::Y),
                    derivative(Vec3::Z),
                );
                assert!(
                    j.is_finite() && j.determinant() > 1e-5,
                    "folded map at {at}: {}",
                    j.determinant()
                );
                assert!(fit(at + Vec3::Y * 1e-6).distance(fit(at - Vec3::Y * 1e-6)) < 0.0001);
            }
        }
    }
}

#[test]
fn witch_hat_hair_clears_actual_lining_and_preserves_uncovered_surface() {
    let center = Vec3::new(0.0, 0.182, -0.012);
    let mut witnesses = 0;
    let mut retained = 0;
    for woman in [false, true] {
        for style in 0..5 {
            for smooth in [false, true] {
                let mut hat = Parts::default();
                if smooth {
                    hat.soften();
                }
                headwear_parts::witch_hat(&mut hat, Vec3::ONE, Vec3::ONE);
                let shell = hat.matte;
                let l = witch(woman, style);
                let mut raw = Parts::default();
                let mut fitted = Parts::default();
                if smooth {
                    raw.soften();
                    fitted.soften();
                }
                hair::authored_covered_hair(&mut raw, &l, true);
                covered_hair(&mut fitted, &l, true);
                let original: Vec<_> = raw.matte.surface_triangles().collect();
                let actual: Vec<_> = fitted.matte.surface_triangles().collect();
                assert_eq!(original.len(), actual.len(), "fitting preserves topology");
                for (a, b) in original.iter().flatten().zip(actual.iter().flatten()) {
                    assert!(b.is_finite());
                    if a.y <= 0.215 {
                        assert!(a.distance(*b) < 1e-7);
                        retained += 1;
                    }
                }
                for tri in actual.iter().step_by(19) {
                    for at in [
                        tri[0],
                        (tri[0] + tri[1]) * 0.5,
                        (tri[0] + tri[1] + tri[2]) / 3.0,
                    ] {
                        if at.y < 0.249 {
                            continue;
                        }
                        let center = center.with_y(at.y);
                        let direction = (at - center).normalize();
                        let wall = shell
                            .inner_surface(center, direction)
                            .expect("real cap lining");
                        assert!(
                            at.distance(center) + 0.001 < wall.distance(center),
                            "{woman}/{style}/{smooth}: hair touches shell at {at}"
                        );
                        witnesses += 1;
                    }
                }
                let mut meshes = Assets::<Mesh>::default();
                let mut materials = Assets::<StandardMaterial>::default();
                let model = fitted.finish(
                    &mut meshes,
                    &crate::props::PropMaterials::new(&mut materials),
                );
                for (handle, _) in model.0 {
                    let mesh = meshes.get(&handle).unwrap();
                    use bevy::render::mesh::VertexAttributeValues;
                    let Some(VertexAttributeValues::Float32x3(normals)) =
                        mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
                    else {
                        panic!("normals")
                    };
                    assert!(normals
                        .iter()
                        .all(|n| Vec3::from_array(*n).is_finite()
                            && Vec3::from_array(*n).length() > 0.9));
                }
            }
        }
    }
    assert!(
        witnesses > 500 && retained > 10000,
        "{witnesses} shell / {retained} retained witnesses"
    );
}

#[test]
fn witch_hat_animated_roots_share_the_static_fit_without_changing_free_fall() {
    let l = witch(true, 0);
    assert!(loose_fall(&l) && shows_hair(&l));
    let mut complete = Parts::default();
    complete.soften();
    long_fall(&mut complete, &l);
    let mut pieces = Parts::default();
    pieces.soften();
    let mut changed = 0;
    for strand in long_hair_strands(hair_seed(&l, 0)) {
        hanging_lock(&mut pieces, &l, &strand);
        for (&at, &attached) in strand.path.iter().zip(&strand.attached) {
            let fitted = hair_envelope::fit_point(&l, at);
            if at != fitted {
                assert!(attached, "only attached controls may move");
                changed += 1;
            }
            if !attached {
                assert_eq!(
                    at, fitted,
                    "free fall and collision radii retain original pattern"
                );
            }
        }
    }
    // This taller/wider crown retains every back/side control; only the
    // three upper swept-fringe controls exceed its reserved interior.
    assert_eq!(changed, 3);
    assert_eq!(
        complete.matte.surface_triangles().collect::<Vec<_>>(),
        pieces.matte.surface_triangles().collect::<Vec<_>>()
    );
    // The complete static hairstyle adds the same fitted fall exactly once.
    let mut without = Parts::default();
    without.soften();
    hair(&mut without, &l, false);
    let mut with = Parts::default();
    with.soften();
    hair(&mut with, &l, true);
    let all: Vec<_> = with.matte.surface_triangles().collect();
    assert_eq!(
        &all[without.matte.surface_triangles().count()..],
        &complete.matte.surface_triangles().collect::<Vec<_>>()
    );
}

#[test]
fn witch_hat_fitted_root_triangles_stay_inside_control_capsules() {
    for tone in [0, 2, 5] {
        let mut l = witch(true, 0);
        l.v.hair_tone = tone;
        for strand in long_hair_strands(hair_seed(&l, 0)) {
            let controls = hanging_controls(&l, &strand);
            let mut p = Parts::default();
            p.soften();
            hanging_lock(&mut p, &l, &strand);
            let last = strand.path.len() - 1;
            for (i, &(point, radius)) in controls.iter().enumerate() {
                let t = i as f32 / last as f32;
                let original = strand.root + (strand.tip - strand.root) * t * t;
                assert!(
                    radius.is_finite() && radius >= original && radius - original < 0.015,
                    "bounded measured expansion: {radius} vs {original}"
                );
                if !strand.attached[i] {
                    assert_eq!(radius, original);
                    assert_eq!(point, strand.path[i]);
                }
            }
            for tri in p.matte.surface_triangles() {
                let centroid = (tri[0] + tri[1] + tri[2]) / 3.0;
                let project = |p: Vec3, a: Vec3, b: Vec3| {
                    ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0)
                };
                let (segment, pair) = controls
                    .windows(2)
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        let d = |q: &[(Vec3, f32)]| {
                            centroid.distance_squared(
                                q[0].0.lerp(q[1].0, project(centroid, q[0].0, q[1].0)),
                            )
                        };
                        d(a).total_cmp(&d(b))
                    })
                    .unwrap();
                if pair[0].0 == strand.path[segment]
                    && pair[1].0 == strand.path[segment + 1]
                    && tri.iter().all(|p| p.y <= 0.215)
                {
                    continue;
                }
                for point in [tri[0], tri[1], tri[2], centroid, (tri[0] + tri[1]) * 0.5] {
                    let t = project(point, pair[0].0, pair[1].0);
                    let distance = point.distance(pair[0].0.lerp(pair[1].0, t));
                    let radius = pair[0].1 + (pair[1].1 - pair[0].1) * t;
                    assert!(
                        distance <= radius + 1e-5,
                        "whole root triangle escapes tapered capsule: {distance} > {radius}"
                    );
                }
            }
        }
    }
}

#[test]
fn witch_hat_bun_retains_its_shape_below_the_brim() {
    let l = witch(true, 2);
    let fitted = hair::fitted_bun(&l).expect("separate bun below broad brim");
    let original = hair::BunProfile::AUTHORED;
    assert_eq!(fitted.radii, Vec3::new(0.062, 0.056, 0.056));
    assert_eq!(fitted.center.x, original.center.x);
    assert_eq!(fitted.center.z, original.center.z);
    let mut before = Parts::default();
    let mut after = Parts::default();
    before.soften();
    after.soften();
    hair::emit_bun(&mut before, &l, original);
    hair::emit_bun(&mut after, &l, fitted);
    let a: Vec<_> = before.matte.surface_triangles().collect();
    let b: Vec<_> = after.matte.surface_triangles().collect();
    assert_eq!(a.len(), b.len());
    let mut high = f32::NEG_INFINITY;
    for (a, b) in a.iter().flatten().zip(b.iter().flatten()) {
        assert!(b.distance(*a + fitted.center - original.center) < 1e-6);
        high = high.max(b.y);
        assert!(b.y < 0.249, "bun penetrates actual brim underside: {b}");
    }
    assert!(high > 0.239, "bun is detached from the brim");
    assert!(hair::fitted_bun(&l.dressed(Wardrobe::Commoner)).is_none());
}

#[test]
fn witch_hat_complete_bun_style_emits_one_rigid_bun_after_covered_hair() {
    let l = witch(true, 2);
    for smooth in [false, true] {
        let mut complete = Parts::default();
        let mut covered = Parts::default();
        let mut bun = Parts::default();
        if smooth {
            complete.soften();
            covered.soften();
            bun.soften();
        }
        hair(&mut complete, &l, true);
        covered_hair(&mut covered, &l, true);
        hair::emit_bun(&mut bun, &l, hair::fitted_bun(&l).unwrap());
        let mut expected: Vec<_> = covered.matte.surface_triangles().collect();
        let bun_triangles: Vec<_> = bun.matte.surface_triangles().collect();
        assert!(bun_triangles.len() > 100);
        expected.extend(bun_triangles);
        assert_eq!(
            complete.matte.surface_triangles().collect::<Vec<_>>(),
            expected,
            "complete witch hairstyle must include exactly one separate exposed bun"
        );
    }
}

#[test]
fn witch_hat_mouth_and_brim_share_the_anatomical_opening() {
    let profile = crown_parts::CrownProfile::witch_hat();
    let mut p = Parts::default();
    headwear_parts::witch_hat(&mut p, Vec3::ONE, Vec3::ONE);
    // Isolate the actual cone from its band and bow so they cannot mask a gap.
    let mut cone = Builder::default();
    for tri in p.matte.surface_triangles().skip(512).take(352) {
        cone.tri(tri, Vec3::ONE);
    }
    for angle in (0..128).map(|i| i as f32 * TAU / 128.0) {
        let origin = Vec3::new(0.0, 0.261, -0.012);
        let out = Vec3::new(angle.sin(), 0.0, angle.cos());
        let wall = cone
            .inner_surface(origin, out)
            .expect("cone starts at brim");
        let inner = profile.opening_radii();
        let opening = 1.0 / (Vec2::new(out.x, out.z) / inner).length();
        assert!(
            wall.distance(origin) >= opening - 0.0005,
            "daylight at {angle}"
        );
        assert!(
            wall.distance(origin) < opening + 0.009,
            "detached crown at {angle}"
        );
    }
}

#[test]
fn witch_hat_annulus_is_closed_and_cone_has_only_its_intended_mouth() {
    use std::collections::BTreeMap;
    let mut p = Parts::default();
    headwear_parts::witch_hat(&mut p, Vec3::ONE, Vec3::ONE);
    let triangles: Vec<_> = p.matte.surface_triangles().collect();
    for (shell, mouth) in [
        (&triangles[..512], false),
        (&triangles[512..864], true),
        (&triangles[864..2912], false),
    ] {
        let mut edges = BTreeMap::new();
        let quantize = |v: Vec3| v.to_array().map(|x| (x * 1e6).round() as i32);
        for tri in shell {
            assert!(tri.iter().all(|p| p.is_finite()));
            assert!((tri[1] - tri[0]).cross(tri[2] - tri[0]).length() > 1e-9);
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                let (a, b) = (quantize(tri[a]), quantize(tri[b]));
                let (key, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                let count = edges.entry(key).or_insert((0i32, 0i32));
                count.0 += 1;
                count.1 += sign;
            }
        }
        let boundary: Vec<_> = edges.iter().filter(|(_, v)| v.0 == 1).collect();
        assert_eq!(boundary.len(), if mouth { 32 } else { 0 });
        assert!(boundary
            .iter()
            .all(|((a, b), _)| a[1] == 255000 && b[1] == 255000));
        assert!(edges
            .values()
            .all(|&(count, sign)| (count == 2 && sign == 0)
                || (mouth && count == 1 && sign.abs() == 1)));
    }
}

#[test]
fn witch_hat_retains_the_authored_bow_sheer_tails_and_bent_tip() {
    let mut native = Parts::default();
    let mut wizard = Parts::default();
    headwear_parts::witch_hat(&mut native, Vec3::ONE, Vec3::ONE);
    headwear_parts::open_witch_hat(&mut wizard, Vec3::ONE, Vec3::ONE);
    // After native brim, cone and 2048-face supported ribbon versus the
    // equipment brim, cone and 128-face torus, the bow remains identical.
    assert_eq!(
        native
            .matte
            .surface_triangles()
            .skip(2912)
            .collect::<Vec<_>>(),
        wizard
            .matte
            .surface_triangles()
            .skip(750)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        native.sheer.surface_triangles().collect::<Vec<_>>(),
        wizard.sheer.surface_triangles().collect::<Vec<_>>()
    );
    assert!(native.sheer.surface_triangles().count() > 0);
    let high = native
        .matte
        .surface_triangles()
        .flatten()
        .map(|p| p.y)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        (high - 0.618255).abs() < 0.002,
        "authored bent tip remains: {high}"
    );
}

#[test]
fn witch_hat_ribbon_is_continuously_exposed_above_its_actual_cone() {
    let mut p = Parts::default();
    headwear_parts::witch_hat(&mut p, Vec3::ONE, Vec3::ONE);
    let mut cone = Builder::default();
    for tri in p.matte.surface_triangles().skip(512).take(352) {
        cone.tri(tri, Vec3::ONE);
    }
    let band = headwear_parts::witch_hat_band(&cone, 0.255, Vec3::ONE);
    assert_eq!(
        p.matte
            .surface_triangles()
            .skip(864)
            .take(2048)
            .collect::<Vec<_>>(),
        band.surface_triangles().collect::<Vec<_>>(),
        "complete native recipe emits the supported ribbon exactly once"
    );
    for height in [0.265, 0.271, 0.277, 0.283, 0.289] {
        for index in 0..128 {
            let a = index as f32 * TAU / 128.0;
            let out = Vec3::new(a.sin(), 0.0, a.cos());
            let origin = Vec3::new(0.0, height, -0.012);
            let cloth = cone.outer_surface(origin, out).expect("cone support");
            let ribbon = band.outer_surface(origin, out).expect("full ribbon width");
            let relief = (ribbon - cloth).dot(out);
            assert!(
                (0.001..0.010).contains(&relief),
                "band must stay exposed and seated: {height}/{a}, relief {relief}"
            );
        }
    }
}

#[test]
fn witch_hat_ribbon_and_cone_keep_the_enclosing_attachment_frame() {
    for smooth in [false, true] {
        let mut plain = Parts::default();
        let mut moved = Parts::default();
        if smooth {
            plain.soften();
            moved.soften();
        }
        moved.set_transform(Affine3A::from_scale_rotation_translation(
            Vec3::new(0.8, 1.2, 0.9),
            Quat::from_rotation_x(0.4),
            Vec3::new(0.12, -0.17, 0.21),
        ));
        headwear_parts::witch_hat(&mut plain, Vec3::ONE, Vec3::ONE);
        headwear_parts::witch_hat(&mut moved, Vec3::ONE, Vec3::ONE);
        for (a, b) in [(&plain.matte, &moved.matte), (&plain.sheer, &moved.sheer)] {
            let a: Vec<_> = a.surface_triangles().collect();
            let b: Vec<_> = b.surface_triangles().collect();
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().flatten().zip(b.iter().flatten()) {
                assert!(
                    a.distance(*b) < 1e-6,
                    "isolated support escaped attachment frame"
                );
            }
        }
    }
}

#[test]
fn witch_hat_finished_softened_mesh_keeps_the_ribbon_exposed() {
    use bevy::render::mesh::VertexAttributeValues;
    for smooth in [false, true] {
        let mut p = Parts::default();
        if smooth {
            p.soften();
        }
        headwear_parts::witch_hat(&mut p, Vec3::X, Vec3::Y);
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let model = p.finish(
            &mut meshes,
            &crate::props::PropMaterials::new(&mut materials),
        );
        let mesh = meshes.get(&model.0[0].0).expect("native matte hat mesh");
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions")
        };
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("colors")
        };
        let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
        let mut cloth = Builder::default();
        let mut ribbon = Builder::default();
        let mut colored_faces = 0;
        for tri in indices.as_chunks::<3>().0 {
            let points = tri.map(|index| Vec3::from(points[index]));
            if tri
                .iter()
                .all(|&index| colors[index][0] > 0.9 && colors[index][1] < 0.001)
            {
                cloth.tri(points, Vec3::ONE);
            } else if tri
                .iter()
                .all(|&index| colors[index][1] > 0.9 && colors[index][0] < 0.001)
            {
                // The complete recipe emits the 64x16 supported ribbon before
                // either bow lobe. Color separates it from variable cone tessellation.
                if colored_faces < 2048 {
                    ribbon.tri(points, Vec3::ONE);
                }
                colored_faces += 1;
            }
        }
        assert!(colored_faces > 2048, "ribbon and separate bow required");
        assert_eq!(ribbon.surface_triangles().count(), 2048);
        // Two-sided support rays cannot detect flipped render faces. Every
        // finished ribbon edge must also be shared with opposite orientation,
        // including when the underlying cone has been softened.
        let mut edges = std::collections::BTreeMap::new();
        let quantize = |v: Vec3| v.to_array().map(|x| (x * 1e6).round() as i32);
        for tri in ribbon.surface_triangles() {
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                let (a, b) = (quantize(tri[a]), quantize(tri[b]));
                let (key, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                let count = edges.entry(key).or_insert((0i32, 0i32));
                count.0 += 1;
                count.1 += sign;
            }
        }
        let inconsistent = edges
            .values()
            .filter(|&&(count, sign)| count != 2 || sign != 0)
            .count();
        assert_eq!(
            inconsistent, 0,
            "finished ribbon smooth={smooth}: incoherent shared edges"
        );
        let volume = ribbon
            .surface_triangles()
            .map(|tri| tri[0].dot(tri[1].cross(tri[2])) / 6.0)
            .sum::<f32>();
        assert!(
            volume > 0.0,
            "finished ribbon smooth={smooth}: outward closed shell, volume {volume}"
        );
        let mut minimum = (f32::INFINITY, 0.0, 0.0);
        let mut maximum = (f32::NEG_INFINITY, 0.0, 0.0);
        for height in [0.265, 0.271, 0.277, 0.283, 0.289] {
            for index in 0..128 {
                let a = index as f32 * TAU / 128.0;
                let out = Vec3::new(a.sin(), 0.0, a.cos());
                let origin = Vec3::new(0.0, height, -0.012);
                let cone = cloth.outer_surface(origin, out).expect("cone above brim");
                let band = ribbon
                    .outer_surface(origin, out)
                    .expect("continuous ribbon");
                let relief = (band - cone).dot(out);
                assert!(relief.is_finite());
                if relief < minimum.0 {
                    minimum = (relief, height, a);
                }
                if relief > maximum.0 {
                    maximum = (relief, height, a);
                }
            }
        }
        eprintln!("finished ribbon smooth={smooth}: minimum={minimum:?}, maximum={maximum:?}");
        assert!(
            minimum.0 >= 0.001 && maximum.0 < 0.010,
            "finished ribbon smooth={smooth}: minimum={minimum:?}, maximum={maximum:?}"
        );
    }
}
