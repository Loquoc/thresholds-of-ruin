use super::*;

pub(super) const RIBBON_THICKNESS: f32 = 0.007;

/// Style width and endpoint treatment, independent of the authored boundary's
/// height curve. Both ends resolve against the actual contiguous support.
#[derive(Clone, Copy)]
pub(super) struct BoundaryTrimProfile {
    pub half_width: f32,
    pub support_fraction: f32,
    pub terminal_width_fraction: f32,
    pub taper_start: f32,
}

impl BoundaryTrimProfile {
    pub fn path(
        self,
        form: &crate::sdf::Sculpt,
        height: &dyn Fn(f32) -> f32,
        front: bool,
    ) -> Vec<(Vec2, f32)> {
        let supported = |x| torso_surface_hit(form, x, height(x), front).is_some();
        if !supported(0.0) {
            return Vec::new();
        }
        let span = |side| {
            let mut inside = 0.0;
            // Stop at the first sampled gap. This 4 mm construction-time
            // scan is not a proof of continuity below its resolution.
            for step in 1..=112 {
                let mut outside = step as f32 * 0.004;
                if supported(outside * side) {
                    inside = outside;
                } else {
                    for _ in 0..10 {
                        let middle = (inside + outside) * 0.5;
                        if supported(middle * side) {
                            inside = middle;
                        } else {
                            outside = middle;
                        }
                    }
                    return Some(inside * self.support_fraction);
                }
            }
            // Beyond the bounded torso search, do not invent a silhouette.
            None
        };
        let (Some(left), Some(right)) = (span(-1.0), span(1.0)) else {
            return Vec::new();
        };
        if left.min(right) <= self.half_width {
            return Vec::new();
        }
        (-24..=24)
            .map(|i| {
                let t = i as f32 / 24.0;
                let x = t * if t < 0.0 { left } else { right };
                let taper = ease((t.abs() - self.taper_start) / (1.0 - self.taper_start));
                let width = self.half_width * (1.0 - taper * (1.0 - self.terminal_width_fraction));
                (Vec2::new(x, height(x)), width)
            })
            .collect()
    }

    pub fn fit(
        self,
        form: &crate::sdf::Sculpt,
        height: &dyn Fn(f32) -> f32,
        front: bool,
        clearance: f32,
    ) -> FittedStrap {
        FittedStrap::fit(
            form,
            &self.path(form, height, front),
            front,
            clearance,
            StrapEdgePolicy::EndUnsupportedRows,
        )
    }
}

/// Find the first actual front/back hit at a fixed lateral position. Whether
/// z=0 lies inside is not a reliable silhouette test on a curved or cut garment.
/// A miss remains explicit so a sewn edge can end at the silhouette.
pub(in crate::creatures) fn torso_surface_hit(
    form: &crate::sdf::Sculpt,
    x: f32,
    y: f32,
    front: bool,
) -> Option<Vec3> {
    torso_surface_hit_offset(form, x, y, front, 0.0)
}

/// Trace a clearance isosurface, reserving volume for attachments rather than
/// flattening every vertex against the unexpanded support.
pub(in crate::creatures) fn torso_surface_hit_offset(
    form: &crate::sdf::Sculpt,
    x: f32,
    y: f32,
    front: bool,
    clearance: f32,
) -> Option<Vec3> {
    let outward = if front { Vec3::Z } else { Vec3::NEG_Z };
    surface_ray_hit(form, Vec3::new(x, y, 0.0), outward, clearance, 0.45)
}

/// Exterior-to-interior ray against an offset support surface.
pub(in crate::creatures) fn surface_ray_hit(
    form: &crate::sdf::Sculpt,
    origin: Vec3,
    outward: Vec3,
    clearance: f32,
    reach: f32,
) -> Option<Vec3> {
    surface_field_ray_hit(&|at| form.distance(at), origin, outward, clearance, reach)
}

/// The same bounded ray query for composed support in different joint frames.
pub(super) fn surface_field_ray_hit(
    field: &dyn Fn(Vec3) -> f32,
    origin: Vec3,
    outward: Vec3,
    clearance: f32,
    reach: f32,
) -> Option<Vec3> {
    let at = |depth: f32| origin + outward * depth;
    let mut depth = reach;
    let mut previous = depth;
    for _ in 0..128 {
        let distance = field(at(depth)) - clearance;
        if distance <= 0.0 {
            let (mut inside, mut outside) = (depth, previous);
            for _ in 0..10 {
                let middle = (inside + outside) * 0.5;
                if field(at(middle)) <= clearance {
                    inside = middle;
                } else {
                    outside = middle;
                }
            }
            return Some(at((inside + outside) * 0.5));
        }
        if distance < 0.0002 {
            return Some(at(depth));
        }
        previous = depth;
        depth -= distance.clamp(0.002, 0.025);
        if depth < -0.35 {
            break;
        }
    }
    None
}

/// Point attachments may slide to a lateral edge when their authored ray misses.
/// Full-width ribbons instead retain the exact pattern plane and end as a row.
pub(super) fn torso_surface(form: &crate::sdf::Sculpt, x: f32, y: f32, front: bool) -> Vec3 {
    torso_surface_clipped_hit(form, x, y, front).unwrap_or(Vec3::new(x, y, 0.0))
}

// Retain hit provenance while applying the same lateral-clipping policy.
fn torso_surface_clipped_hit(
    form: &crate::sdf::Sculpt,
    x: f32,
    y: f32,
    front: bool,
) -> Option<Vec3> {
    let hit = |lateral| torso_surface_hit(form, lateral, y, front);
    if let Some(surface) = hit(x) {
        return Some(surface);
    }
    // All fitted profiles stay within the garment's vertical extent. Near
    // an armhole or waist edge, preserve height and find the last supported x.
    let mut surface = hit(0.0)?;
    let (mut inside, mut outside) = (0.0, x);
    for _ in 0..10 {
        let middle = (inside + outside) * 0.5;
        if let Some(at) = hit(middle) {
            inside = middle;
            surface = at;
        } else {
            outside = middle;
        }
    }
    Some(surface)
}

/// Outward sculpt normal shared by torso, shoulder and waist attachments.
pub(super) fn sculpt_surface_normal(
    form: &crate::sdf::Sculpt,
    surface: Vec3,
    outward: Vec3,
) -> Vec3 {
    let e = 0.0015;
    Vec3::new(
        form.distance(surface + Vec3::X * e) - form.distance(surface - Vec3::X * e),
        form.distance(surface + Vec3::Y * e) - form.distance(surface - Vec3::Y * e),
        form.distance(surface + Vec3::Z * e) - form.distance(surface - Vec3::Z * e),
    )
    .normalize_or(outward)
}

/// A front/back hit paired with its outward surface normal.
pub(super) fn torso_surface_frame(
    form: &crate::sdf::Sculpt,
    at: Vec2,
    front: bool,
) -> (Vec3, Vec3) {
    let surface = torso_surface(form, at.x, at.y, front);
    (
        surface,
        sculpt_surface_normal(form, surface, if front { Vec3::Z } else { Vec3::NEG_Z }),
    )
}

/// Cloth laid onto a chest or back, rather than a second closed torso that
/// intersects the first. Profile entries are height and half width.
pub(super) fn fitted_panel(
    b: &mut Builder,
    form: &crate::sdf::Sculpt,
    profile: &[(f32, f32)],
    front: bool,
    offset: f32,
    color: Vec3,
) {
    let outward = if front { Vec3::Z } else { Vec3::NEG_Z };
    let at = |y: f32, x: f32| {
        // Keep the original profile's clipped lateral footprint. Expanding
        // before clipping would grow panels around unsupported silhouettes.
        // Retain support provenance instead of tracing the same hit again.
        let Some(base) = torso_surface_clipped_hit(form, x, y, front) else {
            return Vec3::new(x, y, 0.0) + outward * offset;
        };
        torso_surface_hit_offset(form, base.x, base.y, front, offset)
            .unwrap_or(base + outward * offset)
    };
    let columns = (profile
        .iter()
        .map(|(_, width)| *width)
        .fold(0.0f32, f32::max)
        * 2.0
        / 0.012)
        .ceil()
        .max(4.0) as usize;
    // Adjacent profile windows share this exact endpoint and column grid.
    // Keep its evaluated row rather than repeat the same support queries.
    let mut previous_row: Option<Vec<Vec3>> = None;
    for pair in profile.windows(2) {
        // Long chords cut through a curved bust even when their endpoints
        // touch the surface. Keep both directions close to the torso mesh's
        // resolution, including the narrow vertical trim on fitted robes.
        let rows = ((pair[1].0 - pair[0].0).abs() / 0.012).ceil().max(1.0) as usize;
        let mut points: Vec<Vec<Vec3>> = (0..=rows)
            .map(|row| {
                if row == 0 {
                    if let Some(points) = previous_row.take() {
                        return points;
                    }
                }
                let t = row as f32 / rows as f32;
                let (height, width) = if row == rows {
                    pair[1]
                } else {
                    (
                        pair[0].0 + (pair[1].0 - pair[0].0) * t,
                        pair[0].1 + (pair[1].1 - pair[0].1) * t,
                    )
                };
                (0..=columns)
                    .map(|column| at(height, (column as f32 / columns as f32 * 2.0 - 1.0) * width))
                    .collect()
            })
            .collect();
        for row in 0..rows {
            for column in 0..columns {
                b.quad_out(
                    [
                        points[row][column],
                        points[row][column + 1],
                        points[row + 1][column + 1],
                        points[row + 1][column],
                    ],
                    Vec3::Y * (points[row][column].y + points[row + 1][column].y) * 0.5,
                    color,
                );
            }
        }
        previous_row = points.pop();
    }
}

// Waist-band landmarks in the hips' frame, shared by belt construction and
// torso accessory anchoring.
pub(in crate::creatures) const BELT_BOTTOM_Y: f32 = 0.045;
pub(in crate::creatures) const BELT_TOP_Y: f32 = 0.095;
pub(in crate::creatures) const BELT_CENTER_Y: f32 = (BELT_BOTTOM_Y + BELT_TOP_Y) * 0.5;

/// Preserve the caller's silhouette contract instead of silently changing fit.
#[derive(Clone, Copy)]
pub(super) enum StrapEdgePolicy {
    ClampToSilhouette,
    EndUnsupportedRows,
}

type StrapRow = Vec<(Vec3, Vec3)>;

/// One fitted path is authoritative for both cloth and attached hardware.
/// Separate strips preserve real gaps where full-width support is missing.
pub(super) struct FittedStrap {
    pub strips: Vec<Vec<StrapRow>>,
    outward: Vec3,
}

pub(super) struct StrapAttachment {
    pub position: Vec3,
    pub normal: Vec3,
    pub rotation: Quat,
}

impl FittedStrap {
    pub fn fit(
        form: &crate::sdf::Sculpt,
        path: &[(Vec2, f32)],
        front: bool,
        clearance: f32,
        policy: StrapEdgePolicy,
    ) -> Self {
        let mut support = form.clone();
        support.inflate(clearance);
        let outward = if front { Vec3::Z } else { Vec3::NEG_Z };
        let mut strips = Vec::new();
        let mut strip = Vec::new();
        for row in ribbon_path_rows(path) {
            let fitted: Option<StrapRow> = row
                .into_iter()
                .map(|at| {
                    let surface = match policy {
                        StrapEdgePolicy::ClampToSilhouette => {
                            torso_surface(&support, at.x, at.y, front)
                        }
                        StrapEdgePolicy::EndUnsupportedRows => {
                            torso_surface_hit(&support, at.x, at.y, front)?
                        }
                    };
                    Some((surface, sculpt_surface_normal(&support, surface, outward)))
                })
                .collect();
            if let Some(row) = fitted {
                strip.push(row);
            } else if !strip.is_empty() {
                strips.push(std::mem::take(&mut strip));
            }
        }
        if !strip.is_empty() {
            strips.push(strip);
        }
        Self { strips, outward }
    }

    /// Sample the exact outer triangles retained for emission in torso XY.
    /// Unsupported strips and points outside the authored patch remain absent.
    pub fn sample_projected(&self, at: Vec2) -> Option<Vec3> {
        if !at.is_finite() {
            return None;
        }
        for strip in &self.strips {
            for pair in strip.windows(2) {
                for column in 0..pair[0].len().min(pair[1].len()).saturating_sub(1) {
                    let q = [
                        pair[0][column].0,
                        pair[0][column + 1].0,
                        pair[1][column + 1].0,
                        pair[1][column].0,
                    ];
                    for [a, b, c] in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
                        let ab = b.truncate() - a.truncate();
                        let ac = c.truncate() - a.truncate();
                        let ap = at - a.truncate();
                        let det = ab.perp_dot(ac);
                        if det.abs() < 1e-12 {
                            continue;
                        }
                        let u = ap.perp_dot(ac) / det;
                        let v = ab.perp_dot(ap) / det;
                        if u >= -1e-6 && v >= -1e-6 && u + v <= 1.0 + 1e-6 {
                            return Some(a + ab.extend(b.z - a.z) * u + ac.extend(c.z - a.z) * v);
                        }
                    }
                }
            }
        }
        None
    }

    pub fn emit(&self, builder: &mut Builder, color: Vec3, cap_ends: bool) {
        for strip in &self.strips {
            emit_ribbon_strip(builder, strip, self.outward, color, cap_ends);
        }
    }

    /// Keep every exposed cap except the terminal end joined to another patch.
    pub fn emit_joined_terminal(&self, builder: &mut Builder, color: Vec3) {
        for (i, strip) in self.strips.iter().enumerate() {
            emit_ribbon_strip_caps(
                builder,
                strip,
                self.outward,
                color,
                true,
                i + 1 != self.strips.len(),
            );
        }
    }

    /// Hardware follows the actual sampled centreline, never an independent ray.
    /// Height must lie within a contiguous supported strip; gaps stay explicit.
    pub fn at_height(&self, height: f32) -> Option<StrapAttachment> {
        let center = |row: &StrapRow| {
            let hi = row.len() / 2;
            let lo = (row.len() - 1) / 2;
            (
                (row[lo].0 + row[hi].0) * 0.5,
                (row[lo].1 + row[hi].1).normalize_or(self.outward),
            )
        };
        if !height.is_finite() {
            return None;
        }
        for strip in &self.strips {
            for pair in strip.windows(2) {
                if pair.iter().any(Vec::is_empty) {
                    continue;
                }
                let (a, na) = center(&pair[0]);
                let (b, nb) = center(&pair[1]);
                let dy = b.y - a.y;
                if dy.abs() < 1e-7 {
                    continue;
                }
                let t = (height - a.y) / dy;
                if !(0.0..=1.0).contains(&t) {
                    continue;
                }
                let normal = na.lerp(nb, t).normalize_or(self.outward);
                let across = (b - a).cross(normal).normalize_or(Vec3::X);
                let along = normal.cross(across).normalize_or(Vec3::Y);
                return Some(StrapAttachment {
                    position: a.lerp(b, t),
                    normal,
                    rotation: Quat::from_mat3(&Mat3::from_cols(across, along, normal)),
                });
            }
        }
        None
    }
}

/// An open buckle frame and tongue in a caller-supplied attachment frame.
/// The strap remains visible through its windows; the tongue joins the rim.
pub(super) fn buckle_frame(
    b: &mut Builder,
    at: Vec3,
    rotation: Quat,
    half_size: Vec2,
    color: Vec3,
) {
    let bar = half_size.min_element() * 0.17;
    let depth = bar * 0.8;
    for side in [-1.0, 1.0] {
        b.cuboid(
            at + rotation * Vec3::new(side * (half_size.x - bar), 0.0, 0.0),
            Vec3::new(bar, half_size.y, depth),
            rotation,
            color,
        );
        b.cuboid(
            at + rotation * Vec3::new(0.0, side * (half_size.y - bar), 0.0),
            Vec3::new(half_size.x - bar * 2.0, bar, depth),
            rotation,
            color,
        );
    }
    b.cuboid(
        at + rotation * Vec3::new(0.0, half_size.y * 0.18, depth * 0.3),
        Vec3::new(bar * 0.6, half_size.y * 0.65, depth * 0.7),
        rotation,
        color,
    );
}

/// A shared accessory assembly: the strap and its hardware use the same
/// garment surface, including the fuller female chest and fitted waist.
/// A bounded layer patch in an explicit orthonormal projection frame.
/// The footprint bounds come from the resolved ribbon, not rendered triangles.
/// Bounds are conservative within one authored segment, never the whole torso.
fn ribbon_layer_patch(
    support: &crate::sdf::Sculpt,
    rows: &[StrapRow],
    along: Vec3,
    outward: Vec3,
) -> crate::sdf::Sculpt {
    use crate::sdf::{Sculpt, Shape};
    if rows.len() < 2 || rows.iter().any(|row| row.len() < 2) {
        return Sculpt::default();
    }
    let Some(outward) = outward.try_normalize() else {
        return Sculpt::default();
    };
    let Some(along) = (along - outward * along.dot(outward)).try_normalize() else {
        return Sculpt::default();
    };
    if rows.iter().flatten().any(|(p, _)| !p.is_finite()) {
        return Sculpt::default();
    }
    let across = along.cross(outward);
    let mut patch = support.clone();
    for axis in [along, across] {
        let (lo, hi) = rows
            .iter()
            .flatten()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), (p, _)| {
                (lo.min(p.dot(axis)), hi.max(p.dot(axis)))
            });
        patch.carve(Shape::Beyond(axis, hi), 0.0);
        patch.carve(Shape::Beyond(-axis, -lo), 0.0);
    }
    // Retain only this face. Cloth is emitted inward from its fitted surface;
    // this is its existing thickness, not extra clearance or global inflation.
    let inner = rows
        .iter()
        .flatten()
        .map(|(p, _)| p.dot(outward))
        .fold(f32::INFINITY, f32::min)
        - RIBBON_THICKNESS;
    patch.carve(Shape::Beyond(-outward, -inner), 0.0);
    patch
}

#[cfg(test)]
mod layer_envelope_tests {
    use super::*;

    #[test]
    fn whole_baldric_hardware_respects_only_complete_armor_coverage() {
        for woman in [false, true] {
            for reverse in [false, true] {
                let body = HumanoidBody::resolve(&Look::with_choice(
                    CreatureModel::Human,
                    2,
                    Some(Variant { woman, ..default() }),
                ));
                let garment = torso_garment(body, TorsoMaterial::Cloth, false);
                let resolved = FittedBaldric::resolve(
                    &garment,
                    body,
                    Baldric {
                        color: LEATHER,
                        reverse,
                        buckle: true,
                    },
                    0.0,
                );
                let mut base = Parts::default();
                resolved.emit(&mut base);
                let metal = base.metal.surface_triangles().count();
                let cloth = base.satin.surface_triangles().count();
                assert!(metal > 0 && cloth > 0);
                let mut armor = garment.clone();
                armor.inflate(0.10);
                let mut hidden = Parts::default();
                resolved.emit_covered(&mut hidden, Some(&armor));
                assert!(
                    hidden.metal.is_empty(),
                    "enclosing armor covers complete buckle"
                );
                assert_eq!(hidden.satin.surface_triangles().count(), cloth);
                let center = resolved.front.at_height(0.3).unwrap().position;
                armor.carve(crate::sdf::Shape::Beyond(Vec3::X, center.x), 0.0);
                let mut partial = Parts::default();
                resolved.emit_covered(&mut partial, Some(&armor));
                assert_eq!(partial.metal.surface_triangles().count(), metal);
                assert_eq!(partial.satin.surface_triangles().count(), cloth);
                let mut outside = Parts::default();
                resolved.emit_covered(&mut outside, Some(&garment));
                assert_eq!(
                    outside.metal.surface_triangles().count(),
                    metal,
                    "hardware outside armor stays visible"
                );
            }
        }
    }

    #[test]
    fn retained_baldric_support_tracks_actual_cloth_across_bodies() {
        for woman in [false, true] {
            for build in 0..3 {
                for reverse in [false, true] {
                    let look = Look::with_choice(
                        CreatureModel::Human,
                        2,
                        Some(Variant {
                            woman,
                            build,
                            ..default()
                        }),
                    );
                    let body = HumanoidBody::resolve(&look);
                    let garment = torso_garment(body, TorsoMaterial::Cloth, false);
                    let mut parts = Parts::default();
                    let resolved = FittedBaldric::resolve(
                        &garment,
                        body,
                        Baldric {
                            color: LEATHER,
                            reverse,
                            buckle: false,
                        },
                        0.0,
                    );
                    resolved.emit(&mut parts);
                    let layer = &resolved.support;
                    // Inspect the emitted over-shoulder turn, not only a front midpoint.
                    let row = &resolved.shoulder[resolved.shoulder.len() / 2];
                    let center = row[row.len() / 2].0;
                    let origin = Vec3::new(center.x, 0.0, center.z);
                    let cloth = parts
                        .satin
                        .outer_surface(origin, Vec3::Y)
                        .expect("actual shoulder cloth");
                    let top = surface_ray_hit(layer, origin, Vec3::Y, 0.0, 0.8)
                        .expect("retained shoulder support");
                    assert!(
                        top.distance(cloth) < 0.003,
                        "shoulder envelope must follow actual turn"
                    );
                    for row in &resolved.shoulder {
                        for (point, _) in row {
                            assert!(
                                layer.distance(*point).abs() < 0.002,
                                "retain every shoulder row"
                            );
                        }
                    }
                    // Both authored diagonals cross this independently known midpoint.
                    let origin = Vec3::new(0.0, 0.26, 0.0);
                    for outward in [Vec3::Z, Vec3::NEG_Z] {
                        let cloth = parts
                            .satin
                            .outer_surface(origin, outward)
                            .expect("actual cloth crosses the midpoint");
                        let retained = surface_ray_hit(layer, origin, outward, 0.0, 0.5)
                            .expect("retained layer carries the crossing");
                        assert!(
                            (retained - cloth).length() < 0.003,
                            "layer must follow cloth without bulky clearance: {woman} {build} {reverse}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn retained_patch_frame_moves_with_its_recipe_and_rejects_empty_rows() {
        use crate::sdf::{Sculpt, Shape};
        let mut form = Sculpt::default();
        form.add(Shape::Ellipsoid(Vec3::ZERO, Vec3::ONE), 0.0);
        let rows: Vec<StrapRow> = [-0.2f32, 0.2]
            .into_iter()
            .map(|z| {
                [-0.04f32, 0.04]
                    .into_iter()
                    .map(|x| {
                        let p = Vec3::new(x, (1.0 - x * x - z * z).sqrt(), z);
                        (p, p.normalize())
                    })
                    .collect()
            })
            .collect();
        let top = ribbon_layer_patch(&form, &rows, Vec3::Z, Vec3::Y);
        assert!(top.distance(Vec3::new(0.0, 0.999, 0.0)).abs() < 0.002);
        assert!(top.distance(Vec3::new(0.4, 0.9, 0.0)) > 0.3);
        let empty = ribbon_layer_patch(&form, &[], Vec3::Z, Vec3::Y);
        assert!(empty.distance(Vec3::ZERO) > 0.0);
        for bad in [Vec3::ZERO, Vec3::Y, Vec3::splat(f32::NAN)] {
            let rejected = ribbon_layer_patch(&form, &rows, bad, Vec3::Y);
            assert!(rejected.distance(Vec3::ZERO) > 0.0);
        }
    }

    #[test]
    fn retained_layer_is_local_and_carries_its_outer_surface() {
        use crate::sdf::{Sculpt, Shape};
        let mut support = Sculpt::default();
        support.add(Shape::Ellipsoid(Vec3::ZERO, Vec3::ONE), 0.0);
        let rows: Vec<StrapRow> = [-0.2f32, 0.2]
            .into_iter()
            .map(|y| {
                [-0.04f32, 0.04]
                    .into_iter()
                    .map(|x| {
                        let p = Vec3::new(x, y, (1.0 - x * x - y * y).sqrt());
                        (p, p.normalize())
                    })
                    .collect()
            })
            .collect();
        let patch = ribbon_layer_patch(&support, &rows, Vec3::Y, Vec3::Z);
        assert!(
            patch.distance(Vec3::new(0.0, 0.0, 0.999)).abs() < 0.002,
            "retained cloth must carry its resolved outer surface"
        );
        assert!(
            patch.distance(Vec3::new(0.4, 0.0, 0.9)) > 0.3,
            "unrelated torso must not be inflated"
        );
        assert!(
            patch.distance(Vec3::new(0.0, 0.0, -1.0)) > 1.0,
            "front patch must not create a back layer"
        );
    }
}

/// One anatomy-relative shoulder strap recipe. The torso owns its landmarks;
/// each garment keeps its own lateral stations, lower attachment and width.
#[derive(Clone, Copy)]
pub(super) struct ShoulderStrapProfile {
    pub chest_fraction: f32,
    pub shoulder_fraction: f32,
    pub chest_drop: f32,
    pub half_width: f32,
}

impl ShoulderStrapProfile {
    pub fn path(
        self,
        core: super::super::anatomy::TorsoCoreProfile,
        side: f32,
    ) -> [(Vec2, f32); 2] {
        let shoulder = core.shoulder_seat(side);
        [
            (
                Vec2::new(
                    side * core.chest.upper.1.x * self.chest_fraction,
                    core.upper_chest_anchor().y - self.chest_drop,
                ),
                self.half_width,
            ),
            (
                Vec2::new(shoulder.x * self.shoulder_fraction, shoulder.y - 0.01),
                self.half_width,
            ),
        ]
    }
}

/// Front, back and shoulder turn retain one fitted surface. Missing terminal
/// support cannot turn an earlier surviving row into a floating bridge.
pub(super) struct FittedShoulderStrap {
    pub front: FittedStrap,
    pub back: FittedStrap,
    pub bridge: Option<Vec<StrapRow>>,
}

impl FittedShoulderStrap {
    pub fn fit(support: &crate::sdf::Sculpt, path: &[(Vec2, f32)], clearance: f32) -> Self {
        let mut grown = support.clone();
        grown.inflate(clearance);
        let front = FittedStrap::fit(&grown, path, true, 0.0, StrapEdgePolicy::EndUnsupportedRows);
        let back = FittedStrap::fit(
            &grown,
            path,
            false,
            0.0,
            StrapEdgePolicy::EndUnsupportedRows,
        );
        let bridge = path
            .last()
            .and_then(|(top, _)| terminal_shoulder_bridge(&grown, &front, &back, top.y));
        Self {
            front,
            back,
            bridge,
        }
    }

    /// Seat sewn lower ends while preserving the shoulder turn and upper rows.
    /// The taper and its normals use one field, not independently moved vertices.
    pub fn seat_lower(
        &mut self,
        shoulder: &crate::sdf::Sculpt,
        garment: &crate::sdf::Sculpt,
        start: f32,
        span: f32,
    ) {
        let field = |p: Vec3| {
            let t = ((p.y - start) / span).clamp(0.0, 1.0);
            let weight = t * t * (3.0 - 2.0 * t);
            (garment.distance(p) - 0.010) * (1.0 - weight) + (shoulder.distance(p) - 0.014) * weight
        };
        for strap in [&mut self.front, &mut self.back] {
            let outward = strap.outward;
            for strip in &mut strap.strips {
                for row in strip {
                    if row.iter().all(|(p, _)| p.y >= start + span) {
                        continue;
                    }
                    let seated = row
                        .iter()
                        .map(|(p, old_normal)| {
                            if p.y >= start + span {
                                return Some((*p, *old_normal));
                            }
                            let p = surface_field_ray_hit(
                                &field,
                                Vec3::new(p.x, p.y, 0.0),
                                outward,
                                0.0,
                                0.45,
                            )?;
                            let e = 0.0015;
                            let normal = Vec3::new(
                                field(p + Vec3::X * e) - field(p - Vec3::X * e),
                                field(p + Vec3::Y * e) - field(p - Vec3::Y * e),
                                field(p + Vec3::Z * e) - field(p - Vec3::Z * e),
                            )
                            .normalize_or(outward);
                            Some((p, normal))
                        })
                        .collect::<Option<StrapRow>>();
                    if let Some(seated) = seated {
                        *row = seated;
                    }
                }
            }
        }
    }

    pub fn emit(&self, builder: &mut Builder, color: Vec3) {
        if self.bridge.is_some() {
            self.front.emit_joined_terminal(builder, color);
            self.back.emit_joined_terminal(builder, color);
        } else {
            self.front.emit(builder, color, true);
            self.back.emit(builder, color, true);
        }
        if let Some(rows) = &self.bridge {
            emit_ribbon_strip(builder, rows, Vec3::Y, color, false);
        }
    }
}

/// Connect only fully supported terminal rows. A missing end is not permission
/// to bridge an unrelated lower opening. Exact cloned endpoints keep seams shared.
pub(super) fn terminal_shoulder_bridge(
    support: &crate::sdf::Sculpt,
    front: &FittedStrap,
    back: &FittedStrap,
    top: f32,
) -> Option<Vec<StrapRow>> {
    let a = front.strips.last()?.last()?;
    let b = back.strips.last()?.last()?;
    if a.is_empty() || a.len() != b.len() {
        return None;
    }
    let center_y = |row: &StrapRow| (row[(row.len() - 1) / 2].0.y + row[row.len() / 2].0.y) * 0.5;
    if (center_y(a) - top).abs() > 1e-5 || (center_y(b) - top).abs() > 1e-5 {
        return None;
    }
    try_ribbon_surface_turn(support, a, b, Vec3::Y)
}

/// The resolved cloth and its analytic support travel together through the
/// composition, so torso and lower-panel roots cannot disagree about a layer.
pub(super) struct FittedBaldric {
    front: FittedStrap,
    back: FittedStrap,
    shoulder: Vec<StrapRow>,
    waist: Vec<StrapRow>,
    pub support: crate::sdf::Sculpt,
    body: HumanoidBody,
    recipe: Baldric,
}

impl FittedBaldric {
    pub fn resolve(
        garment: &crate::sdf::Sculpt,
        body: HumanoidBody,
        baldric: Baldric,
        overlay: f32,
    ) -> Self {
        let b = body.build;
        let core = super::super::anatomy::TorsoCoreProfile::biped(body);
        let side = if baldric.reverse { -1.0 } else { 1.0 };
        let path = [
            (
                Vec2::new(-side * core.shoulder.x * 0.8, core.shoulder.y - 0.01),
                0.025 * b,
            ),
            (
                Vec2::new(
                    -side * core.chest.upper.1.x * (8.0 / 9.0),
                    core.chest.upper.0.y + 0.045,
                ),
                0.025 * b,
            ),
            (Vec2::new(-side * 0.065 * b, 0.325), 0.025 * b),
            (Vec2::new(side * 0.065 * b, 0.195), 0.025 * b),
            (
                Vec2::new(
                    core.waist_anchor(side * (13.0 / 15.0)).x,
                    BELT_CENTER_Y - body.torso.y,
                ),
                0.022 * b,
            ),
        ];
        // Trace the complete support envelope at the authored x/y coordinates.
        // Large normal offsets on a tightly curved waist can cross each other
        // and fold the strip; resolving the grown surface first keeps it a sheet.
        let skin = torso_shape(body, None, true);
        let mut support = torso_layer_support(&skin, garment);
        support.inflate(BALDRIC_SURFACE_OFFSET + overlay);
        let garment = &support;
        let fitted_front = FittedStrap::fit(
            garment,
            &path,
            true,
            0.0,
            StrapEdgePolicy::ClampToSilhouette,
        );
        let fitted_back = FittedStrap::fit(
            garment,
            &path,
            false,
            0.0,
            StrapEdgePolicy::ClampToSilhouette,
        );
        let front = &fitted_front.strips[0];
        let back = &fitted_back.strips[0];
        let shoulder = ribbon_surface_turn(garment, &front[0], &back[0], Vec3::Y);
        let waist = ribbon_surface_turn(
            garment,
            front.last().unwrap(),
            back.last().unwrap(),
            Vec3::X * side,
        );
        // Keep one bounded footprint per authored path segment. The row indices
        // use the same sampling rule as ribbon_path_rows, so clamp adjustments are
        // retained without rebuilding a second approximation of the fitted path.
        let mut retained = crate::sdf::Sculpt::default();
        let mut begin = 0;
        for pair in path.windows(2) {
            let count = ((pair[1].0 - pair[0].0).length() / 0.006).ceil().max(1.0) as usize;
            let along = (pair[1].0 - pair[0].0).extend(0.0);
            for (rows, outward) in [(front, Vec3::Z), (back, Vec3::NEG_Z)] {
                retained.union(
                    ribbon_layer_patch(garment, &rows[begin..=begin + count], along, outward),
                    0.0,
                );
            }
            begin += count;
        }
        retained.union(
            ribbon_layer_patch(garment, &shoulder, Vec3::Z, Vec3::Y),
            0.0,
        );
        retained.union(
            ribbon_layer_patch(garment, &waist, Vec3::Z, Vec3::X * side),
            0.0,
        );
        Self {
            front: fitted_front,
            back: fitted_back,
            shoulder,
            waist,
            support: retained,
            body,
            recipe: baldric,
        }
    }

    /// Emit the resolved cloth and expose its retained obstacle for composition.
    /// Production apron draping does not consume it until transition fitting is
    /// visually qualified; callers may retain this layer without re-solving it.
    #[cfg(test)]
    pub fn emit(&self, p: &mut Parts) -> &crate::sdf::Sculpt {
        self.emit_covered(p, None)
    }

    /// Only an actual outer armor layer may suppress the complete hardware.
    pub fn emit_covered(
        &self,
        p: &mut Parts,
        armor: Option<&crate::sdf::Sculpt>,
    ) -> &crate::sdf::Sculpt {
        let baldric = self.recipe;
        let b = self.body.build;
        let side = if baldric.reverse { -1.0 } else { 1.0 };
        for (rows, outward) in [
            (&self.front.strips[0], Vec3::Z),
            (&self.back.strips[0], Vec3::NEG_Z),
            (&self.shoulder, Vec3::Y),
            (&self.waist, Vec3::X * side),
        ] {
            emit_ribbon_strip(&mut p.satin, rows, outward, baldric.color, false);
        }
        if baldric.buckle {
            let attachment = self
                .front
                .at_height(0.3)
                .expect("baldric crosses buckle height");
            let centre = attachment.position + attachment.normal * 0.007;
            let frame = attachment.rotation;
            let half = Vec2::new(0.034 * b, 0.027 * b);
            let depth = half.min_element() * 0.17 * 0.8;
            let points: Vec<_> = [-1.0, 1.0]
                .into_iter()
                .flat_map(|x| {
                    [-1.0, 1.0].into_iter().flat_map(move |y| {
                        [-1.0, 1.0].into_iter().map(move |z| {
                            centre + frame * Vec3::new(x * half.x, y * half.y, z * depth)
                        })
                    })
                })
                .collect();
            if !armor.is_some_and(|cover| {
                layer_coverage::WholeDetailCoverage::front().hides(cover, &points)
            }) {
                buckle_frame(
                    &mut p.metal,
                    centre,
                    frame,
                    half,
                    Vec3::new(0.74, 0.5, 0.26),
                );
            }
        }
        &self.support
    }
}

#[cfg(test)]
pub(super) fn fitted_baldric(
    p: &mut Parts,
    garment: &crate::sdf::Sculpt,
    body: HumanoidBody,
    baldric: Baldric,
    overlay: f32,
) -> crate::sdf::Sculpt {
    let resolved = FittedBaldric::resolve(garment, body, baldric, overlay);
    resolved.emit(p);
    resolved.support
}

/// Sample a strip in the garment's pattern plane before fitting its depth.
/// Keeping the complete width in each row prevents independent edge clamps
/// from pinching a binding into overlapping triangles at the body silhouette.
pub(super) fn ribbon_path_rows(path: &[(Vec2, f32)]) -> Vec<Vec<Vec2>> {
    if path.len() < 2 {
        return Vec::new();
    }
    let across = |i: usize| {
        let tangent = (path[(i + 1).min(path.len() - 1)].0 - path[i.saturating_sub(1)].0)
            .normalize_or(Vec2::Y);
        Vec2::new(tangent.y, -tangent.x)
    };
    let columns = (path.iter().map(|(_, width)| *width).fold(0.0f32, f32::max) * 2.0 / 0.006)
        .ceil()
        .max(4.0) as usize;
    let mut points = Vec::new();
    for (i, pair) in path.windows(2).enumerate() {
        let rows = ((pair[1].0 - pair[0].0).length() / 0.006).ceil().max(1.0) as usize;
        for row in 0..rows {
            let t = row as f32 / rows as f32;
            let center = pair[0].0.lerp(pair[1].0, t);
            let side = across(i).lerp(across(i + 1), t).normalize_or(Vec2::X);
            let width = pair[0].1 + (pair[1].1 - pair[0].1) * t;
            points.push(
                (0..=columns)
                    .map(|column| {
                        center + side * ((column as f32 / columns as f32 * 2.0 - 1.0) * width)
                    })
                    .collect(),
            );
        }
    }
    let (center, width) = path[path.len() - 1];
    points.push(
        (0..=columns)
            .map(|column| {
                center
                    + across(path.len() - 1)
                        * ((column as f32 / columns as f32 * 2.0 - 1.0) * width)
            })
            .collect(),
    );
    points
}

/// Join two already sampled edge rows around the top or flank of the support.
/// The cloned end rows keep adjoining patches watertight without hidden caps.
pub(super) fn ribbon_surface_turn(
    form: &crate::sdf::Sculpt,
    start: &[(Vec3, Vec3)],
    end: &[(Vec3, Vec3)],
    outward: Vec3,
) -> Vec<Vec<(Vec3, Vec3)>> {
    try_ribbon_surface_turn(form, start, end, outward)
        .expect("ribbon turn requires continuous support")
}

pub(super) fn try_ribbon_surface_turn(
    form: &crate::sdf::Sculpt,
    start: &[(Vec3, Vec3)],
    end: &[(Vec3, Vec3)],
    outward: Vec3,
) -> Option<Vec<Vec<(Vec3, Vec3)>>> {
    if start.len() != end.len() || start.is_empty() {
        return None;
    }

    let span = start
        .iter()
        .zip(end)
        .map(|(a, b)| a.0.distance(b.0))
        .fold(0.0f32, f32::max);
    let steps = (span / 0.012).ceil().max(4.0) as usize;
    let mut rows = vec![start.to_vec()];
    for row in 1..steps {
        let t = row as f32 / steps as f32;
        rows.push(
            start
                .iter()
                .zip(end)
                .map(|(a, b)| {
                    let from = a.0.lerp(b.0, t) + outward * 0.65;
                    // A ray grazing the flank takes short steps; keep
                    // marching from where one bounded march stopped.
                    let mut surface = form.onto(from, -outward);
                    for _ in 0..4 {
                        if form.distance(surface).abs() < 0.0005 {
                            break;
                        }
                        surface = form.onto(surface, -outward);
                    }
                    if !surface.is_finite() || form.distance(surface).abs() >= 0.002 {
                        return None;
                    }
                    Some((surface, sculpt_surface_normal(form, surface, outward)))
                })
                .collect::<Option<Vec<_>>>()?,
        );
    }
    rows.push(end.to_vec());
    Some(rows)
}

pub(super) fn emit_ribbon_strip(
    b: &mut Builder,
    points: &[Vec<(Vec3, Vec3)>],
    outward: Vec3,
    color: Vec3,
    cap_ends: bool,
) {
    emit_ribbon_strip_caps(b, points, outward, color, cap_ends, cap_ends);
}

fn emit_ribbon_strip_caps(
    b: &mut Builder,
    points: &[Vec<(Vec3, Vec3)>],
    outward: Vec3,
    color: Vec3,
    cap_start: bool,
    cap_end: bool,
) {
    if points.len() < 2 || points[0].len() < 2 {
        return;
    }
    let columns = points[0].len() - 1;
    let thickness = RIBBON_THICKNESS;
    let inside_between = |a: (Vec3, Vec3), z: (Vec3, Vec3)| {
        (a.0 + z.0) * 0.5 - (a.1 + z.1).normalize_or(outward) * (thickness * 0.5)
    };
    for pair in points.windows(2) {
        for column in 0..columns {
            let q = [
                pair[0][column],
                pair[0][column + 1],
                pair[1][column + 1],
                pair[1][column],
            ];
            let center = q.iter().map(|point| point.0).sum::<Vec3>() * 0.25;
            let normal = q
                .iter()
                .map(|point| point.1)
                .sum::<Vec3>()
                .normalize_or(outward);
            let top = q.map(|point| point.0);
            // Each fitted face is a graph along this patch's projection ray.
            // A mean sculpt normal can point across a sharp shell opening,
            // reversing one half of a non-planar quad even without a fold.
            for face in [[top[0], top[1], top[2]], [top[0], top[2], top[3]]] {
                let center = face.iter().sum::<Vec3>() / 3.0;
                b.tri_out(face, center - outward * 0.1, color);
            }
            b.quad_out(
                q.map(|(point, normal)| point - normal * thickness),
                center + normal * 0.1,
                color * 0.8,
            );
        }
        let inside = inside_between(pair[0][columns / 2], pair[1][columns / 2]);
        for column in [0, columns] {
            let (a, z) = (pair[0][column], pair[1][column]);
            b.quad_out(
                [a.0, z.0, z.0 - z.1 * thickness, a.0 - a.1 * thickness],
                inside,
                color * 0.8,
            );
        }
    }
    for (enabled, end, next) in [
        (cap_start, 0, 1),
        (cap_end, points.len() - 1, points.len() - 2),
    ] {
        if enabled {
            let inside = inside_between(points[end][columns / 2], points[next][columns / 2]);
            for column in 0..columns {
                let (a, z) = (points[end][column], points[end][column + 1]);
                b.quad_out(
                    [a.0, z.0, z.0 - z.1 * thickness, a.0 - a.1 * thickness],
                    inside,
                    color * 0.8,
                );
            }
        }
    }
}

pub(super) fn fitted_ribbon(
    b: &mut Builder,
    form: &crate::sdf::Sculpt,
    path: &[(Vec2, f32)],
    front: bool,
    offset: f32,
    color: Vec3,
) {
    FittedStrap::fit(
        form,
        path,
        front,
        offset,
        StrapEdgePolicy::EndUnsupportedRows,
    )
    .emit(b, color, true);
}

/// An attachment can cross an opening or the top of an outer garment.
/// Preserve the body underneath instead of tracing only that clipped layer.
pub(super) fn torso_layer_support(
    skin: &crate::sdf::Sculpt,
    garment: &crate::sdf::Sculpt,
) -> crate::sdf::Sculpt {
    let mut support = skin.clone();
    support.union(garment.clone(), 0.0);
    support
}

/// A continuous support transition beneath a strap crossing an outer-layer edge.
pub(super) fn torso_layer_transition_support(
    inner: &crate::sdf::Sculpt,
    outer: &crate::sdf::Sculpt,
) -> crate::sdf::Sculpt {
    let mut support = inner.clone();
    // The union stays outside both layers. It blends wherever their fields
    // differ by less than three ribbon thicknesses, including near a bib edge.
    // One quarter of this radius bounds field expansion, not vertex movement.
    support.union(outer.clone(), RIBBON_THICKNESS * 3.0);
    support
}

// Torso attachments reserve the same layers that the tabard renderer emits.
pub(super) const TABARD_SURFACE_OFFSET: f32 = 0.014;
pub(in crate::creatures) const BALDRIC_SURFACE_OFFSET: f32 = 0.017;
pub(super) const EMBLEM_SURFACE_OFFSET: f32 = 0.022;
pub(super) const EMBLEM_FIELD_OFFSET: f32 = 0.026;

/// Heraldry is laid onto the tabard like embroidery. Project each small
/// triangle, so a badge follows both the chest and the hollow below it.
#[allow(clippy::too_many_arguments)]
pub(super) fn fitted_emblem(
    b: &mut Builder,
    form: &crate::sdf::Sculpt,
    center: Vec2,
    size: f32,
    front: bool,
    color: Vec3,
    field: Vec3,
    kind: u8,
) {
    let outward = if front { Vec3::Z } else { Vec3::NEG_Z };
    let triangle = |b: &mut Builder, tri: [Vec2; 3], offset: f32, pigment: Vec3| {
        let steps = (tri[0]
            .distance(tri[1])
            .max(tri[1].distance(tri[2]))
            .max(tri[2].distance(tri[0]))
            * size
            / 0.012)
            .ceil()
            .max(1.0) as usize;
        let points: Vec<Vec<Vec3>> = (0..=steps)
            .map(|row| {
                (0..=steps - row)
                    .map(|column| {
                        let uv = tri[0]
                            + (tri[1] - tri[0]) * (row as f32 / steps as f32)
                            + (tri[2] - tri[0]) * (column as f32 / steps as f32);
                        let at = center + Vec2::new(uv.x * outward.z, uv.y) * size;
                        torso_surface(form, at.x, at.y, front) + outward * offset
                    })
                    .collect()
            })
            .collect();
        for row in 0..steps {
            for column in 0..steps - row {
                let a = [
                    points[row][column],
                    points[row + 1][column],
                    points[row][column + 1],
                ];
                b.tri_out(a, a[0] - outward * 0.1, pigment);
                if column + row + 1 < steps {
                    let z = [
                        points[row + 1][column],
                        points[row + 1][column + 1],
                        points[row][column + 1],
                    ];
                    b.tri_out(z, z[0] - outward * 0.1, pigment);
                }
            }
        }
    };
    let disc = |b: &mut Builder, at: Vec2, radius: f32, offset: f32, pigment: Vec3| {
        for k in 0..32 {
            let angle = k as f32 * TAU / 32.0;
            let next = (k + 1) as f32 * TAU / 32.0;
            triangle(
                b,
                [
                    at,
                    at + Vec2::new(angle.cos(), angle.sin()) * radius,
                    at + Vec2::new(next.cos(), next.sin()) * radius,
                ],
                offset,
                pigment,
            );
        }
    };
    match kind % 4 {
        0 => {
            disc(b, Vec2::ZERO, 0.38, EMBLEM_SURFACE_OFFSET, color);
            for k in 0..8 {
                let angle = k as f32 * TAU / 8.0;
                let dir = Vec2::new(angle.cos(), angle.sin());
                let side = Vec2::new(-dir.y, dir.x) * 0.1;
                let radius = if k % 2 == 0 { 0.8 } else { 0.66 };
                triangle(
                    b,
                    [dir * 0.42 + side, dir * radius, dir * 0.42 - side],
                    EMBLEM_SURFACE_OFFSET,
                    color,
                );
            }
        }
        1 => {
            for k in 0..16 {
                let angle = k as f32 * TAU / 16.0;
                let next = (k + 1) as f32 * TAU / 16.0;
                let radius = |i: usize| match i % 4 {
                    0 => 0.85,
                    2 => 0.55,
                    _ => 0.22,
                };
                triangle(
                    b,
                    [
                        Vec2::ZERO,
                        Vec2::new(angle.cos(), angle.sin()) * radius(k),
                        Vec2::new(next.cos(), next.sin()) * radius(k + 1),
                    ],
                    EMBLEM_SURFACE_OFFSET,
                    color,
                );
            }
            disc(b, Vec2::ZERO, 0.15, EMBLEM_FIELD_OFFSET, field);
        }
        2 => {
            disc(b, Vec2::ZERO, 0.62, EMBLEM_SURFACE_OFFSET, color);
            disc(b, Vec2::new(0.0, 0.26), 0.56, EMBLEM_FIELD_OFFSET, field);
        }
        _ => {
            for (x, height, width) in [(0.0, 0.85, 0.3), (-0.3, 0.45, 0.2), (0.3, 0.45, 0.2)] {
                triangle(
                    b,
                    [
                        Vec2::new(x - width, -0.6),
                        Vec2::new(x * 0.6, height),
                        Vec2::new(x + width, -0.6),
                    ],
                    EMBLEM_SURFACE_OFFSET,
                    color,
                );
            }
            disc(b, Vec2::new(0.0, -0.45), 0.32, EMBLEM_SURFACE_OFFSET, color);
        }
    }
}

/// A belt or edging seated on the whole circumference of an authored torso.
pub(super) fn fitted_band(
    b: &mut Builder,
    form: &crate::sdf::Sculpt,
    low: f32,
    high: f32,
    offset: f32,
    color: Vec3,
) {
    let at = |y: f32, angle: f32| {
        let outward = Vec3::new(angle.sin(), 0.0, angle.cos());
        form.onto(Vec3::Y * y + outward * 0.45, -outward) + outward * offset
    };
    let points: [[Vec3; 65]; 2] = [low, high]
        .map(|height| std::array::from_fn(|column| at(height, column as f32 * TAU / 64.0)));
    for column in 0..64 {
        b.quad_out(
            [
                points[0][column],
                points[0][column + 1],
                points[1][column + 1],
                points[1][column],
            ],
            Vec3::Y * (low + high) * 0.5,
            color,
        );
    }
}

#[cfg(test)]
mod strap_attachment_tests {
    use super::*;

    #[test]
    fn fitted_strap_joined_terminal_preserves_all_other_exposed_caps() {
        let row = |y| {
            vec![
                (Vec3::new(-0.02, y, 0.2), Vec3::Z),
                (Vec3::new(0.02, y, 0.2), Vec3::Z),
            ]
        };
        let strap = FittedStrap {
            strips: vec![vec![row(0.0), row(0.1)], vec![row(0.3), row(0.4), row(0.5)]],
            outward: Vec3::Z,
        };
        let mut builder = Builder::default();
        strap.emit_joined_terminal(&mut builder, Vec3::ONE);
        // Three cells:12 surface triangles +12 sidewalls +3 exposed caps*2.
        assert_eq!(builder.surface_triangles().count(), 30);
    }

    #[test]
    fn ribbon_turn_rejects_an_unsupported_interior_between_valid_ends() {
        let mut form = crate::sdf::Sculpt::default();
        for z in [-0.15, 0.15] {
            form.add(
                crate::sdf::Shape::Ellipsoid(Vec3::new(0.0, 0.5, z), Vec3::splat(0.04)),
                0.0,
            );
        }
        let row = |z| {
            [-0.01, 0.01].map(|x| {
                (
                    Vec3::new(x, 0.5 + (0.04f32.powi(2) - 0.01f32.powi(2)).sqrt(), z),
                    Vec3::Y,
                )
            })
        };
        let a = row(-0.15);
        let b = row(0.15);
        assert!(a
            .iter()
            .chain(&b)
            .all(|(at, _)| form.distance(*at).abs() < 1e-5));
        assert!(try_ribbon_surface_turn(&form, &a, &b, Vec3::Y).is_none());
    }

    #[test]
    fn fitted_strap_hardware_uses_the_same_supported_rows_across_shapes() {
        for depth in [0.13, 0.22] {
            for side in [-1.0, 1.0] {
                let mut form = crate::sdf::Sculpt::default();
                form.add(
                    crate::sdf::Shape::Ellipsoid(Vec3::Y * 0.25, Vec3::new(0.3, 0.4, depth)),
                    0.0,
                );
                let mut path = [
                    (Vec2::new(-0.08 * side, 0.1), 0.025),
                    (Vec2::new(0.08 * side, 0.4), 0.025),
                ];
                for reverse in [false, true] {
                    if reverse {
                        path.reverse();
                    }
                    let strap = FittedStrap::fit(
                        &form,
                        &path,
                        true,
                        0.01,
                        StrapEdgePolicy::EndUnsupportedRows,
                    );
                    assert_eq!(strap.strips.len(), 1);
                    let at = strap.at_height(0.25).expect("supported center");
                    assert!(at.position.is_finite() && at.normal.is_finite());
                    assert!((at.position.y - 0.25).abs() < 1e-6);
                    assert!(at.position.x.abs() < 0.001);
                    assert!(
                        (form.distance(at.position) - 0.01).abs() < 0.001,
                        "hardware center must stay on strap clearance, not float"
                    );
                    assert!((at.rotation * Vec3::Z).dot(at.normal) > 0.999);
                    for row in &strap.strips[0] {
                        for (point, _) in row {
                            assert!((form.distance(*point) - 0.01).abs() < 0.001);
                        }
                    }
                    assert!(strap.at_height(0.8).is_none());
                    assert!(strap.at_height(f32::NAN).is_none());
                }
            }
        }
    }

    #[test]
    fn fitted_strap_hardware_does_not_bridge_an_unsupported_gap() {
        let mut form = crate::sdf::Sculpt::default();
        for y in [0.1, 0.4] {
            form.add(
                crate::sdf::Shape::Ellipsoid(Vec3::Y * y, Vec3::new(0.15, 0.065, 0.12)),
                0.0,
            );
        }
        let path = [(Vec2::new(0.0, 0.02), 0.025), (Vec2::new(0.0, 0.48), 0.025)];
        let strap = FittedStrap::fit(
            &form,
            &path,
            true,
            0.01,
            StrapEdgePolicy::EndUnsupportedRows,
        );
        assert_eq!(strap.strips.len(), 2);
        assert!(strap.at_height(0.1).is_some());
        assert!(strap.at_height(0.4).is_some());
        assert!(
            strap.at_height(0.25).is_none(),
            "hardware must not bridge a real clothing opening"
        );
    }

    #[test]
    fn fitted_strap_keeps_missing_support_explicit_and_preserves_edge_policy() {
        let mut form = crate::sdf::Sculpt::default();
        form.add(
            crate::sdf::Shape::Ellipsoid(Vec3::Y * 0.25, Vec3::new(0.10, 0.3, 0.10)),
            0.0,
        );
        let path = [(Vec2::new(0.4, 0.1), 0.025), (Vec2::new(0.4, 0.4), 0.025)];
        let end = FittedStrap::fit(
            &form,
            &path,
            true,
            0.01,
            StrapEdgePolicy::EndUnsupportedRows,
        );
        assert!(end.strips.is_empty());
        assert!(end.at_height(0.25).is_none());
        let clamp = FittedStrap::fit(&form, &path, true, 0.0, StrapEdgePolicy::ClampToSilhouette);
        assert_eq!(clamp.strips.len(), 1);
        assert!(clamp.strips[0].iter().flatten().all(|(p, _)| p.x < 0.11));
        let empty = FittedStrap::fit(&form, &[], true, 0.0, StrapEdgePolicy::EndUnsupportedRows);
        assert!(empty.at_height(0.25).is_none());
    }
}

#[cfg(test)]
mod projected_strap_tests {
    use super::*;

    #[test]
    fn projected_sampler_follows_emitted_diagonal_and_preserves_gaps() {
        let row = |y: f32, z: f32| {
            vec![
                (Vec3::new(0.0, y, 0.0), Vec3::Z),
                (Vec3::new(1.0, y, z), Vec3::Z),
            ]
        };
        let patch = FittedStrap {
            strips: vec![
                vec![row(0.0, 0.0), row(1.0, 1.0)],
                vec![row(3.0, 0.0), row(4.0, 1.0)],
            ],
            outward: Vec3::Z,
        };
        // Non-planar quad: fixed diagonal yields min(x,y), not bilinear x*y.
        for (at, z) in [
            (Vec2::new(0.75, 0.25), 0.25),
            (Vec2::new(0.25, 0.75), 0.25),
            (Vec2::splat(0.5), 0.5),
        ] {
            let hit = patch.sample_projected(at).unwrap();
            assert!((hit.z - z).abs() < 1e-6);
            assert!((hit.truncate() - at).length() < 1e-6);
        }
        for at in [
            Vec2::new(0.5, 2.0),
            Vec2::new(-0.1, 0.5),
            Vec2::splat(f32::NAN),
        ] {
            assert!(patch.sample_projected(at).is_none());
        }
        let collapsed = FittedStrap {
            strips: vec![vec![row(0.0, 0.0), row(0.0, 1.0)]],
            outward: Vec3::Z,
        };
        assert!(collapsed.sample_projected(Vec2::ZERO).is_none());
    }
}
