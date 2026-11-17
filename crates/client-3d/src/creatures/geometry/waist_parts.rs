//! Thin bands fitted to the emitted support, including asymmetric hip sections.
use super::*;

pub(super) fn point(
    body: HumanoidBody,
    support: &crate::sdf::Sculpt,
    height: f32,
    angle: f32,
    gap: f32,
) -> Vec3 {
    radial_parts::support_point(support, body.torso.y, height, angle, gap)
}

pub(super) fn band(
    b: &mut Builder,
    body: HumanoidBody,
    support: &crate::sdf::Sculpt,
    heights: Vec2,
    color: Vec3,
) {
    slanted_band(b, body, support, heights, Vec2::new(0.0, 0.004), color);
}

/// A fitted strap: tilt is its sinusoidal rise around the waist; clearance
/// separates crossing straps without changing the underlying support.
pub(super) fn slanted_band(
    b: &mut Builder,
    body: HumanoidBody,
    support: &crate::sdf::Sculpt,
    heights: Vec2,
    tilt_clearance: Vec2,
    color: Vec3,
) {
    radial_parts::RadialBandProfile {
        heights,
        support_to_output_y: body.torso.y,
        tilt: tilt_clearance.x,
        clearance: tilt_clearance.y,
        thickness: 0.003,
        rows: 4,
        sides: 48,
    }
    .emit(b, support, color);
}

/// A soft bag fitted over the union of the clothed waist and pelvic support.
/// The same radial fit carries its back, flap and the loops rising to the belt.
pub(super) fn pouch(
    b: &mut Builder,
    body: HumanoidBody,
    support: &crate::sdf::Sculpt,
    color: Vec3,
) {
    let pelvis = pelvis_shape(body);
    let fit = |height: f32, angle: f32, clearance: f32| {
        let out = Vec3::new(angle.sin(), 0.0, angle.cos());
        let (mut near, mut far) = (0.0, 0.7 * body.build);
        for _ in 0..18 {
            let radius = (near + far) * 0.5;
            let at = out * radius + Vec3::Y * height;
            let distance = support.distance(at - body.torso).min(pelvis.distance(at));
            if distance < clearance {
                near = radius;
            } else {
                far = radius;
            }
        }
        out * ((near + far) * 0.5) + Vec3::Y * height
    };
    let radius = fit(0.0, 0.9, 0.012).with_y(0.0).length();
    let half_angle = 0.035 * body.build / radius;
    let bag = |u: f32, v: f32, front: bool| {
        // Slightly narrowed corners and a convex front give the leather volume.
        let width = 0.86 + 0.14 * (PI * v).sin();
        let angle = 0.9 + (2.0 * u - 1.0) * half_angle * width;
        let height = (0.035 - 0.095 * v) * body.build;
        let thickness = if front {
            (0.012 + 0.025 * (PI * u).sin() * (PI * v).sin()) * body.build
        } else {
            0.0
        };
        fit(height, angle, 0.012) + Vec3::new(angle.sin(), 0.0, angle.cos()) * thickness
    };
    pouch_shell(b, bag, color);
    // A raised flap edge and two leather loops make the attachment readable.
    let flap: Vec<_> = (0..=16)
        .map(|i| {
            let u = i as f32 / 16.0;
            let angle = 0.9 + (2.0 * u - 1.0) * half_angle;
            bag(u, 0.29 + 0.08 * (PI * u).sin(), true)
                + Vec3::new(angle.sin(), 0.0, angle.cos()) * 0.0015
        })
        .collect();
    tube(b, &flap, &vec![(0.0018, 0.0018); flap.len()], color * 0.68);
    let support_radius = |height, angle: f32, clearance| {
        let out = Vec3::new(angle.sin(), 0.0, angle.cos());
        surface_field_ray_hit(
            &|at| support.distance(at - body.torso).min(pelvis.distance(at)),
            Vec3::Y * height,
            out,
            clearance,
            0.7 * body.build,
        )
        .map(|point| point.with_y(0.0).dot(out))
    };
    for side in [-1.0, 1.0] {
        let profile = radial_parts::RadialAttachmentProfile {
            heights: Vec2::new(0.032 * body.build, BELT_TOP_Y + 0.005),
            angles: Vec2::new(
                0.9 + (side * 0.019 - 0.004) * body.build / radius,
                0.9 + (side * 0.019 + 0.004) * body.build / radius,
            ),
            clearance: 0.012,
            thickness: 0.002,
            rows: 8,
            columns: 2,
        };
        if let Some(attachment) = profile.resolve(&support_radius, color * 0.8) {
            for tri in attachment.surface_triangles() {
                b.tri(tri, color * 0.8);
            }
        }
    }
}

/// The fitted leather band stays present when a garment supplies its own buckle.
pub(super) fn belt(
    p: &mut Parts,
    body: HumanoidBody,
    support: &crate::sdf::Sculpt,
    include_buckle: bool,
) {
    band(
        &mut p.satin,
        body,
        support,
        Vec2::new(BELT_BOTTOM_Y, BELT_TOP_Y),
        LEATHER * 0.8,
    );
    if include_buckle {
        p.metal.cuboid(
            point(body, support, BELT_CENTER_Y, 0.0, 0.008),
            Vec3::new(0.035, 0.03, 0.012),
            Quat::IDENTITY,
            Vec3::new(0.74, 0.5, 0.26),
        );
    }
}

#[cfg(test)]
mod belt_hardware_tests {
    use super::*;

    #[test]
    fn replacement_hardware_preserves_exact_belt_band() {
        for model in [
            CreatureModel::Orc,
            CreatureModel::Troll,
            CreatureModel::Human,
        ] {
            let body =
                HumanoidBody::resolve(&Look::with_choice(model, 22, Some(Variant::default())));
            let support = torso_shape(body, Some(TorsoMaterial::Leather), false);
            let mut default = Parts::default();
            let mut replacement = Parts::default();
            belt(&mut default, body, &support, true);
            belt(&mut replacement, body, &support, false);
            assert!(!default.satin.is_empty());
            default.satin.assert_same_recipe(&replacement.satin);
            assert!(
                replacement.metal.is_empty(),
                "replacement garment owns the hardware"
            );
        }
    }

    #[test]
    fn default_buckle_keeps_original_recipe() {
        let body = HumanoidBody::resolve(&Look::with_choice(
            CreatureModel::Troll,
            22,
            Some(Variant::default()),
        ));
        let support = torso_shape(body, Some(TorsoMaterial::Leather), false);
        let mut actual = Parts::default();
        belt(&mut actual, body, &support, true);
        let mut expected = Builder::default();
        expected.cuboid(
            point(body, &support, BELT_CENTER_Y, 0.0, 0.008),
            Vec3::new(0.035, 0.03, 0.012),
            Quat::IDENTITY,
            Vec3::new(0.74, 0.5, 0.26),
        );
        actual.metal.assert_same_recipe(&expected);
    }
}

// Resolve the shared pouch shell separately from its flap and belt mounts.
fn pouch_shell(b: &mut Builder, mut bag: impl FnMut(f32, f32, bool) -> Vec3, color: Vec3) {
    const ROWS: usize = 10;
    const COLUMNS: usize = 8;
    // Adjacent faces share these exact samples. Keep winding witnesses
    // separate: they sample cell interiors, not averaged surface corners.
    let grid: [[[Vec3; 2]; COLUMNS + 1]; ROWS + 1] = std::array::from_fn(|row| {
        std::array::from_fn(|col| {
            [false, true]
                .map(|front| bag(col as f32 / COLUMNS as f32, row as f32 / ROWS as f32, front))
        })
    });
    for row in 0..ROWS {
        let v0 = row as f32 / ROWS as f32;
        let v1 = (row + 1) as f32 / ROWS as f32;
        for col in 0..COLUMNS {
            let u0 = col as f32 / COLUMNS as f32;
            let u1 = (col + 1) as f32 / COLUMNS as f32;
            let inside = (bag((u0 + u1) * 0.5, (v0 + v1) * 0.5, false)
                + bag((u0 + u1) * 0.5, (v0 + v1) * 0.5, true))
                * 0.5;
            for front in [false, true] {
                let side = usize::from(front);
                let q = [
                    grid[row][col][side],
                    grid[row][col + 1][side],
                    grid[row + 1][col + 1][side],
                    grid[row + 1][col][side],
                ];
                b.quad_out(q, inside, color * if front { 1.0 } else { 0.85 });
            }
        }
        for col in [0, COLUMNS] {
            b.quad_out(
                [
                    grid[row][col][0],
                    grid[row][col][1],
                    grid[row + 1][col][1],
                    grid[row + 1][col][0],
                ],
                bag(0.5, (v0 + v1) * 0.5, true),
                color * 0.9,
            );
        }
    }
    for row in [0, ROWS] {
        for col in 0..COLUMNS {
            let u0 = col as f32 / COLUMNS as f32;
            let u1 = (col + 1) as f32 / COLUMNS as f32;
            b.quad_out(
                [
                    grid[row][col][0],
                    grid[row][col + 1][0],
                    grid[row][col + 1][1],
                    grid[row][col][1],
                ],
                bag((u0 + u1) * 0.5, 0.5, true),
                color * 0.9,
            );
        }
    }
}

#[cfg(test)]
mod sampling_tests {
    use super::*;
    #[test]
    fn pouch_shell_reuses_shared_surface_samples() {
        let mut calls = 0;
        let mut mesh = Builder::default();
        pouch_shell(
            &mut mesh,
            |u, v, front| {
                calls += 1;
                Vec3::new(u, v, if front { 0.1 } else { 0.0 })
            },
            Vec3::ONE,
        );
        assert_eq!(mesh.surface_triangles().count(), 392);
        assert!(calls <= 400, "shared pouch shell samples repeated: {calls}");
    }
}

/// A pointed belt follows the same support at every seam sample. The authored
/// height/dip and half-width are independent of body shape and paint palette.
pub(super) fn pointed_band(
    p: &mut Parts,
    body: HumanoidBody,
    support: &crate::sdf::Sculpt,
    height_dip: Vec2,
    half_width: f32,
    color: Vec3,
    edge: Vec3,
) {
    let pelvis = pelvis_shape(body);
    let rings: Vec<Vec<Vec3>> = [-1.0, 1.0]
        .into_iter()
        .map(|edge| {
            (0..=64)
                .map(|k| {
                    let a = k as f32 * TAU / 64.0;
                    let y =
                        height_dip.x - height_dip.y * a.cos().max(0.0).powi(2) + edge * half_width;
                    surface_field_ray_hit(
                        &|at| support.distance(at - body.torso).min(pelvis.distance(at)),
                        Vec3::Y * y,
                        Vec3::new(a.sin(), 0.0, a.cos()),
                        0.010,
                        0.7 * body.build,
                    )
                    .expect("pointed belt requires waist or pelvic support")
                })
                .collect()
        })
        .collect();
    for k in 0..64 {
        p.matte.quad_out(
            [rings[0][k], rings[0][k + 1], rings[1][k + 1], rings[1][k]],
            Vec3::Y * height_dip.x,
            color,
        );
    }
    for ring in rings {
        tube(
            &mut p.metal,
            &ring,
            &vec![(0.0025, 0.0025); ring.len()],
            edge,
        );
    }
}

#[cfg(test)]
mod pointed_tests {
    use super::*;
    #[test]
    fn pointed_bands_keep_real_support_clearance_and_front_dip_across_bodies() {
        for woman in [false, true] {
            for build in 0..3 {
                let look = Look::with_choice(
                    CreatureModel::Human,
                    7,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                );
                let body = HumanoidBody::resolve(&look);
                let support = torso_shape(body, None, true);
                let mut p = Parts::default();
                pointed_band(
                    &mut p,
                    body,
                    &support,
                    Vec2::new(0.10, 0.065),
                    0.025,
                    Vec3::X,
                    Vec3::Y,
                );
                let vertices: Vec<_> = p.matte.surface_triangles().flatten().collect();
                assert!(!vertices.is_empty());
                for at in &vertices {
                    assert!(at.is_finite());
                    let d = support
                        .distance(*at - body.torso)
                        .min(pelvis_shape(body).distance(*at));
                    assert!((0.009..0.012).contains(&d), "actual support clearance {d}");
                }
                let low = vertices.iter().map(|v| v.y).fold(f32::INFINITY, f32::min);
                let high = vertices
                    .iter()
                    .map(|v| v.y)
                    .fold(f32::NEG_INFINITY, f32::max);
                assert!((low - 0.010).abs() < 1e-5);
                assert!((high - 0.125).abs() < 1e-5);
            }
        }
    }
}
