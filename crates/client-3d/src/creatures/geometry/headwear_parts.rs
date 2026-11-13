//! Hats, wreaths, bands and veils, built in the head's space over the hair.
//! Each is its own small recipe any wearer's head can carry.
use super::*;

/// Where a hat's brim sits on the head, over the hair.
const BRIM: f32 = 0.255;

/// A three-cornered hat: a rounded crown, a brim turned up on three sides
/// into walls dipping between their corners, and a gilt edge along them.
pub(super) fn tricorn(p: &mut Parts, color: Vec3, trim: Vec3) {
    let crown = crown_parts::CrownProfile::tricorn();
    crown.emit(&mut p.matte, color);
    annular_brim(
        &mut p.matte,
        crown.opening,
        crown.opening_radii(),
        Vec2::splat(0.24),
        0.0,
        color,
    );
    let corner = |k: usize| {
        let a = k as f32 * TAU / 3.0;
        Vec3::new(a.sin() * 0.255, BRIM, a.cos() * 0.255 - 0.012)
    };
    for k in 0..3 {
        let (a, b) = (corner(k), corner(k + 1));
        let mid = (a + b) * 0.5;
        let along = (b - a).normalize();
        let out = Vec3::new(mid.x, 0.0, mid.z + 0.012).normalize();
        let up = (Vec3::Y + out * 0.35).normalize();
        let half = a.distance(b) * 0.5;
        let h = 0.085;
        p.matte.extrude(
            &[
                Vec2::new(-half, 0.0),
                Vec2::new(half, 0.0),
                Vec2::new(half * 1.02, h),
                Vec2::new(0.0, h * 0.5),
                Vec2::new(-half * 1.02, h),
            ],
            mid,
            along,
            up,
            0.012,
            color,
        );
        tube(
            &mut p.metal,
            &[a + up * h, mid + up * h * 0.5 + out * 0.008, b + up * h],
            &[(0.006, 0.006); 3],
            trim,
        );
    }
}

/// A witch's hat: a wide brim, a tall cone bending back to its point, a
/// band round its foot and a bow with sheer tails falling behind.
pub(super) fn witch_hat(p: &mut Parts, color: Vec3, band: Vec3) {
    witch_hat_parts(p, color, band, Some(crown_parts::CrownProfile::witch_hat()));
}

pub(super) fn open_witch_hat(p: &mut Parts, color: Vec3, band: Vec3) {
    witch_hat_parts(p, color, band, None);
}

fn witch_hat_parts(
    p: &mut Parts,
    color: Vec3,
    band: Vec3,
    attachment: Option<crown_parts::CrownProfile>,
) {
    // Equipment wizard hats keep their established dimensions. Native hats
    // share their opening with the anatomical covered-hair profile instead.
    let opening = attachment.map_or(BRIM, |profile| profile.opening);
    let inner = attachment.map_or(Vec2::new(0.128, 0.133), |profile| profile.opening_radii());
    annular_brim(&mut p.matte, opening, inner, Vec2::splat(0.3), 0.0, color);
    let path = [
        Vec3::new(0.0, BRIM, -0.012),
        Vec3::new(0.0, BRIM + 0.11, -0.025),
        Vec3::new(0.0, BRIM + 0.22, -0.05),
        Vec3::new(0.0, BRIM + 0.31, -0.1),
        Vec3::new(0.0, BRIM + 0.36, -0.17),
    ];
    let radii = [
        (0.135, 0.14),
        (0.1, 0.104),
        (0.064, 0.066),
        (0.03, 0.03),
        (0.004, 0.004),
    ];
    // Match the horizontal annular brim at the mouth before the cone
    // begins its backward sweep; a tilted first ring leaves daylight.
    let mut open_path = path.to_vec();
    open_path[0].y = opening;
    open_path.insert(1, Vec3::new(0.0, opening + 0.025, -0.012));
    let mut open_radii: Vec<_> = radii.iter().map(|&(x, z)| Vec2::new(x, z)).collect();
    let mouth = attachment.map_or(open_radii[0], |profile| {
        profile.opening_radii() + Vec2::splat(profile.wall)
    });
    open_radii[0] = mouth;
    open_radii.insert(
        1,
        attachment.map_or(Vec2::new(0.130, 0.135), |_| mouth - Vec2::splat(0.005)),
    );
    if attachment.is_some() {
        // Resolve against only this cone in recipe-local coordinates. Existing
        // hair, the outer brim edge and the ornament cannot become support.
        let mut cone = Builder::default();
        cone.res = p.matte.res;
        cone.sides_mul = p.matte.sides_mul;
        cone.smooth = p.matte.smooth;
        cone.tube_open_start(&open_path, &open_radii, 32, &|_, _, _| color);
        let ring = witch_hat_band(&cone, opening, band);
        for tri in cone.surface_triangles() {
            p.matte.tri(tri, color);
        }
        for tri in ring.surface_triangles() {
            p.matte.tri(tri, band);
        }
    } else {
        p.matte
            .tube_open_start(&open_path, &open_radii, 10, &|_, _, _| color);
        p.matte.torus(
            Vec3::new(0.0, BRIM + 0.022, -0.015),
            Vec3::Y,
            0.128,
            0.016,
            16,
            4,
            band,
        );
    }
    // The bow at her left, and its sheer tails falling down her back.
    for lobe in [-1.0f32, 1.0] {
        p.matte.blob(
            Vec3::new(0.125, BRIM + 0.03, -0.04 + lobe * 0.035),
            Vec3::new(0.02, 0.026, 0.032),
            2,
            0.0,
            band,
        );
    }
    p.sheer.extrude(
        &[
            Vec2::new(-0.03, 0.0),
            Vec2::new(0.03, 0.0),
            Vec2::new(0.07, 0.42),
            Vec2::new(-0.05, 0.4),
        ],
        Vec3::new(0.12, BRIM + 0.02, -0.08),
        Vec3::new(0.0, 0.0, -1.0),
        Vec3::NEG_Y,
        0.004,
        band,
    );
}

/// Native ribbon attachment, separate from equipment wizard styling.
pub(super) fn witch_hat_band(cone: &Builder, opening: f32, color: Vec3) -> Builder {
    radial_parts::RoundedBandProfile {
        height: opening + 0.022,
        half_height: 0.016,
        clearance: 0.0015,
        radial_half: 0.0025,
        // Follow the curved mouth between the coarser cone meridians.
        sides: 64,
        // The curved mouth also needs intermediate vertical support samples.
        sections: 16,
    }
    .resolve(
        |height, angle, gap| {
            let origin = Vec3::new(0.0, height, -0.012);
            let out = Vec3::new(angle.sin(), 0.0, angle.cos());
            cone.outer_surface(origin, out).map(|hit| hit + out * gap)
        },
        color,
    )
    .expect("native witch cone supports its whole ribbon")
}

/// Common radial attachment frame, in the head's pre-presentation coordinates.
/// Query immutable face/hair support before emitting any matte ornament.
#[derive(Clone, Copy)]
struct BrowAttachmentProfile {
    center: Vec3,
    front_drop: f32,
    clearance: f32,
    fallback_radius: f32,
}

impl Default for BrowAttachmentProfile {
    fn default() -> Self {
        Self {
            center: Vec3::new(0.0, 0.241, -0.012),
            front_drop: 0.006,
            clearance: 0.006,
            fallback_radius: 0.10,
        }
    }
}

impl BrowAttachmentProfile {
    fn support(self, p: &Parts, height_variation: f32) -> Builder {
        // All fitting rays are horizontal. Retain every triangle which can
        // cross the recipe's complete query-height range, including its leaves.
        let reach = self.front_drop.abs() + height_variation.abs();
        radial_parts::band_support_window(&[&p.matte], self.center.y - reach, self.center.y + reach)
    }

    fn point(self, support: &Builder, angle: f32) -> Vec3 {
        let out = Vec3::new(angle.sin(), 0.0, angle.cos());
        let origin = self.center - Vec3::Y * (self.front_drop * angle.cos());
        support
            .outer_surface(origin, out)
            .unwrap_or(origin + out * self.fallback_radius)
            + out * self.clearance
    }
}

/// Leafy crown uses the same support-fitting frame as bands and wreaths.
pub(super) fn leaf_crown(p: &mut Parts, color: Vec3) {
    let profile = BrowAttachmentProfile {
        center: Vec3::new(0.0, 0.27, -0.02),
        front_drop: 0.0,
        ..BrowAttachmentProfile::default()
    };
    let support = profile.support(p, 0.012);
    // Freeze all roots first: leaf geometry must never become its own support.
    let roots: Vec<_> = (0..14)
        .map(|k| {
            let a = k as f32 / 14.0 * TAU;
            let mut at = profile;
            at.center.y += 0.012 * (a * 3.0).sin();
            at.point(&support, a)
        })
        .collect();
    for (k, root) in roots.into_iter().enumerate() {
        let a = k as f32 / 14.0 * TAU;
        let out = Vec3::new(a.sin(), 0.0, a.cos());
        let dir = (out + Vec3::Y * 0.7 + Vec3::new(a.cos(), 0.0, -a.sin()) * 0.4).normalize();
        let shade = 0.85 + 0.3 * rand(97, k as u32);
        ornament_parts::leaf(&mut p.matte, root, dir, 0.06, 0.022, color * shade);
    }
}

fn brow_path(p: &Parts) -> Vec<Vec3> {
    let profile = BrowAttachmentProfile::default();
    let support = profile.support(p, 0.0);
    sample_brow(profile, &support)
}

fn sample_brow(profile: BrowAttachmentProfile, support: &Builder) -> Vec<Vec3> {
    (0..=64)
        .map(|i| profile.point(support, i as f32 * TAU / 64.0))
        .collect()
}

/// A wreath following the wearer's hair, with leaves above the brows.
pub(super) fn wreath(p: &mut Parts, color: Vec3) {
    let profile = BrowAttachmentProfile::default();
    let support = profile.support(p, 0.0);
    let path = sample_brow(profile, &support);
    for k in -6i32..=6 {
        if k == 0 {
            continue;
        }
        let a = k as f32 * 0.24;
        let root = profile.point(&support, a);
        let back = (profile.point(&support, a + 0.02) - profile.point(&support, a - 0.02))
            .normalize_or(Vec3::X)
            * a.signum();
        let dir = (back + Vec3::Y * 0.45 + Vec3::new(a.sin(), 0.0, a.cos()) * 0.15).normalize();
        ornament_parts::leaf(&mut p.metal, root, dir, 0.05, 0.015, color);
    }
    closed_ring(&mut p.metal, &path, 0.003, color * 0.8);
}

/// A close-fitting brow band with an optional forehead plaque and jewel.
pub(super) fn circlet(p: &mut Parts, color: Vec3, gem: Option<Vec3>, broad: bool) {
    let path = brow_path(p);
    let front = path[0];
    closed_ring(
        &mut p.metal,
        &path,
        if broad { 0.009 } else { 0.004 },
        color,
    );
    if broad {
        p.metal.cuboid(
            front + Vec3::Z * 0.004,
            Vec3::new(0.025, 0.016, 0.004),
            Quat::IDENTITY,
            color,
        );
    }
    if let Some(gem) = gem {
        p.satin.blob(
            front + Vec3::Z * 0.011,
            Vec3::new(0.012, 0.014, 0.007),
            1,
            0.0,
            gem,
        );
    }
}

/// A small cap on the crown, rimmed, with a sheer veil hanging behind.
pub(super) fn veiled(p: &mut Parts, cap: Vec3, veil: Vec3) {
    let profile = crown_parts::CrownProfile::veiled();
    profile.emit(&mut p.matte, cap);
    profile.emit_rim(&mut p.metal, crown_parts::VEILED_RIM_RADIUS, GILT);
    // Bend only the top attachment strip onto the rear arc. The join row and
    // lower trapezoid keep the original veil plane and lower endpoints.
    const COLS: usize = 12;
    let axis = Vec3::new(0.0, -1.0, -0.12).normalize();
    let normal = Vec3::X.cross(axis);
    let origin = Vec3::new(0.0, 0.27, -0.13);
    let join = (0.20 - origin.y) / axis.y;
    let rim_x = profile.rim_point(PI * 0.5).x;
    let at = |row: usize, col: usize| {
        let u = col as f32 / COLS as f32 * 2.0 - 1.0;
        if row == 0 {
            let angle = PI - (u * 0.08 / rim_x).asin();
            profile.rim_point(angle) + Vec3::Y * 0.002
        } else {
            let distance = if row == 1 { join } else { 0.46 };
            origin + axis * distance + Vec3::X * u * (0.08 + 0.07 * distance / 0.46)
        }
    };
    for row in 0..2 {
        for col in 0..COLS {
            let base = [
                at(row, col),
                at(row, col + 1),
                at(row + 1, col + 1),
                at(row + 1, col),
            ];
            // The bent top quad is not planar. Each thickness face must be
            // oriented from its own triangle center, not a shared quad center.
            for indices in [[0, 1, 2], [0, 2, 3]] {
                let triangle = indices.map(|i| base[i]);
                let inside = (triangle[0] + triangle[1] + triangle[2]) / 3.0;
                for sign in [-1.0, 1.0] {
                    p.sheer
                        .tri_out(triangle.map(|v| v + normal * (sign * 0.002)), inside, veil);
                }
            }
        }
    }
    // Close the original four-millimetre sheet without interior seam faces.
    for row in [0, 2] {
        for col in 0..COLS {
            let a = at(row, col);
            let b = at(row, col + 1);
            p.sheer.quad_out(
                [
                    a - normal * 0.002,
                    b - normal * 0.002,
                    b + normal * 0.002,
                    a + normal * 0.002,
                ],
                (at(1, col) + at(1, col + 1)) * 0.5,
                veil,
            );
        }
    }
    for col in [0, COLS] {
        for row in 0..2 {
            let a = at(row, col);
            let b = at(row + 1, col);
            p.sheer.quad_out(
                [
                    a - normal * 0.002,
                    b - normal * 0.002,
                    b + normal * 0.002,
                    a + normal * 0.002,
                ],
                (at(row, COLS / 2) + at(row + 1, COLS / 2)) * 0.5,
                veil,
            );
        }
    }
}

/// The braid begins at this cap's own rear opening, not a second nominal skull.
pub(super) fn winged_helm_hair_root() -> Vec3 {
    let profile = crown_parts::CrownProfile::winged();
    // The visible top of the first lobe nests in the lower half of the 6 mm
    // rim. Fit the lobe envelope, not just its center, below the shell opening.
    profile.rim_point(PI) - Vec3::Y * (hair::BRAID_ROOT_RADIUS - 0.002)
}

/// The head recipe routes the complete hairstyle through this attachment.
pub(super) fn winged_helm_braid(p: &mut Parts, color: Vec3) {
    hair::BraidProfile {
        root: winged_helm_hair_root(),
        // The lower plait is an authored hairstyle landmark, independent of
        // the cap opening. Retain it while fitting the upper insertion.
        end: Vec3::new(0.0, -0.045, -0.155),
    }
    .emit(p, color);
}

/// An open, lined cap follows the uncovered cranium. Feather dimensions remain
/// authored; only their attachment roots follow the cap they actually decorate.
pub(super) fn winged_helm(p: &mut Parts, metal: Vec3, feathers: Vec3) {
    let profile = crown_parts::CrownProfile::winged();
    profile.emit(&mut p.metal, metal);
    profile.emit_rim(&mut p.metal, 0.006, GILT);
    let front = profile.rim_point(0.0);
    p.metal.blob(
        front + Vec3::new(0.0, 0.013, 0.003),
        Vec3::new(0.018, 0.022, 0.008),
        1,
        0.0,
        GILT,
    );
    for side in [-1.0f32, 1.0] {
        let root = profile.outer_point(side * PI * 0.5, 0.20);
        for k in 0..6 {
            let t = k as f32 / 5.0;
            feather_parts::vane(
                &mut p.matte,
                profile.outer_point(side * (PI * 0.5 - t * 0.42), 0.20),
                Vec3::new(side * (0.30 + t * 0.5), 1.0 - t * 0.35, -0.25),
                0.22 - t * 0.07,
                0.022,
                Vec3::Z,
                if k < 3 { feathers } else { CREAM },
                Some(GILT),
            );
        }
        p.metal.blob(root, Vec3::splat(0.025), 1, 0.0, GILT);
    }
}

/// Open arches with gem-set points; the supporting band follows actual hair.
pub(super) fn crown(p: &mut Parts, metal: Vec3, gem: Vec3) {
    let path = brow_path(p);
    closed_ring(&mut p.metal, &path, 0.004, metal);
    for k in 0..8 {
        let index = k * 8;
        let a = path[index];
        let b = path[index + 8];
        let peak = path[index + 4] + Vec3::Y * (if k == 0 || k == 7 { 0.095 } else { 0.065 });
        let points = [
            a,
            a.lerp(peak, 0.55) + Vec3::Y * 0.009,
            peak,
            b.lerp(peak, 0.55) + Vec3::Y * 0.009,
            b,
        ];
        tube(&mut p.metal, &points, &[(0.003, 0.003); 5], metal);
        p.metal.blob(peak, Vec3::splat(0.005), 1, 0.0, metal);
        let station = index + 4;
        let tangent = (path[station + 1] - path[station - 1])
            .with_y(0.0)
            .normalize();
        crown_setting(p, path[station], tangent.cross(Vec3::Y), metal, gem);
    }
}

/// One band-mounted crown setting. Every station uses this same small recipe;
/// the support path owns its position and the recipe owns its style dimensions.
fn crown_setting(p: &mut Parts, mount: Vec3, outward: Vec3, metal: Vec3, gem: Vec3) {
    let jewel = mount + Vec3::Y * 0.035;
    // The setting and its connector share the fitted band station. Run the
    // stem into both solids rather than leaving a tangent-only join.
    tube(&mut p.metal, &[mount, jewel], &[(0.0025, 0.0025); 2], metal);
    radial_parts::mounted_oval(
        &mut p.metal,
        jewel,
        outward,
        Vec3::new(0.012, 0.021, 0.009),
        metal,
    );
    radial_parts::mounted_oval(
        &mut p.satin,
        jewel + outward * 0.005,
        outward,
        Vec3::new(0.008, 0.015, 0.008),
        gem,
    );
}

#[cfg(test)]
mod brow_attachment_tests {
    use super::*;

    #[test]
    fn crown_gems_offset_along_the_fitted_band_normal() {
        let mut p = Parts::default();
        p.matte.blob(
            Vec3::new(0.0, 0.22, -0.012),
            Vec3::new(0.15, 0.16, 0.08),
            3,
            0.0,
            Vec3::ONE,
        );
        let path = brow_path(&p);
        crown(&mut p, Vec3::ONE, Vec3::X);
        let faces: Vec<_> = p.satin.surface_triangles().collect();
        assert_eq!(faces.len() % 8, 0);
        for (k, gem) in faces.chunks(faces.len() / 8).enumerate() {
            let i = k * 8 + 4;
            let tangent = (path[i + 1] - path[i - 1]).with_y(0.0).normalize();
            let outward = tangent.cross(Vec3::Y);
            let center = gem.iter().flatten().copied().sum::<Vec3>() / (gem.len() * 3) as f32;
            let expected = path[i] + Vec3::Y * 0.035 + outward * 0.005;
            assert!(
                center.distance(expected) < 0.0001,
                "station {k}: {center:?} != {expected:?}"
            );
        }
    }

    #[test]
    fn crown_settings_have_metal_support_from_the_fitted_band() {
        for radius in [0.08, 0.11, 0.15] {
            let mut p = Parts::default();
            p.matte.blob(
                Vec3::new(0.0, 0.22, -0.012),
                Vec3::new(radius, 0.16, radius * 0.8),
                3,
                0.0,
                Vec3::ONE,
            );
            let path = brow_path(&p);
            crown(&mut p, Vec3::ONE, Vec3::X);
            // The authored setting starts 14 mm above its band station.
            // Probe that gap, not the gem or hair, for a real metal mount.
            for k in 0..8 {
                let base = path[k * 8 + 4];
                let outward = Vec3::new(base.x, 0.0, base.z + 0.012).normalize();
                for rise in [0.006, 0.009, 0.012] {
                    let expected = base + Vec3::Y * rise;
                    let hit = p.metal.outer_surface(expected - outward * 0.02, outward);
                    assert!(hit.is_some_and(|at| at.distance(expected) < 0.004), "crown setting {k} needs a metal connection at rise {rise}, radius {radius}: {hit:?}");
                }
            }
        }
    }

    #[test]
    fn circlet_is_a_periodic_band_without_front_cap_fans() {
        for radius in [0.08, 0.11, 0.15] {
            let mut p = Parts::default();
            p.matte.blob(
                Vec3::new(0.0, 0.22, -0.012),
                Vec3::new(radius, 0.16, radius * 0.8),
                3,
                0.0,
                Vec3::ONE,
            );
            let path = brow_path(&p);
            circlet(&mut p, Vec3::ONE, None, false);
            let faces = p.metal.surface_triangles().collect::<Vec<_>>();
            assert!(
                !faces
                    .iter()
                    .flatten()
                    .any(|point| point.distance(path[0]) < 1e-6),
                "a closed band has no cap fan at its seam"
            );
            super::super::bracelet_tests::closed_ring(&faces);
        }
    }

    #[test]
    fn shared_brow_frame_tracks_translated_scaled_support_and_closes_seam() {
        for center in [Vec3::ZERO, Vec3::new(0.12, 0.08, -0.06)] {
            for radius in [0.08, 0.16] {
                let mut support = Parts::default();
                support
                    .matte
                    .cuboid(center, Vec3::splat(radius), Quat::IDENTITY, Vec3::ONE);
                let profile = BrowAttachmentProfile {
                    center,
                    front_drop: 0.0,
                    clearance: 0.006,
                    fallback_radius: 0.1,
                };
                for k in 0..16 {
                    let angle = k as f32 * TAU / 16.0;
                    let point = profile.point(&support.matte, angle);
                    let distance = point.distance(center);
                    assert!(
                        (distance - (radius / angle.sin().abs().max(angle.cos().abs()) + 0.006))
                            .abs()
                            < 1e-5,
                        "attachment must track support, without clipping or floating: {distance}"
                    );
                }
                assert!(
                    profile
                        .point(&support.matte, 0.0)
                        .distance(profile.point(&support.matte, TAU))
                        < 1e-6
                );
            }
        }
        let empty = Parts::default();
        let profile = BrowAttachmentProfile::default();
        let origin = profile.center - Vec3::Y * profile.front_drop;
        assert!((profile.point(&empty.matte, 0.0).distance(origin) - 0.106).abs() < 1e-6);
    }
}

#[cfg(test)]
mod brow_window_tests {
    use super::*;
    #[test]
    fn brow_window_preserves_actual_head_hits_and_excludes_unreachable_faces() {
        for woman in [false, true] {
            for hair_style in [0, 2] {
                let look = Look::with_choice(
                    CreatureModel::Human,
                    7,
                    Some(Variant {
                        woman,
                        hair: hair_style,
                        ..default()
                    }),
                );
                let mut parts = Parts::default();
                human_face(&mut parts, &look, true);
                if shows_hair(&look) {
                    hair(&mut parts, &look, true);
                }
                let full_count = parts.matte.surface_triangles().count();
                for (profile, variation) in [
                    (BrowAttachmentProfile::default(), 0.0),
                    (
                        BrowAttachmentProfile {
                            center: Vec3::new(0.0, 0.27, -0.02),
                            front_drop: 0.0,
                            ..default()
                        },
                        0.012,
                    ),
                ] {
                    let support = profile.support(&parts, variation);
                    for k in 0..128 {
                        let a = k as f32 * TAU / 128.0;
                        for offset in [-variation, 0.0, variation] {
                            let mut at = profile;
                            at.center.y += offset;
                            assert_eq!(
                                at.point(&support, a),
                                at.point(&parts.matte, a),
                                "filtered hits must match actual full hair/face support"
                            );
                        }
                    }
                    let filtered_count = support.surface_triangles().count();
                    println!("brow support woman={woman} hair={hair_style} variation={variation}: {filtered_count}/{full_count} triangles");
                    assert!(
                        filtered_count * 2 < full_count,
                        "brow rays must not scan unreachable head triangles"
                    );
                }
            }
        }
    }
}
