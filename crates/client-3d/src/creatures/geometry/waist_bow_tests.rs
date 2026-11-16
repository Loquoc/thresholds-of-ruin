use super::*;

fn fixture(model: CreatureModel, woman: bool, build: u8) -> (HumanoidBody, crate::sdf::Sculpt) {
    fixture_in(model, woman, build, Wardrobe::Cleric)
}

fn fixture_in(
    model: CreatureModel,
    woman: bool,
    build: u8,
    wardrobe: Wardrobe,
) -> (HumanoidBody, crate::sdf::Sculpt) {
    let look = Look::with_choice(
        model,
        17,
        Some(Variant {
            woman,
            build,
            ..default()
        }),
    )
    .dressed(wardrobe);
    let body = HumanoidBody::resolve(&look);
    let figure = biped::Figure::resolve(body, &look);
    (body, figure.waist_support)
}

fn parts(smooth: bool) -> Parts {
    let mut p = Parts::default();
    if smooth {
        p.soften();
    }
    p
}

// Independent current contract: same three heights and 5mm cord radius,
// with bounded sampling/clearance. The old recipe remains below as a red witness.
fn cords(p: &mut Parts, body: HumanoidBody, support: &crate::sdf::Sculpt, count: usize) {
    for k in 0..count {
        let y = 0.1 + 0.035 * k as f32;
        let path: Vec<_> = (0..=31)
            .map(|j| {
                let a = j as f32 / 31.0 * TAU;
                waist_parts::point(body, support, y + 0.03 * (a * 2.0).sin(), a, 0.008)
            })
            .collect();
        tube(&mut p.metal, &path, &vec![(0.005, 0.005); path.len()], GILT);
    }
}

// Frozen original cord recipe retained as a penetration-regression witness.
fn legacy_cords(p: &mut Parts, body: HumanoidBody, support: &crate::sdf::Sculpt, count: usize) {
    for k in 0..count {
        let y = 0.1 + 0.035 * k as f32;
        let path: Vec<_> = (0..=16)
            .map(|j| {
                let a = j as f32 / 16.0 * TAU;
                waist_parts::point(body, support, y + 0.03 * (a * 2.0).sin(), a, 0.006)
            })
            .collect();
        tube(&mut p.metal, &path, &vec![(0.005, 0.005); path.len()], GILT);
    }
}

// Independent current upper-cord recipe: the knot clears the separate belt below it.
fn anchor_cord(p: &mut Parts, body: HumanoidBody, support: &crate::sdf::Sculpt) {
    let path: Vec<_> = (0..=31)
        .map(|j| {
            let a = j as f32 / 31.0 * TAU;
            waist_parts::point(body, support, 0.17 + 0.03 * (a * 2.0).sin(), a, 0.008)
        })
        .collect();
    tube(&mut p.metal, &path, &vec![(0.005, 0.005); path.len()], GILT);
}

fn segment_distance(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let edge = b - a;
    p.distance(a + edge * ((p - a).dot(edge) / edge.length_squared()).clamp(0.0, 1.0))
}

// Independent closest-point geometry, not the production radial ray query.
fn triangle_distance(p: Vec3, tri: [Vec3; 3]) -> f32 {
    let [a, b, c] = tri;
    let n = (b - a).cross(c - a);
    let edges = [(a, b), (b, c), (c, a)];
    if n.length_squared() > 1e-18 {
        let q = p - n * (p - a).dot(n) / n.length_squared();
        if edges
            .iter()
            .all(|&(u, v)| (v - u).cross(q - u).dot(n) >= -1e-14)
        {
            return p.distance(q);
        }
    }
    edges
        .into_iter()
        .map(|(a, b)| segment_distance(p, a, b))
        .fold(f32::INFINITY, f32::min)
}

fn tail_root(triangles: &[[Vec3; 3]], _side: f32) -> Vec3 {
    // Closed tubes finish with two triangle fans. Their first vertex is the
    // actual emitted cap center, even after a support-following depth map.
    let end = triangles.last().unwrap()[0];
    let end_count = triangles
        .iter()
        .rev()
        .take_while(|t| t[0].distance(end) < 1e-6)
        .count();
    assert!(end_count >= 10, "closed end fan");
    let start = triangles[triangles.len() - end_count - 1][0];
    let start_count = triangles[..triangles.len() - end_count]
        .iter()
        .rev()
        .take_while(|t| t[0].distance(start) < 1e-6)
        .count();
    assert_eq!(start_count, end_count, "matching closed start fan");
    start
}

fn bow_counts(smooth: bool) -> (usize, usize) {
    let mut p = parts(smooth);
    p.metal
        .blob(Vec3::ZERO, Vec3::new(0.016, 0.01, 0.006), 1, 0.0, GILT);
    let lobe = p.metal.surface_triangles().count();
    tube(
        &mut p.metal,
        &(0..=8)
            .map(|step| Vec3::new(0.012, -0.06, 0.004) * (step as f32 / 8.0))
            .collect::<Vec<_>>(),
        &[(0.004, 0.004); 9],
        GILT,
    );
    (lobe, p.metal.surface_triangles().count() - lobe)
}

#[test]
fn crossed_waist_tail_roots_seat_on_the_actual_upper_cord() {
    let (body, support) = fixture(CreatureModel::Human, true, 1);
    let mut first = parts(false);
    anchor_cord(&mut first, body, &support);
    let first: Vec<_> = first.metal.surface_triangles().collect();
    let mut prefix = parts(false);
    cords(&mut prefix, body, &support, 3);
    let count = prefix.metal.surface_triangles().count();
    let mut actual = parts(false);
    waist(&mut actual, Waist::Crossed(GILT), body, &support);
    actual.metal.assert_preserves_recipe_prefix(&prefix.metal);
    let triangles: Vec<_> = actual.metal.surface_triangles().collect();
    let (lobe, tail) = bow_counts(false);
    assert_eq!(triangles.len(), count + 2 * (lobe + tail));
    for (k, side) in [-1.0, 1.0].into_iter().enumerate() {
        let start = count + k * (lobe + tail) + lobe;
        let root = tail_root(&triangles[start..start + tail], side);
        let gap = first
            .iter()
            .map(|&t| triangle_distance(root, t))
            .fold(f32::INFINITY, f32::min);
        assert!(
            gap < 0.0001,
            "central tail root is not seated on upper cord: side={side} root={root:?} gap={gap}m"
        );
    }
}

fn edge_hit(a: Vec3, b: Vec3, tri: [Vec3; 3]) -> bool {
    let delta = b - a;
    let e1 = tri[1] - tri[0];
    let e2 = tri[2] - tri[0];
    let h = delta.cross(e2);
    let det = e1.dot(h);
    if det.abs() < 1e-12 {
        return false;
    }
    let s = a - tri[0];
    let u = s.dot(h) / det;
    let q = s.cross(e1);
    let v = delta.dot(q) / det;
    let t = e2.dot(q) / det;
    (-1e-5..=1.00001).contains(&t) && u >= -1e-5 && v >= -1e-5 && u + v <= 1.00001
}

fn joined(a: &[[Vec3; 3]], b: &[[Vec3; 3]]) -> bool {
    a.iter().any(|&one| {
        b.iter().any(|&two| {
            (0..3).any(|i| {
                edge_hit(one[i], one[(i + 1) % 3], two) || edge_hit(two[i], two[(i + 1) % 3], one)
            })
        })
    })
}

// Frozen bow recipe, used only for relative shape preservation.
fn bow(p: &mut Parts, root: Vec3) {
    for side in [-1.0, 1.0] {
        p.metal.blob(
            root + Vec3::X * side * 0.018,
            Vec3::new(0.016, 0.01, 0.006),
            1,
            0.0,
            GILT,
        );
        tube(
            &mut p.metal,
            &(0..=8)
                .map(|step| root + Vec3::new(side * 0.012, -0.06, 0.004) * (step as f32 / 8.0))
                .collect::<Vec<_>>(),
            &[(0.004, 0.004); 9],
            GILT,
        );
    }
}

// Depth fitting may bend the pattern, but may not flatten its thickness.
fn preserves_pattern_and_depth(actual: &[[Vec3; 3]], template: &[[Vec3; 3]]) -> bool {
    if actual.len() != template.len() {
        return false;
    }
    let mut shifts = std::collections::BTreeMap::<(i32, i32), (f32, f32)>::new();
    let mut paired = 0;
    for (&a, &b) in actual.iter().flatten().zip(template.iter().flatten()) {
        if a.truncate().distance(b.truncate()) >= 2e-6 || !a.is_finite() {
            return false;
        }
        let key = (
            (b.x * 100_000.0).round() as i32,
            (b.y * 100_000.0).round() as i32,
        );
        let shift = a.z - b.z;
        if let Some(&(old_shift, old_z)) = shifts.get(&key) {
            if (shift - old_shift).abs() > 0.00005 {
                return false;
            }
            if (b.z - old_z).abs() > 0.002 {
                paired += 1;
            }
        } else {
            shifts.insert(key, (shift, b.z));
        }
    }
    paired >= 20
}

fn mounted_bow_witness(
    complete: &[[Vec3; 3]],
    prefix: usize,
    first: &[[Vec3; 3]],
    smooth: bool,
) -> bool {
    let (lobe, tail) = bow_counts(smooth);
    if complete.len() != prefix + 2 * (lobe + tail) {
        return false;
    }
    let mut roots = Vec::new();
    for (k, side) in [-1.0, 1.0].into_iter().enumerate() {
        let start = prefix + k * (lobe + tail);
        let lobe = &complete[start..start + lobe];
        let tail = &complete[start + lobe.len()..start + lobe.len() + tail];
        let root = tail_root(tail, side);
        if root.x.abs() > 1e-6 || (root.y - 0.17).abs() > 1e-6 {
            return false;
        }
        if first
            .iter()
            .map(|&t| triangle_distance(root, t))
            .fold(f32::INFINITY, f32::min)
            >= 0.0001
            || !joined(lobe, tail)
        {
            return false;
        }
        roots.push(root);
    }
    roots[0].distance(roots[1]) < 1e-6
}

#[test]
fn crossed_waist_bow_keeps_cords_shape_and_supported_contact_across_wearers() {
    let mut cases = 0;
    let mut minimum_gap = f32::INFINITY;
    let mut worst_case = String::new();
    for model in [
        CreatureModel::Human,
        CreatureModel::Orc,
        CreatureModel::Nymph,
    ] {
        for (wardrobe, woman) in [
            (Wardrobe::Cleric, false),
            (Wardrobe::Cleric, true),
            (Wardrobe::Goddess, true),
        ] {
            for build in 0..3 {
                let (body, support) = fixture_in(model, woman, build, wardrobe);
                for smooth in [false, true] {
                    for transformed in [false, true] {
                        let make = || {
                            let mut p = parts(smooth);
                            if transformed {
                                p.set_transform(Affine3A::from_scale_rotation_translation(
                                    Vec3::new(1.2, 0.8, 1.1),
                                    Quat::from_rotation_z(0.3),
                                    Vec3::new(0.2, -0.1, 0.07),
                                ));
                            }
                            // A farther pre-existing metal surface must never become the mount.
                            p.metal.blob(
                                Vec3::new(0.0, 0.1, 0.6),
                                Vec3::splat(0.03),
                                2,
                                0.0,
                                STEEL,
                            );
                            p.matte.blob(Vec3::ZERO, Vec3::splat(0.01), 1, 0.0, BONE);
                            p
                        };
                        let before = make();
                        let mut expected = make();
                        cords(&mut expected, body, &support, 3);
                        let prefix = expected.metal.surface_triangles().count();
                        let mut actual = make();
                        waist(&mut actual, Waist::Crossed(GILT), body, &support);
                        actual.metal.assert_preserves_recipe_prefix(&expected.metal);
                        for (a, b) in [
                            &actual.matte,
                            &actual.satin,
                            &actual.sheer,
                            &actual.ghost,
                            &actual.glow,
                        ]
                        .into_iter()
                        .zip([
                            &before.matte,
                            &before.satin,
                            &before.sheer,
                            &before.ghost,
                            &before.glow,
                        ]) {
                            a.assert_same_recipe(b);
                        }
                        let mut first = parts(smooth);
                        anchor_cord(&mut first, body, &support);
                        let first: Vec<_> = first.metal.surface_triangles().collect();
                        let triangles: Vec<_> = actual.metal.surface_triangles().collect();
                        assert!(mounted_bow_witness(&triangles, prefix, &first, smooth), "missing actual attachment {model:?} {wardrobe:?} woman={woman} build={build} smooth={smooth} transformed={transformed}");
                        let (lobe, tail) = bow_counts(smooth);
                        let root = tail_root(&triangles[prefix + lobe..prefix + lobe + tail], -1.0);
                        let mut shape = parts(smooth);
                        bow(&mut shape, root);
                        let template: Vec<_> = shape.metal.surface_triangles().collect();
                        assert_eq!(template.len(), triangles.len() - prefix);
                        assert!(
                            preserves_pattern_and_depth(&triangles[prefix..], &template),
                            "authored pattern or thickness changed"
                        );
                        assert!(
                            triangles[prefix..].iter().flatten().all(|at| at.y > 0.10),
                            "bow must finish above the belt buckle"
                        );
                        for tri in &triangles[prefix..] {
                            for at in [
                                tri[0],
                                tri[1],
                                tri[2],
                                (tri[0] + tri[1]) * 0.5,
                                (tri[1] + tri[2]) * 0.5,
                                (tri[2] + tri[0]) * 0.5,
                                (tri[0] + tri[1] + tri[2]) / 3.0,
                            ] {
                                let gap = support.distance(at - Vec3::Y * body.torso.y);
                                assert!(gap.is_finite(), "non-finite sampled bow clearance");
                                if gap < minimum_gap {
                                    minimum_gap = gap;
                                    worst_case = format!("{model:?} {wardrobe:?} woman={woman} build={build} smooth={smooth} transformed={transformed} at={at:?}");
                                }
                            }
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 108);
    assert!(
        minimum_gap >= 0.001,
        "bow/tail clearance below 1 mm across {cases} cases: {worst_case} gap={minimum_gap}m"
    );
}

#[test]
fn crossed_bow_contact_oracle_rejects_detached_roots_and_lobes() {
    let plane = [Vec3::ZERO, Vec3::X, Vec3::Y];
    assert!((triangle_distance(Vec3::new(0.2, 0.2, 0.5), plane) - 0.5).abs() < 1e-6);
    assert!((triangle_distance(Vec3::X * 2.0, plane) - 1.0).abs() < 1e-6);
    let (body, support) = fixture(CreatureModel::Human, true, 1);
    let mut first = parts(false);
    anchor_cord(&mut first, body, &support);
    let first: Vec<_> = first.metal.surface_triangles().collect();
    let mut prefix = parts(false);
    cords(&mut prefix, body, &support, 3);
    let prefix = prefix.metal.surface_triangles().count();
    let mut p = parts(false);
    waist(&mut p, Waist::Crossed(GILT), body, &support);
    let original: Vec<_> = p.metal.surface_triangles().collect();
    assert!(
        mounted_bow_witness(&original, prefix, &first, false),
        "positive fixture before mutations"
    );
    let (lobe, tail) = bow_counts(false);
    for which in 0..3 {
        let mut broken = original.clone();
        let start = if which == 0 {
            prefix
        } else {
            prefix + (which - 1) * (lobe + tail)
        };
        let end = if which == 0 {
            broken.len()
        } else {
            start + lobe
        };
        for at in broken[start..end].iter_mut().flatten() {
            *at += Vec3::Z * 0.04;
        }
        assert!(
            !mounted_bow_witness(&broken, prefix, &first, false),
            "detached root or lobe accepted: mutation={which}"
        );
    }
}

#[test]
fn crossed_bow_pattern_oracle_rejects_flattened_thickness() {
    let mut p = parts(false);
    bow(&mut p, Vec3::new(0.0, 0.1, 0.2));
    let original: Vec<_> = p.metal.surface_triangles().collect();
    assert!(preserves_pattern_and_depth(&original, &original));
    let flattened: Vec<_> = original
        .iter()
        .map(|tri| tri.map(|point| point.with_z(0.2)))
        .collect();
    assert!(!preserves_pattern_and_depth(&flattened, &original));
}

/// Diagnostic: distinguish support-fitted control knots from emitted tube
/// surfaces. This intentionally reports deficits without redefining clearance.
#[test]
#[ignore = "manual crossing-cord clearance attribution"]
fn crossing_cord_clearance_attribution() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Mage,
        CreatureModel::Nymph,
    ] {
        let look = Look::with_choice(
            model,
            9,
            Some(Variant {
                woman: true,
                ..default()
            }),
        )
        .dressed(Wardrobe::Goddess);
        let body = HumanoidBody::resolve(&look);
        let figure = biped::Figure::resolve(body, &look);
        let support = &figure.waist_support;
        for smooth in [false, true] {
            for k in 0..3 {
                let y = 0.1 + 0.035 * k as f32;
                let path: Vec<_> = (0..=16)
                    .map(|j| {
                        let a = j as f32 / 16.0 * TAU;
                        waist_parts::point(body, support, y + 0.03 * (a * 2.0).sin(), a, 0.006)
                    })
                    .collect();
                let knot_min = path
                    .iter()
                    .map(|p| support.distance(*p - body.torso))
                    .fold(f32::INFINITY, f32::min);
                let mut p = parts(smooth);
                tube(&mut p.metal, &path, &vec![(0.005, 0.005); path.len()], GILT);
                let triangles: Vec<_> = p.metal.surface_triangles().collect();
                let mut worst = (f32::INFINITY, Vec3::ZERO, 0usize);
                let mut vertex_min = f32::INFINITY;
                for (index, tri) in triangles.iter().enumerate() {
                    for at in *tri {
                        vertex_min = vertex_min.min(support.distance(at - body.torso));
                    }
                    for at in [
                        tri[0],
                        tri[1],
                        tri[2],
                        (tri[0] + tri[1]) * 0.5,
                        (tri[1] + tri[2]) * 0.5,
                        (tri[2] + tri[0]) * 0.5,
                        (tri[0] + tri[1] + tri[2]) / 3.0,
                    ] {
                        let gap = support.distance(at - body.torso);
                        if gap < worst.0 {
                            worst = (gap, at, index);
                        }
                    }
                }
                println!("cord model={model:?} smooth={smooth} k={k} knot_min={knot_min} vertex_min={vertex_min} sampled_min={} at={:?} tri={:?}",worst.0,worst.1,triangles[worst.2]);
                assert!(knot_min >= 0.00599, "control knot fit changed");
            }
        }
    }
}

#[test]
fn crossing_cord_identification_pigment_preserves_exact_geometry() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Mage,
        CreatureModel::Nymph,
    ] {
        let (body, support) = fixture_in(model, true, 0, Wardrobe::Goddess);
        for smooth in [false, true] {
            let mut original = parts(smooth);
            let mut tagged = parts(smooth);
            waist(&mut original, Waist::Crossed(GILT), body, &support);
            waist(
                &mut tagged,
                Waist::Crossed(Vec3::new(0.83, 0.07, 0.61)),
                body,
                &support,
            );
            let original: Vec<_> = original.metal.surface_triangles().collect();
            let tagged: Vec<_> = tagged.metal.surface_triangles().collect();
            assert!(original.len() > 100);
            assert_eq!(original, tagged, "identification must not change geometry");
        }
    }
}

/// Fixed-budget alternatives; no production recipe or expected geometry changes.
#[test]
#[ignore = "manual bounded crossing-cord recipe sweep"]
fn crossing_cord_bounded_recipe_sweep() {
    for stations in [16usize, 24, 31, 32, 48, 64] {
        for clearance in [0.006, 0.0065, 0.007, 0.0075, 0.008] {
            let started = std::time::Instant::now();
            let mut worst = (f32::INFINITY, String::new());
            let mut faces = 0usize;
            let mut cases = 0usize;
            for model in [
                CreatureModel::Human,
                CreatureModel::Mage,
                CreatureModel::Nymph,
            ] {
                for build in 0..3 {
                    for (wardrobe, woman) in [
                        (Wardrobe::Cleric, false),
                        (Wardrobe::Cleric, true),
                        (Wardrobe::Goddess, true),
                    ] {
                        let (body, support) = fixture_in(model, woman, build, wardrobe);
                        for smooth in [false, true] {
                            for k in 0..3 {
                                let y = 0.1 + 0.035 * k as f32;
                                let path: Vec<_> = (0..=stations)
                                    .map(|j| {
                                        let a = j as f32 / stations as f32 * TAU;
                                        waist_parts::point(
                                            body,
                                            &support,
                                            y + 0.03 * (a * 2.0).sin(),
                                            a,
                                            clearance,
                                        )
                                    })
                                    .collect();
                                let mut p = parts(smooth);
                                tube(&mut p.metal, &path, &vec![(0.005, 0.005); path.len()], GILT);
                                let faces_before = faces;
                                for tri in p.metal.surface_triangles() {
                                    faces += 1;
                                    for at in [
                                        tri[0],
                                        tri[1],
                                        tri[2],
                                        (tri[0] + tri[1]) * 0.5,
                                        (tri[1] + tri[2]) * 0.5,
                                        (tri[2] + tri[0]) * 0.5,
                                        (tri[0] + tri[1] + tri[2]) / 3.0,
                                    ] {
                                        let gap = support.distance(at - body.torso);
                                        assert!(gap.is_finite(), "nonfinite candidate surface gap");
                                        if gap < worst.0 {
                                            worst=(gap,format!("{model:?}/{wardrobe:?}/woman={woman}/build={build}/smooth={smooth}/cord={k}"));
                                        }
                                    }
                                }
                                assert!(faces > faces_before, "candidate must emit actual faces");
                                cases += 1;
                            }
                        }
                    }
                }
            }
            assert_eq!(cases, 162);
            assert!(worst.0.is_finite());
            println!("sweep stations={stations} clearance={clearance} min={} case={} faces={faces} elapsed_ms={}",worst.0,worst.1,started.elapsed().as_millis());
        }
    }
}

#[test]
fn crossing_cord_actual_faces_clear_support_and_legacy_recipe_exposes_sag() {
    let mut minimum = f32::INFINITY;
    let mut legacy_minimum = f32::INFINITY;
    let mut combinations = 0;
    for model in [
        CreatureModel::Human,
        CreatureModel::Mage,
        CreatureModel::Orc,
        CreatureModel::Nymph,
    ] {
        for build in 0..3 {
            for (wardrobe, woman) in [
                (Wardrobe::Cleric, false),
                (Wardrobe::Cleric, true),
                (Wardrobe::Goddess, true),
            ] {
                let (body, support) = fixture_in(model, woman, build, wardrobe);
                for smooth in [false, true] {
                    let mut expected = parts(smooth);
                    cords(&mut expected, body, &support, 3);
                    let prefix = expected.metal.surface_triangles().count();
                    assert!(prefix > 100);
                    let mut actual = parts(smooth);
                    waist(&mut actual, Waist::Crossed(GILT), body, &support);
                    // Independently fixed heights, winding, radius and recipe;
                    // unrelated changes cannot pass merely by moving outwards.
                    actual.metal.assert_preserves_recipe_prefix(&expected.metal);
                    let triangles: Vec<_> = actual.metal.surface_triangles().take(prefix).collect();
                    for tri in triangles {
                        for i in 0..=8 {
                            for j in 0..=8 - i {
                                let at = tri[0]
                                    + (tri[1] - tri[0]) * (i as f32 / 8.0)
                                    + (tri[2] - tri[0]) * (j as f32 / 8.0);
                                let gap = support.distance(at - body.torso);
                                assert!(gap.is_finite());
                                minimum = minimum.min(gap);
                                assert!(gap>=0.001,"actual cord face enters clearance: {model:?}/{wardrobe:?}/woman={woman}/build={build}/smooth={smooth} at={at:?} gap={gap}");
                            }
                        }
                    }
                    let mut legacy = parts(smooth);
                    legacy_cords(&mut legacy, body, &support, 3);
                    for tri in legacy.metal.surface_triangles() {
                        for at in [
                            tri[0],
                            tri[1],
                            tri[2],
                            (tri[0] + tri[1]) * 0.5,
                            (tri[1] + tri[2]) * 0.5,
                            (tri[2] + tri[0]) * 0.5,
                            (tri[0] + tri[1] + tri[2]) / 3.0,
                        ] {
                            legacy_minimum = legacy_minimum.min(support.distance(at - body.torso));
                        }
                    }
                    combinations += 1;
                }
            }
        }
    }
    assert_eq!(combinations, 72);
    assert!(
        legacy_minimum < 0.0,
        "frozen original recipe must expose its genuine penetration"
    );
    println!(
        "cord actualface combinations={combinations} min={minimum} legacy_min={legacy_minimum}"
    );
}

#[test]
#[ignore = "manual isolated cord construction comparison, not frame performance"]
fn crossing_cord_construction_comparison() {
    let (body, support) = fixture_in(CreatureModel::Human, true, 1, Wardrobe::Goddess);
    for smooth in [false, true] {
        for round in 0..6 {
            for candidate in if round % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let start = std::time::Instant::now();
                for _ in 0..100 {
                    let mut p = parts(smooth);
                    if candidate {
                        cords(&mut p, body, &support, 3);
                    } else {
                        legacy_cords(&mut p, body, &support, 3);
                    }
                    std::hint::black_box(p);
                }
                let micros = start.elapsed().as_micros();
                let mut p = parts(smooth);
                if candidate {
                    cords(&mut p, body, &support, 3);
                } else {
                    legacy_cords(&mut p, body, &support, 3);
                }
                println!("construction smooth={smooth} round={round} candidate={candidate} total100_us={micros} faces={}",p.metal.surface_triangles().count());
            }
        }
    }
}

#[test]
#[ignore = "manual exact mesh export for before/after contact visualization"]
fn crossing_cord_visual_mesh_export() {
    let (body, support) = fixture_in(CreatureModel::Human, false, 2, Wardrobe::Cleric);
    let mut torso = Parts::default();
    support.mesh(
        &mut torso.matte,
        Vec3::new(-0.4, -0.4, -0.4),
        Vec3::new(0.4, 0.3, 0.4),
        0.008,
        &|_, _| Vec3::splat(0.5),
    );
    let body_triangles: Vec<_> = torso
        .matte
        .surface_triangles()
        .map(|tri| tri.map(|p| (p + body.torso).to_array()))
        .collect();
    let mut cases = Vec::new();
    for smooth in [false, true] {
        let mut legacy = parts(smooth);
        legacy_cords(&mut legacy, body, &support, 3);
        let mut candidate = parts(smooth);
        cords(&mut candidate, body, &support, 3);
        let old: Vec<_> = legacy
            .metal
            .surface_triangles()
            .map(|tri| tri.map(|p| p.to_array()))
            .collect();
        let new: Vec<_> = candidate
            .metal
            .surface_triangles()
            .map(|tri| tri.map(|p| p.to_array()))
            .collect();
        cases.push(serde_json::json!({"smooth":smooth,"legacy":old,"candidate":new}));
    }
    let path = ".local/noncloak-review/cord-visual.json";
    std::fs::create_dir_all(".local/noncloak-review").unwrap();
    std::fs::write(
        path,
        serde_json::to_vec(&serde_json::json!({"body":body_triangles,"cases":cases})).unwrap(),
    )
    .unwrap();
    println!("exported exact recipe triangles: {path}");
}
