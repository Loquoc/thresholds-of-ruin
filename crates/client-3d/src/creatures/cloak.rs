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

const ROWS: usize = 20;
// Refine only the shoulder within the existing grid: six roof bands and
// fourteen hanging bands, with no additional vertices or emitted triangles.
const YOKE_ROWS: usize = 6;
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

pub(super) struct Recipe {
    shoulders: f32,
    yoke: Vec2,
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
        };
        recipe.reserve_back_layer(0.0);
        recipe
    }

    /// Actual rendered arm, sleeve and pauldron profiles, in torso query space.
    /// Call after reserving every torso layer so the scene owns the final union.
    pub fn set_contacts(&mut self, contacts: Vec<super::visual_contact::Cylinder>) {
        if self.cut.is_some() {
            self.scene = Some(super::visual_contact::Scene::new(
                self.support.clone(),
                contacts,
            ));
        }
    }

    /// Fit this shoulder layer to an earlier cloak on the same torso frame.
    pub fn layer_over(&mut self, inner: Entity) {
        self.inner_layer = Some(inner);
    }

    /// Study garments use an open angular shoulder wrap. Legacy recipes keep
    /// their original back-sheet coordinates and animation unchanged.
    pub fn set_cut(&mut self, cut: DrapeCut) {
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
        let t = study_row_fraction(row);
        let u = column as f32 / COLUMNS as f32;
        // Left front edge, around the back, to the right front edge. The
        // unsampled angular gap is a real opening, never a painted seam.
        let angle = TAU - cut.opening * 0.5 - (TAU - cut.opening) * u;
        let direction = Vec3::new(angle.sin(), 0.0, angle.cos());
        // A cape begins at the neck, travels OVER the deltoid, then hangs.
        // The old first row was below the shoulder and produced open teal
        // shoulder cups and rigid-looking boards down either side.
        let shoulder = Vec3::new(direction.x * self.yoke.x, 0.47, direction.z * self.yoke.y);
        let neck = Vec3::new(direction.x * 0.075, 0.565, direction.z * 0.072);
        // The opening surrounds the neck; it does not sit on the rounded
        // cap of the torso's neck proxy. Derive the connecting shoulder rows
        // from this fitted ring so an enlarged collar cannot fold underneath
        // the next radial row.
        let neck = if row <= YOKE_ROWS {
            fit_neck_opening(neck, &self.support)
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
        if row > 0 {
            if let Some(scene) = &self.scene {
                at = fit_wrapped_contact(at, row < YOKE_ROWS, &|p| scene.distance(p));
            } else {
                at = fit_wrapped_contact(at, row < YOKE_ROWS, &|p| self.support.distance(p));
            }
        }
        at - Vec3::new(0.0, 0.53, -0.15)
    }
}

/// A neck opening is an annular attachment at fixed anatomical height.
/// Only retained torso/neck support participates here; posed hands are handled
/// by the complete cell solver, not by wrapping the collar around their reach.
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

pub(super) struct CloakRig {
    pub entity: Entity,
    mesh: Handle<Mesh>,
    rest: Vec<Vec3>,
    bindings: Vec<SurfaceBinding<f32>>,
    points: Vec<Vec3>,
    normals: Vec<Vec3>,
    last_rotation: Option<Quat>,
    support: Option<crate::sdf::Sculpt>,
    wrapped: bool,
    inner_layer: Option<Entity>,
    last_inner_rotation: Option<Quat>,
    scene: Option<super::visual_contact::Scene>,
    arm_surface: Option<LayerSurface>,
    opening: Option<f32>,
    neutral_reach: [f32; 2],
}

impl CloakRig {
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
                    .map(|scene| LayerSurface::from_triangles(scene.cloth_hulls()));
                self.invalidate();
                return;
            }
        }
        if let Some(scene) = &mut self.scene {
            if scene.set_pose(contacts) {
                self.arm_surface = Some(LayerSurface::from_triangles(scene.cloth_hulls()));
                self.invalidate();
            }
        }
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
    let rest: Vec<_> = (0..GRID)
        .map(|i| recipe.point(i / (COLUMNS + 1), i % (COLUMNS + 1)))
        .collect();
    let arm_surface = recipe
        .scene
        .as_ref()
        .map(|scene| LayerSurface::from_triangles(scene.cloth_hulls()));
    // Contact fitting is an output, never the next solve's rest shape. In
    // particular, refitting a raised roof changes its inclination and can
    // amplify the same shoulder clearance on every subsequent solve.
    let mut points = rest.clone();
    if let Some(surface) = &arm_surface {
        fit_arm_cells(&mut points, Quat::IDENTITY, surface);
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
        normals,
        last_rotation: None,
        neutral_reach: arm_surface
            .as_ref()
            .map_or([0.0; 2], LayerSurface::arm_reach),
        arm_surface,
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
    if cloak
        .last_rotation
        .is_some_and(|last| rotation.angle_between(last) < 1e-6)
    {
        return;
    }
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
    let CloakRig {
        points,
        bindings,
        rest,
        support,
        wrapped,
        scene,
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
                    support
                        .as_ref()
                        .map_or(at, |body| fit_neck_opening(at, body))
                } else {
                    fit_wrapped_contact(at, row < YOKE_ROWS, &|p| scene.distance(p))
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
    // A mantle is fitted once against the combined inner-cloth/arm support
    // below. Pre-fitting its arms here would derive the second roof solve's
    // normals from a contact-deformed surface rather than the posed recipe.
    if fit_arms {
        if let Some(surface) = &cloak.arm_surface {
            fit_arm_cells(&mut cloak.points, rotation, surface);
        }
    }
    surface_normals(&cloak.points, &mut cloak.normals);
    if let Some(mesh) = meshes.get_mut(&cloak.mesh) {
        write(mesh, &cloak.points, &cloak.normals);
    }
    cloak.last_rotation = Some(rotation);
}

/// Posed triangle contact in the shared torso frame. Height bins keep each
/// mantle vertex query local without assuming the two hems have equal lengths.
struct LayerSurface {
    bins: Vec<Vec<usize>>,
    triangles: Vec<[Vec3; 3]>,
    first_bin: Vec<usize>,
    bottom: f32,
    height: f32,
}

impl LayerSurface {
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
        let mut surface = Self {
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
        for triangle in self.candidates(low, high) {
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
        for &triangle in &self.triangles {
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

/// A triangle clipped by at most four half-planes has at most seven vertices.
/// Fixed scratch buffers avoid heap allocation in the per-cell contact path.
fn clipped_support(triangle: [Vec3; 3], normal: Vec3, planes: &[(Vec3, f32)]) -> Option<f32> {
    let (polygon, count) = clipped_polygon(triangle, planes)?;
    Some(
        polygon[..count]
            .iter()
            .map(|p| p.dot(normal))
            .fold(f32::NEG_INFINITY, f32::max),
    )
}

fn clipped_polygon(triangle: [Vec3; 3], planes: &[(Vec3, f32)]) -> Option<([Vec3; 8], usize)> {
    assert!(
        planes.len() <= 4,
        "fixed clipping buffers support at most four planes"
    );
    if planes
        .iter()
        .any(|&(plane, offset)| triangle.iter().all(|p| plane.dot(*p) < offset))
    {
        return None;
    }
    let mut polygon = [Vec3::ZERO; 8];
    polygon[..3].copy_from_slice(&triangle);
    let mut count = 3;
    for &(plane, offset) in planes {
        let mut next = [Vec3::ZERO; 8];
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
            if (da >= 0.0) != (db >= 0.0) {
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
fn fit_layer_cells(points: &mut [Vec3], rotation: Quat, support: &LayerSurface) {
    fit_cells(points, rotation, support);
}

fn fit_arm_cells(points: &mut [Vec3], rotation: Quat, support: &LayerSurface) {
    fit_cells(points, rotation, support);
}

fn fit_cells(points: &mut [Vec3], rotation: Quat, support: &LayerSurface) {
    let offset = Vec3::new(0.0, 0.53, -0.15);
    let mut posed: Vec<_> = points.iter().map(|p| rotation * *p + offset).collect();
    let directions: Vec<_> = posed
        .iter()
        .map(|p| p.with_y(0.0).normalize_or(Vec3::NEG_Z))
        .collect();
    for row in YOKE_ROWS..ROWS {
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
            let high = if row == YOKE_ROWS {
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
    let mut roofs = Vec::with_capacity(YOKE_ROWS * COLUMNS * 2);
    for row in 0..YOKE_ROWS {
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
        let mut support = LayerSurface::new(
            meshes.get(&inner.mesh).expect("inner cloak mesh"),
            inner_rot,
        );
        // Seat the outer cloth against both retained cloth and the arm hull
        // in one solve. A changed yoke footprint must not discard the earlier
        // arm constraint when the mantle is lifted over its inner cape.
        if let Some(arms) = &cloak.arm_surface {
            support = LayerSurface::from_triangles(
                support
                    .triangles
                    .into_iter()
                    .chain(arms.triangles.iter().copied())
                    .collect(),
            );
        }
        fit_layer_cells(&mut cloak.points, rot, &support);
        surface_normals(&cloak.points, &mut cloak.normals);
        if let Some(mesh) = meshes.get_mut(&cloak.mesh) {
            write(mesh, &cloak.points, &cloak.normals);
        }
        cloak.last_inner_rotation = Some(inner_rot);
    }
}

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

    #[test]
    fn fixed_clipper_accepts_three_and_four_plane_footprints() {
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
        ];
        for count in [3, 4] {
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
    #[should_panic(expected = "at most four planes")]
    fn fixed_clipper_rejects_unsupported_plane_count_before_clipping() {
        let _ = clipped_support([Vec3::ZERO; 3], Vec3::Y, &[(Vec3::X, 0.0); 5]);
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
                    assert!((attachment.y - 0.565).abs() < 1e-6,
                        "{name}: actual equipped neck opening climbed the support cap: {attachment:?}");
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
                let peak_height = peak.y + offset.y;
                let rest_height = cloak.rest[peak_index].y + offset.y;
                assert!(
                    peak_height <= top + 0.12,
                    "{name}: yoke exceeds two-layer allowance: inner={:?}, vertex={peak_index} row={} column={}, actual_height={peak_height}, support_top={top}, limit={}, authored_height={rest_height}, spawned_peak={spawned_peak}, point={peak:?}",
                    cloak.inner_layer, peak_index / (COLUMNS + 1), peak_index % (COLUMNS + 1), top + 0.12
                );
                for column in [COLUMNS / 4, COLUMNS * 3 / 4] {
                    let neck = cloak.points[index(0, column)];
                    let shoulder = cloak.points[index(YOKE_ROWS, column)];
                    let authored_shoulder = cloak.rest[index(YOKE_ROWS, column)];
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
        for (name, added) in [("cloak", 0), ("mummy-wrapping", 0), ("hooded-cloak", 2)] {
            let mut equipment = Equipment::default();
            equipment.select(name).unwrap();
            let (_, mut rig, _, _) =
                super::super::rogue_tests::spawn_one(&base.clone().equipped(equipment));
            assert_eq!(
                rig.visual_contacts.len(),
                native.visual_contacts.len() + added
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
            let (mut world, rig, _, meshes) = super::super::rogue_tests::spawn_one(&look);
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
            let contacts = super::super::visual_contact::posed(&rig.visual_contacts, |joint| {
                rig.joint(joint)
                    .and_then(|e| world.get::<Transform>(e))
                    .map(Transform::compute_affine)
            });
            let meshes = world.resource::<Assets<Mesh>>();
            for cloak in &rig.cloaks {
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
                        assert!(contacts.iter().all(|c|c.distance(p)>0.001),
                            "actual strike cloth faces/lining must clear retained arms: {name}, {p:?}");
                    }
                }
            }
        }
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
        let (_, mut rig, _, mut meshes) = super::super::rogue_tests::spawn_one(&look);
        assert_eq!(
            rig.visual_contacts.len(),
            6,
            "complete upper/fore/hand meshes"
        );
        let mut released_witnesses = 0;
        for angle in [0.0, -1.15, 0.85] {
            let contacts = super::super::visual_contact::posed(&rig.visual_contacts, |joint| {
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
            });
            for cloak in &mut rig.cloaks {
                cloak.set_contacts(contacts.clone());
            }
            let rotation = Quat::from_euler(EulerRot::XYZ, 0.12, -0.18, 0.09);
            place_layers(&mut rig.cloaks, |_| Some(rotation), &mut meshes);
            let mut tested = 0;
            for cloak in &rig.cloaks {
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
                            let gap = contacts
                                .iter()
                                .map(|c| c.distance(p))
                                .fold(f32::INFINITY, f32::min);
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
                for face in indices.as_chunks::<3>().0 {
                    let tri = face.map(|i| mantle * points[i] + offset);
                    let at = (tri[0] + tri[1] + tri[2]) / 3.0;
                    // Leave the supported yoke boundary out of the hem-motion
                    // witness; validate actual faces throughout the shared drape.
                    if !(0.23..0.44).contains(&at.y) {
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
