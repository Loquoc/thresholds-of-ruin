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

// Frozen pre-reconstruction cord recipe; its surface remains the contact oracle.
fn cords(p: &mut Parts, body: HumanoidBody, support: &crate::sdf::Sculpt, count: usize) {
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

// Frozen upper-cord recipe: the knot clears the separate belt below it.
fn anchor_cord(p: &mut Parts, body: HumanoidBody, support: &crate::sdf::Sculpt) {
    let path: Vec<_> = (0..=16)
        .map(|j| {
            let a = j as f32 / 16.0 * TAU;
            waist_parts::point(body, support, 0.17 + 0.03 * (a * 2.0).sin(), a, 0.006)
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
