// Cave rock: stratified slate walls with mineral flecks; umber floors with
// gravel, damp patches and faintly glowing moss. Rock between the camera and
// the player dissolves in a dithered cutaway.
#ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io::VertexOutput
#ifndef CAVE_SHADOW_PASS
#import bevy_pbr::prepass_io::FragmentOutput
#ifdef MOTION_VECTOR_PREPASS
#import bevy_pbr::pbr_prepass_functions::calculate_motion_vector
#endif
#endif
#else
#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings as view_bindings,
    clustered_forward as clustering,
    shadows,
    pbr_types::PbrInput,
}
#endif

struct CaveParams {
    player: vec4<f32>,
    camera: vec4<f32>,
    wall_a: vec4<f32>,
    wall_b: vec4<f32>,
    floor_a: vec4<f32>,
    floor_b: vec4<f32>,
    moss: vec4<f32>,
    accent: vec4<f32>,
    light: vec4<f32>,
}

@group(2) @binding(100) var<uniform> cave: CaveParams;
// The render profile's two rock switches travel in `cave.light`: `y` above a
// half reads surface noise from the baked textures below instead of hashing it
// for every pixel, and `z` above a half lights the rock as a matte surface.
//
// Lattice values of `hash`, one texel per cell, repeating every 32 cells.
@group(2) @binding(101) var cave_lattice: texture_3d<f32>;
@group(2) @binding(102) var cave_noise_sampler: sampler;
// r: four octaves of noise repeating every 16 units; gba: its slope / 3.
@group(2) @binding(103) var cave_sums: texture_3d<f32>;

fn baked_noise() -> bool {
    return cave.light.y > 0.5;
}

fn hash(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.3183099 + vec3<f32>(0.71, 0.113, 0.419));
    q = q * 17.0;
    return fract(q.x * q.y * q.z * (q.x + q.y + q.z));
}

fn vnoise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    if (baked_noise()) {
        // One filtered read between lattice texels equals the eight-corner mix.
        return textureSampleLevel(cave_lattice, cave_noise_sampler, (i + u + vec3(0.5)) / 32.0, 0.0).r;
    }
    return mix(
        mix(mix(hash(i + vec3(0.0, 0.0, 0.0)), hash(i + vec3(1.0, 0.0, 0.0)), u.x),
            mix(hash(i + vec3(0.0, 1.0, 0.0)), hash(i + vec3(1.0, 1.0, 0.0)), u.x), u.y),
        mix(mix(hash(i + vec3(0.0, 0.0, 1.0)), hash(i + vec3(1.0, 0.0, 1.0)), u.x),
            mix(hash(i + vec3(0.0, 1.0, 1.0)), hash(i + vec3(1.0, 1.0, 1.0)), u.x), u.y),
        u.z);
}

fn fbm(p: vec3<f32>) -> f32 {
    if (baked_noise()) {
        return textureSampleLevel(cave_sums, cave_noise_sampler, p / 16.0, 0.0).r;
    }
    var sum = 0.0;
    var amp = 0.5;
    var q = p;
    for (var i = 0; i < 4; i++) {
        sum += amp * vnoise(q);
        q = q * 2.07 + vec3(13.7, 7.1, 3.3);
        amp *= 0.5;
    }
    return sum;
}

fn bayer(frag: vec2<f32>) -> f32 {
    let x = u32(frag.x) % 4u;
    let y = u32(frag.y) % 4u;
    var m = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    return (m[y * 4u + x] + 0.5) / 16.0;
}

// Keep camera colour and normal/depth coverage identical down to the Bayer
// pixel. Shadow views reuse this world-space opening; only its dithered
// transition is sampled on the light's framebuffer instead of the camera's.
fn discard_cutaway(wp: vec3<f32>, frag_coord: vec2<f32>) {
    // Dissolve rock above the floor that stands between camera and player.
    if (wp.y > cave.player.w + 0.12) {
        let chest = cave.player.xyz;
        let to_cam = normalize(cave.camera.xyz - chest);
        let v = wp - chest;
        let t = dot(v, to_cam);
        // Only rock on the camera's side of the player, never behind it.
        let ahead = dot(v.xz, normalize(to_cam.xz));
        // Walls down: rock on the camera's side stands only knee high, so the
        // near row of every room (stairs, items, creatures) stays in view.
        // The cut eases in just ahead of the player and dithers at its top.
        let stub = cave.player.w + 0.45;
        if (ahead > 0.6 && wp.y > stub) {
            let edge = smoothstep(stub, stub + 0.3, wp.y) * smoothstep(0.6, 1.4, ahead);
            if (edge > bayer(frag_coord)) {
                discard;
            }
        }
        if (t > -0.3 && ahead > 0.35) {
            let perp = length(v - to_cam * t);
            let r = cave.camera.w + max(t, 0.0) * 0.12;
            let cut = smoothstep(r, r * 0.6, perp);
            if (cut > bayer(frag_coord)) {
                discard;
            }
        }
    }
}

#ifdef PREPASS_PIPELINE
#ifdef CAVE_SHADOW_PASS
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
@fragment
fn fragment(in: VertexOutput) -> @builtin(frag_depth) f32 {
    discard_cutaway(in.world_position.xyz, in.position.xy);
    return in.unclipped_depth;
}
#else
@fragment
fn fragment(in: VertexOutput) {
    discard_cutaway(in.world_position.xyz, in.position.xy);
}
#endif
#else
@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var out: FragmentOutput;
#ifdef NORMAL_PREPASS
    // Keep the main-camera prepass pixel-identical to the colour pass.
    discard_cutaway(in.world_position.xyz, in.position.xy);
    out.normal = vec4(normalize(in.world_normal) * 0.5 + vec3(0.5), 1.0);
#endif
#ifdef MOTION_VECTOR_PREPASS
    out.motion_vector = calculate_motion_vector(in.world_position, in.previous_world_position);
#endif
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif
    return out;
}
#endif
#else
// Ambient, clustered point lights and directional lights on a matte surface,
// for GPUs that cannot afford the full model: no highlights, and shadows only
// from directional lights that cast them.
fn diffuse_lighting(in: PbrInput) -> vec4<f32> {
    let N = in.N;
    let P = in.world_position.xyz;
    var light = view_bindings::lights.ambient_color.rgb;
    let view_z = dot(vec4<f32>(
        view_bindings::view.view_from_world[0].z,
        view_bindings::view.view_from_world[1].z,
        view_bindings::view.view_from_world[2].z,
        view_bindings::view.view_from_world[3].z
    ), in.world_position);
    let cluster = clustering::fragment_cluster_index(in.frag_coord.xy, view_z, in.is_orthographic);
    var ranges = clustering::unpack_clusterable_object_index_ranges(cluster);
    for (var i: u32 = ranges.first_point_light_index_offset;
            i < ranges.first_spot_light_index_offset;
            i = i + 1u) {
        let id = clustering::get_clusterable_object_id(i);
        let to_light = view_bindings::clusterable_objects.data[id].position_radius.xyz - P;
        let colour = view_bindings::clusterable_objects.data[id].color_inverse_square_range;
        let distance_square = max(dot(to_light, to_light), 0.0001);
        let factor = distance_square * colour.w;
        let smooth_factor = saturate(1.0 - factor * factor);
        let facing = saturate(dot(N, to_light * inverseSqrt(distance_square)));
        light += colour.rgb * (smooth_factor * smooth_factor / distance_square * facing * 0.318309886);
    }
    let suns = view_bindings::lights.n_directional_lights;
    for (var i: u32 = 0u; i < suns; i = i + 1u) {
        var facing = saturate(dot(N, view_bindings::lights.directional_lights[i].direction_to_light.xyz));
        // Bit 1 marks a directional light whose shadows are enabled.
        if ((view_bindings::lights.directional_lights[i].flags & 1u) != 0u) {
            facing *= shadows::fetch_directional_shadow(i, in.world_position, in.world_normal, view_z);
        }
        light += view_bindings::lights.directional_lights[i].color.rgb * (facing * 0.318309886);
    }
    let lit = in.material.base_color.rgb * light + in.material.emissive.rgb;
    return vec4(lit * view_bindings::view.exposure, 1.0);
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let wp = in.world_position.xyz;
    let n = normalize(in.world_normal);

    discard_cutaway(wp, in.position.xy);

    let floor_w = smoothstep(0.55, 0.85, n.y);
    // Most rock is pure wall or pure floor; each is shaded only where it shows.
    var wall = vec3(0.0);
    var fleck = 0.0;
    if (floor_w < 1.0) {
        // Walls: broad tone, strata, crevices and bright mineral flecks.
        let broad = fbm(wp * 0.45);
        let strata = sin(wp.y * 8.5 + fbm(wp * 0.7) * 6.0) * 0.5 + 0.5;
        wall = mix(cave.wall_a.rgb, cave.wall_b.rgb, smoothstep(0.3, 0.7, broad));
        wall *= 0.8 + 0.28 * strata;
        let crevice = smoothstep(0.45, 0.2, fbm(wp * 3.1 + vec3(5.0)));
        wall *= 1.0 - 0.45 * crevice;
        fleck = smoothstep(0.9, 0.96, vnoise(wp * 17.0));
        wall = mix(wall, cave.accent.rgb, fleck * 0.5 * (1.0 - floor_w));
    }

    var ground = vec3(0.0);
    var moss = 0.0;
    var wet = 0.0;
    if (floor_w > 0.0) {
        // Floors: packed earth, gravel, damp hollows and moss.
        ground = mix(cave.floor_a.rgb, cave.floor_b.rgb, fbm(wp * 1.7));
        // Sparse gravel only where the ground is loose, and dark cracks between
        // packed plates.
        let loose = smoothstep(0.45, 0.65, fbm(wp * 0.9 + vec3(3.0, 0.0, 8.0)));
        let gravel = smoothstep(0.8, 0.86, vnoise(wp * 9.0)) * loose;
        ground = mix(ground, ground * 1.35 + vec3(0.03), gravel * 0.45);
        let plates = abs(fbm(wp * vec3(1.1, 0.0, 1.1) + vec3(0.0, 2.0, 0.0)) - 0.5);
        let crack = smoothstep(0.016, 0.0, plates) * (1.0 - loose)
            * smoothstep(0.35, 0.6, vnoise(wp * 0.7 + vec3(4.0)));
        ground *= 1.0 - 0.45 * crack;
        ground *= 0.85 + 0.3 * fbm(wp * 0.3 + vec3(11.0));
        moss = smoothstep(0.56, 0.68, fbm(wp * 0.8 + vec3(7.3, 1.1, 4.2)));
        ground = mix(ground, cave.moss.rgb * (0.8 + 0.4 * vnoise(wp * 9.0)), moss);
        wet = smoothstep(0.6, 0.68, fbm(wp * 0.55 + vec3(31.0, 2.0, 9.0))) * (1.0 - moss);
        ground *= 1.0 - 0.38 * wet;
    }

    var color = mix(wall, ground, floor_w);
    // Tops of walls cut off above the body read as dark cross-sections.
    let cut_top = floor_w * smoothstep(cave.player.w + 1.5, cave.player.w + 1.7, wp.y);
    color = mix(color, vec3(0.025, 0.024, 0.035), cut_top);

    // RGB is geometric occlusion; alpha separately encodes the chart state:
    // 0 remembered, 0.5 perceived without light, 1 physically illuminated.
    let coverage = pbr_input.material.base_color.a;
    let lit = select(0.0, 1.0, coverage > 0.75);
    let shade = pbr_input.material.base_color.rgb;
    pbr_input.material.base_color = vec4(color * shade, 1.0);
    pbr_input.material.perceptual_roughness = mix(0.88, 0.22, wet * floor_w);
    pbr_input.material.reflectance = vec3(mix(0.3, 0.6, wet * floor_w));
    let glow = cave.moss.rgb * cave.moss.w * moss * floor_w
        + cave.accent.rgb * fleck * 0.25 * (1.0 - floor_w);
    let room = color * shade * mix(vec3(0.64, 0.76, 0.95), vec3(1.0, 0.9, 0.78), lit) * cave.light.x;
    pbr_input.material.emissive = vec4((glow * shade * lit + room) * (1.0 - cut_top), 1.0);

    if (coverage < 0.25) {
        // Map memory has texture and shape, but cannot pick up a moving
        // torch, cast a colorful reflection or masquerade as current sight.
        let gray = dot(color * shade, vec3(0.2126, 0.7152, 0.0722));
        let value = gray * (0.16 + 0.17 * floor_w);
        var remembered: FragmentOutput;
        remembered.color = main_pass_post_lighting_processing(pbr_input, vec4(vec3(value), 1.0));
        return remembered;
    }

    // Bump the facets with fine grain so torchlight rakes across the rock.
    var grad = vec3(0.0);
    if (baked_noise()) {
        // The baked slope is per unit of noise; the grain is four times finer.
        let slope = textureSampleLevel(cave_sums, cave_noise_sampler, wp * 0.25, 0.0).gba;
        grad = (slope * 2.0 - 1.0) * 12.0;
    } else {
        let e = 0.03;
        let h0 = fbm(wp * 4.0);
        grad = vec3(fbm((wp + vec3(e, 0.0, 0.0)) * 4.0) - h0,
                    fbm((wp + vec3(0.0, e, 0.0)) * 4.0) - h0,
                    fbm((wp + vec3(0.0, 0.0, e)) * 4.0) - h0) / e;
    }
    let tangent_grad = grad - pbr_input.N * dot(grad, pbr_input.N);
    pbr_input.N = normalize(pbr_input.N - tangent_grad * mix(0.05, 0.03, floor_w));

    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    var out: FragmentOutput;
    if (cave.light.z > 0.5) {
        out.color = diffuse_lighting(pbr_input);
    } else {
        out.color = apply_pbr_lighting(pbr_input);
    }
    if (lit < 0.5) {
        // A disclosed dark cell should read close to torchlight at play
        // distance. This is camera fill, not another in-world light source.
        out.color = vec4(out.color.rgb * vec3(1.05, 1.12, 1.20), out.color.a);
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
#endif
