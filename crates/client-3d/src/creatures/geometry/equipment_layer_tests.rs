//! Instance-owned equipment panels must survive extraction and follow drape poses.
use super::*;
use bevy::asset::RenderAssetUsages;
use bevy::render::mesh::VertexAttributeValues;

fn robe_and_apron() -> Look {
    let mut equipment = Equipment::default();
    equipment.select("apron").unwrap();
    Look::of(CreatureModel::Mage, 42).equipped(equipment)
}

fn spawn_again(
    world: &mut World,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    look: &Look,
) -> Rig {
    let parent = world.spawn(Transform::default()).id();
    let mut queue = bevy::ecs::world::CommandQueue::default();
    let rig = spawn(
        &mut Commands::new(&mut queue, world),
        library,
        meshes,
        parent,
        look,
    );
    queue.apply(world);
    rig
}

fn panel(world: &World, rig: &Rig) -> (Entity, Vec<Handle<Mesh>>) {
    // The equipment panel is registered before retained costume skirts.
    let entity = *rig.skirt_layers.first().expect("apron drape root");
    assert!(rig.drapes.iter().any(|(e, kind)| {
        *e == entity && matches!(kind, Drape::Skirt(length) if (*length - 0.55).abs() < 1e-6)
    }));
    let meshes: Vec<_> = world
        .get::<Children>(entity)
        .expect("panel mesh children")
        .iter()
        .filter_map(|child| world.get::<Mesh3d>(child).map(|mesh| mesh.0.clone()))
        .collect();
    assert!(!meshes.is_empty(), "exercise actual rendered panel meshes");
    (entity, meshes)
}

fn positions(meshes: &Assets<Mesh>, handle: &Handle<Mesh>) -> Vec<[f32; 3]> {
    let mesh = meshes.get(handle).expect("CPU-retained panel mesh");
    let Some(VertexAttributeValues::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("panel positions");
    };
    assert!(!points.is_empty());
    points.clone()
}

fn assert_panel_bounds(world: &World, entity: Entity, meshes: &Assets<Mesh>) {
    for child in world
        .get::<Children>(entity)
        .expect("panel mesh children")
        .iter()
    {
        let Some(mesh) = world.get::<Mesh3d>(child) else {
            continue;
        };
        let bounds = world
            .get::<bevy::render::primitives::Aabb>(child)
            .expect("mutable overlay needs explicit conservative culling bounds");
        assert!(
            world.get::<crate::picking::LooseBounds>(child).is_some(),
            "conservative cloth box must not become a false picking target"
        );
        let center = Vec3::from(bounds.center);
        let half = Vec3::from(bounds.half_extents);
        for point in positions(meshes, &mesh.0) {
            let delta = (Vec3::from_array(point) - center).abs();
            assert!(
                delta.cmple(half).all(),
                "posed panel {point:?} leaves culling box {bounds:?}"
            );
        }
    }
}

#[test]
fn warmed_apron_spawn_keeps_cpu_meshes_and_private_instance_ownership() {
    let look = robe_and_apron();
    let (mut world, first, mut library, mut meshes) =
        crate::creatures::rogue_tests::spawn_one(&look);
    assert!(
        first.robe.is_some(),
        "fixture must exercise mutable robe overlays"
    );
    let (_, first_handles) = panel(&world, &first);
    let before: Vec<_> = first_handles
        .iter()
        .map(|h| positions(&meshes, h))
        .collect();
    for handle in &first_handles {
        assert!(meshes
            .get(handle)
            .unwrap()
            .asset_usage
            .contains(RenderAssetUsages::MAIN_WORLD));
    }

    // Model the asset-lifetime boundary of extraction without a GPU: only
    // RENDER_WORLD-only meshes leave Assets<Mesh>. Keep the appearance cache
    // and its handles warm so an invalid source read cannot hide in a rebuild.
    let extracted: Vec<_> = meshes
        .iter()
        .filter(|(_, mesh)| !mesh.asset_usage.contains(RenderAssetUsages::MAIN_WORLD))
        .map(|(id, _)| id)
        .collect();
    assert!(
        !extracted.is_empty(),
        "exercise extraction of ordinary cached meshes"
    );
    for id in extracted {
        meshes.remove(id);
    }
    let mut second = spawn_again(&mut world, &mut library, &mut meshes, &look);
    let (second_entity, second_handles) = panel(&world, &second);
    assert_eq!(first_handles.len(), second_handles.len());
    for handle in &second_handles {
        assert!(
            !first_handles.iter().any(|other| other.id() == handle.id()),
            "mutable panel handles must be private per wearer"
        );
        assert!(meshes
            .get(handle)
            .unwrap()
            .asset_usage
            .contains(RenderAssetUsages::MAIN_WORLD));
    }
    let rest = second.rest;
    let robe = second.robe.as_mut().unwrap();
    let transform = Affine3A::from_translation(Vec3::NEG_Z * 0.15);
    robe.update_layer_transforms(&|entity| (entity == second_entity).then_some(transform));
    super::super::robe::place(
        robe,
        &rest,
        &[Quat::IDENTITY; JOINTS],
        &[Vec3::ZERO; JOINTS],
        &mut meshes,
    );
    for (handle, expected) in first_handles.iter().zip(before) {
        assert_eq!(
            positions(&meshes, handle),
            expected,
            "another wearer must not mutate this panel"
        );
    }
}

#[test]
fn changed_panel_transform_invalidates_contact_with_unchanged_leg_pose() {
    let (world, mut rig, _, mut meshes) =
        crate::creatures::rogue_tests::spawn_one(&robe_and_apron());
    let (entity, handles) = panel(&world, &rig);
    let rest = rig.rest;
    let robe = rig.robe.as_mut().expect("native mage robe");
    let rotations = [Quat::IDENTITY; JOINTS];
    let offsets = [Vec3::ZERO; JOINTS];
    super::super::robe::place(robe, &rest, &rotations, &offsets, &mut meshes);
    let before: Vec<_> = handles.iter().map(|h| positions(&meshes, h)).collect();
    // Move the panel into its retained robe with legs held fixed. Ignoring
    // this drape transform (or failing to invalidate) leaves vertices unchanged.
    let transform = Affine3A::from_translation(Vec3::NEG_Z * 0.15)
        * Affine3A::from_quat(Quat::from_rotation_x(0.20));
    robe.update_layer_transforms(&|target| (target == entity).then_some(transform));
    super::super::robe::place(robe, &rest, &rotations, &offsets, &mut meshes);
    let after: Vec<_> = handles.iter().map(|h| positions(&meshes, h)).collect();
    assert_ne!(
        before, after,
        "drape-only movement must recompute overlay contact"
    );
    assert!(after
        .iter()
        .flatten()
        .flatten()
        .all(|value| value.is_finite()));
    assert_panel_bounds(&world, entity, &meshes);
    super::super::robe::place(robe, &rest, &rotations, &offsets, &mut meshes);
    for (handle, expected) in handles.iter().zip(after) {
        assert_eq!(
            positions(&meshes, handle),
            expected,
            "unchanged pose remains stable"
        );
    }
}
