// Procedural surface micro-detail for 3D creatures, equipment, and props.
// Driven by SurfaceParams + optional shared 3D noise texture without requiring
// UV maps or tangent attributes on procedural meshes.
#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    pbr_types::{PbrInput, STANDARD_MATERIAL_FLAGS_UNLIT_BIT},
}

struct SurfaceParams {
    // x: SurfaceKind (0=Cloth, 1=Leather, 2=Metal, 3=Wood, 4=Skin, 5=Chitin, 6=Stone, 7=Spectral, 8=Glass, 9=Emissive)
    // y: detail_enabled (0/1)
    // z: baked_noise (0/1)
    kind_and_flags: vec4<u32>,
    // x: noise_scale, y: normal_strength, z: roughness_variation, w: albedo_variation
    detail: vec4<f32>,
    // x: rim_strength, y: wear
    finish: vec4<f32>,
}

@group(2) @binding(100) var<uniform> surface: SurfaceParams;
@group(2) @binding(101) var surface_noise: texture_3d<f32>;
@group(2) @binding(102) var surface_noise_sampler: sampler;

fn hash3(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.3183099 + vec3<f32>(0.71, 0.113, 0.419));
    q = q * 17.0;
    return fract(q.x * q.y * q.z * (q.x + q.y + q.z));
}

fn vnoise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(mix(hash3(i + vec3(0.0, 0.0, 0.0)), hash3(i + vec3(1.0, 0.0, 0.0)), u.x),
            mix(hash3(i + vec3(0.0, 1.0, 0.0)), hash3(i + vec3(1.0, 1.0, 0.0)), u.x), u.y),
        mix(mix(hash3(i + vec3(0.0, 0.0, 1.0)), hash3(i + vec3(1.0, 0.0, 1.0)), u.x),
            mix(hash3(i + vec3(0.0, 1.0, 1.0)), hash3(i + vec3(1.0, 1.0, 1.0)), u.x), u.y),
        u.z);
}

// Returns (value in [0, 1], gradient in local/world space).
fn sample_noise_and_grad(p: vec3<f32>) -> vec4<f32> {
    if (surface.kind_and_flags.z > 0u) {
        let s = textureSampleLevel(surface_noise, surface_noise_sampler, p / 16.0, 0.0);
        return vec4<f32>(s.r, (s.gba * 2.0 - 1.0) * 3.0);
    }
    let h0 = vnoise(p);
    let e = 0.04;
    let gx = (vnoise(p + vec3(e, 0.0, 0.0)) - h0) / e;
    let gy = (vnoise(p + vec3(0.0, e, 0.0)) - h0) / e;
    let gz = (vnoise(p + vec3(0.0, 0.0, e)) - h0) / e;
    return vec4<f32>(h0, gx, gy, gz);
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input: PbrInput = pbr_input_from_standard_material(in, is_front);
    let lit = (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u;

    if (lit && surface.kind_and_flags.y > 0u) {
        let kind = surface.kind_and_flags.x;
        let wp = in.world_position.xyz;
        var sample_pos = wp * surface.detail.x;

        // Wood stretches along the vertical/limb axis; brushed metal stretches along X/Z.
        if (kind == 3u) {
            sample_pos = vec3<f32>(wp.x * 2.2, wp.y * 0.35, wp.z * 2.2) * surface.detail.x;
        } else if (kind == 2u) {
            sample_pos = vec3<f32>(wp.x * 1.4, wp.y * 0.7, wp.z * 1.4) * surface.detail.x;
        }

        let ng = sample_noise_and_grad(sample_pos);
        var n_val = ng.x;
        var grad = ng.yzw;

        // Cloth adds a subtle orthogonal micro-weave on top of organic noise.
        if (kind == 0u) {
            let weave = sin(wp.x * surface.detail.x * 3.0) * sin(wp.z * surface.detail.x * 3.0);
            n_val = mix(n_val, weave * 0.5 + 0.5, 0.45);
            grad += vec3<f32>(
                cos(wp.x * surface.detail.x * 3.0) * sin(wp.z * surface.detail.x * 3.0),
                0.0,
                sin(wp.x * surface.detail.x * 3.0) * cos(wp.z * surface.detail.x * 3.0),
            ) * 1.2;
        } else if (kind == 5u) {
            // Chitin adds subtle overlapping scale ridges.
            let phase = wp.y * surface.detail.x * 1.8 + n_val * 2.2;
            let scale = sin(phase);
            n_val = mix(n_val, scale * 0.5 + 0.5, 0.35);
            grad.y += cos(phase) * 1.4;
        }

        let tangent_grad = grad - pbr_input.N * dot(grad, pbr_input.N);
        pbr_input.N = normalize(pbr_input.N - tangent_grad * (surface.detail.y * 0.12));

        let centered = (n_val - 0.5) * 2.0;
        let wear_bias = mix(0.0, max(0.0, centered) * 0.08, surface.finish.y);
        pbr_input.material.perceptual_roughness = clamp(
            pbr_input.material.perceptual_roughness + centered * surface.detail.z + wear_bias,
            0.05,
            1.0,
        );
        pbr_input.material.base_color = vec4<f32>(
            clamp(pbr_input.material.base_color.rgb * (1.0 + centered * surface.detail.w), vec3(0.0), vec3(1.0)),
            pbr_input.material.base_color.a,
        );

        let ndotv = saturate(dot(pbr_input.N, pbr_input.V));
        let rim = pow(1.0 - ndotv, 3.2) * surface.finish.x;
        if (kind == 7u) {
            pbr_input.material.emissive += vec4<f32>(pbr_input.material.base_color.rgb * rim * 1.5, 0.0);
        } else if (kind == 0u) {
            // Soft velvety fuzz sheen on cloth at grazing angles.
            let sheen = pow(1.0 - ndotv, 2.2) * surface.finish.x * 0.35;
            pbr_input.material.emissive += vec4<f32>(pbr_input.material.base_color.rgb * sheen, 0.0);
        } else if (kind == 4u) {
            // Soft organic wrap scatter on skin at grazing angles.
            let wrap = pow(1.0 - ndotv, 2.5) * surface.finish.x * 0.45;
            pbr_input.material.emissive += vec4<f32>(pbr_input.material.base_color.rgb * wrap, 0.0);
        } else {
            pbr_input.material.emissive += vec4<f32>(pbr_input.material.base_color.rgb * rim * 0.25, 0.0);
        }
    }

    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    // Match the standard forward pass. Undisclosed light must not tint
    // our client's intentionally unlit silhouettes.
    if (lit) {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
