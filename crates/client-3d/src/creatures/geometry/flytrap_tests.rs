//! The trap and leaf masses carry the plant's silhouette; its base stays rooted.
use super::*;
use crate::creatures::rogue_tests::{animate_look, spawn_one};
use bevy::render::mesh::VertexAttributeValues;

fn direct_points(
    world: &World,
    meshes: &Assets<Mesh>,
    joint: Entity,
    green_only: bool,
) -> Vec<Vec3> {
    let mut out = Vec::new();
    for child in world.get::<Children>(joint).unwrap().iter() {
        let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0)) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(vertices)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions");
        };
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("colors");
        };
        let local = world.get::<Transform>(child).unwrap().compute_affine();
        for (p, c) in vertices.iter().zip(colors) {
            if !green_only || (c[1] > c[0] * 1.3 && c[1] > c[2] * 1.3) {
                out.push(local.transform_point3((*p).into()));
            }
        }
    }
    assert!(!out.is_empty() && out.iter().all(|p| p.is_finite()));
    out
}

#[test]
fn flytrap_has_a_large_deep_two_lobed_trap() {
    for build in 0..3 {
        let look = Look::with_choice(
            CreatureModel::Flytrap,
            42,
            Some(Variant { build, ..default() }),
        );
        let (world, rig, _, meshes) = spawn_one(&look);
        for joint in [J::Head, J::Jaw] {
            let p = direct_points(&world, &meshes, rig.joint(joint).unwrap(), false);
            let (low, high) = p.iter().fold(
                (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                |(a, b), p| (a.min(*p), b.max(*p)),
            );
            let size = high - low;
            assert!(
                size.x > 0.55 && size.y > 0.16 && size.z > 0.50,
                "build={build} {joint:?}: trap lobe too small/shallow for the stalk: {size:?}"
            );
        }
    }
}

#[test]
fn flytrap_has_a_broad_leaf_rosette_and_substantial_supporting_stalk() {
    let look = Look::of(CreatureModel::Flytrap, 42);
    let (world, rig, _, meshes) = spawn_one(&look);
    let base = direct_points(&world, &meshes, rig.joint(J::Body).unwrap(), true);
    let extent = base
        .iter()
        .map(|p| Vec2::new(p.x, p.z).length())
        .fold(0.0, f32::max);
    assert!(
        extent > 0.44,
        "leaf rosette is too small: radial extent={extent:.4}"
    );
    for (joint, next) in [(J::Neck, J::Torso), (J::Torso, J::Head)] {
        let points = direct_points(&world, &meshes, rig.joint(joint).unwrap(), true);
        let axis = rig.rest[next as usize].normalize();
        let length = rig.rest[next as usize].length();
        let core = points
            .iter()
            .filter(|p| {
                let t = p.dot(axis) / length;
                (0.0..0.25).contains(&t)
            })
            .map(|p| (*p - axis * p.dot(axis)).length())
            .fold(0.0, f32::max);
        assert!(
            core > 0.07,
            "{joint:?}: thin supporting stalk radius={core:.4}"
        );
    }
}

#[test]
fn flytrap_opens_its_lobes_at_rest_and_snaps_without_uprooting() {
    let (mut app, root) = animate_look(Look::of(CreatureModel::Flytrap, 42), 12, |_| Vec3::ZERO);
    let rig = app.world().get::<Rig>(root).unwrap();
    let jaw = rig.joint(J::Jaw).unwrap();
    let base = rig.joint(J::Body).unwrap();
    let gape = app
        .world()
        .get::<Transform>(jaw)
        .unwrap()
        .rotation
        .angle_between(Quat::IDENTITY);
    assert!(
        gape > 0.65,
        "resting trap should display its red interior, relative gape={gape:.4}"
    );
    let m = Move::Strike;
    app.world_mut().get_mut::<Anim>(root).unwrap().action =
        Some((m, m.impact() - 0.016 / m.duration()));
    app.update();
    let shut = app
        .world()
        .get::<Transform>(jaw)
        .unwrap()
        .rotation
        .angle_between(Quat::IDENTITY);
    assert!(shut < 0.16, "trap must close at impact: {shut:.4}");
    assert!(
        app.world()
            .get::<Transform>(base)
            .unwrap()
            .translation
            .length()
            < 1e-6
    );
    assert!(
        app.world()
            .get::<Transform>(root)
            .unwrap()
            .translation
            .length()
            < 1e-6
    );
}

#[test]
fn flytrap_thrusts_its_mouth_forward_without_biting_the_floor() {
    let (mut app, root) = animate_look(Look::of(CreatureModel::Flytrap, 42), 12, |_| Vec3::ZERO);
    // Use the actor's forward world axis consistently in measurement.
    {
        let mut anim = app.world_mut().get_mut::<Anim>(root).unwrap();
        anim.yaw = 0.0;
        anim.target_yaw = None;
    }
    app.update();
    let rig = app.world().get::<Rig>(root).unwrap();
    let chain =
        [J::Lift, J::Body, J::Neck, J::Torso, J::Head].map(|joint| rig.joint(joint).unwrap());
    let jaw = rig.joint(J::Jaw).unwrap();
    let meshes = app.world().resource::<Assets<Mesh>>();
    let upper_points = direct_points(app.world(), meshes, chain[4], false);
    let lower_points = direct_points(app.world(), meshes, jaw, false);
    let mouth_bounds = |world: &World| {
        let matrix = |entity| world.get::<Transform>(entity).unwrap().compute_affine();
        let upper = chain
            .into_iter()
            .fold(Affine3A::IDENTITY, |path, joint| path * matrix(joint));
        let lower = upper * matrix(jaw);
        let samples = upper_points
            .iter()
            .map(|p| upper.transform_point3(*p))
            .chain(lower_points.iter().map(|p| lower.transform_point3(*p)));
        samples.fold((f32::INFINITY, f32::NEG_INFINITY), |(floor, front), at| {
            (floor.min(at.y), front.max(at.z))
        })
    };
    let (_, rest_front) = mouth_bounds(app.world());
    let strike = Move::Strike;
    for phase in [0.35, strike.impact(), 0.49, 0.55, 0.62] {
        app.world_mut().get_mut::<Anim>(root).unwrap().action =
            Some((strike, phase - 0.016 / strike.duration()));
        app.update();
        let (floor, front) = mouth_bounds(app.world());
        assert!(
            floor > 0.035,
            "flytrap bites the floor at phase {phase}: {floor:.4}"
        );
        if phase == strike.impact() {
            assert!(
                front > rest_front + 0.12,
                "the snap must still reach forward"
            );
        }
    }
}

#[test]
fn flytrap_stalk_does_not_flare_at_the_bending_joint() {
    let look = Look::of(CreatureModel::Flytrap, 42);
    let (world, rig, _, meshes) = spawn_one(&look);
    let joint = rig.joint(J::Torso).unwrap();
    let upper = direct_points(&world, &meshes, joint, false);
    // This neighborhood contains the stem's entry, not the wider supporting
    // stalk above it or its leaves. A wide cap becomes a ledge as the stem bends.
    let entry = (rig.rest[J::Head as usize] * 0.5 + Vec3::Z * 0.02).normalize();
    let widest = upper
        .iter()
        .filter(|p| p.dot(entry).abs() < 0.004 && p.length() < 0.12)
        .map(|p| p.length())
        .fold(0.0, f32::max);
    assert!(
        (0.058..0.080).contains(&widest),
        "upper stem should meet the lower stem without a flared cap: {widest:.4}"
    );
}

#[test]
fn flytrap_wilts_onto_the_floor_without_swallowing_a_lobe_or_lifting_its_roots() {
    use crate::creatures::animation::DeathMotion;

    let (mut app, root) = animate_look(Look::of(CreatureModel::Flytrap, 42), 12, |_| Vec3::ZERO);
    let rig = app.world().get::<Rig>(root).unwrap();
    let chain =
        [J::Lift, J::Body, J::Neck, J::Torso, J::Head].map(|joint| rig.joint(joint).unwrap());
    let jaw = rig.joint(J::Jaw).unwrap();
    let meshes = app.world().resource::<Assets<Mesh>>();
    let upper_points = direct_points(app.world(), meshes, chain[4], false);
    let lower_points = direct_points(app.world(), meshes, jaw, false);
    {
        let mut anim = app.world_mut().get_mut::<Anim>(root).unwrap();
        anim.yaw = 0.0;
        anim.target_yaw = None;
    }
    app.update();
    let base_rest = *app.world().get::<Transform>(chain[1]).unwrap();
    let lift_rest = *app.world().get::<Transform>(chain[0]).unwrap();
    for seed in [2.0, 8.0] {
        for motion in [
            DeathMotion::Collapse,
            DeathMotion::Stagger,
            DeathMotion::Dramatic,
        ] {
            app.world_mut()
                .get_mut::<Anim>(root)
                .unwrap()
                .preview_death(motion);
            for step in 0..=32 {
                let phase = step as f32 / 32.0;
                let mut anim = app.world_mut().get_mut::<Anim>(root).unwrap();
                anim.seed = seed;
                anim.yaw = 0.0;
                anim.target_yaw = None;
                anim.dead = Some((phase - 0.016 / motion.duration()).max(0.0));
                app.update();
                let world = app.world();
                let local = |e| world.get::<Transform>(e).unwrap().compute_affine();
                let upper = chain
                    .into_iter()
                    .fold(Affine3A::IDENTITY, |p, e| p * local(e));
                let lower = upper * local(jaw);
                let low = upper_points
                    .iter()
                    .map(|p| upper.transform_point3(*p).y)
                    .chain(lower_points.iter().map(|p| lower.transform_point3(*p).y))
                    .fold(f32::INFINITY, f32::min);
                assert!(
                    low >= -0.012,
                    "{motion:?} seed {seed} phase {phase:.3}: mouth buried at {low:.3}"
                );
                if step == 32 {
                    assert!(
                        low < 0.075,
                        "{motion:?}: wilted trap hangs above floor at {low:.3}"
                    );
                    assert!(upper.translation.z > 0.34, "stalk should wilt forward");
                }
                assert_eq!(*world.get::<Transform>(chain[0]).unwrap(), lift_rest);
                assert_eq!(*world.get::<Transform>(chain[1]).unwrap(), base_rest);
            }
        }
    }
}

#[test]
fn flytrap_eases_its_open_lobes_into_death_with_roots_planted() {
    use crate::creatures::animation::DeathMotion;
    let strike = Move::Strike;
    for (action, motion, corridor) in [
        (None, DeathMotion::Stagger, false),
        (Some(0.24), DeathMotion::Stagger, false),
        (Some(0.24), DeathMotion::Collapse, false),
        (Some(0.24), DeathMotion::Dramatic, true),
    ] {
        let (mut app, root) =
            animate_look(Look::of(CreatureModel::Flytrap, 42), 12, |_| Vec3::ZERO);
        {
            let mut anim = app.world_mut().get_mut::<Anim>(root).unwrap();
            anim.yaw = 0.0;
            anim.target_yaw = None;
            anim.action = action.map(|phase| (strike, phase - 0.016 / strike.duration()));
        }
        app.update();
        let rig = app.world().get::<Rig>(root).unwrap();
        let chain =
            [J::Lift, J::Body, J::Neck, J::Torso, J::Head].map(|joint| rig.joint(joint).unwrap());
        let jaw = rig.joint(J::Jaw).unwrap();
        let meshes = app.world().resource::<Assets<Mesh>>();
        let upper_points = direct_points(app.world(), meshes, chain[4], false);
        let lower_points = direct_points(app.world(), meshes, jaw, false);
        let jaw_rotation = |app: &App| app.world().get::<Transform>(jaw).unwrap().rotation;
        let mut prior = jaw_rotation(&app);
        assert!(prior.angle_between(Quat::IDENTITY) > if action.is_some() { 1.4 } else { 0.65 });
        let roots = [chain[0], chain[1]].map(|e| *app.world().get::<Transform>(e).unwrap());
        app.world_mut()
            .get_mut::<Anim>(root)
            .unwrap()
            .start_death(motion, corridor);
        for step in 0..25 {
            app.update();
            let now = jaw_rotation(&app);
            assert!(
                prior.angle_between(now) < 0.20,
                "{motion:?} bite={action:?} step {step}: hinge snapped {:.3} rad",
                prior.angle_between(now),
            );
            prior = now;
            let world = app.world();
            let local = |e| world.get::<Transform>(e).unwrap().compute_affine();
            let upper = chain
                .into_iter()
                .fold(Affine3A::IDENTITY, |p, e| p * local(e));
            let lower = upper * local(jaw);
            let low = upper_points
                .iter()
                .map(|p| upper.transform_point3(*p).y)
                .chain(lower_points.iter().map(|p| lower.transform_point3(*p).y))
                .fold(f32::INFINITY, f32::min);
            assert!(
                low >= -0.012,
                "{motion:?} step {step}: mouth buried at {low:.3}"
            );
            assert_eq!(
                [chain[0], chain[1]].map(|e| *world.get::<Transform>(e).unwrap()),
                roots
            );
        }
    }
}
