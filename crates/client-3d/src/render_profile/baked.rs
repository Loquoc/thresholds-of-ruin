//! The rock's surface noise, baked into two small repeating textures so the
//! shader reads it instead of hashing it for every pixel.
use bevy::{
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::{
        render_asset::RenderAssetUsages,
        render_resource::{Extent3d, TextureDimension, TextureFormat},
    },
};

/// Lattice cells along each edge of the lattice texture, one texel each.
pub(super) const LATTICE: usize = 32;
/// Texels along each edge of the sums texture, covering 16 units of noise.
pub(super) const SUMS: usize = 128;
/// The baked slope is divided by this to fit a texel.
pub(super) const SLOPE_RANGE: f32 = 3.0;

/// The rock shader's lattice hash, with the shader's own constants.
#[allow(clippy::approx_constant)]
pub(super) fn hash(p: Vec3) -> f32 {
    let q = (p * 0.318_309_9 + Vec3::new(0.71, 0.113, 0.419)).fract() * 17.0;
    (q.x * q.y * q.z * (q.x + q.y + q.z)).fract()
}

fn byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// The hash at each lattice point of the cube.
pub(super) fn lattice() -> Vec<u8> {
    let mut data = Vec::with_capacity(LATTICE * LATTICE * LATTICE);
    for z in 0..LATTICE {
        for y in 0..LATTICE {
            for x in 0..LATTICE {
                data.push(byte(hash(Vec3::new(x as f32, y as f32, z as f32))));
            }
        }
    }
    data
}

/// Four octaves of value noise that repeat across the cube, each texel
/// holding the sum and then its slope per unit along x, y and z, divided by
/// [`SLOPE_RANGE`] and centred on a half.
pub(super) fn sums() -> Vec<u8> {
    let mut field = vec![0.0_f32; SUMS * SUMS * SUMS];
    let at = |x: usize, y: usize, z: usize| (z * SUMS + y) * SUMS + x;
    let mut amplitude = 0.5;
    for octave in 0..4_usize {
        // 16, 32, 64 then 128 lattice cells across the cube.
        let cells = 16 << octave;
        let step = cells as f32 / SUMS as f32;
        let corner = |i: [usize; 3]| {
            hash(Vec3::new(
                (i[0] % cells) as f32 + 37.0 * octave as f32,
                (i[1] % cells) as f32 + 11.0 * octave as f32,
                (i[2] % cells) as f32 + 5.0 * octave as f32,
            ))
        };
        for z in 0..SUMS {
            for y in 0..SUMS {
                for x in 0..SUMS {
                    let p = Vec3::new(x as f32, y as f32, z as f32) * step;
                    let i = p.floor();
                    let f = p - i;
                    let u = f * f * (Vec3::splat(3.0) - 2.0 * f);
                    let i = [i.x as usize, i.y as usize, i.z as usize];
                    let c =
                        |dx: usize, dy: usize, dz: usize| corner([i[0] + dx, i[1] + dy, i[2] + dz]);
                    let near = (c(0, 0, 0) * (1.0 - u.x) + c(1, 0, 0) * u.x) * (1.0 - u.y)
                        + (c(0, 1, 0) * (1.0 - u.x) + c(1, 1, 0) * u.x) * u.y;
                    let far = (c(0, 0, 1) * (1.0 - u.x) + c(1, 0, 1) * u.x) * (1.0 - u.y)
                        + (c(0, 1, 1) * (1.0 - u.x) + c(1, 1, 1) * u.x) * u.y;
                    field[at(x, y, z)] += amplitude * (near * (1.0 - u.z) + far * u.z);
                }
            }
        }
        amplitude *= 0.5;
    }
    let mut data = Vec::with_capacity(field.len() * 4);
    let wrap = |v: usize, d: isize| (v as isize + d).rem_euclid(SUMS as isize) as usize;
    // One texel is an eighth of a unit, so a two-texel difference spans a quarter.
    let slope = |ahead: f32, behind: f32| {
        byte(((ahead - behind) * 4.0 / SLOPE_RANGE).clamp(-1.0, 1.0) * 0.5 + 0.5)
    };
    for z in 0..SUMS {
        for y in 0..SUMS {
            for x in 0..SUMS {
                data.push(byte(field[at(x, y, z)]));
                data.push(slope(
                    field[at(wrap(x, 1), y, z)],
                    field[at(wrap(x, -1), y, z)],
                ));
                data.push(slope(
                    field[at(x, wrap(y, 1), z)],
                    field[at(x, wrap(y, -1), z)],
                ));
                data.push(slope(
                    field[at(x, y, wrap(z, 1))],
                    field[at(x, y, wrap(z, -1))],
                ));
            }
        }
    }
    data
}

fn repeating(edge: usize, data: Vec<u8>, format: TextureFormat) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: edge as u32,
            height: edge as u32,
            depth_or_array_layers: edge as u32,
        },
        TextureDimension::D3,
        data,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

pub(super) fn lattice_image() -> Image {
    repeating(LATTICE, lattice(), TextureFormat::R8Unorm)
}

pub(super) fn sums_image() -> Image {
    repeating(SUMS, sums(), TextureFormat::Rgba8Unorm)
}
