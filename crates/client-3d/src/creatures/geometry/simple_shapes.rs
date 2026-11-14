use super::*;

/// A weathered stone slab: softly bevelled, its tone drifting across the
/// face, with moss settled on the upper surfaces.
pub(super) fn stone(b: &mut Builder, center: Vec3, half: Vec3, color: Vec3, key: u64) {
    let moss = Vec3::new(0.38, 0.56, 0.17);
    let seed = key as f32 * 1.7;
    b.slab(center, half, Quat::IDENTITY, key, 0.06, &|dir, at| {
        let n = (at.x * 23.0 + seed).sin() * (at.z * 19.0 - seed).cos()
            + (at.y * 29.0 + at.x * 7.0).sin() * 0.6;
        let tone = color * (0.9 + 0.1 * (at.y * 9.0 + at.x * 5.0 + seed).sin());
        if (dir.y > 0.3 && n > 0.55) || (dir.y > -0.3 && n > 1.05) {
            moss
        } else {
            tone
        }
    });
}

/// A fist: a palm, four fingers curled under at the knuckles, and a thumb
/// wrapped across. `side` is +1 for the left hand, -1 for the right; `b`
/// scales it with the body.
pub(super) fn fist(p: &mut Parts, at: Vec3, side: f32, b: f32, color: Vec3) {
    // The arm hangs with the palm toward the thigh (toward -side on x):
    // the knuckles run front to back, the fingers curl in toward the body,
    // and the thumb sits at the front, wrapping over them.
    let inward = -side;
    p.matte
        .blob(at, Vec3::new(0.03, 0.05, 0.044) * b, 2, 0.0, color);
    for k in 0..4 {
        let z = (1.5 - k as f32) * 0.019 * b;
        let knuckle = at + Vec3::new(side * 0.004, -0.036 * b, z);
        let r = (0.0115 - k as f32 * 0.0008) * b;
        tube(
            &mut p.matte,
            &fist_finger_path(knuckle, at, inward, b),
            &[(r, r), (r, r), (r * 0.95, r * 0.95), (r * 0.8, r * 0.8)],
            color * (0.97 - k as f32 * 0.01),
        );
    }
    let base = at + Vec3::new(inward * 0.012, -0.004, 0.03) * b;
    tube(
        &mut p.matte,
        &[
            base,
            base + Vec3::new(inward * 0.014, -0.02, 0.016) * b,
            base + Vec3::new(inward * 0.03, -0.032, 0.004) * b,
        ],
        &[
            (0.014 * b, 0.014 * b),
            (0.012 * b, 0.012 * b),
            (0.01 * b, 0.01 * b),
        ],
        color,
    );
}

/// A free hand with separated long fingers: a relaxed casting palm or an
/// undead reaching claw. Held props retain the existing closed grip.
pub(super) fn open_hand(p: &mut Parts, at: Vec3, side: f32, build: f32, color: Vec3, claw: bool) {
    let inward = -side;
    p.matte
        .blob(at, Vec3::new(0.027, 0.045, 0.044) * build, 2, 0.0, color);
    for finger in 0..4 {
        let spread = (1.5 - finger as f32) * 0.021;
        let short = [0.012, 0.0, 0.006, 0.023][finger];
        let root = at + open_finger_root(side, spread) * build;
        let radius = (0.011 - finger as f32 * 0.00065) * build;
        tube(
            &mut p.matte,
            &[
                root,
                at + Vec3::new(0.0, -0.071 + short * 0.4, spread * 1.15) * build,
                at + Vec3::new(inward * 0.012, -0.103 + short, spread * 1.35) * build,
                at + Vec3::new(
                    inward * if claw { 0.035 } else { 0.018 },
                    -0.112 + short + if claw { 0.012 } else { 0.0 },
                    spread * 1.4,
                ) * build,
            ],
            &[
                (radius, radius),
                (radius, radius),
                (radius * 0.83, radius * 0.83),
                (radius * 0.55, radius * 0.55),
            ],
            color,
        );
    }
    rounded_digit(
        &mut p.matte,
        &[
            at + Vec3::new(inward * 0.014, 0.0, 0.029) * build,
            at + Vec3::new(inward * 0.027, -0.015, 0.057) * build,
            at + Vec3::new(inward * 0.029, -0.042, 0.069) * build,
        ],
        &[
            (0.014 * build, 0.014 * build),
            (0.012 * build, 0.012 * build),
            (0.008 * build, 0.008 * build),
        ],
        color,
    );
}

/// Round the exposed thumb tip, retaining its authored terminal pole.
/// Finger recipes and shared tube defaults remain untouched; clenched grips
/// deliberately keep their existing recipe. The final span bounds the rounding
/// depth so a short hooked claw does not acquire a bulbous extra segment.
fn rounded_digit(b: &mut Builder, path: &[Vec3], radii: &[(f32, f32)], color: Vec3) {
    let last = path.len() - 1;
    let tip = path[last];
    let span = tip - path[last - 1];
    let axis = span.normalize();
    let radius = radii[last];
    let depth = radius.0.max(radius.1).min(span.length() * 0.45);
    let mut rounded_path = path[..last].to_vec();
    let mut rounded_radii = radii[..last].to_vec();
    rounded_path.extend([tip - axis * depth, tip - axis * depth * 0.293, tip]);
    rounded_radii.extend([radius, (radius.0 * 0.707, radius.1 * 0.707), (0.0, 0.0)]);
    tube(b, &rounded_path, &rounded_radii, color);
}

/// Tufts of hair or fur: `count` tapered strands rising from around a
/// crown at `center`, swept along `sweep` and drooping at the tips. The
/// same strands make a fringe, a mane or a beard.
pub(super) fn hair_tufts(
    p: &mut Parts,
    center: Vec3,
    radius: f32,
    count: u32,
    sweep: Vec3,
    color: Vec3,
    seed: u64,
) {
    let sweep = sweep.normalize_or(Vec3::Z);
    for k in 0..count {
        let a = k as f32 / count as f32 * TAU + rand(seed, k) * 0.5;
        let root = center
            + Vec3::new(
                a.cos() * radius * 0.7,
                rand(seed, k + 50) * 0.02,
                a.sin() * radius * 0.7,
            );
        let out = (Vec3::new(a.cos(), 0.1, a.sin()) * 0.6 + sweep).normalize();
        let length = radius * (0.7 + rand(seed, k + 100) * 0.4);
        let w = radius * (0.36 + rand(seed, k + 150) * 0.1);
        // Each lock lies along the head, lifting a little before it falls.
        tube(
            &mut p.matte,
            &[
                root,
                root + out * length * 0.5 + Vec3::Y * length * 0.05,
                root + out * length - Vec3::Y * length * 0.45,
            ],
            &[(w, w * 0.6), (w * 0.8, w * 0.5), (0.003, 0.003)],
            color * (0.92 + rand(seed, k + 200) * 0.16),
        );
    }
}

/// A smooth tube of one colour through `path` with elliptical `radii`.
pub(super) fn tube(b: &mut Builder, path: &[Vec3], radii: &[(f32, f32)], color: Vec3) {
    let radii: Vec<Vec2> = radii.iter().map(|(x, z)| Vec2::new(*x, *z)).collect();
    b.tube(path, &radii, 10, &|_, _, _| color);
}

/// A soft leaf-shaped feather from `root`, curving outward along `out`
/// and drooping as it goes; flat, broad in the middle, pointed at the tip.
pub(super) fn feather(p: &mut Parts, root: Vec3, out: Vec3, length: f32, width: f32, color: Vec3) {
    let down = Vec3::NEG_Y;
    let path = [
        root,
        root + out * length * 0.22 + down * length * 0.25,
        root + out * length * 0.34 + down * length * 0.6,
        root + out * length * 0.38 + down * length,
    ];
    let w = width * 0.5;
    let radii = [
        Vec2::new(w * 0.4, 0.006),
        Vec2::new(w, 0.01),
        Vec2::new(w * 0.62, 0.007),
        Vec2::new(0.002, 0.002),
    ];
    p.matte
        .tube(&path, &radii, 6, &|_, _, t| color * (1.0 - 0.18 * t));
}

/// Eyes with whites, a coloured iris, a pupil and a catchlight, set a
/// little forward of `center` on either side.
pub(super) fn cartoon_eyes(p: &mut Parts, center: Vec3, spread: f32, size: f32, iris: Vec3) {
    for side in [-1.0f32, 1.0] {
        let at = center + Vec3::X * side * spread;
        p.matte.blob(
            at,
            Vec3::new(size * 1.55, size * 1.75, size * 1.0),
            0,
            0.0,
            Vec3::new(0.95, 0.94, 0.9),
        );
        p.matte.blob(
            at + Vec3::new(0.0, 0.0, size * 0.62),
            Vec3::new(size * 1.0, size * 1.1, size * 0.5),
            0,
            0.0,
            iris,
        );
        p.matte.blob(
            at + Vec3::new(0.0, 0.0, size * 0.92),
            Vec3::new(size * 0.55, size * 0.62, size * 0.25),
            0,
            0.0,
            DARK,
        );
        p.matte.blob(
            at + Vec3::new(size * 0.35, size * 0.42, size * 1.05),
            Vec3::splat(size * 0.22),
            0,
            0.0,
            Vec3::splat(1.6),
        );
    }
}

/// Closed horizontal trim uses a periodic frame without coincident end caps.
pub(super) fn closed_ring(b: &mut Builder, points: &[Vec3], width: f32, color: Vec3) {
    // Callers supply the closing sample. Reuse the first frame exactly instead
    // of generating two caps and inconsistent one-sided endpoint tangents.
    let count = points.len() - 1;
    assert!(count >= 3 && points[0].distance(points[count]) < 0.00001);
    let vertex = |i: usize, j: usize| {
        let i = i % count;
        let tangent = (points[(i + 1) % count] - points[(i + count - 1) % count]).normalize();
        let across = Vec3::Y.cross(tangent).normalize();
        let up = across.cross(tangent).normalize();
        let angle = (j % 10) as f32 * TAU / 10.0;
        let normal = across * angle.cos() + up * angle.sin();
        (points[i] + normal * width, normal)
    };
    for i in 0..count {
        for j in 0..10 {
            let q = [
                vertex(i, j),
                vertex(i + 1, j),
                vertex(i + 1, j + 1),
                vertex(i, j + 1),
            ];
            for ids in [[0, 1, 2], [0, 2, 3]] {
                b.tri_smooth(ids.map(|k| q[k].0), ids.map(|k| q[k].1), [color; 3]);
            }
        }
    }
}

/// A brim is an annulus around a head opening, never a flattened solid
/// ellipsoid through the scalp. Its outer width is independent of crown fit.
pub(super) fn annular_brim(
    b: &mut Builder,
    y: f32,
    inner: Vec2,
    outer: Vec2,
    forward: f32,
    color: Vec3,
) {
    let point = |col: usize, outside: bool, top: bool| {
        let a = col as f32 * TAU / 64.0;
        let radius = if outside { outer } else { inner };
        Vec3::new(
            a.sin() * radius.x,
            y + if top { 0.006 } else { -0.006 },
            a.cos() * radius.y - 0.012 + if outside { forward } else { 0.0 },
        )
    };
    for col in 0..64 {
        for top in [false, true] {
            let q = [
                point(col, false, top),
                point(col, true, top),
                point(col + 1, true, top),
                point(col + 1, false, top),
            ];
            b.quad_out(q, Vec3::Y * (y + if top { -0.02 } else { 0.02 }), color);
        }
        for outside in [false, true] {
            let q = [
                point(col, outside, false),
                point(col + 1, outside, false),
                point(col + 1, outside, true),
                point(col, outside, true),
            ];
            let center = Vec3::new(0.0, y, -0.012);
            b.quad_out(
                q,
                if outside {
                    center
                } else {
                    q[0] + q[2] - center
                },
                color,
            );
        }
    }
}

fn fist_finger_path(knuckle: Vec3, at: Vec3, inward: f32, build: f32) -> [Vec3; 4] {
    [
        fist_finger_root(knuckle, at, build),
        knuckle + Vec3::new(inward * 0.018, -0.022, 0.0) * build,
        knuckle + Vec3::new(inward * 0.036, -0.012, 0.0) * build,
        knuckle + Vec3::new(inward * 0.036, 0.008, 0.0) * build,
    ]
}

fn fist_finger_root(knuckle: Vec3, at: Vec3, build: f32) -> Vec3 {
    at + seated_finger_root(knuckle - at, 0.014 * build, 0.5)
}

/// Seat only the proximal cap in the narrowing palm. Distal controls belong
/// to the pose recipe, so a shared fit adjustment never relocates a grip.
fn seated_finger_root(authored: Vec3, lift: f32, spread: f32) -> Vec3 {
    Vec3::new(authored.x, authored.y + lift, authored.z * spread)
}

fn open_finger_root(side: f32, spread: f32) -> Vec3 {
    // Begin inside the palm rather than on its narrowing lower edge. Only
    // the proximal root moves: the separated fingers keep their authored
    // knuckles, lengths and relaxed/clawed tips, and held-prop grips are separate.
    seated_finger_root(Vec3::new(side * 0.003, -0.034, spread), 0.012, 0.7)
}

#[cfg(test)]
mod hand_root_tests {
    use super::*;

    #[test]
    fn rounded_digit_keeps_terminal_extent_and_valid_triangles() {
        for smooth in [false, true] {
            for build in [0.7, 1.0, 1.4] {
                for axis in [Vec3::X, Vec3::NEG_Y, Vec3::new(1.0, -2.0, 3.0).normalize()] {
                    let tip = axis * 0.04 * build;
                    let radius = 0.008 * build;
                    let mut part = Parts::default();
                    part.matte.smooth = smooth;
                    rounded_digit(
                        &mut part.matte,
                        &[Vec3::ZERO, tip],
                        &[(radius, radius), (radius, radius)],
                        Vec3::ONE,
                    );
                    let triangles: Vec<_> = part.matte.surface_triangles().collect();
                    assert!(!triangles.is_empty());
                    let mut pole_found = false;
                    let mut rounded_ring_found = false;
                    for triangle in triangles {
                        assert!(
                            (triangle[1] - triangle[0])
                                .cross(triangle[2] - triangle[0])
                                .length()
                                > 1e-10
                        );
                        for point in triangle {
                            assert!(point.is_finite());
                            let along = (point - tip).dot(axis);
                            assert!(along <= 1e-6, "rounding must not lengthen the digit");
                            pole_found |= point.distance(tip) < 1e-6;
                            let radial = (point - axis * point.dot(axis)).length();
                            rounded_ring_found |= along < -radius * 0.1
                                && along > -radius * 0.5
                                && radial > radius * 0.5
                                && radial < radius * 0.9;
                        }
                    }
                    assert!(pole_found && rounded_ring_found);
                }
            }
        }
    }

    #[test]
    fn thumb_rounding_stays_bounded_on_bent_paths() {
        for smooth in [false, true] {
            for side in [-1.0, 1.0] {
                for build in [0.7, 1.0, 1.4] {
                    let path = [
                        Vec3::new(-side * 0.014, 0.0, 0.029) * build,
                        Vec3::new(-side * 0.027, -0.015, 0.057) * build,
                        Vec3::new(-side * 0.029, -0.042, 0.069) * build,
                    ];
                    let tip = path[2];
                    let axis = (tip - path[1]).normalize();
                    let mut digit = Parts::default();
                    digit.matte.smooth = smooth;
                    rounded_digit(
                        &mut digit.matte,
                        &path,
                        &[
                            (0.014 * build, 0.014 * build),
                            (0.012 * build, 0.012 * build),
                            (0.008 * build, 0.008 * build),
                        ],
                        Vec3::ONE,
                    );
                    for triangle in digit.matte.surface_triangles() {
                        assert!(
                            (triangle[1] - triangle[0])
                                .cross(triangle[2] - triangle[0])
                                .length()
                                > 1e-10
                        );
                        for point in triangle {
                            assert!(point.is_finite());
                            assert!(
                                (point - tip).dot(axis) <= 1e-6,
                                "thumb exceeds authored terminal plane"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn open_thumb_tip_is_rounded_without_extending_authored_length() {
        for smooth in [false, true] {
            for side in [-1.0, 1.0] {
                for build in [0.7, 1.0, 1.4] {
                    let tip = Vec3::new(-side * 0.029, -0.042, 0.069) * build;
                    let previous = Vec3::new(-side * 0.027, -0.015, 0.057) * build;
                    let axis = (tip - previous).normalize();
                    let mut hand = Parts::default();
                    hand.matte.smooth = smooth;
                    open_hand(&mut hand, Vec3::ZERO, side, build, Vec3::ONE, false);
                    let planar = hand
                        .matte
                        .surface_triangles()
                        .filter(|triangle| {
                            triangle.iter().any(|point| point.distance(tip) < 1e-6)
                                && triangle
                                    .iter()
                                    .all(|point| (*point - tip).dot(axis).abs() < 1e-6)
                                && (triangle[1] - triangle[0])
                                    .cross(triangle[2] - triangle[0])
                                    .length()
                                    > 1e-8
                        })
                        .count();
                    assert_eq!(planar, 0, "thumb tip must not end in a flat cut disk");
                }
            }
        }
    }

    #[test]
    fn fist_finger_caps_are_embedded_without_moving_the_grip() {
        for smooth in [false, true] {
            for side in [-1.0, 1.0] {
                for build in [0.7, 1.0, 1.4] {
                    let at = Vec3::new(0.1, 0.2, -0.1);
                    let mut palm = Parts::default();
                    palm.matte.smooth = smooth;
                    palm.matte
                        .blob(at, Vec3::new(0.03, 0.05, 0.044) * build, 2, 0.0, Vec3::ONE);
                    let prefix = palm.matte.surface_triangles().count();
                    let mut hand = Parts::default();
                    hand.matte.smooth = smooth;
                    fist(&mut hand, at, side, build, Vec3::ONE);
                    assert_eq!(
                        hand.matte
                            .surface_triangles()
                            .take(prefix)
                            .collect::<Vec<_>>(),
                        palm.matte.surface_triangles().collect::<Vec<_>>(),
                        "palm geometry remains unchanged"
                    );
                    for finger in 0..4 {
                        let knuckle = at
                            + Vec3::new(
                                side * 0.004,
                                -0.036 * build,
                                (1.5 - finger as f32) * 0.019 * build,
                            );
                        let path = fist_finger_path(knuckle, at, -side, build);
                        for (actual, expected) in path[1..].iter().zip([
                            Vec3::new(-side * 0.018, -0.022, 0.0),
                            Vec3::new(-side * 0.036, -0.012, 0.0),
                            Vec3::new(-side * 0.036, 0.008, 0.0),
                        ]) {
                            assert!(
                                actual.distance(knuckle + expected * build) < 1e-6,
                                "all authored curled grip controls stay fixed"
                            );
                        }
                        let root = path[0];
                        let caps: Vec<_> = hand
                            .matte
                            .surface_triangles()
                            .skip(prefix)
                            .filter(|triangle| {
                                triangle.iter().any(|point| point.distance(root) < 1e-6)
                            })
                            .flatten()
                            .collect();
                        assert!(caps.len() >= 30);
                        for point in caps {
                            let ray = point - at;
                            let surface = palm
                                .matte
                                .outer_surface(at, ray.normalize())
                                .expect("palm surface");
                            assert!(point.distance(at) < surface.distance(at) - 0.0005 * build,
                                "fist finger {finger} smooth {smooth} build {build}: root cap protrudes {} mm",
                                (point.distance(at)-surface.distance(at))*1000.0);
                        }
                        let tip = knuckle + Vec3::new(-side * 0.036, 0.008, 0.0) * build;
                        assert!(
                            hand.matte
                                .surface_triangles()
                                .flatten()
                                .any(|point| point.distance(tip) < 1e-6),
                            "curled grip endpoint must not move"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn open_hand_preserves_authored_fingertips_and_uniform_scaling() {
        for side in [-1.0, 1.0] {
            for claw in [false, true] {
                let mut unit = Parts::default();
                open_hand(&mut unit, Vec3::ZERO, side, 1.0, Vec3::ONE, claw);
                let expected: Vec<_> = unit.matte.surface_triangles().flatten().collect();
                for build in [0.7, 1.0, 1.4] {
                    let at = Vec3::new(0.1, 0.2, -0.1);
                    let mut hand = Parts::default();
                    open_hand(&mut hand, at, side, build, Vec3::ONE, claw);
                    let points: Vec<_> = hand.matte.surface_triangles().flatten().collect();
                    assert_eq!(points.len(), expected.len());
                    for (actual, unit) in points.iter().zip(&expected) {
                        assert!((*actual - (at + *unit * build)).length() < 1e-6);
                    }
                    for (finger, short) in [0.012, 0.0, 0.006, 0.023].into_iter().enumerate() {
                        let tip = at
                            + Vec3::new(
                                -side * if claw { 0.035 } else { 0.018 },
                                -0.112 + short + if claw { 0.012 } else { 0.0 },
                                (1.5 - finger as f32) * 0.021 * 1.4,
                            ) * build;
                        assert!(
                            points.iter().any(|point| point.distance(tip) < 1e-6),
                            "authored fingertip cap center is unchanged"
                        );
                    }
                    let thumb_tip = at + Vec3::new(-side * 0.029, -0.042, 0.069) * build;
                    assert!(points.iter().any(|point| point.distance(thumb_tip) < 1e-6));
                }
            }
        }
    }

    #[test]
    fn open_finger_caps_are_embedded_in_the_emitted_palm() {
        for smooth in [false, true] {
            for side in [-1.0, 1.0] {
                for build in [0.7, 1.0, 1.4] {
                    for claw in [false, true] {
                        let at = Vec3::new(0.1, 0.2, -0.1);
                        let mut palm = Parts::default();
                        palm.matte.smooth = smooth;
                        palm.matte.blob(
                            at,
                            Vec3::new(0.027, 0.045, 0.044) * build,
                            2,
                            0.0,
                            Vec3::ONE,
                        );
                        let prefix = palm.matte.surface_triangles().count();
                        let mut hand = Parts::default();
                        hand.matte.smooth = smooth;
                        open_hand(&mut hand, at, side, build, Vec3::ONE, claw);
                        for finger in 0..4 {
                            let root =
                                at + open_finger_root(side, (1.5 - finger as f32) * 0.021) * build;
                            let caps: Vec<_> = hand
                                .matte
                                .surface_triangles()
                                .skip(prefix)
                                .filter(|triangle| {
                                    triangle.iter().any(|point| point.distance(root) < 1e-6)
                                })
                                .flatten()
                                .collect();
                            assert!(caps.len() >= 30, "find actual capped finger root");
                            for point in caps {
                                let ray = point - at;
                                let surface = palm
                                    .matte
                                    .outer_surface(at, ray.normalize())
                                    .expect("palm surface");
                                assert!(point.distance(at) < surface.distance(at) - 0.0005 * build,
                                    "finger {finger}, smooth {smooth}, side {side}, build {build}: root cap protrudes by {} mm",
                                    (point.distance(at)-surface.distance(at))*1000.0);
                            }
                        }
                    }
                }
            }
        }
    }
}
