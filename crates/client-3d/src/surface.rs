//! Data-driven PBR surface recipes and procedural `ModelMaterial` extension.
//!
//! Models (creatures, garments, equipment, props) are built without UV maps
//! or tangent attributes and encode per-part tint in vertex colours. A
//! [`SurfaceRecipe`] pairs standard Bevy PBR parameters with a [`SurfaceKind`]
//! and micro-detail settings so [`ModelExtension`] can synthesize seamless
//! object/world-space normal, roughness, and rim detail in `model.wgsl`.
use bevy::{
    asset::{load_internal_asset, weak_handle},
    pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin},
    prelude::*,
    render::{
        render_asset::RenderAssetUsages,
        render_resource::{
            AsBindGroup, Extent3d, ShaderRef, ShaderType, TextureDimension, TextureFormat,
            TextureViewDescriptor, TextureViewDimension,
        },
    },
};
use std::collections::HashMap;

pub const SHADER: Handle<Shader> = weak_handle!("9f4c2e1b-7a3d-4e8c-b51a-6d2f8c4e9a12");

pub type ModelMaterial = ExtendedMaterial<StandardMaterial, ModelExtension>;

/// Procedural surface archetype interpreted by `model.wgsl`.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
pub enum SurfaceKind {
    Cloth = 0,
    Leather = 1,
    Metal = 2,
    Wood = 3,
    Skin = 4,
    Chitin = 5,
    Stone = 6,
    Spectral = 7,
    Glass = 8,
    Emissive = 9,
}

impl SurfaceKind {
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

/// Declarative, data-driven surface recipe for 3D models and props.
///
/// Can produce either a plain [`StandardMaterial`] (for `ShadeState` / low
/// profile fallbacks) or a full [`ModelMaterial`] with procedural micro-detail.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceRecipe {
    pub name: &'static str,
    pub kind: SurfaceKind,
    pub metallic: f32,
    pub perceptual_roughness: f32,
    pub reflectance: f32,
    /// Spatial frequency for procedural 3D detail (cycles per world unit).
    pub noise_scale: f32,
    /// Strength of procedural micro-normal perturbation (0.0 disables).
    pub normal_strength: f32,
    /// Amplitude of procedural roughness modulation around `perceptual_roughness`.
    pub roughness_variation: f32,
    /// Amplitude of subtle albedo/grain modulation.
    pub albedo_variation: f32,
    /// Subtle grazing-angle Fresnel rim boost for silhouette readability.
    pub rim_strength: f32,
    /// Edge/surface wear factor (scuffs on leather, hammered wear on iron, etc.).
    pub wear: f32,
    pub alpha: f32,
    pub double_sided: bool,
    pub emissive: LinearRgba,
}

impl SurfaceRecipe {
    pub const WOOL: Self = Self {
        name: "wool",
        kind: SurfaceKind::Cloth,
        metallic: 0.0,
        perceptual_roughness: 0.92,
        reflectance: 0.28,
        noise_scale: 42.0,
        normal_strength: 0.22,
        roughness_variation: 0.06,
        albedo_variation: 0.05,
        rim_strength: 0.08,
        wear: 0.1,
        alpha: 1.0,
        double_sided: false,
        emissive: LinearRgba::BLACK,
    };

    pub const SHEER: Self = Self {
        name: "sheer",
        kind: SurfaceKind::Cloth,
        metallic: 0.0,
        perceptual_roughness: 0.85,
        reflectance: 0.30,
        noise_scale: 56.0,
        normal_strength: 0.12,
        roughness_variation: 0.04,
        albedo_variation: 0.03,
        rim_strength: 0.14,
        wear: 0.0,
        alpha: 0.52,
        double_sided: true,
        emissive: LinearRgba::BLACK,
    };

    pub const LEATHER: Self = Self {
        name: "leather",
        kind: SurfaceKind::Leather,
        metallic: 0.0,
        perceptual_roughness: 0.46,
        reflectance: 0.42,
        noise_scale: 34.0,
        normal_strength: 0.20,
        roughness_variation: 0.10,
        albedo_variation: 0.06,
        rim_strength: 0.06,
        wear: 0.25,
        alpha: 1.0,
        double_sided: false,
        emissive: LinearRgba::BLACK,
    };

    pub const FORGED_IRON: Self = Self {
        name: "forged_iron",
        kind: SurfaceKind::Metal,
        metallic: 0.85,
        perceptual_roughness: 0.34,
        reflectance: 0.50,
        noise_scale: 28.0,
        normal_strength: 0.18,
        roughness_variation: 0.12,
        albedo_variation: 0.04,
        rim_strength: 0.04,
        wear: 0.35,
        alpha: 1.0,
        double_sided: false,
        emissive: LinearRgba::BLACK,
    };

    pub const POLISHED_STEEL: Self = Self {
        name: "polished_steel",
        kind: SurfaceKind::Metal,
        metallic: 0.94,
        perceptual_roughness: 0.20,
        reflectance: 0.55,
        noise_scale: 36.0,
        normal_strength: 0.08,
        roughness_variation: 0.07,
        albedo_variation: 0.02,
        rim_strength: 0.04,
        wear: 0.15,
        alpha: 1.0,
        double_sided: false,
        emissive: LinearRgba::BLACK,
    };

    pub const GOLD: Self = Self {
        name: "gold",
        kind: SurfaceKind::Metal,
        metallic: 0.92,
        perceptual_roughness: 0.26,
        reflectance: 0.55,
        noise_scale: 30.0,
        normal_strength: 0.10,
        roughness_variation: 0.08,
        albedo_variation: 0.03,
        rim_strength: 0.04,
        wear: 0.18,
        alpha: 1.0,
        double_sided: false,
        emissive: LinearRgba::BLACK,
    };

    pub const WOOD: Self = Self {
        name: "wood",
        kind: SurfaceKind::Wood,
        metallic: 0.0,
        perceptual_roughness: 0.68,
        reflectance: 0.36,
        noise_scale: 22.0,
        normal_strength: 0.18,
        roughness_variation: 0.10,
        albedo_variation: 0.10,
        rim_strength: 0.03,
        wear: 0.20,
        alpha: 1.0,
        double_sided: false,
        emissive: LinearRgba::BLACK,
    };

    pub const SKIN: Self = Self {
        name: "skin",
        kind: SurfaceKind::Skin,
        metallic: 0.0,
        perceptual_roughness: 0.68,
        reflectance: 0.44,
        noise_scale: 48.0,
        normal_strength: 0.08,
        roughness_variation: 0.05,
        albedo_variation: 0.03,
        rim_strength: 0.08,
        wear: 0.0,
        alpha: 1.0,
        double_sided: false,
        emissive: LinearRgba::BLACK,
    };

    pub const CHITIN: Self = Self {
        name: "chitin",
        kind: SurfaceKind::Chitin,
        metallic: 0.08,
        perceptual_roughness: 0.32,
        reflectance: 0.56,
        noise_scale: 30.0,
        normal_strength: 0.14,
        roughness_variation: 0.08,
        albedo_variation: 0.05,
        rim_strength: 0.10,
        wear: 0.10,
        alpha: 1.0,
        double_sided: false,
        emissive: LinearRgba::BLACK,
    };

    pub const STONE: Self = Self {
        name: "stone",
        kind: SurfaceKind::Stone,
        metallic: 0.0,
        perceptual_roughness: 0.88,
        reflectance: 0.30,
        noise_scale: 18.0,
        normal_strength: 0.28,
        roughness_variation: 0.12,
        albedo_variation: 0.08,
        rim_strength: 0.02,
        wear: 0.30,
        alpha: 1.0,
        double_sided: false,
        emissive: LinearRgba::BLACK,
    };

    pub const SPECTRAL: Self = Self {
        name: "spectral",
        kind: SurfaceKind::Spectral,
        metallic: 0.0,
        perceptual_roughness: 0.55,
        reflectance: 0.40,
        noise_scale: 12.0,
        normal_strength: 0.10,
        roughness_variation: 0.08,
        albedo_variation: 0.06,
        rim_strength: 0.28,
        wear: 0.0,
        alpha: 0.78,
        double_sided: false,
        emissive: LinearRgba::rgb(0.035, 0.05, 0.09),
    };

    pub const GLASS: Self = Self {
        name: "glass",
        kind: SurfaceKind::Glass,
        metallic: 0.0,
        perceptual_roughness: 0.12,
        reflectance: 0.65,
        noise_scale: 24.0,
        normal_strength: 0.03,
        roughness_variation: 0.03,
        albedo_variation: 0.0,
        rim_strength: 0.18,
        wear: 0.0,
        alpha: 0.72,
        double_sided: false,
        emissive: LinearRgba::BLACK,
    };

    #[cfg_attr(not(test), allow(dead_code))]
    pub const ALL: &'static [Self] = &[
        Self::WOOL,
        Self::SHEER,
        Self::LEATHER,
        Self::FORGED_IRON,
        Self::POLISHED_STEEL,
        Self::GOLD,
        Self::WOOD,
        Self::SKIN,
        Self::CHITIN,
        Self::STONE,
        Self::SPECTRAL,
        Self::GLASS,
    ];

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn by_name(name: &str) -> Option<&'static Self> {
        Self::ALL.iter().find(|recipe| recipe.name == name)
    }

    pub fn to_standard_material(self) -> StandardMaterial {
        let alpha_mode = if self.alpha < 0.999 {
            AlphaMode::Blend
        } else {
            AlphaMode::Opaque
        };
        StandardMaterial {
            base_color: Color::srgba(1.0, 1.0, 1.0, self.alpha),
            emissive: self.emissive,
            metallic: self.metallic,
            perceptual_roughness: self.perceptual_roughness,
            reflectance: self.reflectance,
            alpha_mode,
            cull_mode: if self.double_sided {
                None
            } else {
                Some(bevy::render::render_resource::Face::Back)
            },
            double_sided: self.double_sided,
            ..default()
        }
    }

    pub fn to_surface_params(self, detail_enabled: bool, baked_noise: bool) -> SurfaceParams {
        SurfaceParams {
            kind_and_flags: UVec4::new(
                self.kind.as_u32(),
                u32::from(detail_enabled),
                u32::from(baked_noise),
                0,
            ),
            detail: Vec4::new(
                self.noise_scale,
                self.normal_strength,
                self.roughness_variation,
                self.albedo_variation,
            ),
            finish: Vec4::new(self.rim_strength, self.wear, 0.0, 0.0),
        }
    }

    pub fn to_model_material(
        self,
        noise: Option<Handle<Image>>,
        detail_enabled: bool,
    ) -> ModelMaterial {
        let baked_noise = noise.is_some();
        ModelMaterial {
            base: self.to_standard_material(),
            extension: ModelExtension {
                params: self.to_surface_params(detail_enabled, baked_noise),
                noise,
            },
        }
    }
}

/// GPU uniform block for `model.wgsl`.
#[derive(ShaderType, Reflect, Debug, Clone, Copy)]
pub struct SurfaceParams {
    /// `x`: `SurfaceKind`, `y`: detail enabled (0/1), `z`: use baked 3D noise (0/1).
    pub kind_and_flags: UVec4,
    /// `x`: noise_scale, `y`: normal_strength, `z`: roughness_variation, `w`: albedo_variation.
    pub detail: Vec4,
    /// `x`: rim_strength, `y`: wear, `z`/`w`: reserved.
    pub finish: Vec4,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct ModelExtension {
    #[uniform(100)]
    pub params: SurfaceParams,
    /// Optional shared 3D noise+slope texture (same format as `cave_sums`).
    #[texture(101, dimension = "3d")]
    #[sampler(102)]
    pub noise: Option<Handle<Image>>,
}

impl MaterialExtension for ModelExtension {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(SHADER)
    }
}

/// Registers the procedural model shader, `ModelMaterial` pipeline, and
/// startup-baked cavern environment lighting.
pub struct ModelMaterialPlugin;

impl Plugin for ModelMaterialPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, SHADER, "model.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<ModelMaterial>::default())
            .init_resource::<ModelSurfaceTable>()
            .add_systems(
                Update,
                (
                    attach_cavern_environment_light,
                    sync_model_surface_table,
                    upgrade_model_materials,
                )
                    .chain(),
            );
    }
}

/// Startup-baked cavern cubemap for subtle torch-amber and slate-blue IBL
/// reflections on metals, leather, chitin, and glass.
pub(crate) fn cavern_environment_light(images: &mut Assets<Image>) -> EnvironmentMapLight {
    const SIZE: u32 = 16;
    let face_dir = |face: usize, u: f32, v: f32| -> Vec3 {
        let dir = match face {
            0 => Vec3::new(1.0, -v, -u),  // +X
            1 => Vec3::new(-1.0, -v, u),  // -X
            2 => Vec3::new(u, 1.0, v),    // +Y
            3 => Vec3::new(u, -1.0, -v),  // -Y
            4 => Vec3::new(u, -v, 1.0),   // +Z
            _ => Vec3::new(-u, -v, -1.0), // -Z
        };
        dir.normalize()
    };

    let sample_cavern = |dir: Vec3, mip_blur: f32| -> [u8; 4] {
        // Balanced warm torch-amber horizon with cool slate-blue cavern bounce
        // so silver/steel read cool-neutral while gold/copper stay rich.
        let up = dir.y;
        let horizon = (1.0 - up.abs()).powf(1.8 - 0.8 * mip_blur);
        let torch = Vec3::new(0.32, 0.25, 0.19) * horizon * (0.65 + 0.35 * dir.z.max(0.0));
        let ceiling = Vec3::new(0.14, 0.18, 0.28) * up.max(0.0);
        let floor = Vec3::new(0.05, 0.045, 0.04) * (-up).max(0.0);
        let base = Vec3::new(0.06, 0.065, 0.085) * 0.35;
        let linear = base + torch + ceiling + floor;
        let to_srgb = |c: f32| -> u8 {
            let s = c.clamp(0.0, 1.0).powf(1.0 / 2.2);
            (s * 255.0).round() as u8
        };
        [to_srgb(linear.x), to_srgb(linear.y), to_srgb(linear.z), 255]
    };

    let mut make_cube = |mip_blur: f32| -> Handle<Image> {
        let mut data = Vec::with_capacity((SIZE * SIZE * 6 * 4) as usize);
        for face in 0..6 {
            for y in 0..SIZE {
                let v = ((y as f32 + 0.5) / SIZE as f32) * 2.0 - 1.0;
                for x in 0..SIZE {
                    let u = ((x as f32 + 0.5) / SIZE as f32) * 2.0 - 1.0;
                    data.extend_from_slice(&sample_cavern(face_dir(face, u, v), mip_blur));
                }
            }
        }
        let mut image = Image::new(
            Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 6,
            },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.texture_view_descriptor = Some(TextureViewDescriptor {
            dimension: Some(TextureViewDimension::Cube),
            ..default()
        });
        images.add(image)
    };

    EnvironmentMapLight {
        diffuse_map: make_cube(1.0),
        specular_map: make_cube(0.25),
        intensity: 260.0,
        ..default()
    }
}

fn attach_cavern_environment_light(
    mut commands: Commands,
    cameras: Query<Entity, (With<Camera3d>, Without<EnvironmentMapLight>)>,
    mut images: ResMut<Assets<Image>>,
    mut cached: Local<Option<EnvironmentMapLight>>,
) {
    if cameras.is_empty() {
        return;
    }
    let env = cached
        .get_or_insert_with(|| cavern_environment_light(&mut images))
        .clone();
    for entity in &cameras {
        commands.entity(entity).insert(env.clone());
    }
}

/// Maps `PropMaterials`'s `StandardMaterial` handles to their upgraded
/// `ModelMaterial` equivalents so procedural micro-detail applies automatically
/// at runtime without breaking `StandardMaterial` test contracts.
#[derive(Resource, Default)]
pub struct ModelSurfaceTable {
    pub(crate) by_standard: HashMap<AssetId<StandardMaterial>, Handle<ModelMaterial>>,
}

impl ModelSurfaceTable {
    pub fn register(
        &mut self,
        standard: &Handle<StandardMaterial>,
        recipe: &SurfaceRecipe,
        model_materials: &mut Assets<ModelMaterial>,
        noise: Option<Handle<Image>>,
        detail_enabled: bool,
    ) -> Handle<ModelMaterial> {
        let handle = model_materials.add(recipe.to_model_material(noise, detail_enabled));
        self.by_standard.insert(standard.id(), handle.clone());
        handle
    }

    pub fn get(&self, standard: AssetId<StandardMaterial>) -> Option<&Handle<ModelMaterial>> {
        self.by_standard.get(&standard)
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_model_surface_table(
    library: Option<Res<crate::props::PropLibrary>>,
    profile: Option<Res<crate::render_profile::RenderProfile>>,
    mut baked: Option<ResMut<crate::render_profile::Baked>>,
    mut images: ResMut<Assets<Image>>,
    mut table: ResMut<ModelSurfaceTable>,
    mut model_materials: ResMut<Assets<ModelMaterial>>,
    mut last_state: Local<Option<(bool, bool)>>,
) {
    let Some(library) = library else {
        return;
    };
    let detail_enabled = profile
        .as_ref()
        .is_none_or(|p| p.rock_lighting == crate::render_profile::RockLighting::Full);
    let use_baked = profile
        .as_ref()
        .is_some_and(|p| p.rock_noise == crate::render_profile::RockNoise::Baked);
    if !table.by_standard.is_empty()
        && !library.is_changed()
        && *last_state == Some((detail_enabled, use_baked))
    {
        return;
    }
    *last_state = Some((detail_enabled, use_baked));
    let noise = if use_baked {
        baked.as_deref_mut().map(|b| b.sums(&mut images))
    } else {
        None
    };
    table.by_standard.clear();
    library
        .materials
        .register_model_surfaces(&mut table, &mut model_materials, noise, detail_enabled);
}

#[allow(clippy::type_complexity)]
fn upgrade_model_materials(
    mut commands: Commands,
    table: Res<ModelSurfaceTable>,
    added: Query<(Entity, &MeshMaterial3d<StandardMaterial>), Added<MeshMaterial3d<StandardMaterial>>>,
    all: Query<(Entity, &MeshMaterial3d<StandardMaterial>)>,
) {
    if table.by_standard.is_empty() {
        return;
    }
    if table.is_changed() {
        for (entity, mat) in &all {
            if let Some(model_mat) = table.get(mat.0.id()) {
                commands
                    .entity(entity)
                    .remove::<MeshMaterial3d<StandardMaterial>>()
                    .insert(MeshMaterial3d(model_mat.clone()));
            }
        }
    } else {
        for (entity, mat) in &added {
            if let Some(model_mat) = table.get(mat.0.id()) {
                commands
                    .entity(entity)
                    .remove::<MeshMaterial3d<StandardMaterial>>()
                    .insert(MeshMaterial3d(model_mat.clone()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipes_have_unique_names_and_valid_pbr_bounds() {
        let mut names = std::collections::HashSet::new();
        for recipe in SurfaceRecipe::ALL {
            assert!(
                names.insert(recipe.name),
                "duplicate surface recipe name: {}",
                recipe.name
            );
            assert!(
                (0.0..=1.0).contains(&recipe.metallic),
                "{} metallic out of range",
                recipe.name
            );
            assert!(
                (0.0..=1.0).contains(&recipe.perceptual_roughness),
                "{} roughness out of range",
                recipe.name
            );
            assert!(
                (0.0..=1.0).contains(&recipe.reflectance),
                "{} reflectance out of range",
                recipe.name
            );
            assert!(
                (0.0..=1.0).contains(&recipe.alpha),
                "{} alpha out of range",
                recipe.name
            );
            assert_eq!(SurfaceRecipe::by_name(recipe.name), Some(recipe));
        }
    }

    #[test]
    fn model_material_preserves_recipe_flags_and_alpha_mode() {
        let sheer = SurfaceRecipe::SHEER.to_model_material(None, true);
        assert_eq!(sheer.base.alpha_mode, AlphaMode::Blend);
        assert!(sheer.base.double_sided);
        assert_eq!(sheer.extension.params.kind_and_flags.x, SurfaceKind::Cloth.as_u32());
        assert_eq!(sheer.extension.params.kind_and_flags.y, 1);
        assert_eq!(sheer.extension.params.kind_and_flags.z, 0);

        let iron = SurfaceRecipe::FORGED_IRON.to_model_material(None, false);
        assert_eq!(iron.base.alpha_mode, AlphaMode::Opaque);
        assert!(!iron.base.double_sided);
        assert_eq!(iron.extension.params.kind_and_flags.x, SurfaceKind::Metal.as_u32());
        assert_eq!(iron.extension.params.kind_and_flags.y, 0);
    }

    #[test]
    fn cavern_environment_light_builds_cube_maps() {
        let mut images = Assets::<Image>::default();
        let env = cavern_environment_light(&mut images);
        let diffuse = images.get(&env.diffuse_map).expect("diffuse cubemap");
        let specular = images.get(&env.specular_map).expect("specular cubemap");
        assert_eq!(diffuse.texture_descriptor.size.depth_or_array_layers, 6);
        assert_eq!(specular.texture_descriptor.size.depth_or_array_layers, 6);
        assert_eq!(
            diffuse.texture_view_descriptor.as_ref().unwrap().dimension,
            Some(TextureViewDimension::Cube)
        );
        assert_eq!(
            specular.texture_view_descriptor.as_ref().unwrap().dimension,
            Some(TextureViewDimension::Cube)
        );
    }

    #[test]
    fn prop_materials_register_core_model_surfaces() {
        let mut standard = Assets::<StandardMaterial>::default();
        let mut model = Assets::<ModelMaterial>::default();
        let props = crate::props::PropMaterials::new(&mut standard);
        let mut table = ModelSurfaceTable::default();
        props.register_model_surfaces(&mut table, &mut model, None, true);
        let metal = table
            .get(props.metal().id())
            .expect("metal model material registered");
        let mat = model.get(metal).expect("model material asset");
        assert_eq!(
            mat.extension.params.kind_and_flags.x,
            SurfaceKind::Metal.as_u32()
        );
        assert_eq!(mat.extension.params.kind_and_flags.y, 1);
    }
}
