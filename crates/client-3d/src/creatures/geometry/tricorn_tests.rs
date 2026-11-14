use super::*;

fn pirate(woman: bool, style: u8) -> Look {
    Look::with_choice(
        CreatureModel::Human,
        22,
        Some(Variant {
            woman,
            hair: style,
            ..default()
        }),
    )
    .dressed(Wardrobe::Pirate)
}

#[test]
fn tricorn_has_a_real_opening_without_a_scalp_disk() {
    let mut p = Parts::default();
    headwear_parts::tricorn(&mut p, Vec3::ONE, Vec3::ONE);
    let roof = p
        .matte
        .inner_surface(Vec3::new(0.0, 0.23, -0.012), Vec3::Y)
        .unwrap();
    assert!(
        roof.y > 0.35,
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
fn tricorn_covered_hair_clears_cavity_and_brim_opening() {
    let mut maximum = 0.0f32;
    for woman in [false, true] {
        for style in 0..5 {
            let l = pirate(woman, style);
            let mut p = Parts::default();
            p.soften();
            covered_hair(&mut p, &l, true);
            for at in p
                .matte
                .surface_triangles()
                .flatten()
                .filter(|p| p.y >= 0.249)
            {
                let q = ((at - Vec3::new(0.0, 0.182, -0.012)) / Vec3::new(0.131, 0.177, 0.137))
                    .length();
                maximum = maximum.max(q);
            }
        }
    }
    assert!(maximum < 1.0, "hair exceeds tricorn cavity: {maximum}");
}

#[test]
fn tricorn_envelope_preserves_exposed_hair_and_has_no_folds() {
    let l = pirate(true, 0);
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
fn tricorn_hair_clears_actual_lining_and_preserves_uncovered_surface() {
    let mut shell = Builder::default();
    crown_parts::CrownProfile::tricorn().emit(&mut shell, Vec3::ONE);
    let center = Vec3::new(0.0, 0.182, -0.012);
    let mut witnesses = 0;
    let mut retained = 0;
    for woman in [false, true] {
        for style in 0..5 {
            for smooth in [false, true] {
                let l = pirate(woman, style);
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
fn tricorn_animated_roots_share_the_static_fit_without_changing_free_fall() {
    let l = pirate(true, 0);
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
fn tricorn_fitted_root_triangles_stay_inside_control_capsules() {
    for tone in [0, 2, 5] {
        let mut l = pirate(true, 0);
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
fn tricorn_bun_retains_its_shape_below_the_brim() {
    let l = pirate(true, 2);
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
fn tricorn_crown_brim_and_folds_are_finite_closed_oriented_shells() {
    use std::collections::BTreeMap;
    let mut p = Parts::default();
    headwear_parts::tricorn(&mut p, Vec3::ONE, Vec3::ONE);
    let triangles: Vec<_> = p.matte.surface_triangles().collect();
    // The paired lining and crown, annular brim, then three authored folds.
    assert_eq!(triangles.len(), 768 + 512 + 3 * 16);
    for shell in [
        &triangles[..768],
        &triangles[768..1280],
        &triangles[1280..1296],
        &triangles[1296..1312],
        &triangles[1312..],
    ] {
        let mut edges = BTreeMap::new();
        let quantize = |v: Vec3| v.to_array().map(|x| (x * 1e6).round() as i32);
        for tri in shell {
            assert!(tri.iter().all(|p| p.is_finite()));
            assert!((tri[1] - tri[0]).cross(tri[2] - tri[0]).length() > 1e-9);
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                let (a, b) = (quantize(tri[a]), quantize(tri[b]));
                let (key, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                let count = edges.entry(key).or_insert((0, 0));
                count.0 += 1;
                count.1 += sign;
            }
        }
        assert!(
            edges.values().all(|&v| v == (2, 0)),
            "closed oriented crown/brim/fold required"
        );
    }
    let mut expected = Builder::default();
    let corner = |k: usize| {
        let a = k as f32 * TAU / 3.0;
        Vec3::new(a.sin() * 0.255, 0.255, a.cos() * 0.255 - 0.012)
    };
    for k in 0..3 {
        let (a, b) = (corner(k), corner(k + 1));
        let mid = (a + b) * 0.5;
        let out = Vec3::new(mid.x, 0.0, mid.z + 0.012).normalize();
        let up = (Vec3::Y + out * 0.35).normalize();
        tube(
            &mut expected,
            &[
                a + up * 0.085,
                mid + up * 0.0425 + out * 0.008,
                b + up * 0.085,
            ],
            &[(0.006, 0.006); 3],
            Vec3::ONE,
        );
    }
    assert_eq!(
        p.metal.surface_triangles().collect::<Vec<_>>(),
        expected.surface_triangles().collect::<Vec<_>>(),
        "three authored trim paths remain unchanged"
    );
}

#[test]
fn tricorn_brim_opening_tracks_the_shared_crown_without_a_visible_gap() {
    let c = crown_parts::CrownProfile::tricorn();
    let mut p = Parts::default();
    headwear_parts::tricorn(&mut p, Vec3::ONE, Vec3::ONE);
    let mut hair_witnesses = 0;
    for angle in (0..128).map(|i| i as f32 * TAU / 128.0) {
        let r = c.opening_radii();
        let inside = Vec3::new(
            angle.sin() * r.x * 0.999,
            0.255,
            angle.cos() * r.y * 0.999 - 0.012,
        );
        let out = Vec3::new(angle.sin(), 0.0, angle.cos());
        let wall = p
            .matte
            .inner_surface(inside - out * 0.01, out)
            .expect("joined brim/crown opening");
        assert!(
            wall.distance(inside) < 0.008,
            "daylight between brim and crown at {angle}"
        );
    }
    for woman in [false, true] {
        for style in 0..5 {
            let mut hair = Parts::default();
            hair.soften();
            covered_hair(&mut hair, &pirate(woman, style), true);
            for tri in hair.matte.surface_triangles() {
                for at in [
                    tri[0],
                    (tri[0] + tri[1]) * 0.5,
                    (tri[0] + tri[1] + tri[2]) / 3.0,
                ] {
                    if !(0.245..0.265).contains(&at.y) {
                        continue;
                    }
                    let origin = Vec3::new(0.0, at.y, -0.012);
                    let direction = (at - origin).normalize();
                    if let Some(wall) = p.matte.inner_surface(origin, direction) {
                        assert!(
                            at.distance(origin) + 0.0001 < wall.distance(origin),
                            "hair pierces actual brim/crown: {woman}/{style} at {at}"
                        );
                        hair_witnesses += 1;
                    }
                }
            }
        }
    }
    assert!(hair_witnesses > 100);
}

#[test]
fn tricorn_complete_bun_style_emits_one_rigid_bun_after_covered_hair() {
    let l = pirate(true, 2);
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
            "complete pirate hairstyle must include exactly one separate exposed bun"
        );
    }
}
