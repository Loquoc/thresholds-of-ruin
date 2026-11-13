use super::*;

fn veiled(woman: bool, style: u8) -> Look {
    let l = Look::with_choice(
        CreatureModel::Human,
        22,
        Some(Variant {
            woman,
            hair: style,
            ..default()
        }),
    );
    l.dressed(Wardrobe::Juliet)
}

#[test]
fn covered_native_hairstyles_fit_inside_the_veiled_cap_cavity() {
    let mut maximum = 0.0f32;
    for woman in [false, true] {
        for style in 0..5 {
            let l = veiled(woman, style);
            let mut p = Parts::default();
            p.soften();
            covered_hair(&mut p, &l, true);
            for point in p
                .matte
                .surface_triangles()
                .flatten()
                .filter(|p| p.y >= 0.240)
            {
                // Independent cap interior envelope, with three millimetres
                // reserved for the polygonal shell rather than its ideal sphere.
                let q = ((point - Vec3::new(0.0, 0.182, -0.012)) / Vec3::new(0.111, 0.123, 0.117))
                    .length();
                maximum = maximum.max(q);
            }
        }
    }
    assert!(
        maximum <= 1.0001,
        "covered hair exceeds the cap cavity: {maximum}"
    );
}

#[test]
fn cap_envelope_is_continuous_invertible_and_preserves_exposed_hair() {
    let l = veiled(true, 0);
    let fit = |at| hair_envelope::fit_point(&l, at);
    for y in [-0.25, 0.0, 0.1, 0.2] {
        for x in [-0.17, 0.0, 0.17] {
            let at = Vec3::new(x, y, -0.16);
            assert_eq!(fit(at), at);
        }
    }
    for y in [
        0.1999, 0.2001, 0.219, 0.2399, 0.2401, 0.2649, 0.2651, 0.30, 0.36,
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
fn veiled_hair_clears_actual_lining_and_retains_exposed_landmarks() {
    let mut shell = Builder::default();
    crown_parts::CrownProfile::veiled().emit(&mut shell, Vec3::ONE);
    let center = Vec3::new(0.0, 0.182, -0.012);
    let mut witnesses = 0;
    let mut retained = 0;
    for woman in [false, true] {
        for style in 0..5 {
            for smooth in [false, true] {
                let l = veiled(woman, style);
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
                    if a.y <= 0.2 {
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
                        if at.y < 0.242 {
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
fn unrelated_headwear_preserves_authored_hair_exactly() {
    for wardrobe in [Wardrobe::Commoner, Wardrobe::RoyalPrincess, Wardrobe::Genie] {
        for woman in [false, true] {
            for style in 0..5 {
                let l = veiled(woman, style).dressed(wardrobe);
                let mut raw = Parts::default();
                let mut actual = Parts::default();
                raw.soften();
                actual.soften();
                hair::authored_hair(&mut raw, &l, true);
                hair(&mut actual, &l, true);
                assert_eq!(
                    raw.matte.surface_triangles().collect::<Vec<_>>(),
                    actual.matte.surface_triangles().collect::<Vec<_>>()
                );
            }
        }
    }
}

#[test]
fn cap_attachment_fits_mesh_and_controls_once_without_changing_free_fall() {
    let l = veiled(true, 0);
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
    assert!(changed >= 11);
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
fn veil_mount_and_trim_share_the_open_cap_profile() {
    let profile = crown_parts::CrownProfile::veiled();
    let mut p = Parts::default();
    headwear_parts::veiled(&mut p, Vec3::ONE, Vec3::ONE);
    let upper: Vec<_> = p
        .sheer
        .surface_triangles()
        .flatten()
        .filter(|v| v.y > 0.235)
        .collect();
    assert!(!upper.is_empty());
    let rim_x = profile.rim_point(PI * 0.5).x;
    for point in upper {
        let angle = PI - (point.x / rim_x).asin();
        assert!(
            point.distance(profile.rim_point(angle)) < 0.005,
            "full seam follows cap: {point}"
        );
    }
    let origin = Vec3::new(0.0, 0.27, -0.13);
    let axis = Vec3::new(0.0, -1.0, -0.12).normalize();
    let normal = Vec3::X.cross(axis);
    let points: Vec<_> = p.sheer.surface_triangles().flatten().collect();
    for side in [-1.0, 1.0] {
        for face in [-1.0, 1.0] {
            let corner = origin + axis * 0.46 + Vec3::X * (side * 0.15) + normal * (face * 0.002);
            assert!(
                points.iter().any(|v| v.distance(corner) < 1e-6),
                "retain lower veil corner"
            );
        }
    }
    for point in points.iter().filter(|p| p.y < 0.20) {
        assert!(
            ((point - origin).dot(normal).abs() - 0.002).abs() < 1e-6,
            "lower veil plane stays authored"
        );
    }
    assert!(
        p.matte
            .inner_surface(Vec3::new(0.0, 0.23, -0.012), Vec3::Y)
            .unwrap()
            .y
            > 0.30,
        "no cap disk across opening"
    );
    assert_eq!(p.matte.surface_triangles().count(), 768);
    assert_eq!(p.metal.surface_triangles().count(), 384);
}

#[test]
fn fitted_loose_roots_stay_inside_their_control_capsules() {
    for tone in [0, 2, 5] {
        let mut l = veiled(true, 0);
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
                    && tri.iter().all(|p| p.y <= 0.20)
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
fn veiled_cap_and_curved_sheet_have_closed_oriented_topology() {
    use std::collections::BTreeMap;
    let mut p = Parts::default();
    headwear_parts::veiled(&mut p, Vec3::ONE, Vec3::ONE);
    for builder in [&p.matte, &p.metal, &p.sheer] {
        let mut edges = BTreeMap::new();
        let quantize = |v: Vec3| v.to_array().map(|x| (x * 1e6).round() as i32);
        for triangle in builder.surface_triangles() {
            assert!(
                (triangle[1] - triangle[0])
                    .cross(triangle[2] - triangle[0])
                    .length()
                    > 1e-9
            );
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                let a = quantize(triangle[a]);
                let b = quantize(triangle[b]);
                let (key, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                let count = edges.entry(key).or_insert((0, 0));
                count.0 += 1;
                count.1 += sign;
            }
        }
        assert!(
            edges.values().all(|&count| count == (2, 0)),
            "sheet/crown must be an oriented closed shell"
        );
    }
}

#[test]
fn hair_transition_clears_the_actual_cap_trim() {
    let mut trim = Builder::default();
    crown_parts::CrownProfile::veiled().emit_rim(&mut trim, 0.006, Vec3::ONE);
    let center = Vec3::new(0.0, 0.182, -0.012);
    let mut witnesses = 0;
    for woman in [false, true] {
        for style in 0..5 {
            let l = veiled(woman, style);
            let mut p = Parts::default();
            p.soften();
            covered_hair(&mut p, &l, true);
            for triangle in p.matte.surface_triangles() {
                for at in [
                    triangle[0],
                    (triangle[0] + triangle[1]) * 0.5,
                    (triangle[0] + triangle[1] + triangle[2]) / 3.0,
                ] {
                    if !(0.23..0.25).contains(&at.y) {
                        continue;
                    }
                    if let Some(wall) = trim.inner_surface(center, (at - center).normalize()) {
                        assert!(
                            at.distance(center) + 0.0001 < wall.distance(center),
                            "hair intersects actual cap trim: {woman}/{style} {at}"
                        );
                        witnesses += 1;
                    }
                }
            }
        }
    }
    assert!(witnesses > 100);
}

#[test]
fn veiled_bun_keeps_a_substantial_exposed_round_landmark() {
    let l = veiled(true, 2);
    let mut p = Parts::default();
    p.soften();
    hair(&mut p, &l, true);
    let exposed: Vec<_> = p
        .matte
        .surface_triangles()
        .flatten()
        .filter(|p| p.z < -0.16 && (0.215..0.29).contains(&p.y))
        .collect();
    let (low, high) = exposed
        .iter()
        .fold((Vec3::MAX, Vec3::MIN), |(a, b), &p| (a.min(p), b.max(p)));
    assert!(
        exposed.len() > 100 && high.x - low.x > 0.085 && high.y - low.y > 0.06,
        "cap fitting erased the exposed bun: {} vertices, bounds {low}..{high}",
        exposed.len()
    );
}

#[test]
fn exposed_bun_preserves_authored_shape_and_seats_on_the_rear_band() {
    let l = veiled(true, 2);
    let profile = hair::fitted_bun(&l).expect("separate exposed bun");
    let original = hair::BunProfile::AUTHORED;
    assert_eq!(profile.radii, original.radii);
    assert_eq!(profile.center.x, original.center.x);
    assert_eq!(profile.center.y, original.center.y);
    assert_eq!(original.center, Vec3::new(0.0, 0.25, -0.115));
    assert_eq!(original.radii, Vec3::new(0.062, 0.056, 0.056));
    let mut raw = Parts::default();
    raw.soften();
    // Independent pre-reconstruction literals, not a second call to the helper.
    raw.matte.blob(
        Vec3::new(0.0, 0.25, -0.115),
        Vec3::new(0.062, 0.056, 0.056),
        2,
        0.0,
        Vec3::ONE,
    );
    raw.matte.torus(
        Vec3::new(0.0, 0.25, -0.115),
        Vec3::new(0.0, 0.3, -1.0).normalize(),
        0.055,
        0.008,
        12,
        4,
        Vec3::ONE,
    );
    let mut fitted = Parts::default();
    fitted.soften();
    hair::emit_bun(&mut fitted, &l, profile);
    let shift = profile.center - original.center;
    let a: Vec<_> = raw.matte.surface_triangles().collect();
    let b: Vec<_> = fitted.matte.surface_triangles().collect();
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().flatten().zip(b.iter().flatten()) {
        assert!(b.distance(*a + shift) < 1e-6);
    }
    let cap = crown_parts::CrownProfile::veiled();
    let mut shell = Builder::default();
    cap.emit(&mut shell, Vec3::ONE);
    let mut band = Builder::default();
    cap.emit_rim(&mut band, crown_parts::VEILED_RIM_RADIUS, Vec3::ONE);
    let mut gap = f32::INFINITY;
    let rim: Vec<_> = band.surface_triangles().flatten().collect();
    for point in b.iter().flatten() {
        gap = gap.min(
            rim.iter()
                .map(|r| r.distance(*point))
                .fold(f32::INFINITY, f32::min),
        );
        // The retained bun is outside the back of the actual shell. Its
        // lower front sits close to the band without crossing either surface.
        if point.y >= cap.opening {
            let origin = Vec3::new(point.x, point.y, 0.15);
            for surface in [&shell, &band] {
                if let Some(rear) = surface.outer_surface(origin, Vec3::NEG_Z) {
                    assert!(
                        point.z < rear.z - 0.0005,
                        "bun pierces rear cap or trim at {point}"
                    );
                }
            }
        }
    }
    assert!(gap < 0.006, "bun has a detached mount: {gap}");
    // Outfit changes restore the original bun, not the fitted rear station.
    assert!(hair::fitted_bun(&l.dressed(Wardrobe::Commoner)).is_none());
}
