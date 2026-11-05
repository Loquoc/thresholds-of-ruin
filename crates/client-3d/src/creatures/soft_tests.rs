//! Soft regions answer their frame's motion and settle back to rest.
use super::*;
use crate::props::SoftBone;
use bevy::time::TimeUpdateStrategy;

fn bone(app: &mut App, bone: Entity) -> (Vec3, Vec3) {
    let world = app.world();
    let rest = world.get::<SoftBone>(bone).unwrap().rest;
    (world.get::<Transform>(bone).unwrap().translation, rest)
}

#[test]
fn soft_regions_bounce_with_the_stride_and_settle_at_rest() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .add_systems(Update, soften);
    let rest = Vec3::new(0.08, 0.34, 0.1);
    let frame = app.world_mut().spawn(Transform::default()).id();
    let soft = app
        .world_mut()
        .spawn((
            Transform::from_translation(rest),
            SoftBone {
                rest,
                give: 1.0,
                breath: 0.0,
                offset: Vec3::ZERO,
                velocity: Vec3::ZERO,
                history: None,
            },
            ChildOf(frame),
        ))
        .id();
    // A walking bob: three centimetres at two steps a second.
    let mut furthest: f32 = 0.0;
    for k in 0..90 {
        let t = k as f32 * 0.016;
        let bob = 0.03 * (t * 2.0 * std::f32::consts::TAU).sin();
        app.world_mut()
            .get_mut::<Transform>(frame)
            .unwrap()
            .translation
            .y = bob;
        app.update();
        let (at, rest) = bone(&mut app, soft);
        furthest = furthest.max(at.distance(rest));
    }
    assert!(
        furthest > 0.003,
        "the region answers the stride: {furthest:.4}"
    );
    assert!(
        furthest <= 0.0201,
        "and stays within its reach: {furthest:.4}"
    );
    for _ in 0..150 {
        app.update();
    }
    let (at, rest) = bone(&mut app, soft);
    assert!(
        at.distance(rest) < 2e-4,
        "it settles once the body is still: {:.5}",
        at.distance(rest)
    );
}

/// A woman in an unsupported leotard, jumping on the spot through the real
/// animation: her chest's bones must swing and then settle.
#[test]
fn a_jumping_woman_moves_her_bust_and_plate_holds_it() {
    let travel = |wardrobe: Wardrobe| {
        let look = Look::with_choice(
            CreatureModel::Human,
            5,
            Some(Variant {
                woman: true,
                bust: 3,
                ..default()
            }),
        )
        .dressed(wardrobe);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(16),
            ))
            .init_resource::<Assets<Mesh>>()
            .add_systems(Update, (animate, soften).chain());
        let mut materials = Assets::<StandardMaterial>::default();
        let mut library =
            crate::props::PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
        let root = app
            .world_mut()
            .spawn((Anim::new(5, Vec3::ZERO), Transform::default()))
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
        for _ in 0..30 {
            app.update();
        }
        app.world_mut()
            .get_mut::<Anim>(root)
            .unwrap()
            .act(Move::Jump);
        let mut furthest: f32 = 0.0;
        for _ in 0..90 {
            app.update();
            let mut bones = app.world_mut().query::<&SoftBone>();
            for bone in bones.iter(app.world()) {
                furthest = furthest.max(bone.offset.length() * bone.give);
            }
        }
        let count = app
            .world_mut()
            .query::<&SoftBone>()
            .iter(app.world())
            .count();
        (count, furthest)
    };
    let (bones, swing) = travel(Wardrobe::Leotard);
    assert_eq!(bones, 4, "a soft region each side of her bust and her seat");
    assert!(swing > 0.004, "the jump swings her bust: {swing:.4}");
    let (bones, _) = travel(Wardrobe::Knight);
    assert_eq!(bones, 0, "plate holds her bust and seat still");
}

#[test]
fn a_still_bust_rises_and_falls_gently_with_her_breath() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .add_systems(Update, soften);
    let rest = Vec3::new(0.08, 0.34, 0.1);
    let frame = app.world_mut().spawn(Transform::default()).id();
    let soft = app
        .world_mut()
        .spawn((
            Transform::from_translation(rest),
            SoftBone {
                rest,
                give: 1.0,
                breath: 1.0,
                offset: Vec3::ZERO,
                velocity: Vec3::ZERO,
                history: None,
            },
            ChildOf(frame),
        ))
        .id();
    let (mut low, mut high) = (f32::MAX, f32::MIN);
    for _ in 0..300 {
        app.update();
        let (at, _) = bone(&mut app, soft);
        low = low.min(at.y);
        high = high.max(at.y);
    }
    let swing = high - low;
    assert!(swing > 0.002, "her breath lifts the bust: {swing:.4}");
    assert!(swing < 0.006, "and only gently: {swing:.4}");
}

/// The soft frame is below the actor, not itself the owner of animation state.
fn owned_soft_region(anim: Anim, breath: f32) -> (App, Entity, Entity, Entity) {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .add_systems(Update, soften);
    let (root, frame, soft) = add_owned_soft_region(&mut app, anim, breath);
    (app, root, frame, soft)
}

fn add_owned_soft_region(app: &mut App, anim: Anim, breath: f32) -> (Entity, Entity, Entity) {
    let root = app.world_mut().spawn((anim, Transform::default())).id();
    let frame = app
        .world_mut()
        .spawn((Transform::default(), ChildOf(root)))
        .id();
    let rest = Vec3::new(0.08, 0.34, 0.1);
    let soft = app
        .world_mut()
        .spawn((
            Transform::from_translation(rest),
            SoftBone {
                rest,
                give: 1.0,
                breath,
                offset: Vec3::ZERO,
                velocity: Vec3::ZERO,
                history: None,
            },
            ChildOf(frame),
        ))
        .id();
    (root, frame, soft)
}

#[test]
fn owned_soft_breath_stops_at_death_and_returns_after_revival() {
    let (mut app, root, _, soft) = owned_soft_region(Anim::new(5, Vec3::ZERO), 1.0);
    let (_, _, living_neighbor) = add_owned_soft_region(&mut app, Anim::new(11, Vec3::ZERO), 1.0);
    let mut living_swing = 0.0_f32;
    let mut revived_swing = 0.0_f32;
    let mut dead_excursion = 0.0_f32;
    // Include the live scene's negative death pre-roll, a settled corpse, and
    // revival. Only soften runs: the stationary frame cannot drive a spring.
    for (stage, dead) in [None, Some(-0.35), Some(1.0), None].into_iter().enumerate() {
        app.world_mut().get_mut::<Anim>(root).unwrap().dead = dead;
        let (mut low, mut high) = (f32::MAX, f32::MIN);
        let (mut neighbor_low, mut neighbor_high) = (f32::MAX, f32::MIN);
        for _ in 0..300 {
            app.update();
            let (at, rest) = bone(&mut app, soft);
            low = low.min(at.y);
            high = high.max(at.y);
            let (neighbor_at, _) = bone(&mut app, living_neighbor);
            neighbor_low = neighbor_low.min(neighbor_at.y);
            neighbor_high = neighbor_high.max(neighbor_at.y);
            if dead.is_some() {
                dead_excursion = dead_excursion.max(at.distance(rest));
            }
        }
        assert!(
            neighbor_high - neighbor_low > 0.002,
            "the independent living owner must keep breathing during stage {stage}"
        );
        match stage {
            0 => living_swing = high - low,
            3 => revived_swing = high - low,
            _ => {}
        }
    }
    // Check positive controls before the regression assertion, without deriving
    // an expected value from soften's sinusoid or entity-dependent phase.
    assert!(living_swing > 0.002, "living owner must breathe");
    assert!(revived_swing > 0.002, "revived owner must breathe again");
    assert!(
        dead_excursion < 1e-6,
        "a dead owner's stationary soft region must stay at rest, including negative pre-roll: {dead_excursion}"
    );
}

#[test]
fn dead_soft_regions_still_answer_motion_and_settle_passively() {
    // No autonomous drive: this positive control rejects disabling the entire
    // soft-body solver for corpses instead of just its breathing contribution.
    let (mut app, _, frame, soft) = owned_soft_region(Anim::corpse(5), 0.0);
    for _ in 0..3 {
        app.update();
    }
    let mut furthest = 0.0_f32;
    for k in 0..90 {
        let t = k as f32 * 0.016;
        app.world_mut()
            .get_mut::<Transform>(frame)
            .unwrap()
            .translation
            .y = 0.03 * (t * 2.0 * std::f32::consts::TAU).sin();
        app.update();
        let (at, rest) = bone(&mut app, soft);
        furthest = furthest.max(at.distance(rest));
    }
    assert!(
        furthest > 0.003,
        "a falling body's soft regions must respond"
    );
    assert!(furthest <= 0.0201, "passive response stays bounded");
    for _ in 0..300 {
        app.update();
    }
    for _ in 0..300 {
        app.update();
        let (at, rest) = bone(&mut app, soft);
        assert!(
            at.distance(rest) < 2e-4,
            "passive motion must settle and remain at rest"
        );
    }
}

#[test]
fn composed_leotard_corpse_soft_regions_remain_at_rest_after_settling() {
    let look = Look::with_choice(
        CreatureModel::Human,
        5,
        Some(Variant {
            woman: true,
            bust: 3,
            ..default()
        }),
    )
    .dressed(Wardrobe::Leotard);
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .add_systems(Update, (animate, soften).chain());
    let mut materials = Assets::<StandardMaterial>::default();
    let mut library =
        crate::props::PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
    let root = app
        .world_mut()
        .spawn((Anim::corpse(5), Transform::default()))
        .id();
    let mut queue = bevy::ecs::world::CommandQueue::default();
    let mut meshes = Assets::<Mesh>::default();
    let rig = spawn(
        &mut Commands::new(&mut queue, app.world()),
        &mut library,
        &mut meshes,
        root,
        &look,
    );
    queue.apply(app.world_mut());
    app.insert_resource(meshes);
    app.world_mut().entity_mut(root).insert(rig);
    let bones: Vec<_> = app
        .world_mut()
        .query::<(Entity, &SoftBone)>()
        .iter(app.world())
        .map(|(entity, bone)| (entity, bone.breath))
        .collect();
    assert_eq!(
        bones.len(),
        4,
        "real leotard fixture has bust and seat bones"
    );
    assert_eq!(bones.iter().filter(|(_, breath)| *breath > 0.0).count(), 2);
    for &(entity, _) in &bones {
        let mut ancestor = entity;
        let mut depth = 0;
        while ancestor != root {
            ancestor = app.world().get::<ChildOf>(ancestor).unwrap().parent();
            depth += 1;
            assert!(depth < 20, "soft region must belong to the composed owner");
        }
        assert!(
            depth > 2,
            "exercise real nested ownership, not a direct child"
        );
    }
    // Initial corpse posing may accelerate the frames. Let that passive
    // response settle before checking several seconds without a living drive.
    for _ in 0..300 {
        app.update();
    }
    let mut furthest = 0.0_f32;
    for _ in 0..300 {
        app.update();
        for &(entity, _) in &bones {
            let (at, rest) = bone(&mut app, entity);
            furthest = furthest.max(at.distance(rest));
        }
    }
    assert!(
        furthest < 2e-4,
        "a settled composed corpse must not keep breathing: {furthest}"
    );
}
