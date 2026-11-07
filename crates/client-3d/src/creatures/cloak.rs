//! Connected, lined shoulder cloth. Recipes share construction and materials;
//! each wearer owns the vertices that follow its motion.
use super::outfit::outerwear::{DrapeCut, DrapeHem};
use super::{anatomy::HumanoidBody, deformation::SurfaceBinding, *};
use bevy::asset::RenderAssetUsages;
use bevy::render::{
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    primitives::Aabb,
};
use bevy::tasks::ParallelSliceMut;
use std::sync::{Arc, OnceLock};

mod attachment;

const ROWS: usize = 20;
// Refine only the shoulder within the existing grid: five roof bands, one
// descending turnover band and fourteen hanging bands; no mesh growth.
const YOKE_ROWS: usize = 6;
// The last yoke band is the descending side of the cap, not its roof.
const ROOF_ROWS: usize = YOKE_ROWS - 1;
const ROOF_VERTICES: usize = ROOF_ROWS * (COLUMNS + 1);
const YOKE_FRACTION: f32 = 0.15;

fn study_row_fraction(row: usize) -> f32 {
    if row <= YOKE_ROWS {
        YOKE_FRACTION * row as f32 / YOKE_ROWS as f32
    } else {
        YOKE_FRACTION + (1.0 - YOKE_FRACTION) * (row - YOKE_ROWS) as f32 / (ROWS - YOKE_ROWS) as f32
    }
}
const COLUMNS: usize = 48;
const GRID: usize = (ROWS + 1) * (COLUMNS + 1);
const THICKNESS: f32 = 0.012;
// Reserve lining thickness and interpolation across the curved plate shoulder.
const LAYER_GAP: f32 = 0.020;
// A sloped yoke may rise only this much above its exact horizontal fallback.
// This selects a separating plane; it never clamps a collision correction.
const ROOF_SLOPE_SLACK: f32 = 0.010;

/// One cheap pose snapshot shared by the main cape and mantle. Geometry and
/// spatial indices are materialized once, only when a joint pose changes.
pub(super) struct ContactSurface {
    parts: Vec<super::visual_contact::PosedTriangles>,
    cached: OnceLock<Arc<LayerSurface>>,
}
impl ContactSurface {
    pub fn new(parts: Vec<super::visual_contact::PosedTriangles>) -> Self {
        Self {
            parts,
            cached: OnceLock::new(),
        }
    }
    fn surface(&self) -> Arc<LayerSurface> {
        self.cached
            .get_or_init(|| {
                let mut triangles = Vec::new();
                let mut groups = Vec::new();
                let mut offset = 0;
                for part in &self.parts {
                    triangles.extend(
                        part.triangles
                            .iter()
                            .map(|tri| tri.map(|p| part.transform.transform_point3(p))),
                    );
                    groups.extend(
                        part.closed_components()
                            .iter()
                            .map(|group| group.map(|id| id + offset)),
                    );
                    offset += part.closed_component_count();
                }
                let surface = LayerSurface::from_triangles(triangles);
                let _ = surface.obstacle_components.set(groups);
                Arc::new(surface)
            })
            .clone()
    }
}

pub(super) struct Recipe {
    shoulders: f32,
    yoke: Vec2,
    fitted_turnover: OnceLock<[Vec3; COLUMNS + 1]>,
    fitted_outer_collar: OnceLock<Option<[Vec3; COLUMNS + 1]>>,
    hem: f32,
    top: [Vec3; COLUMNS + 1],
    support: crate::sdf::Sculpt,
    contact: bool,
    pub length: f32,
    exterior: Vec3,
    lining: Vec3,
    ragged: bool,
    cut: Option<DrapeCut>,
    pub satin: bool,
    pub embroidery: Option<Vec3>,
    inner_layer: Option<Entity>,
    scene: Option<super::visual_contact::Scene>,
    contact_surface: Option<Arc<ContactSurface>>,
    head_contact_surface: Option<Arc<ContactSurface>>,
}

impl Recipe {
    pub fn new(body: HumanoidBody, exterior: Vec3, lining: Vec3, ragged: bool) -> Self {
        let shoulders = 0.205 * body.shoulders * body.build * anatomy::HIP_SCALE.x;
        // The upper back has the same supports in the existing fitted torso
        // layers. Leather's ease keeps this yoke just above the cloth beneath.
        let support = geometry::torso_shape(body, Some(geometry::TorsoMaterial::Leather), false);
        let mut recipe = Self {
            shoulders,
            yoke: Vec2::new(
                0.25 * body.shoulders * body.build + 0.115 * body.build,
                0.17 * body.build,
            ),
            fitted_turnover: OnceLock::new(),
            fitted_outer_collar: OnceLock::new(),
            hem: 0.36 * body.hips_width * body.build * anatomy::HIP_SCALE.x,
            top: [Vec3::ZERO; COLUMNS + 1],
            support,
            contact: false,
            length: 1.08,
            exterior,
            lining,
            ragged,
            cut: None,
            satin: false,
            embroidery: None,
            inner_layer: None,
            scene: None,
            contact_surface: None,
            head_contact_surface: None,
        };
        recipe.reserve_back_layer(0.0);
        recipe
    }

    /// Actual rendered arm, sleeve and pauldron profiles, in torso query space.
    /// Call after reserving every torso layer so the scene owns the final union.
    pub fn set_contacts(&mut self, contacts: Vec<super::visual_contact::Cylinder>) {
        self.fitted_turnover.take();
        self.fitted_outer_collar.take();
        if self.cut.is_some() {
            self.scene = Some(super::visual_contact::Scene::new(
                self.support.clone(),
                contacts,
            ));
        }
    }

    pub fn set_contact_surface(&mut self, surface: Arc<ContactSurface>) {
        self.fitted_turnover.take();
        self.fitted_outer_collar.take();
        if self.cut.is_some() {
            self.contact_surface = Some(surface);
        }
    }

    pub fn set_head_surface(&mut self, surface: Arc<ContactSurface>) {
        self.head_contact_surface = Some(surface);
        self.fitted_outer_collar.take();
    }

    /// Fit this shoulder layer to an earlier cloak on the same torso frame.
    pub fn layer_over(&mut self, inner: Entity) {
        self.inner_layer = Some(inner);
        self.fitted_outer_collar.take();
    }

    /// Study garments use an open angular shoulder wrap. Legacy recipes keep
    /// their original back-sheet coordinates and animation unchanged.
    pub fn set_cut(&mut self, cut: DrapeCut) {
        self.fitted_turnover.take();
        self.fitted_outer_collar.take();
        self.length = cut.length;
        self.cut = Some(cut);
        self.contact = true;
    }

    /// Fit over an actual equipped torso shell and retain that support for
    /// pose-aware upper-back clearance during the cloak's eased swing.
    pub fn reserve_body_layer(&mut self, layer: &crate::sdf::Sculpt) {
        self.support.union(layer.clone(), 0.0);
        self.contact = true;
        self.reserve_back_layer(0.0);
    }

    /// Reserve the selected rear accessory layer before fitting the yoke and
    /// upper drape. A strap belongs underneath the cloak, not half through it.
    pub fn reserve_back_layer(&mut self, depth: f32) {
        self.fitted_turnover.take();
        self.fitted_outer_collar.take();
        // Reserve the lining too: fitting only the middle sheet to an
        // accessory envelope can leave its inner face cutting the strap.
        self.support
            .inflate(depth + if depth > 0.0 { THICKNESS * 0.5 } else { 0.0 });
        self.top = std::array::from_fn(|column| {
            let u = column as f32 / COLUMNS as f32 * 2.0 - 1.0;
            let x = u * self.shoulders / anatomy::HIP_SCALE.x;
            let y = 0.505 - 0.035 * u * u;
            let at = geometry::torso_surface_hit(&self.support, x, y, false)
                .expect("resolved shoulder yoke must have upper-back support");
            at * anatomy::HIP_SCALE + Vec3::NEG_Z * if self.contact { LAYER_GAP } else { 0.012 }
                - Vec3::new(0.0, 0.53, -0.15)
        });
    }

    fn point(&self, row: usize, column: usize) -> Vec3 {
        if let Some(cut) = self.cut {
            return self.wrap_point(cut, row, column);
        }
        let t = row as f32 / ROWS as f32;
        let u = column as f32 / COLUMNS as f32 * 2.0 - 1.0;
        let spread = t * t * (3.0 - 2.0 * t);
        let width = self.shoulders + (self.hem - self.shoulders) * spread;
        // A gathered shoulder edge, broad hanging folds and a gently returned
        // side edge: the lining occupies the whole inside, not colored rods.
        let fold = (u * PI * 6.0).cos() * (0.006 + 0.027 * t);
        let returned_edge = u.abs().powi(4) * (0.09 - 0.035 * t);
        let hem = if self.ragged {
            // Connected notches of unequal depth, with a substantial web of
            // cloth above them. Never separate strips hanging in empty space.
            let teeth = (u * PI * 7.0 + 0.4).cos().abs();
            0.12 * teeth + 0.035 * (u * PI * 3.0).sin()
        } else {
            0.018 * (u * PI).cos()
        };
        let top = self.top[column];
        let mut point = Vec3::new(
            u * width,
            top.y * (1.0 - t) - self.length * t - hem * t.powi(8),
            top.z * (1.0 - t) + (-0.17 - 0.035 * (t * PI).sin() + returned_edge + fold) * t,
        );
        let at = (point + Vec3::new(0.0, 0.53, -0.15)) / anatomy::HIP_SCALE;
        if at.y >= 0.075 {
            if let Some(back) = geometry::torso_surface_hit(&self.support, at.x, at.y, false) {
                point.z = point.z.min(
                    back.z * anatomy::HIP_SCALE.z - if self.contact { LAYER_GAP } else { 0.012 }
                        + 0.15,
                );
            }
        }
        point
    }
    fn wrap_point(&self, cut: DrapeCut, row: usize, column: usize) -> Vec3 {
        self.wrap_geometry(cut, row, column, true)
    }

    fn shoulder_profile(&self) -> Vec<Vec2> {
        let Some(cut) = self.cut else {
            return Vec::new();
        };
        (0..GRID)
            .map(|i| self.shoulder_coefficient(cut, i / (COLUMNS + 1), i % (COLUMNS + 1)))
            .collect()
    }

    fn shoulder_coefficient(&self, cut: DrapeCut, row: usize, column: usize) -> Vec2 {
        if row == 0 {
            return Vec2::Y;
        }
        if row >= YOKE_ROWS {
            return Vec2::X;
        }
        shoulder_coefficient(
            row,
            self.wrap_geometry(cut, row - 1, column, false),
            self.wrap_geometry(cut, row + 1, column, false),
        )
    }

    fn wrap_geometry(&self, cut: DrapeCut, row: usize, column: usize, fit: bool) -> Vec3 {
        let t = study_row_fraction(row);
        let u = column as f32 / COLUMNS as f32;
        // Left front edge, around the back, to the right front edge. The
        // unsampled angular gap is a real opening, never a painted seam.
        let angle = TAU - cut.opening * 0.5 - (TAU - cut.opening) * u;
        let direction = Vec3::new(angle.sin(), 0.0, angle.cos());
        // A cape begins at the neck, travels OVER the deltoid, then hangs.
        // The old first row was below the shoulder and produced open teal
        // shoulder cups and rigid-looking boards down either side.
        let shoulder = self.fitted_turnover.get_or_init(|| {
            // Resolve only 49 support rays per recipe, not once per grid row.
            // Both the yoke and the hanging section share this exact ring.
            let arms = self.contact_surface.as_ref().map(|s| s.surface());
            std::array::from_fn(|col| {
                let angle =
                    TAU - cut.opening * 0.5 - (TAU - cut.opening) * col as f32 / COLUMNS as f32;
                let authored =
                    Vec3::new(angle.sin() * self.yoke.x, 0.47, angle.cos() * self.yoke.y);
                supported_turnover(authored, &self.support, arms.as_deref())
            })
        })[column];
        let neck = Vec3::new(direction.x * 0.075, 0.565, direction.z * 0.072);
        // The opening surrounds the neck; it does not sit on the rounded
        // cap of the torso's neck proxy. Derive the connecting shoulder rows
        // from this fitted ring so an enlarged collar cannot fold underneath
        // the next radial row.
        let neck = if row <= YOKE_ROWS {
            if self.inner_layer.is_some() {
                self.fitted_outer_collar
                    .get_or_init(|| {
                        let mut ring = [Vec3::ZERO; COLUMNS + 1];
                        for (col, point) in ring.iter_mut().enumerate() {
                            let angle = TAU
                                - cut.opening * 0.5
                                - (TAU - cut.opening) * col as f32 / COLUMNS as f32;
                            let direction =
                                Vec3::new(angle.sin() * 0.075, 0.0, angle.cos() * 0.072)
                                    .normalize();
                            *point = supported_outer_collar(direction, &self.support)?;
                            if let Some(head) = &self.head_contact_surface {
                                *point = head.surface().clearance_point(
                                    *point,
                                    direction,
                                    LAYER_GAP + THICKNESS,
                                )?;
                                if self.support.distance(*point / anatomy::HIP_SCALE)
                                    < LAYER_GAP + 0.0097
                                {
                                    return None;
                                }
                            }
                        }
                        Some(ring)
                    })
                    .as_ref()
                    .map_or_else(
                        || fit_neck_opening(neck, &self.support),
                        |ring| ring[column],
                    )
            } else {
                fit_neck_opening(neck, &self.support)
            }
        } else {
            neck
        };
        let fold = (angle * cut.folds as f32).cos() * (0.004 + 0.015 * t);
        let radius = self.shoulders * cut.fullness + fold;
        let hem_drop = match cut.hem {
            DrapeHem::Rounded => 0.018 * (angle * 2.0).cos(),
            DrapeHem::Frayed => 0.035 * (angle * 19.0).sin().abs(),
            DrapeHem::Torn => {
                let notches = (angle * 7.0 + 0.8).cos().max(0.0).powi(3);
                0.15 * notches + 0.045 * (angle * 3.0 + 0.6).sin() + 0.025 * (angle * 17.0).cos()
            }
            DrapeHem::Rectangular => 0.13 * angle.sin().abs(),
        };
        // A rectangular blanket retains its four lower corners. Round capes
        // remain elliptic. A broad cope spreads its semicircular skirt.
        let square = if cut.hem == DrapeHem::Rectangular {
            1.0 / angle.sin().abs().max(angle.cos().abs()).max(0.1)
        } else {
            1.0
        };
        let hem = Vec3::new(
            direction.x * radius * square,
            0.53 - self.length - hem_drop,
            direction.z * radius * 0.78 * square,
        );
        let mut at = if row <= YOKE_ROWS {
            let across = row as f32 / YOKE_ROWS as f32;
            neck.lerp(shoulder, across) + Vec3::Y * (0.045 * (across * PI).sin())
        } else {
            shoulder.lerp(hem, (row - YOKE_ROWS) as f32 / (ROWS - YOKE_ROWS) as f32)
        };
        if fit && row > 0 {
            let axis = profile_direction(at, self.shoulder_coefficient(cut, row, column));
            if let Some(surface) = &self.contact_surface {
                at = fit_profile_body(at, axis, &|p| self.support.distance(p));
                at = surface.surface().outside_point(at, axis).unwrap_or(at);
            } else if let Some(scene) = &self.scene {
                at = fit_profile_body(at, axis, &|p| scene.distance(p));
            } else {
                at = fit_profile_body(at, axis, &|p| self.support.distance(p));
            }
        }
        at - Vec3::new(0.0, 0.53, -0.15)
    }
}

/// Fit a study turnover around its actual neutral supports at the authored
/// height. The old ellipse is a no-support fallback, not an immutable width
/// minimum. Resolve torso and arm hits independently: a missing arm hit must
/// neither erase the torso nor force the broad authored shoulder back in.
fn supported_turnover(
    authored: Vec3,
    body: &crate::sdf::Sculpt,
    arms: Option<&LayerSurface>,
) -> Vec3 {
    let direction = authored.with_y(0.0).normalize_or(Vec3::NEG_Z);
    let local = authored / anatomy::HIP_SCALE;
    let local_direction = local.with_y(0.0).normalize_or(Vec3::NEG_Z);
    // Same SDF clearance and coordinate convention as fit_wrapped_contact,
    // but search from outside through the center instead of stopping at the
    // oversized authored radius. Reject opposite-half-ray hits explicitly.
    let torso = geometry::surface_ray_hit(
        body,
        Vec3::Y * local.y,
        local_direction,
        LAYER_GAP + 0.010,
        1.10,
    )
    .filter(|hit| hit.dot(local_direction) >= 0.0)
    .map(|hit| (hit * anatomy::HIP_SCALE).with_y(0.0).length());
    let arm = arms
        .and_then(|surface| surface.ray(Vec3::Y * authored.y, direction))
        .map(|distance| distance + LAYER_GAP + THICKNESS);
    let radius = match (torso, arm) {
        (Some(torso), Some(arm)) => torso.max(arm),
        (Some(radius), None) | (None, Some(radius)) => radius,
        (None, None) => return authored,
    };
    Vec3::Y * authored.y + direction * radius
}

// Freeze material-profile coefficients BEFORE contact fitting. A monotone
// radial/+Y direction preserves the azimuth while following the shoulder's
// outward normal instead of abruptly switching coordinates at its last band.
fn shoulder_coefficient(row: usize, before: Vec3, after: Vec3) -> Vec2 {
    if row == 0 {
        return Vec2::Y;
    }
    if row >= YOKE_ROWS {
        return Vec2::X;
    }
    let offset = Vec3::new(0.0, 0.53, -0.15);
    let dr = (after + offset).with_y(0.0).length() - (before + offset).with_y(0.0).length();
    Vec2::new((before.y - after.y).max(0.0), dr.max(0.0)).normalize_or(Vec2::Y)
}

fn profile_direction(at: Vec3, coefficient: Vec2) -> Vec3 {
    at.with_y(0.0).normalize_or(Vec3::NEG_Z) * coefficient.x + Vec3::Y * coefficient.y
}

fn fit_profile_body(at: Vec3, axis: Vec3, distance: &impl Fn(Vec3) -> f32) -> Vec3 {
    // Trace inward from a known remote point along the SAME fixed axis used
    // by layer seating and certification. Keep the previous body's gap.
    let reserve = if axis.y == 0.0 {
        LAYER_GAP + 0.010
    } else {
        0.025
    };
    let mut travel = 1.5;
    while travel > 0.0 {
        let probe = at + axis * travel;
        let gap = distance(probe / anatomy::HIP_SCALE) - reserve;
        if gap <= 0.0002 {
            return probe;
        }
        travel -= gap.clamp(0.002, 0.02);
    }
    at
}

fn seat_profile_surface(at: Vec3, axis: Vec3, surface: &LayerSurface) -> Vec3 {
    // The complete line through the authored point retains outermost layer
    // semantics. Only positive travel is applied; a ray hit is initialization,
    // never a replacement for the full-face certificate.
    let Some(bounds) = surface.tree.first() else {
        return at;
    };
    let minimum = (bounds.low * axis).min(bounds.high * axis).element_sum();
    let extent = (at.dot(axis) - minimum).max(0.0) + THICKNESS;
    let origin = at - axis * extent;
    surface.ray(origin, axis).map_or(at, |hit| {
        at + axis * (hit - extent + LAYER_GAP + THICKNESS).max(0.0)
    })
}

/// Inner cloth uses intentional outermost layer seating. Arm obstacles use
/// membership-gated initialization instead, followed by exact face clearance.
fn seat_outermost_surface(mut at: Vec3, roof: bool, surface: &LayerSurface) -> Vec3 {
    if surface.triangles.is_empty() {
        return at;
    }
    if roof {
        let origin = Vec3::new(at.x, surface.bottom - THICKNESS, at.z);
        if let Some(distance) = surface.ray(origin, Vec3::Y) {
            at.y = at.y.max(origin.y + distance + LAYER_GAP + THICKNESS);
        }
    } else {
        let origin = Vec3::Y * at.y;
        let direction = at.with_y(0.0).normalize_or(Vec3::NEG_Z);
        if let Some(distance) = surface.ray(origin, direction) {
            let radius = at
                .with_y(0.0)
                .length()
                .max(distance + LAYER_GAP + THICKNESS);
            at = origin + direction * radius;
        }
    }
    at
}

#[cfg(test)]
fn fit_mesh_contact(at: Vec3, roof: bool, surface: &LayerSurface) -> Vec3 {
    let axis = if roof {
        Vec3::Y
    } else {
        at.with_y(0.0).normalize_or(Vec3::NEG_Z)
    };
    // The face solver validates initialization and reports an explicit
    // failure if bounded component exits cannot establish outside membership.
    surface.outside_point(at, axis).unwrap_or(at)
}

#[cfg(test)]
fn seat_arm_obstacles(points: &mut [Vec3], rotation: Quat, arms: &LayerSurface) {
    seat_arm_profile(points, rotation, arms, None);
}
#[cfg(test)]
fn seat_arm_profile(
    points: &mut [Vec3],
    rotation: Quat,
    arms: &LayerSurface,
    profile: Option<&[Vec2]>,
) {
    let offset = Vec3::new(0.0, 0.53, -0.15);
    let inverse = rotation.inverse();
    for (i, p) in points.iter_mut().enumerate().skip(COLUMNS + 1) {
        let at = rotation * *p + offset;
        let axis = profile.map_or_else(
            || contact_axis(i, at.with_y(0.0).normalize_or(Vec3::NEG_Z)),
            |profile| profile_direction(at, profile[i]),
        );
        *p = inverse * (arms.outside_point(at, axis).unwrap_or(at) - offset);
    }
}

/// A neck opening is an annular attachment at fixed anatomical height.
/// Only retained torso/neck support participates here; posed hands are handled
/// by the complete cell solver, not by wrapping the collar around their reach.
// A separate outer layer attaches one existing layer reserve above the main
// station, on the actual neck taper. This is authored initialization, NOT a
// normal-distance certificate: the full inner/arm solve remains authoritative.
const OUTER_COLLAR_STATION: f32 = 0.565 + LAYER_GAP + THICKNESS;

fn supported_outer_collar(direction: Vec3, support: &crate::sdf::Sculpt) -> Option<Vec3> {
    supported_collar_station(direction, support, OUTER_COLLAR_STATION)
}

fn supported_collar_station(
    direction: Vec3,
    support: &crate::sdf::Sculpt,
    station: f32,
) -> Option<Vec3> {
    let origin = Vec3::Y * station;
    let reserve = LAYER_GAP + 0.010;
    // A missing expanded neck slice must not be mistaken for the tiny seed.
    if support.distance(origin / anatomy::HIP_SCALE) >= reserve
        || support.distance((origin + direction * 1.10) / anatomy::HIP_SCALE) <= reserve
    {
        return None;
    }
    let field = |radius: f32| {
        support.distance((origin + direction * radius) / anatomy::HIP_SCALE) - reserve
    };
    let mut outside = 1.10;
    // Retain outside-in root selection, then refine the actual sign bracket.
    // The old fit marcher's 2mm minimum step is not a root certificate.
    for _ in 0..=550 {
        let inside = (outside - field(outside).clamp(0.002, 0.02)).max(0.0);
        if field(inside) <= 0.0 {
            let mut low = inside;
            let mut high = outside;
            for _ in 0..24 {
                let middle = (low + high) * 0.5;
                if field(middle) <= 0.0 {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            let fitted = origin + direction * high;
            return ((support.distance(fitted / anatomy::HIP_SCALE) - reserve).abs() <= 0.0003)
                .then_some(fitted);
        }
        outside = inside;
    }
    None
}

fn fit_neck_opening(at: Vec3, support: &crate::sdf::Sculpt) -> Vec3 {
    fit_wrapped_contact(at, false, &|p| support.distance(p))
}

/// Fit in scaled torso coordinates. Shoulder rows travel above the arm cap;
/// hanging rows approach the complete posed scene from outside radially.
fn fit_wrapped_contact(at: Vec3, shoulder: bool, distance: &impl Fn(Vec3) -> f32) -> Vec3 {
    let mut query = at / anatomy::HIP_SCALE;
    if shoulder {
        let mut y = 0.85;
        while y > query.y {
            let probe = Vec3::new(query.x, y, query.z);
            let gap = distance(probe) - 0.025;
            if gap <= 0.0002 {
                query.y = y;
                break;
            }
            y -= gap.clamp(0.002, 0.02);
        }
    } else if query.y >= -0.12 {
        let out = query.with_y(0.0).normalize_or(Vec3::NEG_Z);
        let radius = query.with_y(0.0).length();
        let mut reach = 1.10;
        while reach > radius {
            let probe = Vec3::Y * query.y + out * reach;
            let gap = distance(probe) - (LAYER_GAP + 0.010);
            if gap <= 0.0002 {
                query = probe;
                break;
            }
            reach -= gap.clamp(0.002, 0.025);
        }
    }
    query * anatomy::HIP_SCALE
}

fn study_pigment(recipe: &Recipe, row: usize, col: usize) -> Vec3 {
    let Some(gold) = recipe.embroidery else {
        return recipe.exterior;
    };
    let edge = row == ROWS
        || row == ROWS - 3
        || col == 0
        || col == COLUMNS
        || col == 2
        || col == COLUMNS - 2;
    let band = row >= ROWS - 3 || col <= 2 || col >= COLUMNS - 2;
    // A readable blue orphrey bounded in gold, rather than orange dashes on
    // plain cream. Small paired leaves stay on the interior field.
    let leaf = row > 4
        && row < ROWS - 4
        && ((row.is_multiple_of(6) && col % 12 == 6)
            || (row % 6 == 1 && matches!(col % 12, 5 | 7)));
    if edge || leaf {
        gold
    } else if band {
        recipe.lining
    } else {
        recipe.exterior
    }
}

#[cfg(test)]
thread_local! {
    static AUDIT_CONTEXT: std::cell::RefCell<serde_json::Value> = const { std::cell::RefCell::new(serde_json::Value::Null) };
}

#[cfg(test)]
struct AuditContextGuard(serde_json::Value);
#[cfg(test)]
impl AuditContextGuard {
    fn enter(context: serde_json::Value) -> Self {
        Self(AUDIT_CONTEXT.with(|slot| slot.replace(context)))
    }
}
#[cfg(test)]
impl Drop for AuditContextGuard {
    fn drop(&mut self) {
        AUDIT_CONTEXT.with(|slot| {
            slot.replace(self.0.take());
        });
    }
}
#[cfg(test)]
fn audit_context() -> serde_json::Value {
    AUDIT_CONTEXT.with(|slot| slot.borrow().clone())
}

pub(super) struct CloakRig {
    #[cfg(test)]
    audit_context: serde_json::Value,
    pub entity: Entity,
    mesh: Handle<Mesh>,
    rest: Vec<Vec3>,
    shoulder_profile: Vec<Vec2>,
    attachment_reference: Vec<Vec3>,
    attachment_limits: Option<[f32; 2 * (COLUMNS + 1)]>,
    bindings: Vec<SurfaceBinding<f32>>,
    points: Vec<Vec3>,
    normals: Vec<Vec3>,
    last_rotation: Option<Quat>,
    support: Option<crate::sdf::Sculpt>,
    wrapped: bool,
    inner_layer: Option<Entity>,
    last_inner_rotation: Option<Quat>,
    scene: Option<super::visual_contact::Scene>,
    arm_surface: Option<Arc<LayerSurface>>,
    contact_surface: Option<Arc<ContactSurface>>,
    head_contact_surface: Option<Arc<ContactSurface>>,
    head_surface: Option<Arc<LayerSurface>>,
    opening: Option<f32>,
    neutral_reach: [f32; 2],
    contact_fallbacks: u64,
    contact_reference: Vec<Vec3>,
    contact_seated: Vec<Vec3>,
    contact_seating_valid: bool,
    contact_radial_trace: Option<serde_json::Value>,
    contact_cpu_ms: [f64; 4],
    contact_solves: u64,
}

impl CloakRig {
    pub(super) fn contact_diagnostics(&self) -> Option<serde_json::Value> {
        if !self.wrapped {
            return None;
        }
        let rotation = self.last_rotation.unwrap_or(Quat::IDENTITY);
        let offset = Vec3::new(0.0, 0.53, -0.15);
        let reference = &self.contact_reference;
        let point = |i: usize| rotation * self.points[i] + offset;
        let before = |i: usize| rotation * reference[i] + offset;
        let radius = |p: Vec3| p.with_y(0.0).length();
        let seated = |i: usize| {
            rotation * self.contact_seated.get(i).copied().unwrap_or(reference[i]) + offset
        };
        let rest = |i: usize| self.rest[i] + offset;
        let range = |row: usize, kind: u8| {
            (0..=COLUMNS).fold(
                [
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                ],
                |mut r, col| {
                    let i = index(row, col);
                    let p = match kind {
                        0 => rest(i),
                        1 => before(i),
                        2 => seated(i),
                        _ => point(i),
                    };
                    let d = radius(p);
                    r[0] = r[0].min(d);
                    r[1] = r[1].max(d);
                    r[2] = r[2].min(p.y);
                    r[3] = r[3].max(p.y);
                    r
                },
            )
        };
        let rows:Vec<_>=(0..=YOKE_ROWS).map(|row| {
            let tip=(0..=COLUMNS).map(|col|index(row,col))
                .max_by(|&a,&b|radius(point(a)).total_cmp(&radius(point(b)))).unwrap();
            serde_json::json!({"row":row,"rest":range(row,0),"reference":range(row,1),"seated":range(row,2),"final":range(row,3),
                "radius_max_vertex":tip,"tip_rest":rest(tip).to_array(),"tip_reference":before(tip).to_array(),"tip_seated":seated(tip).to_array(),"tip_final":point(tip).to_array()})
        }).collect();
        let (vertex, travel) = (0..GRID)
            .map(|i| (i, point(i).distance(before(i))))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        let mut inversions = 0;
        let mut worst_drop = 0.0f32;
        let mut stretch = (0usize, 0usize, 0.0f32);
        for row in 0..=YOKE_ROWS {
            for col in 0..=COLUMNS {
                let a = index(row, col);
                if row < YOKE_ROWS {
                    let drop = radius(point(a)) - radius(point(index(row + 1, col)));
                    if drop > 1e-6 {
                        inversions += 1;
                        worst_drop = worst_drop.max(drop);
                    }
                }
                for b in [
                    if col < COLUMNS { Some(a + 1) } else { None },
                    if row < YOKE_ROWS {
                        Some(a + COLUMNS + 1)
                    } else {
                        None
                    },
                    if row < YOKE_ROWS && col > 0 {
                        Some(a + COLUMNS)
                    } else {
                        None
                    },
                ]
                .into_iter()
                .flatten()
                {
                    let ratio =
                        point(a).distance(point(b)) / before(a).distance(before(b)).max(1e-6);
                    if ratio > stretch.2 {
                        stretch = (a, b, ratio);
                    }
                }
            }
        }
        Some(
            serde_json::json!({"layer":if self.inner_layer.is_some(){"mantle"}else{"main"},
            "head_triangles":self.head_surface.as_ref().map_or(0,|s|s.triangles.len()),
            "reference_scope":"posed body/arm point fit before inner seating and whole-face constraints; not an unstretched material metric",
            "rest_scope":"neutral recipe rest, already body/arm point-fitted", "seating_valid":self.contact_seating_valid, "radial_witness":self.contact_radial_trace,
            "row_range_order":["radius_min","radius_max","y_min","y_max"],"rows":rows,
            "max_travel":{"vertex":vertex,"row":vertex/(COLUMNS+1),"column":vertex%(COLUMNS+1),"distance":travel,"reference":before(vertex).to_array(),"final":point(vertex).to_array()},
            "radial_order_inversions":inversions,"max_radial_order_drop":worst_drop,
            "max_yoke_edge_stretch":{"vertices":[stretch.0,stretch.1],"ratio":stretch.2},
            "attachment_reference_radius_max":self.attachment_reference.iter().map(|p|(*p+offset).with_y(0.0).length()).fold(0.0f32,f32::max),
            "cpu_scope":"cumulative phase wall elapsed ms; shared support lookup includes first materialization; excludes render/GPU wait",
            "cpu_order":["support_lookup_build","combined_support_build","point_fit_pose_prep","constraints"],"cpu_ms":self.contact_cpu_ms,"solves":self.contact_solves}),
        )
    }
    pub(super) fn contact_fallbacks(&self) -> u64 {
        self.contact_fallbacks
    }
    pub(super) fn contact_workload(&self) -> Option<[usize; 3]> {
        self.contact_surface.as_ref()?;
        let surface = self.arm_surface.as_ref()?;
        Some([
            surface.triangles.len(),
            surface.tree.len(),
            surface.bins.iter().map(Vec::len).sum(),
        ])
    }

    pub(super) fn needs_contacts(&self) -> bool {
        self.wrapped
    }

    /// Body-only legacy capes ignore study arm contacts entirely.
    pub(super) fn set_contacts(&mut self, contacts: Vec<super::visual_contact::Cylinder>) {
        if self.wrapped && self.scene.is_none() {
            if let Some(support) = &self.support {
                self.scene = Some(super::visual_contact::Scene::new(support.clone(), contacts));
                self.arm_surface = self
                    .scene
                    .as_ref()
                    .map(|scene| Arc::new(LayerSurface::from_triangles(scene.cloth_hulls())));
                self.invalidate();
                return;
            }
        }
        if let Some(scene) = &mut self.scene {
            if scene.set_pose(contacts) {
                if self.contact_surface.is_none() {
                    self.arm_surface =
                        Some(Arc::new(LayerSurface::from_triangles(scene.cloth_hulls())));
                }
                self.invalidate();
            }
        }
    }

    pub(super) fn set_contact_surface(&mut self, contacts: Arc<ContactSurface>) {
        if !self.wrapped
            || self
                .contact_surface
                .as_ref()
                .is_some_and(|old| old.parts == contacts.parts)
        {
            return;
        }
        let started = std::time::Instant::now();
        self.arm_surface = Some(contacts.surface());
        self.contact_cpu_ms[0] += started.elapsed().as_secs_f64() * 1000.0;
        self.contact_surface = Some(contacts);
        self.invalidate();
    }

    pub(super) fn set_head_surface(&mut self, contacts: Arc<ContactSurface>) {
        if !self.wrapped
            || self.inner_layer.is_none()
            || self
                .head_contact_surface
                .as_ref()
                .is_some_and(|old| old.parts == contacts.parts)
        {
            return;
        }
        let started = std::time::Instant::now();
        self.head_surface = Some(contacts.surface());
        self.contact_cpu_ms[0] += started.elapsed().as_secs_f64() * 1000.0;
        self.head_contact_surface = Some(contacts);
        self.invalidate();
    }

    pub(super) fn map_references(
        &mut self,
        entity: &impl Fn(Entity) -> Entity,
        mesh: &mut impl FnMut(&Handle<Mesh>) -> Handle<Mesh>,
    ) {
        self.entity = entity(self.entity);
        self.inner_layer = self.inner_layer.map(entity);
        self.mesh = mesh(&self.mesh);
    }
    pub(super) fn invalidate(&mut self) {
        self.last_rotation = None;
        self.last_inner_rotation = None;
    }
}

fn index(row: usize, column: usize) -> usize {
    row * (COLUMNS + 1) + column
}

fn surface_normals(points: &[Vec3], normals: &mut [Vec3]) {
    for row in 0..=ROWS {
        for column in 0..=COLUMNS {
            let across = points[index(row, (column + 1).min(COLUMNS))]
                - points[index(row, column.saturating_sub(1))];
            let down = points[index((row + 1).min(ROWS), column)]
                - points[index(row.saturating_sub(1), column)];
            normals[index(row, column)] = across.cross(down).normalize_or(-Vec3::Z);
        }
    }
}

fn write(mesh: &mut Mesh, points: &[Vec3], normals: &[Vec3]) {
    if let Some(VertexAttributeValues::Float32x3(vertices)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for i in 0..GRID {
            vertices[i] = (points[i] + normals[i] * THICKNESS * 0.5).to_array();
            vertices[GRID + i] = (points[i] - normals[i] * THICKNESS * 0.5).to_array();
        }
    }
    if let Some(VertexAttributeValues::Float32x3(vertices)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
    {
        for i in 0..GRID {
            vertices[i] = normals[i].to_array();
            vertices[GRID + i] = (-normals[i]).to_array();
        }
    }
}

pub(super) fn spawn(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    parent: Entity,
    recipe: Recipe,
    ghost: bool,
) -> CloakRig {
    let material = library
        .get(
            format!("creature/cloak-material/{ghost}/{}", recipe.satin),
            meshes,
            || {
                let mut parts = Parts::default();
                let builder = if recipe.satin {
                    &mut parts.satin
                } else {
                    &mut parts.matte
                };
                builder.tri([Vec3::ZERO, Vec3::X, Vec3::Y], Vec3::ONE);
                if ghost {
                    parts.ghostify();
                }
                parts
            },
        )
        .0[0]
        .1
        .clone();
    let support_started = std::time::Instant::now();
    let arm_surface = recipe
        .contact_surface
        .as_ref()
        .map(|surface| surface.surface())
        .or_else(|| {
            recipe
                .scene
                .as_ref()
                .map(|scene| Arc::new(LayerSurface::from_triangles(scene.cloth_hulls())))
        });
    let head_surface = recipe
        .head_contact_surface
        .as_ref()
        .filter(|_| recipe.inner_layer.is_some())
        .map(|head| head.surface());
    let support_ms = support_started.elapsed().as_secs_f64() * 1000.0;
    let point_started = std::time::Instant::now();
    let shoulder_profile = recipe.shoulder_profile();
    let mut rest: Vec<_> = (0..GRID)
        .map(|i| recipe.point(i / (COLUMNS + 1), i % (COLUMNS + 1)))
        .collect();
    let mut attachment_reference = Vec::new();
    let mut attachment_limits = None;
    let mut attachment_failed = false;
    let mut attachment_constraint_ms = 0.0;
    if recipe.inner_layer.is_some() {
        if let Some(head) = &recipe.head_contact_surface {
            let offset = Vec3::new(0.0, 0.53, -0.15);
            attachment_reference.extend_from_slice(&rest[..2 * (COLUMNS + 1)]);
            // Preserve the original ring's overall neck-localization envelope.
            // Per-ray torso radii are not a head silhouette: front torso roots
            // can be narrower than the actual jaw before any cloth correction.
            // This envelope adds no padding to the old ring's maximum extent.
            let original_widths = std::array::from_fn(|i| {
                let column = i % (COLUMNS + 1);
                let row = i / (COLUMNS + 1);
                let collar = rest[column] + offset;
                let ray = collar.with_y(0.0).normalize_or(Vec3::NEG_Z);
                let original = supported_collar_station(ray, &recipe.support, 0.565)?;
                let delta = (original - collar).with_y(0.0) * (1.0 - row as f32 / YOKE_ROWS as f32);
                Some((rest[i] + offset + delta).with_y(0.0).length())
            });
            let limits = attachment_radius_envelope(&original_widths)
                .unwrap_or([f32::NAN; 2 * (COLUMNS + 1)]);
            let attachment_started = std::time::Instant::now();
            if let Err(failure) = fit_mantle_head_reference(
                &mut rest,
                Quat::IDENTITY,
                &head.surface(),
                &recipe.support,
                &limits,
            ) {
                attachment_failed = true;
                #[cfg(test)]
                eprintln!("head attachment initialization failed: {failure:?}");
                #[cfg(not(test))]
                warn!("head attachment initialization failed: {failure:?}");
            }
            attachment_constraint_ms += attachment_started.elapsed().as_secs_f64() * 1000.0;
            attachment_limits = Some(limits);
        }
    }
    let point_ms = point_started.elapsed().as_secs_f64() * 1000.0 - attachment_constraint_ms;
    // Contact fitting is an output, never the next solve's rest shape. In
    // particular, refitting a raised roof changes its inclination and can
    // amplify the same shoulder clearance on every subsequent solve.
    let mut points = rest.clone();
    let contact_reference = if recipe.cut.is_some() {
        points.clone()
    } else {
        Vec::new()
    };
    let mut contact_cpu_ms = [support_ms, 0.0, point_ms, attachment_constraint_ms];
    let mut contact_solves = u64::from(attachment_limits.is_some());
    let constraint_started = std::time::Instant::now();
    let mut contact_fallbacks = u64::from(attachment_failed)
        + u64::from(
            recipe.inner_layer.is_some()
                && recipe.cut.is_some()
                && recipe.fitted_outer_collar.get().is_none_or(Option::is_none),
        );
    let mut contact_radial_trace = None;
    // A dependent mantle is first fitted by place_layers against its seated
    // inner cloth, not an incomplete startup-only arm solve.
    if let (None, Some(surface)) = (recipe.inner_layer, &arm_surface) {
        contact_fallbacks += u64::from(fit_arm_cells(
            &mut points,
            Quat::IDENTITY,
            surface,
            &shoulder_profile,
            &mut contact_radial_trace,
        ));
        contact_solves += 1;
        contact_cpu_ms[3] += constraint_started.elapsed().as_secs_f64() * 1000.0;
    }
    let mut normals = vec![Vec3::ZERO; GRID];
    surface_normals(&points, &mut normals);
    let mut indices = Vec::new();
    for row in 0..ROWS {
        for column in 0..COLUMNS {
            let a = index(row, column) as u32;
            let b = a + 1;
            let c = index(row + 1, column) as u32;
            let d = c + 1;
            indices.extend([a, b, c, b, d, c]);
            let g = GRID as u32;
            indices.extend([a + g, c + g, b + g, b + g, c + g, d + g]);
        }
    }
    // Follow the exterior boundary clockwise and bridge it to the lining.
    let perimeter: Vec<_> = (0..=COLUMNS)
        .map(|c| index(0, c))
        .chain((1..=ROWS).map(|r| index(r, COLUMNS)))
        .chain((0..COLUMNS).rev().map(|c| index(ROWS, c)))
        .chain((1..ROWS).rev().map(|r| index(r, 0)))
        .collect();
    for i in 0..perimeter.len() {
        let a = perimeter[i] as u32;
        let b = perimeter[(i + 1) % perimeter.len()] as u32;
        let g = GRID as u32;
        indices.extend([a, a + g, b, b, a + g, b + g]);
    }
    let pigment = |color: Vec3| {
        let display = color.min(Vec3::ONE);
        let linear = Color::srgb(display.x, display.y, display.z).to_linear();
        let excess = color - display;
        [
            linear.red + excess.x,
            linear.green + excess.y,
            linear.blue + excess.z,
            1.0,
        ]
    };
    let colors: Vec<_> = (0..GRID)
        .map(|i| {
            let row = i / (COLUMNS + 1);
            let col = i % (COLUMNS + 1);
            pigment(study_pigment(&recipe, row, col))
        })
        .chain(std::iter::repeat_n(pigment(recipe.lining), GRID))
        .collect();
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0; 3]; GRID * 2])
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0; 3]; GRID * 2])
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices));
    write(&mut mesh, &points, &normals);
    let mesh = meshes.add(mesh);
    let entity = commands
        .spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material),
            Transform::from_xyz(0.0, 0.53, -0.15),
            Visibility::default(),
            Aabb {
                center: bevy::math::Vec3A::new(0.0, -recipe.length * 0.5, -0.12),
                half_extents: bevy::math::Vec3A::new(1.0, 1.5, 1.0),
            },
            crate::picking::LooseBounds,
        ))
        .id();
    commands.entity(parent).add_child(entity);
    CloakRig {
        #[cfg(test)]
        audit_context: audit_context(),
        entity,
        mesh,
        points,
        bindings: (0..=ROWS)
            .map(|row| {
                let t = if recipe.cut.is_some() {
                    (study_row_fraction(row) * ROWS as f32 - 1.0).max(0.0) / (ROWS - 1) as f32
                } else {
                    row.saturating_sub(1) as f32 / (ROWS - 1) as f32
                };
                SurfaceBinding::single(t * t * (3.0 - 2.0 * t))
            })
            .collect(),
        rest,
        shoulder_profile,
        attachment_reference,
        attachment_limits,
        normals,
        last_rotation: None,
        neutral_reach: arm_surface
            .as_ref()
            .map_or([0.0; 2], |surface| surface.arm_reach()),
        arm_surface,
        contact_surface: recipe.contact_surface,
        head_surface,
        head_contact_surface: recipe.head_contact_surface,
        contact_fallbacks,
        contact_reference,
        contact_seated: Vec::new(),
        contact_seating_valid: true,
        contact_radial_trace,
        contact_cpu_ms,
        contact_solves,
        opening: recipe.cut.map(|cut| cut.opening),
        scene: recipe.scene,
        wrapped: recipe.cut.is_some(),
        inner_layer: recipe.inner_layer,
        last_inner_rotation: None,
        support: recipe.contact.then_some(recipe.support),
    }
}

pub(super) fn place(cloak: &mut CloakRig, rotation: Quat, meshes: &mut Assets<Mesh>) {
    place_base(cloak, rotation, meshes, true);
}

fn place_base(cloak: &mut CloakRig, rotation: Quat, meshes: &mut Assets<Mesh>, fit_arms: bool) {
    #[cfg(test)]
    let _audit = AuditContextGuard::enter(cloak.audit_context.clone());
    if cloak
        .last_rotation
        .is_some_and(|last| rotation.angle_between(last) < 1e-6)
    {
        return;
    }
    let point_started = std::time::Instant::now();
    // The drape entity retains its existing eased swing. Cancel that rotation
    // at the shoulder and introduce it gradually down the private rest grid.
    // The physically fixed collar band forms a supported yoke, not a hinge gap.
    let inverse = rotation.inverse();
    let release = cloak
        .arm_surface
        .as_ref()
        .map_or(FreeEdges::default(), |surface| {
            surface.free_edges(cloak.neutral_reach)
        });
    let opening = cloak.opening;
    let outer_attachment = cloak.inner_layer.is_some();
    let head_contact = cloak.head_surface.clone();
    let mesh_contact = cloak
        .contact_surface
        .as_ref()
        .map(|contacts| contacts.surface());
    let CloakRig {
        points,
        bindings,
        rest,
        support,
        wrapped,
        scene,
        shoulder_profile,
        ..
    } = cloak;
    // Each point traces the back on its own, and tracing dominates a worn
    // cloak's motion, so rows are placed side by side on the compute threads.
    let pool = bevy::tasks::ComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
    points.par_chunk_map_mut(pool, COLUMNS + 1, |row, points| {
        for (column, point) in points.iter_mut().enumerate() {
            let i = index(row, column);
            *point = bindings[row]
                .evaluate(|&give| inverse * Quat::IDENTITY.slerp(rotation, give) * rest[i]);
            if let Some(scene) = scene.as_ref() {
                let at = rotation * *point + Vec3::new(0.0, 0.53, -0.15);
                let at = opening.map_or(at, |opening| {
                    release_front(at, row, column, opening, release)
                });
                let fitted = if row == 0 {
                    if outer_attachment {
                        // The supported ring is immutable torso-local geometry;
                        // re-running the coarse historical march here would
                        // expand a bracket-refined attachment on every pose.
                        let fixed = rest[i] + Vec3::new(0.0, 0.53, -0.15);
                        head_contact
                            .as_ref()
                            .and_then(|head| {
                                head.clearance_point(
                                    fixed,
                                    fixed.with_y(0.0).normalize_or(Vec3::NEG_Z),
                                    LAYER_GAP + THICKNESS,
                                )
                            })
                            .filter(|p| {
                                support.as_ref().is_none_or(|body| {
                                    body.distance(*p / anatomy::HIP_SCALE) >= LAYER_GAP + 0.0097
                                })
                            })
                            .unwrap_or(fixed)
                    } else {
                        support
                            .as_ref()
                            .map_or(at, |body| fit_neck_opening(at, body))
                    }
                } else if let Some(surface) = &mesh_contact {
                    let axis = shoulder_profile.get(i).map_or_else(
                        || contact_axis(i, at.with_y(0.0).normalize_or(Vec3::NEG_Z)),
                        |&coefficient| profile_direction(at, coefficient),
                    );
                    let body_fit = fit_profile_body(at, axis, &|p| scene.body.distance(p));
                    surface.outside_point(body_fit, axis).unwrap_or(body_fit)
                } else {
                    let axis = shoulder_profile.get(i).map_or_else(
                        || contact_axis(i, at.with_y(0.0).normalize_or(Vec3::NEG_Z)),
                        |&coefficient| profile_direction(at, coefficient),
                    );
                    fit_profile_body(at, axis, &|p| scene.distance(p))
                };
                *point = inverse * (fitted - Vec3::new(0.0, 0.53, -0.15));
            }
            if let Some(support) = support.as_ref().filter(|_| row > 1 && scene.is_none()) {
                let mut at = rotation * *point + Vec3::new(0.0, 0.53, -0.15);
                let query = at / anatomy::HIP_SCALE;
                if query.y >= 0.075 {
                    if *wrapped {
                        let out = Vec3::new(query.x, 0.0, query.z).normalize_or(Vec3::NEG_Z);
                        if let Some(hit) = geometry::surface_ray_hit(
                            support,
                            Vec3::Y * query.y,
                            out,
                            LAYER_GAP + 0.006,
                            0.70,
                        ) {
                            let hit = hit * anatomy::HIP_SCALE;
                            if Vec2::new(at.x, at.z).length() < Vec2::new(hit.x, hit.z).length() {
                                at.x = hit.x;
                                at.z = hit.z;
                                *point = inverse * (at - Vec3::new(0.0, 0.53, -0.15));
                            }
                        }
                    } else if let Some(back) =
                        geometry::torso_surface_hit(support, query.x, query.y, false)
                    {
                        // Reserve both faces of the lining after rotation.
                        at.z = at.z.min(back.z * anatomy::HIP_SCALE.z - LAYER_GAP);
                        *point = inverse * (at - Vec3::new(0.0, 0.53, -0.15));
                    }
                }
            }
        }
    });
    cloak.contact_cpu_ms[2] += point_started.elapsed().as_secs_f64() * 1000.0;
    if cloak.wrapped {
        cloak.contact_reference.clone_from(&cloak.points);
        cloak.contact_seated.clear();
        cloak.contact_seating_valid = true;
        cloak.contact_radial_trace = None;
    }
    // A mantle is fitted once against the combined inner-cloth/arm support
    // below. Pre-fitting its arms here would derive the second roof solve's
    // normals from a contact-deformed surface rather than the posed recipe.
    if fit_arms {
        if let Some(surface) = &cloak.arm_surface {
            let started = std::time::Instant::now();
            cloak.contact_fallbacks += u64::from(fit_arm_cells(
                &mut cloak.points,
                rotation,
                surface,
                &cloak.shoulder_profile,
                &mut cloak.contact_radial_trace,
            ));
            cloak.contact_cpu_ms[3] += started.elapsed().as_secs_f64() * 1000.0;
            cloak.contact_solves += 1;
        }
    }
    surface_normals(&cloak.points, &mut cloak.normals);
    if let Some(mesh) = meshes.get_mut(&cloak.mesh) {
        write(mesh, &cloak.points, &cloak.normals);
    }
    cloak.last_rotation = Some(rotation);
}

#[derive(Clone, Copy)]
struct ContactNode {
    low: Vec3,
    high: Vec3,
    start: usize,
    count: usize,
    children: Option<[usize; 2]>,
}

fn contact_tree(
    triangles: &[[Vec3; 3]],
    ids: &mut [usize],
    start: usize,
    nodes: &mut Vec<ContactNode>,
) -> usize {
    let low = ids
        .iter()
        .flat_map(|&i| triangles[i])
        .fold(Vec3::splat(f32::INFINITY), Vec3::min);
    let high = ids
        .iter()
        .flat_map(|&i| triangles[i])
        .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
    let node = nodes.len();
    nodes.push(ContactNode {
        low,
        high,
        start,
        count: ids.len(),
        children: None,
    });
    if ids.len() > 8 {
        let span = high - low;
        let axis = if span.x >= span.y && span.x >= span.z {
            0
        } else if span.y >= span.z {
            1
        } else {
            2
        };
        let middle = ids.len() / 2;
        ids.select_nth_unstable_by(middle, |&a, &b| {
            let center = |i: usize| triangles[i].iter().map(|p| p[axis]).sum::<f32>();
            center(a).total_cmp(&center(b))
        });
        let (left, right) = ids.split_at_mut(middle);
        let a = contact_tree(triangles, left, start, nodes);
        let b = contact_tree(triangles, right, start + middle, nodes);
        nodes[node].children = Some([a, b]);
    }
    node
}

struct ContactCandidates<'a> {
    surface: &'a LayerSurface,
    planes: &'a [(Vec3, f32)],
    // Median splitting bounds stack depth by usize::BITS for any Vec length.
    stack: [usize; usize::BITS as usize],
    count: usize,
    leaf: std::ops::Range<usize>,
    support_floor: Option<(Vec3, f32)>,
}
impl<'a> Iterator for ContactCandidates<'a> {
    type Item = &'a [Vec3; 3];
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(i) = self.leaf.next() {
                return Some(&self.surface.triangles[self.surface.tree_ids[i]]);
            }
            if self.count == 0 {
                return None;
            }
            self.count -= 1;
            let node = self.surface.tree[self.stack[self.count]];
            if self
                .planes
                .iter()
                .chain(self.support_floor.iter())
                .any(|&(normal, offset)| {
                    let extreme = Vec3::new(
                        if normal.x >= 0.0 {
                            node.high.x
                        } else {
                            node.low.x
                        },
                        if normal.y >= 0.0 {
                            node.high.y
                        } else {
                            node.low.y
                        },
                        if normal.z >= 0.0 {
                            node.high.z
                        } else {
                            node.low.z
                        },
                    );
                    normal.dot(extreme) < offset - 1e-6
                })
            {
                continue;
            }
            if let Some(children) = node.children {
                self.stack[self.count] = children[0];
                self.stack[self.count + 1] = children[1];
                self.count += 2;
            } else {
                self.leaf = node.start..node.start + node.count;
            }
        }
    }
}

/// Posed triangle contact in the shared torso frame. Height bins keep each
/// mantle vertex query local without assuming the two hems have equal lengths.
struct LayerSurface {
    bins: Vec<Vec<usize>>,
    triangles: Vec<[Vec3; 3]>,
    first_bin: Vec<usize>,
    bottom: f32,
    height: f32,
    tree: Vec<ContactNode>,
    tree_ids: Vec<usize>,
    obstacle_components: OnceLock<Vec<Option<usize>>>,
    #[cfg(test)]
    closed_faces: OnceLock<Vec<Option<usize>>>,
}

impl LayerSurface {
    fn obstacle_groups(&self) -> &[Option<usize>] {
        self.obstacle_components
            .get_or_init(|| super::visual_contact::closed_components(&self.triangles))
    }

    /// Signed ray crossings are evaluated only for proven closed components.
    /// Open decorative skins remain collision triangles, never solid interiors.
    fn component_crossings(
        &self,
        p: Vec3,
        ray: Vec3,
        ambiguous: &mut bool,
    ) -> Vec<(usize, f32, i32)> {
        let a = ray.any_orthonormal_vector();
        let b = ray.cross(a);
        let planes = [
            (a, a.dot(p) - 1e-6),
            (-a, -a.dot(p) - 1e-6),
            (b, b.dot(p) - 1e-6),
            (-b, -b.dot(p) - 1e-6),
        ];
        let groups = self.obstacle_groups();
        let mut hits = Vec::new();
        for tri in self.clipped_candidates(&planes) {
            let index = (tri as *const [Vec3; 3] as usize - self.triangles.as_ptr() as usize)
                / std::mem::size_of::<[Vec3; 3]>();
            let Some(group) = groups[index] else { continue };
            let e1 = tri[1] - tri[0];
            let e2 = tri[2] - tri[0];
            let cross = ray.cross(e2);
            let det = e1.dot(cross);
            if det.abs() < 1e-9 {
                continue;
            }
            let delta = p - tri[0];
            let u = delta.dot(cross) / det;
            let q = delta.cross(e1);
            let v = ray.dot(q) / det;
            let distance = e2.dot(q) / det;
            if u >= -1e-6 && v >= -1e-6 && u + v <= 1.000001 && distance >= 0.0 {
                if u.abs() < 1e-5 || v.abs() < 1e-5 || (1.0 - u - v).abs() < 1e-5 {
                    *ambiguous = true;
                }
                hits.push((group, distance, if det > 0.0 { 1 } else { -1 }));
            }
        }
        hits.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));
        hits
    }

    fn containing_components(&self, p: Vec3) -> Vec<usize> {
        let mut answers: Vec<Vec<usize>> = Vec::new();
        for ray in [
            Vec3::new(1.0, 0.371, 0.173),
            Vec3::new(-0.217, 1.0, 0.433),
            Vec3::new(0.319, -0.271, 1.0),
            Vec3::new(0.713, 0.337, -0.619),
        ] {
            let mut ambiguous = false;
            let hits = self.component_crossings(p, ray.normalize(), &mut ambiguous);
            let mut groups = Vec::new();
            let mut i = 0;
            while i < hits.len() {
                let group = hits[i].0;
                let mut winding = 0;
                while i < hits.len() && hits[i].0 == group {
                    let at = hits[i].1;
                    let mut signed = 0i32;
                    while i < hits.len() && hits[i].0 == group && (hits[i].1 - at).abs() < 1e-5 {
                        signed += hits[i].2;
                        i += 1;
                    }
                    winding += signed.signum();
                }
                if winding != 0 {
                    groups.push(group);
                }
            }
            // A ray through an emitted seam is not a reliable inside oracle.
            // Retry a deterministic independent direction, not a looser gap.
            if !ambiguous {
                if answers.contains(&groups) {
                    return groups;
                }
                answers.push(groups);
            }
        }
        // Require agreement of two non-seam directions. No real component
        // has this ID. The exit lookup then fails, and
        // initialization is explicitly rejected rather than treating an
        // all-direction seam ambiguity as certified outside membership.
        vec![usize::MAX]
    }

    /// Exit only solids that actually contain this point. A remote arm may
    /// intersect the outward ray without containing the point: never project
    /// an already-clear vertex to that arm's far side. Near-surface gap is
    /// certified by the complete finite face domain, expanded by the gap.
    fn outside_point(&self, mut p: Vec3, axis: Vec3) -> Option<Vec3> {
        for _ in 0..12 {
            let inside = self.containing_components(p);
            if inside.is_empty() {
                return Some(p);
            }
            let hits = self.component_crossings(p, axis, &mut false);
            let mut distance = 0.0f32;
            for group in inside {
                let mut i = 0;
                let mut exit = None;
                while i < hits.len() {
                    if hits[i].0 != group {
                        i += 1;
                        continue;
                    }
                    let at = hits[i].1;
                    let mut signed = 0i32;
                    while i < hits.len() && hits[i].0 == group && (hits[i].1 - at).abs() < 1e-5 {
                        signed += hits[i].2;
                        i += 1;
                    }
                    if signed != 0 {
                        exit = Some(at);
                        break;
                    }
                }
                distance = distance.max(exit?);
            }
            p += axis * (distance + LAYER_GAP + THICKNESS);
        }
        self.containing_components(p).is_empty().then_some(p)
    }

    fn clearance_point(&self, mut p: Vec3, axis: Vec3, gap: f32) -> Option<Vec3> {
        for _ in 0..128 {
            p = self.outside_point(p, axis)?;
            let planes = [
                (Vec3::X, p.x - gap),
                (Vec3::NEG_X, -p.x - gap),
                (Vec3::Y, p.y - gap),
                (Vec3::NEG_Y, -p.y - gap),
                (Vec3::Z, p.z - gap),
                (Vec3::NEG_Z, -p.z - gap),
            ];
            let distance = self
                .clipped_candidates(&planes)
                .map(|&tri| triangle_distance_squared(p, tri).sqrt())
                .fold(gap, f32::min);
            if distance >= gap {
                return Some(p);
            }
            p += axis * (gap - distance).max(0.0005);
        }
        None
    }

    fn new(mesh: &Mesh, rotation: Quat) -> Self {
        let Some(VertexAttributeValues::Float32x3(vertices)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("cloak mesh positions");
        };
        let vertices: Vec<_> = vertices
            .iter()
            .map(|p| rotation * Vec3::from_array(*p) + Vec3::new(0.0, 0.53, -0.15))
            .collect();
        let indices: Vec<_> = mesh
            .indices()
            .expect("cloak mesh triangles")
            .iter()
            .collect();
        Self::from_triangles(
            indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|face| face.map(|i| vertices[i]))
                .collect(),
        )
    }

    fn from_triangles(triangles: Vec<[Vec3; 3]>) -> Self {
        let bottom = triangles
            .iter()
            .flatten()
            .map(|p| p.y)
            .fold(f32::INFINITY, f32::min);
        let top = triangles
            .iter()
            .flatten()
            .map(|p| p.y)
            .fold(f32::NEG_INFINITY, f32::max);
        let mut tree_ids: Vec<_> = (0..triangles.len()).collect();
        let mut tree = Vec::new();
        if !triangles.is_empty() {
            contact_tree(&triangles, &mut tree_ids, 0, &mut tree);
        }
        let mut surface = Self {
            obstacle_components: OnceLock::new(),
            #[cfg(test)]
            closed_faces: OnceLock::new(),
            tree,
            tree_ids,
            bins: vec![Vec::new(); 64],
            triangles,
            first_bin: Vec::new(),
            bottom,
            height: (top - bottom).max(0.001),
        };
        for (id, triangle) in surface.triangles.iter().enumerate() {
            let low = triangle.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
            let high = triangle
                .iter()
                .map(|p| p.y)
                .fold(f32::NEG_INFINITY, f32::max);
            let first = surface.bin(low);
            surface.first_bin.push(first);
            for bin in first..=surface.bin(high) {
                surface.bins[bin].push(id);
            }
        }
        surface
    }

    fn clipped_candidates<'a>(&'a self, planes: &'a [(Vec3, f32)]) -> ContactCandidates<'a> {
        ContactCandidates {
            surface: self,
            planes,
            stack: [0; usize::BITS as usize],
            count: usize::from(!self.tree.is_empty()),
            leaf: 0..0,
            support_floor: None,
        }
    }

    fn ray(&self, origin: Vec3, direction: Vec3) -> Option<f32> {
        let a = direction.any_orthonormal_vector();
        let b = direction.cross(a);
        let planes = [
            (a, a.dot(origin) - 1e-6),
            (-a, -a.dot(origin) - 1e-6),
            (b, b.dot(origin) - 1e-6),
            (-b, -b.dot(origin) - 1e-6),
        ];
        let mut far: Option<f32> = None;
        for tri in self.clipped_candidates(&planes) {
            let e1 = tri[1] - tri[0];
            let e2 = tri[2] - tri[0];
            let cross = direction.cross(e2);
            let det = e1.dot(cross);
            if det.abs() < 1e-9 {
                continue;
            }
            let delta = origin - tri[0];
            let u = delta.dot(cross) / det;
            let q = delta.cross(e1);
            let v = direction.dot(q) / det;
            if u < -1e-6 || v < -1e-6 || u + v > 1.000001 {
                continue;
            }
            let distance = e2.dot(q) / det;
            if distance >= 0.0 {
                far = Some(far.map_or(distance, |old| old.max(distance)));
            }
        }
        far
    }

    fn bin(&self, y: f32) -> usize {
        (((y - self.bottom) / self.height * 64.0).floor() as isize).clamp(0, 63) as usize
    }

    /// Exact support of inner triangles clipped to an outer cell's complete
    /// height/angular footprint. A peak between outer vertex rays still
    /// contributes, unlike pointwise radial sampling.
    fn support_plane(
        &self,
        normal: Vec3,
        low: f32,
        high: f32,
        lower: Vec3,
        upper: Vec3,
    ) -> Option<f32> {
        if high < self.bottom || low > self.bottom + self.height {
            return None;
        }
        let planes = [
            (Vec3::Y, low),
            (Vec3::NEG_Y, -high),
            (lower, -THICKNESS),
            (upper, -THICKNESS),
        ];
        let mut support: Option<f32> = None;
        // Height bins alone admit every arm in this band. Query the exact
        // angular/height footprint in the existing BVH instead; a subtree's
        // AABB also bounds its possible improvement of the current support.
        // Both rejections are conservative, with no simplification error.
        let mut candidates = self.clipped_candidates(&planes);
        while let Some(triangle) = candidates.next() {
            // A triangle's unclipped support bounds all its clipped vertices.
            let bound = triangle
                .iter()
                .map(|p| p.dot(normal))
                .fold(f32::NEG_INFINITY, f32::max);
            if support.is_some_and(|old| bound <= old) {
                continue;
            }
            if let Some(value) = clipped_support(*triangle, normal, &planes) {
                support = Some(support.map_or(value, |old| old.max(value)));
                candidates.support_floor = support.map(|value| (normal, value));
            }
        }
        support
    }

    fn arm_reach(&self) -> [f32; 2] {
        let mut reach = [0.0f32; 2];
        for p in self.triangles.iter().flatten().filter(|p| p.y >= 0.15) {
            let side = usize::from(p.x >= 0.0);
            reach[side] = reach[side].max(p.z.abs());
        }
        reach
    }

    fn free_edges(&self, neutral: [f32; 2]) -> FreeEdges {
        let reach = self.arm_reach();
        let strength = std::array::from_fn(|side| {
            ((reach[side] - neutral[side]) / neutral[side].max(0.05)).clamp(0.0, 1.0)
        });
        let mut angles = [PI * 0.68; 2];
        for p in self.triangles.iter().flatten().filter(|p| p.y >= 0.15) {
            let side = usize::from(p.x >= 0.0);
            if p.z.abs() <= neutral[side] {
                continue;
            }
            let radius = p.with_y(0.0).length();
            let padding = ((LAYER_GAP + THICKNESS) / radius.max(0.05)).min(1.0).asin();
            angles[side] = angles[side].max(p.x.abs().atan2(p.z) + padding);
        }
        FreeEdges {
            strength,
            angles: angles.map(|angle| angle.min(PI - 0.04)),
        }
    }

    /// A roof triangle needs an inclined supporting plane, not a horizontal
    /// box enclosing its entire footprint. Expanding the three edge planes by
    /// the lining thickness also protects the emitted offset faces.
    fn roof_supports<const N: usize>(
        &self,
        points: [Vec3; 3],
        normals: [Vec3; N],
    ) -> Option<[f32; N]> {
        let mut planes = [(Vec3::ZERO, 0.0); 3];
        for (edge, plane) in planes.iter_mut().enumerate() {
            let a = points[edge];
            let b = points[(edge + 1) % 3];
            let opposite = points[(edge + 2) % 3];
            let delta = b - a;
            let mut inward = Vec3::new(-delta.z, 0.0, delta.x).normalize_or_zero();
            if inward.dot(opposite - a) < 0.0 {
                inward = -inward;
            }
            *plane = (inward, inward.dot(a) - THICKNESS);
        }
        let mut supports = [f32::NEG_INFINITY; N];
        let mut found = false;
        for &triangle in self.clipped_candidates(&planes) {
            // Reject only if the unclipped triangle cannot improve ANY of
            // the candidate supports. All candidates share one clipping pass.
            if normals
                .iter()
                .zip(&supports)
                .all(|(normal, &old)| triangle.iter().all(|p| normal.dot(*p) <= old))
            {
                continue;
            }
            if let Some((polygon, count)) = clipped_polygon(triangle, &planes) {
                found = true;
                for (normal, support) in normals.iter().zip(&mut supports) {
                    for point in &polygon[..count] {
                        *support = support.max(normal.dot(*point));
                    }
                }
            }
        }
        found.then_some(supports)
    }

    #[cfg(test)]
    fn roof_plane(&self, points: [Vec3; 3], normal: Vec3) -> Option<f32> {
        self.roof_supports(points, [normal]).map(|values| values[0])
    }

    fn bounded_roof_plane(&self, points: [Vec3; 3]) -> Option<(Vec3, f32)> {
        let mut authored = (points[1] - points[0]).cross(points[2] - points[0]);
        if authored.y < 0.0 {
            authored = -authored;
        }
        // Finite initial slope even for a nearly vertical transition. The
        // height budget below, not this numerical guard, controls inflation.
        authored.y = authored.y.max(authored.with_y(0.0).length() * 0.25);
        authored = authored.normalize_or(Vec3::Y);
        let normals = [1.0, 0.5, 0.25, 0.125, 0.0]
            .map(|weight| (authored.with_y(0.0) * weight + Vec3::Y * authored.y).normalize());
        let supports = self.roof_supports(points, normals)?;
        let margin = LAYER_GAP + THICKNESS;
        let horizontal = supports[4] + margin;
        for (normal, support) in normals.into_iter().zip(supports) {
            let plane = support + margin;
            // Preserve an already higher authored point, but do not borrow
            // the full roof drop as clearance budget at a lower neighbor.
            if points.iter().all(|p| {
                let target = p.y + ((plane - normal.dot(*p)) / normal.y).max(0.0);
                target <= p.y.max(horizontal) + ROOF_SLOPE_SLACK
            }) {
                return Some((normal, plane));
            }
        }
        // +Y has exactly the horizontal target, so is always admissible.
        Some((Vec3::Y, horizontal))
    }

    /// IDs occur in several height bins, but are yielded only at their first
    /// overlap with this query. Full-height yoke queries no longer clip a face
    /// up to 64 times; geometry storage is also no longer duplicated per bin.
    #[cfg(test)]
    fn candidates(&self, low: f32, high: f32) -> impl Iterator<Item = &[Vec3; 3]> {
        let first = self.bin(low);
        (first..=self.bin(high)).flat_map(move |bin| {
            self.bins[bin]
                .iter()
                .filter(move |&&id| self.first_bin[id].max(first) == bin)
                .map(move |&id| &self.triangles[id])
        })
    }

    /// Outermost intersection with either face or a closed boundary edge.
    #[cfg(test)]
    fn radius(&self, origin: Vec3, ray: Vec3) -> Option<f32> {
        if origin.y < self.bottom || origin.y > self.bottom + self.height {
            return None;
        }
        let mut far: Option<f32> = None;
        for &id in &self.bins[self.bin(origin.y)] {
            let [a, b, c] = self.triangles[id];
            let e1 = b - a;
            let e2 = c - a;
            let cross = ray.cross(e2);
            let determinant = e1.dot(cross);
            if determinant.abs() < 1e-10 {
                continue;
            }
            let offset = origin - a;
            let u = offset.dot(cross) / determinant;
            let q = offset.cross(e1);
            let v = ray.dot(q) / determinant;
            if u < -1e-5 || v < -1e-5 || u + v > 1.00001 {
                continue;
            }
            let distance = e2.dot(q) / determinant;
            if distance >= 0.0 {
                far = Some(far.map_or(distance, |old| old.max(distance)));
            }
        }
        far
    }
}

// Six endpoints have at most twenty supporting triples, tested in both
// orientations. Degenerate branches emit at most32 planes instead. Intersect
// with five unit prism planes plus an inward radial bevel. A convex polygon gains at most one vertex per
// clip; strict crossings avoid duplicates.
const MAX_HULL_PLANES: usize = 40;
const MAX_CONTACT_PLANES: usize = MAX_HULL_PLANES + 6;
const MAX_CLIPPED_VERTICES: usize = 3 + MAX_CONTACT_PLANES;

/// Fixed scratch buffers keep leaf clipping bounded without heap allocation.
fn clipped_support(triangle: [Vec3; 3], normal: Vec3, planes: &[(Vec3, f32)]) -> Option<f32> {
    let (polygon, count) = clipped_polygon(triangle, planes)?;
    Some(
        polygon[..count]
            .iter()
            .map(|p| p.dot(normal))
            .fold(f32::NEG_INFINITY, f32::max),
    )
}

fn clipped_polygon(
    triangle: [Vec3; 3],
    planes: &[(Vec3, f32)],
) -> Option<([Vec3; MAX_CLIPPED_VERTICES], usize)> {
    assert!(
        planes.len() <= MAX_CONTACT_PLANES,
        "fixed clipping buffers exceed bounded contact planes"
    );
    if planes
        .iter()
        .any(|&(plane, offset)| triangle.iter().all(|p| plane.dot(*p) < offset))
    {
        return None;
    }
    let mut polygon = [Vec3::ZERO; MAX_CLIPPED_VERTICES];
    polygon[..3].copy_from_slice(&triangle);
    let mut count = 3;
    for &(plane, offset) in planes {
        let mut next = [Vec3::ZERO; MAX_CLIPPED_VERTICES];
        let mut length = 0;
        for i in 0..count {
            let a = polygon[i];
            let b = polygon[(i + 1) % count];
            let da = plane.dot(a) - offset;
            let db = plane.dot(b) - offset;
            if da >= 0.0 {
                next[length] = a;
                length += 1;
            }
            if (da > 0.0 && db < 0.0) || (da < 0.0 && db > 0.0) {
                next[length] = a.lerp(b, da / (da - db));
                length += 1;
            }
        }
        if length == 0 {
            return None;
        }
        polygon = next;
        count = length;
    }
    Some((polygon, count))
}

/// Clip against a closed half-space; extrema of a linear support function
/// lie at these vertices, including newly created edge intersections.
#[cfg(test)]
fn clip_layer_polygon(points: &[Vec3], normal: Vec3, offset: f32) -> Vec<Vec3> {
    let mut result = Vec::with_capacity(points.len() + 1);
    if points.is_empty() {
        return result;
    }
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let da = normal.dot(a) - offset;
        let db = normal.dot(b) - offset;
        if da >= 0.0 {
            result.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            result.push(a.lerp(b, da / (da - db)));
        }
    }
    result
}

#[derive(Clone, Copy, Default)]
struct FreeEdges {
    strength: [f32; 2],
    angles: [f32; 2],
}

/// A lifted arm can be in forward extension OR backward wind-up. Sweep its
/// free cloth edge beyond the actual posed hull's polar angle, not a fixed
/// front-only threshold. Neck attachment, radius and cloth topology survive.
fn release_front(at: Vec3, row: usize, column: usize, opening: f32, release: FreeEdges) -> Vec3 {
    let strength = release.strength[0].max(release.strength[1]);
    if strength == 0.0 || row == 0 {
        return at;
    }
    let u = column as f32 / COLUMNS as f32;
    let weight = (row as f32 / YOKE_ROWS as f32).min(1.0);
    let left = (release.angles[0] - opening * 0.5).max(0.0) * release.strength[0];
    let right = (release.angles[1] - opening * 0.5).max(0.0) * release.strength[1];
    let base = -opening * 0.5 - (TAU - opening) * u;
    let actual = at.x.atan2(at.z);
    let lag = (actual - base + PI).rem_euclid(TAU) - PI;
    // At full release use the geometric target angle exactly: subtract the
    // eased cloak rotation/elliptic offset so it cannot put the edge back over
    // the fist. At rest the authored geometry is untouched.
    let turn = (-left * (1.0 - u) + right * u - lag * strength) * weight;
    Quat::from_rotation_y(turn) * at
}

/// Constrain whole connected cells, not their separate vertices. Directions
/// and angular footprints remain fixed throughout the sweep; every later
/// nonnegative correction preserves earlier neighboring support planes.
#[cfg(test)]
fn fit_layer_cells(points: &mut [Vec3], rotation: Quat, support: &LayerSurface) {
    fit_cells(points, rotation, support, &mut None);
}

fn fit_arm_cells(
    points: &mut [Vec3],
    rotation: Quat,
    support: &LayerSurface,
    profile: &[Vec2],
    trace: &mut Option<serde_json::Value>,
) -> bool {
    if let Err(failure) = fit_radial_supports_traced(
        points,
        rotation,
        FaceSupport::arms(support).with_profile(profile),
        trace,
    ) {
        fallback_radial_fit(points, rotation, support, failure);
        true
    } else {
        false
    }
}

#[derive(Debug)]
struct RadialFitFailure {
    row: usize,
    column: usize,
    reason: &'static str,
    detail: [f32; 3],
}

#[cfg(test)]
fn fit_cells(
    points: &mut [Vec3],
    rotation: Quat,
    support: &LayerSurface,
    trace: &mut Option<serde_json::Value>,
) -> bool {
    if let Err(failure) = fit_radial_faces_traced(points, rotation, support, trace) {
        fallback_radial_fit(points, rotation, support, failure);
        true
    } else {
        false
    }
}

fn fallback_radial_fit(
    points: &mut [Vec3],
    rotation: Quat,
    support: &LayerSurface,
    failure: RadialFitFailure,
) {
    // Never omit an unseparated face. Keep the previous conservative
    // solver as the runtime fallback, and make qualification fail loudly.
    #[cfg(test)]
    {
        let _ = (points, rotation, support);
        panic!(
            "radial cloth fit infeasible at row {}, column {}: {} {:?}",
            failure.row, failure.column, failure.reason, failure.detail
        );
    }
    #[cfg(not(test))]
    {
        static REPORTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !REPORTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            eprintln!(
                "radial cloth fit fallback at row {}, column {}: {} {:?}",
                failure.row, failure.column, failure.reason, failure.detail
            );
        }
        fit_cells_conservative(points, rotation, support);
    }
}

/// Study vertices follow frozen outward normals of the authored shoulder
/// profile; collar anchors stay vertical and the turnover stays radial.
/// Collar anchors retain their absolute ceiling.
/// Every face accepts only normals positive on all incident movement axes.
/// The fifth query plane bounds the possible outward
/// travel, so an unrelated cap beyond a collar face cannot move its anchors.
/// Any radial OR vertical budget expansion restarts every face query. Only a solution contained
/// in all queried budgets is committed; convexity then contains every final
/// face, and positive radial corrections preserve all separating planes.
/// Twelve rounds and a fixed candidate set bound work; exhaustion is reported,
/// never treated as convergence or an excuse to omit collision constraints.
#[cfg(test)]
fn fit_radial_faces(
    points: &mut [Vec3],
    rotation: Quat,
    support: &LayerSurface,
) -> Result<(), RadialFitFailure> {
    fit_radial_faces_traced(points, rotation, support, &mut None)
}

#[derive(Clone, Copy)]
struct FaceSupport<'a> {
    inner: Option<&'a LayerSurface>,
    arms: Option<&'a LayerSurface>,
    head: Option<&'a LayerSurface>,
    profile: Option<&'a [Vec2]>,
    radial_limits: Option<&'a [f32]>,
}
impl<'a> FaceSupport<'a> {
    #[cfg(test)]
    fn inner(surface: &'a LayerSurface) -> Self {
        Self {
            inner: Some(surface),
            arms: None,
            head: None,
            profile: None,
            radial_limits: None,
        }
    }
    fn arms(surface: &'a LayerSurface) -> Self {
        Self {
            inner: None,
            arms: Some(surface),
            head: None,
            profile: None,
            radial_limits: None,
        }
    }
    fn with_profile(mut self, profile: &'a [Vec2]) -> Self {
        self.profile = (!profile.is_empty()).then_some(profile);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SupportSource {
    Inner,
    Arms,
    Head,
}
impl SupportSource {
    fn name(self) -> &'static str {
        match self {
            Self::Inner => "inner",
            Self::Arms => "arms",
            Self::Head => "head",
        }
    }
    fn obstacle(self) -> bool {
        self != Self::Inner
    }
}

#[derive(Clone, Copy)]
enum FaceOutcome {
    Empty,
    Fit {
        shifts: [f32; 3],
        planes: [(Vec3, f32, Vec3, SupportSource); 3],
        plane_count: usize,
    },
}

// Work counters are absent from production; the uncached test path exercises
// the same arithmetic and face order without consulting the memoized results.
#[derive(Default, Debug)]
struct FaceFitWork {
    #[cfg(test)]
    rounds: usize,
    #[cfg(test)]
    queries: usize,
    #[cfg(test)]
    clips: usize,
    #[cfg(test)]
    projections: usize,
    #[cfg(test)]
    hits: usize,
    #[cfg(test)]
    empty_hits: usize,
    #[cfg(test)]
    invalidations: usize,
    // Proposed-position outside_point calls only, not initial validation or
    // the individual ray tests performed inside one membership query.
    #[cfg(test)]
    membership_calls: usize,
    #[cfg(test)]
    membership_hits: usize,
    #[cfg(test)]
    membership_changes: usize,
}

// One entry per vertex, scoped to one solve. Its arm surface and movement axis
// are fixed throughout that solve; only the exact proposed position is keyed.
#[derive(Clone, Copy)]
struct ProposalMembership {
    position: [u32; 3],
    outside: Option<Vec3>,
}

// Voronoi-region closest point, independent of the test oracle's plane/edge
// enumeration. Degenerate faces retain their edge-distance contribution.
fn triangle_distance_squared(p: Vec3, [a, b, c]: [Vec3; 3]) -> f32 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    if ab.cross(ac).length_squared() < 1e-16 {
        return [(a, b), (b, c), (c, a)]
            .into_iter()
            .map(|(a, b)| {
                let e = b - a;
                let t = ((p - a).dot(e) / e.length_squared().max(1e-16)).clamp(0.0, 1.0);
                p.distance_squared(a + e * t)
            })
            .fold(f32::INFINITY, f32::min);
    }
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return p.distance_squared(a);
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return p.distance_squared(b);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return p.distance_squared(a + ab * (d1 / (d1 - d3)));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return p.distance_squared(c);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return p.distance_squared(a + ac * (d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        return p.distance_squared(b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6))));
    }
    let inv = 1.0 / (va + vb + vc);
    p.distance_squared(a + ab * (vb * inv) + ac * (vc * inv))
}

fn outside_obstacles(
    arms: Option<&LayerSurface>,
    head: Option<&LayerSurface>,
    mut p: Vec3,
    axis: Vec3,
) -> Option<Vec3> {
    match (arms, head) {
        (Some(a), None) => a.outside_point(p, axis),
        (None, Some(h)) => h.outside_point(p, axis),
        (None, None) => Some(p),
        (Some(a), Some(h)) => {
            for _ in 0..12 {
                p = a.outside_point(p, axis)?;
                p = h.outside_point(p, axis)?;
                if a.containing_components(p).is_empty() {
                    return Some(p);
                }
            }
            None
        }
    }
}

#[cfg(test)]
fn memoized_outside_point(
    arms: &LayerSurface,
    proposed: Vec3,
    axis: Vec3,
    entry: &mut Option<ProposalMembership>,
    enabled: bool,
    work: &mut FaceFitWork,
) -> Option<Vec3> {
    memoized_outside_obstacles(Some(arms), None, proposed, axis, entry, enabled, work)
}

fn memoized_outside_obstacles(
    arms: Option<&LayerSurface>,
    head: Option<&LayerSurface>,
    proposed: Vec3,
    axis: Vec3,
    entry: &mut Option<ProposalMembership>,
    enabled: bool,
    work: &mut FaceFitWork,
) -> Option<Vec3> {
    #[cfg(not(test))]
    let _ = work;
    let position = proposed.to_array().map(f32::to_bits);
    if enabled {
        if let Some(previous) = entry {
            if previous.position == position {
                #[cfg(test)]
                {
                    work.membership_hits += 1;
                }
                return previous.outside;
            }
            #[cfg(test)]
            {
                work.membership_changes += 1;
            }
        }
    }
    #[cfg(test)]
    {
        work.membership_calls += 1;
    }
    let outside = outside_obstacles(arms, head, proposed, axis);
    if enabled {
        *entry = Some(ProposalMembership { position, outside });
    }
    outside
}

#[cfg(test)]
fn fit_radial_faces_traced(
    points: &mut [Vec3],
    rotation: Quat,
    support: &LayerSurface,
    trace: &mut Option<serde_json::Value>,
) -> Result<(), RadialFitFailure> {
    fit_radial_supports_traced(points, rotation, FaceSupport::inner(support), trace)
}
fn fit_radial_supports_traced(
    points: &mut [Vec3],
    rotation: Quat,
    support: FaceSupport<'_>,
    trace: &mut Option<serde_json::Value>,
) -> Result<(), RadialFitFailure> {
    fit_radial_faces_impl(
        points,
        rotation,
        support,
        trace,
        true,
        true,
        &mut FaceFitWork::default(),
    )
}

fn contact_axis(vertex: usize, radial: Vec3) -> Vec3 {
    if vertex < ROOF_VERTICES {
        Vec3::Y
    } else {
        radial
    }
}

fn swept_face_bounds(
    vertices: [Vec3; 3],
    endpoints: [Vec3; 3],
    horizontal: Vec3,
) -> (f32, f32, f32) {
    let low = vertices.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
    let high = vertices
        .iter()
        .chain(&endpoints)
        .map(|p| p.y)
        .fold(f32::NEG_INFINITY, f32::max);
    let radial = endpoints
        .iter()
        .map(|p| horizontal.dot(*p))
        .fold(f32::NEG_INFINITY, f32::max);
    (low, high, radial)
}

fn face_domain_key(high: f32, radial: f32, arms: bool) -> [u32; 2] {
    [high.to_bits(), if arms { radial.to_bits() } else { 0 }]
}

const FACE_NORMALS: usize = 45;

fn face_normals(horizontal: Vec3, authored: f32) -> [Vec3; FACE_NORMALS] {
    let slopes = [
        authored, 0.0, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, -0.25, -0.5, -1.0,
        -2.0, -4.0, -8.0, -16.0, -32.0, -64.0, -128.0,
    ];
    std::array::from_fn(|i| {
        if i < slopes.len() {
            (horizontal + Vec3::Y * slopes[i]).normalize()
        } else if i == slopes.len() {
            Vec3::Y
        } else {
            // A roof rising away from its collar needs an inward/upward
            // separating plane. Append after the original family to preserve
            // tie ordering. The per-face axis gate rejects these on radial
            // hanging vertices; each admitted plane still queries all support.
            (-horizontal + Vec3::Y * slopes[i - slopes.len() - 1]).normalize()
        }
    })
}

/// Arm proximity is bounded by the actual six-endpoint movement hull, not
/// the independent angular/height/radius prism. All offsets use ALL endpoints,
/// so a near-degenerate rank decision can only loosen this conservative hull.
fn swept_arm_planes(vertices: [Vec3; 3], endpoints: [Vec3; 3]) -> Vec<(Vec3, f32)> {
    let points = [
        vertices[0],
        vertices[1],
        vertices[2],
        endpoints[0],
        endpoints[1],
        endpoints[2],
    ];
    let mut planes: Vec<(Vec3, f32)> = Vec::with_capacity(MAX_HULL_PLANES);
    let mut add = |normal: Vec3| {
        let Some(normal) = normal.try_normalize() else {
            return;
        };
        // Dropping a nearly duplicate valid plane is conservative. Never
        // average normals/offsets, which could shrink the feasible domain.
        if planes.iter().any(|(old, _)| old.dot(normal) > 1.0 - 1e-6) {
            return;
        }
        let offset = points
            .iter()
            .map(|p| normal.dot(*p))
            .fold(f32::INFINITY, f32::min)
            - LAYER_GAP
            - THICKNESS;
        planes.push((normal, offset));
    };
    let origin = points[0];
    let far = points
        .iter()
        .copied()
        .max_by(|a, b| {
            a.distance_squared(origin)
                .total_cmp(&b.distance_squared(origin))
        })
        .unwrap();
    let line = far - origin;
    if line.length_squared() < 1e-14 {
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            add(axis);
            add(-axis);
        }
    } else {
        let normal = points
            .iter()
            .map(|p| line.cross(*p - origin))
            .max_by(|a, b| a.length_squared().total_cmp(&b.length_squared()))
            .unwrap();
        if normal.length_squared() < 1e-18 {
            let along = line.normalize();
            let basis = if along.x.abs() < 0.8 {
                Vec3::X
            } else {
                Vec3::Y
            };
            let across = along.cross(basis).normalize();
            for axis in [along, across, along.cross(across)] {
                add(axis);
                add(-axis);
            }
        } else {
            let normal = normal.normalize();
            let height = points
                .iter()
                .map(|p| normal.dot(*p - origin).abs())
                .fold(0.0f32, f32::max);
            if height <= 1e-6 {
                add(normal);
                add(-normal);
                for a in 0..6 {
                    for b in a + 1..6 {
                        let side = (points[b] - points[a]).cross(normal);
                        add(side);
                        add(-side);
                    }
                }
            } else {
                for a in 0..6 {
                    for b in a + 1..6 {
                        for c in b + 1..6 {
                            let Some(normal) = (points[b] - points[a])
                                .cross(points[c] - points[a])
                                .try_normalize()
                            else {
                                continue;
                            };
                            let (low, high) = points
                                .iter()
                                .map(|p| normal.dot(*p - points[a]))
                                .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| {
                                    (lo.min(v), hi.max(v))
                                });
                            if low >= -1e-6 {
                                add(normal);
                            }
                            if high <= 1e-6 {
                                add(-normal);
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(planes.len() <= MAX_HULL_PLANES);
    planes
}

fn arm_contact_planes(
    vertices: [Vec3; 3],
    endpoints: [Vec3; 3],
    prism: &[(Vec3, f32); 5],
) -> Vec<(Vec3, f32)> {
    let mut planes = swept_arm_planes(vertices, endpoints);
    // Offset hull facets alone are a conservative polyhedral neighborhood,
    // not a rounded Minkowski sum. Retain the old unit prism planes as valid
    // bevel constraints: BOTH domains contain the complete clearance sweep.
    planes.extend(prism.iter().map(|&(n, offset)| (n, offset - LAYER_GAP)));
    // An acute offset hull can include a distant neck cap inward of this
    // entire face. This unit supporting bevel contains every endpoint and
    // its clearance neighborhood, without forcing cloth around that cap.
    let inward = vertices
        .iter()
        .map(|p| p.with_y(0.0).normalize_or(Vec3::NEG_Z))
        .sum::<Vec3>()
        .normalize_or(Vec3::NEG_Z);
    let minimum = vertices
        .iter()
        .chain(&endpoints)
        .map(|p| inward.dot(*p))
        .fold(f32::INFINITY, f32::min);
    planes.push((inward, minimum - LAYER_GAP - THICKNESS));
    assert!(planes.len() <= MAX_CONTACT_PLANES);
    planes
}

fn swept_domain_key(high: f32, radial: f32, endpoints: Option<[Vec3; 3]>) -> [u32; 11] {
    let mut key = [0; 11];
    key[..2].copy_from_slice(&face_domain_key(high, radial, endpoints.is_some()));
    if let Some(endpoints) = endpoints {
        for (slot, coordinate) in key[2..]
            .iter_mut()
            .zip(endpoints.into_iter().flat_map(|p| p.to_array()))
        {
            *slot = coordinate.to_bits();
        }
    }
    key
}

type FeatureSource<'a> = (Option<&'a LayerSurface>, &'a [(Vec3, f32)], SupportSource);

// Proposal geometry does not authorize a plane: every source keeps its own
// query domain and contributes to this authoritative maximum.
fn project_feature_support(
    normal: Vec3,
    sources: &[FeatureSource<'_>],
    work: &mut FaceFitWork,
) -> (f32, Vec3, SupportSource) {
    #[cfg(not(test))]
    let _ = work;
    let mut result = (f32::NEG_INFINITY, Vec3::ZERO, SupportSource::Inner);
    for &(surface, domain, kind) in sources {
        let Some(surface) = surface else {
            continue;
        };
        for &triangle in surface.clipped_candidates(domain) {
            #[cfg(test)]
            {
                work.clips += 1;
            }
            if let Some((polygon, count)) = clipped_polygon(triangle, domain) {
                for &point in &polygon[..count] {
                    #[cfg(test)]
                    {
                        work.projections += 1;
                    }
                    let value = normal.dot(point);
                    if value > result.0 {
                        result = (value, point, kind);
                    }
                }
            }
        }
    }
    result
}

fn fit_radial_faces_impl(
    points: &mut [Vec3],
    rotation: Quat,
    support: FaceSupport<'_>,
    trace: &mut Option<serde_json::Value>,
    cache_enabled: bool,
    membership_cache_enabled: bool,
    work: &mut FaceFitWork,
) -> Result<(), RadialFitFailure> {
    #[cfg(not(test))]
    let _ = work;
    *trace = None;
    let offset = Vec3::new(0.0, 0.53, -0.15);
    let reference: Vec<_> = points.iter().map(|p| rotation * *p + offset).collect();
    for arms in [support.arms, support.head].into_iter().flatten() {
        for (i, &point) in reference.iter().enumerate() {
            if !arms.containing_components(point).is_empty() {
                return Err(RadialFitFailure {
                    row: i / (COLUMNS + 1),
                    column: i % (COLUMNS + 1),
                    reason: "head/arm obstacle initialization remains inside a closed component",
                    detail: point.to_array(),
                });
            }
        }
    }
    let directions: Vec<_> = reference
        .iter()
        .map(|p| p.with_y(0.0).normalize_or(Vec3::NEG_Z))
        .collect();
    let axes: Vec<_> = directions
        .iter()
        .enumerate()
        .map(|(i, &radial)| {
            support.profile.map_or_else(
                || contact_axis(i, radial),
                |profile| radial * profile[i].x + Vec3::Y * profile[i].y,
            )
        })
        .collect();
    let collar_ceiling = 0.565 + 2.0 * (LAYER_GAP + THICKNESS + ROOF_SLOPE_SLACK);
    if reference[..=COLUMNS].iter().any(|p| p.y > collar_ceiling) {
        return Err(RadialFitFailure {
            row: 0,
            column: 0,
            reason: "attachment starts above collar ceiling",
            detail: [collar_ceiling, 0.0, 0.0],
        });
    }
    if let Some(limits) = support.radial_limits {
        for (i, &limit) in limits.iter().enumerate() {
            let radius = reference[i].with_y(0.0).length();
            if !limit.is_finite() || radius > limit {
                return Err(RadialFitFailure {
                    row: i / (COLUMNS + 1),
                    column: i % (COLUMNS + 1),
                    reason: "attachment reference exceeds radial cap",
                    detail: [radius, limit, 0.0],
                });
            }
        }
    }
    let mut travels = vec![0.0f32; GRID];
    let mut budgets = vec![0.0f32; GRID];
    // Scalar travel follows each frozen axis. Query BOTH radial and Y
    // endpoint extrema; mixed-axis travel must never enter only one bound.

    // Pure-radial inner hanging domains are invariant. The mixed last shoulder
    // band can change height, and arm domains can change radius: key BOTH
    // extents. Roof faces always requery. Cache lasts one pose/support solve.
    let mut face_cache = vec![None; ROWS * COLUMNS * 2];
    let mut membership_cache = vec![
        None;
        if support.arms.is_some() || support.head.is_some() {
            GRID
        } else {
            0
        }
    ];
    let mut radial_trace = None;
    let mut volume_trace = None;
    #[cfg(test)]
    let mut collar_trace: Option<(f32, String)> = None;
    for iteration in 0..12 {
        #[cfg(test)]
        {
            work.rounds += 1;
        }
        #[cfg(test)]
        {
            collar_trace = None;
        }
        travels.fill(0.0);
        radial_trace = None;
        volume_trace = None;
        for row in 0..ROWS {
            for column in 0..COLUMNS {
                let [a, b, c, d] = [
                    index(row, column),
                    index(row, column + 1),
                    index(row + 1, column),
                    index(row + 1, column + 1),
                ];
                for (face_number, face) in [[a, b, c], [b, d, c]].into_iter().enumerate() {
                    let vertices = face.map(|i| reference[i]);
                    let rays = face.map(|i| directions[i]);
                    let horizontal = rays.iter().copied().sum::<Vec3>().normalize_or(Vec3::NEG_Z);
                    if rays.iter().any(|ray| horizontal.dot(*ray) <= 0.05) {
                        return Err(RadialFitFailure {
                            row,
                            column,
                            reason: "nonconvex angular footprint",
                            detail: [
                                rays.iter()
                                    .map(|r| horizontal.dot(*r))
                                    .fold(f32::INFINITY, f32::min),
                                0.0,
                                0.0,
                            ],
                        });
                    }
                    let tangent = Vec3::new(horizontal.z, 0.0, -horizontal.x);
                    let (lo, hi) =
                        rays.iter()
                            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), ray| {
                                let angle = ray.dot(tangent).atan2(ray.dot(horizontal));
                                (lo.min(angle), hi.max(angle))
                            });
                    let left = horizontal * lo.cos() + tangent * lo.sin();
                    let right = horizontal * hi.cos() + tangent * hi.sin();
                    let endpoints = face.map(|i| reference[i] + axes[i] * budgets[i]);
                    let (low, high, radial_limit) =
                        swept_face_bounds(vertices, endpoints, horizontal);
                    let planes = [
                        (Vec3::Y, low - THICKNESS),
                        (Vec3::NEG_Y, -high - THICKNESS),
                        (Vec3::new(left.z, 0.0, -left.x), -THICKNESS),
                        (Vec3::new(-right.z, 0.0, right.x), -THICKNESS),
                        (-horizontal, -radial_limit - THICKNESS),
                    ];
                    // Hanging cloth must stay outside the outermost retained
                    // surface, including peaks beyond every vertex ray. Only
                    // the roof uses a finite swept radial footprint.
                    let inner_planes = &planes[..if row < ROOF_ROWS { 5 } else { 4 }];

                    let cache_index = (row * COLUMNS + column) * 2 + face_number;
                    let cacheable = cache_enabled && row >= ROOF_ROWS;
                    let domain_key = swept_domain_key(
                        high,
                        radial_limit,
                        (support.arms.is_some() || support.head.is_some()).then_some(endpoints),
                    );
                    let cached = face_cache[cache_index].filter(|(key, _)| *key == domain_key);
                    #[cfg(test)]
                    if face_cache[cache_index].is_some() && cached.is_none() {
                        work.invalidations += 1;
                    }
                    let outcome = if let Some((_, outcome)) = cached {
                        #[cfg(test)]
                        {
                            work.hits += 1;
                            work.empty_hits += usize::from(matches!(outcome, FaceOutcome::Empty));
                        }
                        outcome
                    } else {
                        #[cfg(test)]
                        {
                            work.queries += 1;
                        }
                        // One immutable hull per face/cache miss, shared by every
                        // source evaluation. Inner containment keeps its own prism.
                        let arm_planes = (support.arms.is_some() || support.head.is_some())
                            .then(|| arm_contact_planes(vertices, endpoints, &planes));
                        let evaluate =
                            |work: &mut FaceFitWork,
                             source_filter: Option<SupportSource>|
                             -> Result<FaceOutcome, RadialFitFailure> {
                                #[cfg(not(test))]
                                let _ = work;
                                let cross =
                                    (vertices[1] - vertices[0]).cross(vertices[2] - vertices[0]);
                                let authored = if cross.dot(horizontal).abs() > 1e-6 {
                                    (cross.y / cross.dot(horizontal)).clamp(-128.0, 128.0)
                                } else {
                                    0.0
                                };
                                let normals = face_normals(horizontal, authored);
                                let admissible =
                                    normals.map(|n| face.iter().all(|&i| n.dot(axes[i]) > 1e-6));
                                let mut supports = [f32::NEG_INFINITY; FACE_NORMALS];
                                let mut support_points = [Vec3::ZERO; FACE_NORMALS];
                                let mut support_is_arm = [SupportSource::Inner; FACE_NORMALS];
                                #[cfg(test)]
                                let authored_normal =
                                    cross
                                        .try_normalize()
                                        .map(|n| if n.y < 0.0 { -n } else { n });
                                #[cfg(test)]
                                let mut authored_support =
                                    (f32::NEG_INFINITY, Vec3::ZERO, SupportSource::Inner);
                                let mut found = false;
                                for (surface, domain, is_arm) in [
                                    (support.inner, inner_planes, SupportSource::Inner),
                                    (
                                        support.arms,
                                        arm_planes.as_deref().unwrap_or(&[]),
                                        SupportSource::Arms,
                                    ),
                                    (
                                        support.head,
                                        arm_planes.as_deref().unwrap_or(&[]),
                                        SupportSource::Head,
                                    ),
                                ]
                                .into_iter()
                                .filter_map(|(surface, domain, is_arm)| {
                                    surface
                                        .filter(|_| {
                                            source_filter.is_none_or(|wanted| wanted == is_arm)
                                        })
                                        .map(|s| (s, domain, is_arm))
                                }) {
                                    for &triangle in surface.clipped_candidates(domain) {
                                        #[cfg(test)]
                                        {
                                            work.clips += 1;
                                        }
                                        if let Some((polygon, count)) =
                                            clipped_polygon(triangle, domain)
                                        {
                                            found = true;
                                            #[cfg(test)]
                                            if let Some(n) = authored_normal {
                                                for &p in &polygon[..count] {
                                                    let value = n.dot(p);
                                                    if value > authored_support.0 {
                                                        authored_support = (value, p, is_arm);
                                                    }
                                                }
                                            }
                                            for ((((normal, value), witness), source), allowed) in
                                                normals
                                                    .iter()
                                                    .zip(&mut supports)
                                                    .zip(&mut support_points)
                                                    .zip(&mut support_is_arm)
                                                    .zip(admissible)
                                            {
                                                if !allowed {
                                                    continue;
                                                }
                                                for p in &polygon[..count] {
                                                    #[cfg(test)]
                                                    {
                                                        work.projections += 1;
                                                    }
                                                    let projection = normal.dot(*p);
                                                    if projection > *value {
                                                        *value = projection;
                                                        *witness = *p;
                                                        *source = is_arm;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if !found {
                                    return Ok(FaceOutcome::Empty);
                                }
                                let mut best: Option<([f32; 3], f32)> = None;
                                let mut winning_plane =
                                    (Vec3::ZERO, 0.0, Vec3::ZERO, SupportSource::Inner);
                                let mut best_anchor_deficit = f32::INFINITY;
                                #[cfg(test)]
                                let mut rejected_plane = None;
                                // Preserve the original family and tie order. Only a failed
                                // bounded face pays for at most one actual-feature proposal
                                // per source, each certified again against ALL sources.
                                let mut candidates: [(Vec3, f32, Vec3, SupportSource, bool);
                                    FACE_NORMALS + 3] = std::array::from_fn(|i| {
                                    if i < FACE_NORMALS {
                                        (
                                            normals[i],
                                            supports[i],
                                            support_points[i],
                                            support_is_arm[i],
                                            admissible[i],
                                        )
                                    } else {
                                        (
                                            Vec3::ZERO,
                                            f32::NEG_INFINITY,
                                            Vec3::ZERO,
                                            SupportSource::Inner,
                                            false,
                                        )
                                    }
                                });
                                let mut candidate_count = FACE_NORMALS;
                                let mut candidate_index = 0;
                                loop {
                                    if candidate_index == FACE_NORMALS && best.is_none() {
                                        let caps: Option<Vec<Vec3>> = face
                                            .iter()
                                            .map(|&i| {
                                                let mut available =
                                                    if i <= COLUMNS && axes[i].y > 0.0 {
                                                        (collar_ceiling - reference[i].y).max(0.0)
                                                            / axes[i].y
                                                    } else {
                                                        f32::INFINITY
                                                    };
                                                if let Some(&limit) =
                                                    support.radial_limits.and_then(|v| v.get(i))
                                                {
                                                    let radial = axes[i].with_y(0.0).length();
                                                    if radial > 0.0 {
                                                        available = available.min(
                                                            (limit
                                                                - reference[i]
                                                                    .with_y(0.0)
                                                                    .length())
                                                            .max(0.0)
                                                                / radial,
                                                        );
                                                    }
                                                }
                                                available
                                                    .is_finite()
                                                    .then(|| reference[i] + axes[i] * available)
                                            })
                                            .collect();
                                        if let Some(caps) = caps {
                                            let caps = [caps[0], caps[1], caps[2]];
                                            let sources = [
                                                (support.inner, inner_planes, SupportSource::Inner),
                                                (
                                                    support.arms,
                                                    arm_planes.as_deref().unwrap_or(&[]),
                                                    SupportSource::Arms,
                                                ),
                                                (
                                                    support.head,
                                                    arm_planes.as_deref().unwrap_or(&[]),
                                                    SupportSource::Head,
                                                ),
                                            ]
                                            .map(|(surface, domain, kind)| {
                                                (
                                                    surface.filter(|_| {
                                                        source_filter
                                                            .is_none_or(|wanted| wanted == kind)
                                                    }),
                                                    domain,
                                                    kind,
                                                )
                                            });
                                            for (surface, domain, _) in sources {
                                                let Some(surface) = surface else {
                                                    continue;
                                                };
                                                let Some(normal) = attachment::cap_surface_normal(
                                                    surface, domain, caps,
                                                ) else {
                                                    continue;
                                                };
                                                if face.iter().any(|&i| normal.dot(axes[i]) <= 1e-6)
                                                {
                                                    continue;
                                                }
                                                let (height, witness, source) =
                                                    project_feature_support(normal, &sources, work);
                                                candidates[candidate_count] =
                                                    (normal, height, witness, source, true);
                                                candidate_count += 1;
                                            }
                                        }
                                    }
                                    if candidate_index == candidate_count {
                                        break;
                                    }
                                    let (normal, height, witness, is_arm, allowed) =
                                        candidates[candidate_index];
                                    candidate_index += 1;
                                    // Every later shared-vertex correction must preserve
                                    // this plane. Test each frozen profile axis, including
                                    // both components of the shoulder transition.
                                    if !allowed {
                                        continue;
                                    }
                                    let plane = height + LAYER_GAP + THICKNESS;
                                    let anchor_deficit = face
                                        .iter()
                                        .filter(|&&i| i <= COLUMNS)
                                        .map(|&i| {
                                            let mut available = if axes[i].y > 0.0 {
                                                (collar_ceiling - reference[i].y).max(0.0)
                                                    / axes[i].y
                                            } else {
                                                f32::INFINITY
                                            };
                                            if let Some(limit) = support
                                                .radial_limits
                                                .and_then(|limits| limits.get(i))
                                            {
                                                let radial = axes[i].with_y(0.0).length();
                                                if radial > 0.0 {
                                                    available = available.min(
                                                        (limit - reference[i].with_y(0.0).length())
                                                            .max(0.0)
                                                            / radial,
                                                    );
                                                }
                                            }
                                            (plane
                                                - normal.dot(reference[i])
                                                - normal.dot(axes[i]) * available)
                                                .max(0.0)
                                        })
                                        .fold(0.0, f32::max);
                                    #[cfg(test)]
                                    if anchor_deficit < best_anchor_deficit {
                                        rejected_plane = Some((normal, plane, witness, is_arm));
                                    }
                                    best_anchor_deficit = best_anchor_deficit.min(anchor_deficit);
                                    let mut shifts = [0.0; 3];
                                    let mut feasible = true;
                                    for (j, &i) in face.iter().enumerate() {
                                        let deficit = (plane - normal.dot(reference[i])).max(0.0);
                                        shifts[j] = deficit / normal.dot(axes[i]);
                                        if i <= COLUMNS
                                            && reference[i].y + shifts[j] * axes[i].y
                                                > collar_ceiling
                                        {
                                            feasible = false;
                                            break;
                                        }
                                        if support
                                            .radial_limits
                                            .and_then(|limits| limits.get(i))
                                            .is_some_and(|&limit| {
                                                !limit.is_finite()
                                                    || (reference[i] + axes[i] * shifts[j])
                                                        .with_y(0.0)
                                                        .length()
                                                        > limit
                                            })
                                        {
                                            feasible = false;
                                            break;
                                        }
                                    }
                                    if !feasible {
                                        continue;
                                    }
                                    let cost = shifts.iter().map(|s| s * s).sum::<f32>();
                                    if best.is_none_or(|(_, old)| cost < old) {
                                        best = Some((shifts, cost));
                                        winning_plane = (normal, plane, witness, is_arm);
                                    }
                                }
                                let Some((shifts, _)) = best else {
                                    #[cfg(test)]
                                    if std::env::var_os("TOR_CLOAK_SUPPORT_AUDIT").is_some() {
                                        let mut count = 0usize;
                                        for (surface, domain, source) in [
                                            (support.inner, inner_planes, "inner"),
                                            (
                                                support.arms,
                                                arm_planes.as_deref().unwrap_or(&[]),
                                                "arms",
                                            ),
                                            (
                                                support.head,
                                                arm_planes.as_deref().unwrap_or(&[]),
                                                "head",
                                            ),
                                        ]
                                        .into_iter()
                                        .filter_map(|(s, d, k)| {
                                            s.filter(|_| {
                                                source_filter
                                                    .is_none_or(|wanted| wanted.name() == k)
                                            })
                                            .map(|s| (s, d, k))
                                        }) {
                                            for &triangle in surface.clipped_candidates(domain) {
                                                if let Some((polygon, size)) =
                                                    clipped_polygon(triangle, domain)
                                                {
                                                    eprintln!(
                                                        "support audit polygon: {}",
                                                        serde_json::json!({
                                                            "context":audit_context(),"layer":if support.inner.is_some(){"mantle"}else{"main"},"rotation":rotation.to_array(),
                                                        "face":face,"iteration":iteration,"source":source,
                                                            "points":polygon[..size].iter().map(|p|p.to_array()).collect::<Vec<_>>()
                                                        })
                                                    );
                                                    count += 1;
                                                }
                                            }
                                        }
                                        eprintln!(
                                            "support audit complete: {}",
                                            serde_json::json!({"context":audit_context(),"layer":if support.inner.is_some(){"mantle"}else{"main"},"rotation":rotation.to_array(),"face":face,"iteration":iteration,"polygons":count})
                                        );
                                    }
                                    #[cfg(test)]
                                    if let Some((normal, plane, witness, is_arm)) = rejected_plane {
                                        eprintln!(
                                            "infeasible face diagnostics: {}",
                                            serde_json::json!({
                                                "context":audit_context(),"layer":if support.inner.is_some(){"mantle"}else{"main"},"rotation":rotation.to_array(),
                                                "face":face,"row":row,"column":column,"iteration":iteration,
                                                "source_mode":if source_filter.is_some(){"separate_source_planes"}else{"merged_plane"},"evaluated_source":source_filter.map(|s|s.name()),
                                                "normal":normal.to_array(),"plane":plane,"support_point":witness.to_array(),
                                                "source":is_arm.name(),"collar_deficit":best_anchor_deficit,
                                                "face_reference":vertices.map(|p|p.to_array()),
                                                "face_budget_endpoints":endpoints.map(|p|p.to_array()),
                                                "face_axes":face.map(|i|axes[i].to_array()),"face_budgets":face.map(|i|budgets[i]),
                                                "face_radial_limits":face.map(|i|support.radial_limits.and_then(|limits|limits.get(i)).copied()),
                                                "collar_ceiling":collar_ceiling,"required_clearance":LAYER_GAP+THICKNESS,
                                                "authored_plane_probe": authored_normal.map(|n| serde_json::json!({
                                                    "normal": n.to_array(), "plane": authored_support.0 + LAYER_GAP + THICKNESS,
                                                    "support_point": authored_support.1.to_array(), "source":authored_support.2.name(),
                                                    "axis_admissible":face.iter().all(|&i|n.dot(axes[i])>1e-6),
                                                    "predicted_heights":face.map(|i| if n.dot(axes[i])>1e-6 {
                                                        Some(reference[i].y + axes[i].y * (authored_support.0 + LAYER_GAP + THICKNESS - n.dot(reference[i])).max(0.0)/n.dot(axes[i]))
                                                    } else {None})
                                                }))
                                            })
                                        );
                                    }
                                    return Err(RadialFitFailure {
                                    row,
                                    column,
                                    reason:
                                        "no separating plane preserves bounded collar attachment",
                                    detail: [best_anchor_deficit, low, high],
                                });
                                };
                                Ok(FaceOutcome::Fit {
                                    shifts,
                                    planes: [winning_plane; 3],
                                    plane_count: 1,
                                })
                            };
                        let outcome = if support.inner.is_some() && support.head.is_some() {
                            // A mantle may pass between the head and inner cloth.
                            // Each source needs a separate separating plane, but all
                            // planes use the SAME frozen vertex axes and swept domains.
                            // Nonnegative shared maxima therefore preserve every plane.
                            let mut combined = [0.0f32; 3];
                            let mut planes = [None; 3];
                            let mut plane_count = 0;
                            for source in [
                                SupportSource::Inner,
                                SupportSource::Arms,
                                SupportSource::Head,
                            ] {
                                if let FaceOutcome::Fit {
                                    shifts,
                                    planes: selected,
                                    plane_count: count,
                                } = evaluate(work, Some(source))?
                                {
                                    plane_count += count;
                                    for j in 0..3 {
                                        if planes[j].is_none() || shifts[j] > combined[j] {
                                            planes[j] = Some(selected[j]);
                                        }
                                        combined[j] = combined[j].max(shifts[j]);
                                    }
                                }
                            }
                            if plane_count == 0 {
                                FaceOutcome::Empty
                            } else {
                                FaceOutcome::Fit {
                                    shifts: combined,
                                    planes: planes.map(Option::unwrap),
                                    plane_count,
                                }
                            }
                        } else {
                            evaluate(work, None)?
                        };
                        if cacheable {
                            face_cache[cache_index] = Some((domain_key, outcome));
                        }
                        outcome
                    };
                    let FaceOutcome::Fit {
                        shifts,
                        planes: winning_planes,
                        plane_count,
                    } = outcome
                    else {
                        continue;
                    };
                    #[cfg(test)]
                    if row == 0 {
                        let (j, &vertex) = face
                            .iter()
                            .enumerate()
                            .filter(|(_, i)| **i <= COLUMNS)
                            .max_by(|(a, &ia), (b, &ib)| {
                                (reference[ia].y + shifts[*a] * axes[ia].y)
                                    .total_cmp(&(reference[ib].y + shifts[*b] * axes[ib].y))
                            })
                            .unwrap();
                        let winning_plane = winning_planes[j];
                        let peak = reference[vertex].y + shifts[j] * axes[vertex].y;
                        if peak > collar_ceiling - 0.01
                            && collar_trace.as_ref().is_none_or(|(old, _)| peak > *old)
                        {
                            let collar_cost = face
                                .iter()
                                .zip(shifts)
                                .filter(|(i, _)| **i <= COLUMNS)
                                .map(|(_, s)| s * s)
                                .sum::<f32>();
                            let radial_cost = face
                                .iter()
                                .zip(shifts)
                                .filter(|(i, _)| **i > COLUMNS)
                                .map(|(_, s)| s * s)
                                .sum::<f32>();
                            collar_trace=Some((peak,format!("collar solve vertex={vertex} row={row} column={column} iteration={iteration} reference_y={} final_y={peak} normal={:?} plane={} collar_cost={collar_cost} radial_cost={radial_cost}",reference[vertex].y,winning_plane.0,winning_plane.1)));
                        }
                    }
                    for ((i, shift), winning_plane) in
                        face.into_iter().zip(shifts).zip(winning_planes)
                    {
                        #[cfg(test)]
                        if shift > travels[i]
                            && std::env::var("TOR_CLOAK_VERTEX_AUDIT")
                                .ok()
                                .and_then(|v| v.parse::<usize>().ok())
                                == Some(i)
                        {
                            eprintln!(
                                "attachment shared vertex constraint: {}",
                                serde_json::json!({
                                    "context": audit_context(), "vertex": i, "face": face,
                                    "iteration": iteration, "shift": shift,
                                    "previous_shift": travels[i], "source": winning_plane.3.name(),
                                    "normal": winning_plane.0.to_array(), "plane": winning_plane.1,
                                    "witness": winning_plane.2.to_array(),
                                    "face_reference": vertices.map(|p| p.to_array()),
                                    "face_budget_endpoints": endpoints.map(|p| p.to_array()),
                                    "face_axes": face.map(|v| axes[v].to_array()),
                                })
                            );
                        }
                        travels[i] = travels[i].max(shift);
                        if axes[i].with_y(0.0).length_squared() > 0.0
                            && shift > 0.0
                            && radial_trace.as_ref().is_none_or(|(old, _)| shift > *old)
                        {
                            radial_trace = Some((
                                shift,
                                (
                                    i,
                                    row,
                                    column,
                                    face,
                                    winning_plane,
                                    plane_count,
                                    low,
                                    high,
                                    radial_limit,
                                    iteration,
                                ),
                            ));
                        }
                    }
                }
            }
        }
        if support.arms.is_some() || support.head.is_some() {
            for (i, travel) in travels
                .iter_mut()
                .enumerate()
                .filter(|(_, travel)| **travel > 0.0)
            {
                let proposed = reference[i] + axes[i] * *travel;
                let Some(outside) = memoized_outside_obstacles(
                    support.arms,
                    support.head,
                    proposed,
                    axes[i],
                    &mut membership_cache[i],
                    membership_cache_enabled,
                    work,
                ) else {
                    return Err(RadialFitFailure {
                        row: i / (COLUMNS + 1),
                        column: i % (COLUMNS + 1),
                        reason: "proposed arm-volume exit is ambiguous or unbounded",
                        detail: proposed.to_array(),
                    });
                };
                if i <= COLUMNS && outside.y > collar_ceiling {
                    return Err(RadialFitFailure {
                        row: 0,
                        column: i,
                        reason: "arm-volume exit exceeds collar ceiling",
                        detail: [outside.y, collar_ceiling, reference[i].y],
                    });
                }
                let required = (outside - reference[i]).dot(axes[i]);
                if outside != proposed && required > *travel {
                    *travel = required;
                    if axes[i].with_y(0.0).length_squared() > 0.0
                        && volume_trace
                            .as_ref()
                            .is_none_or(|(_, old, _, _)| required > *old)
                    {
                        volume_trace = Some((i, required, outside, iteration));
                    }
                }
            }
        }
        if let Some(limits) = support.radial_limits {
            for (i, &limit) in limits.iter().enumerate() {
                let radius = (reference[i] + axes[i] * travels[i]).with_y(0.0).length();
                if radius > limit {
                    return Err(RadialFitFailure {
                        row: i / (COLUMNS + 1),
                        column: i % (COLUMNS + 1),
                        reason: "aggregate attachment exceeds radial cap",
                        detail: [radius, limit, travels[i]],
                    });
                }
            }
        }
        let expanded = travels
            .iter()
            .zip(&budgets)
            .any(|(&travel, &budget)| travel > budget);
        if !expanded {
            break;
        }
        if iteration == 11 {
            return Err(RadialFitFailure {
                row: 0,
                column: 0,
                reason: "radial/vertical swept domains did not converge",
                detail: [
                    travels.iter().copied().fold(0.0, f32::max),
                    budgets.iter().copied().fold(0.0, f32::max),
                    12.0,
                ],
            });
        }
        for (i, (budget, &travel)) in budgets.iter_mut().zip(&travels).enumerate() {
            if travel > *budget {
                *budget = if i <= COLUMNS && axes[i].y > 0.0 {
                    (travel + THICKNESS)
                        .min((collar_ceiling - reference[i].y).max(0.0) / axes[i].y)
                        .max(travel)
                } else {
                    travel + THICKNESS
                };
                if let Some(&limit) = support.radial_limits.and_then(|limits| limits.get(i)) {
                    let radial = axes[i].with_y(0.0).length();
                    if radial > 0.0 {
                        // Bound only the conservative future query budget;
                        // candidate geometry was already checked, never clamped.
                        let available =
                            (limit - reference[i].with_y(0.0).length()).max(0.0) / radial;
                        *budget = budget.min(available).max(travel);
                    }
                }
            }
        }
    }
    // Commit only after every face has a feasible constraint. A diagnostic
    // failure leaves the original mesh intact for the conservative fallback.
    #[cfg(test)]
    if let Some((_, trace)) = collar_trace {
        eprintln!("{trace}");
    }
    if let Some((
        travel,
        (
            vertex,
            row,
            column,
            face,
            (normal, plane, witness, is_arm),
            plane_count,
            low,
            high,
            radial_limit,
            iteration,
        ),
    )) = radial_trace
    {
        *trace = Some(
            serde_json::json!({"vertex":vertex,"face":face,"row":row,"column":column,
            "iteration":iteration,"travel":travel,"normal":normal.to_array(),"plane":plane,
            "source_mode":if support.inner.is_some() && support.head.is_some(){"separate_source_planes"}else{"merged_plane"},
            "certified_plane_count":plane_count,"plane_scope":"controlling plane for this vertex; all source planes certified before aggregation",
            "face_reference":face.map(|i|reference[i].to_array()),
            "face_axes":face.map(|i|axes[i].to_array()),
            "face_budgets":face.map(|i|budgets[i]),
            "face_budget_endpoints":face.map(|i|(reference[i]+axes[i]*budgets[i]).to_array()),
            "face_final":face.map(|i|(reference[i]+axes[i]*travels[i]).to_array()),
            "support_projection":plane-LAYER_GAP-THICKNESS,"support_point":witness.to_array(),"source":is_arm.name(),
            "domain_kind":if is_arm.obstacle() {"swept_endpoint_hull_and_prism"}else{"inner_prism"},
            "height_domain":(!is_arm.obstacle()).then_some([low-THICKNESS,high+THICKNESS]),
            "reference_prism_height":[low,high],
            "inner_height_domain":support.inner.map(|_|[low-THICKNESS,high+THICKNESS]),
            "reference_prism_radial":radial_limit,
            "inner_radial_domain":support.inner.and(if row<ROOF_ROWS {Some(radial_limit+THICKNESS)} else {None}),
            "reference":reference[vertex].to_array(),"axis":axes[vertex].to_array(),"final":(reference[vertex]+axes[vertex]*travel).to_array()}),
        );
    }
    if let Some((vertex, travel, final_point, iteration)) = volume_trace {
        if radial_trace.as_ref().is_none_or(|(old, _)| travel > *old) {
            *trace = Some(
                serde_json::json!({"source":"obstacle_volume_exit","vertex":vertex,
                "row":vertex/(COLUMNS+1),"column":vertex%(COLUMNS+1),"travel":travel,"iteration":iteration,
                "reference":reference[vertex].to_array(),"final":final_point.to_array()}),
            );
        }
    }
    let inverse = rotation.inverse();
    for (i, point) in points.iter_mut().enumerate() {
        let direction = axes[i];
        *point = inverse * (reference[i] + direction * travels[i] - offset);
    }
    Ok(())
}

#[cfg_attr(test, allow(dead_code))]
fn fit_cells_conservative(points: &mut [Vec3], rotation: Quat, support: &LayerSurface) {
    let offset = Vec3::new(0.0, 0.53, -0.15);
    let mut posed: Vec<_> = points.iter().map(|p| rotation * *p + offset).collect();
    let directions: Vec<_> = posed
        .iter()
        .map(|p| p.with_y(0.0).normalize_or(Vec3::NEG_Z))
        .collect();
    for row in ROOF_ROWS..ROWS {
        for column in 0..COLUMNS {
            let cell = [
                index(row, column),
                index(row, column + 1),
                index(row + 1, column),
                index(row + 1, column + 1),
            ];
            let normal = cell
                .iter()
                .map(|&i| directions[i])
                .sum::<Vec3>()
                .normalize_or(Vec3::NEG_Z);
            let tangent = Vec3::new(normal.z, 0.0, -normal.x);
            let (low_angle, high_angle) =
                cell.iter()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &i| {
                        let a = directions[i].dot(tangent).atan2(directions[i].dot(normal));
                        (lo.min(a), hi.max(a))
                    });
            let low_ray = normal * low_angle.cos() + tangent * low_angle.sin();
            let high_ray = normal * high_angle.cos() + tangent * high_angle.sin();
            let lower = Vec3::new(low_ray.z, 0.0, -low_ray.x);
            let upper = Vec3::new(-high_ray.z, 0.0, high_ray.x);
            let (low, high) = cell
                .iter()
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &i| {
                    (lo.min(posed[i].y), hi.max(posed[i].y))
                });
            // Only geometry inside this fixed angular corridor can meet the
            // shared boundary during its subsequent upward roof correction.
            // Reserve that corridor's top, not an unrelated raised hand on the
            // other side of the wearer. Above it there is no remaining contact.
            let high = if row == ROOF_ROWS {
                support
                    .support_plane(
                        Vec3::Y,
                        low - THICKNESS,
                        support.bottom + support.height + THICKNESS,
                        lower,
                        upper,
                    )
                    .unwrap_or(high)
                    .max(high)
            } else {
                high
            };
            let Some(plane) =
                support.support_plane(normal, low - THICKNESS, high + THICKNESS, lower, upper)
            else {
                continue;
            };
            for i in cell {
                let denominator = normal.dot(directions[i]);
                debug_assert!(
                    denominator > 0.05,
                    "cloak cell angular footprint must be locally convex"
                );
                if denominator > 0.05 {
                    let deficit =
                        (plane + LAYER_GAP + THICKNESS * 0.5 - normal.dot(posed[i])).max(0.0);
                    posed[i] += directions[i] * (deficit / denominator);
                }
            }
        }
    }
    // Freeze every upward plane before applying corrections. Later positive
    // vertical moves preserve all accepted planes; deriving normals from an
    // already raised neighbor instead would propagate a table-shaped roof.
    let mut roofs = Vec::with_capacity(ROOF_ROWS * COLUMNS * 2);
    for row in 0..ROOF_ROWS {
        for column in 0..COLUMNS {
            let a = index(row, column);
            let b = index(row, column + 1);
            let c = index(row + 1, column);
            let d = index(row + 1, column + 1);
            for face in [[a, b, c], [b, d, c]] {
                let triangle = face.map(|i| posed[i]);
                if let Some((normal, plane)) = support.bounded_roof_plane(triangle) {
                    roofs.push((face, normal, plane));
                }
            }
        }
    }
    for (face, normal, plane) in roofs {
        for i in face {
            let rise = ((plane - normal.dot(posed[i])) / normal.y).max(0.0);
            posed[i].y += rise;
        }
    }
    let inverse = rotation.inverse();
    for (point, fitted) in points.iter_mut().zip(posed) {
        *point = inverse * (fitted - offset);
    }
}

/// Former edge-only seating, retained solely as a regression oracle: seating
/// this strip before the coupled face solve biases its displacement objective
/// upward and can reject valid bounded attachments before the solve starts.
#[cfg(test)]
fn seat_layer_collar(
    points: &mut [Vec3],
    rotation: Quat,
    inner: &LayerSurface,
) -> Result<(), RadialFitFailure> {
    let offset = Vec3::new(0.0, 0.53, -0.15);
    let collar: Vec<_> = points[..=COLUMNS]
        .iter()
        .map(|p| rotation * *p + offset)
        .collect();
    let mut heights: Vec<_> = collar.iter().map(|p| p.y).collect();
    for column in 0..COLUMNS {
        let a = collar[column];
        let b = collar[column + 1];
        let along = (b - a).with_y(0.0).normalize_or(Vec3::X);
        let across = Vec3::new(along.z, 0.0, -along.x);
        let planes = [
            (along, along.dot(a) - THICKNESS),
            (-along, -along.dot(b) - THICKNESS),
            (across, across.dot(a) - THICKNESS),
            (-across, -across.dot(a) - THICKNESS),
        ];
        let top = inner
            .clipped_candidates(&planes)
            .filter_map(|&tri| clipped_support(tri, Vec3::Y, &planes))
            .reduce(f32::max);
        if let Some(top) = top {
            for i in [column, column + 1] {
                heights[i] = heights[i].max(top + LAYER_GAP + THICKNESS);
            }
        }
    }
    for (column, (&height, p)) in heights.iter().zip(&collar).enumerate() {
        if height > p.y.min(0.565) + 2.0 * (LAYER_GAP + THICKNESS + ROOF_SLOPE_SLACK) + 1e-6 {
            return Err(RadialFitFailure {
                row: 0,
                column,
                reason: "inner cloth exceeds bounded collar seating",
                detail: [
                    height,
                    p.y.min(0.565) + 2.0 * (LAYER_GAP + THICKNESS + ROOF_SLOPE_SLACK),
                    p.y,
                ],
            });
        }
    }
    let inverse = rotation.inverse();
    for (column, (p, height)) in collar.into_iter().zip(heights).enumerate() {
        points[column] = inverse * (p.with_y(height) - offset);
    }
    Ok(())
}

fn attachment_radius_envelope(
    widths: &[Option<f32>; 2 * (COLUMNS + 1)],
) -> Option<[f32; 2 * (COLUMNS + 1)]> {
    if widths
        .iter()
        .any(|width| width.is_none_or(|w| !w.is_finite() || w <= 0.0))
    {
        return None;
    }
    let maxima: [f32; 2] = std::array::from_fn(|row| {
        widths[row * (COLUMNS + 1)..(row + 1) * (COLUMNS + 1)]
            .iter()
            .map(|width| width.unwrap())
            .fold(0.0, f32::max)
    });
    Some(std::array::from_fn(|i| maxima[i / (COLUMNS + 1)]))
}

fn validate_attachment_width(
    points: &[Vec3],
    rotation: Quat,
    widths: &[f32; 2 * (COLUMNS + 1)],
) -> Result<(), RadialFitFailure> {
    let offset = Vec3::new(0.0, 0.53, -0.15);
    for (i, &limit) in widths.iter().enumerate() {
        let radius = (rotation * points[i] + offset).with_y(0.0).length();
        if !limit.is_finite() || radius > limit {
            return Err(RadialFitFailure {
                row: i / (COLUMNS + 1),
                column: i % (COLUMNS + 1),
                reason: "combined solve exceeded supported attachment envelope",
                detail: [radius, limit, 0.0],
            });
        }
    }
    Ok(())
}

/// Fit the actual first band around head geometry before fixing its collar XZ.
/// The authoritative all-layer solve follows with fresh swept domains.
fn fit_mantle_head_reference(
    points: &mut [Vec3],
    rotation: Quat,
    head: &LayerSurface,
    body: &crate::sdf::Sculpt,
    limits: &[f32; 2 * (COLUMNS + 1)],
) -> Result<(), RadialFitFailure> {
    let offset = Vec3::new(0.0, 0.53, -0.15);
    let reference: Vec<_> = points.iter().map(|p| rotation * *p + offset).collect();
    let candidate = attachment::fit_head_attachment(&reference, head, limits)?;
    // Head-only initialization must not move its first band into retained torso
    // geometry. The final emitted-head/torso oracle remains independently tested.
    for row in 0..2 {
        for column in 0..COLUMNS {
            let [a, b, c, d] = [
                index(row, column),
                index(row, column + 1),
                index(row + 1, column),
                index(row + 1, column + 1),
            ];
            for face in [[a, b, c], [b, d, c]] {
                for u in 0..=6 {
                    for v in 0..=6 - u {
                        let p = (candidate[face[0]] * u as f32
                            + candidate[face[1]] * v as f32
                            + candidate[face[2]] * (6 - u - v) as f32)
                            / 6.0;
                        if body.distance(p / anatomy::HIP_SCALE) <= THICKNESS + 0.001 {
                            return Err(RadialFitFailure {
                                row,
                                column,
                                reason: "head attachment incident band enters retained torso",
                                detail: p.to_array(),
                            });
                        }
                    }
                }
            }
        }
    }
    for (p, fitted) in points.iter_mut().zip(candidate).take(2 * (COLUMNS + 1)) {
        *p = rotation.inverse() * (fitted - offset);
    }
    Ok(())
}

/// Establish a physically outside reference for the noncollar mantle rows, rather
/// than asking a fixed-height solver to escape a horizontal inner sheet by
/// moving to its rim. Rays are only initialization: all emitted faces still
/// require the combined inner-cloth/arm certificate afterwards.
#[cfg(test)]
fn seat_inner_layer(
    points: &mut [Vec3],
    rotation: Quat,
    inner: &LayerSurface,
) -> Result<(), RadialFitFailure> {
    seat_inner_profile(points, rotation, inner, None)
}
fn seat_inner_profile(
    points: &mut [Vec3],
    rotation: Quat,
    inner: &LayerSurface,
    profile: Option<&[Vec2]>,
) -> Result<(), RadialFitFailure> {
    let offset = Vec3::new(0.0, 0.53, -0.15);
    let inverse = rotation.inverse();
    // Keep the anatomical collar as the joint solver's attachment reference.
    // An unbounded upward ray can hit a remote fold above its finite faces;
    // only the coupled certificate decides the collar's necessary Y travel.
    for (i, point) in points.iter_mut().enumerate().skip(COLUMNS + 1) {
        let at = rotation * *point + offset;
        let fitted = if let Some(profile) = profile {
            seat_profile_surface(at, profile_direction(at, profile[i]), inner)
        } else if i / (COLUMNS + 1) < YOKE_ROWS {
            seat_outermost_surface(at, true, inner)
        } else {
            // Retained cloth reaches the hem. The arm-only contact cutoff
            // must not suppress seating of a longer inner cape below hips.
            let origin = Vec3::Y * at.y;
            let direction = at.with_y(0.0).normalize_or(Vec3::NEG_Z);
            inner.ray(origin, direction).map_or(at, |distance| {
                origin
                    + direction
                        * at.with_y(0.0)
                            .length()
                            .max(distance + LAYER_GAP + THICKNESS)
            })
        };
        *point = inverse * (fitted - offset);
    }
    let ceiling = 0.565 + 2.0 * (LAYER_GAP + THICKNESS + ROOF_SLOPE_SLACK);
    for (column, point) in points[..=COLUMNS].iter().enumerate() {
        let at = rotation * *point + offset;
        if at.y > ceiling {
            return Err(RadialFitFailure {
                row: 0,
                column,
                reason: "inner layer seating exceeds collar ceiling",
                detail: [at.y, ceiling, 0.565],
            });
        }
    }
    Ok(())
}

/// Pose in dependency order, then seat each explicitly linked mantle outside
/// the inner cloak's actual two-sided mesh. An unlinked cloak retains the
/// original deformation path and caching behavior.
pub(super) fn place_layers(
    cloaks: &mut [CloakRig],
    rotation: impl Fn(Entity) -> Option<Quat>,
    meshes: &mut Assets<Mesh>,
) {
    for i in 0..cloaks.len() {
        let (earlier, current) = cloaks.split_at_mut(i);
        let cloak = &mut current[0];
        #[cfg(test)]
        let _audit = AuditContextGuard::enter(cloak.audit_context.clone());
        let Some(rot) = rotation(cloak.entity) else {
            continue;
        };
        let Some(inner_entity) = cloak.inner_layer else {
            place(cloak, rot, meshes);
            continue;
        };
        let inner = earlier
            .iter()
            .find(|inner| inner.entity == inner_entity)
            .expect("a cloak layer must follow its support on the same torso");
        let Some(inner_rot) = rotation(inner.entity) else {
            continue;
        };
        if cloak
            .last_rotation
            .is_some_and(|last| rot.angle_between(last) < 1e-6)
            && cloak
                .last_inner_rotation
                .is_some_and(|last| inner_rot.angle_between(last) < 1e-6)
        {
            continue;
        }
        // Always start from authored rest geometry when either layer moves;
        // projecting an already projected mantle accumulates outward drift.
        cloak.invalidate();
        place_base(cloak, rot, meshes, false);
        let support_started = std::time::Instant::now();
        let support = LayerSurface::new(
            meshes.get(&inner.mesh).expect("inner cloak mesh"),
            inner_rot,
        );
        cloak.contact_cpu_ms[1] += support_started.elapsed().as_secs_f64() * 1000.0;
        let mut candidate = cloak.points.clone();
        let seating_started = std::time::Instant::now();
        let seated =
            seat_inner_profile(&mut candidate, rot, &support, Some(&cloak.shoulder_profile))
                .and_then(|()| {
                    let offset = Vec3::new(0.0, 0.53, -0.15);
                    for (i, p) in candidate.iter_mut().enumerate().skip(COLUMNS + 1) {
                        let at = rot * *p + offset;
                        let axis = profile_direction(at, cloak.shoulder_profile[i]);
                        let fitted = outside_obstacles(
                            cloak.arm_surface.as_deref(),
                            cloak.head_surface.as_deref(),
                            at,
                            axis,
                        )
                        .ok_or(RadialFitFailure {
                            row: i / (COLUMNS + 1),
                            column: i % (COLUMNS + 1),
                            reason: "joint obstacle initialization did not exit head and arms",
                            detail: at.to_array(),
                        })?;
                        *p = rot.inverse() * (fitted - offset);
                    }
                    Ok(())
                });
        cloak.contact_cpu_ms[2] += seating_started.elapsed().as_secs_f64() * 1000.0;
        cloak.contact_seated.clone_from(&candidate);
        cloak.contact_seating_valid = seated.is_ok();
        // Keep containment and obstacle BVHs separate. Arms never inherit
        // the inner layer's unbounded outermost containment requirement.
        let constraint_started = std::time::Instant::now();
        // Certify the seated reference against BOTH retained surfaces. The
        // one-shot ray offsets do not certify slopes or triangle interiors.
        match seated.and_then(|()| {
            let limits = cloak.attachment_limits.as_ref().map(|widths| {
                let offset = Vec3::new(0.0, 0.53, -0.15);
                std::array::from_fn::<_, { 2 * (COLUMNS + 1) }, _>(|i| {
                    if i <= COLUMNS {
                        widths[i].min((cloak.rest[i] + offset).with_y(0.0).length() + 0.015)
                    } else {
                        widths[i]
                    }
                })
            });
            let mut contact_profile = cloak.shoulder_profile.clone();
            if let (Some(head), Some(body), Some(limits)) = (
                cloak.head_surface.as_deref(),
                cloak.support.as_ref(),
                limits.as_ref(),
            ) {
                fit_mantle_head_reference(&mut candidate, rot, head, body, limits)?;
                // Preserve the previous bounded radial initialization.
                // The original near-vertical profile is infeasible at real jaw
                // attachments even with independently certified source planes.
                // A replacement must satisfy all incident constraints; until
                // then each source uses this same frozen bounded radial axis.
                #[cfg(test)]
                if std::env::var_os("TOR_CLOAK_ATTACHMENT_AXES").is_some() {
                    let offset = Vec3::new(0.0, 0.53, -0.15);
                    let reference: Vec<_> = candidate[..2 * (COLUMNS + 1)]
                        .iter()
                        .map(|p| rot * *p + offset)
                        .collect();
                    eprintln!(
                        "attachment axis alternatives: {}",
                        serde_json::json!({
                            "context": audit_context(), "layer": "mantle",
                            "rotation": rot.to_array(),
                            "scope": "Immutable pre-solve alternatives only; changed axes require fresh domains and all incident face certificates",
                            "reference": reference.iter().map(|p| p.to_array()).collect::<Vec<_>>(),
                            "radial_axes": reference.iter().map(|p| profile_direction(*p, Vec2::X).to_array()).collect::<Vec<_>>(),
                            "material_axes": reference.iter().enumerate().map(|(i,p)| profile_direction(*p, cloak.shoulder_profile[i]).to_array()).collect::<Vec<_>>(),
                            "material_coefficients": cloak.shoulder_profile[..2 * (COLUMNS + 1)].iter().map(|p| p.to_array()).collect::<Vec<_>>(),
                            "radial_limits": limits.to_vec(),
                        })
                    );
                }
                #[cfg(test)]
                if std::env::var_os("TOR_CLOAK_ATTACHMENT_SURFACES").is_some() {
                    for (name, surface) in [
                        ("inner", Some(&support)),
                        ("arms", cloak.arm_surface.as_deref()),
                        ("head", Some(head)),
                    ] {
                        if let Some(surface) = surface {
                            eprintln!(
                                "attachment source snapshot: {}",
                                serde_json::json!({
                                    "context": audit_context(), "layer": "mantle",
                                    "rotation": rot.to_array(), "source": name,
                                    "triangles": surface.triangles.iter().map(|tri| tri.map(|p| p.to_array())).collect::<Vec<_>>(),
                                })
                            );
                        }
                    }
                }
                contact_profile[..2 * (COLUMNS + 1)].fill(Vec2::X);
            }
            fit_radial_supports_traced(
                &mut candidate,
                rot,
                FaceSupport {
                    inner: Some(&support),
                    arms: cloak.arm_surface.as_deref(),
                    head: cloak.head_surface.as_deref(),
                    profile: Some(&contact_profile),
                    radial_limits: limits.as_ref().map(|v| v.as_slice()),
                },
                &mut cloak.contact_radial_trace,
            )?;
            if let Some(widths) = &cloak.attachment_limits {
                validate_attachment_width(&candidate, rot, widths)?;
            }
            Ok(())
        }) {
            Ok(()) => cloak.points = candidate,
            Err(failure) => {
                #[cfg(test)]
                if std::env::var_os("TOR_CLOAK_SUPPORT_AUDIT").is_some() {
                    let offset = Vec3::new(0.0, 0.53, -0.15);
                    let body_profile: Vec<_> = [failure.column, (failure.column + 1).min(COLUMNS)].into_iter().map(|column| {
                        let at = rot * candidate[index(0,column)] + offset;
                        let direction = at.with_y(0.0).normalize_or(Vec3::NEG_Z);
                        let samples: Vec<_> = [0.565,0.585,0.605,0.625,0.645,0.649].into_iter().map(|y| {
                            let point = cloak.support.as_ref().map(|body|fit_neck_opening((direction*0.01).with_y(y),body));
                            serde_json::json!({"y":y,"body_attachment":point.map(|p|p.to_array()),
                                "radial_sdf": (0..=25).map(|i| {
                                    let radius=i as f32*0.01;
                                    (radius,cloak.support.as_ref().map(|body|body.distance((direction*radius).with_y(y)/anatomy::HIP_SCALE)))
                                }).collect::<Vec<_>>()})
                        }).collect();
                        serde_json::json!({"column":column,"reference":at.to_array(),"samples":samples})
                    }).collect();
                    eprintln!(
                        "collar attachment region: {}",
                        serde_json::json!({
                            "context":audit_context(),"layer":"mantle","body_profile":body_profile,
                            "inner_ring":(0..=COLUMNS).map(|c|serde_json::json!({"point":(inner_rot*inner.points[index(0,c)]+offset).to_array(),"normal":(inner_rot*inner.normals[index(0,c)]).to_array()})).collect::<Vec<_>>()
                        })
                    );
                }
                cloak.contact_fallbacks += 1;
                let fallback = LayerSurface::from_triangles(
                    support
                        .triangles
                        .iter()
                        .copied()
                        .chain(
                            cloak
                                .arm_surface
                                .iter()
                                .flat_map(|arms| arms.triangles.iter().copied()),
                        )
                        .chain(
                            cloak
                                .head_surface
                                .iter()
                                .flat_map(|head| head.triangles.iter().copied()),
                        )
                        .collect(),
                );
                fallback_radial_fit(&mut cloak.points, rot, &fallback, failure);
            }
        }
        cloak.contact_cpu_ms[3] += constraint_started.elapsed().as_secs_f64() * 1000.0;
        cloak.contact_solves += 1;
        surface_normals(&cloak.points, &mut cloak.normals);
        if let Some(mesh) = meshes.get_mut(&cloak.mesh) {
            write(mesh, &cloak.points, &cloak.normals);
        }
        cloak.last_inner_rotation = Some(inner_rot);
    }
}

#[cfg(test)]
mod source_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn positions(mesh: &Mesh) -> Vec<Vec3> {
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("cloth positions")
        };
        points.iter().map(|p| Vec3::from_array(*p)).collect()
    }

    fn rendered_head_surface(world: &World, rig: &Rig, meshes: &Assets<Mesh>) -> LayerSurface {
        let head = rig.joint(J::Head).expect("head joint");
        let frame = world.get::<Transform>(head).unwrap().compute_affine();
        let mut stack = vec![(head, frame)];
        let mut triangles = Vec::new();
        while let Some((entity, to_torso)) = stack.pop() {
            if let Some(mesh) = world.get::<Mesh3d>(entity).and_then(|m| meshes.get(&m.0)) {
                let vertices = positions(mesh);
                if let Some(indices) = mesh.indices() {
                    let indices: Vec<_> = indices.iter().collect();
                    triangles.extend(
                        indices
                            .as_chunks::<3>()
                            .0
                            .iter()
                            .map(|face| face.map(|i| to_torso.transform_point3(vertices[i]))),
                    );
                }
            }
            if let Some(children) = world.get::<Children>(entity) {
                for child in children.iter() {
                    let local = world
                        .get::<Transform>(child)
                        .map_or(Affine3A::IDENTITY, |t| t.compute_affine());
                    stack.push((child, to_torso * local));
                }
            }
        }
        assert!(
            !triangles.is_empty(),
            "actual head/neck meshes must be present"
        );
        LayerSurface::from_triangles(triangles)
    }

    fn assert_supported_attachment_clear(
        cloak: &CloakRig,
        rotation: Quat,
        mesh: &Mesh,
        head: &LayerSurface,
    ) {
        if cloak.inner_layer.is_none() {
            return;
        }
        assert_eq!(
            cloak.contact_fallbacks, 0,
            "attachment or contact fallback is not acceptance"
        );
        let body = cloak.support.as_ref().expect("retained torso support");
        if let Some(limits) = &cloak.attachment_limits {
            let offset = Vec3::new(0.0, 0.53, -0.15);
            for (i, limit) in limits.iter().enumerate() {
                assert!(
                    (cloak.rest[i] + offset).with_y(0.0).length() <= *limit + 1e-6,
                    "head initialization exceeded the original supported neck footprint"
                );
                assert!(
                    (rotation * cloak.points[i] + offset).with_y(0.0).length() <= *limit + 1e-6,
                    "final first band exceeded the supported neck footprint"
                );
            }
        }
        let vertices = positions(mesh);
        let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
        let offset = Vec3::new(0.0, 0.53, -0.15);
        for face in indices.as_chunks::<3>().0.iter().take(8 * COLUMNS) {
            let face = face.map(|i| rotation * vertices[i] + offset);
            for a in 0..=6 {
                for b in 0..=6 - a {
                    let p =
                        (face[0] * a as f32 + face[1] * b as f32 + face[2] * (6 - a - b) as f32)
                            / 6.0;
                    assert!(
                        body.distance(p / anatomy::HIP_SCALE) > 0.001,
                        "outer attachment/first band penetrates retained torso: {p:?}"
                    );
                    assert!(
                        rendered_clearance_checked(head, p, true) > 0.001,
                        "outer attachment/first band penetrates actual head/neck: {p:?}"
                    );
                }
            }
        }
    }

    fn closed_components(triangles: &[[Vec3; 3]]) -> Vec<Option<usize>> {
        use std::collections::HashMap;
        type Point = [i64; 3];
        let key = |p: Vec3| p.to_array().map(|v| (v as f64 * 100_000.0).round() as i64);
        let mut edges: HashMap<(Point, Point), Vec<(usize, bool)>> = HashMap::new();
        let mut parent: Vec<_> = (0..triangles.len()).collect();
        let mut eligible = vec![false; triangles.len()];
        fn root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        for (i, tri) in triangles.iter().enumerate() {
            let keys = tri.map(key);
            if keys[0] == keys[1]
                || keys[1] == keys[2]
                || keys[2] == keys[0]
                || (tri[1] - tri[0]).cross(tri[2] - tri[0]).length_squared() <= 1e-16
            {
                continue;
            }
            eligible[i] = true;
            for [a, b] in [[tri[0], tri[1]], [tri[1], tri[2]], [tri[2], tri[0]]] {
                let (a, b) = (key(a), key(b));
                if a == b {
                    continue;
                }
                let forward = a < b;
                edges
                    .entry(if forward { (a, b) } else { (b, a) })
                    .or_default()
                    .push((i, forward));
            }
        }
        for faces in edges.values() {
            let first = root(&mut parent, faces[0].0);
            for &(i, _) in &faces[1..] {
                let other = root(&mut parent, i);
                parent[other] = first;
            }
        }
        let mut closed = vec![true; triangles.len()];
        for faces in edges.values() {
            if faces.len() != 2 || faces[0].1 == faces[1].1 {
                let group = root(&mut parent, faces[0].0);
                closed[group] = false;
            }
        }
        (0..triangles.len())
            .map(|i| {
                let group = root(&mut parent, i);
                (closed[group] && eligible[i]).then_some(group)
            })
            .collect()
    }

    // Independent signed mesh witness: winding crossings detect interior
    // samples; nearest triangle/edge distance detects surface penetration.
    // This does not use the cloth support-plane construction or cylinder proxy.
    fn rendered_clearance(surface: &LayerSurface, p: Vec3) -> f32 {
        rendered_clearance_checked(surface, p, false)
    }

    fn rendered_clearance_checked(surface: &LayerSurface, p: Vec3, exhaustive: bool) -> f32 {
        let ray = Vec3::new(1.0, 0.371, 0.173).normalize();
        let a = ray.any_orthonormal_vector();
        let b = ray.cross(a);
        let planes = [
            (a, a.dot(p) - 1e-6),
            (-a, -a.dot(p) - 1e-6),
            (b, b.dot(p) - 1e-6),
            (-b, -b.dot(p) - 1e-6),
        ];
        let mut crossings = Vec::new();
        let faces: Box<dyn Iterator<Item = &[Vec3; 3]> + '_> = if exhaustive {
            Box::new(surface.triangles.iter())
        } else {
            Box::new(surface.clipped_candidates(&planes))
        };
        let closed = surface
            .closed_faces
            .get_or_init(|| closed_components(&surface.triangles));
        for tri in faces {
            // Iterator references point into this immutable Vec. Integer
            // address arithmetic identifies its cached topology flag; no raw
            // pointer is dereferenced. Open cloth patches remain in the
            // nearest-face test but must not invent a solid interior.
            let index = (tri as *const [Vec3; 3] as usize - surface.triangles.as_ptr() as usize)
                / std::mem::size_of::<[Vec3; 3]>();
            let Some(group) = closed[index] else {
                continue;
            };
            let e1 = tri[1] - tri[0];
            let e2 = tri[2] - tri[0];
            let cross = ray.cross(e2);
            let det = e1.dot(cross);
            if det.abs() < 1e-9 {
                continue;
            }
            let delta = p - tri[0];
            let u = delta.dot(cross) / det;
            let q = delta.cross(e1);
            let v = ray.dot(q) / det;
            let t = e2.dot(q) / det;
            if u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t >= 0.0 {
                crossings.push((group, t, if det > 0.0 { 1i32 } else { -1 }));
            }
        }
        crossings.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));
        let mut i = 0;
        while i < crossings.len() {
            let group = crossings[i].0;
            let mut winding = 0;
            while i < crossings.len() && crossings[i].0 == group {
                let at = crossings[i].1;
                let mut signed = 0;
                while i < crossings.len()
                    && crossings[i].0 == group
                    && (crossings[i].1 - at).abs() < 1e-5
                {
                    signed += crossings[i].2;
                    i += 1;
                }
                winding += signed.signum();
            }
            if winding != 0 {
                return -0.001;
            }
        }
        let near = 0.01;
        let planes = [
            (Vec3::X, p.x - near),
            (Vec3::NEG_X, -p.x - near),
            (Vec3::Y, p.y - near),
            (Vec3::NEG_Y, -p.y - near),
            (Vec3::Z, p.z - near),
            (Vec3::NEG_Z, -p.z - near),
        ];
        let mut distance = near;
        let faces: Box<dyn Iterator<Item = &[Vec3; 3]> + '_> = if exhaustive {
            Box::new(surface.triangles.iter())
        } else {
            Box::new(surface.clipped_candidates(&planes))
        };
        for &[a, b, c] in faces {
            let n = (b - a).cross(c - a);
            let area = n.length_squared();
            if area > 1e-16 {
                let projected = p - n * ((p - a).dot(n) / area);
                if [(a, b), (b, c), (c, a)]
                    .iter()
                    .all(|&(a, b)| (b - a).cross(projected - a).dot(n) >= 0.0)
                {
                    distance = distance.min(projected.distance(p));
                }
            }
            for (a, b) in [(a, b), (b, c), (c, a)] {
                let edge = b - a;
                let t = ((p - a).dot(edge) / edge.length_squared().max(1e-16)).clamp(0.0, 1.0);
                distance = distance.min((a + edge * t).distance(p));
            }
        }
        distance
    }

    fn study_recipe(kind: outfit::outerwear::OuterwearAppearance) -> Recipe {
        let body = HumanoidBody::resolve(&Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                ..default()
            }),
        ));
        let r = kind.recipe();
        let outfit::outerwear::OuterwearCut::Drape(cut) = r.cut else {
            panic!("drape fixture");
        };
        let mut recipe = Recipe::new(body, r.cloth, r.lining, false);
        recipe.set_cut(cut);
        recipe
    }

    fn rounded_cap() -> Vec<[Vec3; 3]> {
        let rings = 12;
        let columns = 32;
        let point = |row: usize, col: usize| {
            let theta = PI * row as f32 / rings as f32;
            let angle = TAU * col as f32 / columns as f32;
            Vec3::new(
                theta.sin() * angle.cos(),
                theta.cos(),
                theta.sin() * angle.sin(),
            ) * 0.1
        };
        let mut triangles = Vec::new();
        for row in 0..rings {
            for col in 0..columns {
                let [a, b, c, d] = [
                    point(row, col),
                    point(row, col + 1),
                    point(row + 1, col),
                    point(row + 1, col + 1),
                ];
                triangles.extend([[a, c, b], [b, c, d]]);
            }
        }
        triangles
    }

    #[test]
    fn feature_proposal_reprojects_other_sources_in_their_actual_domains() {
        let triangle = |z| {
            [
                Vec3::new(-0.1, 0.0, z),
                Vec3::new(0.1, 0.0, z),
                Vec3::new(0.0, 0.2, z),
            ]
        };
        let inner = LayerSurface::from_triangles(vec![triangle(0.1)]);
        let head = LayerSurface::from_triangles(vec![triangle(0.2), triangle(0.8)]);
        let domain = [(Vec3::NEG_Z, -0.3)];
        let sources = [
            (Some(&inner), &[][..], SupportSource::Inner),
            (Some(&head), &domain[..], SupportSource::Head),
        ];
        let (height, point, source) =
            project_feature_support(Vec3::Z, &sources, &mut FaceFitWork::default());
        assert!((height-0.2).abs()<1e-6, "other-source decoy must control instead of proposed inner plane, but outside-domain geometry must not");
        assert_eq!(point.z, height);
        assert!(matches!(source, SupportSource::Head));
        let cap = Vec3::new(0.0, 0.1, 0.15);
        assert!(cap.z - 0.1 >= LAYER_GAP + THICKNESS);
        assert!(
            cap.z - height < LAYER_GAP + THICKNESS,
            "full reduction rejects a proposal which only clears its originating source"
        );
    }

    #[test]
    fn rendered_oracle_does_not_treat_open_decoration_as_a_solid() {
        let surface = LayerSurface::from_triangles(vec![[
            Vec3::new(0.3, -1.0, -1.0),
            Vec3::new(0.3, 1.0, -1.0),
            Vec3::new(0.3, 0.0, 1.0),
        ]]);
        assert!(rendered_clearance(&surface, Vec3::ZERO) > 0.001);
        assert!(rendered_clearance_checked(&surface, Vec3::ZERO, true) > 0.001);
        assert!(
            rendered_clearance(&surface, Vec3::new(0.3, 0.0, 0.0)) < 0.001,
            "open decorations still participate in surface clearance"
        );
        let mixed = LayerSurface::from_triangles(
            rounded_cap()
                .into_iter()
                .chain(surface.triangles.iter().copied())
                .collect(),
        );
        assert!(rendered_clearance(&mixed, Vec3::ZERO) < 0.0);
        assert!(rendered_clearance(&mixed, Vec3::new(0.3, 0.0, 0.0)) < 0.001);
        assert!(rendered_clearance(&mixed, Vec3::new(0.15, 0.0, 0.0)) > 0.001);
    }

    #[test]
    fn captured_head_plane_requires_radial_attachment_motion() {
        // Native leather spawn face [1,50,49], complete 60-polygon head audit.
        let normal = Vec3::new(-0.6857347, -0.44721365, 0.57425433);
        let points = [
            Vec3::new(-0.10689167, 0.597, 0.094662756),
            Vec3::new(-0.12577401, 0.60962844, 0.09210095),
            Vec3::new(-0.11395504, 0.5983333, 0.10292),
        ];
        let old_axes = [
            Vec3::Y,
            Vec3::new(-0.06497155, 0.9967523, 0.047576927),
            Vec3::new(-0.07732702, 0.9945568, 0.06983891),
        ];
        for (point, old) in points.into_iter().zip(old_axes) {
            assert!(normal.dot(point) > -0.14292856 + 0.003);
            assert!(normal.dot(old) < -0.35);
            assert!(normal.dot(point.with_y(0.0).normalize()) > 0.89);
        }
    }

    #[test]
    fn radial_attachment_uses_real_axis_ceiling_and_rejects_width_overrun() {
        let offset = Vec3::new(0.0, 0.53, -0.15);
        let mut reference = radial_test_grid();
        for (i, p) in reference.iter_mut().enumerate().take(2 * (COLUMNS + 1)) {
            let at = *p + offset;
            *p = at.with_y(0.0).normalize() * 0.18
                + Vec3::Y * (0.60 + (i / (COLUMNS + 1)) as f32 * 0.01)
                - offset;
        }
        let head = LayerSurface::from_triangles(vec![
            [
                Vec3::new(-0.035, 0.58, 0.20),
                Vec3::new(0.035, 0.58, 0.20),
                Vec3::new(0.035, 0.64, 0.20),
            ],
            [
                Vec3::new(-0.035, 0.58, 0.20),
                Vec3::new(0.035, 0.64, 0.20),
                Vec3::new(-0.035, 0.64, 0.20),
            ],
        ]);
        let mut old = reference.clone();
        assert!(fit_radial_supports_traced(
            &mut old,
            Quat::IDENTITY,
            FaceSupport {
                inner: None,
                arms: None,
                head: Some(&head),
                profile: None,
                radial_limits: None,
            },
            &mut None
        )
        .is_err());
        assert_eq!(old, reference);
        let profile = vec![Vec2::X; GRID];
        let widths = [0.26; 2 * (COLUMNS + 1)];
        let mut fitted = reference.clone();
        fit_radial_supports_traced(
            &mut fitted,
            Quat::IDENTITY,
            FaceSupport {
                inner: None,
                arms: None,
                head: Some(&head),
                profile: Some(&profile),
                radial_limits: Some(&widths),
            },
            &mut None,
        )
        .unwrap();
        assert!(fitted[0].distance(reference[0]) > 0.049);
        assert_eq!(fitted[0].y, reference[0].y);
        validate_attachment_width(&fitted, Quat::IDENTITY, &widths).unwrap();
        let mut mixed_profile = test_shoulder_profile(&reference);
        mixed_profile[..2 * (COLUMNS + 1)].fill(Vec2::X);
        assert!(mixed_profile[index(2, 0)].y > 0.0);
        let mut mixed = reference.clone();
        fit_radial_supports_traced(
            &mut mixed,
            Quat::IDENTITY,
            FaceSupport {
                inner: None,
                arms: None,
                head: Some(&head),
                profile: Some(&mixed_profile),
                radial_limits: Some(&widths),
            },
            &mut None,
        )
        .unwrap();
        validate_attachment_width(&mixed, Quat::IDENTITY, &widths).unwrap();
        for column in 0..COLUMNS {
            let [a, b, c, d] = [
                index(1, column),
                index(1, column + 1),
                index(2, column),
                index(2, column + 1),
            ];
            for face in [[a, b, c], [b, d, c]] {
                let vertices = face.map(|i| mixed[i] + offset);
                let normal = (vertices[1] - vertices[0])
                    .cross(vertices[2] - vertices[0])
                    .normalize_or(Vec3::Z);
                for u in 0..=4 {
                    for v in 0..=4 - u {
                        let p = (vertices[0] * u as f32
                            + vertices[1] * v as f32
                            + vertices[2] * (4 - u - v) as f32)
                            / 4.0;
                        for side in [-1.0, 1.0] {
                            assert!(
                                rendered_clearance_checked(
                                    &head,
                                    p + normal * (side * THICKNESS * 0.5),
                                    true
                                ) > 0.001,
                                "radial-to-mixed incident band intersects actual support"
                            );
                        }
                    }
                }
            }
        }
        let tight = [0.21; 2 * (COLUMNS + 1)];
        let mut rejected = reference.clone();
        assert!(fit_radial_supports_traced(
            &mut rejected,
            Quat::IDENTITY,
            FaceSupport {
                inner: None,
                arms: None,
                head: Some(&head),
                profile: Some(&profile),
                radial_limits: Some(&tight),
            },
            &mut None
        )
        .is_err());
        assert_eq!(rejected, reference);
    }

    #[test]
    fn attachment_envelope_accepts_head_front_without_extending_original_maximum() {
        let mut widths = [Some(0.1174841); 2 * (COLUMNS + 1)];
        widths[8] = Some(0.18);
        widths[index(1, 8)] = Some(0.22);
        let limits = attachment_radius_envelope(&widths).unwrap();
        assert!(0.13802217 > widths[0].unwrap());
        assert!(0.13802217 < limits[0]);
        assert_eq!(limits[0], 0.18);
        assert_eq!(limits[index(1, 0)], 0.22);
        let mut reference: Vec<_> = radial_test_grid()
            .into_iter()
            .map(|p| p + Vec3::new(0.0, 0.53, -0.15))
            .collect();
        for p in &mut reference[..=COLUMNS] {
            *p = p.with_y(0.0).normalize() * 0.13802217 + Vec3::Y * p.y;
        }
        assert!(attachment::fit_head_attachment(
            &reference,
            &LayerSurface::from_triangles(vec![]),
            &limits
        )
        .is_ok());
    }

    #[test]
    fn attachment_envelope_cannot_hide_one_missing_support_root() {
        let mut widths = [Some(0.22); 2 * (COLUMNS + 1)];
        widths[17] = None;
        assert!(attachment_radius_envelope(&widths).is_none());
        widths[17] = Some(f32::NAN);
        assert!(attachment_radius_envelope(&widths).is_none());
    }

    #[test]
    fn final_attachment_gate_rejects_mixed_axis_row_one_growth() {
        let original = radial_test_grid();
        let limits = [0.20; 2 * (COLUMNS + 1)];
        validate_attachment_width(&original, Quat::IDENTITY, &limits).unwrap();
        let mut candidate = original.clone();
        let i = index(1, 16);
        let at = candidate[i] + Vec3::new(0.0, 0.53, -0.15);
        candidate[i] += profile_direction(at, Vec2::splat(std::f32::consts::FRAC_1_SQRT_2)) * 0.07;
        let rejected = validate_attachment_width(&candidate, Quat::IDENTITY, &limits).unwrap_err();
        assert_eq!((rejected.row, rejected.column), (1, 16));
        assert_eq!(
            original,
            radial_test_grid(),
            "a rejected candidate cannot mutate its reference"
        );
    }

    fn radial_test_grid() -> Vec<Vec3> {
        let offset = Vec3::new(0.0, 0.53, -0.15);
        (0..GRID)
            .map(|i| {
                let row = i / (COLUMNS + 1);
                let angle = TAU * (i % (COLUMNS + 1)) as f32 / COLUMNS as f32;
                let (radius, y) = if row <= YOKE_ROWS {
                    let t = row as f32 / YOKE_ROWS as f32;
                    (0.12 + 0.24 * t, 0.565 - 0.095 * t + 0.045 * (PI * t).sin())
                } else {
                    let t = (row - YOKE_ROWS) as f32 / (ROWS - YOKE_ROWS) as f32;
                    (0.36 + 0.03 * t, 0.47 - 0.6 * t)
                };
                Vec3::new(angle.sin() * radius, y, angle.cos() * radius) - offset
            })
            .collect()
    }

    fn assert_face_cache_equivalent(
        reference: &[Vec3],
        rotation: Quat,
        support: FaceSupport<'_>,
    ) -> (FaceFitWork, FaceFitWork, bool) {
        assert_cache_equivalent(reference, rotation, support, false)
    }

    fn assert_cache_equivalent(
        reference: &[Vec3],
        rotation: Quat,
        support: FaceSupport<'_>,
        keep_face_cache: bool,
    ) -> (FaceFitWork, FaceFitWork, bool) {
        let mut cached = reference.to_vec();
        let mut uncached = reference.to_vec();
        let mut cached_trace = None;
        let mut uncached_trace = None;
        let mut cached_work = FaceFitWork::default();
        let mut uncached_work = FaceFitWork::default();
        let cached_result = fit_radial_faces_impl(
            &mut cached,
            rotation,
            support,
            &mut cached_trace,
            true,
            true,
            &mut cached_work,
        );
        let uncached_result = fit_radial_faces_impl(
            &mut uncached,
            rotation,
            support,
            &mut uncached_trace,
            keep_face_cache,
            false,
            &mut uncached_work,
        );
        assert_eq!(
            cached_result
                .as_ref()
                .map_err(|e| (e.row, e.column, e.reason, e.detail)),
            uncached_result
                .as_ref()
                .map_err(|e| (e.row, e.column, e.reason, e.detail)),
            "memoization must preserve success, failure location and diagnostics"
        );
        let bits = |points: &[Vec3]| {
            points
                .iter()
                .map(|p| p.to_array().map(f32::to_bits))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            bits(&cached),
            bits(&uncached),
            "every fitted vertex must be bit-for-bit unchanged"
        );
        assert_eq!(
            cached_trace, uncached_trace,
            "face order, tie winner and final-round witness must agree"
        );
        assert_eq!(cached_work.rounds, uncached_work.rounds);
        if keep_face_cache {
            assert_eq!(cached_work.hits, uncached_work.hits);
            assert_eq!(cached_work.queries, uncached_work.queries);
            assert_eq!(cached_work.clips, uncached_work.clips);
            assert_eq!(cached_work.projections, uncached_work.projections);
        } else {
            assert_eq!(uncached_work.hits, 0);
            assert_eq!(
                cached_work.queries + cached_work.hits,
                uncached_work.queries
            );
        }
        assert_eq!(uncached_work.membership_hits, 0);
        assert_eq!(
            cached_work.membership_calls + cached_work.membership_hits,
            uncached_work.membership_calls
        );
        assert!(cached_work.clips <= uncached_work.clips);
        assert!(cached_work.projections <= uncached_work.projections);
        if cached_result.is_err() {
            assert_eq!(cached, reference, "failed solves remain transactional");
            assert!(cached_trace.is_none());
        }
        eprintln!("face cache work: cached={cached_work:?}, uncached={uncached_work:?}");
        (cached_work, uncached_work, cached_result.is_ok())
    }

    #[test]
    fn proposal_membership_cache_reuses_exact_results_and_rechecks_changed_bits() {
        let arms = LayerSurface::from_triangles(
            rounded_cap()
                .into_iter()
                .map(|tri| tri.map(|p| p + Vec3::Z * 0.5))
                .collect(),
        );
        let mut entry = None;
        let mut work = FaceFitWork::default();
        let first = Vec3::new(0.0, 0.0, 0.2);
        let changed = Vec3::new(f32::from_bits(1), 0.0, 0.2);
        for proposed in [first, first, changed, changed] {
            let expected = arms.outside_point(proposed, Vec3::Z);
            let actual =
                memoized_outside_point(&arms, proposed, Vec3::Z, &mut entry, true, &mut work);
            let bits = |p: Option<Vec3>| p.map(|p| p.to_array().map(f32::to_bits));
            assert_eq!(bits(actual), bits(expected));
        }
        assert_eq!(work.membership_calls, 2);
        assert_eq!(work.membership_hits, 2);
        assert_eq!(
            work.membership_changes, 1,
            "even a one-bit coordinate change must query again"
        );
        // Different vertices have different slots even at identical XYZ; their
        // axes are fixed independently, never part of a global position cache.
        let inside = Vec3::Z * 0.5;
        let mut entries = [None; 2];
        for (slot, axis) in entries.iter_mut().zip([Vec3::X, Vec3::Z]) {
            assert_eq!(
                memoized_outside_point(&arms, inside, axis, slot, true, &mut work),
                arms.outside_point(inside, axis),
            );
        }
        assert_eq!(work.membership_calls, 4);
        assert_ne!(entries[0].unwrap().outside, entries[1].unwrap().outside);
    }

    #[test]
    fn head_clearance_initialization_distinguishes_near_and_remote_geometry() {
        let head = LayerSurface::from_triangles(rounded_cap());
        let near = Vec3::Z * 0.115;
        let fitted = head
            .clearance_point(near, Vec3::Z, LAYER_GAP + THICKNESS)
            .unwrap();
        assert!(fitted.z >= 0.132 - 1e-6 && fitted.z < 0.134);
        assert!(rendered_clearance_checked(&head, fitted, true) >= 0.0099);
        let remote = LayerSurface::from_triangles(
            rounded_cap()
                .into_iter()
                .map(|tri| tri.map(|p| p + Vec3::Z * 2.0))
                .collect(),
        );
        assert_eq!(
            remote.clearance_point(Vec3::ZERO, Vec3::Z, LAYER_GAP + THICKNESS),
            Some(Vec3::ZERO)
        );
        let sample = [
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        assert!(
            (triangle_distance_squared(Vec3::new(0.0, 0.2, 0.03), sample) - 0.0009).abs() < 1e-7
        );
        assert!((triangle_distance_squared(Vec3::new(0.0, -0.2, 0.0), sample) - 0.04).abs() < 1e-7);
    }

    #[test]
    fn joint_obstacle_exit_and_cache_finish_outside_head_and_arms() {
        let a = LayerSurface::from_triangles(rounded_cap());
        let b = LayerSurface::from_triangles(
            rounded_cap()
                .into_iter()
                .map(|tri| tri.map(|p| p + Vec3::Z * 0.18))
                .collect(),
        );
        for (arms, head) in [(&a, &b), (&b, &a)] {
            let escaped = outside_obstacles(Some(arms), Some(head), Vec3::ZERO, Vec3::Z).unwrap();
            assert!(escaped.z >= 0.312 - 1e-6);
            assert!(
                arms.containing_components(escaped).is_empty()
                    && head.containing_components(escaped).is_empty()
            );
            let mut entry = None;
            let mut work = FaceFitWork::default();
            for _ in 0..2 {
                assert_eq!(
                    memoized_outside_obstacles(
                        Some(arms),
                        Some(head),
                        Vec3::ZERO,
                        Vec3::Z,
                        &mut entry,
                        true,
                        &mut work
                    ),
                    Some(escaped)
                );
            }
            assert_eq!(work.membership_calls, 1);
            assert_eq!(work.membership_hits, 1);
        }
    }

    #[test]
    fn head_only_obstacle_uses_finite_domains_and_exact_cache() {
        let reference = radial_test_grid();
        let at = reference[index(8, 0)] + Vec3::new(0.0, 0.53, -0.15);
        let head = LayerSurface::from_triangles(vec![[
            at + Vec3::new(-0.03, -0.03, 0.015),
            at + Vec3::new(0.03, -0.03, 0.015),
            at + Vec3::new(0.0, 0.03, 0.015),
        ]]);
        let support = FaceSupport {
            inner: None,
            arms: None,
            head: Some(&head),
            profile: None,
            radial_limits: None,
        };
        let (_, _, success) = assert_face_cache_equivalent(&reference, Quat::IDENTITY, support);
        assert!(success);
        let mut points = reference;
        let mut trace = None;
        fit_radial_supports_traced(&mut points, Quat::IDENTITY, support, &mut trace).unwrap();
        assert_eq!(trace.unwrap()["source"], "head");
    }

    #[test]
    fn proposal_membership_cache_preserves_failed_bounded_exits() {
        let part = super::super::visual_contact::PosedTriangles::new(
            Arc::new(rounded_cap()),
            Affine3A::IDENTITY,
        );
        // Each exit enters the next overlapping component. Thirteen solids
        // exceed the production twelve-exit budget without changing that gate.
        let snapshot = ContactSurface::new(
            (0..13)
                .map(|i| {
                    let mut part = part.clone();
                    part.transform = Affine3A::from_translation(Vec3::Z * (i as f32 * 0.15));
                    part
                })
                .collect(),
        );
        let arms = snapshot.surface();
        assert!(arms.outside_point(Vec3::ZERO, Vec3::Z).is_none());
        let mut entry = None;
        let mut work = FaceFitWork::default();
        for _ in 0..2 {
            assert!(memoized_outside_point(
                &arms,
                Vec3::ZERO,
                Vec3::Z,
                &mut entry,
                true,
                &mut work
            )
            .is_none());
        }
        assert_eq!(work.membership_calls, 1);
        assert_eq!(
            work.membership_hits, 1,
            "None is a cached result, not an unqueried entry"
        );
        let clear = Vec3::X * 2.0;
        assert_eq!(
            memoized_outside_point(&arms, clear, Vec3::Z, &mut entry, true, &mut work),
            Some(clear)
        );
        assert_eq!(work.membership_calls, 2);
        assert_eq!(work.membership_changes, 1);
    }

    #[test]
    fn proposal_membership_cache_is_equivalent_with_fixed_and_growing_face_domains() {
        let reference = radial_test_grid();
        let triangle = |z| {
            [
                Vec3::new(-0.02, -0.15, z),
                Vec3::new(0.02, -0.15, z),
                Vec3::new(0.0, -0.11, z),
            ]
        };
        let inner = LayerSurface::from_triangles(vec![triangle(0.45)]);
        let remote = LayerSurface::from_triangles(
            rounded_cap()
                .into_iter()
                .map(|tri| tri.map(|p| p + Vec3::X * 2.0))
                .collect(),
        );
        let (cached, uncached, success) = assert_cache_equivalent(
            &reference,
            Quat::IDENTITY,
            FaceSupport {
                inner: Some(&inner),
                arms: Some(&remote),
                head: None,
                profile: None,
                radial_limits: None,
            },
            true,
        );
        assert!(success && cached.rounds >= 2);
        assert!(cached.membership_hits > 0);
        assert!(
            cached.membership_calls < uncached.membership_calls,
            "identical proposals across budget rounds must avoid repeated multi-ray membership"
        );
        let reached = LayerSurface::from_triangles(vec![triangle(0.505)]);
        let (cached, _, success) = assert_cache_equivalent(
            &reference,
            Quat::IDENTITY,
            FaceSupport {
                inner: Some(&inner),
                arms: Some(&reached),
                head: None,
                profile: None,
                radial_limits: None,
            },
            true,
        );
        assert!(success && cached.rounds >= 3);
        assert!(
            cached.invalidations > 0,
            "the existing arm-face cache still follows changing domains"
        );
        assert!(
            cached.membership_changes > 0,
            "changed proposals must not reuse an old outside answer"
        );
    }

    #[test]
    fn arm_membership_handles_opposite_overlaps_open_sheets_and_grazing_rays() {
        let first = rounded_cap();
        let second: Vec<_> = first
            .iter()
            .map(|tri| {
                let [a, b, c] = tri.map(|p| p + Vec3::X * 0.03);
                [a, c, b]
            })
            .collect();
        let posed = ContactSurface::new(vec![
            super::super::visual_contact::PosedTriangles::new(
                Arc::new(first.clone()),
                Affine3A::IDENTITY,
            ),
            super::super::visual_contact::PosedTriangles::new(
                Arc::new(second.clone()),
                Affine3A::IDENTITY,
            ),
        ]);
        assert_eq!(
            posed.surface().containing_components(Vec3::X * 0.015).len(),
            2,
            "per-part component labels must remain distinct after snapshot merging"
        );
        let mut triangles = first.clone();
        triangles.extend(second);
        triangles.push([
            Vec3::new(0.5, -0.2, -0.2),
            Vec3::new(0.5, 0.2, -0.2),
            Vec3::new(0.5, 0.0, 0.2),
        ]);
        let arms = LayerSurface::from_triangles(triangles);
        assert_eq!(
            arms.containing_components(Vec3::X * 0.015).len(),
            2,
            "oppositely wound overlapping solids must not cancel union membership"
        );
        assert!(
            arms.containing_components(Vec3::new(0.48, 0.0, 0.0))
                .is_empty(),
            "an open sheet ahead of a clear point must not invent a solid interior"
        );
        let escaped = arms.outside_point(Vec3::X * 0.015, Vec3::X).unwrap();
        assert!(rendered_clearance_checked(&arms, escaped, true) > 0.001);
        let sphere = LayerSurface::from_triangles(first);
        let ray = Vec3::new(1.0, 0.371, 0.173).normalize();
        // Aim exactly through emitted vertices and edge midpoints, rather
        // than relying only on generic rays away from triangulation seams.
        for tri in sphere.triangles.iter().step_by(37) {
            for target in [tri[0], (tri[0] + tri[1]) * 0.5] {
                let point = target - ray * 0.4;
                for _ in 0..3 {
                    assert!(sphere.containing_components(point).is_empty(),
                        "shared-edge/vertex ray crossings must not classify an exterior point as inside: {point:?}");
                }
            }
        }
    }

    #[test]
    fn arm_membership_preserves_clear_ray_hits_and_exits_embedded_low_hands() {
        let center = Vec3::new(0.0, -0.13, 0.50);
        let arms = LayerSurface::from_triangles(
            rounded_cap()
                .into_iter()
                .map(|tri| tri.map(|p| p * 0.4 + center))
                .collect(),
        );
        let clear = Vec3::new(0.0, -0.13, 0.39);
        assert!(
            arms.ray(Vec3::Y * clear.y, Vec3::Z).unwrap() > clear.z,
            "the distant hand really intersects the outward ray"
        );
        assert!(arms.containing_components(clear).is_empty());
        assert_eq!(
            fit_mesh_contact(clear, false, &arms),
            clear,
            "already-outside cloth must not enclose a distant hand"
        );
        assert!(!arms.containing_components(center).is_empty());
        let outside = fit_mesh_contact(center, false, &arms);
        assert!(outside.y < -0.12 && (outside.y - center.y).abs() < 1e-6);
        assert!(
            rendered_clearance_checked(&arms, outside, true) > 0.001,
            "inside initialization covers low hands without the old height cutoff"
        );
        let sheet = LayerSurface::from_triangles(vec![[
            Vec3::new(-0.02, -0.15, 0.45),
            Vec3::new(0.02, -0.15, 0.45),
            Vec3::new(0.0, -0.11, 0.45),
        ]]);
        let mut points = radial_test_grid();
        let offset = Vec3::new(0.0, 0.53, -0.15);
        seat_inner_layer(&mut points, Quat::IDENTITY, &sheet).unwrap();
        assert!(
            !arms
                .containing_components(points[index(ROWS, 0)] + offset)
                .is_empty(),
            "inner seating must actually enter the separate arm component in this fixture"
        );
        seat_arm_obstacles(&mut points, Quat::IDENTITY, &arms);
        assert!(
            rendered_clearance_checked(&arms, points[index(ROWS, 0)] + offset, true) > 0.001,
            "actual-arm outside membership must be restored after inner seating"
        );
    }

    #[test]
    fn distant_arm_obstacles_do_not_inherit_inner_cloth_containment() {
        let reference = radial_test_grid();
        let remote = LayerSurface::from_triangles(vec![[
            Vec3::new(-0.02, -0.15, 0.70),
            Vec3::new(0.02, -0.15, 0.70),
            Vec3::new(0.0, -0.11, 0.70),
        ]]);
        let mut obstacle = reference.clone();
        fit_radial_supports_traced(
            &mut obstacle,
            Quat::IDENTITY,
            FaceSupport::arms(&remote),
            &mut None,
        )
        .unwrap();
        assert!(
            obstacle
                .iter()
                .zip(&reference)
                .all(|(a, b)| a.distance(*b) < 1e-6),
            "a remote obstacle beyond the complete face sweep must not pull the cape outward"
        );
        let mut containing = reference.clone();
        fit_radial_faces(&mut containing, Quat::IDENTITY, &remote).unwrap();
        assert!(
            containing[index(ROWS, 0)].z > reference[index(ROWS, 0)].z + 0.25,
            "retained inner cloth still enforces outermost containment across missed vertex rays"
        );
    }

    #[test]
    fn finite_arm_entry_wall_cannot_leave_proposed_vertices_inside_the_solid() {
        let vertices = [
            Vec3::new(-0.10, -0.16, 0.41),
            Vec3::new(0.10, -0.16, 0.41),
            Vec3::new(0.10, -0.10, 0.41),
            Vec3::new(-0.10, -0.10, 0.41),
            Vec3::new(-0.10, -0.16, 0.55),
            Vec3::new(0.10, -0.16, 0.55),
            Vec3::new(0.10, -0.10, 0.55),
            Vec3::new(-0.10, -0.10, 0.55),
        ];
        let faces = [
            [0, 3, 2],
            [0, 2, 1],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [3, 7, 6],
            [3, 6, 2],
            [0, 4, 7],
            [0, 7, 3],
            [1, 2, 6],
            [1, 6, 5],
        ];
        let arms = LayerSurface::from_triangles(
            faces
                .into_iter()
                .map(|face| face.map(|i| vertices[i]))
                .collect(),
        );
        let reference = radial_test_grid();
        let offset = Vec3::new(0.0, 0.53, -0.15);
        let hem = reference[index(ROWS, 0)] + offset;
        assert!(arms.containing_components(hem).is_empty());
        let entry_only = Vec3::new(0.0, hem.y, 0.41 + LAYER_GAP + THICKNESS);
        assert!(
            rendered_clearance_checked(&arms, entry_only, true) < 0.0,
            "separating from the entry wall alone enters this thick closed arm"
        );
        assert!(arms.outside_point(entry_only, Vec3::Z).unwrap().z >= 0.582 - 1e-6);
        let support = FaceSupport::arms(&arms);
        let (cached, _, success) =
            assert_face_cache_equivalent(&reference, Quat::IDENTITY, support);
        assert!(success && cached.rounds >= 2 && cached.invalidations > 0);
        let mut fitted = reference;
        fit_radial_supports_traced(&mut fitted, Quat::IDENTITY, support, &mut None).unwrap();
        assert!((fitted[index(ROWS, 0)] + offset).z >= 0.582 - 1e-6);
        for point in fitted.iter().map(|p| *p + offset) {
            assert!(rendered_clearance_checked(&arms, point, true) > 0.001,
                "final vertices must remain outside the solid, not merely beyond its entry face: {point:?}");
        }
        for row in ROWS - 2..ROWS {
            for column in 0..COLUMNS {
                let [a, b, c, d] = [
                    index(row, column),
                    index(row, column + 1),
                    index(row + 1, column),
                    index(row + 1, column + 1),
                ];
                for face in [[a, b, c], [b, d, c]] {
                    let [a, b, c] = face.map(|i| fitted[i] + offset);
                    let normal = (b - a).cross(c - a).normalize_or_zero();
                    for weights in [
                        [1.0, 1.0, 1.0],
                        [2.0, 1.0, 1.0],
                        [1.0, 2.0, 1.0],
                        [1.0, 1.0, 2.0],
                    ] {
                        let at = (a * weights[0] + b * weights[1] + c * weights[2])
                            / weights.into_iter().sum::<f32>();
                        for sign in [-1.0, 1.0] {
                            let lining = at + normal * sign * THICKNESS * 0.5;
                            assert!(rendered_clearance_checked(&arms, lining, true) > 0.001,
                                "finite obstacle fitting must clear complete emitted faces and lining: {lining:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn arm_domain_growth_invalidates_cache_and_discovers_new_contact() {
        let reference = radial_test_grid();
        let triangle = |z| {
            [
                Vec3::new(-0.02, -0.15, z),
                Vec3::new(0.02, -0.15, z),
                Vec3::new(0.0, -0.11, z),
            ]
        };
        let inner = LayerSurface::from_triangles(vec![triangle(0.45)]);
        let arms = LayerSurface::from_triangles(vec![triangle(0.505)]);
        let support = FaceSupport {
            inner: Some(&inner),
            arms: Some(&arms),
            head: None,
            profile: None,
            radial_limits: None,
        };
        let (cached, uncached, success) =
            assert_face_cache_equivalent(&reference, Quat::IDENTITY, support);
        assert!(success && cached.rounds >= 3);
        assert!(
            cached.invalidations > 0,
            "radial growth must discard the earlier no-arm-contact result"
        );
        assert!(
            cached.hits > 0 && cached.queries < uncached.queries,
            "unchanged face domains still reuse exact results"
        );
        let mut fitted = reference.clone();
        let mut trace = None;
        fit_radial_supports_traced(&mut fitted, Quat::IDENTITY, support, &mut trace).unwrap();
        let trace = trace.expect("growing arm domain has a controlling face");
        for field in [
            "face_reference",
            "face_axes",
            "face_budgets",
            "face_budget_endpoints",
            "face_final",
        ] {
            assert_eq!(trace[field].as_array().unwrap().len(), 3);
        }
        for vertex in 0..3 {
            let budget = trace["face_budgets"][vertex].as_f64().unwrap();
            let start = &trace["face_reference"][vertex];
            let direction = &trace["face_axes"][vertex];
            let end = &trace["face_budget_endpoints"][vertex];
            for coordinate in 0..3 {
                let reference = start[coordinate].as_f64().unwrap();
                let axis = direction[coordinate].as_f64().unwrap();
                let endpoint = end[coordinate].as_f64().unwrap();
                assert!((reference + axis * budget - endpoint).abs() < 1e-6);
            }
        }
        let offset = Vec3::new(0.0, 0.53, -0.15);
        assert!(
            (fitted[index(ROWS, 0)] + offset).z >= 0.505 + LAYER_GAP + THICKNESS - 1e-6,
            "newly reached arm face must receive complete separation"
        );
    }

    #[test]
    fn face_cache_reuses_hits_and_misses_across_vertical_and_radial_growth() {
        let reference = radial_test_grid();
        let anchor = reference[0] + Vec3::new(0.0, 0.53, -0.15);
        let lower = [
            anchor - Vec3::X * 0.01,
            anchor + Vec3::X * 0.01,
            anchor + Vec3::Y * 0.01,
        ];
        let upper = lower.map(|p| p + Vec3::Y * 0.02);
        // Retain the independent collar-growth fixture, and force a hanging
        // radial hit as well. Most other hanging faces are explicit misses.
        let hem = [
            Vec3::new(-0.02, -0.15, 0.45),
            Vec3::new(0.02, -0.15, 0.45),
            Vec3::new(0.0, -0.11, 0.45),
        ];
        let surface = LayerSurface::from_triangles(vec![lower, upper, hem]);
        let (cached, uncached, success) =
            assert_face_cache_equivalent(&reference, Quat::IDENTITY, FaceSupport::inner(&surface));
        assert!(success);
        assert!(
            cached.rounds >= 3,
            "must requery newly reached collar support"
        );
        assert_eq!(
            cached.hits,
            (cached.rounds - 1) * (ROWS - ROOF_ROWS) * COLUMNS * 2
        );
        assert!(cached.empty_hits > 0 && cached.empty_hits < cached.hits);
        assert!(cached.clips < uncached.clips);
        assert!(cached.projections < uncached.projections);
    }

    #[test]
    fn face_cache_preserves_empty_support_and_failed_solve_rollback() {
        let reference = radial_test_grid();
        let (cached, uncached, success) = assert_face_cache_equivalent(
            &reference,
            Quat::IDENTITY,
            FaceSupport::inner(&LayerSurface::from_triangles(Vec::new())),
        );
        assert!(success);
        assert_eq!(cached.rounds, 1);
        assert_eq!(cached.queries, uncached.queries);
        assert_eq!(cached.clips, 0);
        let anchor = reference[0] + Vec3::new(0.0, 0.53, -0.15);
        let barrier = LayerSurface::from_triangles(vec![[
            anchor - Vec3::X * 0.01,
            anchor + Vec3::X * 0.01,
            anchor + Vec3::Y * 0.2,
        ]]);
        let (_, _, success) =
            assert_face_cache_equivalent(&reference, Quat::IDENTITY, FaceSupport::inner(&barrier));
        assert!(
            !success,
            "must preserve the existing impossible collar failure"
        );
    }

    #[test]
    fn face_cache_matches_actual_neutral_and_tick_93_strike() {
        let mut equipment = Equipment::default();
        equipment.select("hooded-cloak").unwrap();
        let look = Look::with_choice(CreatureModel::Human, 18, Some(Variant::default()))
            .equipped(equipment);
        let (mut world, mut rig, _, mut meshes) = super::super::rogue_tests::spawn_one(&look);
        place_layers(&mut rig.cloaks, |_| Some(Quat::IDENTITY), &mut meshes);
        for cloak in &rig.cloaks {
            let (cached, uncached, success) = assert_face_cache_equivalent(
                &cloak.contact_reference,
                Quat::IDENTITY,
                FaceSupport::arms(cloak.arm_surface.as_ref().unwrap())
                    .with_profile(&cloak.shoulder_profile),
            );
            assert!(success);
            // An already clear pose may finish in one round; the synthetic
            // growth fixture independently requires strict clip/work savings.
            if cached.rounds > 1 {
                assert!(cached.hits > 0 && cached.queries < uncached.queries);
            }
        }
        world.insert_resource(meshes);
        let mut anim = Anim::new(18, Vec3::ZERO);
        anim.yaw = 0.0;
        anim.time = 93.0 / 50.0;
        let phase = (85..93).fold(0.0, |time, _| time + 0.02 / Move::Strike.duration());
        anim.action = Some((Move::Strike, phase));
        let actor = world
            .spawn((rig, anim, Transform::default(), super::super::PoseRefresh))
            .id();
        let mut schedule = Schedule::default();
        schedule.add_systems(super::super::refresh_pose);
        schedule.run(&mut world);
        let rig = world.get::<Rig>(actor).unwrap();
        for cloak in &rig.cloaks {
            let (cached, uncached, success) = assert_face_cache_equivalent(
                &cloak.contact_reference,
                cloak.last_rotation.unwrap(),
                FaceSupport::arms(cloak.arm_surface.as_ref().unwrap())
                    .with_profile(&cloak.shoulder_profile),
            );
            assert!(success);
            // An already clear pose may finish in one round; the synthetic
            // growth fixture independently requires strict clip/work savings.
            if cached.rounds > 1 {
                assert!(cached.hits > 0 && cached.queries < uncached.queries);
            }
        }
    }

    fn test_shoulder_profile(points: &[Vec3]) -> Vec<Vec2> {
        (0..GRID)
            .map(|i| {
                let row = i / (COLUMNS + 1);
                let col = i % (COLUMNS + 1);
                shoulder_coefficient(
                    row,
                    points[index(row.saturating_sub(1), col)],
                    points[index((row + 1).min(ROWS), col)],
                )
            })
            .collect()
    }

    #[test]
    fn mixed_row_sweep_requeries_high_growth_even_when_radial_extent_is_unchanged() {
        let grid = radial_test_grid();
        let profile = test_shoulder_profile(&grid);
        let vertices = [
            Vec3::new(-0.01, 0.508, 0.30),
            Vec3::new(0.01, 0.508, 0.30),
            Vec3::new(0.0, 0.47, 0.45),
        ];
        let axis = profile_direction(vertices[0], profile[index(5, 0)]);
        let endpoints = [vertices[0] + axis * 0.04, vertices[1], vertices[2]];
        let initial = swept_face_bounds(vertices, vertices, Vec3::Z);
        let grown = swept_face_bounds(vertices, endpoints, Vec3::Z);
        assert_eq!(initial.2, grown.2);
        assert!(grown.1 > initial.1 + 0.02);
        assert_ne!(
            face_domain_key(initial.1, initial.2, true),
            face_domain_key(grown.1, grown.2, true)
        );
        assert_ne!(
            face_domain_key(initial.1, initial.2, false),
            face_domain_key(grown.1, grown.2, false)
        );
        for bits in 0..8 {
            let points = std::array::from_fn::<_, 3, _>(|i| {
                if bits & (1 << i) == 0 {
                    vertices[i]
                } else {
                    endpoints[i]
                }
            });
            for point in points
                .into_iter()
                .chain([points.iter().sum::<Vec3>() / 3.0])
            {
                assert!(point.y >= grown.0 && point.y <= grown.1 && point.z <= grown.2);
            }
        }
        let obstacle = [
            Vec3::new(-0.005, 0.528, 0.31),
            Vec3::new(0.005, 0.528, 0.31),
            Vec3::new(0.0, 0.528, 0.32),
        ];
        assert!(clipped_support(obstacle, Vec3::Y, &[(Vec3::NEG_Y, -initial.1)]).is_none());
        assert!(clipped_support(obstacle, Vec3::Y, &[(Vec3::NEG_Y, -grown.1)]).is_some());
    }

    #[test]
    fn smooth_profile_preserves_turnover_body_reserve() {
        let grid = radial_test_grid();
        let profile = test_shoulder_profile(&grid);
        let at = Vec3::new(0.0, 0.47, 0.10);
        let axis = profile_direction(at, profile[index(YOKE_ROWS, 0)]);
        assert_eq!(axis, Vec3::Z);
        let plane = |p: Vec3| p.z - 0.20;
        let fitted = fit_profile_body(at, axis, &plane);
        assert!((fitted.y - at.y).abs() < 1e-6);
        assert!(
            plane(fitted / anatomy::HIP_SCALE) > 0.0275,
            "radial turnover must retain its .030 body reserve, not the .025 roof reserve"
        );
    }

    #[test]
    fn measured_neutral_mixed_plane_has_well_conditioned_profile_motion() {
        let grid = radial_test_grid();
        let coefficient = test_shoulder_profile(&grid)[index(5, 3)];
        let reference = Vec3::new(-0.26514566, 0.5083333, 0.07850698);
        let normal = Vec3::new(-0.42936885, 0.8944272, 0.12506948);
        let witness = Vec3::new(-0.28437114, 0.5107909, 0.039386477);
        let plane = 0.6158914;
        assert!((normal.dot(witness) + LAYER_GAP + THICKNESS - plane).abs() < 1e-6);
        let radial = reference.with_y(0.0).normalize();
        let deficit = plane - normal.dot(reference);
        let old_travel = deficit / normal.dot(radial);
        assert!((old_travel - 0.083987765).abs() < 1e-5);
        let axis = profile_direction(reference, coefficient);
        assert!(axis.y > 0.0 && axis.with_y(0.0).length() > 0.0);
        let travel = deficit / normal.dot(axis);
        let fitted = reference + axis * travel;
        assert!(normal.dot(fitted) >= plane - 1e-6);
        assert!(
            travel < old_travel * 0.6,
            "the actual unchanged plane margin must not require the measured radial fin"
        );
        assert!(
            fitted.y < 0.565 && fitted.with_y(0.0).length() < reference.with_y(0.0).length() + 0.04
        );
    }

    #[test]
    fn row_five_seating_and_certificate_share_the_frozen_profile_axis() {
        let mut points = radial_test_grid();
        let profile = test_shoulder_profile(&points);
        let offset = Vec3::new(0.0, 0.53, -0.15);
        let i = index(5, 0);
        let before = points[i] + offset;
        let axis = profile_direction(before, profile[i]);
        let inner = LayerSurface::from_triangles(vec![
            [
                Vec3::new(-0.1, 0.53, 0.20),
                Vec3::new(0.1, 0.53, 0.20),
                Vec3::new(0.1, 0.53, 0.50),
            ],
            [
                Vec3::new(-0.1, 0.53, 0.20),
                Vec3::new(0.1, 0.53, 0.50),
                Vec3::new(-0.1, 0.53, 0.50),
            ],
        ]);
        seat_inner_profile(&mut points, Quat::IDENTITY, &inner, Some(&profile)).unwrap();
        let seated = points[i] + offset;
        let displacement = seated - before;
        assert!(displacement.y > 0.0 && displacement.z > 0.0);
        assert!(displacement.cross(axis).length() < 1e-6);
        assert_eq!(profile[0], Vec2::Y);
        assert_eq!(profile[index(YOKE_ROWS, 0)], Vec2::X);
        let support = FaceSupport::inner(&inner).with_profile(&profile);
        let (cached, _, success) = assert_cache_equivalent(&points, Quat::IDENTITY, support, true);
        assert!(success && cached.rounds >= 1);
        let frozen = profile.clone();
        fit_radial_supports_traced(&mut points, Quat::IDENTITY, support, &mut None).unwrap();
        assert_eq!(profile, frozen);
        let final_point = points[i] + offset;
        assert!((final_point - seated).cross(axis).length() < 1e-6);
        assert!(rendered_clearance_checked(&inner, final_point, true) > 0.001);
        assert!((points[index(YOKE_ROWS, 0)] + offset).y == 0.47);
    }

    #[test]
    fn captured_strike_roof_plane_uses_vertical_separation_without_radial_escape() {
        // Exact tick93 first-red witness from fead943f, vertex63/face[14,63,62].
        let reference = Vec3::new(-0.15152939, 0.57166666, -0.116466805);
        let normal = Vec3::new(-0.1049089, 0.99227786, -0.06617203);
        let witness = Vec3::new(-0.30472833, 0.58165556, -0.13908452);
        let plane = 0.65033615;
        assert!((normal.dot(witness) + LAYER_GAP + THICKNESS - plane).abs() < 1e-6);
        let radial = reference.with_y(0.0).normalize();
        let deficit = plane - normal.dot(reference);
        assert!(
            deficit / normal.dot(radial) > 0.48,
            "fixture must reproduce the measured half-metre radial wing"
        );
        let profile = test_shoulder_profile(&radial_test_grid());
        let axis = profile_direction(reference, profile[63]);
        assert_eq!(axis, Vec3::Y);
        let fitted = reference + axis * (deficit / normal.dot(axis));
        assert!(normal.dot(fitted) >= plane - 1e-6);
        assert!(fitted.with_y(0.0).distance(reference.with_y(0.0)) < 1e-6);
        assert!(
            fitted.y - reference.y < 0.061 && fitted.y < 0.649,
            "the exact plane has bounded vertical clearance, not a radial fin"
        );
        assert_eq!(
            profile_direction(reference, profile[index(YOKE_ROWS, 0)]),
            radial,
            "turnover and hanging vertices retain their radial coordinate"
        );
    }

    #[test]
    fn collar_domain_ignores_an_overhead_obstacle_outside_actual_travel() {
        let offset = Vec3::new(0.0, 0.53, -0.15);
        let mut points = radial_test_grid();
        let before = points.clone();
        let anchor = points[0] + offset;
        let overhead = LayerSurface::from_triangles(vec![[
            anchor + Vec3::new(-0.01, 0.07, 0.0),
            anchor + Vec3::new(0.01, 0.07, 0.0),
            anchor + Vec3::new(0.0, 0.08, 0.0),
        ]]);
        let mut trace = None;
        fit_radial_faces_traced(&mut points, Quat::IDENTITY, &overhead, &mut trace)
            .expect("an unreachable overhead obstacle must not consume collar clearance");
        assert!(points
            .iter()
            .zip(&before)
            .all(|(a, b)| a.distance(*b) < 1e-6));
        assert!(
            trace.is_none(),
            "no radial constraint is needed outside the actual swept domain"
        );
    }

    #[test]
    fn collar_growth_requeries_an_obstacle_above_the_initial_face_domain() {
        let offset = Vec3::new(0.0, 0.53, -0.15);
        let reference = radial_test_grid();
        let anchor = reference[0] + offset;
        let lower = [
            anchor - Vec3::X * 0.01,
            anchor + Vec3::X * 0.01,
            anchor + Vec3::Y * 0.01,
        ];
        let upper = lower.map(|p| p + Vec3::Y * 0.02);
        assert!(upper
            .iter()
            .all(|p| p.y > reference[index(1, 0)].y + offset.y + THICKNESS));
        let mut low_only = reference.clone();
        fit_radial_faces(
            &mut low_only,
            Quat::IDENTITY,
            &LayerSurface::from_triangles(vec![lower]),
        )
        .expect("lower obstacle can be cleared inside the existing ceiling");
        let support = LayerSurface::from_triangles(vec![lower, upper]);
        let mut points = reference.clone();
        let mut trace = None;
        let mut work = FaceFitWork::default();
        fit_radial_faces_impl(
            &mut points,
            Quat::IDENTITY,
            FaceSupport::inner(&support),
            &mut trace,
            true,
            true,
            &mut work,
        )
        .expect("a newly reached upper obstacle must be included on the next solve round");
        assert!(points[0].y > low_only[0].y + 0.015);
        assert!(
            work.rounds >= 2,
            "newly reached support requires another round even without radial escape"
        );
        let ceiling = 0.565 + 2.0 * (LAYER_GAP + THICKNESS + ROOF_SLOPE_SLACK);
        for (p, original) in points[..=COLUMNS].iter().zip(&reference) {
            assert!(p.with_y(0.0).distance(original.with_y(0.0)) < 1e-6);
            assert!(p.y + offset.y <= ceiling);
        }
        // Independently sample the actual emitted exterior and lining, not
        // the BVH or the solver's clipped support projections.
        let mut normals = vec![Vec3::ZERO; GRID];
        surface_normals(&points, &mut normals);
        for col in 0..COLUMNS {
            let [a, b, c, d] = [
                index(0, col),
                index(0, col + 1),
                index(1, col),
                index(1, col + 1),
            ];
            for face in [[a, b, c], [b, d, c]] {
                for side in [-1.0, 1.0] {
                    let emitted =
                        face.map(|i| points[i] + offset + normals[i] * (side * THICKNESS * 0.5));
                    for weights in [
                        [1.0 / 3.0; 3],
                        [0.6, 0.2, 0.2],
                        [0.2, 0.6, 0.2],
                        [0.2, 0.2, 0.6],
                    ] {
                        let p = emitted
                            .into_iter()
                            .zip(weights)
                            .map(|(p, w)| p * w)
                            .sum::<Vec3>();
                        assert!(
                            rendered_clearance_checked(&support, p, true) > 0.001,
                            "newly reached upper obstacle clips emitted collar lining at {p:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn inner_point_ray_overshoot_defers_to_bounded_joint_collar_clearance() {
        let offset = Vec3::new(0.0, 0.53, -0.15);
        let mut points = radial_test_grid();
        let reference = points.clone();
        // A small fold above the front attachment is within its infinite
        // point ray, but outside every final finite face near that opening.
        let a = reference[0] + offset + Vec3::Y * 0.062;
        let panel = [
            a + Vec3::new(-0.005, 0.0, -0.003),
            a + Vec3::new(0.005, 0.0, -0.003),
            a + Vec3::new(0.005, 0.0, 0.003),
            a + Vec3::new(-0.005, 0.0, 0.003),
        ];
        // A separate rear obstacle genuinely requires joint collar lift.
        // Thus success cannot be obtained by simply ignoring collar contact.
        let rear = reference[COLUMNS / 2] + offset;
        let support = LayerSurface::from_triangles(vec![
            [panel[0], panel[1], panel[2]],
            [panel[0], panel[2], panel[3]],
            [
                rear - Vec3::X * 0.01,
                rear + Vec3::X * 0.01,
                rear + Vec3::Y * 0.01,
            ],
        ]);
        let ceiling = 0.565 + 2.0 * (LAYER_GAP + THICKNESS + ROOF_SLOPE_SLACK);
        let raw = seat_outermost_surface(reference[0] + offset, true, &support);
        assert!(
            raw.y > ceiling,
            "fixture must retain the raw-ray ceiling overshoot"
        );
        seat_inner_layer(&mut points, Quat::IDENTITY, &support)
            .expect("point initialization must leave collar feasibility to complete faces");
        assert_eq!(&points[..=COLUMNS], &reference[..=COLUMNS]);
        fit_radial_faces(&mut points, Quat::IDENTITY, &support)
            .expect("finite joint clearance fits without raising the absolute ceiling");
        assert!(points[COLUMNS / 2].y > reference[COLUMNS / 2].y + 0.02);
        assert!(points[0].distance(reference[0]) < 1e-6);
        for (p, before) in points[..=COLUMNS].iter().zip(&reference) {
            assert!(p.y + offset.y <= ceiling);
            assert!(p.with_y(0.0).distance(before.with_y(0.0)) < 1e-6);
        }
        let mut normals = vec![Vec3::ZERO; GRID];
        surface_normals(&points, &mut normals);
        for row in 0..ROWS {
            for col in 0..COLUMNS {
                let [a, b, c, d] = [
                    index(row, col),
                    index(row, col + 1),
                    index(row + 1, col),
                    index(row + 1, col + 1),
                ];
                for face in [[a, b, c], [b, d, c]] {
                    for side in [-1.0, 1.0] {
                        let emitted = face
                            .map(|i| points[i] + offset + normals[i] * (side * THICKNESS * 0.5));
                        for weights in [
                            [1.0 / 3.0; 3],
                            [0.6, 0.2, 0.2],
                            [0.2, 0.6, 0.2],
                            [0.2, 0.2, 0.6],
                        ] {
                            let p = emitted
                                .into_iter()
                                .zip(weights)
                                .map(|(p, w)| p * w)
                                .sum::<Vec3>();
                            assert!(
                                rendered_clearance_checked(&support, p, true) > 0.001,
                                "joint collar fitting must retain emitted-face clearance: {p:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn retained_inner_hem_seating_is_not_limited_by_the_arm_contact_band() {
        let offset = Vec3::new(0.0, 0.53, -0.15);
        let mut points = radial_test_grid();
        let at = points[index(ROWS, 0)] + offset;
        assert!(at.y < -0.12);
        let inner = LayerSurface::from_triangles(vec![[
            Vec3::new(-0.02, -0.15, 0.45),
            Vec3::new(0.02, -0.15, 0.45),
            Vec3::new(0.0, -0.11, 0.45),
        ]]);
        assert_eq!(
            fit_mesh_contact(at, false, &inner),
            at,
            "a distant open arm patch does not contain this point"
        );
        seat_inner_layer(&mut points, Quat::IDENTITY, &inner).unwrap();
        let fitted = points[index(ROWS, 0)] + offset;
        assert!(fitted.z >= 0.45 + LAYER_GAP + THICKNESS - 1e-6);
        assert!((fitted.y - at.y).abs() < 1e-6);
    }

    #[test]
    fn rising_inner_roof_has_bounded_inward_plane_and_lining_clearance() {
        let roof = [
            Vec3::new(-0.1, 0.56, 0.10),
            Vec3::new(0.1, 0.56, 0.10),
            Vec3::new(0.0, 0.66, 0.20),
        ];
        let reference = [
            Vec3::new(-0.05, 0.565, 0.10),
            Vec3::new(0.05, 0.565, 0.10),
            Vec3::new(0.0, 0.60, 0.20),
        ];
        let normals = face_normals(Vec3::Z, 1.0);
        let candidate = normals
            .into_iter()
            .filter(|n| n.y > 1e-6)
            .find_map(|n| {
                let plane = roof
                    .iter()
                    .map(|q| n.dot(*q))
                    .fold(f32::NEG_INFINITY, f32::max)
                    + LAYER_GAP
                    + THICKNESS;
                let points = reference.map(|p| p + Vec3::Y * ((plane - n.dot(p)).max(0.0) / n.y));
                (points[0].y <= 0.649 && points[1].y <= 0.649).then_some((n, plane, points))
            })
            .expect("complete rising roof must admit bounded collar without outward escape");
        let (n, plane, points) = candidate;
        assert!(n.z < 0.0, "fixture requires an inward-facing roof plane");
        assert!(
            n.dot(Vec3::Z) < 0.0,
            "this plane must reject a pure-radial incident vertex"
        );
        for i in 0..=12 {
            for j in 0..=12 - i {
                let a = i as f32 / 12.0;
                let b = j as f32 / 12.0;
                let p = points[0] * a + points[1] * b + points[2] * (1.0 - a - b);
                for side in [-1.0, 1.0] {
                    let emitted = p + n * (side * THICKNESS * 0.5);
                    assert!(n.dot(emitted) >= plane - THICKNESS * 0.5 - 1e-6);
                    for q in roof {
                        assert!(n.dot(emitted - q) >= LAYER_GAP + THICKNESS * 0.5 - 1e-6);
                    }
                }
            }
        }
    }

    #[test]
    fn horizontal_inner_cloth_seats_row_one_vertically_and_keeps_emitted_lining_clear() {
        let offset = Vec3::new(0.0, 0.53, -0.15);
        let y = 0.57166666;
        let vertices = [
            Vec3::new(-0.29, y - THICKNESS * 0.5, -0.29),
            Vec3::new(0.29, y - THICKNESS * 0.5, -0.29),
            Vec3::new(0.29, y + THICKNESS * 0.5, -0.29),
            Vec3::new(-0.29, y + THICKNESS * 0.5, -0.29),
            Vec3::new(-0.29, y - THICKNESS * 0.5, 0.29),
            Vec3::new(0.29, y - THICKNESS * 0.5, 0.29),
            Vec3::new(0.29, y + THICKNESS * 0.5, 0.29),
            Vec3::new(-0.29, y + THICKNESS * 0.5, 0.29),
        ];
        let faces = [
            [0, 3, 2],
            [0, 2, 1],
            [4, 5, 6],
            [4, 6, 7],
            [0, 4, 7],
            [0, 7, 3],
            [1, 2, 6],
            [1, 6, 5],
            [0, 1, 5],
            [0, 5, 4],
            [3, 7, 6],
            [3, 6, 2],
        ];
        for (inner_angle, outer_angle) in [(0.0, 0.0), (0.025, -0.015), (-0.025, 0.015)] {
            let inner_rotation = Quat::from_rotation_z(inner_angle);
            let outer_rotation = Quat::from_rotation_z(outer_angle);
            let support = LayerSurface::from_triangles(
                faces
                    .into_iter()
                    .map(|face| face.map(|i| inner_rotation * (vertices[i] - offset) + offset))
                    .collect(),
            );
            let mut points = radial_test_grid();
            let before = points.clone();
            seat_inner_layer(&mut points, outer_rotation, &support)
                .expect("bounded inner sheet seating");
            let seated = points.clone();
            fit_radial_faces(&mut points, outer_rotation, &support)
                .expect("seated inner sheet face certification");
            for col in 0..=COLUMNS {
                let i = index(1, col);
                let old = outer_rotation * before[i] + offset;
                let reference = outer_rotation * seated[i] + offset;
                let final_point = outer_rotation * points[i] + offset;
                assert!(
                    reference.y > old.y + 0.02,
                    "row1 must gain vertical layer separation"
                );
                assert!(
                    (reference.with_y(0.0) - old.with_y(0.0)).length() < 1e-6,
                    "roof seating must not escape radially"
                );
                assert!(final_point.with_y(0.0).length()<=old.with_y(0.0).length()+LAYER_GAP+2.0*THICKNESS,
                    "certification must not turn the seated first row into a fin: {old:?} -> {final_point:?}");
            }
            let mut normals = vec![Vec3::ZERO; GRID];
            surface_normals(&points, &mut normals);
            for row in 0..ROWS {
                for col in 0..COLUMNS {
                    let [a, b, c, d] = [
                        index(row, col),
                        index(row, col + 1),
                        index(row + 1, col),
                        index(row + 1, col + 1),
                    ];
                    for face in [[a, b, c], [b, d, c]] {
                        for side in [-0.5, 0.5] {
                            let emitted = face.map(|i| {
                                outer_rotation * (points[i] + normals[i] * THICKNESS * side)
                                    + offset
                            });
                            for weights in [
                                [1.0 / 3.0; 3],
                                [0.6, 0.2, 0.2],
                                [0.2, 0.6, 0.2],
                                [0.2, 0.2, 0.6],
                            ] {
                                let p = emitted
                                    .into_iter()
                                    .zip(weights)
                                    .map(|(p, w)| p * w)
                                    .sum::<Vec3>();
                                assert!(rendered_clearance_checked(&support,p,true)>0.001,
                            "seated moving cloth exterior/lining must clear the inner sheet: {p:?}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn shoulder_faces_keep_supported_roof_and_descending_turnover() {
        let offset = Vec3::new(0.0, 0.53, -0.15);
        for tilt in [0.0, 0.35] {
            let rotation = Quat::from_rotation_z(tilt);
            let support = LayerSurface::from_triangles(
                rounded_cap()
                    .into_iter()
                    .map(|tri| {
                        tri.map(|p| {
                            rotation * (p * Vec3::new(0.9, 1.1, 1.0)) + Vec3::new(0.25, 0.4, 0.0)
                        })
                    })
                    .collect(),
            );
            let mut points = radial_test_grid();
            let reference = points.clone();
            fit_radial_faces(&mut points, Quat::IDENTITY, &support)
                .expect("bounded curved shoulder must be feasible");
            for (i, (p, before)) in points.iter().zip(&reference).enumerate() {
                if i < ROOF_VERTICES {
                    assert!(
                        p.with_y(0.0).distance(before.with_y(0.0)) < 1e-6,
                        "roof support cannot escape radially around a shoulder"
                    );
                    assert!(
                        p.y + offset.y <= support.bottom + support.height + 0.12,
                        "supported roof cannot become a raised tent"
                    );
                } else {
                    assert!(
                        (p.y - before.y).abs() < 1e-6,
                        "radial turnover must preserve its descending authored height"
                    );
                }
                if i <= COLUMNS {
                    assert!(
                        (*p - *before).length() < 1e-6,
                        "collar must remain anchored"
                    );
                }
                if i / (COLUMNS + 1) <= YOKE_ROWS {
                    assert!(
                        (*p + offset).with_y(0.0).length() < 0.43,
                        "shoulder fit cannot become a broad wing: {p:?}"
                    );
                }
            }
            assert!(
                points[index(0, COLUMNS / 4)].y - points[index(YOKE_ROWS, COLUMNS / 4)].y > 0.09
            );
            for row in 0..ROWS {
                for column in 0..COLUMNS {
                    let [a, b, c, d] = [
                        index(row, column),
                        index(row, column + 1),
                        index(row + 1, column),
                        index(row + 1, column + 1),
                    ];
                    for face in [[a, b, c], [b, d, c]] {
                        for weights in [
                            [1.0 / 3.0; 3],
                            [0.6, 0.2, 0.2],
                            [0.2, 0.6, 0.2],
                            [0.2, 0.2, 0.6],
                        ] {
                            let p = face
                                .into_iter()
                                .zip(weights)
                                .map(|(i, w)| (points[i] + offset) * w)
                                .sum::<Vec3>();
                            assert!(
                                rendered_clearance_checked(&support, p, false) > 0.001,
                                "curved shoulder triangle contacts support at {p:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn finite_first_faces_seat_collar_without_moving_its_opening_or_lower_rows() {
        let offset = Vec3::new(0.0, 0.53, -0.15);
        let mut points = radial_test_grid();
        let before = points.clone();
        let anchor = points[0] + offset;
        let support = LayerSurface::from_triangles(vec![[
            anchor - Vec3::X * 0.01,
            anchor + Vec3::X * 0.01,
            anchor + Vec3::Y * 0.01,
        ]]);
        fit_radial_faces(&mut points, Quat::IDENTITY, &support)
            .expect("finite first-face support must seat the collar inside its existing ceiling");
        let mut sequential = before.clone();
        seat_layer_collar(&mut sequential, Quat::IDENTITY, &support).unwrap();
        fit_radial_faces(&mut sequential, Quat::IDENTITY, &support).unwrap();
        assert!(
            points[0].y + 0.001 < sequential[0].y,
            "one joint solve must avoid the redundant strip-seat lift: joint={:?}, sequential={:?}",
            points[0],
            sequential[0]
        );
        assert!(points[0].y > before[0].y + 0.025);
        for (i, (p, old)) in points.iter().zip(&before).enumerate() {
            if i <= COLUMNS {
                assert!((p.with_y(0.0) - old.with_y(0.0)).length() < 1e-6);
                assert!(
                    p.y + offset.y
                        <= 0.565 + 2.0 * (LAYER_GAP + THICKNESS + ROOF_SLOPE_SLACK) + 1e-6
                );
            } else if i < ROOF_VERTICES {
                assert!(p.with_y(0.0).distance(old.with_y(0.0)) < 1e-6);
            } else {
                assert!((p.y - old.y).abs() < 1e-6);
            }
        }
        for column in 0..COLUMNS {
            let [a, b, c, d] = [
                index(0, column),
                index(0, column + 1),
                index(1, column),
                index(1, column + 1),
            ];
            for face in [[a, b, c], [b, d, c]] {
                for weights in [
                    [1.0 / 3.0; 3],
                    [0.6, 0.2, 0.2],
                    [0.2, 0.6, 0.2],
                    [0.2, 0.2, 0.6],
                ] {
                    let p = face
                        .into_iter()
                        .zip(weights)
                        .map(|(i, w)| (points[i] + offset) * w)
                        .sum::<Vec3>();
                    assert!(
                        rendered_clearance_checked(&support, p, true) > 0.001,
                        "seated first-face interior must clear finite support: {p:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn radial_collar_height_failure_is_explicit_and_transactional() {
        let mut points = radial_test_grid();
        let before = points.clone();
        let anchor = points[0] + Vec3::new(0.0, 0.53, -0.15);
        let support = LayerSurface::from_triangles(vec![[
            anchor - Vec3::X * 0.01,
            anchor + Vec3::X * 0.01,
            anchor + Vec3::Y * 0.2,
        ]]);
        let failure = fit_radial_faces(&mut points, Quat::IDENTITY, &support).unwrap_err();
        assert_eq!(failure.row, 0);
        assert_eq!(
            points, before,
            "infeasible anchors must not leave a partially fitted mesh"
        );
    }

    #[test]
    fn collar_seating_preserves_xz_and_rejects_unbounded_lift_transactionally() {
        let offset = Vec3::new(0.0, 0.53, -0.15);
        for rise in [0.0, 0.2] {
            let mut points = radial_test_grid();
            let before = points.clone();
            let a = points[0] + offset + Vec3::Y * rise;
            let b = points[1] + offset + Vec3::Y * rise;
            let support =
                LayerSurface::from_triangles(vec![[a, b, (a + b) * 0.5 + Vec3::Z * 0.004]]);
            let result = seat_layer_collar(&mut points, Quat::IDENTITY, &support);
            if rise == 0.0 {
                result.expect("one cloth layer must fit bounded collar reserve");
                assert!(points[0].y >= before[0].y + LAYER_GAP + THICKNESS - 1e-6);
                for (p, old) in points.iter().zip(&before) {
                    assert!((p.with_y(0.0) - old.with_y(0.0)).length() < 1e-6);
                }
            } else {
                assert!(result.is_err());
                assert_eq!(points, before);
            }
        }
    }

    #[test]
    fn real_pauldron_contact_keeps_curvature_instead_of_slab_cap() {
        let surface = LayerSurface::from_triangles(rounded_cap());
        let inner = fit_mesh_contact(Vec3::new(0.02, 0.0, 0.0), true, &surface);
        let outer = fit_mesh_contact(Vec3::new(0.08, 0.0, 0.0), true, &surface);
        assert!(
            inner.y - outer.y > 0.025,
            "rounded shoulder cannot become a flat cylinder lid: {inner:?}, {outer:?}"
        );
        assert!(
            outer.y < 0.105,
            "outer shoulder must stay below the old .103 cylinder lid plus cloth margin"
        );
        assert!(rendered_clearance(&surface, inner) > 0.001);
        assert!(rendered_clearance(&surface, outer) > 0.001);
        assert!(
            rendered_clearance(&surface, Vec3::ZERO) < 0.0,
            "independent winding oracle must reject an interior point"
        );
        let planes = [
            (Vec3::X, 0.07),
            (Vec3::NEG_X, -0.09),
            (Vec3::Z, -0.01),
            (Vec3::NEG_Z, -0.01),
        ];
        let mut candidates = 0;
        let actual = surface
            .clipped_candidates(&planes)
            .filter_map(|&tri| {
                candidates += 1;
                clipped_support(tri, Vec3::Y, &planes)
            })
            .reduce(f32::max);
        let expected = surface
            .triangles
            .iter()
            .filter_map(|&tri| clipped_support(tri, Vec3::Y, &planes))
            .reduce(f32::max);
        assert!((actual.unwrap() - expected.unwrap()).abs() < 1e-6);
        assert!(
            candidates < surface.triangles.len() / 2,
            "local queries must reject distant faces before clipping"
        );
        assert!(surface.tree.len() <= surface.triangles.len() * 2);
        let mut independent = Builder::default();
        for &triangle in &surface.triangles {
            independent.tri(triangle, Vec3::ONE);
        }
        for (origin, ray) in [
            (Vec3::ZERO, Vec3::X),
            (Vec3::new(0.08, -0.2, 0.0), Vec3::Y),
            (Vec3::new(0.2, -0.2, 0.0), Vec3::Y),
            (Vec3::new(-0.2, 0.04, 0.02), Vec3::X),
        ] {
            let expected = independent.outer_surface(origin, ray);
            let actual = surface.ray(origin, ray).map(|d| origin + ray * d);
            match (actual, expected) {
                (Some(a), Some(b)) => assert!(a.distance(b) < 1e-5),
                (None, None) => {}
                _ => panic!("BVH omitted an exhaustive ray witness"),
            }
        }
    }

    #[test]
    fn main_and_mantle_share_real_geometry_and_unchanged_pose_cache() {
        let mut equipment = Equipment::default();
        equipment.select("leather-cloak").unwrap();
        let (_, mut rig, _, _) = super::super::rogue_tests::spawn_one(
            &Look::of(CreatureModel::Human, 18).equipped(equipment),
        );
        assert_eq!(rig.cloaks.len(), 2);
        assert!(Arc::ptr_eq(
            rig.cloaks[0].arm_surface.as_ref().unwrap(),
            rig.cloaks[1].arm_surface.as_ref().unwrap()
        ));
        let old = rig.cloaks[0].arm_surface.clone().unwrap();
        let snapshot = rig.cloaks[0].contact_surface.clone().unwrap();
        let again = Arc::new(ContactSurface::new(snapshot.parts.clone()));
        let cpu_before = rig.cloaks[0].contact_cpu_ms;
        rig.cloaks[0].set_contact_surface(again.clone());
        assert_eq!(
            rig.cloaks[0].contact_cpu_ms, cpu_before,
            "cache hits must not report another build"
        );
        for cloak in &rig.cloaks {
            let report = cloak.contact_diagnostics().unwrap();
            assert_eq!(report["rows"].as_array().unwrap().len(), YOKE_ROWS + 1);
            assert!(
                serde_json::to_vec(&report).unwrap().len() < 8192,
                "diagnostics must stay bounded per layer"
            );
            assert_eq!(cloak.contact_reference.len(), GRID);
            assert!(cloak
                .contact_cpu_ms
                .iter()
                .all(|ms| ms.is_finite() && *ms >= 0.0));
        }
        assert!(Arc::ptr_eq(
            &old,
            rig.cloaks[0].arm_surface.as_ref().unwrap()
        ));
        assert!(
            again.cached.get().is_none(),
            "unchanged poses must not transform triangles or rebuild indices"
        );
        eprintln!(
            "study cloth contact: triangles={}, nodes={}, height_bin_refs={}, triangle_bytes={}",
            old.triangles.len(),
            old.tree.len(),
            old.bins.iter().map(Vec::len).sum::<usize>(),
            old.triangles.len() * std::mem::size_of::<[Vec3; 3]>()
        );
    }

    #[test]
    fn captured_arm_hulls_exclude_proven_prism_only_witnesses() {
        // Actual source-frozen native first-red traces, not reconstructed poses.
        for (reference, endpoints, witness) in [
            (
                [
                    Vec3::new(
                        f32::from_bits(0xbe18dc4a),
                        f32::from_bits(0x3f03c44f),
                        f32::from_bits(0xbe0d46c3),
                    ),
                    Vec3::new(
                        f32::from_bits(0xbe0f7183),
                        f32::from_bits(0x3f022223),
                        f32::from_bits(0xbe19a168),
                    ),
                    Vec3::new(
                        f32::from_bits(0xbdeb4c14),
                        f32::from_bits(0x3ef0a3d9),
                        f32::from_bits(0xbe4630bc),
                    ),
                ],
                [
                    Vec3::new(
                        f32::from_bits(0xbea7c436),
                        f32::from_bits(0x3f211a8b),
                        f32::from_bits(0xbe9b0d82),
                    ),
                    Vec3::new(
                        f32::from_bits(0xbeaf63c6),
                        f32::from_bits(0x3f2bb33e),
                        f32::from_bits(0xbebbd879),
                    ),
                    Vec3::new(
                        f32::from_bits(0xbe9f4417),
                        f32::from_bits(0x3ef0a3d9),
                        f32::from_bits(0xbf062642),
                    ),
                ],
                Vec3::new(
                    f32::from_bits(0xbeb12dd2),
                    f32::from_bits(0x3f0b7e0d),
                    f32::from_bits(0xbea47019),
                ),
            ),
            (
                [
                    Vec3::new(
                        f32::from_bits(0xbe618226),
                        f32::from_bits(0x3f022222),
                        f32::from_bits(0x3dcb1b54),
                    ),
                    Vec3::new(
                        f32::from_bits(0xbe79aa27),
                        f32::from_bits(0x3f022222),
                        f32::from_bits(0x3db77b80),
                    ),
                    Vec3::new(
                        f32::from_bits(0xbe7c0188),
                        f32::from_bits(0x3ef0a3d7),
                        f32::from_bits(0x3dd2c6d4),
                    ),
                ],
                [
                    Vec3::new(
                        f32::from_bits(0xbe85633f),
                        f32::from_bits(0x3f0a2e53),
                        f32::from_bits(0x3df0463a),
                    ),
                    Vec3::new(
                        f32::from_bits(0xbe970f4f),
                        f32::from_bits(0x3f0d0078),
                        f32::from_bits(0x3dde084a),
                    ),
                    Vec3::new(
                        f32::from_bits(0xbea59d95),
                        f32::from_bits(0x3ef0a3d7),
                        f32::from_bits(0x3e0a8526),
                    ),
                ],
                Vec3::new(
                    f32::from_bits(0xbe8b66f3),
                    f32::from_bits(0x3ef91303),
                    f32::from_bits(0x3d8713bf),
                ),
            ),
        ] {
            let planes = swept_arm_planes(reference, endpoints);
            assert!(planes.len() <= MAX_CONTACT_PLANES);
            assert!(planes.iter().any(|(n, h)| n.dot(witness) < *h - 0.001));
            let remote = [
                witness,
                witness + Vec3::X * 0.0001,
                witness + Vec3::Y * 0.0001,
            ];
            assert!(clipped_polygon(remote, &planes).is_none());
            for mask in 0..8 {
                let face = std::array::from_fn::<_, 3, _>(|i| {
                    if mask & (1 << i) == 0 {
                        reference[i]
                    } else {
                        endpoints[i]
                    }
                });
                for weights in [
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [0.2, 0.3, 0.5],
                ] {
                    let point = face
                        .into_iter()
                        .zip(weights)
                        .map(|(p, w)| p * w)
                        .sum::<Vec3>();
                    for (normal, offset) in &planes {
                        assert!(normal.dot(point) >= *offset + LAYER_GAP + THICKNESS - 1e-6);
                    }
                }
            }
        }
    }

    #[test]
    fn captured_cope_inward_bevel_rejects_distant_neck_corner() {
        // Exact spawned cope face[5,54,53], iteration1, source-frozen native trace.
        let vertices = [
            Vec3::from_array([0xbdf4612e, 0x3f18d4fe, 0x3d691b38].map(f32::from_bits)),
            Vec3::from_array([0xbe19a8e7, 0x3f1aacaf, 0x3d65b274].map(f32::from_bits)),
            Vec3::from_array([0xbe1767e0, 0x3f1bb4ff, 0x3d92b748].map(f32::from_bits)),
        ];
        let endpoints = [
            Vec3::from_array([0xbdf4612e, 0x3f18d4fe, 0x3d691b38].map(f32::from_bits)),
            Vec3::from_array([0xbe7f19da, 0x3f1aacaf, 0x3dbeaaf6].map(f32::from_bits)),
            Vec3::from_array([0xbe1767e0, 0x3f1bb4ff, 0x3d92b748].map(f32::from_bits)),
        ];
        let witness = Vec3::from_array([0xbd2e5239, 0x3f10a3d7, 0x3c9710b2].map(f32::from_bits));
        let rays = vertices.map(|p| p.with_y(0.0).normalize());
        let horizontal = rays.into_iter().sum::<Vec3>().normalize();
        let tangent = Vec3::new(horizontal.z, 0.0, -horizontal.x);
        let angles = rays.map(|r| r.dot(tangent).atan2(r.dot(horizontal)));
        let lo = angles.into_iter().fold(f32::INFINITY, f32::min);
        let hi = angles.into_iter().fold(f32::NEG_INFINITY, f32::max);
        let left = horizontal * lo.cos() + tangent * lo.sin();
        let right = horizontal * hi.cos() + tangent * hi.sin();
        let (low, high, outer) = swept_face_bounds(vertices, endpoints, horizontal);
        let prism = [
            (Vec3::Y, low - THICKNESS),
            (Vec3::NEG_Y, -high - THICKNESS),
            (Vec3::new(left.z, 0.0, -left.x), -THICKNESS),
            (Vec3::new(-right.z, 0.0, right.x), -THICKNESS),
            (-horizontal, -outer - THICKNESS),
        ];
        let mut old = swept_arm_planes(vertices, endpoints);
        old.extend(prism.map(|(n, h)| (n, h - LAYER_GAP)));
        assert!(
            old.iter().all(|(n, h)| n.dot(witness) >= *h - 1e-6),
            "fixture must reproduce the admitted corner"
        );
        let planes = arm_contact_planes(vertices, endpoints, &prism);
        assert!(planes.iter().any(|(n, h)| *h - n.dot(witness) > 0.05));
        for mask in 0..8 {
            let face = std::array::from_fn::<_, 3, _>(|i| {
                if mask & (1 << i) == 0 {
                    vertices[i]
                } else {
                    endpoints[i]
                }
            });
            for p in face
                .into_iter()
                .chain([face.into_iter().sum::<Vec3>() / 3.0])
            {
                for n in [Vec3::X, Vec3::Y, Vec3::Z, horizontal, Vec3::ONE.normalize()] {
                    for sign in [-1.0, 1.0] {
                        let expanded = p + n * (sign * (LAYER_GAP + THICKNESS));
                        assert!(planes.iter().all(|(n, h)| n.dot(expanded) >= *h - 1e-6));
                    }
                }
            }
        }
        assert_eq!(MAX_CONTACT_PLANES, 46);
        assert_eq!(MAX_CLIPPED_VERTICES, 49);
    }

    #[test]
    fn arm_hull_prism_intersection_preserves_clearance_neighborhoods() {
        let vertices = [
            Vec3::new(0.0, 0.4, 0.2),
            Vec3::new(0.1, 0.4, 0.2),
            Vec3::new(0.0, 0.5, 0.3),
        ];
        let endpoints = [
            vertices[0] + Vec3::Z * 0.1,
            vertices[1] + Vec3::Y * 0.1,
            vertices[2],
        ];
        let prism = [
            (Vec3::X, -THICKNESS),
            (Vec3::NEG_X, -0.1 - THICKNESS),
            (Vec3::Y, 0.4 - THICKNESS),
            (Vec3::NEG_Y, -0.5 - THICKNESS),
            (Vec3::NEG_Z, -0.3 - THICKNESS),
        ];
        let planes = arm_contact_planes(vertices, endpoints, &prism);
        let gap = LAYER_GAP + THICKNESS;
        for mask in 0..8 {
            let face = std::array::from_fn::<_, 3, _>(|i| {
                if mask & (1 << i) == 0 {
                    vertices[i]
                } else {
                    endpoints[i]
                }
            });
            for point in face.into_iter().chain([face.iter().sum::<Vec3>() / 3.0]) {
                for normal in [Vec3::X, Vec3::Y, Vec3::Z, Vec3::ONE.normalize()] {
                    for sign in [-1.0, 1.0] {
                        let expanded = point + normal * sign * gap;
                        assert!(planes.iter().all(|(n, h)| n.dot(expanded) >= *h - 1e-6));
                    }
                }
            }
        }
        // The prism's unit bevel remains present even when not a hull facet.
        for (normal, offset) in prism {
            assert!(planes
                .iter()
                .any(|(n, h)| *n == normal && *h == offset - LAYER_GAP));
        }
    }

    #[test]
    fn arm_hull_degeneracies_and_clipping_remain_bounded() {
        for (reference, endpoints) in [
            (
                [Vec3::ZERO, Vec3::X, Vec3::Y],
                [Vec3::ZERO, Vec3::X, Vec3::Y],
            ),
            (
                [Vec3::ZERO, Vec3::X, Vec3::Y],
                [Vec3::Z * 1e-8, Vec3::X, Vec3::Y],
            ),
            (
                [Vec3::ZERO, Vec3::ONE, Vec3::ONE * 0.5],
                [Vec3::ZERO, Vec3::ONE, Vec3::ONE * 0.7],
            ),
            ([Vec3::ZERO; 3], [Vec3::ZERO; 3]),
        ] {
            let planes = swept_arm_planes(reference, endpoints);
            assert!(!planes.is_empty() && planes.len() <= MAX_CONTACT_PLANES);
            for p in reference.into_iter().chain(endpoints) {
                assert!(planes
                    .iter()
                    .all(|(n, h)| n.dot(p) >= *h + LAYER_GAP + THICKNESS - 1e-6));
            }
            let triangle = [
                Vec3::new(-0.2, -0.1, 0.0),
                Vec3::new(1.2, 0.2, 0.0),
                Vec3::new(0.1, 1.2, 0.0),
            ];
            let mut expected = triangle.to_vec();
            for &(n, h) in &planes {
                expected = clip_layer_polygon(&expected, n, h);
            }
            let actual = clipped_polygon(triangle, &planes);
            assert_eq!(actual.is_some(), !expected.is_empty());
            if let Some((points, count)) = actual {
                assert!(count <= MAX_CLIPPED_VERTICES);
                for normal in [Vec3::X, Vec3::Y, Vec3::Z] {
                    let a = points[..count]
                        .iter()
                        .map(|p| normal.dot(*p))
                        .fold(f32::NEG_INFINITY, f32::max);
                    let b = expected
                        .iter()
                        .map(|p| normal.dot(*p))
                        .fold(f32::NEG_INFINITY, f32::max);
                    assert!((a - b).abs() < 1e-6);
                }
            }
        }
    }

    #[test]
    fn arm_hull_cache_tracks_endpoint_changes_inside_same_prism() {
        let first = [
            Vec3::new(0.0, 0.5, 0.2),
            Vec3::new(0.1, 0.5, 0.2),
            Vec3::new(0.0, 0.4, 0.5),
        ];
        let mut second = first;
        second[0].x += 0.01;
        assert_ne!(
            swept_domain_key(0.5, 0.5, Some(first)),
            swept_domain_key(0.5, 0.5, Some(second))
        );
    }

    #[test]
    fn fixed_clipper_accepts_three_four_and_five_plane_footprints() {
        let triangle = [
            Vec3::new(-1.0, 0.2, -1.0),
            Vec3::new(2.0, 0.8, -1.0),
            Vec3::new(-1.0, 0.5, 2.0),
        ];
        let planes = [
            (Vec3::X, 0.0),
            (Vec3::Z, 0.0),
            (Vec3::new(-1.0, 0.0, -1.0).normalize(), -0.5),
            (Vec3::NEG_X, -0.2),
            (Vec3::NEG_Z, -0.25),
        ];
        for count in [3, 4, 5] {
            let footprint = &planes[..count];
            let mut polygon = triangle.to_vec();
            for &(normal, offset) in footprint {
                polygon = clip_layer_polygon(&polygon, normal, offset);
            }
            assert!(
                !polygon.is_empty(),
                "fixture must exercise surviving intersections"
            );
            let expected = polygon
                .iter()
                .map(|p| p.y)
                .fold(f32::NEG_INFINITY, f32::max);
            let actual = clipped_support(triangle, Vec3::Y, footprint).unwrap();
            assert!(
                (actual - expected).abs() < 1e-6,
                "{count}-plane clipping changed support"
            );
        }
    }

    #[test]
    fn maximum_contact_plane_count_matches_dynamic_clipping() {
        let triangle = [
            Vec3::new(-3.0, 0.2, -2.0),
            Vec3::new(3.0, 0.8, -2.0),
            Vec3::new(0.0, 0.5, 3.0),
        ];
        let planes: Vec<_> = (0..MAX_CONTACT_PLANES)
            .map(|i| {
                let angle = TAU * i as f32 / MAX_CONTACT_PLANES as f32;
                (Vec3::new(angle.cos(), 0.0, angle.sin()), -0.5)
            })
            .collect();
        let mut expected = triangle.to_vec();
        for &(n, h) in &planes {
            expected = clip_layer_polygon(&expected, n, h);
        }
        let (actual, count) = clipped_polygon(triangle, &planes).unwrap();
        assert_eq!(count, MAX_CONTACT_PLANES);
        assert_eq!(count, expected.len());
        let expected_height = expected
            .iter()
            .map(|p| p.y)
            .fold(f32::NEG_INFINITY, f32::max);
        let actual_height = actual[..count]
            .iter()
            .map(|p| p.y)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!((actual_height - expected_height).abs() < 1e-6);
    }

    #[test]
    #[should_panic(expected = "exceed bounded contact planes")]
    fn fixed_clipper_rejects_unsupported_plane_count_before_clipping() {
        let _ = clipped_support(
            [Vec3::ZERO; 3],
            Vec3::Y,
            &[(Vec3::X, 0.0); MAX_CONTACT_PLANES + 1],
        );
    }

    #[test]
    fn steep_roof_reselects_plane_without_clamping_collision_displacement() {
        let face = [
            Vec3::new(0.0, 0.6, 0.0),
            Vec3::new(0.1, 0.2, 0.0),
            Vec3::new(0.0, 0.6, 0.1),
        ];
        let under = face.map(|p| p - Vec3::Y * 0.01);
        let surface = LayerSurface::from_triangles(vec![under]);
        let steep = Vec3::new(4.0, 1.0, 0.0).normalize();
        let margin = LAYER_GAP + THICKNESS;
        let old_plane = surface.roof_plane(face, steep).unwrap() + margin;
        let old_peak = face[0].y + (old_plane - steep.dot(face[0])) / steep.y;
        let horizontal = surface.roof_plane(face, Vec3::Y).unwrap() + margin;
        assert!(
            old_peak > horizontal + 0.09,
            "fixture must reproduce steep amplification"
        );
        let (normal, plane) = surface.bounded_roof_plane(face).unwrap();
        assert!(
            normal.y > steep.y,
            "select a genuinely shallower supporting plane"
        );
        let fitted = face.map(|p| p + Vec3::Y * ((plane - normal.dot(p)) / normal.y).max(0.0));
        for (original, point) in face.into_iter().zip(fitted) {
            assert!(point.y <= original.y.max(horizontal) + ROOF_SLOPE_SLACK + 1e-6);
            assert!(normal.dot(point) >= plane - 1e-6);
        }
        for a in 0..=8 {
            for b in 0..=8 - a {
                let contact =
                    (under[0] * a as f32 + under[1] * b as f32 + under[2] * (8 - a - b) as f32)
                        / 8.0;
                // The complete support face, not just its vertices, remains
                // behind the selected separator by the original margin.
                assert!(plane - normal.dot(contact) >= margin - 1e-6);
            }
        }
        assert!(
            fitted[0].y - fitted[1].y > 0.025,
            "bounded selection must preserve a downward shoulder slope here"
        );
    }

    #[test]
    fn inclined_roof_keeps_slope_and_excludes_bbox_only_obstacles() {
        let face = [
            Vec3::new(0.0, 0.6, 0.0),
            Vec3::new(0.3, 0.45, 0.0),
            Vec3::new(0.0, 0.6, 0.3),
        ];
        let normal = Vec3::new(0.5, 1.0, 0.0).normalize();
        let under = face.map(|p| p - Vec3::Y * 0.01);
        // Inside the bounding rectangle, outside the actual triangle.
        let remote = [
            Vec3::new(0.25, 0.9, 0.25),
            Vec3::new(0.29, 0.9, 0.25),
            Vec3::new(0.25, 0.9, 0.29),
        ];
        let support = LayerSurface::from_triangles(vec![under, remote]);
        let (selected, plane) = support.bounded_roof_plane(face).unwrap();
        assert!(
            (selected - normal).length() < 1e-6,
            "safe authored slope should be retained"
        );
        let fitted = face.map(|p| p + Vec3::Y * ((plane - normal.dot(p)) / normal.y).max(0.0));
        assert!(
            (fitted[0].y - fitted[1].y - 0.15).abs() < 1e-6,
            "an inclined roof must not become a horizontal shelf"
        );
        assert!(
            fitted.iter().all(|p| p.y < 0.64),
            "unrelated bbox peak must not lift the roof"
        );
        for point in under {
            assert!(normal.dot(point) + LAYER_GAP + THICKNESS <= plane + 1e-6);
        }
    }

    #[test]
    fn support_derived_turnover_removes_authored_width_floor_but_keeps_armor() {
        let mut body = crate::sdf::Sculpt::default();
        body.add(
            crate::sdf::Shape::Ellipsoid(
                Vec3::new(0.0, 0.47 / anatomy::HIP_SCALE.y, 0.0),
                Vec3::new(0.20, 0.10, 0.15),
            ),
            0.0,
        );
        let authored = Vec3::new(0.375, 0.47, 0.0);
        let no_arms = LayerSurface::from_triangles(Vec::new());
        let fitted = supported_turnover(authored, &body, Some(&no_arms));
        assert!(
            fitted.x < authored.x - 0.05,
            "the authored shoulder must not defeat actual narrower support"
        );
        assert_eq!(fitted.y, authored.y);
        assert!(body.distance(fitted / anatomy::HIP_SCALE) >= LAYER_GAP + 0.0098);
        let armor = LayerSurface::from_triangles(vec![[
            Vec3::new(0.40, 0.43, -0.04),
            Vec3::new(0.40, 0.51, -0.04),
            Vec3::new(0.40, 0.47, 0.04),
        ]]);
        let armored = supported_turnover(authored, &body, Some(&armor));
        assert!(
            armored.x >= 0.40 + LAYER_GAP + THICKNESS - 1e-6,
            "real armor must expand beyond the original recipe when required"
        );
        let opposite = authored * Vec3::new(-1.0, 1.0, 1.0);
        let back = supported_turnover(opposite, &body, Some(&armor));
        let torso_only = supported_turnover(opposite, &body, None);
        assert!(
            back.distance(torso_only) < 1e-6,
            "a no-hit arm direction must still use torso support, not authored width"
        );
        assert_eq!(
            supported_turnover(authored, &crate::sdf::Sculpt::default(), Some(&no_arms)),
            authored,
            "missing all support retains the explicit authored fallback"
        );
    }

    #[test]
    fn supported_turnover_cache_tracks_equipment_without_changing_hem_fullness() {
        let look = Look::with_choice(CreatureModel::Human, 18, Some(Variant::default()));
        let body = HumanoidBody::resolve(&look);
        let mut recipe = Recipe::new(body, Vec3::ONE, Vec3::ONE, false);
        let mut cut = DrapeCut {
            length: 1.04,
            fullness: 1.72,
            folds: 6,
            opening: 1.45,
            hem: DrapeHem::Rounded,
            shoulder_layer: false,
        };
        recipe.set_cut(cut);
        let column = COLUMNS / 4;
        let edge = recipe.point(YOKE_ROWS, column);
        let hem = recipe.point(ROWS, column);
        assert!(recipe.fitted_turnover.get().is_some());
        cut.fullness += 0.5;
        recipe.set_cut(cut);
        assert!(recipe.fitted_turnover.get().is_none());
        assert!(
            recipe.point(YOKE_ROWS, column).distance(edge) < 1e-6,
            "hanging fullness cannot widen the fitted shoulder attachment"
        );
        assert!(
            recipe.point(ROWS, column).distance(hem) > 0.05,
            "support-derived turnover must preserve independently authored hem fullness"
        );
        let ring = recipe.fitted_turnover.get().unwrap();
        let offset = Vec3::new(0.0, 0.53, -0.15);
        assert!(
            recipe
                .point(YOKE_ROWS, column)
                .distance(ring[column] - offset)
                < 0.001,
            "turnover row must share the ring used by both interpolation branches"
        );
        let direction = ring[column].with_y(0.0).normalize();
        let mut armor = crate::sdf::Sculpt::default();
        armor.add(
            crate::sdf::Shape::Ellipsoid(
                (Vec3::Y * 0.47 + direction * 0.46) / anatomy::HIP_SCALE,
                Vec3::splat(0.05),
            ),
            0.0,
        );
        recipe.reserve_body_layer(&armor);
        assert!(
            recipe.fitted_turnover.get().is_none(),
            "a retained armor change invalidates the support reference"
        );
        let armored = recipe.point(YOKE_ROWS, column) + offset;
        assert!(
            armored.with_y(0.0).length() > (edge + offset).with_y(0.0).length() + 0.05,
            "retained armor remains a real fit constraint, not clipped to the narrower recipe"
        );
    }

    #[test]
    fn shoulder_turnover_fits_outward_without_inheriting_cap_height() {
        let mut cap = crate::sdf::Sculpt::default();
        cap.add(
            crate::sdf::Shape::Ellipsoid(Vec3::new(0.28, 0.50, 0.0), Vec3::new(0.10, 0.09, 0.09)),
            0.0,
        );
        let authored = Vec3::new(0.29, 0.47, 0.0) * anatomy::HIP_SCALE;
        let roof = fit_wrapped_contact(authored, true, &|p| cap.distance(p));
        let turnover = fit_wrapped_contact(authored, false, &|p| cap.distance(p));
        assert!(
            roof.y > 0.60,
            "fixture must reproduce the old cap-top floor"
        );
        assert!(
            (turnover.y - authored.y).abs() < 1e-6,
            "turnover must fit around the side, not climb above the cap"
        );
        assert!(cap.distance(turnover / anatomy::HIP_SCALE) >= LAYER_GAP + 0.008);
        assert!(
            turnover.x > roof.x + 0.08,
            "outward displacement must actually clear the cap"
        );
        let recipe = study_recipe(outfit::outerwear::OuterwearAppearance::LeatherCloak);
        for column in 0..=COLUMNS {
            let edge = recipe.point(YOKE_ROWS, column) + Vec3::new(0.0, 0.53, -0.15);
            assert!(
                (edge.y - 0.47).abs() < 1e-6,
                "authored turnover inherited cap height: {edge:?}"
            );
        }
        let rows: Vec<_> = (0..=ROWS).map(study_row_fraction).collect();
        assert_eq!(
            rows.len(),
            21,
            "local yoke refinement must not enlarge the grid"
        );
        assert_eq!(rows.len() * (COLUMNS + 1), 1029);
        assert!((study_row_fraction(YOKE_ROWS) - YOKE_FRACTION).abs() < 1e-6);
        assert!((study_row_fraction(ROWS) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn mantle_station_refits_all_rays_without_changing_main_or_hanging_pattern() {
        let main = study_recipe(outfit::outerwear::OuterwearAppearance::LeatherCloak);
        let mut mantle = study_recipe(outfit::outerwear::OuterwearAppearance::LeatherCloak);
        mantle.layer_over(Entity::PLACEHOLDER);
        let offset = Vec3::new(0.0, 0.53, -0.15);
        for column in 0..=COLUMNS {
            let a = main.point(0, column) + offset;
            let b = mantle.point(0, column) + offset;
            assert!((a.y - 0.565).abs() < 1e-6);
            assert!((b.y - OUTER_COLLAR_STATION).abs() < 1e-6);
            assert!(mantle.support.distance(b / anatomy::HIP_SCALE) >= LAYER_GAP + 0.0097);
            assert!(a.with_y(0.0).normalize().dot(b.with_y(0.0).normalize()) > 0.9999);
            for row in YOKE_ROWS..=ROWS {
                assert!(
                    main.point(row, column).distance(mantle.point(row, column)) < 1e-7,
                    "attachment cannot change turnover/fullness/hem"
                );
            }
        }
        assert!(mantle.fitted_outer_collar.get().unwrap().is_some());
        let mut empty = crate::sdf::Sculpt::default();
        empty.add(
            crate::sdf::Shape::Ellipsoid(Vec3::new(0.0, -2.0, 0.0), Vec3::splat(0.05)),
            0.0,
        );
        assert!(
            supported_outer_collar(Vec3::Z, &empty).is_none(),
            "missing support is not a fitted seed"
        );
    }

    #[test]
    fn neck_opening_surrounds_actual_torso_without_climbing_its_cap() {
        let recipe = study_recipe(outfit::outerwear::OuterwearAppearance::LeatherCloak);
        let offset = Vec3::new(0.0, 0.53, -0.15);
        for column in 0..=COLUMNS {
            let collar = recipe.point(0, column) + offset;
            assert!(
                (collar.y - 0.565).abs() < 1e-6,
                "neck opening must stay at the anatomical attachment: {collar:?}"
            );
            let query = collar / anatomy::HIP_SCALE;
            assert!(
                recipe.support.distance(query) >= LAYER_GAP + 0.008,
                "neck opening must clear the actual retained torso surface"
            );
            assert!(
                collar.with_y(0.0).length() < 0.18,
                "collar must remain local to the neck, not encircle an arm"
            );
            let next = recipe.point(1, column) + offset;
            assert!(
                next.with_y(0.0).length() >= collar.with_y(0.0).length(),
                "shoulder transition must start outside the fitted opening"
            );
        }
    }

    #[test]
    fn neutral_layered_yokes_have_bounded_height_and_shoulder_slope() {
        for (name, armor) in [
            ("leather-cloak", "hawaiian-shirt"),
            ("ornamental-cope", "plate-mail"),
        ] {
            let look = Look::with_choice(CreatureModel::Human, 18, Some(Variant::default()));
            let mut equipment = Equipment::default();
            equipment.select(armor).unwrap();
            equipment.select(name).unwrap();
            let (_, mut rig, _, mut meshes) =
                super::super::rogue_tests::spawn_one(&look.equipped(equipment));
            let spawned_peaks: Vec<_> = rig
                .cloaks
                .iter()
                .map(|cloak| {
                    cloak.points[..(YOKE_ROWS + 1) * (COLUMNS + 1)]
                        .iter()
                        .map(|p| p.y + 0.53)
                        .fold(f32::NEG_INFINITY, f32::max)
                })
                .collect();
            place_layers(&mut rig.cloaks, |_| Some(Quat::IDENTITY), &mut meshes);
            let initial: Vec<_> = rig
                .cloaks
                .iter()
                .map(|cloak| cloak.points.clone())
                .collect();
            let authored: Vec<_> = rig.cloaks.iter().map(|cloak| cloak.rest.clone()).collect();
            for cloak in &mut rig.cloaks {
                cloak.invalidate();
            }
            place_layers(&mut rig.cloaks, |_| Some(Quat::IDENTITY), &mut meshes);
            for (((cloak, first), rest), spawned_peak) in rig
                .cloaks
                .iter()
                .zip(&initial)
                .zip(&authored)
                .zip(&spawned_peaks)
            {
                assert_eq!(&cloak.rest, rest, "contact must not mutate authored rest");
                for column in 0..=COLUMNS {
                    let attachment = cloak.rest[index(0, column)] + Vec3::new(0.0, 0.53, -0.15);
                    let station = if cloak.inner_layer.is_some() {
                        OUTER_COLLAR_STATION
                    } else {
                        0.565
                    };
                    assert!((attachment.y - station).abs() < 1e-6,
                        "{name}: attachment departed from its supported layer station: {attachment:?}");
                    let final_attachment =
                        cloak.points[index(0, column)] + Vec3::new(0.0, 0.53, -0.15);
                    assert!(
                        (final_attachment.with_y(0.0) - attachment.with_y(0.0)).length() < 1e-6,
                        "{name}: neck attachment must retain its body-fitted opening"
                    );
                    let collar_limit = 0.565 + 2.0 * (LAYER_GAP + THICKNESS + ROOF_SLOPE_SLACK);
                    assert!(final_attachment.y <= collar_limit + 1e-6,
                        "{name}: final collar exceeds two exact layer clearances: {final_attachment:?}, limit={collar_limit}");
                }
                assert!(
                    cloak
                        .points
                        .iter()
                        .zip(first)
                        .all(|(a, b)| a.distance(*b) < 1e-6),
                    "neutral refitting must not accumulate roof height"
                );
                let offset = Vec3::new(0.0, 0.53, -0.15);
                let scene = cloak.scene.as_ref().unwrap();
                for column in 0..COLUMNS {
                    let [a, b, c, d] = [
                        index(ROOF_ROWS, column),
                        index(ROOF_ROWS, column + 1),
                        index(YOKE_ROWS, column),
                        index(YOKE_ROWS, column + 1),
                    ]
                    .map(|i| (cloak.points[i] + offset) / anatomy::HIP_SCALE);
                    for face in [[a, b, c], [b, d, c]] {
                        for weights in [
                            [1.0, 1.0, 1.0],
                            [2.0, 1.0, 1.0],
                            [1.0, 2.0, 1.0],
                            [1.0, 1.0, 2.0],
                        ] {
                            let p = (face[0] * weights[0]
                                + face[1] * weights[1]
                                + face[2] * weights[2])
                                / weights.into_iter().sum::<f32>();
                            assert!(scene.body.distance(p) > 0.001 && rendered_clearance(cloak.arm_surface.as_ref().unwrap(), p * anatomy::HIP_SCALE) > 0.001,
                                "{name}: descending turnover penetrates actual body/arm support at {p:?}");
                        }
                    }
                }
                let arms = cloak.arm_surface.as_ref().unwrap();
                let top = arms.bottom + arms.height;
                let roof = &cloak.points[..(YOKE_ROWS + 1) * (COLUMNS + 1)];
                // Two cloth layers may reserve their lining and gap, but not
                // create a raised rigid box above the actual shoulder support.
                let (peak_index, peak) = roof
                    .iter()
                    .enumerate()
                    .max_by(|(_, a), (_, b)| a.y.total_cmp(&b.y))
                    .unwrap();
                eprintln!(
                    "neutral {name} cloth diagnostics: {}",
                    cloak.contact_diagnostics().unwrap()
                );
                let peak_height = peak.y + offset.y;
                let rest_height = cloak.rest[peak_index].y + offset.y;
                let cap_radius = arms
                    .triangles
                    .iter()
                    .flatten()
                    .filter(|p| p.y >= 0.47 - THICKNESS)
                    .map(|p| p.with_y(0.0).length())
                    .fold(0.0f32, f32::max);
                let radial_limit = cap_radius + 2.0 * (LAYER_GAP + THICKNESS) + THICKNESS;
                for row in 0..=YOKE_ROWS {
                    for column in 0..=COLUMNS {
                        let at = cloak.points[index(row, column)] + offset;
                        assert!(at.with_y(0.0).length()<=radial_limit,
                        "{name}: yoke row overexpanded beyond actual cap: row={row} column={column} point={at:?} reference={:?} cap_radius={cap_radius} limit={radial_limit}",cloak.contact_reference[index(row,column)]+offset);
                    }
                }
                assert!(
                    peak_height <= top + 0.12,
                    "{name}: yoke exceeds two-layer allowance: inner={:?}, vertex={peak_index} row={} column={}, actual_height={peak_height}, support_top={top}, limit={}, authored_height={rest_height}, spawned_peak={spawned_peak}, point={peak:?}",
                    cloak.inner_layer, peak_index / (COLUMNS + 1), peak_index % (COLUMNS + 1), top + 0.12
                );
                for column in [COLUMNS / 4, COLUMNS * 3 / 4] {
                    let neck = cloak.points[index(0, column)];
                    let shoulder = cloak.points[index(YOKE_ROWS, column)];
                    let authored_shoulder = cloak.rest[index(YOKE_ROWS, column)];
                    let crest = cloak.points[index(ROOF_ROWS, column)];
                    assert!(
                        (shoulder.y - authored_shoulder.y).abs() < 1e-6,
                        "{name}: descending turnover must not inherit roof lift: {shoulder:?}"
                    );
                    let cap_radius = arms
                        .triangles
                        .iter()
                        .flatten()
                        .filter(|p| p.y >= authored_shoulder.y + offset.y - THICKNESS)
                        .map(|p| p.with_y(0.0).length())
                        .fold(0.0f32, f32::max);
                    // Two complete cloth clearances plus the small angular
                    // facet allowance; never hide overexpansion by clipping.
                    let radial_limit = cap_radius + 2.0 * (LAYER_GAP + THICKNESS) + THICKNESS;
                    for row in [ROOF_ROWS, YOKE_ROWS] {
                        let at = cloak.points[index(row, column)] + offset;
                        assert!(at.with_y(0.0).length() <= radial_limit,
                            "{name}: turnover overexpanded beyond actual cap: row={row}, point={at:?}, cap_radius={cap_radius}, limit={radial_limit}");
                    }
                    assert!(crest.y - shoulder.y > 0.025,
                        "{name}: descending cap side lost its drop: crest={crest:?}, edge={shoulder:?}");
                    assert!(neck.y - shoulder.y > 0.025,
                        "{name}: neck-to-shoulder roof must descend, not form a table or raised rim: neck={neck:?}, shoulder={shoulder:?}, authored_shoulder={authored_shoulder:?}");
                }
            }
        }
    }

    #[test]
    fn yoke_support_is_local_and_arm_release_preserves_neck_attachment() {
        let distant_fist = [
            Vec3::new(-0.4, 0.8, 0.4),
            Vec3::new(-0.5, 0.8, 0.5),
            Vec3::new(-0.4, 0.9, 0.5),
        ];
        let surface = LayerSurface::from_triangles(vec![distant_fist]);
        let neck = [
            Vec3::new(-0.05, 0.56, 0.05),
            Vec3::new(-0.08, 0.56, 0.05),
            Vec3::new(-0.08, 0.55, 0.08),
            Vec3::new(-0.05, 0.55, 0.08),
        ];
        assert!(
            surface
                .roof_plane([neck[0], neck[1], neck[2]], Vec3::Y)
                .is_none(),
            "a remote hand in the neck's angular sector cannot lift the attachment"
        );
        assert!(surface.roof_plane(distant_fist, Vec3::Y).is_some());
        let opening = 1.3;
        let left = Vec3::new(
            -(opening * 0.5f32).sin() * 0.4,
            0.4,
            (opening * 0.5f32).cos() * 0.4,
        );
        assert_eq!(
            release_front(
                left,
                0,
                0,
                opening,
                FreeEdges {
                    strength: [1.0, 0.0],
                    angles: [PI * 0.68; 2]
                }
            ),
            left
        );
        assert_eq!(
            release_front(left, 7, 0, opening, FreeEdges::default()),
            left
        );
        let released = release_front(
            left,
            7,
            0,
            opening,
            FreeEdges {
                strength: [1.0, 0.0],
                angles: [PI * 0.68; 2],
            },
        );
        assert!(
            released.z < -0.20,
            "free edge must sweep behind the fist, not form a sleeve around it"
        );
        assert!((released.with_y(0.0).length() - left.with_y(0.0).length()).abs() < 1e-6);
        // Quat-vector multiplication reconstructs all components; even a
        // pure Y rotation may round the unchanged Y coordinate by one ULP.
        assert!((released.y - left.y).abs() <= 4.0 * f32::EPSILON * left.y.abs().max(1.0));
    }

    #[test]
    fn contact_candidates_are_unique_and_stack_clipping_matches_exhaustive_oracle() {
        let triangles: Vec<_> = (0..80)
            .map(|i| {
                let angle = i as f32 * 0.37;
                [
                    Vec3::new(angle.sin() * 0.5, -0.4, angle.cos() * 0.4),
                    Vec3::new((angle + 0.1).sin() * 0.5, 0.8, (angle + 0.1).cos() * 0.4),
                    Vec3::new((angle + 0.2).sin() * 0.5, 0.2, (angle + 0.2).cos() * 0.4),
                ]
            })
            .collect();
        let surface = LayerSurface::from_triangles(triangles.clone());
        assert_eq!(surface.candidates(-0.4, 0.8).count(), triangles.len());
        assert!(
            surface.bins.iter().map(Vec::len).sum::<usize>() > triangles.len() * 32,
            "fixture must exercise former repeated-bin work"
        );
        for low in [-0.4, -0.1, 0.3] {
            for high in [0.35, 0.8] {
                for angle in [0.0, 0.9, 2.8] {
                    let normal = Vec3::new(f32::sin(angle), 0.0, f32::cos(angle));
                    let lower = Vec3::new((angle - 0.16).cos(), 0.0, -(angle - 0.16).sin());
                    let upper = Vec3::new(-(angle + 0.16).cos(), 0.0, (angle + 0.16).sin());
                    let planes = [
                        (Vec3::Y, low),
                        (Vec3::NEG_Y, -high),
                        (lower, -THICKNESS),
                        (upper, -THICKNESS),
                    ];
                    let mut expected: Option<f32> = None;
                    for triangle in &triangles {
                        let mut polygon = triangle.to_vec();
                        for (plane, offset) in planes {
                            polygon = clip_layer_polygon(&polygon, plane, offset);
                        }
                        for p in polygon {
                            let v = p.dot(normal);
                            expected = Some(expected.map_or(v, |old| old.max(v)));
                        }
                    }
                    let actual = surface.support_plane(normal, low, high, lower, upper);
                    match (actual, expected) {
                        (Some(a), Some(b)) => assert!((a - b).abs() < 1e-6),
                        (None, None) => {}
                        _ => panic!("candidate rejection changed contact support"),
                    }
                }
            }
        }
    }

    #[test]
    fn radial_support_bvh_rejects_remote_geometry_without_contact_lod() {
        let triangles: Vec<_> = (0..1024)
            .map(|i| {
                let angle = TAU * i as f32 / 1024.0;
                let radial = Vec3::new(angle.sin(), 0.0, angle.cos());
                [
                    radial * 0.4,
                    radial * 0.42 + Vec3::Y * 0.3,
                    Quat::from_rotation_y(0.003) * radial * 0.4 + Vec3::Y * 0.15,
                ]
            })
            .collect();
        let surface = LayerSurface::from_triangles(triangles);
        let planes = [
            (Vec3::Y, 0.1),
            (Vec3::NEG_Y, -0.2),
            (Vec3::X, -0.015),
            (Vec3::NEG_X, -0.015),
        ];
        let count = surface.clipped_candidates(&planes).count();
        assert!(
            count < surface.candidates(0.1, 0.2).count() / 8,
            "angular BVH must reject distant geometry before exact clipping: {count}"
        );
        let mut candidates = surface.clipped_candidates(&planes);
        candidates.support_floor = Some((Vec3::Z, 0.5));
        assert_eq!(
            candidates.count(),
            0,
            "a support bound above the whole mesh must reject every subtree"
        );
        for normal in [Vec3::Z, (Vec3::Z + Vec3::Y).normalize(), Vec3::NEG_Z] {
            let expected = surface
                .triangles
                .iter()
                .filter_map(|&t| clipped_support(t, normal, &planes))
                .reduce(f32::max);
            let mut actual: Option<f32> = None;
            let mut query = surface.clipped_candidates(&planes);
            while let Some(&triangle) = query.next() {
                if let Some(value) = clipped_support(triangle, normal, &planes) {
                    actual = Some(actual.map_or(value, |old| old.max(value)));
                    query.support_floor = actual.map(|v| (normal, v));
                }
            }
            assert!((actual.unwrap() - expected.unwrap()).abs() < 1e-6);
        }
    }

    #[test]
    fn mantle_cell_contains_inner_peak_missed_by_both_vertex_rays() {
        let offset = Vec3::new(0.0, 0.53, -0.15);
        let triangle = [
            Vec3::new(0.004, 0.34, 0.45),
            Vec3::new(0.010, 0.34, 0.45),
            Vec3::new(0.007, 0.36, 0.45),
        ];
        let mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            triangle.map(|p| (p - offset).to_array()).to_vec(),
        )
        .with_inserted_indices(Indices::U32(vec![0, 1, 2]));
        let support = LayerSurface::new(&mesh, Quat::IDENTITY);
        let origin = Vec3::Y * 0.35;
        for x in [0.0, 0.0125] {
            assert!(
                support
                    .radius(origin, Vec3::new(x, 0.0, 0.35).normalize())
                    .is_none(),
                "fixture peak must lie between vertex rays"
            );
        }
        let mut points: Vec<_> = (0..GRID)
            .map(|i| {
                let x = (i % (COLUMNS + 1)) as f32 / COLUMNS as f32 * 0.6 - 0.3;
                let y = 0.6 - (i / (COLUMNS + 1)) as f32 / ROWS as f32 * 0.5;
                Vec3::new(x, y, 0.35) - offset
            })
            .collect();
        fit_layer_cells(&mut points, Quat::IDENTITY, &support);
        let mut oracle = Builder::default();
        oracle.tri(triangle, Vec3::ONE);
        let mut witnesses = 0;
        for row in 0..ROWS {
            for col in 0..COLUMNS {
                let [a, b, c, d] = [
                    index(row, col),
                    index(row, col + 1),
                    index(row + 1, col),
                    index(row + 1, col + 1),
                ]
                .map(|i| points[i] + offset);
                for face in [[a, b, c], [b, d, c]] {
                    let center = (face[0] + face[1] + face[2]) / 3.0;
                    let origin = Vec3::Y * center.y;
                    if let Some(hit) = oracle.outer_surface(origin, center.with_y(0.0).normalize())
                    {
                        witnesses += 1;
                        assert!(
                            center.distance(origin) > hit.distance(origin) + 0.001,
                            "whole mantle face must clear the inter-ray peak"
                        );
                    }
                }
            }
        }
        assert!(
            witnesses > 0,
            "interior contact fixture must exercise a face"
        );
    }

    #[test]
    fn study_capes_add_complete_hand_contact_chains_without_changing_legacy_profiles() {
        let base = Look::of(CreatureModel::Human, 17);
        let (_, native, _, _) = super::super::rogue_tests::spawn_one(&base);
        assert!(
            native
                .visual_contacts
                .iter()
                .all(|part| part.retained_triangle_count() == 0),
            "legacy hair contacts must not retain study-cloth triangle storage"
        );
        for (name, added) in [("cloak", 0), ("mummy-wrapping", 0), ("hooded-cloak", 2)] {
            let mut equipment = Equipment::default();
            equipment.select(name).unwrap();
            let (_, mut rig, _, _) =
                super::super::rogue_tests::spawn_one(&base.clone().equipped(equipment));
            assert_eq!(
                rig.visual_contacts.len(),
                native.visual_contacts.len() + added
            );
            assert!(
                rig.visual_contacts
                    .iter()
                    .all(|part| (part.retained_triangle_count() > 0) == (added == 2)),
                "only study capes retain the actual rendered contact mesh"
            );
            if added == 2 {
                let neutral = super::super::visual_contact::posed(&rig.visual_contacts, |joint| {
                    Some(Affine3A::from_translation(rig.rest[joint as usize]))
                });
                let posed = super::super::visual_contact::posed(&rig.visual_contacts, |joint| {
                    let rotation = if matches!(joint, J::ArmL | J::ForeL) {
                        Quat::from_rotation_x(0.65)
                    } else {
                        Quat::IDENTITY
                    };
                    Some(Affine3A::from_rotation_translation(
                        rotation,
                        rig.rest[joint as usize],
                    ))
                });
                assert_ne!(neutral, posed);
                let cloak = &mut rig.cloaks[0];
                cloak.last_rotation = Some(Quat::IDENTITY);
                cloak.set_contacts(posed);
                assert!(
                    cloak.last_rotation.is_none(),
                    "posed arms invalidate a still cape"
                );
            }
        }
    }

    #[test]
    fn captured_tick_93_backward_strike_releases_hand_without_losing_back_cloth() {
        // Same 50 Hz action event at tick 85 / capture at 93 as the review
        // fixture. Run the production pose system, not guessed arm rotations.
        for name in ["hooded-cloak", "leather-cloak", "ornamental-cope"] {
            let mut equipment = Equipment::default();
            if name == "ornamental-cope" {
                equipment.select("plate-mail").unwrap();
            }
            equipment.select(name).unwrap();
            let look = Look::with_choice(CreatureModel::Human, 18, Some(Variant::default()))
                .equipped(equipment);
            let _stage = AuditContextGuard::enter(
                serde_json::json!({"actor":name,"fixture":"captured_tick_93","stage":"spawn"}),
            );
            let (mut world, mut rig, _, meshes) = super::super::rogue_tests::spawn_one(&look);
            for cloak in &mut rig.cloaks {
                cloak.audit_context = serde_json::json!({"actor":name,"fixture":"captured_tick_93","stage":"pose","tick":93});
            }
            world.insert_resource(meshes);
            let mut anim = Anim::new(18, Vec3::ZERO);
            anim.yaw = 0.0;
            anim.time = 93.0 / 50.0;
            let phase = (85..93).fold(0.0, |time, _| time + 0.02 / Move::Strike.duration());
            assert!((phase - 0.258_064_5).abs() < 1e-7);
            anim.action = Some((Move::Strike, phase));
            let actor = world
                .spawn((rig, anim, Transform::default(), super::super::PoseRefresh))
                .id();
            let mut schedule = Schedule::default();
            schedule.add_systems(super::super::refresh_pose);
            schedule.run(&mut world);
            let rig = world.get::<Rig>(actor).unwrap();
            let hand = [J::ArmR, J::ForeR, J::HandR]
                .into_iter()
                .fold(Affine3A::IDENTITY, |pose, joint| {
                    pose * world
                        .get::<Transform>(rig.joint(joint).unwrap())
                        .unwrap()
                        .compute_affine()
                })
                .transform_point3(Vec3::ZERO);
            assert!(
                hand.z < -0.4 && hand.x < -0.3 && hand.y > 0.2,
                "must reproduce the captured rearward wind-up, not a forward strike: {hand:?}"
            );
            let meshes = world.resource::<Assets<Mesh>>();
            let head_surface = rendered_head_surface(&world, rig, meshes);
            for cloak in &rig.cloaks {
                eprintln!(
                    "tick93 cloth diagnostics: {}",
                    cloak.contact_diagnostics().unwrap()
                );
                let release = cloak
                    .arm_surface
                    .as_ref()
                    .unwrap()
                    .free_edges(cloak.neutral_reach);
                assert_eq!(
                    release.strength[0], 1.0,
                    "rearward hand must activate release"
                );
                assert!(
                    release.angles[0] > hand.x.abs().atan2(hand.z) + 0.03,
                    "edge must sweep beyond the actual hand, not stop at a fixed forward target"
                );
                let rotation = world.get::<Transform>(cloak.entity).unwrap().rotation;
                let offset = Vec3::new(0.0, 0.53, -0.15);
                for row in YOKE_ROWS + 1..=ROWS {
                    let edge = rotation * cloak.points[index(row, 0)] + offset;
                    assert!(
                        edge.x.abs().atan2(edge.z) >= release.angles[0] - 0.005,
                        "safety fitting must not return the edge over the fist: {edge:?}"
                    );
                }
                let back = rotation * cloak.points[index(ROWS, COLUMNS / 2)] + offset;
                assert!(back.z < -0.10, "retain continuous hanging back cloth");
                let mesh = meshes.get(&cloak.mesh).unwrap();
                assert_supported_attachment_clear(cloak, rotation, mesh, &head_surface);
                let vertices = positions(mesh);
                let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                for tri in indices.as_chunks::<3>().0 {
                    let face = tri.map(|i| (rotation * vertices[i] + offset) / anatomy::HIP_SCALE);
                    for weights in [
                        [1.0, 1.0, 1.0],
                        [2.0, 1.0, 0.0],
                        [0.0, 2.0, 1.0],
                        [1.0, 0.0, 2.0],
                    ] {
                        let p =
                            (face[0] * weights[0] + face[1] * weights[1] + face[2] * weights[2])
                                / 3.0;
                        assert!(rendered_clearance(cloak.arm_surface.as_ref().unwrap(), p * anatomy::HIP_SCALE)>0.001,
                            "actual strike cloth faces/lining must clear retained arms: {name}, {p:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn posed_head_snapshots_invalidate_only_dependent_mantles() {
        let mut equipment = Equipment::default();
        equipment.select("leather-cloak").unwrap();
        let look = Look::with_choice(CreatureModel::Human, 18, Some(Variant::default()))
            .equipped(equipment);
        let (_, mut rig, _, _) = super::super::rogue_tests::spawn_one(&look);
        let posed = Arc::new(ContactSurface::new(
            super::super::visual_contact::posed_triangles(&rig.cloth_head_contacts, |joint| {
                Some(Affine3A::from_rotation_translation(
                    Quat::from_rotation_y(0.1),
                    rig.rest[joint as usize],
                ))
            }),
        ));
        assert_eq!(rig.cloaks.len(), 2);
        rig.cloaks[0].set_head_surface(posed.clone());
        assert!(rig.cloaks[0].head_surface.is_none());
        rig.cloaks[1].set_head_surface(posed.clone());
        assert!(rig.cloaks[1].last_rotation.is_none());
        let surface = rig.cloaks[1].head_surface.clone().unwrap();
        rig.cloaks[1].last_rotation = Some(Quat::IDENTITY);
        rig.cloaks[1].set_head_surface(posed);
        assert!(Arc::ptr_eq(
            &surface,
            rig.cloaks[1].head_surface.as_ref().unwrap()
        ));
        assert_eq!(rig.cloaks[1].last_rotation, Some(Quat::IDENTITY));
    }

    #[test]
    fn posed_cope_triangles_clear_plate_arms_and_scaled_hands() {
        let mut equipment = Equipment::default();
        equipment.select("plate-mail").unwrap();
        equipment.select("ornamental-cope").unwrap();
        let look = Look::with_choice(
            CreatureModel::Human,
            17,
            Some(Variant {
                woman: true,
                build: 2,
                ..default()
            }),
        )
        .equipped(equipment);
        let _stage = AuditContextGuard::enter(
            serde_json::json!({"actor":"ornamental-cope+plate-mail","fixture":"posed_cope","stage":"spawn"}),
        );
        let (world, mut rig, _, mut meshes) = super::super::rogue_tests::spawn_one(&look);
        let head_surface = rendered_head_surface(&world, &rig, &meshes);
        assert_eq!(
            rig.visual_contacts.len(),
            6,
            "complete upper/fore/hand meshes"
        );
        let mut released_witnesses = 0;
        for angle in [0.0, -1.15, 0.85] {
            let frame = |joint| {
                let rotation = match joint {
                    J::ArmR => Quat::from_rotation_x(angle) * Quat::from_rotation_z(-0.25),
                    J::ForeR => Quat::from_rotation_x(angle * 0.65),
                    _ => Quat::IDENTITY,
                };
                Some(Affine3A::from_scale_rotation_translation(
                    if matches!(joint, J::HandL | J::HandR) {
                        Vec3::splat(1.22)
                    } else {
                        Vec3::ONE
                    },
                    rotation,
                    rig.rest[joint as usize],
                ))
            };
            let contacts = super::super::visual_contact::posed(&rig.visual_contacts, frame);
            let surface = Arc::new(ContactSurface::new(
                super::super::visual_contact::posed_triangles(&rig.visual_contacts, frame),
            ));
            let head_contacts = Arc::new(ContactSurface::new(
                super::super::visual_contact::posed_triangles(&rig.cloth_head_contacts, frame),
            ));
            for cloak in &mut rig.cloaks {
                cloak.set_head_surface(head_contacts.clone());
                cloak.audit_context = serde_json::json!({"actor":"ornamental-cope+plate-mail","fixture":"posed_cope","stage":"pose","angle":angle});
                cloak.set_contacts(contacts.clone());
                cloak.set_contact_surface(surface.clone());
            }
            let rotation = Quat::from_euler(EulerRot::XYZ, 0.12, -0.18, 0.09);
            place_layers(&mut rig.cloaks, |_| Some(rotation), &mut meshes);
            let mut tested = 0usize;
            for cloak in &rig.cloaks {
                assert_supported_attachment_clear(
                    cloak,
                    rotation,
                    meshes.get(&cloak.mesh).unwrap(),
                    &head_surface,
                );
                let actual = cloak.arm_surface.as_ref().unwrap();
                assert!(actual.closed_faces.get_or_init(||closed_components(&actual.triangles)).iter().any(Option::is_some),
                    "actual arm fixture must retain closed components for interior penetration checks");
                let offset = Vec3::new(0.0, 0.53, -0.15);
                for column in 0..=COLUMNS {
                    let neck = rotation * cloak.points[index(0, column)] + offset;
                    let rest = cloak.rest[index(0, column)] + offset;
                    assert!(
                        neck.with_y(0.0).length() <= rest.with_y(0.0).length() + 0.015,
                        "neck attachment may not expand into a shoulder loop"
                    );
                }
                if let Some(surface) = &cloak.arm_surface {
                    let release = surface.free_edges(cloak.neutral_reach);
                    for (side, &strength) in release.strength.iter().enumerate() {
                        if strength < 1.0 {
                            continue;
                        }
                        let columns = if side == 0 {
                            0..=COLUMNS / 4
                        } else {
                            COLUMNS * 3 / 4..=COLUMNS
                        };
                        for row in YOKE_ROWS + 1..=ROWS {
                            for col in columns.clone() {
                                let at = rotation * cloak.points[index(row, col)] + offset;
                                assert!(at.z < 0.05, "lifted hand must emerge through the released front opening, not an enveloping pouch: {at:?}");
                            }
                        }
                        released_witnesses += 1;
                    }
                }
                let mesh = meshes.get(&cloak.mesh).unwrap();
                let vertices = positions(mesh);
                let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                for tri in indices.as_chunks::<3>().0 {
                    let face = tri.map(|i| {
                        (rotation * vertices[i] + Vec3::new(0.0, 0.53, -0.15)) / anatomy::HIP_SCALE
                    });
                    for a in 0..=4 {
                        for b in 0..=4 - a {
                            let p = (face[0] * a as f32
                                + face[1] * b as f32
                                + face[2] * (4 - a - b) as f32)
                                / 4.0;
                            let gap = rendered_clearance(
                                cloak.arm_surface.as_ref().unwrap(),
                                p * anatomy::HIP_SCALE,
                            );
                            if tested.is_multiple_of(257) {
                                let exhaustive = rendered_clearance_checked(
                                    cloak.arm_surface.as_ref().unwrap(),
                                    p * anatomy::HIP_SCALE,
                                    true,
                                );
                                assert!((gap-exhaustive).abs()<1e-6,
                                    "posed actual-mesh broadphase disagrees with exhaustive clearance at {p:?}");
                            }
                            assert!(gap > 0.001, "posed cloth triangle penetrates actual arm profile: angle={angle}, p={p:?}, gap={gap}");
                            tested += 1;
                        }
                    }
                }
            }
            assert!(
                tested > 10000,
                "both linings, cell interiors, edges and boundary faces"
            );
        }
        assert!(
            released_witnesses > 0,
            "posed fixture must exercise a forward-released arm opening"
        );
    }

    #[test]
    fn study_yoke_crosses_shoulder_from_neck_and_contact_clears_deltoid() {
        let r = study_recipe(outfit::outerwear::OuterwearAppearance::HoodedCloak);
        let origin = Vec3::new(0.0, 0.53, -0.15);
        let neck = r.point(0, COLUMNS / 4) + origin;
        let edge = r.point(YOKE_ROWS, COLUMNS / 4) + origin;
        assert!((neck.y - 0.565).abs() < 1e-6);
        let neck_query = neck / anatomy::HIP_SCALE;
        assert!(
            r.support.distance(neck_query) >= LAYER_GAP + 0.008,
            "collar opening must clear the actual retained neck geometry"
        );
        assert!(
            neck.with_y(0.0).length() < edge.with_y(0.0).length(),
            "collar must remain inside the shoulder turnover"
        );
        assert!(
            edge.x.abs() > neck.x.abs() + 0.17,
            "continuous shoulder slope"
        );
        let mut support = crate::sdf::Sculpt::default();
        support.add(
            crate::sdf::Shape::Ellipsoid(Vec3::new(0.28, 0.50, 0.0), Vec3::new(0.10, 0.09, 0.09)),
            0.0,
        );
        let top = fit_wrapped_contact(
            Vec3::new(0.28, 0.49, 0.0) * anatomy::HIP_SCALE,
            true,
            &|p| support.distance(p),
        ) / anatomy::HIP_SCALE;
        assert!(
            top.y >= 0.61,
            "yoke clears shoulder cap instead of beginning below it"
        );
        let side = fit_wrapped_contact(
            Vec3::new(0.29, 0.50, 0.0) * anatomy::HIP_SCALE,
            false,
            &|p| support.distance(p),
        ) / anatomy::HIP_SCALE;
        assert!(side.x >= 0.40, "opaque side clears retained arm surface");
    }

    #[test]
    fn cope_has_a_blue_orphrey_between_gold_edges() {
        let mut r = study_recipe(outfit::outerwear::OuterwearAppearance::OrnamentalCope);
        let gold = Vec3::new(0.72, 0.49, 0.18);
        r.embroidery = Some(gold);
        assert_eq!(study_pigment(&r, ROWS - 2, 12), r.lining);
        assert_eq!(study_pigment(&r, ROWS, 12), gold);
        assert_eq!(study_pigment(&r, ROWS - 3, 12), gold);
        assert_ne!(r.lining, r.exterior);
    }

    #[test]
    fn mantle_clears_main_cloak_at_shared_heights_in_neutral_walk_and_turn() {
        for name in ["leather-cloak", "ornamental-cope"] {
            let look = Look::with_choice(
                CreatureModel::Human,
                7,
                Some(Variant {
                    woman: true,
                    build: 2,
                    ..default()
                }),
            );
            let mut equipment = Equipment::default();
            equipment.select(name).unwrap();
            let (_, mut rig, _, mut meshes) =
                super::super::rogue_tests::spawn_one(&look.equipped(equipment));
            assert_eq!(rig.cloaks.len(), 2);
            let inner = rig.cloaks[0].entity;
            assert_eq!(rig.cloaks[1].inner_layer, Some(inner));
            // The mantle and cape deliberately have different angular lag,
            // including an inner-only move to exercise dependency invalidation.
            for (cape, mantle) in [
                (Quat::IDENTITY, Quat::IDENTITY),
                (Quat::from_rotation_x(0.28), Quat::from_rotation_x(0.13)),
                (
                    Quat::from_euler(EulerRot::XYZ, 0.18, 0.27, -0.20),
                    Quat::from_euler(EulerRot::XYZ, 0.08, -0.10, 0.14),
                ),
                (
                    Quat::from_euler(EulerRot::XYZ, -0.12, -0.23, 0.16),
                    Quat::from_euler(EulerRot::XYZ, 0.08, -0.10, 0.14),
                ),
                (Quat::IDENTITY, Quat::IDENTITY),
            ] {
                place_layers(
                    &mut rig.cloaks,
                    |entity| Some(if entity == inner { cape } else { mantle }),
                    &mut meshes,
                );
                let offset = Vec3::new(0.0, 0.53, -0.15);
                // Independent triangle-ray oracle uses the emitted inner mesh,
                // rather than the solver's binned contact representation.
                let mesh = meshes.get(&rig.cloaks[0].mesh).unwrap();
                let inner_points = positions(mesh);
                let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                let mut support = Builder::default();
                for face in indices.as_chunks::<3>().0 {
                    support.tri(face.map(|i| cape * inner_points[i] + offset), Vec3::ONE);
                }
                let mesh = meshes.get(&rig.cloaks[1].mesh).unwrap();
                let points = positions(mesh);
                let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                let mut contacts = 0;
                for (face_index, face) in indices.as_chunks::<3>().0.iter().enumerate() {
                    let tri = face.map(|i| mantle * points[i] + offset);
                    let at = (tri[0] + tri[1] + tri[2]) / 3.0;
                    // Validate the shared drape and the descending turnover
                    // band, including both emitted lining/exterior faces.
                    let turnover_face = face_index / (COLUMNS * 4) == ROOF_ROWS;
                    if !(0.23..0.44).contains(&at.y) && !turnover_face {
                        continue;
                    }
                    let origin = Vec3::Y * at.y;
                    let direction = (at - origin).normalize();
                    if let Some(hit) = support.outer_surface(origin, direction) {
                        contacts += 1;
                        assert!(at.distance(origin) > hit.distance(origin)+0.001,
                            "{name}: mantle triangle intersects cape at {at:?}, support {hit:?}, poses {cape:?}/{mantle:?}");
                    }
                }
                assert!(contacts > 100, "shared-height mesh contact witness");
            }
        }
    }

    #[test]
    fn legacy_back_sheet_coordinates_are_unchanged_without_a_cut() {
        let body = HumanoidBody::resolve(&Look::of(CreatureModel::Human, 7));
        let recipe = Recipe::new(body, Vec3::ONE, Vec3::ONE, false);
        assert_eq!(recipe.cut, None);
        let center = recipe.point(ROWS, COLUMNS / 2);
        assert!((center - Vec3::new(0.0, -1.098, -0.137)).length() < 1e-5);
    }

    #[test]
    fn study_wraps_reach_both_front_edges_and_leave_a_real_opening() {
        use outfit::outerwear::OuterwearAppearance as A;
        for kind in A::ALL {
            if !matches!(kind.recipe().cut, outfit::outerwear::OuterwearCut::Drape(_)) {
                continue;
            }
            let r = study_recipe(kind);
            let origin = Vec3::new(0.0, 0.53, -0.15);
            let left = r.point(ROWS, 0) + origin;
            let right = r.point(ROWS, COLUMNS) + origin;
            let back = r.point(ROWS, COLUMNS / 2) + origin;
            assert!(
                left.x < -0.05 && right.x > 0.05,
                "{kind:?} has an open front"
            );
            assert!(left.z > 0.02 && right.z > 0.02 && back.z < -0.10);
            for row in 0..=ROWS {
                for column in 0..=COLUMNS {
                    assert!(r.point(row, column).is_finite());
                }
            }
        }
        let short = study_recipe(A::CoarseMantelet);
        let long = study_recipe(A::OperaCloak);
        assert!(long.point(ROWS, COLUMNS / 2).y < short.point(ROWS, COLUMNS / 2).y - 0.75);
        assert!(long.point(ROWS, COLUMNS * 3 / 4).x > short.point(ROWS, COLUMNS * 3 / 4).x + 0.07);
    }

    #[test]
    fn angular_wrap_clears_retained_front_back_and_side_armour() {
        use outfit::outerwear::OuterwearAppearance as A;
        let mut r = study_recipe(A::OrnamentalCope);
        let mut armour = r.support.clone();
        armour.inflate(0.04);
        r.reserve_body_layer(&armour);
        for row in 0..=6 {
            for column in 0..=COLUMNS {
                let at = (r.point(row, column) + Vec3::new(0.0, 0.53, -0.15)) / anatomy::HIP_SCALE;
                assert!(
                    armour.distance(at) >= 0.017,
                    "outer lining clearance at {row}/{column}"
                );
            }
        }
    }

    #[test]
    fn wraith_cloak_recipe_copies_and_equipment_replaces_then_restores_it() {
        let native = Look::with_choice(
            CreatureModel::Wraith,
            42,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        let (_, native_rig, _, native_meshes) = super::super::rogue_tests::spawn_one(&native);
        assert_eq!(native_rig.cloaks.len(), 1);
        let original = positions(native_meshes.get(&native_rig.cloaks[0].mesh).unwrap());
        let colors = |mesh: &Mesh| match mesh.attribute(Mesh::ATTRIBUTE_COLOR).unwrap() {
            VertexAttributeValues::Float32x4(values) => values.clone(),
            _ => panic!("cloak colors"),
        };
        let original_colors = colors(native_meshes.get(&native_rig.cloaks[0].mesh).unwrap());
        let mut equipment = Equipment::default();
        equipment.select("cloak").unwrap();
        let equipped = native.clone().equipped(equipment);
        let (_, rig, _, meshes) = super::super::rogue_tests::spawn_one(&equipped);
        assert_eq!(
            rig.cloaks.len(),
            1,
            "back item must replace the native cloak"
        );
        assert_ne!(
            positions(meshes.get(&rig.cloaks[0].mesh).unwrap()),
            original
        );
        let restored = equipped.equipped(Equipment::default());
        let (_, rig, _, meshes) = super::super::rogue_tests::spawn_one(&restored);
        assert_eq!(rig.cloaks.len(), 1);
        assert_eq!(
            positions(meshes.get(&rig.cloaks[0].mesh).unwrap()),
            original
        );
        let recipe = outfit::Outfit::native(&native);
        assert!(recipe.cloak.unwrap().ragged);
        assert!(
            !outfit::Composition::resolve(&native.clone().equipped(equipment))
                .outfit
                .cloak
                .unwrap()
                .ragged
        );
        let mut copied = native.clone();
        copied.outfit = Some(recipe);
        let (_, rig, _, meshes) = super::super::rogue_tests::spawn_one(&copied);
        assert_eq!(rig.cloaks.len(), 1, "copied recipe lost its cloak");
        assert_eq!(
            positions(meshes.get(&rig.cloaks[0].mesh).unwrap()),
            original
        );
        let mut recipient = Look::with_choice(
            CreatureModel::Human,
            91,
            Some(Variant {
                woman: true,
                build: 2,
                ..default()
            }),
        );
        recipient.outfit = Some(recipe);
        let (_, mut rig, _, mut meshes) = super::super::rogue_tests::spawn_one(&recipient);
        assert_eq!(rig.cloaks.len(), 1, "cross-body recipe lost its cloak");
        let cloak = &mut rig.cloaks[0];
        assert_eq!(
            colors(meshes.get(&cloak.mesh).unwrap()),
            original_colors,
            "transferred cloak must retain its palette"
        );
        let before = positions(meshes.get(&cloak.mesh).unwrap());
        place(cloak, Quat::from_rotation_x(0.3), &mut meshes);
        let after = positions(meshes.get(&cloak.mesh).unwrap());
        assert_ne!(before, after, "transferred cloak must animate");
        assert_eq!(before.len(), after.len());
        assert!(after.iter().all(|p| p.is_finite()));
    }

    #[test]
    fn gathered_yoke_fits_the_resolved_shoulders_instead_of_floating_behind_them() {
        for model in [
            CreatureModel::Vampire,
            CreatureModel::Troll,
            CreatureModel::Zombie,
        ] {
            for woman in [false, true] {
                for build in 0..3 {
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
                    let recipe = Recipe::new(body, Vec3::ONE, Vec3::ZERO, false);
                    let support =
                        geometry::torso_shape(body, Some(geometry::TorsoMaterial::Leather), false);
                    let edge = recipe.point(0, COLUMNS);
                    let shoulder = body.shoulder(1.0).x;
                    assert!(
                        (0.85..1.05).contains(&(edge.x / shoulder)),
                        "{model:?} {woman} {build}: yoke must follow the wearer's shoulder span"
                    );
                    for column in 0..=COLUMNS {
                        let at = (recipe.point(0, column) + Vec3::new(0.0, 0.53, -0.15))
                            / anatomy::HIP_SCALE;
                        let gap = support.distance(at);
                        assert!((-0.005..0.03).contains(&gap), "{model:?} {woman} {build}: unsupported shoulder cloth at {at:?}, gap {gap}");
                    }
                }
            }
        }
    }

    #[test]
    fn shoulder_rows_stay_attached_and_motion_changes_only_the_wearers_mesh() {
        for (ragged, armored) in [(false, false), (true, false), (false, true)] {
            for woman in [false, true] {
                for build in 0..3 {
                    let look = Look::with_choice(
                        CreatureModel::Vampire,
                        42,
                        Some(Variant {
                            woman,
                            build,
                            ..default()
                        }),
                    );
                    let body = HumanoidBody::resolve(&look);
                    let mut materials = Assets::<StandardMaterial>::default();
                    let mut library =
                        PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
                    let mut meshes = Assets::<Mesh>::default();
                    let mut world = World::new();
                    let parent = world.spawn_empty().id();
                    let mut queue = bevy::ecs::world::CommandQueue::default();
                    let mut commands = Commands::new(&mut queue, &world);
                    let recipe = || {
                        let mut recipe = Recipe::new(
                            body,
                            Vec3::new(0.06, 0.04, 0.09),
                            Vec3::new(0.7, 0.05, 0.1),
                            ragged,
                        );
                        if armored {
                            let mut armor = geometry::torso_shape(
                                body,
                                Some(geometry::TorsoMaterial::Plate),
                                true,
                            );
                            armor
                                .inflate(0.04)
                                .carve(crate::sdf::Shape::Beyond(Vec3::Y, 0.485), 0.003);
                            recipe.reserve_body_layer(&armor);
                        }
                        recipe
                    };
                    let mut first = spawn(
                        &mut commands,
                        &mut library,
                        &mut meshes,
                        parent,
                        recipe(),
                        false,
                    );
                    let second = spawn(
                        &mut commands,
                        &mut library,
                        &mut meshes,
                        parent,
                        recipe(),
                        false,
                    );
                    queue.apply(&mut world);
                    assert_ne!(first.mesh, second.mesh);
                    assert_eq!(
                        world
                            .get::<MeshMaterial3d<StandardMaterial>>(first.entity)
                            .unwrap()
                            .0,
                        world
                            .get::<MeshMaterial3d<StandardMaterial>>(second.entity)
                            .unwrap()
                            .0
                    );
                    let untouched = positions(meshes.get(&second.mesh).unwrap());
                    let colors = meshes
                        .get(&first.mesh)
                        .unwrap()
                        .attribute(Mesh::ATTRIBUTE_COLOR)
                        .unwrap()
                        .clone();
                    let indices = meshes.get(&first.mesh).unwrap().indices().unwrap().clone();
                    for rotation in [
                        Quat::from_rotation_x(0.35),
                        Quat::from_euler(EulerRot::XYZ, 0.3, 0.0, -0.3),
                        Quat::IDENTITY,
                    ] {
                        place(&mut first, rotation, &mut meshes);
                        let mesh = meshes.get(&first.mesh).unwrap();
                        let points = positions(mesh);
                        for i in 0..(COLUMNS + 1) * 2 {
                            let middle = rotation * (points[i] + points[GRID + i]) * 0.5;
                            assert!(
                                middle.distance(first.rest[i]) < 1e-5,
                                "shoulders must stay attached through travel and turns"
                            );
                        }
                        let hem = GRID - 1 - COLUMNS / 2;
                        let moved = rotation * (points[hem] + points[GRID + hem]) * 0.5;
                        if rotation != Quat::IDENTITY {
                            assert!(
                                moved.distance(first.rest[hem]) > 0.2,
                                "lower cloth must follow the eased swing"
                            );
                        }
                        for i in 0..GRID {
                            assert!(
                                (points[i].distance(points[GRID + i]) - THICKNESS).abs() < 1e-5
                            );
                        }
                        let bounds = world.get::<Aabb>(first.entity).unwrap();
                        assert!(points.iter().all(|point| point.is_finite()
                            && (bevy::math::Vec3A::from(*point) - bounds.center)
                                .abs()
                                .cmple(bounds.half_extents)
                                .all()));
                        let Some(VertexAttributeValues::Float32x3(normals)) =
                            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
                        else {
                            panic!("cloth normals")
                        };
                        assert!(normals
                            .iter()
                            .all(|n| (Vec3::from_array(*n).length() - 1.0).abs() < 1e-5));
                        assert_eq!(
                            format!("{:?}", mesh.attribute(Mesh::ATTRIBUTE_COLOR).unwrap()),
                            format!("{colors:?}")
                        );
                        assert_eq!(
                            format!("{:?}", mesh.indices().unwrap()),
                            format!("{indices:?}")
                        );
                        assert_eq!(untouched, positions(meshes.get(&second.mesh).unwrap()));
                    }
                    let restored = positions(meshes.get(&first.mesh).unwrap());
                    assert!(restored
                        .iter()
                        .zip(&untouched)
                        .all(|(a, b)| a.distance(*b) < 1e-5));
                }
            }
        }
    }

    #[test]
    fn both_sides_have_distinct_pigments_and_outward_winding() {
        let recipe = Recipe::new(
            HumanoidBody::resolve(&Look::of(CreatureModel::Vampire, 42)),
            Vec3::new(0.05, 0.03, 0.08),
            Vec3::new(0.7, 0.04, 0.06),
            false,
        );
        let mut materials = Assets::<StandardMaterial>::default();
        let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
        let mut meshes = Assets::<Mesh>::default();
        let mut world = World::new();
        let parent = world.spawn_empty().id();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let rig = spawn(
            &mut Commands::new(&mut queue, &world),
            &mut library,
            &mut meshes,
            parent,
            recipe,
            false,
        );
        let mesh = meshes.get(&rig.mesh).unwrap();
        let points = positions(mesh);
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("cloth pigments")
        };
        assert_ne!(colors[0], colors[GRID]);
        assert!(colors[..GRID].iter().all(|c| *c == colors[0]));
        assert!(colors[GRID..].iter().all(|c| *c == colors[GRID]));
        let triangles: Vec<_> = mesh.indices().unwrap().iter().collect();
        for triangle in triangles.as_chunks::<3>().0.iter().take(ROWS * COLUMNS * 4) {
            let [a, b, c] = [
                points[triangle[0]],
                points[triangle[1]],
                points[triangle[2]],
            ];
            let normal = (b - a).cross(c - a);
            assert!(normal.length() > 1e-6, "no zero-area cloth faces");
            let side = if triangle[0] < GRID { -1.0 } else { 1.0 };
            assert!(
                normal.z * side > 0.0,
                "lining and exterior face opposite directions"
            );
        }
    }
}
