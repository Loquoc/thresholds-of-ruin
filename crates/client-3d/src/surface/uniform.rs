//! GPU uniform layout. `ShaderType` (encase) emits unused compile-time size checks.
#![allow(dead_code)]

use bevy::{prelude::*, render::render_resource::ShaderType};

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
