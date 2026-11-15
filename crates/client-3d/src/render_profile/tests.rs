use super::*;
use bevy::ecs::system::RunSystemOnce;

#[test]
fn full_keeps_every_effect_and_the_original_rock() {
    assert_eq!(
        RenderProfile::full(),
        RenderProfile {
            scale: 1.0,
            adaptive_scale: false,
            rock_noise: RockNoise::Hashed,
            rock_lighting: RockLighting::Full,
            ambient_occlusion: true,
            bloom: true,
            antialiasing: true,
            shadows: true,
            scheduling: Scheduling::Parallel,
            cloak_interval: 0,
            pin_threads: false,
        }
    );
    assert_eq!(RenderProfile::full().rock_switches(), Vec2::ZERO);
}

#[test]
fn the_desktop_default_draws_as_it_did_before_profiles() {
    assert_eq!(RenderProfile::for_platform(false), RenderProfile::full());
}

#[test]
fn android_defaults_to_the_low_profile() {
    let low = RenderProfile::for_platform(true);
    assert_eq!(low, RenderProfile::low());
    assert!(low.scale < 1.0 && low.adaptive_scale);
    assert!(!low.ambient_occlusion && !low.bloom && !low.shadows);
    assert_eq!(low.rock_switches(), Vec2::ONE);
}

#[test]
fn an_empty_choice_is_the_platform_default() {
    assert_eq!(RenderProfile::parse(false, ""), Ok(RenderProfile::full()));
    assert_eq!(RenderProfile::parse(true, " "), Ok(RenderProfile::low()));
}

#[test]
fn a_name_replaces_the_profile_and_settings_change_one_thing_each() {
    assert_eq!(
        RenderProfile::parse(true, "full"),
        Ok(RenderProfile::full())
    );
    assert_eq!(RenderProfile::parse(false, "low"), Ok(RenderProfile::low()));
    assert_eq!(
        RenderProfile::parse(
            false,
            "low, scale=0.67, adaptive=off, noise=hashed, lighting=full, occlusion=on, \
             bloom=1, antialiasing=false, shadows=true, scheduling=parallel, cloaks=3, pin=0"
        ),
        Ok(RenderProfile {
            scale: 0.67,
            adaptive_scale: false,
            rock_noise: RockNoise::Hashed,
            rock_lighting: RockLighting::Full,
            ambient_occlusion: true,
            bloom: true,
            antialiasing: false,
            shadows: true,
            scheduling: Scheduling::Parallel,
            cloak_interval: 3,
            pin_threads: false,
        })
    );
    // A later name discards earlier settings.
    assert_eq!(
        RenderProfile::parse(false, "bloom=off,full"),
        Ok(RenderProfile::full())
    );
}

#[test]
fn unknown_or_malformed_choices_are_rejected() {
    for text in [
        "ultra",
        "standard",
        "sharpness=1",
        "scale=2",
        "scale=0.1",
        "scale=half",
        "bloom=maybe",
        "noise=fractal",
        "lighting=flat",
        "scheduling=fast",
        "cloaks=-1",
    ] {
        assert!(RenderProfile::parse(false, text).is_err(), "{text}");
    }
}

#[test]
fn rock_switches_follow_noise_and_lighting_separately() {
    let full = RenderProfile::full();
    assert_eq!(
        RenderProfile {
            rock_noise: RockNoise::Baked,
            ..full
        }
        .rock_switches(),
        Vec2::new(1.0, 0.0)
    );
    assert_eq!(
        RenderProfile {
            rock_lighting: RockLighting::Diffuse,
            ..full
        }
        .rock_switches(),
        Vec2::new(0.0, 1.0)
    );
}

fn camera_world(profile: RenderProfile) -> (World, Entity) {
    let mut world = World::new();
    world.insert_resource(profile);
    let camera = world
        .spawn((
            Camera3d::default(),
            crate::graphics::camera_antialiasing(),
            Bloom::NATURAL,
        ))
        .id();
    world.run_system_once(cameras).unwrap();
    (world, camera)
}

#[test]
fn cameras_lose_only_the_effects_their_profile_leaves_out() {
    let (world, camera) = camera_world(RenderProfile::low());
    assert!(world.get::<ScreenSpaceAmbientOcclusion>(camera).is_none());
    assert!(world.get::<DepthPrepass>(camera).is_none());
    assert!(world.get::<NormalPrepass>(camera).is_none());
    assert!(world.get::<Bloom>(camera).is_none());
    assert!(
        world.get::<Fxaa>(camera).is_some(),
        "low keeps antialiasing"
    );

    let (world, camera) = camera_world(RenderProfile {
        antialiasing: false,
        ..RenderProfile::full()
    });
    assert!(world.get::<ScreenSpaceAmbientOcclusion>(camera).is_some());
    assert!(world.get::<Bloom>(camera).is_some());
    assert!(world.get::<Fxaa>(camera).is_none());
}

#[test]
fn a_camera_is_adjusted_once_so_later_choices_by_the_scene_stand() {
    let (mut world, camera) = camera_world(RenderProfile::low());
    world.entity_mut(camera).insert(Bloom::NATURAL);
    world.run_system_once(cameras).unwrap();
    assert!(world.get::<Bloom>(camera).is_some());
}

#[test]
fn lights_stop_casting_shadows() {
    let mut world = World::new();
    let point = world
        .spawn(PointLight {
            shadows_enabled: true,
            ..default()
        })
        .id();
    let sun = world
        .spawn(DirectionalLight {
            shadows_enabled: true,
            ..default()
        })
        .id();
    let spot = world
        .spawn(SpotLight {
            shadows_enabled: true,
            ..default()
        })
        .id();
    world.run_system_once(lights).unwrap();
    assert!(!world.get::<PointLight>(point).unwrap().shadows_enabled);
    assert!(!world.get::<DirectionalLight>(sun).unwrap().shadows_enabled);
    assert!(!world.get::<SpotLight>(spot).unwrap().shadows_enabled);
}

fn material() -> CaveMaterial {
    CaveMaterial {
        base: StandardMaterial::default(),
        extension: crate::cave::CaveExtension {
            params: crate::cave::CaveParams::default(),
            lattice: None,
            sums: None,
        },
    }
}

fn rock_world(profile: RenderProfile) -> (World, Handle<CaveMaterial>) {
    let mut world = World::new();
    world.insert_resource(profile);
    world.init_resource::<Baked>();
    world.init_resource::<Assets<Image>>();
    let mut materials = Assets::<CaveMaterial>::default();
    let handle = materials.add(material());
    world.insert_resource(materials);
    world.run_system_once(rock).unwrap();
    (world, handle)
}

#[test]
fn baked_rock_receives_both_textures_and_its_switches() {
    let (mut world, handle) = rock_world(RenderProfile::low());
    let materials = world.resource::<Assets<CaveMaterial>>();
    let extension = &materials.get(&handle).unwrap().extension;
    assert_eq!(extension.params.light.y, 1.0);
    assert_eq!(extension.params.light.z, 1.0);
    let lattice = extension.lattice.clone().expect("lattice texture");
    let sums = extension.sums.clone().expect("sums texture");
    assert_eq!(world.resource::<Assets<Image>>().len(), 2);

    // A material added later shares the same textures.
    let later = world.resource_mut::<Assets<CaveMaterial>>().add(material());
    world.run_system_once(rock).unwrap();
    let materials = world.resource::<Assets<CaveMaterial>>();
    let extension = &materials.get(&later).unwrap().extension;
    assert_eq!(extension.lattice.as_ref(), Some(&lattice));
    assert_eq!(extension.sums.as_ref(), Some(&sums));
    assert_eq!(world.resource::<Assets<Image>>().len(), 2);
}

#[test]
fn hashed_diffuse_rock_gets_its_switch_and_no_textures() {
    let (world, handle) = rock_world(RenderProfile {
        rock_lighting: RockLighting::Diffuse,
        ..RenderProfile::full()
    });
    let materials = world.resource::<Assets<CaveMaterial>>();
    let extension = &materials.get(&handle).unwrap().extension;
    assert_eq!(extension.params.light.y, 0.0);
    assert_eq!(extension.params.light.z, 1.0);
    assert!(extension.lattice.is_none() && extension.sums.is_none());
    assert!(world.resource::<Assets<Image>>().is_empty());
}

#[test]
fn the_room_light_is_left_alone() {
    let (world, handle) = rock_world(RenderProfile::low());
    let materials = world.resource::<Assets<CaveMaterial>>();
    let light = materials.get(&handle).unwrap().extension.params.light;
    assert_eq!(light.x, crate::cave::CaveParams::default().light.x);
}

#[test]
#[allow(clippy::approx_constant)]
fn the_lattice_holds_the_shader_hash_at_each_cell() {
    let data = baked::lattice();
    assert_eq!(data.len(), baked::LATTICE.pow(3));
    for [x, y, z] in [[0, 0, 0], [1, 0, 0], [5, 17, 31], [31, 31, 31]] {
        let expected = (baked::hash(Vec3::new(x as f32, y as f32, z as f32)) * 255.0).round();
        let texel = data[(z * baked::LATTICE + y) * baked::LATTICE + x];
        assert_eq!(f32::from(texel), expected, "{x},{y},{z}");
    }
    // An independent evaluation of the shader's formula at one point.
    let q = [
        (3.0_f32 * 0.318_309_9 + 0.71).fract() * 17.0,
        (4.0_f32 * 0.318_309_9 + 0.113).fract() * 17.0,
        (5.0_f32 * 0.318_309_9 + 0.419).fract() * 17.0,
    ];
    let by_hand = (q[0] * q[1] * q[2] * (q[0] + q[1] + q[2])).fract();
    assert!((baked::hash(Vec3::new(3.0, 4.0, 5.0)) - by_hand).abs() < 1e-5);
}

#[test]
fn baked_sums_span_the_noise_range_and_carry_their_own_slope() {
    let data = baked::sums();
    let edge = baked::SUMS;
    assert_eq!(data.len(), edge.pow(3) * 4);
    let texel = |x: usize, y: usize, z: usize| {
        let at = ((z * edge + y) * edge + x) * 4;
        [data[at], data[at + 1], data[at + 2], data[at + 3]]
    };
    let values: Vec<u8> = data.iter().step_by(4).copied().collect();
    let (low, high) = (values.iter().min().unwrap(), values.iter().max().unwrap());
    // Four octaves sum to at most 0.9375, and noise this varied uses most of it.
    assert!(*high <= 240, "{high}");
    assert!(*low < 60 && *high > 180, "{low}..{high}");

    // The slope channels are the central difference of the sum, including
    // across the edge where the cube repeats.
    for [x, y, z] in [
        [0, 0, 0],
        [edge - 1, 5, 9],
        [40, edge - 1, 0],
        [7, 8, edge - 1],
    ] {
        let wrap = |v: usize, d: isize| (v as isize + d).rem_euclid(edge as isize) as usize;
        let here = texel(x, y, z);
        let expected = |ahead: u8, behind: u8| {
            let slope = (f32::from(ahead) - f32::from(behind)) / 255.0 * 4.0 / baked::SLOPE_RANGE;
            slope.clamp(-1.0, 1.0) * 0.5 + 0.5
        };
        let pairs = [
            (texel(wrap(x, 1), y, z)[0], texel(wrap(x, -1), y, z)[0]),
            (texel(x, wrap(y, 1), z)[0], texel(x, wrap(y, -1), z)[0]),
            (texel(x, y, wrap(z, 1))[0], texel(x, y, wrap(z, -1))[0]),
        ];
        for (axis, (ahead, behind)) in pairs.into_iter().enumerate() {
            let stored = f32::from(here[1 + axis]) / 255.0;
            // Each stored sum is rounded to a byte before this comparison.
            assert!(
                (stored - expected(ahead, behind)).abs() < 0.02,
                "axis {axis} at {x},{y},{z}: {stored} against {}",
                expected(ahead, behind)
            );
        }
    }
}

#[test]
fn baked_images_repeat_and_filter() {
    use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler};
    for (image, edge) in [
        (baked::lattice_image(), baked::LATTICE),
        (baked::sums_image(), baked::SUMS),
    ] {
        let size = image.texture_descriptor.size;
        assert_eq!(
            (size.width, size.height, size.depth_or_array_layers),
            (edge as u32, edge as u32, edge as u32)
        );
        let ImageSampler::Descriptor(sampler) = &image.sampler else {
            panic!("explicit sampler");
        };
        assert!(matches!(sampler.address_mode_u, ImageAddressMode::Repeat));
        assert!(matches!(sampler.address_mode_v, ImageAddressMode::Repeat));
        assert!(matches!(sampler.address_mode_w, ImageAddressMode::Repeat));
        assert!(matches!(sampler.mag_filter, ImageFilterMode::Linear));
    }
}

#[test]
fn the_governor_steps_down_when_frames_miss_and_no_further_than_a_fifth() {
    let profile = RenderProfile::low();
    let mut governor = ScaleGovernor::default();
    assert_eq!(governor.scale(&profile), 0.5);
    assert!(!governor.observe(33.4, 2.0), "on pace");
    assert!(governor.observe(40.0, 2.0));
    assert!((governor.scale(&profile) - 0.45).abs() < 1e-6);
    assert!(governor.observe(40.0, 2.0));
    assert!((governor.scale(&profile) - 0.40).abs() < 1e-6);
    assert!(!governor.observe(60.0, 2.0), "already at the floor");
    assert!((governor.scale(&profile) - 0.40).abs() < 1e-6);
}

#[test]
fn the_governor_ignores_stalls() {
    let mut governor = ScaleGovernor::default();
    assert!(!governor.observe(450.0, 2.0));
    assert_eq!(governor.scale(&RenderProfile::low()), 0.5);
}

#[test]
fn the_governor_retries_the_chosen_scale_and_waits_longer_after_each_failure() {
    let profile = RenderProfile::low();
    let mut governor = ScaleGovernor::default();
    assert!(governor.observe(40.0, 2.0));
    // Fourteen steady seconds are not enough; the fifteenth is.
    for _ in 0..7 {
        assert!(!governor.observe(33.0, 2.0));
    }
    assert!(governor.observe(33.0, 2.0));
    assert_eq!(governor.scale(&profile), 0.5);
    // The trial fails, so the next one needs thirty steady seconds.
    assert!(governor.observe(40.0, 2.0));
    for _ in 0..14 {
        assert!(!governor.observe(33.0, 2.0));
    }
    assert!(governor.observe(33.0, 2.0));
    // A trial that holds for six seconds restores the usual patience.
    for _ in 0..3 {
        assert!(!governor.observe(33.0, 2.0));
    }
    assert!(governor.observe(40.0, 2.0));
    for _ in 0..7 {
        assert!(!governor.observe(33.0, 2.0));
    }
    assert!(governor.observe(33.0, 2.0));
}
