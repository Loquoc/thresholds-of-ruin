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
const COLUMNS: usize = 48;
const GRID: usize = (ROWS + 1) * (COLUMNS + 1);
const THICKNESS: f32 = 0.012;
// Reserve lining thickness and interpolation across the curved plate shoulder.
const LAYER_GAP: f32 = 0.020;

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
        let t = row as f32 / ROWS as f32;
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
        let mut at = if row <= 3 {
            let across = row as f32 / 3.0;
            neck.lerp(shoulder, across) + Vec3::Y * (0.045 * (across * PI).sin())
        } else {
            shoulder.lerp(hem, (row - 3) as f32 / (ROWS - 3) as f32)
        };
        if let Some(scene) = &self.scene {
            at = fit_wrapped_contact(at, row <= 3, &|p| scene.distance(p));
        } else {
            at = fit_wrapped_contact(at, row <= 3, &|p| self.support.distance(p));
        }
        at - Vec3::new(0.0, 0.53, -0.15)
    }
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
    let mut rest: Vec<_> = (0..GRID)
        .map(|i| recipe.point(i / (COLUMNS + 1), i % (COLUMNS + 1)))
        .collect();
    let arm_surface = recipe
        .scene
        .as_ref()
        .map(|scene| LayerSurface::from_triangles(scene.cloth_hulls()));
    if let Some(surface) = &arm_surface {
        fit_arm_cells(&mut rest, Quat::IDENTITY, surface);
    }
    let mut normals = vec![Vec3::ZERO; GRID];
    surface_normals(&rest, &mut normals);
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
    write(&mut mesh, &rest, &normals);
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
        points: rest.clone(),
        bindings: (0..=ROWS)
            .map(|row| {
                let t = row.saturating_sub(1) as f32 / (ROWS - 1) as f32;
                SurfaceBinding::single(t * t * (3.0 - 2.0 * t))
            })
            .collect(),
        rest,
        normals,
        last_rotation: None,
        arm_surface,
        scene: recipe.scene,
        wrapped: recipe.cut.is_some(),
        inner_layer: recipe.inner_layer,
        last_inner_rotation: None,
        support: recipe.contact.then_some(recipe.support),
    }
}

pub(super) fn place(cloak: &mut CloakRig, rotation: Quat, meshes: &mut Assets<Mesh>) {
    if cloak
        .last_rotation
        .is_some_and(|last| rotation.angle_between(last) < 1e-6)
    {
        return;
    }
    // The drape entity retains its existing eased swing. Cancel that rotation
    // at the shoulder and introduce it gradually down the private rest grid.
    // Two fixed rows form a supported yoke, rather than a hinge-shaped gap.
    let inverse = rotation.inverse();
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
                let fitted = fit_wrapped_contact(at, row <= 3, &|p| scene.distance(p));
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
    if let Some(surface) = &cloak.arm_surface {
        fit_arm_cells(&mut cloak.points, rotation, surface);
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
    bins: Vec<Vec<[Vec3; 3]>>,
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
            bottom,
            height: (top - bottom).max(0.001),
        };
        for triangle in triangles {
            let low = triangle.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
            let high = triangle
                .iter()
                .map(|p| p.y)
                .fold(f32::NEG_INFINITY, f32::max);
            for bin in surface.bin(low)..=surface.bin(high) {
                surface.bins[bin].push(triangle);
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
        let mut support: Option<f32> = None;
        for bin in self.bin(low)..=self.bin(high) {
            for &triangle in &self.bins[bin] {
                let mut polygon = triangle.to_vec();
                for (plane, offset) in [
                    (Vec3::Y, low),
                    (Vec3::NEG_Y, -high),
                    (lower, -THICKNESS),
                    (upper, -THICKNESS),
                ] {
                    polygon = clip_layer_polygon(&polygon, plane, offset);
                    if polygon.is_empty() {
                        break;
                    }
                }
                for point in polygon {
                    let projected = point.dot(normal);
                    support = Some(support.map_or(projected, |old| old.max(projected)));
                }
            }
        }
        support
    }

    /// Outermost intersection with either face or a closed boundary edge.
    #[cfg(test)]
    fn radius(&self, origin: Vec3, ray: Vec3) -> Option<f32> {
        if origin.y < self.bottom || origin.y > self.bottom + self.height {
            return None;
        }
        let mut far: Option<f32> = None;
        for &[a, b, c] in &self.bins[self.bin(origin.y)] {
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

/// Clip against a closed half-space; extrema of a linear support function
/// lie at these vertices, including newly created edge intersections.
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

/// Constrain whole connected cells, not their separate vertices. Directions
/// and angular footprints remain fixed throughout the sweep; every later
/// nonnegative correction preserves earlier neighboring support planes.
fn fit_layer_cells(points: &mut [Vec3], rotation: Quat, support: &LayerSurface) {
    fit_cells(points, rotation, support, false);
}

fn fit_arm_cells(points: &mut [Vec3], rotation: Quat, support: &LayerSurface) {
    fit_cells(points, rotation, support, true);
}

fn fit_cells(points: &mut [Vec3], rotation: Quat, support: &LayerSurface, arms: bool) {
    let offset = Vec3::new(0.0, 0.53, -0.15);
    let mut posed: Vec<_> = points.iter().map(|p| rotation * *p + offset).collect();
    let directions: Vec<_> = posed
        .iter()
        .map(|p| p.with_y(0.0).normalize_or(Vec3::NEG_Z))
        .collect();
    for row in 0..ROWS {
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
            // Shoulder cells are above the entire contact hull in their
            // angular footprint. This remains valid after radial mantle fitting.
            // No unrelated arm in another angular sector raises this yoke.
            if arms && row < 3 {
                if let Some(top) = support.support_plane(
                    Vec3::Y,
                    support.bottom,
                    support.bottom + support.height,
                    lower,
                    upper,
                ) {
                    for i in cell {
                        posed[i].y = posed[i].y.max(top + LAYER_GAP + THICKNESS);
                    }
                }
                continue;
            }
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
        place(cloak, rot, meshes);
        let support = LayerSurface::new(
            meshes.get(&inner.mesh).expect("inner cloak mesh"),
            inner_rot,
        );
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
    }

    #[test]
    fn study_yoke_crosses_shoulder_from_neck_and_contact_clears_deltoid() {
        let r = study_recipe(outfit::outerwear::OuterwearAppearance::HoodedCloak);
        let origin = Vec3::new(0.0, 0.53, -0.15);
        let neck = r.point(0, COLUMNS / 4) + origin;
        let edge = r.point(3, COLUMNS / 4) + origin;
        assert!(neck.y > 0.55 && neck.x.abs() < 0.10);
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
