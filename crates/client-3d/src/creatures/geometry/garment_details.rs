//! Portable attached garment details, fitted to the resolved wearer.
use super::ornament_parts::leaf;
use super::*;
use crate::creatures::outfit::{GarmentDetails, HipDecoration};

/// Resolved emission recipes, retained only until the accessory cache lookup.
/// Re-emitting on a cold miss preserves real slits, trim and mesh diagonals.
pub(super) enum PanelSupport {
    Skirt {
        surface: ClothSurface,
        skirt: Box<Skirt>,
        scale: Vec3,
    },
    Peplum {
        surface: ClothSurface,
        color: Vec3,
        trim: Option<Vec3>,
        offset: Vec3,
    },
}

impl PanelSupport {
    fn triangles(self) -> Vec<[Vec3; 3]> {
        let mut mesh = Builder::default();
        let (scale, offset) = match self {
            Self::Skirt {
                surface,
                skirt,
                scale,
            } => {
                skirt_cloth(&mut mesh, surface, *skirt);
                (scale, Vec3::ZERO)
            }
            Self::Peplum {
                surface,
                color,
                trim,
                offset,
            } => {
                draped_cloth(&mut mesh, surface, color, trim);
                (Vec3::ONE, offset)
            }
        };
        mesh.surface_triangles()
            .map(|tri| tri.map(|p| p * scale + offset))
            .collect()
    }
}

// Clip a convex polygon against a linear half-plane, retaining the full 3D
// interpolation. Used for the pattern footprint and exact fold boundaries.
fn clip_panel(poly: &[Vec3], distance: impl Fn(Vec3) -> f32) -> Vec<Vec3> {
    let mut result = Vec::new();
    if poly.is_empty() {
        return result;
    }
    for (a, b) in poly
        .iter()
        .copied()
        .zip(poly.iter().copied().cycle().skip(1))
        .take(poly.len())
    {
        let (da, db) = (distance(a), distance(b));
        if da >= 0.0 {
            result.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            result.push(a.lerp(b, da / (da - db)));
        }
    }
    result
}

fn panel_outline(build: f32) -> [Vec2; 5] {
    [
        Vec2::new(-0.075 * build, 0.0),
        Vec2::new(0.075 * build, 0.0),
        Vec2::new(0.09 * build, -0.4),
        Vec2::new(0.0, -0.44),
        Vec2::new(-0.09 * build, -0.4),
    ]
}

/// Least concave majorant of downward-station/displacement constraints.
/// Unlike a height neighborhood this never imports a lower obstacle at the root.
fn upper_fold(mut points: Vec<Vec2>) -> Vec<Vec2> {
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    // The same geometric station reconstructed through different triangle
    // diagonals can differ by a few float ulps. Different demands at those
    // almost identical stations must not create a vertical jump in the shell.
    // Raise BOTH ends of each sub-micron group to its maximum: concavity then
    // bounds every original constraint without moving or averaging a peak.
    let mut unique: Vec<Vec2> = Vec::new();
    let mut first = 0;
    while first < points.len() {
        let mut end = first + 1;
        let mut height = points[first].y;
        while end < points.len() && points[end].x - points[first].x <= 1e-6 {
            height = height.max(points[end].y);
            end += 1;
        }
        unique.push(Vec2::new(points[first].x, height));
        if points[end - 1].x != points[first].x {
            unique.push(Vec2::new(points[end - 1].x, height));
        }
        first = end;
    }
    let mut hull: Vec<Vec2> = Vec::new();
    for p in unique {
        while hull.len() >= 2 {
            let a = hull[hull.len() - 2];
            let b = hull[hull.len() - 1];
            if (b - a).perp_dot(p - b) < 0.0 {
                break;
            }
            hull.pop();
        }
        hull.push(p);
    }
    hull
}

pub(super) fn fold_shift(fold: &[Vec2], station: f32) -> f32 {
    for pair in fold.windows(2) {
        if station <= pair[1].x {
            let t = ((station - pair[0].x) / (pair[1].x - pair[0].x)).clamp(0.0, 1.0);
            return pair[0].y + t * (pair[1].y - pair[0].y);
        }
    }
    fold.last().map_or(0.0, |p| p.y)
}

pub(super) fn panel_fold(support: &[[Vec3; 3]], body: HumanoidBody, front: f32) -> Vec<Vec2> {
    let down = Vec3::new(0.0, 1.0, -0.12 * front).normalize();
    let origin = Vec3::new(0.0, 0.04, front * (body.pelvis_radii().z + 0.03));
    pattern_fold(
        support,
        &panel_outline(body.build),
        origin,
        down,
        0.012,
        front,
    )
}

/// Fit a closed hanging pattern against actual lower-layer triangles. Both
/// native loincloth and ragged apron retain their own pattern and root frame.
fn pattern_fold(
    support: &[[Vec3; 3]],
    outline: &[Vec2],
    origin: Vec3,
    down: Vec3,
    thickness: f32,
    front: f32,
) -> Vec<Vec2> {
    let length = outline.iter().map(|p| -p.y).fold(0.0, f32::max);
    let mut original = Builder::default();
    original.extrude(outline, origin, Vec3::X, down, thickness, Vec3::ONE);
    let mut demands = vec![Vec2::ZERO, Vec2::new(length, 0.0)];
    // Bound the complete closed shell, not only its inner face: the tilted
    // thickness exposes a 1.43mm top rim above that face's projected footprint.
    // At every clipped overlap vertex both depth and pattern station are affine.
    // A concave shift dominating those vertices therefore clears the interior.
    for panel in original.surface_triangles() {
        let [a, b, c] = panel;
        let (u, v) = (b.truncate() - a.truncate(), c.truncate() - a.truncate());
        let det = u.perp_dot(v);
        if det.abs() < 1e-12 {
            continue;
        }
        let low = panel
            .into_iter()
            .fold(Vec3::splat(f32::INFINITY), Vec3::min);
        let high = panel
            .into_iter()
            .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
        for tri in support {
            let support_low = tri
                .iter()
                .copied()
                .fold(Vec3::splat(f32::INFINITY), Vec3::min);
            let support_high = tri
                .iter()
                .copied()
                .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
            if support_high.x < low.x
                || support_low.x > high.x
                || support_high.y < low.y
                || support_low.y > high.y
            {
                continue;
            }
            let mut poly = tri.to_vec();
            for (start, end) in [(a, b), (b, c), (c, a)] {
                poly = clip_panel(&poly, |p| {
                    det.signum()
                        * (end.truncate() - start.truncate())
                            .perp_dot(p.truncate() - start.truncate())
                });
            }
            for p in poly {
                let q = p.truncate() - a.truncate();
                let base = a + (b - a) * (q.perp_dot(v) / det) + (c - a) * (u.perp_dot(q) / det);
                let station = (-(base - origin).dot(down)).clamp(0.0, length);
                demands.push(Vec2::new(
                    station,
                    (front * (p.z - base.z) + 0.001).max(0.0),
                ));
            }
        }
    }
    upper_fold(demands)
}

/// Slice the original closed extrusion at fold stations, then move paired
/// vertices equally. Orthogonal pattern coordinates preserve the authored
/// thickness vector, including its Y component, instead of sampling world Y.
#[allow(clippy::too_many_arguments)]
fn folded_extrusion(
    builder: &mut Builder,
    outline: &[Vec2],
    origin: Vec3,
    down: Vec3,
    thickness: f32,
    color: Vec3,
    front: f32,
    fold: &[Vec2],
) {
    if fold.iter().all(|p| p.y == 0.0) {
        builder.extrude(outline, origin, Vec3::X, down, thickness, color);
        return;
    }
    let mut original = Builder::default();
    original.extrude(outline, origin, Vec3::X, down, thickness, color);
    let station = |p: Vec3| -(p - origin).dot(down);
    for tri in original.surface_triangles() {
        let low = tri
            .iter()
            .map(|p| station(*p))
            .fold(f32::INFINITY, f32::min);
        let high = tri
            .iter()
            .map(|p| station(*p))
            .fold(f32::NEG_INFINITY, f32::max);
        let mut cuts = vec![low];
        cuts.extend(
            fold.iter()
                .map(|p| p.x)
                .filter(|s| *s > low + 1e-7 && *s < high - 1e-7),
        );
        cuts.push(high);
        for range in cuts.windows(2) {
            let mut poly = clip_panel(&tri, |p| station(p) - range[0]);
            poly = clip_panel(&poly, |p| range[1] - station(p));
            for i in 1..poly.len().saturating_sub(1) {
                let shifted = [poly[0], poly[i], poly[i + 1]]
                    .map(|p| p + Vec3::Z * front * fold_shift(fold, station(p)));
                if (shifted[1] - shifted[0])
                    .cross(shifted[2] - shifted[0])
                    .length_squared()
                    > 1e-16
                {
                    builder.tri(shifted, color);
                }
            }
        }
    }
}

pub(super) fn fitted_panel(
    builder: &mut Builder,
    body: HumanoidBody,
    front: f32,
    fold: &[Vec2],
    cloth: Vec3,
    edge: Vec3,
) {
    let b = body.build;
    let down = Vec3::new(0.0, 1.0, -0.12 * front).normalize();
    let origin = Vec3::new(0.0, 0.04, front * (body.pelvis_radii().z + 0.03));
    folded_extrusion(
        builder,
        &panel_outline(b),
        origin,
        down,
        0.012,
        cloth,
        front,
        fold,
    );
    folded_extrusion(
        builder,
        &[
            Vec2::new(-0.088 * b, -0.33),
            Vec2::new(0.088 * b, -0.33),
            Vec2::new(0.09 * b, -0.36),
            Vec2::new(-0.09 * b, -0.36),
        ],
        Vec3::new(0.0, 0.04, front * (body.pelvis_radii().z + 0.037)),
        down,
        0.004,
        edge,
        front,
        fold,
    );
}

#[cfg(test)]
mod panel_fold_tests {
    use super::*;

    #[test]
    fn concave_fold_dominates_peaks_and_keeps_endpoints() {
        let points = vec![
            Vec2::ZERO,
            Vec2::new(0.03, 0.007),
            Vec2::new(0.18, 0.028),
            Vec2::new(0.3, 0.002),
            Vec2::new(0.44, 0.0),
        ];
        let hull = upper_fold(points.clone());
        assert_eq!(hull.first(), points.first());
        assert_eq!(hull.last(), points.last());
        for p in points {
            assert!(fold_shift(&hull, p.x) >= p.y - 1e-7);
        }
        assert!(hull.len() < 5);
    }

    #[test]
    fn coincident_root_constraints_keep_paired_shell_thickness() {
        // Native male rear witness: independently reconstructed top-cap
        // stations used to assign different shifts to the two shell faces.
        let points = vec![
            Vec2::new(0.0, 0.011286167),
            Vec2::new(7.566996e-10, 0.012781171),
            Vec2::new(1.1874363e-8, 0.012781186),
            Vec2::new(0.0059944885, 0.014765007),
            Vec2::new(0.014387629, 0.015496386),
            Vec2::new(0.2285091, 0.021161778),
            Vec2::new(0.44, 0.0),
        ];
        let hull = upper_fold(points.clone());
        assert_eq!(hull[0], Vec2::new(0.0, 0.012781186));
        for p in points {
            assert!(fold_shift(&hull, p.x) >= p.y - 1e-8);
        }
        let down = Vec3::new(0.0, 1.0, 0.12).normalize();
        let outward = -Vec3::X.cross(down);
        let origin = Vec3::new(0.0, 0.04, -0.17);
        let outline = panel_outline(1.0904);
        let mut mesh = Builder::default();
        folded_extrusion(
            &mut mesh,
            &outline,
            origin,
            down,
            0.012,
            Vec3::ONE,
            -1.0,
            &hull,
        );
        let points: Vec<_> = mesh.surface_triangles().flatten().collect();
        for x in [outline[0].x, outline[1].x] {
            let pair = [-1.0, 1.0].map(|sign| {
                let original = origin + Vec3::X * x + outward * (sign * 0.006);
                points
                    .iter()
                    .copied()
                    .find(|p| (p.x - original.x).abs() < 1e-7 && (p.y - original.y).abs() < 1e-7)
                    .expect("both authored root vertices survive")
            });
            assert!(
                (pair[1] - pair[0] - outward * 0.012).length() < 1e-7,
                "numerical root stations must not split paired shell: {pair:?}"
            );
        }
    }

    #[test]
    fn nearby_interior_constraints_are_conservatively_coalesced() {
        let points = vec![
            Vec2::ZERO,
            Vec2::new(0.2, 0.02),
            Vec2::new(0.20000002, 0.03),
            Vec2::new(0.44, 0.0),
        ];
        let hull = upper_fold(points.clone());
        for p in points {
            assert!(fold_shift(&hull, p.x) >= p.y - 1e-8);
        }
        assert_eq!(fold_shift(&hull, 0.2), 0.03);
        assert_eq!(fold_shift(&hull, 0.20000002), 0.03);
    }

    #[test]
    fn clipped_triangle_retains_interpolated_depth() {
        let tri = [
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 2.0),
            Vec3::new(0.0, 1.0, 1.0),
        ];
        let poly = clip_panel(&tri, |p| p.x);
        assert!(poly.iter().all(|p| p.x >= 0.0));
        assert!(poly
            .iter()
            .any(|p| p.distance(Vec3::new(0.0, 0.0, 1.0)) < 1e-6));
        for p in poly {
            assert!((p.z - p.x - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn zero_fold_retains_original_extrusion_bits() {
        let outline = panel_outline(1.0);
        let down = Vec3::new(0.0, 1.0, -0.12).normalize();
        let mut original = Builder::default();
        original.extrude(&outline, Vec3::ZERO, Vec3::X, down, 0.012, Vec3::ONE);
        let mut folded = Builder::default();
        folded_extrusion(
            &mut folded,
            &outline,
            Vec3::ZERO,
            down,
            0.012,
            Vec3::ONE,
            1.0,
            &[Vec2::ZERO, Vec2::new(0.44, 0.0)],
        );
        assert_eq!(
            original.surface_triangles().collect::<Vec<_>>(),
            folded.surface_triangles().collect::<Vec<_>>()
        );
    }
}

/// Rigid details share the existing hip mesh and its presentation transform.
/// Keep this separate from `spawn`: no extra attachment or drape is required.
pub(super) fn emit_hip_static(
    p: &mut Parts,
    details: GarmentDetails,
    body: HumanoidBody,
    tunic: Vec3,
) {
    let Some(HipDecoration::TunicTatters { shade }) = details.hip else {
        return;
    };
    for k in 0..5 {
        let a = k as f32 * 1.3;
        let at = Vec3::new(a.cos() * 0.2 * body.profile.hips, -0.2, a.sin() * 0.2);
        p.matte.prism(
            at,
            at + Vec3::new(a.cos() * 0.03 * body.profile.hips, -0.1, a.sin() * 0.03),
            0.04,
            0.0,
            3,
            tunic * shade,
            0.0,
        );
    }
}

/// Custom belt hardware shares the band's output frame and fitted support.
/// An explicit independent pouch takes precedence over the legacy bundled bag.
pub(super) fn emit_belt_static(
    p: &mut Parts,
    outfit: Outfit,
    body: HumanoidBody,
    support: &crate::sdf::Sculpt,
) {
    let Some(HipDecoration::BucklePouch { metal, pouch }) = outfit.details.hip else {
        return;
    };
    let mut at = waist_parts::point(body, support, BELT_CENTER_Y, 0.0, 0.011);
    // A rigid frame spans several waist heights: its lower corners must
    // clear the fuller support too, not just the centre of the belt.
    for x in [-0.045, 0.0, 0.045] {
        for y in [-0.035, 0.0, 0.035] {
            let origin = Vec3::new(x, BELT_CENTER_Y + y, 0.0) - body.torso;
            if let Some(hit) = fitting::surface_ray_hit(support, origin, Vec3::Z, 0.006, 0.7) {
                at.z = at.z.max(hit.z);
            }
        }
    }
    fitting::buckle_frame(
        &mut p.metal,
        at,
        Quat::IDENTITY,
        Vec2::new(0.045, 0.035),
        metal,
    );
    if outfit.pouch.is_none() {
        waist_parts::pouch(&mut p.satin, body, support, pouch);
    }
}

pub(super) fn spawn(
    kit: &mut Kit,
    details: GarmentDetails,
    body: HumanoidBody,
    hips: Entity,
    waist_support: &crate::sdf::Sculpt,
    support: Vec<PanelSupport>,
) {
    let b = body.build;
    let pelvis = body.pelvis_radii();
    match details.hip {
        Some(HipDecoration::Loincloth {
            cloth: red,
            edge,
            ring,
        }) => {
            // Support reconstruction and fitting run only on this cached mesh's miss.
            kit.drape(
                hips,
                Transform::IDENTITY,
                Drape::Skirt(0.4),
                "loincloth",
                |p| {
                    let support: Vec<_> = support
                        .into_iter()
                        .flat_map(PanelSupport::triangles)
                        .collect();
                    let folds = [1.0, -1.0].map(|front| panel_fold(&support, body, front));
                    for (front, fold) in [1.0, -1.0].into_iter().zip(&folds) {
                        fitted_panel(&mut p.matte, body, front, fold, red, edge);
                    }
                    let original_lower = original_ring_contact(body);
                    let down = Vec3::new(0.0, 1.0, -0.12).normalize();
                    let station =
                        -(original_lower - Vec3::new(0.0, 0.04, pelvis.z + 0.03)).dot(down);
                    let lower = original_lower + Vec3::Z * fold_shift(&folds[0], station);
                    if let Some((center, normal)) = loincloth_ring_mount(body, waist_support, lower)
                    {
                        p.metal.torus(center, normal, 0.035, 0.01, 14, 4, ring);
                    } else {
                        // A failed fit is visible and diagnosable, never a floating substitute.
                        warn!("Loincloth ring fit rejected for build {b}, lower={lower:?}");
                    }
                },
            );
        }

        Some(HipDecoration::RaggedApron { cloth: kilt, apron }) => {
            // Ragged strips of the kilt and a hanging leather apron.
            kit.drape(hips, Transform::IDENTITY, Drape::Skirt(0.3), "rags", |p| {
                tatters(
                    &mut p.matte,
                    Vec3::Y * -0.16,
                    Vec2::new(pelvis.x * 1.15, pelvis.z * 1.15),
                    16,
                    0.14,
                    0.03,
                    kilt,
                    11,
                );
                let mut support: Vec<_> = support
                    .into_iter()
                    .flat_map(PanelSupport::triangles)
                    .collect();
                // The same-drape tatters are real lower cloth too.
                support.extend(p.matte.surface_triangles());
                let outline = [
                    Vec2::new(-0.08 * b, 0.0),
                    Vec2::new(0.08 * b, 0.0),
                    Vec2::new(0.07 * b, -0.32),
                    Vec2::new(-0.07 * b, -0.32),
                ];
                let origin = Vec3::new(0.0, 0.03, pelvis.z + 0.035);
                let down = Vec3::new(0.0, 1.0, -0.1).normalize();
                let fold = pattern_fold(&support, &outline, origin, down, 0.012, 1.0);
                folded_extrusion(
                    &mut p.satin,
                    &outline,
                    origin,
                    down,
                    0.012,
                    apron,
                    1.0,
                    &fold,
                );
            });
        }
        Some(HipDecoration::Leaves { cloth: green, trim }) => {
            // Two layers of long leaves over the short skirt.
            kit.drape(
                hips,
                Transform::IDENTITY,
                Drape::Skirt(0.25),
                "leaves",
                |p| {
                    // Each leaf is pinned to the sculpted hips it hangs from.
                    let form = super::pelvis_shape(body);
                    for layer in 0..2 {
                        let count = 13 + layer * 3;
                        for k in 0..count {
                            let a = (k as f32 + layer as f32 * 0.5) / count as f32 * TAU;
                            let out = Vec3::new(a.sin(), 0.0, a.cos());
                            let root =
                                super::pelvis_surface(&form, 0.04 - layer as f32 * 0.09, out)
                                    + out * 0.006;
                            let dir = (Vec3::NEG_Y + out * 0.35).normalize();
                            let shade = 0.8 + 0.35 * rand(99, k + layer * 40);
                            leaf(
                                &mut p.matte,
                                root,
                                dir,
                                0.16 + 0.05 * rand(100, k),
                                0.045,
                                green.lerp(trim, 0.25 * layer as f32) * shade,
                            );
                        }
                    }
                },
            );
        }
        Some(HipDecoration::TornHem { length, cloth }) => {
            // Torn hems below the robe-skirt, and rags from the sleeves.
            let hem = Vec2::new(
                (0.208 + length * 0.045) * b * body.hips_width,
                (0.208 + length * 0.045) * 0.74 * b,
            );
            kit.drape(
                hips,
                Transform::IDENTITY,
                Drape::Skirt(length + 0.24),
                "tatters",
                |p| {
                    tatters(
                        &mut p.matte,
                        Vec3::Y * (0.06 - length),
                        hem,
                        22,
                        0.24,
                        0.035,
                        cloth,
                        21,
                    );
                },
            );
        }
        Some(HipDecoration::TunicTatters { .. } | HipDecoration::BucklePouch { .. }) | None => {}
    }
    if let Some(trim) = details.shoulder_discs {
        // Bronze discs on the shoulders.
        for (arm, side) in [(J::ArmL, 1.0f32), (J::ArmR, -1.0)] {
            if let Some(joint) = kit.rig.joint(arm) {
                kit.attach(joint, Transform::IDENTITY, &format!("disc{side}"), |p| {
                    ornament_parts::shoulder_disc(p, side, b, trim);
                });
            }
        }
    }

    if let Some(sleeve) = details.sleeve_tatters {
        for (fore, side) in [(J::ForeL, 1.0f32), (J::ForeR, -1.0)] {
            if let Some(joint) = kit.rig.joint(fore) {
                kit.attach(joint, Transform::IDENTITY, &format!("rags{side}"), |p| {
                    tatters(
                        &mut p.matte,
                        Vec3::Y * -0.06,
                        Vec2::new(0.055 * b, 0.055 * b),
                        6,
                        0.16,
                        0.024,
                        sleeve,
                        31 + side as u32,
                    );
                });
            }
        }
    }
}

/// A flat ribbon of cloth hanging from `top`, its end torn to a ragged point.
#[allow(clippy::too_many_arguments)]
fn strip(
    b: &mut Builder,
    top: Vec3,
    out: Vec3,
    across: Vec3,
    length: f32,
    width: f32,
    color: Vec3,
    seed: u32,
) {
    let flare = out.with_y(0.0).normalize_or(Vec3::Z);
    let sway = across * (rand(91, seed) - 0.5) * 0.04;
    let path = [
        top,
        top + Vec3::NEG_Y * length * 0.35 + flare * length * 0.08,
        top + Vec3::NEG_Y * length * 0.72 + flare * length * 0.14 + sway * 0.5,
        top + Vec3::NEG_Y * length + flare * length * 0.17 + sway,
    ];
    let ragged = 0.35 + 0.4 * rand(92, seed);
    let radii = [
        Vec2::new(width, 0.006),
        Vec2::new(width * 0.95, 0.006),
        Vec2::new(width * ragged, 0.005),
        Vec2::splat(0.002),
    ];
    b.tube_framed(
        &path,
        &radii,
        6,
        &|_, _, t| color * (1.0 - 0.22 * t),
        Some(across),
    );
}

/// A ring of torn hanging strips around an elliptical hem.
#[allow(clippy::too_many_arguments)]
fn tatters(
    b: &mut Builder,
    center: Vec3,
    radius: Vec2,
    count: u32,
    length: f32,
    width: f32,
    color: Vec3,
    seed: u32,
) {
    for k in 0..count {
        let a = (k as f32 + rand(93, seed + k) * 0.4) / count as f32 * TAU;
        let out = Vec3::new(a.sin(), 0.0, a.cos());
        let top = center + Vec3::new(a.sin() * radius.x, 0.0, a.cos() * radius.y);
        let across = Vec3::new(a.cos(), 0.0, -a.sin());
        let long = length * (0.55 + 0.6 * rand(94, seed + k));
        let shade = 0.82 + 0.3 * rand(95, seed + k);
        strip(b, top, out, across, long, width, color * shade, seed + k);
    }
}

/// Rigid hardware joins the fitted cloth to its actual waist support. Solve
/// for a diameter, not an arbitrary chord that would stretch the ring or leave
/// either end hanging in space. The tube stands 0.5mm off both contact anchors.
fn original_ring_contact(body: HumanoidBody) -> Vec3 {
    let cloth_normal = Vec3::new(0.0, 0.12, 1.0).normalize();
    Vec3::new(0.0, 0.025, body.pelvis_radii().z + 0.03 + 0.015 * 0.12) + cloth_normal * 0.006
}

fn loincloth_ring_mount(
    body: HumanoidBody,
    waist: &crate::sdf::Sculpt,
    lower: Vec3,
) -> Option<(Vec3, Vec3)> {
    let scale = storybook_part(Plan::Biped, J::Torso);
    let upper = |height: f32| {
        // Query in torso-local coordinates, then restore the actual joint offset.
        let unscaled_height = body.torso.y + (height - body.torso.y) / scale.y;
        let point = waist_parts::point(body, waist, unscaled_height, 0.0, 0.003);
        (point - body.torso) * scale + body.torso
    };
    let mut low = 0.065;
    let mut high = 0.095;
    if upper(low).distance(lower) > 0.07 || upper(high).distance(lower) < 0.07 {
        warn!(
            "Loincloth ring bracket rejected: lower={lower:?}, spans=[{}, {}]",
            upper(low).distance(lower),
            upper(high).distance(lower)
        );
        return None;
    }
    for _ in 0..24 {
        let middle = (low + high) * 0.5;
        if upper(middle).distance(lower) < 0.07 {
            low = middle;
        } else {
            high = middle;
        }
    }
    debug!(
        "Loincloth ring fitted: lower={lower:?}, upper={:?}",
        upper((low + high) * 0.5)
    );
    ring_from_diameter(lower, upper((low + high) * 0.5))
}

fn ring_from_diameter(lower: Vec3, upper: Vec3) -> Option<(Vec3, Vec3)> {
    let span = upper - lower;
    if (span.length() - 0.07).abs() > 1e-5 {
        return None;
    }
    let normal = Vec3::X.cross(span).try_normalize()?;
    Some(((lower + upper) * 0.5 + normal * 0.0105, normal))
}

#[cfg(test)]
mod loincloth_ring_tests {
    use super::*;

    #[test]
    fn ring_diameter_preserves_radius_and_supported_endpoints() {
        let lower = Vec3::new(0.0, 0.025, 0.2);
        let direction = Vec3::new(0.0, 0.9, -0.4).normalize();
        let upper = lower + direction * 0.07;
        let (center, axis) = ring_from_diameter(lower, upper).unwrap();
        let contact_center = center - axis * 0.0105;
        assert!((contact_center.distance(lower) - 0.035).abs() < 1e-6);
        assert!((contact_center.distance(upper) - 0.035).abs() < 1e-6);
        assert!(axis.dot(direction).abs() < 1e-6);
        assert!((axis.length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn ring_diameter_rejects_unsupported_span_without_resizing() {
        assert!(ring_from_diameter(Vec3::ZERO, Vec3::Y * 0.09).is_none());
    }
}

#[cfg(test)]
mod custom_buckle_tests {
    use super::*;

    #[test]
    fn custom_buckle_respects_explicit_pouch_and_waist_support() {
        for model in [CreatureModel::Human, CreatureModel::Orc] {
            for woman in [false, true] {
                for build in [0, 2] {
                    let look = Look::with_choice(
                        model,
                        42,
                        Some(Variant {
                            woman,
                            build,
                            ..default()
                        }),
                    );
                    let body = HumanoidBody::resolve(&look);
                    let mut outfit = Outfit::native(&look);
                    outfit.details.hip = Some(HipDecoration::BucklePouch {
                        metal: Vec3::ONE,
                        pouch: Vec3::new(0.1, 0.9, 0.3),
                    });
                    outfit.pouch = None;
                    let support = torso_shape(body, Some(TorsoMaterial::Leather), false);
                    let mut bundled = Parts::default();
                    emit_belt_static(&mut bundled, outfit, body, &support);
                    assert!(!bundled.satin.is_empty(), "bundled pouch positive control");
                    assert!(!bundled.metal.is_empty());
                    for point in bundled.metal.surface_triangles().flatten() {
                        assert!(
                            support.distance(point - body.torso) > -0.001,
                            "custom buckle penetrates support: {point:?}"
                        );
                    }
                    outfit.pouch = Some(super::super::super::outfit::Pouch {
                        color: Vec3::new(0.8, 0.2, 0.7),
                    });
                    let mut explicit = Parts::default();
                    emit_belt_static(&mut explicit, outfit, body, &support);
                    assert!(
                        explicit.satin.is_empty(),
                        "independent pouch suppresses bundled duplicate"
                    );
                    bundled.metal.assert_same_recipe(&explicit.metal);
                }
            }
        }
    }
}
