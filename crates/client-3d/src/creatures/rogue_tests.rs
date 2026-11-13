//! The Rogue scenario's whole bestiary: every monster it names is drawn with a
//! recipe of its own, with the rig landmarks its body plan needs.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use tor_client_common::Palette;
use tor_protocol::{PaletteBody, PaletteUpdate};

/// The monsters the Rogue scenario names in its dungeon asset list; the
/// player's own body is named elsewhere in the manifest.
fn rogue_creature_assets() -> BTreeSet<String> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios/rogue-exploration/scenario.toml");
    let text = std::fs::read_to_string(&manifest).expect("rogue scenario manifest");
    let dungeon = text
        .split("dungeon = [")
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .expect("dungeon asset list");
    dungeon
        .split('"')
        .filter(|token| token.starts_with("creature."))
        .map(str::to_owned)
        .collect()
}

/// Each Rogue asset with the model the client resolves for it.
fn rogue_models() -> Vec<(String, CreatureModel)> {
    let assets = rogue_creature_assets();
    let mut palette = Palette::default();
    palette.apply(&PaletteUpdate {
        revision: 1,
        body: PaletteBody::Full {
            assets: assets.clone(),
        },
    });
    assets
        .into_iter()
        .map(|asset| {
            let model = tor_client_3d_model::creature_model(&palette, Some(&asset));
            (asset, model)
        })
        .collect()
}

#[test]
fn every_rogue_model_has_generated_body_meshes_for_camera_visibility() {
    let models = rogue_models();
    assert_eq!(models.len(), 26);
    for (asset, model) in models {
        let (mut world, _, _, _) = spawn_one(&Look::of(model, 7));
        let count = world
            .query_filtered::<Entity, With<BodySurface>>()
            .iter(&world)
            .count();
        assert!(count > 0, "{asset} has no countable body surface");
        for entity in world
            .query_filtered::<Entity, With<BodySurface>>()
            .iter(&world)
        {
            assert!(
                world.get::<Mesh3d>(entity).is_some(),
                "{asset} tags a non-mesh frame"
            );
        }
    }
}

#[test]
fn every_rogue_monster_is_drawn_with_its_own_recipe() {
    let models = rogue_models();
    assert_eq!(models.len(), 26);
    let mut recipes: BTreeMap<String, String> = BTreeMap::new();
    for (asset, model) in models {
        let look = Look::of(model, 7);
        assert_ne!(look.plan, Plan::Marker, "{asset} is drawn as a marker");
        let recipe = format!("{:?}", look.style);
        assert!(
            !matches!(look.style, Style::Common),
            "{asset} is drawn as a generic person"
        );
        if let Some(other) = recipes.insert(recipe.clone(), asset.clone()) {
            panic!("{asset} and {other} share the {recipe} recipe");
        }
    }
}

/// Spawn one creature into a fresh world, returning the world and its rig.
pub(super) fn spawn_one(look: &Look) -> (World, Rig, PropLibrary, Assets<Mesh>) {
    let mut materials = Assets::<StandardMaterial>::default();
    let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
    let mut meshes = Assets::<Mesh>::default();
    let mut world = World::new();
    let parent = world.spawn(Transform::default()).id();
    let mut queue = bevy::ecs::world::CommandQueue::default();
    let rig = spawn(
        &mut Commands::new(&mut queue, &world),
        &mut library,
        &mut meshes,
        parent,
        look,
    );
    queue.apply(&mut world);
    (world, rig, library, meshes)
}

#[test]
fn yeti_forearm_coat_keeps_volume_toward_the_resolved_wrist() {
    for woman in [false, true] {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Yeti,
                17,
                Some(Variant {
                    woman,
                    build,
                    ..default()
                }),
            );
            let body = anatomy::HumanoidBody::resolve(&look);
            let reach = body.hand.length();
            for joint in [J::ForeL, J::ForeR] {
                let scale = anatomy::storybook_part(Plan::Biped, joint);
                let points: Vec<_> = joint_surface_samples(&look, joint)
                    .into_iter()
                    .map(|(point, _)| point / scale)
                    .collect();
                let width = |near: f32, far: f32| {
                    let section: Vec<_> = points
                        .iter()
                        .filter(|p| p.y <= -reach * near && p.y >= -reach * far)
                        .collect();
                    assert!(
                        !section.is_empty(),
                        "actual forearm section must be present"
                    );
                    section
                        .iter()
                        .map(|p| p.x)
                        .fold(f32::NEG_INFINITY, f32::max)
                        - section.iter().map(|p| p.x).fold(f32::INFINITY, f32::min)
                };
                assert!(
                    width(0.15, 0.45) > 0.13 * body.build,
                    "positive witness: upper forearm carries a substantial fur coat"
                );
                let distal = width(0.65, 0.90);
                assert!(distal > 0.13 * body.build,
                    "fur volume must reach toward the wrist instead of ending in a short elbow cuff: {woman}/{build}/{joint:?} width={distal}");
            }
        }
    }
}

#[test]
fn yeti_surface_coat_keeps_a_bounded_mesh_and_reuses_warm_geometry() {
    for woman in [false, true] {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Yeti,
                17,
                Some(Variant {
                    woman,
                    build,
                    ..default()
                }),
            );
            let (mut world, _, mut library, mut meshes) = spawn_one(&look);
            let vertices: usize = meshes.iter().map(|(_, mesh)| mesh.count_vertices()).sum();
            assert!(
                vertices > 1_000 && vertices <= 35_000,
                "bounded complete Yeti geometry: woman={woman} build={build} vertices={vertices}"
            );
            let triangles: usize = meshes
                .iter()
                .map(|(_, mesh)| {
                    mesh.indices()
                        .map_or(mesh.count_vertices() / 3, |indices| indices.len() / 3)
                })
                .sum();
            assert!(
                triangles <= 42_000,
                "complete Yeti triangle budget: woman={woman} build={build} triangles={triangles}"
            );
            let count = meshes.len();
            let parent = world.spawn(Transform::default()).id();
            let mut queue = bevy::ecs::world::CommandQueue::default();
            let rig = spawn(
                &mut Commands::new(&mut queue, &world),
                &mut library,
                &mut meshes,
                parent,
                &look,
            );
            queue.apply(&mut world);
            assert!(rig.joint(J::Torso).is_some() && rig.joint(J::FootR).is_some());
            assert_eq!(meshes.len(), count, "warm furred bodies reuse every mesh");
            assert_eq!(
                meshes
                    .iter()
                    .map(|(_, mesh)| mesh.count_vertices())
                    .sum::<usize>(),
                vertices
            );
            println!("Yeti woman={woman} build={build}: {vertices} vertices, {triangles} triangles, {count} meshes; warm allocations=0");
        }
    }
}

#[test]
fn rogue_rigs_carry_their_landmarks_and_finite_meshes() {
    for (asset, model) in rogue_models() {
        let look = Look::of(model, 11);
        let (_, rig, _, meshes) = spawn_one(&look);
        let has = |j: J| rig.joint(j).is_some();
        match look.style {
            Style::Bat | Style::Kestrel => {
                for j in [
                    J::Body,
                    J::Head,
                    J::WingL,
                    J::WingR,
                    J::WingTipL,
                    J::WingTipR,
                ] {
                    assert!(has(j), "{asset} lacks {j:?}");
                }
                assert!(
                    !has(J::FootL) && !has(J::FootR),
                    "{asset} must have no planted feet to stand on"
                );
            }
            Style::Griffin | Style::Dragon => {
                for j in [
                    J::WingL,
                    J::WingR,
                    J::WingTipL,
                    J::WingTipR,
                    J::Tail,
                    J::HoofFL,
                ] {
                    assert!(has(j), "{asset} lacks {j:?}");
                }
            }
            Style::Flytrap => {
                for j in [J::Body, J::Neck, J::Torso, J::Head, J::Jaw] {
                    assert!(has(j), "{asset} lacks {j:?}");
                }
            }
            _ => {}
        }
        if matches!(
            look.style,
            Style::Dragon | Style::Jabberwock | Style::Xeroc | Style::Aquator
        ) {
            assert!(has(J::Jaw), "{asset} needs a jaw to open");
        }
        let segments = match look.style {
            Style::Medusa => 11,
            Style::Aquator => 8,
            Style::Rattlesnake | Style::Snake => geometry::SNAKE,
            Style::Ice => 6,
            _ => 0,
        };
        if segments > 0 {
            assert_eq!(rig.segments.len(), segments, "{asset} segments");
        }
        let mut vertices = 0;
        for (_, mesh) in meshes.iter() {
            let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                continue;
            };
            vertices += positions.len();
            assert!(
                positions.iter().flatten().all(|v| v.is_finite()),
                "{asset} has a non-finite vertex"
            );
        }
        assert!(vertices > 500, "{asset} has almost no geometry: {vertices}");
    }
}

#[test]
fn rogue_beasts_stand_with_their_soles_on_the_floor() {
    for model in [
        CreatureModel::Griffin,
        CreatureModel::Dragon,
        CreatureModel::Jabberwock,
        CreatureModel::Aquator,
        CreatureModel::Xeroc,
    ] {
        let (_, rig, _, _) = spawn_one(&Look::of(model, 3));
        for (thigh, shin, foot) in [
            (J::FL, J::FLs, J::HoofFL),
            (J::FR, J::FRs, J::HoofFR),
            (J::BL, J::BLs, J::HoofBL),
            (J::BR, J::BRs, J::HoofBR),
        ] {
            // Reared beasts keep their grasping hands free. Their hind soles
            // retain this floor guarantee; every ordinary quadruped keeps all four.
            if !rig.supports(foot) {
                continue;
            }
            let ankle = rig.rest[J::Body as usize]
                + rig.rest[thigh as usize]
                + rig.rest[shin as usize]
                + rig.rest[foot as usize];
            assert!(
                (rig.ankle_floor - 0.008 - ankle.y).abs() < 1e-4,
                "{model:?} {foot:?} ankle {} against floor {}",
                ankle.y,
                rig.ankle_floor
            );
        }
        assert!(
            rig.legs > 0.2 && rig.legs < 1.5,
            "{model:?} legs {}",
            rig.legs
        );
        assert!(rig.girth > 0.1, "{model:?} girth {}", rig.girth);
    }
}

#[test]
fn jabberwock_has_a_reared_chest_grasping_arms_and_crouched_hindlegs() {
    let mut failures = Vec::new();
    for woman in [false, true] {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Jabberwock,
                42,
                Some(Variant {
                    woman,
                    build,
                    ..default()
                }),
            );
            let (_, rig, _, _) = spawn_one(&look);
            for (shoulder, elbow, hand, hip, knee, foot) in [
                (J::FL, J::FLs, J::HoofFL, J::BL, J::BLs, J::HoofBL),
                (J::FR, J::FRs, J::HoofFR, J::BR, J::BRs, J::HoofBR),
            ] {
                let root = rig.rest[J::Body as usize];
                let shoulder_at = root + rig.rest[shoulder as usize];
                let hip_at = root + rig.rest[hip as usize];
                let upper_arm = rig.rest[elbow as usize];
                let forearm = rig.rest[hand as usize];
                let upper_leg = rig.rest[knee as usize];
                let lower_leg = rig.rest[foot as usize];
                let wrist_at = shoulder_at + upper_arm + forearm;
                let ankle_at = hip_at + upper_leg + lower_leg;
                // These silhouette relationships distinguish the approved
                // reared, grasping animal from four straight ground posts.
                let rise = shoulder_at.y - hip_at.y;
                let knee_bend = upper_leg.normalize().angle_between(lower_leg.normalize());
                let elbow_bend = upper_arm.normalize().angle_between(forearm.normalize());
                let clearance = wrist_at.y - ankle_at.y;
                if rise < 0.30
                    || knee_bend < 0.45
                    || elbow_bend < 0.35
                    || clearance < 0.18
                    || forearm.length() < upper_arm.length() * 0.9
                {
                    failures.push(format!("woman={woman} build={build} {shoulder:?}: shoulder/pelvis rise={rise:.4}, knee bend={knee_bend:.4}, elbow bend={elbow_bend:.4}, wrist/ankle clearance={clearance:.4}, upper/forearm lengths={:.4}/{:.4}", upper_arm.length(), forearm.length()));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn jabberwock_emits_broad_hanging_claws_instead_of_forefeet() {
    let mut failures = Vec::new();
    for build in 0..3 {
        let look = Look::with_choice(
            CreatureModel::Jabberwock,
            42,
            Some(Variant { build, ..default() }),
        );
        let (world, rig, _, meshes) = spawn_one(&look);
        for (arm, forearm, hand, foot) in [
            (J::FL, J::FLs, J::HoofFL, J::HoofBL),
            (J::FR, J::FRs, J::HoofFR, J::HoofBR),
        ] {
            // Native child transforms and Storybook mesh scale are included;
            // these are emitted hand/foot silhouettes, not recipe radii.
            let hand_points = points_under(&world, &meshes, rig.joint(hand).unwrap());
            let foot_points = points_under(&world, &meshes, rig.joint(foot).unwrap());
            assert!(!hand_points.is_empty() && !foot_points.is_empty());
            let bounds = |points: &[Vec3]| {
                points.iter().fold(
                    (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                    |(low, high), p| (low.min(*p), high.max(*p)),
                )
            };
            let (low, high) = bounds(&hand_points);
            let (foot_low, foot_high) = bounds(&foot_points);
            let width = high.x - low.x;
            let foot_width = foot_high.x - foot_low.x;
            let wrist = rig.rest[J::Body as usize]
                + rig.rest[arm as usize]
                + rig.rest[forearm as usize]
                + rig.rest[hand as usize];
            let tip_floor = wrist.y + low.y;
            let hanging_span = -low.y;
            // Broad palms and long down-curved digits are recognizable even
            // without pigments. The lowest tip remains free of the floor.
            if width < foot_width * 1.15 || hanging_span < 0.14 || tip_floor < 0.055 {
                failures.push(format!("build={build} {hand:?}: hand/foot width={width:.4}/{foot_width:.4}, hanging span={hanging_span:.4}, lowest tip height={tip_floor:.4}, bounds={low:?}..{high:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn jabberwock_walk_uses_hind_support_without_planting_its_grasping_hands() {
    let (app, root) = animate_along(CreatureModel::Jabberwock, 90, |f| {
        Vec3::Z * f as f32 * 0.025
    });
    let anim = app.world().get::<Anim>(root).unwrap();
    assert!(
        anim.contacts.rig.is_some(),
        "Jabberwock needs grounded hind support"
    );
    let contacts = anim
        .contacts
        .feet
        .iter()
        .filter(|foot| foot.is_some())
        .count();
    assert_eq!(
        contacts, 2,
        "only the two hind feet plant; grasping hands must stay free, contacts={contacts}"
    );
}

#[test]
fn beast_support_roles_work_independently_of_the_species_recipe() {
    let (mut app, root) = animate_along(CreatureModel::Dragon, 0, |_| Vec3::ZERO);
    // An unrelated rig with crossed free/supporting limbs proves that this is
    // authored role ownership, not a special case naming the Jabberwock.
    app.world_mut().get_mut::<Rig>(root).unwrap().limb_roles = [
        LimbRole::Grasp,
        LimbRole::Support,
        LimbRole::Support,
        LimbRole::Grasp,
    ];
    for frame in 0..90 {
        app.world_mut()
            .get_mut::<Transform>(root)
            .unwrap()
            .translation = Vec3::Z * frame as f32 * 0.025;
        app.update();
    }
    let contacts = &app.world().get::<Anim>(root).unwrap().contacts;
    assert!(contacts.rig.is_some());
    assert_eq!(
        contacts.feet.map(|foot| foot.is_some()),
        [false, true, true, false]
    );
}

#[test]
fn jabberwock_attacks_reach_with_both_grasping_forelimbs() {
    let (mut app, root) = animate_along(CreatureModel::Jabberwock, 20, |_| Vec3::ZERO);
    let arms =
        [J::FL, J::FR].map(|joint| app.world().get::<Rig>(root).unwrap().joint(joint).unwrap());
    let idle = arms.map(|e| app.world().get::<Transform>(e).unwrap().rotation);
    app.world_mut()
        .get_mut::<Anim>(root)
        .unwrap()
        .act(Move::Strike);
    let mut reach = [0.0f32; 2];
    for _ in 0..30 {
        app.update();
        for (i, arm) in arms.into_iter().enumerate() {
            reach[i] = reach[i]
                .max(idle[i].angle_between(app.world().get::<Transform>(arm).unwrap().rotation));
        }
    }
    assert!(
        reach.iter().all(|angle| *angle > 0.45),
        "both arms must rake, not remain planted during a neck-only bite: {reach:?}"
    );
}

#[test]
fn the_phantom_is_drawn_through_the_translucent_material() {
    for (model, ghostly) in [
        (CreatureModel::Phantom, true),
        (CreatureModel::Wraith, false),
    ] {
        let (mut world, _, library, _) = spawn_one(&Look::of(model, 5));
        let ghost = library.materials.ghost();
        let mut query = world.query::<&MeshMaterial3d<StandardMaterial>>();
        let translucent = query.iter(&world).filter(|m| m.0 == ghost).count();
        let total = query.iter(&world).count();
        if ghostly {
            assert!(
                translucent * 2 > total,
                "{translucent} of {total} parts translucent"
            );
        } else {
            assert_eq!(translucent, 0, "the wraith stays solid");
        }
    }
}

/// Run the shared animation system over a rig moved along `path` each frame.
fn animate_along(
    model: CreatureModel,
    frames: usize,
    path: impl Fn(usize) -> Vec3,
) -> (App, Entity) {
    animate_look(Look::of(model, 9), frames, path)
}

/// As `animate_along`, for an exact individual.
pub(super) fn animate_look(
    look: Look,
    frames: usize,
    path: impl Fn(usize) -> Vec3,
) -> (App, Entity) {
    use bevy::time::TimeUpdateStrategy;
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .init_resource::<Assets<Mesh>>()
        .add_systems(Update, animate);
    let mut materials = Assets::<StandardMaterial>::default();
    let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
    let root = app
        .world_mut()
        .spawn((Anim::new(9, path(0)), Transform::from_translation(path(0))))
        .id();
    let mut queue = bevy::ecs::world::CommandQueue::default();
    let (rig, meshes) = {
        let world = app.world_mut();
        let mut meshes = Assets::<Mesh>::default();
        let rig = spawn(
            &mut Commands::new(&mut queue, world),
            &mut library,
            &mut meshes,
            root,
            &look,
        );
        queue.apply(world);
        (rig, meshes)
    };
    app.insert_resource(meshes);
    app.world_mut().entity_mut(root).insert(rig);
    for frame in 0..frames {
        app.world_mut()
            .get_mut::<Transform>(root)
            .unwrap()
            .translation = path(frame);
        app.update();
    }
    (app, root)
}

#[test]
fn fliers_stay_on_the_wing_without_ground_contacts() {
    for model in [CreatureModel::Bat, CreatureModel::Kestrel] {
        let (app, root) = animate_along(model, 90, |f| Vec3::Z * f as f32 * 0.03);
        let rig = app.world().get::<Rig>(root).unwrap();
        let anim = app.world().get::<Anim>(root).unwrap();
        assert!(anim.contacts.rig.is_none(), "{model:?} must not plant feet");
        let lift = app
            .world()
            .get::<Transform>(rig.joint(J::Lift).unwrap())
            .unwrap();
        let body = lift.translation.y + rig.rest[J::Body as usize].y * rig.scale;
        assert!(body > 0.5, "{model:?} sank to {body}");
        let wing = app
            .world()
            .get::<Transform>(rig.joint(J::WingL).unwrap())
            .unwrap();
        assert!(
            wing.rotation.angle_between(Quat::IDENTITY) > 0.01,
            "{model:?} wings must beat"
        );
    }
}

#[test]
fn walking_rogue_beasts_plant_their_feet() {
    for model in [CreatureModel::Dragon, CreatureModel::Aquator] {
        let (app, root) = animate_along(model, 60, |f| Vec3::Z * f as f32 * 0.02);
        let anim = app.world().get::<Anim>(root).unwrap();
        assert!(
            anim.contacts.rig.is_some(),
            "{model:?} must walk on its own feet"
        );
    }
}

#[test]
fn rogue_signature_abilities_have_their_own_motions() {
    for (id, motion) in [
        ("rogue.breath_weapon", Move::Breath(Accent::Ember)),
        ("rogue.confusing_gaze", Move::Gaze),
        ("rogue.corrode", Move::Touch(Accent::Amber)),
        ("rogue.energy_drain", Move::Drain),
        ("rogue.ambush_from_guise", Move::Shove),
        ("rogue.venomous_bite", Move::Bite(Accent::Teal)),
    ] {
        assert_eq!(Move::for_ability(id, None), motion, "{id}");
        assert!(motion.duration() > 0.0 && (0.0..1.0).contains(&motion.impact()));
    }
}

#[test]
fn cloaks_flare_and_skirts_sway_as_their_wearers_travel() {
    let drape_angles = |moving: bool| {
        let speed = if moving { 0.03 } else { 0.0 };
        let (app, root) = animate_along(CreatureModel::Human, 60, |f| Vec3::Z * f as f32 * speed);
        let rig = app.world().get::<Rig>(root).unwrap();
        let mut cloak = None;
        let mut skirt = None;
        for &(e, kind) in &rig.drapes {
            let angle = app
                .world()
                .get::<Transform>(e)
                .unwrap()
                .rotation
                .angle_between(Quat::IDENTITY);
            match kind {
                Drape::Cloak => cloak = Some(angle),
                Drape::Skirt(_) => skirt = Some(angle),
                Drape::Hair => {}
            }
        }
        (
            cloak.expect("the warrior wears a cloak"),
            skirt.expect("and a tunic skirt"),
        )
    };
    let (cloak, skirt) = drape_angles(true);
    assert!(cloak > 0.15, "a travelling cloak flares back: {cloak}");
    assert!(skirt > 0.01, "a travelling skirt sways: {skirt}");
    let (cloak, skirt) = drape_angles(false);
    assert!(
        cloak < 0.05 && skirt < 0.03,
        "cloth at rest barely stirs: {cloak} {skirt}"
    );
}

#[test]
fn leather_jerkins_hang_free_below_the_belt_and_stay_shorter_than_the_tunic() {
    for (model, length) in [
        (CreatureModel::Human, 0.08),
        (CreatureModel::Scout, 0.08),
        (CreatureModel::Hobgoblin, 0.06),
        (CreatureModel::Orc, 0.06),
    ] {
        let look = Look::of(model, 4);
        let (_, rig, _, _) = spawn_one(&look);
        let skirts: Vec<f32> = rig
            .drapes
            .iter()
            .filter_map(|(_, kind)| match kind {
                Drape::Skirt(length) => Some(*length),
                Drape::Cloak | Drape::Hair => None,
            })
            .collect();
        let Some(outfit::Outer::Shell {
            peplum: Some(peplum),
            ..
        }) = outfit::Outfit::worn(&look).outer
        else {
            panic!("{model:?} wears a jerkin with a free hem")
        };
        assert_eq!(
            peplum.length, length,
            "the native garment keeps its authored length"
        );
        assert!(
            skirts.contains(&length),
            "{model:?} hangs its peplum as a drape"
        );
        assert!(
            skirts.iter().any(|&tunic| tunic > length),
            "{model:?}: the tunic skirt shows below the jerkin's hem"
        );
    }
    assert!(outfit::Outfit::worn(&Look::of(CreatureModel::Mage, 4))
        .outer
        .is_none());
}

/// Walk a creature at a steady pace with the real animation system and
/// return, for each foot, the median horizontal slide between consecutive
/// frames while it is planted (within 3 mm of its lowest height), and the
/// share of those frames that slide more than a millimetre.
fn planted_foot_slide(model: CreatureModel, path: &dyn Fn(usize) -> Vec3) -> Vec<(J, f32, f32)> {
    planted_foot_slide_with(Look::of(model, 21), WalkingCarriage::Native, path)
}

fn planted_foot_slide_with(
    look: Look,
    carriage: WalkingCarriage,
    path: &dyn Fn(usize) -> Vec3,
) -> Vec<(J, f32, f32)> {
    use bevy::time::TimeUpdateStrategy;
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .init_resource::<Assets<Mesh>>()
        .add_systems(Update, animate);
    let mut materials = Assets::<StandardMaterial>::default();
    let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
    let mut anim = Anim::new(21, Vec3::ZERO);
    anim.carriage = carriage;
    let root = app
        .world_mut()
        .spawn((anim, Transform::default(), Visibility::default()))
        .id();
    let mut queue = bevy::ecs::world::CommandQueue::default();
    let rig = {
        let world = app.world_mut();
        let mut meshes = Assets::<Mesh>::default();
        let rig = spawn(
            &mut Commands::new(&mut queue, world),
            &mut library,
            &mut meshes,
            root,
            &look,
        );
        queue.apply(world);
        rig
    };
    let feet: Vec<J> = [
        J::FootL,
        J::FootR,
        J::HoofFL,
        J::HoofFR,
        J::HoofBL,
        J::HoofBR,
    ]
    .into_iter()
    .filter(|j| rig.joint(*j).is_some() && rig.supports(*j))
    .collect();
    let entities: Vec<Entity> = feet.iter().map(|j| rig.joint(*j).unwrap()).collect();
    app.world_mut().entity_mut(root).insert(rig);
    let mut tracks: Vec<Vec<Vec3>> = vec![Vec::new(); feet.len()];
    for frame in 0..200 {
        app.world_mut()
            .get_mut::<Transform>(root)
            .unwrap()
            .translation = path(frame);
        app.update();
        if frame >= 60 {
            for (track, e) in tracks.iter_mut().zip(&entities) {
                track.push(
                    app.world()
                        .get::<GlobalTransform>(*e)
                        .unwrap()
                        .translation(),
                );
            }
        }
    }
    feet.into_iter()
        .zip(tracks)
        .map(|(foot, track)| {
            // The planted height: low, but robust to one unusually deep frame.
            let mut heights: Vec<f32> = track.iter().map(|p| p.y).collect();
            heights.sort_by(f32::total_cmp);
            let low = heights[heights.len() / 10];
            let mut slides: Vec<f32> = track
                .windows(2)
                .filter(|w| w[0].y < low + 0.003 && w[1].y < low + 0.003)
                .map(|w| (w[1] - w[0]).with_y(0.0).length())
                .collect();
            slides.sort_by(f32::total_cmp);
            if slides.len() < 10 {
                return (foot, f32::NAN, f32::NAN);
            }
            let median = slides[slides.len() / 2];
            let sliding =
                slides.iter().filter(|s| **s > 0.001).count() as f32 / slides.len() as f32;
            (foot, median, sliding)
        })
        .collect()
}

#[test]
fn real_speed_walk_review_keeps_planted_feet_from_sliding() {
    let mut report = Vec::new();
    for model in [
        CreatureModel::Human,
        CreatureModel::Scout,
        CreatureModel::Hobgoblin,
        CreatureModel::Zombie,
        CreatureModel::Guardian,
        CreatureModel::Yeti,
        CreatureModel::Orc,
        CreatureModel::Troll,
        CreatureModel::Leprechaun,
        CreatureModel::Emu,
        CreatureModel::Quagga,
        CreatureModel::BlackUnicorn,
        CreatureModel::Centaur,
        CreatureModel::Griffin,
        CreatureModel::Dragon,
        CreatureModel::Jabberwock,
        CreatureModel::Aquator,
        CreatureModel::Xeroc,
    ] {
        for (foot, median, sliding) in planted_foot_slide(model, &|f| Vec3::Z * f as f32 * 0.03) {
            report.push(format!(
                "{model:?} {foot:?} median {median:.4} sliding {sliding:.2}"
            ));
            assert!(
                median < 0.001,
                "{model:?} {foot:?} slides while planted: {median}"
            );
            // Touchdown and lift-off frames may move; the stance between may not.
            assert!(
                sliding < 0.2,
                "{model:?} {foot:?} drags through its stance: {sliding}"
            );
        }
    }
    eprintln!("{}", report.join("\n"));
}

/// The same review on harder paths: a steady turn, and a walk that stops,
/// settles and sets off again.
#[test]
fn real_speed_turns_and_stops_keep_planted_feet_from_sliding() {
    let turn = |f: usize| {
        let a = f as f32 * 0.02;
        Vec3::new(1.5 - 1.5 * a.cos(), 0.0, 1.5 * a.sin())
    };
    let stop = |f: usize| {
        let moving = f.min(90) + f.saturating_sub(140);
        Vec3::Z * moving as f32 * 0.03
    };
    let mut report = Vec::new();
    for model in [
        CreatureModel::Human,
        CreatureModel::Zombie,
        CreatureModel::Troll,
        CreatureModel::Emu,
        CreatureModel::Quagga,
        CreatureModel::BlackUnicorn,
        CreatureModel::Centaur,
        CreatureModel::Griffin,
        CreatureModel::Dragon,
        CreatureModel::Aquator,
    ] {
        for (name, path) in [("turn", &turn as &dyn Fn(usize) -> Vec3), ("stop", &stop)] {
            for (foot, median, sliding) in planted_foot_slide(model, path) {
                report.push(format!(
                    "{name} {model:?} {foot:?} median {median:.4} sliding {sliding:.2}"
                ));
                // Tight turns may yield a little rather than cross the legs,
                // and a stop settles each foot into place; neither may skate.
                assert!(
                    median.is_nan() || median < 0.0025,
                    "{name}: {model:?} {foot:?} skates while planted: {median}"
                );
            }
        }
    }
    eprintln!(
        "{}",
        report.join(
            "
"
        )
    );
}

/// The largest frame-to-frame jump of any foot or the head while a creature
/// walks, is interrupted by an action, falls dead and is revived.
fn largest_pop(model: CreatureModel) -> (f32, f32, f32) {
    use bevy::time::TimeUpdateStrategy;
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .init_resource::<Assets<Mesh>>()
        .add_systems(Update, animate);
    let mut materials = Assets::<StandardMaterial>::default();
    let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
    let look = Look::of(model, 33);
    let root = app
        .world_mut()
        .spawn((
            Anim::new(33, Vec3::ZERO),
            Transform::default(),
            Visibility::default(),
        ))
        .id();
    let mut queue = bevy::ecs::world::CommandQueue::default();
    let rig = {
        let world = app.world_mut();
        let mut meshes = Assets::<Mesh>::default();
        let rig = spawn(
            &mut Commands::new(&mut queue, world),
            &mut library,
            &mut meshes,
            root,
            &look,
        );
        queue.apply(world);
        rig
    };
    let watched: Vec<Entity> = [
        J::FootL,
        J::FootR,
        J::HoofFL,
        J::HoofFR,
        J::HoofBL,
        J::HoofBR,
        J::Head,
    ]
    .into_iter()
    .filter_map(|j| rig.joint(j))
    .collect();
    app.world_mut().entity_mut(root).insert(rig);
    let (mut interrupt, mut revive, mut react) = (0.0f32, 0.0f32, 0.0f32);
    let mut last: Vec<Vec3> = Vec::new();
    for frame in 0..320 {
        let travel = frame.min(100) as f32 * 0.03;
        app.world_mut()
            .get_mut::<Transform>(root)
            .unwrap()
            .translation = Vec3::Z * travel;
        {
            let mut anim = app.world_mut().get_mut::<Anim>(root).unwrap();
            match frame {
                30 => anim.hit(0.0),
                80 => anim.act(Move::Strike),
                140 => anim.miss(0.0),
                160 => anim.dead = Some(0.0),
                260 => anim.dead = None,
                _ => {}
            }
        }
        app.update();
        let now: Vec<Vec3> = watched
            .iter()
            .map(|e| {
                app.world()
                    .get::<GlobalTransform>(*e)
                    .unwrap()
                    .translation()
            })
            .collect();
        if !last.is_empty() {
            let jump = now
                .iter()
                .zip(&last)
                .map(|(a, b)| a.distance(*b))
                .fold(0.0, f32::max);
            if (30..70).contains(&frame) || (140..160).contains(&frame) {
                react = react.max(jump);
            } else if (80..140).contains(&frame) {
                interrupt = interrupt.max(jump);
            } else if (260..320).contains(&frame) {
                revive = revive.max(jump);
            }
        }
        last = now;
    }
    (interrupt, revive, react)
}

#[test]
fn real_speed_interruptions_and_revival_do_not_pop() {
    let mut report = Vec::new();
    for model in [
        CreatureModel::Human,
        CreatureModel::Zombie,
        CreatureModel::Troll,
        CreatureModel::Emu,
        CreatureModel::Quagga,
        CreatureModel::BlackUnicorn,
        CreatureModel::Centaur,
        CreatureModel::Griffin,
        CreatureModel::Dragon,
        CreatureModel::Aquator,
        CreatureModel::Bat,
        CreatureModel::Flytrap,
    ] {
        let (interrupt, revive, react) = largest_pop(model);
        // A flinch or sidestep while walking keeps the feet planted.
        assert!(react < 0.25, "{model:?} pops when hit or missed: {react}");
        report.push(format!(
            "{model:?} interrupt {interrupt:.3} revive {revive:.3}"
        ));
        // An action cut into the walk moves no faster than its own lunge.
        assert!(
            interrupt < 0.4,
            "{model:?} pops when interrupted: {interrupt}"
        );
        // Revival rises out of the death pose rather than snapping upright.
        assert!(revive < 0.2, "{model:?} pops when revived: {revive}");
    }
    eprintln!("{}", report.join("\n"));
}

/// The largest frame-to-frame jump of the hands and head as an action ends
/// and the idle fidgets take over again, for a creature standing still.
fn action_exit_pop(model: CreatureModel) -> f32 {
    use bevy::time::TimeUpdateStrategy;
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .init_resource::<Assets<Mesh>>()
        .add_systems(Update, animate);
    let mut materials = Assets::<StandardMaterial>::default();
    let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
    let look = Look::of(model, 5);
    let root = app
        .world_mut()
        .spawn((
            Anim::new(5, Vec3::ZERO),
            Transform::default(),
            Visibility::default(),
        ))
        .id();
    let mut queue = bevy::ecs::world::CommandQueue::default();
    let rig = {
        let world = app.world_mut();
        let mut meshes = Assets::<Mesh>::default();
        let rig = spawn(
            &mut Commands::new(&mut queue, world),
            &mut library,
            &mut meshes,
            root,
            &look,
        );
        queue.apply(world);
        rig
    };
    let watched: Vec<Entity> = [J::HandL, J::HandR, J::Head, J::Neck]
        .into_iter()
        .filter_map(|j| rig.joint(j))
        .collect();
    app.world_mut().entity_mut(root).insert(rig);
    let mut worst = 0.0f32;
    let mut last: Vec<Vec3> = Vec::new();
    // Each start is timed so the action ends while a fidget is under way.
    for frame in 0..900 {
        if frame % 150 == 40 {
            app.world_mut()
                .get_mut::<Anim>(root)
                .unwrap()
                .act(Move::Focus);
        }
        app.update();
        let now: Vec<Vec3> = watched
            .iter()
            .map(|e| {
                app.world()
                    .get::<GlobalTransform>(*e)
                    .unwrap()
                    .translation()
            })
            .collect();
        if !last.is_empty() && frame % 150 > 100 {
            let jump = now
                .iter()
                .zip(&last)
                .map(|(a, b)| a.distance(*b))
                .fold(0.0, f32::max);
            worst = worst.max(jump);
        }
        last = now;
    }
    worst
}

#[test]
fn ending_an_action_hands_back_to_the_idle_without_a_snap() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Mage,
        CreatureModel::Vampire,
        CreatureModel::Quagga,
        CreatureModel::BlackUnicorn,
    ] {
        let pop = action_exit_pop(model);
        assert!(pop < 0.12, "{model:?} snaps back to its idle: {pop}");
    }
}

#[test]
fn at_rest_women_stand_with_their_weight_on_one_leg() {
    let tilt = |woman: bool| {
        let look = Look::with_choice(
            CreatureModel::Human,
            9,
            Some(Variant {
                woman,
                // This long idle witness owns hip posing, not loose-hair fitting.
                // Hair contact has separate full-mesh jump/isolation witnesses.
                hair: if woman { 4 } else { 1 },
                ..Default::default()
            }),
        );
        let (mut app, root) = animate_look(look, 1, |_| Vec3::ZERO);
        assert!(
            app.world().get::<Rig>(root).unwrap().hair.is_none(),
            "pose fixture must not spend its 4000 frames fitting loose hair"
        );
        let hips = app
            .world()
            .get::<Rig>(root)
            .unwrap()
            .joint(J::Hips)
            .unwrap();
        let mut most = 0.0f32;
        for _ in 0..2000 {
            app.update();
            let rotation = app.world().get::<Transform>(hips).unwrap().rotation;
            most = most.max(rotation.to_euler(EulerRot::ZXY).0.abs());
        }
        most
    };
    let (hers, his) = (tilt(true), tilt(false));
    assert!(hers > 0.08, "her hip rises over the standing leg: {hers}");
    assert!(his < hers * 0.7, "he stands squarer: {his} against {hers}");
}

#[test]
fn walking_women_sway_their_hips_and_men_do_not() {
    let sway = |woman: bool| {
        let look = Look::with_choice(
            CreatureModel::Human,
            9,
            Some(Variant {
                woman,
                ..Default::default()
            }),
        );
        let (mut app, root) = animate_look(look, 1, |_| Vec3::ZERO);
        let hips = app
            .world()
            .get::<Rig>(root)
            .unwrap()
            .joint(J::Hips)
            .unwrap();
        let (mut low, mut high) = (f32::INFINITY, f32::NEG_INFINITY);
        for frame in 1..240 {
            app.world_mut()
                .get_mut::<Transform>(root)
                .unwrap()
                .translation = Vec3::Z * frame as f32 * 0.03;
            app.update();
            if frame > 60 {
                let roll = app
                    .world()
                    .get::<Transform>(hips)
                    .unwrap()
                    .rotation
                    .to_euler(EulerRot::ZXY)
                    .0;
                low = low.min(roll);
                high = high.max(roll);
            }
        }
        high - low
    };
    let (hers, his) = (sway(true), sway(false));
    assert!(hers > 0.1, "her hips sway as she walks: {hers}");
    assert!(his < hers * 0.6, "his stay squarer: {his} against {hers}");
}

#[test]
fn women_of_the_heeled_kinds_wear_heels_and_others_do_not() {
    for (style, heeled) in [
        (Style::Vampire, true),
        (Style::Medusa, true),
        (Style::Leprechaun, true),
        (Style::Hero, false),
        (Style::Nymph, false),
        (Style::Troll, false),
    ] {
        assert_eq!(geometry::heeled_boots(style, true), heeled, "{style:?}");
        assert!(
            !geometry::heeled_boots(style, false),
            "{style:?} men stay flat-footed"
        );
    }
}

#[test]
fn skirts_hug_the_waist_and_clear_the_thighs_for_every_figure() {
    // The spawned skirt, swept around the body in 20-degree sectors, is never
    // inside the spawned thighs anywhere down their length. The front sector
    // is left out, where some skirts are split to show the legs.
    let sectors = 18;
    let in_sector = |p: &Vec3, k: usize| {
        let lo = -PI + TAU * k as f32 / sectors as f32;
        (lo..lo + TAU / sectors as f32).contains(&p.x.atan2(p.z))
    };
    let reach = |points: &[Vec3], k: usize, y: f32| {
        points
            .iter()
            .filter(|p| (y - 0.05..y).contains(&p.y) && in_sector(p, k))
            .map(|p| Vec2::new(p.x, p.z).length())
            .fold(None, |m: Option<f32>, r| Some(m.map_or(r, |m| m.max(r))))
    };
    for model in [
        CreatureModel::Medusa,
        CreatureModel::Human,
        CreatureModel::Leprechaun,
        CreatureModel::Nymph,
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                let look = Look::with_choice(
                    model,
                    1,
                    Some(Variant {
                        woman,
                        build,
                        ..Default::default()
                    }),
                );
                let (world, rig, _, meshes) = spawn_one(&look);
                let placed = |e: Entity| {
                    let at = world.get::<Transform>(e).unwrap().compute_affine();
                    points_under(&world, &meshes, e)
                        .into_iter()
                        .map(move |p| at.transform_point3(p))
                };
                let thighs: Vec<Vec3> = [J::LegL, J::LegR]
                    .into_iter()
                    .flat_map(|j| placed(rig.joint(j).expect("legs")))
                    .collect();
                // The tunic's skirt: longer than any jerkin's peplum over it.
                let skirt = rig
                    .drapes
                    .iter()
                    .filter_map(|&(e, kind)| match kind {
                        Drape::Skirt(length) => Some((e, length)),
                        Drape::Cloak | Drape::Hair => None,
                    })
                    .max_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(e, _)| placed(e).collect::<Vec<_>>())
                    .expect("a skirt");
                // The pelvis's own meshes, without the legs and drapes it carries.
                let hips = rig.joint(J::Hips).expect("hips");
                let pelvis: Vec<Vec3> = world
                    .get::<Children>(hips)
                    .expect("pelvis")
                    .iter()
                    .filter(|&child| world.get::<Mesh3d>(child).is_some())
                    .flat_map(|child| {
                        let at = world.get::<Transform>(child).unwrap().compute_affine();
                        points_under(&world, &meshes, child)
                            .into_iter()
                            .map(move |p| at.transform_point3(p))
                    })
                    .collect();
                // The waistband sits on the hips, not standing off them.
                for k in 0..sectors {
                    let band = |points: &[Vec3]| {
                        points
                            .iter()
                            .filter(|p| (0.025..0.045).contains(&p.y) && in_sector(p, k))
                            .map(|p| Vec2::new(p.x, p.z).length())
                            .reduce(f32::max)
                    };
                    if let (Some(cloth), Some(body)) = (band(&skirt), band(&pelvis)) {
                        assert!(
                            cloth < body + 0.015,
                            "{model:?} woman {woman} build {build}: waistband {cloth:.3} stands off the hips {body:.3}, sector {k}"
                        );
                    }
                }
                // Cloth flares only as far as it needs to: never into a bell.
                // Measured behind, where no split parts the front panels.
                let flare = skirt
                    .iter()
                    .filter(|p| p.z < 0.0)
                    .map(|p| p.x.abs())
                    .fold(0.0f32, f32::max);
                // The body's hip line: the pelvis and the thigh roots below it.
                let hips = pelvis
                    .iter()
                    .chain(thighs.iter().filter(|p| p.y > -0.1))
                    .map(|p| p.x.abs())
                    .fold(0.0f32, f32::max);
                assert!(
                    flare < hips * 1.55,
                    "{model:?} woman {woman} build {build}: skirt flares to {flare:.3} over hips {hips:.3}"
                );
                let hem = skirt.iter().fold(0.0f32, |m, p| m.min(p.y));
                let mut y = 0.0;
                // A cutaway coat's fronts part around the legs: only its sides
                // and back must clear them.
                let open = |k: usize| {
                    let mid = -PI + TAU * (k as f32 + 0.5) / sectors as f32;
                    k == sectors / 2 || (geometry::cutaway_coat(look.style) && mid.cos() > 0.1)
                };
                while y > hem + 0.05 {
                    for k in (0..sectors).filter(|&k| !open(k)) {
                        if let (Some(cloth), Some(leg)) =
                            (reach(&skirt, k, y), reach(&thighs, k, y))
                        {
                            assert!(
                                cloth > leg,
                                "{model:?} woman {woman} build {build}: thigh {leg:.3} through skirt {cloth:.3} at height {y:.2}, sector {k}"
                            );
                        }
                    }
                    y -= 0.05;
                }
            }
        }
    }
}

/// Every mesh vertex under `root`, in `root`'s own space.
pub(super) fn points_under(world: &World, meshes: &Assets<Mesh>, root: Entity) -> Vec<Vec3> {
    let mut out = Vec::new();
    let mut stack = vec![(root, Affine3A::IDENTITY)];
    while let Some((e, to_root)) = stack.pop() {
        if let Some(mesh) = world.get::<Mesh3d>(e).and_then(|m| meshes.get(&m.0)) {
            if let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            {
                out.extend(
                    positions
                        .iter()
                        .map(|v| to_root.transform_point3(Vec3::from(*v))),
                );
            }
        }
        if let Some(children) = world.get::<Children>(e) {
            for child in children.iter() {
                let local = world
                    .get::<Transform>(child)
                    .map_or(Affine3A::IDENTITY, |t| t.compute_affine());
                stack.push((child, to_root * local));
            }
        }
    }
    out
}

#[path = "composition_cases.rs"]
mod composition_cases;

#[test]
fn every_dressable_recipe_keeps_identity_and_distinct_cache_keys() {
    for model in composition_cases::MODELS {
        for woman in [false, true] {
            let look = Look::with_choice(model, 2, Some(Variant { woman, ..default() }));
            assert!(look.dresses(), "{model:?} wears separable clothes");
            assert_eq!(look.clone().dressed(Wardrobe::Native).key, look.key);
            let mut keys = BTreeSet::new();
            for wardrobe in Wardrobe::ALL {
                let dressed = look.clone().dressed(wardrobe);
                assert!(
                    keys.insert(dressed.key.clone()),
                    "{model:?} {wardrobe:?} cached apart"
                );
                let _recipe = outfit::Outfit::worn(&dressed);
            }
        }
    }
}

#[test]
fn focused_and_seeded_compositions_generate_their_declared_parts() {
    let seed = std::env::var("TOR_COMPOSITION_SEED").map_or(0x5eed_cafe, |value| {
        value
            .parse::<u64>()
            .expect("TOR_COMPOSITION_SEED must be a decimal u64")
    });
    let cases = composition_cases::plan(seed);
    let selected = std::env::var("TOR_COMPOSITION_CASE").ok().map(|value| {
        value
            .parse::<usize>()
            .expect("TOR_COMPOSITION_CASE must be a nonnegative case index")
    });
    assert!(
        selected.is_none_or(|index| index < cases.len()),
        "TOR_COMPOSITION_CASE is outside this plan"
    );
    for (index, case) in cases.into_iter().enumerate() {
        if selected.is_some_and(|selected| selected != index) {
            continue;
        }
        eprintln!("composition batch_seed={seed} case={index} {case:?}");
        check_generated_composition(case);
    }
}

fn check_generated_composition(case: composition_cases::Case) {
    let model = case.model;
    let woman = case.variant.woman;
    let wardrobe = case.wardrobe;
    let dressed = Look::with_choice(model, case.seed, Some(case.variant))
        .dressed(wardrobe)
        .equipped(case.equipment);
    let outfit = outfit::Outfit::worn(&dressed);
    let (world, rig, library, meshes) = spawn_one(&dressed);
    // What the body grows stays with it whatever it wears.
    if model == CreatureModel::Medusa {
        assert!(!rig.segments.is_empty(), "{wardrobe:?} keeps her serpents");
    }
    // What the outfit puts on the head decides whether hair shows:
    // a helm hides it, and a sentinel out of her helm shows hers.
    let fall = rig.drapes.iter().any(|(_, kind)| *kind == Drape::Hair);
    assert_eq!(
        fall,
        geometry::loose_fall(&dressed),
        "{model:?} {wardrobe:?} hair"
    );
    if wardrobe == Wardrobe::Knight || case.equipment.head.is_some() {
        assert!(!fall, "{model:?} wears her hair under the helm");
    }
    // Pointed ears fold inside a helm rather than poking through.
    if wardrobe == Wardrobe::Knight || case.equipment.head.is_some() {
        let tip = geometry::rogue::ear_tip(&dressed, Vec3::new(0.24, 0.2, -0.08));
        assert!(
            tip.x <= geometry::rogue::EAR_UNDER_COVER,
            "{model:?} ears under the helm"
        );
    }
    // Hair and what is worn in it (wreaths, topknots) go under
    // a helm, and show again out of it.
    assert_eq!(
        geometry::rogue::hair_shows(&dressed),
        match wardrobe {
            Wardrobe::Knight => false,
            _ if case.equipment.head.is_some() => false,
            Wardrobe::Commoner | Wardrobe::Courtier => true,
            _ => geometry::rogue::hair_shows(&dressed),
        },
        "{model:?} {wardrobe:?} hair"
    );
    if model == CreatureModel::Sentinel && woman && wardrobe == Wardrobe::Courtier {
        assert!(fall, "out of her helm, a sentinel lets her hair down");
    }
    for (_, mesh) in meshes.iter() {
        if let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(ps)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        {
            assert!(ps.iter().flatten().all(|v| v.is_finite()));
        }
    }
    let cloaks = rig
        .drapes
        .iter()
        .filter(|(_, kind)| *kind == Drape::Cloak)
        .count();
    let skirts = rig
        .drapes
        .iter()
        .filter(|(_, kind)| matches!(kind, Drape::Skirt(_)))
        .count();
    if case.equipment.back.is_some() {
        assert_eq!(
            cloaks, 1,
            "equipped cloak must be generated once, including over native robes"
        );
    }
    if wardrobe != Wardrobe::Native {
        assert_eq!(
            cloaks,
            usize::from(outfit.cloak.is_some()),
            "{model:?} {wardrobe:?} cloak"
        );
        let peplum = matches!(
            outfit.outer,
            Some(outfit::Outer::Shell {
                peplum: Some(_),
                ..
            })
        );
        // Each layer beneath a skirt hangs and sways on its own.
        let layers = outfit
            .skirt
            .map_or(0, |skirt| skirt.layers.iter().flatten().count());
        assert_eq!(
            skirts,
            usize::from(outfit.skirt.is_some()) + usize::from(peplum) + layers,
            "{model:?} {wardrobe:?} skirts"
        );
    }
    // Plate is steel on the arms and legs; cloth outfits have none
    // there. A ghost's whole body below the head is translucent.
    if dressed.style.ghostly() {
        return;
    }
    let metal = library.materials.metal().clone();
    let steel_on = |joint: J| {
        let e = rig.joint(joint).expect("limb");
        world
            .get::<Children>(e)
            .into_iter()
            .flat_map(|children| children.iter())
            .any(|child| {
                world
                    .get::<MeshMaterial3d<StandardMaterial>>(child)
                    .is_some_and(|m| m.0 == metal)
            })
    };
    if case.equipment.body.is_some() {
        assert!(
            steel_on(J::Torso),
            "equipped breastplate must have rendered torso geometry"
        );
    }
    // Metal on the arm is plate, or jewellery worn round it.
    let plated = outfit.arms == outfit::Arms::Plate;
    let jewelled = outfit.trimmings.armlets.is_some();
    assert_eq!(
        steel_on(J::ArmL),
        plated || jewelled,
        "{model:?} {wardrobe:?} arms"
    );
    assert_eq!(
        steel_on(J::LegL),
        outfit.legs == outfit::Legs::Plate,
        "{model:?} {wardrobe:?} legs"
    );
}

#[test]
fn long_hair_hangs_where_it_did_and_swings_smoothly_as_she_moves() {
    let look = Look::with_choice(
        CreatureModel::Nymph,
        9,
        Some(Variant {
            woman: true,
            ..Default::default()
        }),
    );
    assert!(
        geometry::loose_fall(&look),
        "the nymph wears her hair long and loose"
    );
    // The authored fall retains its exact head placement and crown pivot.
    // Contact may bend the fall around the composed body, but not move its scalp.
    let (world, rig, _, meshes) = spawn_one(&look);
    let (hair, _) = *rig
        .drapes
        .iter()
        .find(|(_, kind)| *kind == Drape::Hair)
        .expect("long hair hangs on its own");
    let at = world.get::<Transform>(hair).unwrap().compute_affine();
    let placed: Vec<Vec3> = points_under(&world, &meshes, hair)
        .into_iter()
        .map(|p| at.transform_point3(p))
        .collect();
    let mut on_head = Parts::default();
    on_head.set_transform(
        Affine3A::from_translation(Vec3::Y * 0.08)
            * Affine3A::from_scale(Vec3::splat(1.1))
            * Affine3A::from_translation(Vec3::Y * -0.08),
    );
    geometry::long_hair_form_for(&mut on_head, &look);
    let (low, high) = on_head.matte.bounds();
    let scale = anatomy::storybook_part(Plan::Biped, J::Head);
    let (low, high) = (low * scale, high * scale);
    let authored: Vec<_> = super::hair_contact::authored_points(rig.hair.as_ref().unwrap())
        .into_iter()
        .map(|p| at.transform_point3(p))
        .collect();
    let bounds = authored.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| {
        (lo.min(*p), hi.max(*p))
    });
    assert!(
        (bounds.0 - low).abs().max_element() < 1e-4 && (bounds.1 - high).abs().max_element() < 1e-4,
        "authored {bounds:?} against {low} {high}"
    );
    // Crown vertices must still occupy those exact rendered positions. Lower
    // contact and edge stretch have their own animated hair_contact regression.
    assert_eq!(placed.len(), authored.len());
    let (attached, root) = super::hair_contact::attachment_witnesses(rig.hair.as_ref().unwrap());
    let crown_floor = at.transform_point3(root).y;
    let mut crown = 0;
    for ((fitted, source), attached) in placed.iter().zip(&authored).zip(attached) {
        // Exact attachment follows authored ownership, not a global height
        // that accidentally includes the jaw/neck fall on another body.
        if attached {
            assert!(
                fitted.distance(*source) < 1e-5,
                "contact moved an attachment"
            );
        }
        // Independent spatial witness: the real crown cannot be made mobile
        // merely by changing the ownership mask.
        if source.y >= crown_floor {
            assert!(attached, "crown lost attachment ownership");
            assert!(fitted.distance(*source) < 1e-5, "contact moved the crown");
            crown += 1;
        }
    }
    assert!(crown > 20);
    // Walking, then turning a right angle: it trails and lifts with each
    // step, more through the turn, and never jumps between frames.
    let path = |f: usize| {
        if f < 70 {
            Vec3::Z * f as f32 * 0.03
        } else {
            Vec3::Z * 70.0 * 0.03 + Vec3::X * (f - 70) as f32 * 0.03
        }
    };
    let (mut app, root) = animate_look(look, 1, path);
    let hair = app
        .world()
        .get::<Rig>(root)
        .unwrap()
        .drapes
        .iter()
        .find(|(_, kind)| *kind == Drape::Hair)
        .unwrap()
        .0;
    let mut previous = Quat::IDENTITY;
    let (mut walking, mut turning, mut jump) = (0.0f32, 0.0f32, 0.0f32);
    for frame in 1..140 {
        app.world_mut()
            .get_mut::<Transform>(root)
            .unwrap()
            .translation = path(frame);
        app.update();
        let rotation = app.world().get::<Transform>(hair).unwrap().rotation;
        if frame > 1 {
            jump = jump.max(rotation.angle_between(previous));
        }
        previous = rotation;
        let swing = rotation.angle_between(Quat::IDENTITY);
        if (40..70).contains(&frame) {
            walking = walking.max(swing);
        } else if (70..100).contains(&frame) {
            turning = turning.max(swing);
        }
    }
    assert!(walking > 0.06, "her hair trails as she walks: {walking}");
    assert!(
        turning > walking,
        "and swings out through a turn: {turning} {walking}"
    );
    assert!(jump < 0.03, "and never jumps between frames: {jump}");
    // Cloth on the same walk and turn eases into its swing just as smoothly.
    let (mut app, root) = animate_along(CreatureModel::Human, 1, path);
    let drapes = app.world().get::<Rig>(root).unwrap().drapes.clone();
    let mut previous: Vec<Quat> = vec![Quat::IDENTITY; drapes.len()];
    for frame in 1..140 {
        app.world_mut()
            .get_mut::<Transform>(root)
            .unwrap()
            .translation = path(frame);
        app.update();
        for (k, &(e, kind)) in drapes.iter().enumerate() {
            let rotation = app.world().get::<Transform>(e).unwrap().rotation;
            if frame > 1 {
                let jump = rotation.angle_between(previous[k]);
                assert!(jump < 0.04, "{kind:?} jumps {jump} at frame {frame}");
            }
            previous[k] = rotation;
        }
    }
}

fn assert_joint_pigment(look: &Look, joint: J, color: Vec3) {
    let linear = Color::srgb(color.x, color.y, color.z).to_linear();
    let expected = Vec3::new(linear.red, linear.green, linear.blue).normalize();
    let (world, rig, _, meshes) = spawn_one(look);
    let mut stack = vec![rig.joint(joint).unwrap()];
    let mut colors = Vec::new();
    while let Some(entity) = stack.pop() {
        if let Some(mesh) = world.get::<Mesh3d>(entity).and_then(|m| meshes.get(&m.0)) {
            if let Some(bevy::render::mesh::VertexAttributeValues::Float32x4(values)) =
                mesh.attribute(Mesh::ATTRIBUTE_COLOR)
            {
                colors.extend(values.iter().map(|c| Vec3::new(c[0], c[1], c[2])));
            }
        }
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter());
        }
    }
    assert!(colors.len() > 100, "{joint:?}: no meaningful mesh witness");
    let matching = colors
        .iter()
        .filter(|c| c.normalize().distance(expected) < 0.0001)
        .count();
    assert!(
        matching > 100,
        "{joint:?}: expected pigment {color:?}, found {matching} vertices"
    );
}

#[test]
fn transferred_bare_surfaces_use_the_recipient_skin_without_recoloring_cloth() {
    let mut donor = Look::with_choice(
        CreatureModel::Nymph,
        42,
        Some(Variant {
            woman: true,
            hair: 4,
            ..default()
        }),
    );
    donor.skin = Vec3::new(0.1, 0.65, 0.2);
    let mut recipient = Look::with_choice(
        CreatureModel::Human,
        91,
        Some(Variant {
            woman: true,
            hair: 4,
            ..default()
        }),
    );
    recipient.skin = Vec3::new(0.72, 0.35, 0.23);
    recipient.outfit = Some(outfit::Outfit::native(&donor));
    for joint in [J::HandL, J::LegL, J::FootL] {
        assert_joint_pigment(&recipient, joint, recipient.skin);
    }
    // The same numeric donor pigment can be authored cloth. It must stay fixed.
    recipient.outfit.as_mut().unwrap().legs = outfit::Legs::Trousers(donor.skin);
    assert_joint_pigment(&recipient, J::LegL, donor.skin);
    let mut equipment = Equipment::default();
    equipment.select("boots").unwrap();
    let restored = recipient.equipped(equipment).equipped(Equipment::default());
    assert_joint_pigment(&restored, J::FootL, restored.skin);
}

#[test]
fn copied_native_boots_keep_wearer_skin_and_fixed_leather_distinct() {
    let assert_foot = |look: &Look, expected: Vec3| {
        let mut reference = look.clone();
        let mut recipe = outfit::Composition::resolve(look).outfit;
        let outfit::Feet::Boots(ref mut boots) = recipe.feet else {
            panic!("boot witness")
        };
        boots.color = outfit::Pigment::Fixed(expected);
        reference.outfit = Some(recipe);
        // The fixed-color reference uses the same actual geometry, not a guessed
        // vertex quota; rounded footwear shades many vertices independently.
        reference.equipment = Equipment::default();
        let actual = joint_surface_samples(look, J::FootL);
        let expected = joint_surface_samples(&reference, J::FootL);
        assert!(actual.len() > 100 && expected.len() > 100);
        let actual = surface_signature(actual);
        assert_eq!(actual, surface_signature(expected));
        let outfit::Feet::Boots(ref mut boots) = reference.outfit.as_mut().unwrap().feet else {
            panic!("boot negative control")
        };
        boots.color = outfit::Pigment::Fixed(Vec3::new(0.02, 0.03, 0.94));
        assert_ne!(
            actual,
            surface_signature(joint_surface_samples(&reference, J::FootL)),
            "the foot witness must detect an unrelated fixed color"
        );
    };
    for (model, shade) in [
        (CreatureModel::Zombie, 0.8),
        (CreatureModel::Guardian, 0.85),
        (CreatureModel::Yeti, 1.0),
    ] {
        let mut donor = Look::of(model, 42);
        donor.skin = Vec3::new(0.1, 0.65, 0.2);
        let mut wearer = Look::of(CreatureModel::Human, 91);
        wearer.skin = Vec3::new(0.72, 0.35, 0.23);
        wearer.outfit = Some(outfit::Outfit::native(&donor));
        assert_foot(&wearer, wearer.skin * shade);
        let mut gear = Equipment::default();
        gear.select("boots").unwrap();
        let covered = wearer.clone().equipped(gear);
        assert_eq!(
            outfit::Composition::resolve(&covered)
                .outfit
                .boot(covered.skin),
            LEATHER * 0.75
        );
        let restored = covered.equipped(Equipment::default());
        assert_foot(&restored, restored.skin * shade);
        let outfit::Feet::Boots(ref mut boots) = wearer.outfit.as_mut().unwrap().feet else {
            panic!("native boot recipe")
        };
        boots.color = outfit::Pigment::Fixed(donor.skin * shade);
        assert_foot(&wearer, donor.skin * shade);
    }
}

#[test]
fn custom_belt_hardware_follows_its_band_frame_across_recipes() {
    let pigment = Vec3::new(0.11, 0.91, 0.67);
    let linear = Color::srgb(pigment.x, pigment.y, pigment.z).to_linear();
    let tag = Vec3::new(linear.red, linear.green, linear.blue).normalize();
    let count = |look: &Look, joint| {
        joint_surface_samples(look, joint)
            .iter()
            .filter(|(_, c)| c.normalize().distance(tag) < 0.0001)
            .count()
    };
    for model in [CreatureModel::Human, CreatureModel::Orc] {
        let look = Look::with_choice(
            model,
            42,
            Some(Variant {
                woman: true,
                build: 2,
                ..default()
            }),
        );
        for fitted in [false, true] {
            for belt in [false, true] {
                let mut dressed = look.clone().dressed(Wardrobe::Commoner);
                let mut recipe = if fitted {
                    outfit::Outfit::native(&Look::with_choice(CreatureModel::Mage, 42, None))
                } else {
                    outfit::Outfit::worn(&dressed)
                };
                assert_eq!(recipe.robe.is_some(), fitted);
                recipe.belt = belt;
                recipe.details.hip = Some(outfit::HipDecoration::BucklePouch {
                    metal: pigment,
                    pouch: LEATHER,
                });
                dressed.outfit = Some(recipe);
                let (owner, other) = if fitted {
                    (J::Hips, J::Torso)
                } else {
                    (J::Torso, J::Hips)
                };
                assert!(
                    count(&dressed, owner) > 24,
                    "custom hardware survives on belt owner"
                );
                assert_eq!(
                    count(&dressed, other),
                    0,
                    "hardware must not attach to another moving joint"
                );
            }
        }
    }
}

#[test]
fn custom_belt_hardware_replaces_the_default_buckle() {
    let look = Look::with_choice(CreatureModel::Leprechaun, 42, None);
    let color = Color::srgb(0.74, 0.5, 0.26).to_linear();
    let default_buckle = Vec3::new(color.red, color.green, color.blue).normalize();
    let count = |look: &Look| {
        let points = joint_surface_samples(look, J::Torso);
        assert!(points.len() > 100);
        points
            .iter()
            .filter(|(_, c)| c.normalize().distance(default_buckle) < 0.0001)
            .count()
    };
    let mut plain = look.clone();
    let mut recipe = outfit::Outfit::native(&look);
    recipe.details.hip = None;
    plain.outfit = Some(recipe);
    assert!(
        count(&plain) > 12,
        "positive control must contain ordinary belt hardware"
    );
    assert_eq!(
        count(&look),
        0,
        "custom buckle must replace rather than overlap default hardware"
    );
}

#[test]
fn closed_troll_boots_have_no_detached_claws_below_the_sole() {
    let mut boots = Equipment::default();
    boots.select("boots").unwrap();
    let look = Look::of(CreatureModel::Troll, 42).equipped(boots);
    let points = joint_surface_samples(&look, J::FootL);
    assert!(points.len() > 100);
    let lowest = points
        .iter()
        .map(|(p, _)| p.y)
        .fold(f32::INFINITY, f32::min);
    assert!(
        lowest > -0.12,
        "closed boot has detached anatomy below its sole: {lowest}"
    );
}

#[test]
fn toe_claws_follow_resolved_foot_coverage_and_restore_on_removal() {
    let linear = Color::srgb(BONE.x, BONE.y, BONE.z).to_linear();
    let bone = Vec3::new(linear.red, linear.green, linear.blue).normalize();
    let samples = |look: &Look| {
        let points = joint_surface_samples(look, J::FootL);
        assert!(points.len() > 100);
        assert!(points.iter().all(|(p, _)| p.is_finite() && p.y > -0.12));
        let claws: Vec<_> = points
            .iter()
            .filter(|(_, c)| c.normalize().distance(bone) < 0.001)
            .map(|(p, _)| *p)
            .collect();
        (surface_signature(points), claws)
    };
    for woman in [false, true] {
        for build in [0, 2] {
            let native = Look::with_choice(
                CreatureModel::Troll,
                42,
                Some(Variant {
                    woman,
                    build,
                    ..default()
                }),
            );
            let (original, claws) = samples(&native);
            assert!(
                claws.len() >= 12,
                "bare Troll must retain visible claw geometry"
            );
            for name in ["boots", "combat-boots"] {
                let mut gear = Equipment::default();
                gear.select(name).unwrap();
                let dressed = native.clone().equipped(gear);
                assert!(
                    samples(&dressed).1.is_empty(),
                    "{name}: closed footwear covers claws"
                );
                assert_eq!(samples(&dressed.equipped(Equipment::default())).0, original);
            }
            assert!(samples(&native.clone().dressed(Wardrobe::Ranger))
                .1
                .is_empty());
            for sandal in [false, true] {
                let mut look = native.clone();
                let mut recipe = outfit::Outfit::native(&look);
                recipe.feet = outfit::Feet::Shoes(outfit::Shoes {
                    color: Vec3::new(0.03, 0.08, 0.7),
                    heel: woman,
                    sandal,
                    straps: 0.0,
                });
                look.outfit = Some(recipe);
                let (_, claws) = samples(&look);
                if sandal {
                    assert!(claws.len() >= 12, "sandals retain exposed toe anatomy");
                    // All three claw lanes must survive the surface query, not
                    // merely a left/right pair after a missed centre ray.
                    let lowest = claws.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
                    let lanes: std::collections::BTreeSet<_> = claws
                        .iter()
                        .filter(|p| (p.y - lowest).abs() < 0.00001)
                        .map(|p| (p.x * 10000.0).round() as i32)
                        .collect();
                    assert_eq!(lanes.len(), 3, "each exposed toe needs its fitted claw");
                    assert!(lanes.contains(&0));
                    assert!(
                        claws.iter().all(|p| p.z > 0.10),
                        "claws belong at the emitted toe front"
                    );
                } else {
                    assert!(claws.is_empty(), "closed shoes cover claws");
                }
            }
        }
    }
}

#[test]
fn copied_native_neck_cut_preserves_torso_and_waist_consumers() {
    let sample = |look: &Look| {
        let (world, rig, _, meshes) = spawn_one(look);
        [J::Hips, J::Torso].map(|joint| {
            let points = joint_surface_samples_in(&world, &rig, &meshes, joint, true);
            assert!(points.len() > 100);
            surface_signature(points)
        })
    };
    for model in [
        CreatureModel::Orc,
        CreatureModel::Zombie,
        CreatureModel::Hobgoblin,
        CreatureModel::Sentinel,
        CreatureModel::Troll,
    ] {
        for woman in [false, true] {
            for build in [0, 2] {
                for names in [
                    vec![],
                    vec!["breastplate"],
                    vec!["hawaiian-shirt", "chain-mail"],
                ] {
                    let mut native = Look::with_choice(
                        model,
                        42,
                        Some(Variant {
                            woman,
                            build,
                            hair: 0,
                            ..default()
                        }),
                    );
                    let mut gear = Equipment::default();
                    for name in &names {
                        gear.select(name).unwrap();
                    }
                    native = native.equipped(gear);
                    let mut copied = native.clone();
                    copied.outfit = Some(outfit::Outfit::native(&native));
                    assert_eq!(sample(&copied), sample(&native),
                        "{model:?}, woman={woman}, build={build}, gear={names:?}: native and explicit cut consumers");
                }
            }
        }
    }
}

#[test]
fn zombie_hip_rags_survive_explicit_outfit_copy() {
    for woman in [false, true] {
        let native = Look::with_choice(
            CreatureModel::Zombie,
            42,
            Some(Variant { woman, ..default() }),
        );
        let mut copied = native.clone();
        copied.outfit = Some(outfit::Outfit::native(&native));
        let original = joint_surface_samples(&native, J::Hips);
        let actual = joint_surface_samples(&copied, J::Hips);
        assert!(original.len() > 100 && actual.len() > 100);
        assert_eq!(
            surface_signature(actual),
            surface_signature(original),
            "explicit outfit copy must retain the hip rags"
        );
    }
}

#[test]
fn tunic_tatters_transfer_remove_and_follow_resolved_shirt_without_extra_attachments() {
    let donor = Look::of(CreatureModel::Zombie, 42);
    let detail = outfit::Outfit::native(&donor).details.hip;
    assert!(matches!(
        detail,
        Some(outfit::HipDecoration::TunicTatters { shade: 0.7 })
    ));
    let sample = |look: &Look| {
        let (world, rig, _, meshes) = spawn_one(look);
        (
            surface_signature(joint_surface_samples_in(
                &world,
                &rig,
                &meshes,
                J::Hips,
                true,
            )),
            world.entities().len(),
            rig.drapes.len(),
        )
    };
    for (model, woman, build) in [
        (CreatureModel::Human, true, 0),
        (CreatureModel::Orc, false, 2),
    ] {
        let mut recipient = Look::with_choice(
            model,
            91,
            Some(Variant {
                woman,
                build,
                ..default()
            }),
        );
        let mut recipe = outfit::Outfit::from_wardrobe(Wardrobe::Commoner, &recipient);
        recipe.details.hip = detail;
        recipient.outfit = Some(recipe);
        for shirt in [None, Some("t-shirt"), Some("hawaiian-shirt")] {
            let mut gear = Equipment::default();
            if let Some(shirt) = shirt {
                gear.select(shirt).unwrap();
            }
            let dressed = recipient.clone().equipped(gear);
            let (with, entities, drapes) = sample(&dressed);
            let mut removed = dressed.clone();
            removed.outfit.as_mut().unwrap().details.hip = None;
            let (without, base_entities, base_drapes) = sample(&removed);
            assert!(
                without.is_subset(&with),
                "static detail must not replace unrelated hip geometry"
            );
            let scraps: BTreeSet<_> = with.difference(&without).copied().collect();
            assert!(
                scraps.len() >= 15,
                "five visible geometric scraps, not an empty recipe"
            );
            assert_eq!((entities, drapes), (base_entities, base_drapes));
            let mut resolved = dressed.clone();
            resolved.outfit = Some(outfit::Composition::resolve(&dressed).outfit);
            resolved.equipment = Equipment::default();
            assert_eq!(
                with,
                sample(&resolved).0,
                "scraps use the resolved tunic, not species pigment"
            );
            let restored = dressed.equipped(Equipment::default());
            assert_eq!(sample(&restored).0, sample(&recipient).0);
            assert_eq!(
                recipient.outfit.unwrap(),
                recipe,
                "composition must not mutate the base recipe"
            );
        }
    }
}

#[test]
fn native_accessories_survive_explicit_recipe_copy() {
    fn attached_points(look: &Look) -> Vec<Vec<Vec3>> {
        let (world, rig, _, meshes) = spawn_one(look);
        let mut result = Vec::new();
        for joint in [J::Hips, J::ArmL, J::ArmR, J::ForeL, J::ForeR] {
            let parent = rig.joint(joint).unwrap();
            for child in world.get::<Children>(parent).unwrap().iter() {
                // Separately attached garments, not the body mesh or child joints.
                if world.get::<Mesh3d>(child).is_none() && !rig.joints.contains(&Some(child)) {
                    result.push(points_under(&world, &meshes, child));
                }
            }
        }
        result
    }
    for model in [
        CreatureModel::Orc,
        CreatureModel::Troll,
        CreatureModel::Leprechaun,
        CreatureModel::Nymph,
        CreatureModel::Medusa,
        CreatureModel::Phantom,
        CreatureModel::Wraith,
    ] {
        let native = Look::with_choice(
            model,
            42,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        let original = attached_points(&native);
        assert!(original.iter().any(|p| !p.is_empty()));
        let mut copied = native.clone();
        copied.outfit = Some(outfit::Outfit::native(&native));
        let actual = attached_points(&copied);
        assert_eq!(
            actual.len(),
            original.len(),
            "{model:?}: copied recipe lost attached garments"
        );
        let mut recipient = Look::with_choice(
            CreatureModel::Human,
            91,
            Some(Variant {
                woman: true,
                build: 2,
                shade: 3,
                ..default()
            }),
        );
        recipient.outfit = copied.outfit;
        let transferred = attached_points(&recipient);
        assert_eq!(
            transferred.len(),
            original.len(),
            "{model:?}: transfer lost attachments"
        );
        assert!(transferred.iter().flatten().all(|p| p.is_finite()));
        for (a, b) in actual.iter().zip(&original) {
            assert_eq!(
                a.len(),
                b.len(),
                "{model:?}: copied garment topology changed"
            );
            assert!(
                a.iter().zip(b).all(|(a, b)| a.distance(*b) < 0.00001),
                "{model:?}: copied garment placement changed"
            );
        }
    }
    // Details from three recipes coexist on one wearer and can be removed
    // independently without losing the other attachments.
    let mut mixed = Look::with_choice(
        CreatureModel::Human,
        71,
        Some(Variant {
            woman: true,
            hair: 4,
            ..default()
        }),
    );
    let mut recipe = outfit::Outfit::from_wardrobe(Wardrobe::Commoner, &mixed);
    mixed.outfit = Some(recipe);
    let base_count = attached_points(&mixed).len();
    let details = |model| outfit::Outfit::native(&Look::of(model, 42)).details;
    recipe.details.hip = details(CreatureModel::Nymph).hip;
    recipe.details.shoulder_discs = details(CreatureModel::Medusa).shoulder_discs;
    recipe.details.sleeve_tatters = details(CreatureModel::Phantom).sleeve_tatters;
    for (hip, shoulders, sleeves, added) in [
        (true, true, true, 5),
        (false, true, true, 4),
        (true, false, true, 3),
        (true, true, false, 3),
    ] {
        let mut selected = recipe;
        if !hip {
            selected.details.hip = None;
        }
        if !shoulders {
            selected.details.shoulder_discs = None;
        }
        if !sleeves {
            selected.details.sleeve_tatters = None;
        }
        mixed.outfit = Some(selected);
        assert_eq!(attached_points(&mixed).len(), base_count + added);
    }
}

#[test]
fn explicit_recipe_changes_do_not_alias_in_a_shared_model_cache() {
    let mut materials = Assets::<StandardMaterial>::default();
    let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
    let mut meshes = Assets::<Mesh>::default();
    let mut world = World::new();
    let base = Look::with_choice(
        CreatureModel::Nymph,
        2,
        Some(Variant {
            woman: true,
            ..default()
        }),
    );
    let mut first = base.clone();
    first.outfit = Some(outfit::Outfit::from_wardrobe(Wardrobe::Commoner, &base));
    let mut second = first.clone();
    second.outfit.as_mut().unwrap().tunic = Vec3::new(0.8, 0.05, 0.6);
    assert_eq!(
        first.key, second.key,
        "exercise raw recipe overrides, not catalog IDs"
    );
    let mut draw = |look: &Look| {
        let parent = world.spawn(Transform::default()).id();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let rig = spawn(
            &mut Commands::new(&mut queue, &world),
            &mut library,
            &mut meshes,
            parent,
            look,
        );
        queue.apply(&mut world);
        let handles = |joint| -> Vec<AssetId<Mesh>> {
            world
                .get::<Children>(rig.joint(joint).unwrap())
                .into_iter()
                .flat_map(|children| children.iter())
                .filter_map(|child| world.get::<Mesh3d>(child).map(|m| m.0.id()))
                .collect()
        };
        let torso = handles(J::Torso);
        let colors: Vec<[f32; 4]> = torso
            .iter()
            .flat_map(|id| {
                match meshes
                    .get(*id)
                    .unwrap()
                    .attribute(Mesh::ATTRIBUTE_COLOR)
                    .unwrap()
                {
                    bevy::render::mesh::VertexAttributeValues::Float32x4(values) => values.clone(),
                    _ => panic!("garment colors"),
                }
            })
            .collect();
        (
            torso,
            handles(J::Head),
            rig.hair.as_ref().map(super::hair_contact::source_meshes),
            colors,
        )
    };
    let original = draw(&base);
    let a = draw(&first);
    let b = draw(&second);
    assert!(!a.0.is_empty() && !a.3.is_empty());
    assert_ne!(
        a.0, b.0,
        "different authored recipes reused the same torso mesh"
    );
    assert_ne!(
        a.3, b.3,
        "cached pixels would retain the previous garment pigment"
    );
    assert_eq!(a.1, b.1, "unchanged heads should still share geometry");
    assert!(a.2.is_some(), "fixture must exercise reusable hair sources");
    assert_eq!(a.2, b.2, "unchanged hair sources should still be shared");
    assert_eq!(
        draw(&first),
        a,
        "repeat selection must reuse its own composition"
    );
    assert_eq!(
        draw(&base),
        original,
        "native restoration must reuse its original geometry"
    );
}

#[test]
fn warm_spawn_does_not_publish_modifications_to_existing_meshes() {
    use bevy::asset::{AssetEvent, AssetPlugin};
    let look = Look::with_choice(
        CreatureModel::Nymph,
        2,
        Some(Variant {
            woman: true,
            ..default()
        }),
    )
    .dressed(Wardrobe::RoyalPrincess);
    let (_, cold_rig, mut library, meshes) = spawn_one(&look);
    assert!(cold_rig.hair.is_some(), "exercise fitting of hanging hair");
    let existing: std::collections::HashSet<_> = meshes.iter().map(|(id, _)| id).collect();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>();
    app.insert_resource(meshes);
    app.update();
    app.world_mut()
        .resource_mut::<Events<AssetEvent<Mesh>>>()
        .clear();
    // Positive witness: the observer must detect a real mesh mutation.
    let handle = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(Mesh::from(Cuboid::default()));
    app.update();
    app.world_mut()
        .resource_mut::<Events<AssetEvent<Mesh>>>()
        .clear();
    app.world_mut()
        .resource_mut::<Assets<Mesh>>()
        .get_mut(&handle)
        .unwrap()
        .insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32, 0.0, 0.0]]);
    app.update();
    assert!(app
        .world_mut()
        .resource_mut::<Events<AssetEvent<Mesh>>>()
        .drain()
        .any(|e| matches!(e, AssetEvent::Modified { id } if id == handle.id())));
    let parent = app.world_mut().spawn(Transform::default()).id();
    let mut queue = bevy::ecs::world::CommandQueue::default();
    app.world_mut()
        .resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            let rig = spawn(
                &mut Commands::new(&mut queue, world),
                &mut library,
                &mut meshes,
                parent,
                &look,
            );
            assert!(rig.hair.is_some());
        });
    queue.apply(app.world_mut());
    app.update();
    let events: Vec<_> = app
        .world_mut()
        .resource_mut::<Events<AssetEvent<Mesh>>>()
        .drain()
        .collect();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AssetEvent::Added { id } if !existing.contains(id))),
        "deforming instances still allocate their private meshes"
    );
    let modified: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, AssetEvent::Modified { id } if existing.contains(id)))
        .collect();
    assert!(
        modified.is_empty(),
        "warm composition must not trigger render re-extraction of existing meshes: {modified:?}"
    );
}

#[test]
fn redressing_reuses_the_head_and_hair_the_outfit_leaves_alone() {
    // Changing clothes rebuilds only what they shape: outfits that leave
    // the head bare share its face and hair, and a helm gets its own head.
    let mut materials = Assets::<StandardMaterial>::default();
    let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
    let mut meshes = Assets::<Mesh>::default();
    let mut world = World::new();
    let base = Look::with_choice(
        CreatureModel::Nymph,
        2,
        Some(Variant {
            woman: true,
            ..Default::default()
        }),
    );
    let mut current = base.clone();
    let mut dress = |wardrobe: Wardrobe| {
        current = current.clone().dressed(wardrobe);
        let parent = world.spawn(Transform::default()).id();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let rig = spawn(
            &mut Commands::new(&mut queue, &world),
            &mut library,
            &mut meshes,
            parent,
            &current,
        );
        queue.apply(&mut world);
        let meshes_of = |e: Entity| -> Vec<AssetId<Mesh>> {
            world
                .get::<Children>(e)
                .into_iter()
                .flat_map(|children| children.iter())
                .filter_map(|child| world.get::<Mesh3d>(child).map(|m| m.0.id()))
                .collect()
        };
        // Mutable contact meshes belong to each wearer; redressing reuses
        // their immutable source geometry. Instance isolation is covered by
        // wearers_keep_independent_contact_meshes_and_cached_sources.
        let hair = rig.hair.as_ref().map(super::hair_contact::source_meshes);
        (
            meshes_of(rig.joint(J::Head).unwrap()),
            hair,
            meshes_of(rig.joint(J::Torso).unwrap()),
        )
    };
    let native = dress(Wardrobe::Native);
    let commoner = dress(Wardrobe::Commoner);
    let courtier = dress(Wardrobe::Courtier);
    let knight = dress(Wardrobe::Knight);
    assert_eq!(
        commoner.0, courtier.0,
        "a bare head is shared between outfits"
    );
    assert_eq!(commoner.1, courtier.1, "and so is her hanging hair");
    assert!(commoner.1.is_some());
    assert_ne!(commoner.2, courtier.2, "while the clothes are their own");
    assert_ne!(commoner.0, knight.0, "a helm makes its own head");
    assert_eq!(
        commoner,
        dress(Wardrobe::Commoner),
        "returning to an outfit reuses its complete model, head and hair"
    );
    assert_eq!(
        native,
        dress(Wardrobe::Native),
        "native clothing is restored"
    );
    assert_eq!(
        native,
        dress(Wardrobe::Native),
        "native selection is idempotent"
    );
}

/// Sample chest-surface vertices with a particular authored pigment, in the
/// wearer's unscaled torso space rather than at a camera-dependent pixel.
fn chest_pigment_points(
    look: &Look,
    color: Vec3,
    region: impl Fn(Vec3) -> bool,
) -> (usize, Vec<Vec3>) {
    use bevy::render::mesh::VertexAttributeValues;
    let (world, rig, _, meshes) = spawn_one(look);
    let torso = rig.joint(J::Torso).unwrap();
    let scale = anatomy::storybook_part(Plan::Biped, J::Torso);
    let linear = Color::srgb(color.x, color.y, color.z).to_linear();
    let expected = Vec3::new(linear.red, linear.green, linear.blue).normalize_or_zero();
    let (mut sampled, mut matched) = (0, Vec::new());
    for child in world.get::<Children>(torso).unwrap().iter() {
        let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0)) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            continue;
        };
        let transform = world.get::<Transform>(child).unwrap().compute_affine();
        for (position, pigment) in positions.iter().zip(colors) {
            let at = transform.transform_point3(Vec3::from(*position)) / scale;
            if region(at) {
                sampled += 1;
                if Vec3::new(pigment[0], pigment[1], pigment[2])
                    .normalize_or_zero()
                    .distance(expected)
                    < 0.001
                {
                    matched.push(at);
                }
            }
        }
    }
    (sampled, matched)
}

fn chest_color_count(look: &Look, color: Vec3, region: impl Fn(Vec3) -> bool) -> (usize, usize) {
    let (sampled, points) = chest_pigment_points(look, color, region);
    (sampled, points.len())
}

#[test]
fn native_torso_ornaments_remain_with_the_native_costume() {
    for model in TORSO_COSTUME_MODELS {
        for woman in [false, true] {
            let (look, pigment, low, high) = torso_costume_witness(model, woman);
            let (sampled, ornament) = chest_color_count(&look, pigment, |at| {
                at.x.abs() < 0.15 && at.z > 0.0 && (low..high).contains(&at.y)
            });
            assert!(
                sampled > 0 && ornament > 0,
                "{model:?} woman {woman}: preserve the native torso ornament"
            );
        }
    }
}

#[test]
fn replacement_tunics_do_not_inherit_native_torso_ornaments() {
    let mut inherited = Vec::new();
    for model in TORSO_COSTUME_MODELS {
        for woman in [false, true] {
            let (look, pigment, low, high) = torso_costume_witness(model, woman);
            let look = look.dressed(Wardrobe::Commoner);
            let (sampled, ornament) = chest_color_count(&look, pigment, |at| {
                at.x.abs() < 0.15 && at.z > 0.0 && (low..high).contains(&at.y)
            });
            assert!(sampled > 0, "{model:?}: sample the replacement torso");
            if ornament > 0 {
                inherited.push((model, woman, ornament));
            }
        }
    }
    assert!(
        inherited.is_empty(),
        "replacement tunics inherit native harness, stock, bodice jewelry or sash vertices: {inherited:?}"
    );
}

const TORSO_COSTUME_MODELS: [CreatureModel; 7] = [
    CreatureModel::Orc,
    CreatureModel::Troll,
    CreatureModel::Leprechaun,
    CreatureModel::Nymph,
    CreatureModel::Medusa,
    CreatureModel::Phantom,
    CreatureModel::Wraith,
];

#[test]
fn torso_ornament_recipes_transfer_to_an_unrelated_wearer_and_can_be_removed() {
    use outfit::TorsoOrnament;
    let base = Look::with_choice(
        CreatureModel::Human,
        2,
        Some(Variant {
            woman: true,
            ..default()
        }),
    )
    .dressed(Wardrobe::Commoner);
    assert_eq!(base.style, Style::Hero);
    let bare = joint_surface_samples(&base, J::Torso);
    let bare_signature = surface_signature(bare.clone());
    let count = |samples: &[(Vec3, Vec3)], pigment: Vec3, low: f32, high: f32| {
        let linear = Color::srgb(pigment.x, pigment.y, pigment.z).to_linear();
        let expected = Vec3::new(linear.red, linear.green, linear.blue).normalize_or_zero();
        samples
            .iter()
            .filter(|(at, color)| {
                at.x.abs() < 0.2
                    && at.z > 0.0
                    && (low..high).contains(&at.y)
                    && color.normalize_or_zero().distance(expected) < 0.001
            })
            .count()
    };
    for source in TORSO_COSTUME_MODELS {
        let (native, pigment, low, high) = torso_costume_witness(source, true);
        let recipe = outfit::Outfit::worn(&native)
            .torso_ornament
            .expect("the native costume owns its torso ornament");
        let mut transferred = base.clone();
        transferred.outfit.as_mut().unwrap().torso_ornament = Some(recipe);
        let worn = joint_surface_samples(&transferred, J::Torso);
        assert_eq!(count(&bare, pigment, low, high), 0);
        assert!(
            count(&worn, pigment, low, high) > 0,
            "{source:?}: the copied ornament is emitted on an ordinary Human body"
        );
        // The two optional assemblies must also travel with their recipes,
        // rather than testing the new wearer's native species.
        let optional = match recipe {
            TorsoOrnament::Harness {
                strap,
                ring,
                shoulder_straps: Some(color),
            } => Some((
                TorsoOrnament::Harness {
                    strap,
                    ring,
                    shoulder_straps: None,
                },
                color,
                0.51,
                0.57,
            )),
            TorsoOrnament::KnottedSash {
                sash,
                scarf: Some(color),
            } => Some((
                TorsoOrnament::KnottedSash { sash, scarf: None },
                color,
                0.2,
                0.4,
            )),
            _ => None,
        };
        if let Some((without, color, low, high)) = optional {
            assert!(
                count(&worn, color, low, high) > 0,
                "{source:?} optional trim"
            );
            transferred.outfit.as_mut().unwrap().torso_ornament = Some(without);
            let omitted = joint_surface_samples(&transferred, J::Torso);
            assert_eq!(
                count(&omitted, color, low, high),
                0,
                "{source:?}: clearing the optional assembly removes its mesh"
            );
        }
        transferred.outfit.as_mut().unwrap().torso_ornament = None;
        assert!(
            surface_signature(joint_surface_samples(&transferred, J::Torso)) == bare_signature,
            "{source:?}: removing the ornament restores the unadorned torso mesh"
        );
    }
}

fn torso_costume_witness(model: CreatureModel, woman: bool) -> (Look, Vec3, f32, f32) {
    let mut look = Look::with_choice(model, 2, Some(Variant { woman, ..default() }));
    // Distinct pigments isolate the ornament from skin, its native tunic,
    // and the replacement Commoner tunic, including translucent phantoms.
    look.skin = Vec3::new(0.0, 0.0, 0.7);
    look.cloth = Vec3::new(0.7, 0.0, 0.0);
    look.trim = Vec3::new(0.0, 0.7, 0.0);
    let (pigment, low, high) = match model {
        CreatureModel::Orc | CreatureModel::Troll => {
            (Vec3::new(0.82, 0.58, 0.26), 0.27, 0.35) // Harness ring.
        }
        CreatureModel::Leprechaun => (GILT, 0.1, 0.34), // Waistcoat buttons.
        CreatureModel::Nymph => (look.trim, 0.07, 0.13), // Leaf-bodice pin.
        CreatureModel::Medusa => (look.trim, 0.035, 0.11), // Gilt waist disc.
        CreatureModel::Phantom => (LEATHER * 0.7, 0.025, 0.065), // Knotted sash.
        CreatureModel::Wraith => (look.trim, 0.025, 0.065), // Knotted sash.
        _ => panic!("a native torso costume witness is required"),
    };
    (look, pigment, low, high)
}

/// Direct mesh vertices on one joint, excluding separate drapes, limbs and
/// body features. Positions include the mesh's actual Storybook transform.
fn joint_surface_samples(look: &Look, joint: J) -> Vec<(Vec3, Vec3)> {
    joint_surface_samples_with_drapes(look, joint, true)
}

fn joint_surface_samples_with_drapes(
    look: &Look,
    joint: J,
    include_drapes: bool,
) -> Vec<(Vec3, Vec3)> {
    let (world, rig, _, meshes) = spawn_one(look);
    joint_surface_samples_in(&world, &rig, &meshes, joint, include_drapes)
}

fn joint_surface_samples_in(
    world: &World,
    rig: &Rig,
    meshes: &Assets<Mesh>,
    joint: J,
    include_drapes: bool,
) -> Vec<(Vec3, Vec3)> {
    use bevy::render::mesh::VertexAttributeValues;
    let mut samples = Vec::new();
    for child in world
        .get::<Children>(rig.joint(joint).unwrap())
        .unwrap()
        .iter()
    {
        if !include_drapes && rig.drapes.iter().any(|(entity, _)| *entity == child) {
            continue;
        }
        let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0)) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("joint positions")
        };
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("joint pigments")
        };
        let transform = world.get::<Transform>(child).unwrap().compute_affine();
        samples.extend(positions.iter().zip(colors).map(|(at, color)| {
            (
                transform.transform_point3(Vec3::from(*at)),
                Vec3::new(color[0], color[1], color[2]),
            )
        }));
    }
    samples
}

fn surface_signature(samples: impl IntoIterator<Item = (Vec3, Vec3)>) -> BTreeSet<[i32; 6]> {
    samples
        .into_iter()
        .map(|(at, pigment)| {
            [at.x, at.y, at.z, pigment.x, pigment.y, pigment.z]
                .map(|v| (v * 1_000_000.0).round() as i32)
        })
        .collect()
}

#[test]
fn hip_pouches_belong_to_the_selected_outfit() {
    for (model, equivalent_style) in [
        (CreatureModel::Human, Style::Common),
        (CreatureModel::Scout, Style::Zombie),
    ] {
        for woman in [false, true] {
            let base = Look::with_choice(model, 2, Some(Variant { woman, ..default() }));
            let native = joint_surface_samples(&base, J::Hips);
            let leather = Color::srgb(LEATHER.x, LEATHER.y, LEATHER.z).to_linear();
            let leather = Vec3::new(leather.red, leather.green, leather.blue);
            assert!(
                native.iter().any(|(at, pigment)| {
                    at.x > 0.08 && at.y < 0.035 && at.z > 0.02 && pigment.distance(leather) < 1e-6
                }),
                "{model:?}: preserve the native belt pouch"
            );
            let mut equivalent = base.clone();
            equivalent.style = equivalent_style;
            let left_body = anatomy::HumanoidBody::resolve(&base);
            let right_body = anatomy::HumanoidBody::resolve(&equivalent);
            assert_eq!(left_body.pelvis_radii(), right_body.pelvis_radii());
            for wardrobe in [
                Wardrobe::Commoner,
                Wardrobe::Ranger,
                Wardrobe::Knight,
                Wardrobe::Courtier,
            ] {
                let left = surface_signature(joint_surface_samples(
                    &base.clone().dressed(wardrobe),
                    J::Hips,
                ));
                let right = surface_signature(joint_surface_samples(
                    &equivalent.clone().dressed(wardrobe),
                    J::Hips,
                ));
                assert!(!left.is_empty() && !right.is_empty());
                assert!(
                    left == right,
                    "{model:?} woman {woman} {wardrobe:?}: equivalent hips wearing one outfit must carry the same pouch choices"
                );
            }
        }
    }
}

#[test]
fn ranger_outfits_carry_their_pouch_on_other_body_families() {
    for model in [CreatureModel::Orc, CreatureModel::Nymph] {
        for woman in [false, true] {
            let look = Look::with_choice(model, 2, Some(Variant { woman, ..default() }))
                .dressed(Wardrobe::Ranger);
            let pouch = outfit::Outfit::worn(&look)
                .pouch
                .expect("Ranger includes its belt pouch");
            let color = Color::srgb(pouch.color.x, pouch.color.y, pouch.color.z).to_linear();
            let expected = Vec3::new(color.red, color.green, color.blue);
            assert!(
                joint_surface_samples(&look, J::Hips)
                    .iter()
                    .any(|(at, pigment)| {
                        at.x > 0.08
                            && at.y < 0.035
                            && at.z > 0.02
                            && pigment.distance(expected) < 1e-6
                    }),
                "{model:?} woman {woman}: emit the Ranger pouch on a non-Hero/Scout wearer"
            );
        }
    }
}

#[test]
fn replacement_neckline_edging_does_not_inherit_medusa_gilt() {
    for build in 0..3 {
        let mut medusa = Look::with_choice(
            CreatureModel::Medusa,
            2,
            Some(Variant {
                woman: true,
                build,
                ..default()
            }),
        );
        medusa.trim = Vec3::new(0.0, 0.7, 0.0);
        let (_, native_edge) = chest_color_count(&medusa, medusa.trim, |at| {
            (0.47..0.525).contains(&at.y) && at.x.abs() > 0.08 && at.z > 0.0
        });
        assert!(native_edge > 0, "keep Medusa's native gilt sewn edging");
        // The same Medusa with another trim: a replacement bodice's edging
        // that inherited her gilt would change with it. Isolate the upper
        // sewn edge, above her native torso ornament.
        let mut regilt = medusa.clone();
        regilt.trim = Vec3::new(0.7, 0.0, 0.7);
        for wardrobe in [Wardrobe::Commoner, Wardrobe::Ranger, Wardrobe::Courtier] {
            let neckline = |look: Look| {
                surface_signature(
                    joint_surface_samples(&look.dressed(wardrobe), J::Torso)
                        .into_iter()
                        .filter(|(at, _)| {
                            (0.47..0.525).contains(&at.y) && at.x.abs() > 0.08 && at.z > 0.0
                        }),
                )
            };
            let actual = neckline(medusa.clone());
            let expected = neckline(regilt.clone());
            assert!(!actual.is_empty() && !expected.is_empty());
            assert!(
                actual == expected,
                "{wardrobe:?} build {build}: the same female bodice must carry its own sewn neckline color"
            );
        }
    }
}

#[test]
fn selected_outfits_own_the_collar_and_gorget_pigment() {
    let collar = |look: &Look| {
        let scale = anatomy::storybook_part(Plan::Biped, J::Torso);
        joint_surface_samples(look, J::Torso)
            .into_iter()
            .map(|(at, pigment)| (at / scale, pigment))
            .filter(|(at, _)| (0.533..0.58).contains(&at.y))
            .collect::<Vec<_>>()
    };
    let pigments = [Vec3::new(0.7, 0.0, 0.0), Vec3::new(0.0, 0.7, 0.0)];
    let wearer = |model, trim| {
        let mut look = Look::with_choice(
            model,
            2,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        look.trim = trim;
        look
    };
    // Native cloth collars, native armor gorgets and the Courtier recipe
    // deliberately retain the selected trim. These positive witnesses also
    // establish that the slice includes the actual collar assemblies.
    for (model, wardrobe) in [
        (CreatureModel::Human, Wardrobe::Native),
        (CreatureModel::Sentinel, Wardrobe::Native),
        (CreatureModel::Orc, Wardrobe::Courtier),
        (CreatureModel::Medusa, Wardrobe::Courtier),
    ] {
        for trim in pigments {
            let look = wearer(model, trim).dressed(wardrobe);
            let samples = collar(&look);
            let linear = Color::srgb(trim.x, trim.y, trim.z).to_linear();
            let expected = Vec3::new(linear.red, linear.green, linear.blue).normalize_or_zero();
            assert!(
                samples
                    .iter()
                    .any(|(_, pigment)| { pigment.normalize_or_zero().distance(expected) < 0.001 }),
                "{model:?} {wardrobe:?}: preserve the authored collar/gorget pigment"
            );
        }
    }
    let mut inherited = Vec::new();
    for model in [CreatureModel::Orc, CreatureModel::Medusa] {
        for wardrobe in [Wardrobe::Commoner, Wardrobe::Ranger, Wardrobe::Knight] {
            let first = collar(&wearer(model, pigments[0]).dressed(wardrobe));
            let second = collar(&wearer(model, pigments[1]).dressed(wardrobe));
            assert!(
                first.len() > 20 && second.len() > 20,
                "{model:?} {wardrobe:?}: sample the emitted collar/gorget"
            );
            if surface_signature(first) != surface_signature(second) {
                inherited.push((model, wardrobe));
            }
        }
    }
    assert!(
        inherited.is_empty(),
        "these selected outfits inherit the wearer's native collar/gorget pigment: {inherited:?}"
    );
}

#[test]
fn wardrobe_female_bodices_keep_their_shaped_neckline_on_every_wearer() {
    for model in [
        CreatureModel::Nymph,
        CreatureModel::Sentinel,
        CreatureModel::Zombie,
        CreatureModel::Medusa,
    ] {
        for build in 0..3 {
            let base = Look::with_choice(
                model,
                2,
                Some(Variant {
                    woman: true,
                    build,
                    ..default()
                }),
            );
            for wardrobe in [Wardrobe::Commoner, Wardrobe::Courtier] {
                let look = base.clone().dressed(wardrobe);
                let (sampled, exposed) = chest_color_count(&look, look.skin, |at| {
                    at.x.abs() < 0.025 && (0.43..0.48).contains(&at.y) && at.z > 0.0
                });
                assert!(
                    sampled > 0,
                    "{model:?} {wardrobe:?} build {build}: sample the neckline"
                );
                assert!(exposed > 0, "{model:?} {wardrobe:?} build {build}: her outfit must retain its shaped open neckline");
            }
        }
    }
}

#[test]
fn wardrobe_keeps_zombie_decay_on_replacement_cloth() {
    for woman in [false, true] {
        let look = Look::with_choice(
            CreatureModel::Zombie,
            2,
            Some(Variant { woman, ..default() }),
        )
        .dressed(Wardrobe::Commoner);
        let (sampled, torn) = chest_color_count(&look, look.skin * 0.85, |at| {
            (0.03..0.08).contains(&at.x) && (0.23..0.28).contains(&at.y) && at.z > 0.0
        });
        assert!(
            sampled > 0 && torn > 0,
            "zombie decay survives a change of outfit"
        );
    }
}

#[test]
fn wardrobe_ranger_carries_a_fitted_baldric_on_different_bodies() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Orc,
        CreatureModel::Nymph,
    ] {
        for woman in [false, true] {
            let look = Look::with_choice(model, 2, Some(Variant { woman, ..default() }))
                .dressed(Wardrobe::Ranger);
            let (sampled, strap) = chest_color_count(&look, LEATHER, |at| {
                at.x.abs() < 0.12 && (0.24..0.38).contains(&at.y) && at.z > 0.0
            });
            assert!(
                sampled > 0 && strap > 0,
                "{model:?} woman {woman}: ranger outfit must bring its fitted chest strap"
            );
        }
    }
}

#[test]
fn wardrobe_lower_panels_follow_the_outfit_rather_than_the_native_costume() {
    // Hero and Common use the same body recipe. Their native clothes have
    // different skirt splits; dressing either body in the same outfit must
    // produce the same tunic AND outer peplum, for either sex and every build.
    let panels = |style, wardrobe, woman, build| {
        let mut look = Look::with_choice(
            CreatureModel::Human,
            2,
            Some(Variant {
                woman,
                build,
                ..default()
            }),
        );
        look.style = style;
        let look = look.dressed(wardrobe);
        let (world, rig, _, meshes) = spawn_one(&look);
        rig.drapes
            .iter()
            .filter(|(_, kind)| matches!(kind, Drape::Skirt(_)))
            .map(|(entity, _)| points_under(&world, &meshes, *entity))
            .collect::<Vec<_>>()
    };
    for wardrobe in [
        Wardrobe::Commoner,
        Wardrobe::Ranger,
        Wardrobe::Knight,
        Wardrobe::Courtier,
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                let hero = panels(Style::Hero, wardrobe, woman, build);
                assert_eq!(hero.len(), if wardrobe == Wardrobe::Ranger { 2 } else { 1 });
                assert!(hero.iter().all(|panel| !panel.is_empty()));
                assert!(
                    hero == panels(Style::Common, wardrobe, woman, build),
                    "{wardrobe:?} woman {woman} build {build}: all lower panels travel with the outfit"
                );
            }
        }
    }
}

#[test]
fn peplum_attachment_rings_meet_the_emitted_torso_shell() {
    use bevy::render::mesh::VertexAttributeValues;

    // Both roots are direct children of hips, but their mesh scales differ.
    // Keep only the shell's authored pigment range on the torso so belts,
    // baldrics and skin cannot masquerade as attachment support. Sculpt
    // occlusion multiplies linear RGB after paint, so compare chromaticity
    // rather than rejecting valid shell vertices for their baked brightness.
    let triangles = |world: &World, meshes: &Assets<Mesh>, root: Entity, shell: Option<Vec3>| {
        let root_transform = world.get::<Transform>(root).unwrap().compute_affine();
        let mut result = Vec::new();
        for child in world.get::<Children>(root).unwrap().iter() {
            let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0)) else {
                continue;
            };
            let Some(VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("attachment mesh positions")
            };
            let Some(VertexAttributeValues::Float32x4(colors)) =
                mesh.attribute(Mesh::ATTRIBUTE_COLOR)
            else {
                panic!("attachment mesh pigments")
            };
            let transform =
                root_transform * world.get::<Transform>(child).unwrap().compute_affine();
            let indices: Vec<usize> = mesh.indices().unwrap().iter().collect();
            for ids in indices.as_chunks::<3>().0 {
                if let Some(color) = shell {
                    let matches = |i: usize| {
                        let in_range = |factor: f32| {
                            let low = Color::srgb(
                                color.x * factor * 0.965,
                                color.y * factor * 0.965,
                                color.z * factor * 0.965,
                            )
                            .to_linear();
                            let high = Color::srgb(
                                color.x * factor * 1.035,
                                color.y * factor * 1.035,
                                color.z * factor * 1.035,
                            )
                            .to_linear();
                            let low = Vec3::new(low.red, low.green, low.blue).normalize_or_zero();
                            let high =
                                Vec3::new(high.red, high.green, high.blue).normalize_or_zero();
                            let pigment = Vec3::new(colors[i][0], colors[i][1], colors[i][2])
                                .normalize_or_zero();
                            let minimum = low.min(high);
                            let maximum = low.max(high);
                            (0..3).all(|axis| {
                                pigment[axis] >= minimum[axis] - 1e-5
                                    && pigment[axis] <= maximum[axis] + 1e-5
                            })
                        };
                        in_range(1.0)
                            || (positions[i][0].abs() < 0.008
                                && positions[i][2] > 0.0
                                && in_range(0.7))
                    };
                    if !ids.iter().all(|&i| matches(i)) {
                        continue;
                    }
                }
                result.push(ids.map(|i| transform.transform_point3(Vec3::from(positions[i]))));
            }
        }
        assert!(
            !result.is_empty(),
            "the authored attachment mesh must be present"
        );
        result
    };
    let mut failures = Vec::new();
    for (model, wardrobe) in [
        (CreatureModel::Human, Wardrobe::Native),
        (CreatureModel::Scout, Wardrobe::Native),
        (CreatureModel::Hobgoblin, Wardrobe::Native),
        (CreatureModel::Orc, Wardrobe::Native),
        (CreatureModel::Human, Wardrobe::Ranger),
        (CreatureModel::Sentinel, Wardrobe::Ranger),
        (CreatureModel::Zombie, Wardrobe::Ranger),
        (CreatureModel::Orc, Wardrobe::Ranger),
        (CreatureModel::Nymph, Wardrobe::Ranger),
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                let look = Look::with_choice(
                    model,
                    2,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                )
                .dressed(wardrobe);
                let Some(outfit::Outer::Shell {
                    color,
                    peplum: Some(peplum),
                    ..
                }) = outfit::Outfit::worn(&look).outer
                else {
                    panic!("seam fixture must wear a shell and peplum")
                };
                let (world, rig, _, meshes) = spawn_one(&look);
                let hips = rig.joint(J::Hips).unwrap();
                let torso = rig.joint(J::Torso).unwrap();
                let drape = rig
                    .drapes
                    .iter()
                    .find(|(_, kind)| *kind == Drape::Skirt(peplum.length))
                    .unwrap()
                    .0;
                assert!(world.get::<Children>(hips).unwrap().contains(&torso));
                assert!(world.get::<Children>(hips).unwrap().contains(&drape));
                let shell = triangles(&world, &meshes, torso, Some(color));
                let outer = triangles(&world, &meshes, drape, None);
                let top_y = outer
                    .iter()
                    .flatten()
                    .map(|p| p.y)
                    .fold(f32::NEG_INFINITY, f32::max);
                // The first side row can flare sharply enough that its
                // normal is mostly vertical. Recover the outer boundary of
                // the horizontal top cap instead of rejecting those faces
                // by a radial-normal threshold. Inner-cap edges cannot win
                // the farthest intersection along a horizontal ray.
                let mut top_edges = Vec::new();
                for tri in &outer {
                    for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                        if (tri[a].y - top_y).abs() < 1e-5 && (tri[b].y - top_y).abs() < 1e-5 {
                            top_edges.push((tri[a].xz(), tri[b].xz()));
                        }
                    }
                }
                let ring: Vec<Vec3> = (0..128)
                    .map(|i| {
                        let angle = i as f32 * std::f32::consts::TAU / 128.0;
                        let direction = Vec2::new(angle.sin(), angle.cos());
                        let mut radius = 0.0f32;
                        for &(a, b) in &top_edges {
                            let edge = b - a;
                            let determinant = direction.perp_dot(edge);
                            if determinant.abs() < 1e-10 {
                                continue;
                            }
                            let along = a.perp_dot(direction) / determinant;
                            if (-1e-5..=1.0 + 1e-5).contains(&along) {
                                radius = radius.max(a.perp_dot(edge) / determinant);
                            }
                        }
                        assert!(radius > 0.0, "{model:?} {wardrobe:?} woman {woman} build {build}: top y={top_y:.5} has no boundary at angle={angle:.5}, edges={}", top_edges.len());
                        Vec3::new(direction.x * radius, top_y, direction.y * radius)
                    })
                    .collect();
                let mut hits = [0usize; 4];
                let mut minimum = [f32::INFINITY; 4];
                let mut maximum = [f32::NEG_INFINITY; 4];
                for point in &ring {
                    let radius = point.xz().length();
                    let dir = Vec3::new(point.x, 0.0, point.z) / radius;
                    let origin = Vec3::Y * point.y;
                    let mut reach = 0.0f32;
                    for tri in &shell {
                        if tri.iter().all(|v| v.y < point.y - 1e-6)
                            || tri.iter().all(|v| v.y > point.y + 1e-6)
                        {
                            continue;
                        }
                        let edge1 = tri[1] - tri[0];
                        let edge2 = tri[2] - tri[0];
                        let cross = dir.cross(edge2);
                        let determinant = edge1.dot(cross);
                        if determinant.abs() < 1e-10 {
                            continue;
                        }
                        let delta = origin - tri[0];
                        let u = delta.dot(cross) / determinant;
                        let q = delta.cross(edge1);
                        let v = dir.dot(q) / determinant;
                        if u >= -1e-5 && v >= -1e-5 && u + v <= 1.0 + 1e-5 {
                            reach = reach.max(edge2.dot(q) / determinant);
                        }
                    }
                    if reach <= 0.0 {
                        continue;
                    }
                    let sector = if point.x.abs() > point.z.abs() {
                        if point.x > 0.0 {
                            1
                        } else {
                            3
                        }
                    } else if point.z > 0.0 {
                        0
                    } else {
                        2
                    };
                    hits[sector] += 1;
                    minimum[sector] = minimum[sector].min(radius - reach);
                    maximum[sector] = maximum[sector].max(radius - reach);
                }
                // A seam can overlap by its thickness and tolerate the SDF
                // mesh step, but cannot float centimetres away from support.
                if hits.iter().any(|&n| n < 16)
                    || hits.iter().sum::<usize>() * 10 < ring.len() * 9
                    || minimum.iter().any(|&gap| gap < -0.008)
                    || maximum.iter().any(|&gap| gap > 0.012)
                {
                    failures.push(format!("{model:?} {wardrobe:?} woman {woman} build {build}: attachment y={top_y:.5}, ring={}, front/right/back/left hits={hits:?}, radial gaps min={minimum:?} max={maximum:?}", ring.len()));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "peplum top must meet its emitted shell around the waist:\n{}",
        failures.join("\n")
    );
}

#[test]
fn outer_peplums_clear_the_tunic_mesh_in_the_shared_hips_frame() {
    use bevy::render::mesh::VertexAttributeValues;

    // Include the drape's own transform as well as its mesh children's
    // proportions: the two garments have different offsets and scales.
    let triangles = |world: &World, meshes: &Assets<Mesh>, root: Entity| {
        let mut out = Vec::new();
        let mut stack = vec![(root, world.get::<Transform>(root).unwrap().compute_affine())];
        while let Some((entity, to_hips)) = stack.pop() {
            if let Some(mesh) = world.get::<Mesh3d>(entity).and_then(|m| meshes.get(&m.0)) {
                let Some(VertexAttributeValues::Float32x3(positions)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    panic!("drape positions")
                };
                let indices: Vec<usize> = mesh.indices().unwrap().iter().collect();
                out.extend(indices.as_chunks::<3>().0.iter().map(|ids| {
                    [ids[0], ids[1], ids[2]]
                        .map(|i| to_hips.transform_point3(Vec3::from(positions[i])))
                }));
            }
            if let Some(children) = world.get::<Children>(entity) {
                for child in children.iter() {
                    let local = world.get::<Transform>(child).unwrap().compute_affine();
                    stack.push((child, to_hips * local));
                }
            }
        }
        out
    };
    let cases = [
        (CreatureModel::Human, Wardrobe::Ranger, false),
        (CreatureModel::Sentinel, Wardrobe::Ranger, false),
        (CreatureModel::Zombie, Wardrobe::Ranger, false),
        (CreatureModel::Orc, Wardrobe::Ranger, false),
        (CreatureModel::Nymph, Wardrobe::Ranger, false),
        (CreatureModel::Human, Wardrobe::Native, false),
        (CreatureModel::Scout, Wardrobe::Native, false),
        (CreatureModel::Hobgoblin, Wardrobe::Native, false),
        (CreatureModel::Orc, Wardrobe::Native, false),
        // A short trimmed tunic places its raised hem inside the overlap.
        (CreatureModel::Human, Wardrobe::Ranger, true),
    ];
    for (model, wardrobe, trimmed) in cases {
        for woman in [false, true] {
            for build in 0..3 {
                // Include a wide phase difference (shade 4, pattern 2), while
                // retaining the original first-red seed pair in the matrix.
                let (shade, pattern) =
                    [(0, 3), (4, 2), (3, 0)][(build as usize + usize::from(woman)) % 3];
                let mut look = Look::with_choice(
                    model,
                    2,
                    Some(Variant {
                        woman,
                        build,
                        shade,
                        pattern,
                        ..default()
                    }),
                )
                .dressed(wardrobe);
                if trimmed {
                    let skirt = look.outfit.as_mut().unwrap().skirt.as_mut().unwrap();
                    skirt.length = 0.055;
                    skirt.trim = Some(Vec3::new(0.8, 0.6, 0.1));
                }
                let outfit = outfit::Outfit::worn(&look);
                let tunic_length = outfit.skirt.unwrap().length;
                let Some(outfit::Outer::Shell {
                    peplum: Some(peplum),
                    ..
                }) = outfit.outer
                else {
                    panic!("the case must wear both lower garment layers")
                };
                let (world, rig, _, meshes) = spawn_one(&look);
                let hips = rig.joint(J::Hips).unwrap();
                let find = |length| {
                    let entity = rig
                        .drapes
                        .iter()
                        .find(|(_, kind)| *kind == Drape::Skirt(length))
                        .expect("the specified garment is emitted")
                        .0;
                    assert!(world.get::<Children>(hips).unwrap().contains(&entity));
                    triangles(&world, &meshes, entity)
                };
                let tunic = find(tunic_length);
                let outer = find(peplum.length);
                // Precompute plane edges and rejection bounds for the real
                // outer tunic triangles, including its turned hem decoration.
                let under: Vec<_> = tunic
                    .iter()
                    .filter_map(|tri| {
                        let center = (tri[0] + tri[1] + tri[2]) / 3.0;
                        let edge1 = tri[1] - tri[0];
                        let edge2 = tri[2] - tri[0];
                        let normal = edge1.cross(edge2);
                        if normal.dot(Vec3::new(center.x, 0.0, center.z)) <= 0.0 {
                            return None;
                        }
                        Some((
                            tri[0],
                            edge1,
                            edge2,
                            tri.iter().map(|v| v.y).fold(f32::INFINITY, f32::min),
                            tri.iter().map(|v| v.y).fold(f32::NEG_INFINITY, f32::max),
                            tri.iter().map(|v| v.xz().length()).fold(0.0, f32::max),
                        ))
                    })
                    .collect();
                assert!(!under.is_empty());
                let mut samples = BTreeMap::new();
                for tri in outer {
                    let center = (tri[0] + tri[1] + tri[2]) / 3.0;
                    let radial = Vec3::new(center.x, 0.0, center.z);
                    let normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]);
                    // Select the inward shell faces, excluding horizontal
                    // closures and the split's narrow turned edges.
                    if normal.dot(radial) >= -0.35 * normal.length() * radial.length() {
                        continue;
                    }
                    for point in [
                        tri[0],
                        tri[1],
                        tri[2],
                        center,
                        (tri[0] + tri[1]) * 0.5,
                        (tri[1] + tri[2]) * 0.5,
                        (tri[2] + tri[0]) * 0.5,
                    ] {
                        let key = point.to_array().map(|v| (v * 1_000_000.0).round() as i32);
                        samples.entry(key).or_insert(point);
                    }
                }
                let mut coverage = [0usize; 4];
                let mut low = [f32::INFINITY; 4];
                let mut high = [f32::NEG_INFINITY; 4];
                let mut minimum_gap = f32::INFINITY;
                let mut worst = Vec3::ZERO;
                let mut maximum_gap = f32::NEG_INFINITY;
                let mut farthest = Vec3::ZERO;
                for point in samples.values() {
                    let radius = point.xz().length();
                    let dir = Vec3::new(point.x, 0.0, point.z) / radius;
                    let origin = Vec3::Y * point.y;
                    let mut reach = 0.0f32;
                    for &(at, edge1, edge2, min_y, max_y, max_radius) in &under {
                        if point.y < min_y - 1e-6 || point.y > max_y + 1e-6 || max_radius < reach {
                            continue;
                        }
                        let cross = dir.cross(edge2);
                        let determinant = edge1.dot(cross);
                        if determinant.abs() < 1e-10 {
                            continue;
                        }
                        let delta = origin - at;
                        let u = delta.dot(cross) / determinant;
                        let q = delta.cross(edge1);
                        let v = dir.dot(q) / determinant;
                        if u < -1e-5 || v < -1e-5 || u + v > 1.0 + 1e-5 {
                            continue;
                        }
                        reach = reach.max(edge2.dot(q) / determinant);
                    }
                    if reach <= 0.0 {
                        // Above the tunic or through its actual front split:
                        // there is no underlying cloth to intersect here.
                        continue;
                    }
                    let sector = if point.x.abs() > point.z.abs() {
                        if point.x > 0.0 {
                            1
                        } else {
                            3
                        }
                    } else if point.z > 0.0 {
                        0
                    } else {
                        2
                    };
                    coverage[sector] += 1;
                    low[sector] = low[sector].min(point.y);
                    high[sector] = high[sector].max(point.y);
                    let gap = radius - reach;
                    if gap > maximum_gap {
                        maximum_gap = gap;
                        farthest = *point;
                    }
                    if gap < minimum_gap {
                        minimum_gap = gap;
                        worst = *point;
                    }
                }
                for sector in 0..4 {
                    assert!(
                        coverage[sector] >= 40 && high[sector] - low[sector] > 0.025,
                        "{model:?} {wardrobe:?} woman {woman} build {build} trimmed {trimmed}: insufficient layer coverage in sector {sector}: {} hits over {:.4}",
                        coverage[sector], high[sector] - low[sector]
                    );
                }
                assert!(
                    minimum_gap > 0.0005,
                    "{model:?} {wardrobe:?} woman {woman} build {build} shade {shade} pattern {pattern} trimmed {trimmed}: peplum inner surface must clear tunic outer surface; radial gap {minimum_gap:.5} at {worst:?}"
                );
                // A short outer layer needs ease, but must not become a wide
                // shelf around an already fitted tunic. Measure both emitted
                // meshes at matching physical heights, independently of cuts.
                let resolved_build = rig.humanoid.unwrap().build;
                let maximum_ease = 0.06 * resolved_build;
                assert!(
                    maximum_gap <= maximum_ease,
                    "{model:?} {wardrobe:?} woman {woman} build {build} shade {shade} pattern {pattern} trimmed {trimmed}: short peplum stands too far from tunic; maximum gap {maximum_gap:.5} exceeds {maximum_ease:.5} (normalized {:.5}) at {farthest:?}",
                    maximum_gap / resolved_build
                );
            }
        }
    }
}

#[test]
fn wardrobe_accessories_clear_plate_and_its_tabard_on_every_build() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Orc,
        CreatureModel::Nymph,
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                for reverse in [false, true] {
                    let mut look = Look::with_choice(
                        model,
                        2,
                        Some(Variant {
                            woman,
                            build,
                            pattern: build,
                            ..default()
                        }),
                    )
                    .dressed(Wardrobe::Knight);
                    // Keep the preset's own accessory and test either shoulder.
                    // Clothing, armor and hardware must remain composable.
                    look.outfit
                        .as_mut()
                        .unwrap()
                        .baldric
                        .as_mut()
                        .expect("the knight preset must carry its baldric")
                        .reverse = reverse;
                    let body = anatomy::HumanoidBody::resolve(&look);
                    let plate =
                        geometry::torso_shape(body, Some(geometry::TorsoMaterial::Plate), true);
                    let (_, points) = chest_pigment_points(&look, LEATHER, |at| {
                        (0.24..0.38).contains(&at.y) && at.x.abs() < 0.12 && at.z > 0.0
                    });
                    assert!(points.len() > 10, "{model:?} woman {woman} build {build} reverse {reverse}: sample the actual strap mesh");
                    let clearance = points
                        .into_iter()
                        .map(|at| plate.distance(at))
                        .fold(f32::INFINITY, f32::min);
                    assert!(clearance > 0.028, "{model:?} woman {woman} build {build} reverse {reverse}: strap must clear plate, tabard and heraldry; minimum clearance {clearance:.4}");
                }
            }
        }
    }
}

#[test]
fn layered_skirts_drive_motion_with_their_rendered_lengths() {
    for woman in [false, true] {
        let look = Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman,
                ..Default::default()
            }),
        )
        .dressed(Wardrobe::Goddess);
        let skirt = outfit::Outfit::worn(&look).skirt.unwrap();
        let (_, rig, _, _) = spawn_one(&look);
        let mut lengths: Vec<f32> = rig
            .drapes
            .iter()
            .filter_map(|(_, drape)| {
                if let Drape::Skirt(length) = drape {
                    Some(*length)
                } else {
                    None
                }
            })
            .collect();
        lengths.sort_by(f32::total_cmp);
        let mut expected = vec![skirt.length];
        expected.extend(
            skirt
                .layers
                .iter()
                .flatten()
                .map(|layer| skirt.length + layer.longer),
        );
        expected.sort_by(f32::total_cmp);
        assert_eq!(lengths.len(), expected.len());
        for (actual, expected) in lengths.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-6,
                "cloth motion must use its rendered length: woman={woman}, actual={actual}, expected={expected}");
        }
    }
}

#[test]
fn layered_skirts_preserve_their_relative_fit_while_swaying() {
    for wardrobe in [Wardrobe::Cleric, Wardrobe::Druid] {
        let look = Look::with_choice(
            CreatureModel::Human,
            9,
            Some(Variant {
                woman: true,
                ..default()
            }),
        )
        .dressed(wardrobe);
        let (mut app, root) = animate_look(look, 0, |_| Vec3::ZERO);
        let mut largest_sway = 0.0f32;
        let mut final_sway = 0.0;
        for frame in 0..160 {
            let position = if frame < 45 {
                Vec3::Z * frame as f32 * 0.03
            } else if frame < 90 {
                Vec3::new((frame - 45) as f32 * 0.03, 0.0, 1.35)
            } else {
                Vec3::new(1.35, 0.0, 1.35)
            };
            app.world_mut()
                .get_mut::<Transform>(root)
                .unwrap()
                .translation = position;
            app.update();
            let rig = app.world().get::<Rig>(root).unwrap();
            let rotations: Vec<_> = rig
                .drapes
                .iter()
                .filter(|(_, kind)| matches!(kind, Drape::Skirt(_)))
                .map(|(entity, _)| app.world().get::<Transform>(*entity).unwrap().rotation)
                .collect();
            assert!(rotations.len() >= 2);
            final_sway = rotations[0].angle_between(Quat::IDENTITY);
            largest_sway = largest_sway.max(final_sway);
            for rotation in &rotations[1..] {
                assert!(rotation.abs_diff_eq(rotations[0],1e-5),"{wardrobe:?} frame={frame}: layers lose fitted clearance while walking, turning or settling");
            }
        }
        assert!(
            largest_sway > 0.01,
            "the layered garment must still sway while travelling"
        );
        assert!(
            final_sway < 0.03,
            "the garment must settle after stopping: {final_sway}"
        );
    }
}

#[test]
fn fitted_belt_front_clears_the_torso_through_a_jump() {
    use bevy::render::mesh::VertexAttributeValues;
    for model in [
        CreatureModel::Human,
        CreatureModel::Orc,
        CreatureModel::Nymph,
    ] {
        for woman in [false, true] {
            let look = Look::with_choice(model, 9, Some(Variant { woman, ..default() }))
                .dressed(Wardrobe::Cleric);
            let body = anatomy::HumanoidBody::resolve(&look);
            let support = geometry::torso_shape(body, None, false);
            let (mut app, root) = animate_look(look, 1, |_| Vec3::ZERO);
            let rig = app.world().get::<Rig>(root).unwrap();
            let hips = rig.joint(J::Hips).unwrap();
            let torso = rig.joint(J::Torso).unwrap();
            let leather = LEATHER * 0.8;
            let color = Color::srgb(leather.x, leather.y, leather.z).to_linear();
            let target = Vec3::new(color.red, color.green, color.blue);
            let mut samples = Vec::new();
            for joint in [hips, torso] {
                for child in app.world().get::<Children>(joint).unwrap().iter() {
                    let Some(handle) = app.world().get::<Mesh3d>(child) else {
                        continue;
                    };
                    let mesh = app
                        .world()
                        .resource::<Assets<Mesh>>()
                        .get(&handle.0)
                        .unwrap();
                    let Some(VertexAttributeValues::Float32x3(points)) =
                        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                    else {
                        panic!("points")
                    };
                    let Some(VertexAttributeValues::Float32x4(colors)) =
                        mesh.attribute(Mesh::ATTRIBUTE_COLOR)
                    else {
                        panic!("colors")
                    };
                    let transform = app
                        .world()
                        .get::<Transform>(child)
                        .unwrap()
                        .compute_affine();
                    for (point, color) in points.iter().zip(colors) {
                        let point = transform.transform_point3(Vec3::from(*point));
                        let hip_point = point
                            + if joint == torso {
                                body.torso
                            } else {
                                Vec3::ZERO
                            };
                        if hip_point.x.abs() < 0.02
                            && hip_point.z > 0.0
                            && (0.045..=0.095).contains(&hip_point.y)
                            && Vec3::new(color[0], color[1], color[2]).distance(target) < 1e-5
                        {
                            samples.push((joint, point));
                        }
                    }
                }
            }
            assert!(samples.len() > 20, "exercise actual emitted belt vertices");
            app.world_mut().get_mut::<Anim>(root).unwrap().action = Some((Move::Jump, 0.0));
            for frame in 0..70 {
                app.update();
                let transform = app
                    .world()
                    .get::<Transform>(torso)
                    .unwrap()
                    .compute_affine();
                for &(joint, point) in &samples {
                    let torso_point = if joint == torso {
                        point
                    } else {
                        transform.inverse().transform_point3(point)
                    };
                    let gap = support.distance(torso_point / anatomy::HIP_SCALE);
                    assert!(
                        gap >= 0.001,
                        "frame={frame}: fitted belt loses clearance from moving torso, gap={gap}"
                    );
                }
            }
        }
    }
}

#[test]
fn both_mages_swap_every_wardrobe_and_restore_their_native_identity() {
    for model in [CreatureModel::Mage, CreatureModel::RedMage] {
        for woman in [false, true] {
            let native = Look::with_choice(model, 17, Some(Variant { woman, ..default() }));
            assert!(
                native.dresses(),
                "{model:?} must join the swappable humanoids"
            );
            for wardrobe in Wardrobe::ALL {
                let dressed = native.clone().dressed(wardrobe);
                assert_eq!(dressed.style, native.style);
                assert_eq!(dressed.skin, native.skin);
                assert_eq!(dressed.scale, native.scale);
                assert_eq!(dressed.v.woman, woman);
                assert_eq!(dressed.outfit.is_some(), wardrobe != Wardrobe::Native);
                let restored = dressed.dressed(Wardrobe::Native);
                assert_eq!(restored.key, native.key);
                assert!(restored.outfit.is_none());
            }
        }
    }
}

#[test]
fn mage_replacements_own_their_garments_without_native_robe_or_bells() {
    for model in [CreatureModel::Mage, CreatureModel::RedMage] {
        for woman in [false, true] {
            let native = Look::with_choice(model, 17, Some(Variant { woman, ..default() }));
            let (_, native_rig, _, _) = spawn_one(&native);
            assert!(native_rig.robe.is_some());
            for wardrobe in [
                Wardrobe::Commoner,
                Wardrobe::Knight,
                Wardrobe::Leotard,
                Wardrobe::RoyalPrincess,
            ] {
                let dressed = native.clone().dressed(wardrobe);
                assert!(dressed.outfit.is_some(), "{model:?} {wardrobe:?}");
                let (_, rig, _, _) = spawn_one(&dressed);
                assert!(
                    rig.robe.is_none(),
                    "native robe must not overlay {wardrobe:?}"
                );
                assert_eq!(
                    rig.rest, native_rig.rest,
                    "clothes must preserve skeletal landmarks"
                );
                // Holding anatomy constant isolates accidental caster-only costume additions.
                let mut equivalent = dressed.clone();
                equivalent.style = Style::Scout;
                for joint in [J::ForeL, J::ForeR] {
                    assert!(
                        surface_signature(joint_surface_samples(&dressed, joint))
                            == surface_signature(joint_surface_samples(&equivalent, joint)),
                        "{model:?} {wardrobe:?}: native bell sleeve leaked into replacement"
                    );
                }
            }
            let (_, restored, _, _) = spawn_one(
                &native
                    .clone()
                    .dressed(Wardrobe::Ranger)
                    .dressed(Wardrobe::Native),
            );
            assert!(restored.robe.is_some());
        }
    }
}

#[test]
fn equipment_composes_visible_parts_without_replacing_the_gown_or_native_robe() {
    let mut equipment = Equipment::default();
    for name in ["helm", "boots", "cloak"] {
        equipment.select(name).unwrap();
    }
    let base = Look::with_choice(
        CreatureModel::Human,
        17,
        Some(Variant {
            woman: true,
            ..default()
        }),
    )
    .dressed(Wardrobe::RoyalPrincess);
    let worn = base.clone().equipped(equipment);
    let (_, rig, _, _) = spawn_one(&worn);
    assert_eq!(rig.cloaks.len(), 1);
    assert!(!geometry::loose_fall(&worn));
    for joint in [J::Torso, J::Hips, J::ArmL] {
        assert!(
            surface_signature(joint_surface_samples_with_drapes(&base, joint, false))
                == surface_signature(joint_surface_samples_with_drapes(&worn, joint, false)),
            "{joint:?}: equipment changed unrelated gown geometry"
        );
    }
    for joint in [J::Head, J::FootL] {
        assert!(
            surface_signature(joint_surface_samples_with_drapes(&base, joint, false))
                != surface_signature(joint_surface_samples_with_drapes(&worn, joint, false)),
            "{joint:?}: selected equipment must change emitted geometry"
        );
        let restored = worn.clone().equipped(Equipment::default());
        assert!(
            surface_signature(joint_surface_samples_with_drapes(&restored, joint, false))
                == surface_signature(joint_surface_samples_with_drapes(&base, joint, false))
        );
    }
    for model in [CreatureModel::Mage, CreatureModel::RedMage] {
        let native = Look::of(model, 17).equipped(equipment);
        let (_, rig, _, _) = spawn_one(&native);
        assert!(
            rig.robe.is_some(),
            "equipping accessories retains native robe deformation"
        );
    }
}

#[test]
fn equipment_head_cache_restores_crown_and_reuses_head_when_only_boots_change() {
    let mut materials = Assets::<StandardMaterial>::default();
    let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
    let mut meshes = Assets::<Mesh>::default();
    let mut world = World::new();
    let base = Look::with_choice(
        CreatureModel::Human,
        17,
        Some(Variant {
            woman: true,
            ..default()
        }),
    )
    .dressed(Wardrobe::RoyalPrincess);
    let mut head = |look: &Look| {
        let parent = world.spawn(Transform::default()).id();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let rig = spawn(
            &mut Commands::new(&mut queue, &world),
            &mut library,
            &mut meshes,
            parent,
            look,
        );
        queue.apply(&mut world);
        world
            .get::<Children>(rig.joint(J::Head).unwrap())
            .unwrap()
            .iter()
            .filter_map(|e| world.get::<Mesh3d>(e).map(|m| m.0.id()))
            .collect::<Vec<_>>()
    };
    let crown = head(&base);
    let mut equipment = Equipment::default();
    equipment.select("boots").unwrap();
    assert_eq!(head(&base.clone().equipped(equipment)), crown);
    equipment.select("helm").unwrap();
    let helmet = head(&base.clone().equipped(equipment));
    assert_ne!(helmet, crown);
    assert_eq!(
        head(
            &base
                .clone()
                .equipped(equipment)
                .equipped(Equipment::default())
        ),
        crown
    );
}

#[test]
fn breastplate_changes_torso_preserves_limbs_and_restores_gown() {
    let mut equipment = Equipment::default();
    equipment.select("breastplate").unwrap();
    for wardrobe in [Wardrobe::RoyalPrincess, Wardrobe::Bard] {
        let base = Look::with_choice(
            CreatureModel::Human,
            17,
            Some(Variant {
                woman: true,
                ..default()
            }),
        )
        .dressed(wardrobe);
        let armored = base.clone().equipped(equipment);
        assert!(
            surface_signature(joint_surface_samples_with_drapes(&base, J::Torso, false))
                != surface_signature(joint_surface_samples_with_drapes(&armored, J::Torso, false))
        );
        for joint in [J::ArmL, J::ForeL, J::LegL, J::ShinL, J::FootL, J::Head] {
            assert!(
                surface_signature(joint_surface_samples(&base, joint))
                    == surface_signature(joint_surface_samples(&armored, joint)),
                "{wardrobe:?} {joint:?}: torso armor must retain compatible pieces"
            );
        }
        let restored = armored.equipped(Equipment::default());
        assert!(
            surface_signature(joint_surface_samples_with_drapes(&base, J::Torso, false))
                == surface_signature(joint_surface_samples_with_drapes(
                    &restored,
                    J::Torso,
                    false
                ))
        );
    }
}

#[test]
fn sylvan_duelist_generates_finite_layers_and_keeps_mage_identity() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Orc,
        CreatureModel::Mage,
        CreatureModel::RedMage,
    ] {
        for woman in [false, true] {
            let base = Look::with_choice(
                model,
                17,
                Some(Variant {
                    woman,
                    build: 2,
                    ..default()
                }),
            );
            let look = base.clone().dressed(Wardrobe::SylvanDuelist);
            let (_, rig, _, meshes) = spawn_one(&look);
            assert!(rig.joint(J::HandR).is_some());
            assert!(!rig.cloaks.is_empty());
            assert_eq!(look.style, base.style);
            assert_eq!(look.clone().dressed(Wardrobe::Native).key, base.key);
            assert!(meshes.len() > 5);
            for (_, mesh) in meshes.iter() {
                let positions = mesh
                    .attribute(Mesh::ATTRIBUTE_POSITION)
                    .unwrap()
                    .as_float3()
                    .unwrap();
                assert!(!positions.is_empty());
                assert!(
                    positions.iter().flatten().all(|v| v.is_finite()),
                    "{model:?} woman={woman}"
                );
            }
        }
    }
}

#[test]
fn crossed_belts_follow_their_fitted_torso_through_a_jump() {
    use bevy::render::mesh::VertexAttributeValues;
    for model in [
        CreatureModel::Human,
        CreatureModel::Mage,
        CreatureModel::Nymph,
    ] {
        for woman in [true] {
            let look = Look::with_choice(model, 9, Some(Variant { woman, ..default() }))
                .dressed(Wardrobe::SylvanDuelist);
            let body = anatomy::HumanoidBody::resolve(&look);
            let support = geometry::torso_shape(body, None, false);
            let (mut app, root) = animate_look(look, 1, |_| Vec3::ZERO);
            let rig = app.world().get::<Rig>(root).unwrap();
            let hips = rig.joint(J::Hips).unwrap();
            let torso = rig.joint(J::Torso).unwrap();
            let leather = Vec3::new(0.10, 0.075, 0.065);
            let color = Color::srgb(leather.x, leather.y, leather.z).to_linear();
            let target = Vec3::new(color.red, color.green, color.blue);
            let mut samples = Vec::new();
            for joint in [hips, torso] {
                for child in app.world().get::<Children>(joint).unwrap().iter() {
                    let Some(handle) = app.world().get::<Mesh3d>(child) else {
                        continue;
                    };
                    let mesh = app
                        .world()
                        .resource::<Assets<Mesh>>()
                        .get(&handle.0)
                        .unwrap();
                    let Some(VertexAttributeValues::Float32x3(points)) =
                        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                    else {
                        panic!("points")
                    };
                    let Some(VertexAttributeValues::Float32x4(colors)) =
                        mesh.attribute(Mesh::ATTRIBUTE_COLOR)
                    else {
                        panic!("colors")
                    };
                    let transform = app
                        .world()
                        .get::<Transform>(child)
                        .unwrap()
                        .compute_affine();
                    for (point, color) in points.iter().zip(colors) {
                        let point = transform.transform_point3(Vec3::from(*point));
                        let hip_point = point
                            + if joint == torso {
                                body.torso
                            } else {
                                Vec3::ZERO
                            };
                        if hip_point.x.abs() < 0.02
                            && hip_point.z > 0.0
                            && (0.075..=0.105).contains(&hip_point.y)
                            && Vec3::new(color[0], color[1], color[2]).distance(target) < 1e-5
                        {
                            samples.push((joint, point));
                        }
                    }
                }
            }
            assert!(samples.len() > 20, "exercise actual emitted belt vertices");
            app.world_mut().get_mut::<Anim>(root).unwrap().action = Some((Move::Jump, 0.0));
            for frame in 0..70 {
                app.update();
                let transform = app
                    .world()
                    .get::<Transform>(torso)
                    .unwrap()
                    .compute_affine();
                for &(joint, point) in &samples {
                    let torso_point = if joint == torso {
                        point
                    } else {
                        transform.inverse().transform_point3(point)
                    };
                    let gap = support.distance(torso_point / anatomy::HIP_SCALE);
                    assert!(
                        (0.003..0.025).contains(&gap),
                        "frame={frame}: fitted belt loses clearance from moving torso, gap={gap}"
                    );
                }
            }
        }
    }
}

/// Cloth and hair that deform keep generous culling boxes; picking must know
/// them as such, or a figure's swinging box claims the cursor cells away.
#[test]
fn every_culling_sized_box_is_marked_loose_and_no_body_box_is() {
    let woman = |model| {
        Look::with_choice(
            model,
            7,
            Some(Variant {
                woman: true,
                bust_lean: 1,
                ..default()
            }),
        )
    };
    let haired = woman(CreatureModel::Human).dressed(Wardrobe::Leotard);
    let cloaked = woman(CreatureModel::Human).dressed(Wardrobe::Paladin);
    let robed = woman(CreatureModel::Mage);
    for (name, look) in [("hair", haired), ("cloak", cloaked), ("robe", robed)] {
        let (mut world, rig, ..) = spawn_one(&look);
        let covered = match name {
            "hair" => rig.hair.is_some(),
            "cloak" => !rig.cloaks.is_empty(),
            _ => rig.robe.is_some(),
        };
        assert!(covered, "the {name} fixture must wear its drape");
        let mut boxes = world.query::<(
            &bevy::render::primitives::Aabb,
            Has<crate::picking::LooseBounds>,
        )>();
        let (mut loose, mut tight) = (0, 0);
        for (bounds, marked) in boxes.iter(&world) {
            let culling = bounds.half_extents.max_element() >= 1.0;
            assert_eq!(
                culling, marked,
                "{name}: a box of {:?} must be loose exactly when it only culls",
                bounds.half_extents
            );
            if marked {
                loose += 1
            } else {
                tight += 1
            }
        }
        assert!(
            loose > 0,
            "{name}: its drape's culling box is marked ({tight} tight boxes)"
        );
    }
}

#[test]
fn shared_head_cache_matches_fresh_components_in_both_outfit_orders() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Nymph,
        CreatureModel::Phantom,
    ] {
        let base = Look::with_choice(
            model,
            17,
            Some(Variant {
                woman: true,
                hair: 2,
                ..default()
            }),
        );
        let plain = base.clone().dressed(Wardrobe::Commoner);
        let banded = base.dressed(Wardrobe::Princess);
        let expected = [&plain, &banded]
            .map(|look| surface_signature(joint_surface_samples_with_drapes(look, J::Head, false)));
        assert!(
            expected[0] != expected[1],
            "{model:?}: the choker must change actual head geometry"
        );
        for order in [[0, 1, 0, 1], [1, 0, 1, 0]] {
            let mut materials = Assets::<StandardMaterial>::default();
            let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
            let mut meshes = Assets::<Mesh>::default();
            let mut world = World::new();
            for index in order {
                let look = [&plain, &banded][index];
                let parent = world.spawn(Transform::default()).id();
                let mut queue = bevy::ecs::world::CommandQueue::default();
                let rig = spawn(
                    &mut Commands::new(&mut queue, &world),
                    &mut library,
                    &mut meshes,
                    parent,
                    look,
                );
                queue.apply(&mut world);
                let actual = surface_signature(joint_surface_samples_in(
                    &world,
                    &rig,
                    &meshes,
                    J::Head,
                    false,
                ));
                assert!(actual == expected[index], "{model:?}: head composition must not depend on cache history, order={order:?}, outfit={index}");
            }
        }
    }
}

#[test]
fn explicit_walking_carriage_retains_contact_across_outfits_and_turns() {
    for carriage in [WalkingCarriage::Grounded, WalkingCarriage::Poised] {
        for (model, wardrobe, woman) in [
            (CreatureModel::Human, Wardrobe::Ranger, true),
            (CreatureModel::Human, Wardrobe::Knight, true),
            (CreatureModel::Human, Wardrobe::SylvanDuelist, true),
            (CreatureModel::Human, Wardrobe::Commoner, false),
            (CreatureModel::Orc, Wardrobe::Ranger, true),
        ] {
            let look = Look::with_choice(model, 21, Some(Variant { woman, ..default() }))
                .dressed(wardrobe);
            let straight = |frame: usize| Vec3::Z * frame as f32 * 0.03;
            let turn = |frame: usize| {
                let angle = frame as f32 * 0.02;
                Vec3::new(1.5 - 1.5 * angle.cos(), 0.0, 1.5 * angle.sin())
            };
            let stop =
                |frame: usize| Vec3::Z * (frame.min(90) + frame.saturating_sub(140)) as f32 * 0.03;
            for (name, path) in [
                ("straight", &straight as &dyn Fn(usize) -> Vec3),
                ("turn", &turn),
                ("stop", &stop),
            ] {
                let result = planted_foot_slide_with(look.clone(), carriage, path);
                assert_eq!(
                    result.len(),
                    2,
                    "{model:?}/{wardrobe:?}: actual supported biped feet"
                );
                for (foot, median, sliding) in result {
                    // Same owners/limits as the existing straight and curved
                    // walk contracts; a turn's settling frames are not straight stance.
                    assert!(
                        median.is_finite()
                            && median < if name == "straight" { 0.001 } else { 0.0025 },
                        "{name}/{carriage:?}/{model:?}/{wardrobe:?}/{foot:?}: median={median}"
                    );
                    if name == "straight" {
                        assert!(
                            sliding < 0.2,
                            "{carriage:?}/{model:?}/{wardrobe:?}/{foot:?}: sliding={sliding}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn ordinary_skirts_clear_the_pelvis_between_waist_and_thighs() {
    use bevy::render::mesh::VertexAttributeValues;
    for wardrobe in [Wardrobe::Courtier, Wardrobe::Tavern, Wardrobe::Juliet] {
        for woman in [false, true] {
            for build in 0..3 {
                for figure in 0..3 {
                    let look = Look::with_choice(
                        CreatureModel::Human,
                        7,
                        Some(Variant {
                            woman,
                            build,
                            figure,
                            ..default()
                        }),
                    )
                    .composed(wardrobe, Equipment::default())
                    .unwrap();
                    let body = anatomy::HumanoidBody::resolve(&look);
                    let pelvis = geometry::pelvis_shape(body);
                    let (world, rig, _, meshes) = spawn_one(&look);
                    let mut witnesses = 0;
                    let mut lowest = f32::INFINITY;
                    for &root in &rig.skirt_layers {
                        let root_at = world.get::<Transform>(root).unwrap().compute_affine();
                        let mut pending = vec![(root, root_at)];
                        while let Some((entity, at)) = pending.pop() {
                            if let Some(mesh) =
                                world.get::<Mesh3d>(entity).and_then(|m| meshes.get(&m.0))
                            {
                                let Some(VertexAttributeValues::Float32x3(vertices)) =
                                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                                else {
                                    panic!("cloth positions")
                                };
                                let indices: Vec<_> =
                                    mesh.indices().expect("cloth triangles").iter().collect();
                                for tri in indices.as_chunks::<3>().0 {
                                    let points = [tri[0], tri[1], tri[2]].map(|i| {
                                        at.transform_point3(Vec3::from(vertices[i]))
                                            / anatomy::HIP_SCALE
                                    });
                                    // Witness actual emitted triangle interiors, not just fitted rings.
                                    for point in
                                        [points[0], (points[0] + points[1] + points[2]) / 3.0]
                                    {
                                        if (-0.09..0.035).contains(&point.y) {
                                            witnesses += 1;
                                            lowest = lowest.min(pelvis.distance(point));
                                        }
                                    }
                                }
                            }
                            if let Some(children) = world.get::<Children>(entity) {
                                for child in children.iter() {
                                    let child_at = world
                                        .get::<Transform>(child)
                                        .map_or(Affine3A::IDENTITY, |t| t.compute_affine());
                                    pending.push((child, at * child_at));
                                }
                            }
                        }
                    }
                    assert!(witnesses > 100, "exercise actual cloth below waist");
                    assert!(
                    lowest > 0.0,
                    "{wardrobe:?} woman {woman} build {build} figure {figure}: cloth penetrates pelvis by {}",
                    -lowest
                );
                }
            }
        }
    }
}

#[test]
fn study_headwear_uses_centaur_head_frame_and_restores_native_geometry() {
    let base = Look::of(CreatureModel::Centaur, 17);
    let mut equipment = Equipment::default();
    equipment.select("crested-helmet").unwrap();
    let worn = base.clone().equipped(equipment);
    let signature = |look: &Look, joint| {
        surface_signature(joint_surface_samples_with_drapes(look, joint, false))
    };
    assert_ne!(signature(&base, J::Head), signature(&worn, J::Head));
    assert_eq!(signature(&base, J::Body), signature(&worn, J::Body));
    let restored = worn.equipped(Equipment::default());
    assert_eq!(signature(&base, J::Head), signature(&restored, J::Head));
}

#[test]
fn earring_head_cache_matches_fresh_meshes_through_swaps_and_removal() {
    for model in [CreatureModel::Medusa, CreatureModel::Nymph] {
        let base = Look::with_choice(
            model,
            17,
            Some(Variant {
                woman: true,
                hair: 2,
                ..default()
            }),
        );
        let mut bare = base.clone();
        let mut outfit = outfit::Outfit::native(&bare);
        outfit.trimmings.earrings = None;
        bare.outfit = Some(outfit);
        let selected = base.clone().dressed(Wardrobe::RoyalPrincess);
        let mut equipment = outfit::Equipment::default();
        equipment.select("helm").unwrap();
        let helmed = base.clone().equipped(equipment);
        let restored = helmed.clone().equipped(outfit::Equipment::default());
        let looks = [&base, &selected, &bare, &helmed, &restored];
        let expected = looks
            .map(|look| surface_signature(joint_surface_samples_with_drapes(look, J::Head, false)));
        assert!(
            expected[0] != expected[2],
            "native removal must change head geometry"
        );
        assert!(
            expected[0] == expected[4],
            "helm removal must restore native head"
        );
        for order in [[0, 1, 2, 3, 4], [4, 3, 2, 1, 0]] {
            let mut materials = Assets::<StandardMaterial>::default();
            let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
            let mut meshes = Assets::<Mesh>::default();
            let mut world = World::new();
            for index in order {
                let look = looks[index];
                let parent = world.spawn(Transform::default()).id();
                let mut queue = bevy::ecs::world::CommandQueue::default();
                let rig = spawn(
                    &mut Commands::new(&mut queue, &world),
                    &mut library,
                    &mut meshes,
                    parent,
                    look,
                );
                queue.apply(&mut world);
                let actual = surface_signature(joint_surface_samples_in(
                    &world,
                    &rig,
                    &meshes,
                    J::Head,
                    false,
                ));
                assert!(actual == expected[index], "{model:?}: head composition must not depend on cache history, order={order:?}, outfit={index}");
            }
        }
    }
}

#[test]
fn leaf_crown_head_cache_restores_after_headwear_and_hood_swaps() {
    for model in [CreatureModel::Nymph] {
        let base = Look::with_choice(
            model,
            17,
            Some(Variant {
                woman: true,
                hair: 2,
                ..default()
            }),
        );
        let mut bare = base.clone();
        let mut outfit = outfit::Outfit::native(&bare);
        outfit.head = outfit::Headwear::Bare;
        bare.outfit = Some(outfit);
        let selected = base.clone().dressed(Wardrobe::RoyalPrincess);
        let mut equipment = outfit::Equipment::default();
        equipment.select("helm").unwrap();
        let helmed = base.clone().equipped(equipment);
        let restored = helmed.clone().equipped(outfit::Equipment::default());
        let mut cloak = outfit::Equipment::default();
        cloak.select("hooded-cloak").unwrap();
        let hooded = base.clone().equipped(cloak);
        assert!(matches!(
            outfit::Outfit::worn(&hooded).head,
            outfit::Headwear::Hood { .. }
        ));
        let unhooded = hooded.clone().equipped(outfit::Equipment::default());
        let looks = [
            &base, &selected, &bare, &helmed, &restored, &hooded, &unhooded,
        ];
        let expected = looks
            .map(|look| surface_signature(joint_surface_samples_with_drapes(look, J::Head, false)));
        assert!(
            expected[0] != expected[2],
            "native removal must change head geometry"
        );
        assert!(
            expected[0] == expected[4],
            "helm removal must restore native head"
        );
        assert!(
            expected[0] == expected[6],
            "cloak removal restores leaf crown"
        );
        for order in [[0, 1, 2, 3, 4, 5, 6], [6, 5, 4, 3, 2, 1, 0]] {
            let mut materials = Assets::<StandardMaterial>::default();
            let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
            let mut meshes = Assets::<Mesh>::default();
            let mut world = World::new();
            for index in order {
                let look = looks[index];
                let parent = world.spawn(Transform::default()).id();
                let mut queue = bevy::ecs::world::CommandQueue::default();
                let rig = spawn(
                    &mut Commands::new(&mut queue, &world),
                    &mut library,
                    &mut meshes,
                    parent,
                    look,
                );
                queue.apply(&mut world);
                let actual = surface_signature(joint_surface_samples_in(
                    &world,
                    &rig,
                    &meshes,
                    J::Head,
                    false,
                ));
                assert!(actual == expected[index], "{model:?}: head composition must not depend on cache history, order={order:?}, outfit={index}");
            }
        }
    }
}

#[test]
fn veiled_loose_hair_cache_matches_fresh_geometry_in_both_dressing_orders() {
    let base = Look::with_choice(
        CreatureModel::Human,
        22,
        Some(Variant {
            woman: true,
            hair: 0,
            ..default()
        }),
    );
    let plain = base.clone().dressed(Wardrobe::Commoner);
    let veiled = base.clone().dressed(Wardrobe::Juliet);
    let mut recolored = veiled.clone();
    let mut recipe = outfit::Outfit::worn(&recolored);
    recipe.head = outfit::Headwear::Veiled {
        cap: Vec3::ONE,
        veil: Vec3::splat(0.4),
    };
    recolored.outfit = Some(recipe);
    let other = base.dressed(Wardrobe::Courtier);
    let looks = [plain, veiled, recolored, other];
    let expected: Vec<_> = looks
        .iter()
        .map(|look| {
            let (_, rig, _, _) = spawn_one(look);
            super::hair_contact::authored_points(rig.hair.as_ref().expect("long fall"))
        })
        .collect();
    assert_ne!(
        expected[0], expected[1],
        "fixture must change actual hanging roots"
    );
    assert_eq!(expected[0], expected[3]);
    assert_eq!(expected[1], expected[2]);
    for order in [[0, 1, 2, 3, 0], [1, 0, 3, 2, 1]] {
        let mut materials = Assets::<StandardMaterial>::default();
        let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
        let mut meshes = Assets::<Mesh>::default();
        let mut world = World::new();
        let mut cached = [None, None];
        for index in order {
            let parent = world.spawn(Transform::default()).id();
            let mut queue = bevy::ecs::world::CommandQueue::default();
            let rig = spawn(
                &mut Commands::new(&mut queue, &world),
                &mut library,
                &mut meshes,
                parent,
                &looks[index],
            );
            queue.apply(&mut world);
            let hair = rig.hair.as_ref().expect("cap must retain loose hair");
            let authored = super::hair_contact::authored_points(hair);
            assert_eq!(
                authored, expected[index],
                "stale cache for {order:?}/{index}"
            );
            let handles = super::hair_contact::source_meshes(hair);
            let family = usize::from(index == 1 || index == 2);
            if let Some(prior) = &cached[family] {
                assert_eq!(prior, &handles, "same fitted geometry must reuse source");
            }
            cached[family] = Some(handles);
            // Every crown vertex remains pinned after initial body contact.
            let (entity, _) = rig
                .drapes
                .iter()
                .find(|(_, kind)| *kind == Drape::Hair)
                .unwrap();
            let points = points_under(&world, &meshes, *entity);
            let (attached, _) = super::hair_contact::attachment_witnesses(hair);
            assert_eq!(points.len(), authored.len());
            for ((point, rest), attached) in points.iter().zip(&authored).zip(attached) {
                if attached {
                    assert!(point.distance(*rest) < 1e-5);
                }
            }
        }
        assert_ne!(cached[0], cached[1], "bare and capped roots cannot alias");
    }
}

#[test]
fn tricorn_loose_hair_cache_is_separate_from_bare_and_veiled_in_both_orders() {
    let base = Look::with_choice(
        CreatureModel::Human,
        22,
        Some(Variant {
            woman: true,
            hair: 0,
            ..default()
        }),
    );
    let plain = base.clone().dressed(Wardrobe::Commoner);
    let veiled = base.clone().dressed(Wardrobe::Juliet);
    let mut recolored = veiled.clone();
    let mut recipe = outfit::Outfit::worn(&recolored);
    recipe.head = outfit::Headwear::Veiled {
        cap: Vec3::ONE,
        veil: Vec3::splat(0.4),
    };
    recolored.outfit = Some(recipe);
    let other = base.clone().dressed(Wardrobe::Courtier);
    let pirate = base.dressed(Wardrobe::Pirate);
    let looks = [plain, veiled, recolored, other, pirate];
    let expected: Vec<_> = looks
        .iter()
        .map(|look| {
            let (_, rig, _, _) = spawn_one(look);
            super::hair_contact::authored_points(rig.hair.as_ref().expect("long fall"))
        })
        .collect();
    assert_ne!(
        expected[0], expected[1],
        "fixture must change actual hanging roots"
    );
    assert_eq!(expected[0], expected[3]);
    assert_eq!(expected[1], expected[2]);
    assert_ne!(expected[0], expected[4]);
    assert_ne!(expected[1], expected[4]);
    for order in [[0, 1, 4, 2, 3, 4, 0], [4, 1, 0, 4, 3, 2, 1]] {
        let mut materials = Assets::<StandardMaterial>::default();
        let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
        let mut meshes = Assets::<Mesh>::default();
        let mut world = World::new();
        let mut cached = [None, None, None];
        for index in order {
            let parent = world.spawn(Transform::default()).id();
            let mut queue = bevy::ecs::world::CommandQueue::default();
            let rig = spawn(
                &mut Commands::new(&mut queue, &world),
                &mut library,
                &mut meshes,
                parent,
                &looks[index],
            );
            queue.apply(&mut world);
            let hair = rig.hair.as_ref().expect("cap must retain loose hair");
            let authored = super::hair_contact::authored_points(hair);
            assert_eq!(
                authored, expected[index],
                "stale cache for {order:?}/{index}"
            );
            let handles = super::hair_contact::source_meshes(hair);
            let family = if index == 4 {
                2
            } else {
                usize::from(index == 1 || index == 2)
            };
            if let Some(prior) = &cached[family] {
                assert_eq!(prior, &handles, "same fitted geometry must reuse source");
            }
            cached[family] = Some(handles);
            // Every crown vertex remains pinned after initial body contact.
            let (entity, _) = rig
                .drapes
                .iter()
                .find(|(_, kind)| *kind == Drape::Hair)
                .unwrap();
            let points = points_under(&world, &meshes, *entity);
            let (attached, _) = super::hair_contact::attachment_witnesses(hair);
            assert_eq!(points.len(), authored.len());
            for ((point, rest), attached) in points.iter().zip(&authored).zip(attached) {
                if attached {
                    assert!(point.distance(*rest) < 1e-5);
                }
            }
        }
        assert_ne!(cached[0], cached[1], "bare and capped roots cannot alias");
        assert_ne!(cached[0], cached[2], "bare and tricorn roots cannot alias");
        assert_ne!(
            cached[1], cached[2],
            "veiled and tricorn roots cannot alias"
        );
    }
}

#[test]
fn witch_hat_loose_hair_cache_is_separate_from_bare_and_veiled_in_both_orders() {
    let base = Look::with_choice(
        CreatureModel::Human,
        22,
        Some(Variant {
            woman: true,
            hair: 0,
            ..default()
        }),
    );
    let plain = base.clone().dressed(Wardrobe::Commoner);
    let veiled = base.clone().dressed(Wardrobe::Juliet);
    let mut recolored = veiled.clone();
    let mut recipe = outfit::Outfit::worn(&recolored);
    recipe.head = outfit::Headwear::Veiled {
        cap: Vec3::ONE,
        veil: Vec3::splat(0.4),
    };
    recolored.outfit = Some(recipe);
    let other = base.clone().dressed(Wardrobe::Courtier);
    let pirate = base.clone().dressed(Wardrobe::Pirate);
    let witch = base.dressed(Wardrobe::Witch);
    let looks = [plain, veiled, recolored, other, witch, pirate];
    let expected: Vec<_> = looks
        .iter()
        .map(|look| {
            let (_, rig, _, _) = spawn_one(look);
            super::hair_contact::authored_points(rig.hair.as_ref().expect("long fall"))
        })
        .collect();
    assert_ne!(
        expected[0], expected[1],
        "fixture must change actual hanging roots"
    );
    assert_eq!(expected[0], expected[3]);
    assert_eq!(expected[1], expected[2]);
    assert_ne!(expected[0], expected[4]);
    assert_ne!(expected[1], expected[4]);
    assert_ne!(expected[4], expected[5]);
    for order in [[0, 1, 4, 5, 2, 3, 4, 5, 0], [5, 4, 1, 0, 5, 4, 3, 2, 1]] {
        let mut materials = Assets::<StandardMaterial>::default();
        let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
        let mut meshes = Assets::<Mesh>::default();
        let mut world = World::new();
        let mut cached = [None, None, None, None];
        for index in order {
            let parent = world.spawn(Transform::default()).id();
            let mut queue = bevy::ecs::world::CommandQueue::default();
            let rig = spawn(
                &mut Commands::new(&mut queue, &world),
                &mut library,
                &mut meshes,
                parent,
                &looks[index],
            );
            queue.apply(&mut world);
            let hair = rig.hair.as_ref().expect("cap must retain loose hair");
            let authored = super::hair_contact::authored_points(hair);
            assert_eq!(
                authored, expected[index],
                "stale cache for {order:?}/{index}"
            );
            let handles = super::hair_contact::source_meshes(hair);
            let family = if index >= 4 {
                index - 2
            } else {
                usize::from(index == 1 || index == 2)
            };
            if let Some(prior) = &cached[family] {
                assert_eq!(prior, &handles, "same fitted geometry must reuse source");
            }
            cached[family] = Some(handles);
            // Every crown vertex remains pinned after initial body contact.
            let (entity, _) = rig
                .drapes
                .iter()
                .find(|(_, kind)| *kind == Drape::Hair)
                .unwrap();
            let points = points_under(&world, &meshes, *entity);
            let (attached, _) = super::hair_contact::attachment_witnesses(hair);
            assert_eq!(points.len(), authored.len());
            for ((point, rest), attached) in points.iter().zip(&authored).zip(attached) {
                if attached {
                    assert!(point.distance(*rest) < 1e-5);
                }
            }
        }
        assert_ne!(cached[2], cached[3], "witch and tricorn roots cannot alias");
        assert_ne!(cached[0], cached[1], "bare and capped roots cannot alias");
        assert_ne!(
            cached[0], cached[2],
            "bare and witch_hat roots cannot alias"
        );
        assert_ne!(
            cached[1], cached[2],
            "veiled and witch_hat roots cannot alias"
        );
    }
}

#[test]
fn yeti_hand_claws_are_attached_to_the_emitted_hand() {
    use bevy::math::Affine3A;
    use bevy::render::mesh::VertexAttributeValues;

    // Work on emitted triangles, including nested/scaled hand mesh children.
    // Do not use the production surface query or its root placement recipe.
    fn collect(
        world: &World,
        meshes: &Assets<Mesh>,
        entity: Entity,
        frame: Affine3A,
        bone: Vec3,
        claws: &mut Vec<[Vec3; 3]>,
        hand: &mut Vec<[Vec3; 3]>,
    ) {
        let frame = frame
            * world
                .get::<Transform>(entity)
                .map(Transform::compute_affine)
                .unwrap_or_default();
        if let Some(mesh) = world.get::<Mesh3d>(entity).and_then(|m| meshes.get(&m.0)) {
            if let (
                Some(VertexAttributeValues::Float32x3(points)),
                Some(VertexAttributeValues::Float32x4(colors)),
            ) = (
                mesh.attribute(Mesh::ATTRIBUTE_POSITION),
                mesh.attribute(Mesh::ATTRIBUTE_COLOR),
            ) {
                let indices: Vec<usize> = mesh
                    .indices()
                    .map(|indices| indices.iter().collect())
                    .unwrap_or_else(|| (0..points.len()).collect());
                for face in indices.as_chunks::<3>().0 {
                    let is_bone = |i: usize| {
                        let c = colors[i];
                        Vec3::new(c[0], c[1], c[2]).normalize().distance(bone) < 0.001
                    };
                    let triangle = [face[0], face[1], face[2]]
                        .map(|i| frame.transform_point3(Vec3::from(points[i])));
                    // Ignore zero-area cone tip triangles.
                    if (triangle[1] - triangle[0])
                        .cross(triangle[2] - triangle[0])
                        .length_squared()
                        < 1e-16
                    {
                        continue;
                    }
                    if face.iter().all(|i| is_bone(*i)) {
                        claws.push(triangle);
                    } else {
                        assert!(face.iter().all(|i| !is_bone(*i)), "mixed hand/claw face");
                        hand.push(triangle);
                    }
                }
            }
        }
        if let Some(children) = world.get::<Children>(entity) {
            for child in children.iter() {
                collect(world, meshes, child, frame, bone, claws, hand);
            }
        }
    }

    // Segment/triangle intersection; shared endpoints count as contact.
    fn pierces(a: Vec3, b: Vec3, triangle: [Vec3; 3]) -> bool {
        let direction = b - a;
        let e1 = triangle[1] - triangle[0];
        let e2 = triangle[2] - triangle[0];
        let p = direction.cross(e2);
        let det = e1.dot(p);
        if det.abs() < 1e-12 {
            return false;
        }
        let s = a - triangle[0];
        let u = s.dot(p) / det;
        let q = s.cross(e1);
        let v = direction.dot(q) / det;
        let t = e2.dot(q) / det;
        let epsilon = 1e-5;
        u >= -epsilon
            && v >= -epsilon
            && u + v <= 1.0 + epsilon
            && t >= -epsilon
            && t <= 1.0 + epsilon
    }
    fn intersects(a: [Vec3; 3], b: [Vec3; 3]) -> bool {
        (0..3).any(|i| pierces(a[i], a[(i + 1) % 3], b) || pierces(b[i], b[(i + 1) % 3], a))
    }

    let linear = Color::srgb(BONE.x, BONE.y, BONE.z).to_linear();
    let bone = Vec3::new(linear.red, linear.green, linear.blue).normalize();
    for woman in [false, true] {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Yeti,
                42,
                Some(Variant {
                    woman,
                    build,
                    ..default()
                }),
            );
            let (world, rig, _, meshes) = spawn_one(&look);
            // Both sexes and all builds retain the native Yeti closed-hand recipe.
            for joint in [J::HandL, J::HandR] {
                let mut claws = Vec::new();
                let mut hand = Vec::new();
                collect(
                    &world,
                    &meshes,
                    rig.joint(joint).unwrap(),
                    Affine3A::IDENTITY,
                    bone,
                    &mut claws,
                    &mut hand,
                );
                assert!(!hand.is_empty(), "positive emitted hand witness");
                assert!(!claws.is_empty(), "positive emitted claw witness");

                // Weld duplicate face vertices geometrically, not by mesh index:
                // flat-shaded prism faces duplicate their corner vertices.
                let mut unseen = vec![true; claws.len()];
                let mut components = Vec::new();
                while let Some(start) = unseen.iter().position(|v| *v) {
                    unseen[start] = false;
                    let mut component = vec![start];
                    let mut cursor = 0;
                    while cursor < component.len() {
                        let current = claws[component[cursor]];
                        for next in 0..claws.len() {
                            if unseen[next]
                                && current.iter().any(|a| {
                                    claws[next].iter().any(|b| a.distance_squared(*b) < 1e-12)
                                })
                            {
                                unseen[next] = false;
                                component.push(next);
                            }
                        }
                        cursor += 1;
                    }
                    components.push(component);
                }
                assert_eq!(
                    components.len(),
                    3,
                    "three separate claws: {woman}/{build}/{joint:?}"
                );
                for (index, component) in components.iter().enumerate() {
                    assert!(component.iter().any(|i| hand.iter().any(|surface|
                        intersects(claws[*i], *surface))),
                        "claw {index} must intersect the actual hand surface: {woman}/{build}/{joint:?}");
                }
            }
        }
    }
}
#[test]
fn low_shoes_do_not_create_boot_shafts_on_fitted_legs() {
    let shoe_color = Vec3::new(0.03, 0.08, 0.7);
    let tag = |color: Vec3| {
        let c = Color::srgb(color.x, color.y, color.z).to_linear();
        Vec3::new(c.red, c.green, c.blue).normalize()
    };
    let shoe_tag = tag(shoe_color);
    for woman in [false, true] {
        for build in [0, 2] {
            let variant = Variant {
                woman,
                build,
                ..default()
            };
            let donor = Look::with_choice(CreatureModel::Troll, 42, Some(variant));
            let recipe = outfit::Outfit::native(&donor);
            for model in [
                CreatureModel::Troll,
                CreatureModel::Human,
                CreatureModel::Orc,
            ] {
                let mut wearer = Look::with_choice(model, 42, Some(variant));
                // Distinct recipient pigment makes accidental donor skin a failure.
                wearer.skin = Vec3::new(0.67, 0.31, 0.12);
                let skin_tag = tag(wearer.skin);
                for sandal in [false, true] {
                    let mut low_shoes = recipe;
                    low_shoes.feet = outfit::Feet::Shoes(outfit::Shoes {
                        color: shoe_color,
                        heel: woman,
                        sandal,
                        straps: 0.0,
                    });
                    wearer.outfit = Some(low_shoes);
                    for (shin, foot) in [(J::ShinL, J::FootL), (J::ShinR, J::FootR)] {
                        let calf = joint_surface_samples(&wearer, shin);
                        assert!(calf.len() > 100, "positive emitted calf witness");
                        let is_shoe = |c: Vec3| c.normalize().distance(shoe_tag) < 0.001;
                        let is_skin = |c: Vec3| c.normalize().distance(skin_tag) < 0.001;
                        assert!(calf.iter().any(|(p, c)| p.y > -0.30 && is_skin(*c)),
                            "fitted calf must retain wearer skin: {model:?}/{woman}/{build}/{shin:?}");
                        // A zero-rise shoe has a narrow ankle band above its upper,
                        // not a boot shaft reaching up to -0.24.
                        let upper_shoe: Vec<_> = calf
                            .iter()
                            .filter(|(p, c)| p.y > -0.30 && is_shoe(*c))
                            .collect();
                        assert!(upper_shoe.is_empty(),
                            "low shoe emitted {} boot-shaft vertices above ankle: {model:?}/{woman}/{build}/{sandal}/{shin:?}", upper_shoe.len());
                        assert!(
                            calf.iter().any(|(p, c)| p.y < -0.30 && is_shoe(*c)),
                            "low shoe must retain its actual ankle strap"
                        );
                        let shoe = joint_surface_samples(&wearer, foot);
                        assert!(
                            shoe.len() > 100
                                && shoe.iter().any(|(_, c)| c.normalize().distance(tag(shoe_color * 0.6)) < 0.001),
                            "shoe sole must remain: {model:?}/{woman}/{build}/{sandal}/{foot:?}, vertices {}", shoe.len()
                        );
                        if sandal {
                            assert!(
                                shoe.iter().any(|(_, c)| is_skin(*c)),
                                "sandal upper must expose the recipient skin"
                            );
                        }
                    }
                    // Actual native boots still own shafts, independent of leg
                    // pigment. Equipping/removing boots restores the same shoe.
                    let original = surface_signature(joint_surface_samples(&wearer, J::ShinL));
                    let mut booted = wearer.clone();
                    booted.outfit.as_mut().unwrap().feet = outfit::Feet::Boots(outfit::Boots {
                        color: outfit::Pigment::Fixed(shoe_color),
                        tall: false,
                        heel: woman,
                        buckles: false,
                        cuffed: false,
                    });
                    assert!(
                        joint_surface_samples(&booted, J::ShinL)
                            .iter()
                            .any(|(p, c)| p.y > -0.30 && c.normalize().distance(shoe_tag) < 0.001),
                        "positive native boot shaft witness"
                    );
                    for name in ["boots", "combat-boots"] {
                        let mut gear = Equipment::default();
                        gear.select(name).unwrap();
                        let equipped = wearer.clone().equipped(gear);
                        let covered = joint_surface_samples(&equipped, J::ShinL);
                        assert!(covered.len() > 100);
                        assert!(
                            covered
                                .iter()
                                .all(|(_, c)| c.normalize().distance(shoe_tag) >= 0.001),
                            "equipment boots replace the shoe and its ankle strap"
                        );
                        let restored = equipped.equipped(Equipment::default());
                        assert_eq!(
                            surface_signature(joint_surface_samples(&restored, J::ShinL)),
                            original,
                            "removing equipment boots must restore low shoes"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn armored_wardrobe_keeps_low_shoes_separate_from_leg_armor() {
    for wardrobe in [Wardrobe::Witch, Wardrobe::Princess] {
        let base = Look::with_choice(
            CreatureModel::Human,
            42,
            Some(Variant {
                woman: true,
                ..default()
            }),
        )
        .dressed(wardrobe);
        let mut gear = Equipment::default();
        gear.select("plate-mail").unwrap();
        let dressed = base.equipped(gear);
        let resolved = outfit::Composition::resolve(&dressed).outfit;
        assert!(matches!(resolved.legs, outfit::Legs::Plate));
        let outfit::Feet::Shoes(shoes) = resolved.feet else {
            panic!("positive low shoe recipe");
        };
        assert_eq!(shoes.straps, 0.0);
        let color = Color::srgb(shoes.color.x, shoes.color.y, shoes.color.z).to_linear();
        let tag = Vec3::new(color.red, color.green, color.blue).normalize();
        for joint in [J::ShinL, J::ShinR] {
            let calf = joint_surface_samples(&dressed, joint);
            let shoe: Vec<_> = calf
                .iter()
                .filter(|(_, c)| c.normalize().distance(tag) < 0.001)
                .collect();
            assert!(!shoe.is_empty(), "ankle strap remains present");
            assert!(
                shoe.iter().all(|(p, _)| p.y < -0.30),
                "low shoe must not become a boot under plate mail: {wardrobe:?}/{joint:?}"
            );
        }
    }
}

#[test]
fn sandal_bands_follow_actual_calf_and_leg_armor() {
    use bevy::render::mesh::VertexAttributeValues;
    let color = Vec3::new(0.03, 0.08, 0.7);
    let c = Color::srgb(color.x, color.y, color.z).to_linear();
    let tag = Vec3::new(c.red, c.green, c.blue).normalize();
    for model in [CreatureModel::Human, CreatureModel::Orc] {
        for woman in [false, true] {
            for build in [0, 2] {
                for plate in [false, true] {
                    let mut look = Look::with_choice(
                        model,
                        42,
                        Some(Variant {
                            woman,
                            build,
                            ..default()
                        }),
                    )
                    .dressed(Wardrobe::Goddess);
                    let recipe = look.outfit.as_mut().unwrap();
                    recipe.feet = outfit::Feet::Shoes(outfit::Shoes {
                        color,
                        heel: woman,
                        sandal: true,
                        straps: 1.0,
                    });
                    if plate {
                        recipe.legs = outfit::Legs::Plate;
                    }
                    let (world, rig, _, meshes) = spawn_one(&look);
                    for joint in [J::ShinL, J::ShinR] {
                        let mut bands = Vec::new();
                        let mut support = Vec::new();
                        for child in world
                            .get::<Children>(rig.joint(joint).unwrap())
                            .unwrap()
                            .iter()
                        {
                            let Some(mesh) =
                                world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0))
                            else {
                                continue;
                            };
                            let Some(VertexAttributeValues::Float32x3(pos)) =
                                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                            else {
                                panic!("positions");
                            };
                            let Some(VertexAttributeValues::Float32x4(colors)) =
                                mesh.attribute(Mesh::ATTRIBUTE_COLOR)
                            else {
                                panic!("colors");
                            };
                            let frame = world.get::<Transform>(child).unwrap().compute_affine();
                            let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                            for ids in indices.as_chunks::<3>().0 {
                                let tri = ids.map(|i| frame.transform_point3(Vec3::from(pos[i])));
                                let is_band = ids.iter().all(|i| {
                                    Vec3::new(colors[*i][0], colors[*i][1], colors[*i][2])
                                        .normalize()
                                        .distance(tag)
                                        < 0.001
                                });
                                if is_band {
                                    bands.push(tri);
                                } else {
                                    support.push(tri);
                                }
                            }
                        }
                        assert!(
                            bands.len() >= 300 && support.len() > 100,
                            "positive complete geometry"
                        );
                        // Independently intersect the emitted supporting mesh, including
                        // the actual calf contour and any retained leg-armor surfaces.
                        let radius = |at: Vec3| -> Option<f32> {
                            let origin = Vec3::Y * at.y;
                            let ray = at.xz().normalize();
                            let ray = Vec3::new(ray.x, 0.0, ray.y);
                            support
                                .iter()
                                .filter_map(|tri| {
                                    let a = tri[0];
                                    let e1 = tri[1] - a;
                                    let e2 = tri[2] - a;
                                    let h = ray.cross(e2);
                                    let det = e1.dot(h);
                                    if det.abs() < 1e-9 {
                                        return None;
                                    }
                                    let s = origin - a;
                                    let u = s.dot(h) / det;
                                    let q = s.cross(e1);
                                    let v = ray.dot(q) / det;
                                    let t = e2.dot(q) / det;
                                    (u >= -1e-5 && v >= -1e-5 && u + v <= 1.00001 && t >= 0.0)
                                        .then_some(t)
                                })
                                .reduce(f32::max)
                        };
                        let mut intervals = Vec::new();
                        let mut minimum = f32::INFINITY;
                        let mut maximum = f32::NEG_INFINITY;
                        for tri in bands {
                            let center = (tri[0] + tri[1] + tri[2]) / 3.0;
                            intervals.push((
                                tri.iter().map(|p| p.y).fold(f32::INFINITY, f32::min),
                                tri.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max),
                            ));
                            for at in [
                                tri[0],
                                tri[1],
                                tri[2],
                                center,
                                (tri[0] + tri[1]) * 0.5,
                                (tri[1] + tri[2]) * 0.5,
                                (tri[2] + tri[0]) * 0.5,
                            ] {
                                assert!(
                                    at.y > -0.4,
                                    "strap stays above the separately articulated shoe"
                                );
                                let r=radius(at).unwrap_or_else(|| panic!("band has no calf support at {at:?}: {model:?}/{woman}/{build}/{plate}/{joint:?}"));
                                let gap = at.xz().length() - r;
                                minimum = minimum.min(gap);
                                maximum = maximum.max(gap);
                            }
                        }
                        intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
                        let mut levels: Vec<(f32, f32)> = Vec::new();
                        for (low, high) in intervals {
                            if let Some(last) =
                                levels.last_mut().filter(|last| low <= last.1 + 1e-5)
                            {
                                last.1 = last.1.max(high);
                            } else {
                                levels.push((low, high));
                            }
                        }
                        assert_eq!(levels.len(), 7, "seven independent strap stations");
                        let foot = if joint == J::ShinL {
                            J::FootL
                        } else {
                            J::FootR
                        };
                        let shoe_top = joint_surface_samples(&look, foot)
                            .into_iter()
                            .map(|(p, _)| p.y)
                            .fold(f32::NEG_INFINITY, f32::max);
                        assert!(
                            levels[0].0 > -0.4 + shoe_top + 0.002,
                            "lowest band clears actual shoe upper"
                        );
                        assert!(minimum >= -0.0005 && maximum <= 0.025,
                            "band must clear and follow actual support, gap {minimum}..{maximum}: {model:?}/{woman}/{build}/{plate}/{joint:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn bare_feet_do_not_emit_boot_shafts_on_fitted_legs() {
    use bevy::render::mesh::VertexAttributeValues;
    let foot_color = Vec3::new(0.03, 0.08, 0.7);
    let linear = Color::srgb(foot_color.x, foot_color.y, foot_color.z).to_linear();
    let tag = Vec3::new(linear.red, linear.green, linear.blue).normalize();
    for model in [CreatureModel::Nymph, CreatureModel::Troll] {
        for woman in [false, true] {
            for build in [0, 2] {
                let mut look = Look::with_choice(
                    model,
                    42,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                );
                let mut recipe = outfit::Outfit::native(&look);
                recipe.feet = outfit::Feet::Bare(outfit::Pigment::Fixed(foot_color));
                look.outfit = Some(recipe);
                let (world, rig, _, meshes) = spawn_one(&look);
                for (shin, foot) in [(J::ShinL, J::FootL), (J::ShinR, J::FootR)] {
                    let calf = joint_surface_samples_in(&world, &rig, &meshes, shin, false);
                    let toes = joint_surface_samples_in(&world, &rig, &meshes, foot, false);
                    assert!(
                        calf.len() > 100 && toes.len() > 100,
                        "positive calf and foot"
                    );
                    assert!(
                        toes.iter()
                            .any(|(_, c)| c.normalize().distance(tag) < 0.001),
                        "bare foot retains its authored pigment"
                    );
                    let leather = LEATHER * 0.8;
                    let c = Color::srgb(leather.x, leather.y, leather.z).to_linear();
                    let leather_tag = Vec3::new(c.red, c.green, c.blue).normalize();
                    let mut support = crate::props::Builder::default();
                    let mut bindings = Vec::new();
                    for child in world
                        .get::<Children>(rig.joint(shin).unwrap())
                        .unwrap()
                        .iter()
                    {
                        let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0))
                        else {
                            continue;
                        };
                        let Some(VertexAttributeValues::Float32x3(pos)) =
                            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                        else {
                            panic!("positions");
                        };
                        let Some(VertexAttributeValues::Float32x4(colors)) =
                            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
                        else {
                            panic!("colors");
                        };
                        let frame = world.get::<Transform>(child).unwrap().compute_affine();
                        let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                        for ids in indices.as_chunks::<3>().0 {
                            let tri = ids.map(|i| frame.transform_point3(Vec3::from(pos[i])));
                            if ids.iter().all(|i| {
                                Vec3::new(colors[*i][0], colors[*i][1], colors[*i][2])
                                    .normalize()
                                    .distance(leather_tag)
                                    < 0.001
                            }) {
                                bindings.push(tri);
                            } else {
                                support.tri(tri, Vec3::ONE);
                            }
                        }
                    }
                    assert!(bindings.len() >= 200, "two complete binding meshes");
                    for height in [-0.3_f32, -0.36] {
                        assert!(
                            bindings
                                .iter()
                                .any(|t| t.iter().all(|v| (v.y - height).abs() < 0.011)),
                            "both authored stations retained"
                        );
                    }
                    for tri in bindings {
                        for at in [
                            tri[0],
                            tri[1],
                            tri[2],
                            (tri[0] + tri[1]) * 0.5,
                            (tri[1] + tri[2]) * 0.5,
                            (tri[2] + tri[0]) * 0.5,
                            (tri[0] + tri[1] + tri[2]) / 3.0,
                        ] {
                            let origin = Vec3::Y * at.y;
                            let ray = (at - origin).normalize();
                            let hit = support
                                .outer_surface(origin, ray)
                                .expect("actual emitted calf supports binding");
                            let gap = (at - hit).dot(ray);
                            assert!((-0.0005..=0.04).contains(&gap), "binding fits emitted calf: gap {gap}, {model:?}/{woman}/{build}/{shin:?}");
                        }
                    }
                    assert!(!calf.iter().any(|(_, c)| c.normalize().distance(tag) < 0.001),
                        "bare foot pigment must not create a boot shaft: {model:?}/{woman}/{build}/{shin:?}");
                }
            }
        }
    }
}
