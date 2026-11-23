//! Readable skulls and actual hinged bites on both independently placed snakes.
use super::*;
use crate::creatures::rogue_tests::{animate_look, spawn_one};
use bevy::render::mesh::VertexAttributeValues;

fn direct_vertices(world: &World, meshes: &Assets<Mesh>, part: Entity) -> Vec<Vec3> {
    let mut out = Vec::new();
    for child in world.get::<Children>(part).unwrap().iter() {
        let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0)) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions");
        };
        let transform = world.get::<Transform>(child).unwrap().compute_affine();
        for point in points {
            let p = transform.transform_point3((*point).into());
            assert!(p.is_finite());
            out.push(p);
        }
    }
    assert!(!out.is_empty());
    out
}

fn direct_bounds(world: &World, meshes: &Assets<Mesh>, part: Entity) -> (Vec3, Vec3) {
    direct_vertices(world, meshes, part).into_iter().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(low, high), p| (low.min(p), high.max(p)),
    )
}

#[test]
fn serpent_is_assembled_and_above_the_floor_before_the_first_update() {
    for model in [CreatureModel::Snake, CreatureModel::Rattlesnake] {
        let (world, rig, _, meshes) = spawn_one(&Look::of(model, 42));
        assert_eq!(rig.segments.len(), SNAKE);
        let transform = |part| world.get::<Transform>(part).unwrap();
        let head = transform(rig.segments[0]);
        let tail = transform(*rig.segments.last().unwrap());
        assert!(
            head.translation.z - tail.translation.z > 1.05,
            "{model:?}: pre-animation body must already be extended"
        );
        for mates in rig.segments.windows(2) {
            let [front, back] = [mates[0], mates[1]];
            let gap = transform(front)
                .translation
                .distance(transform(back).translation);
            assert!(
                (0.065..0.071).contains(&gap),
                "{model:?}: rest band spacing {gap}"
            );
        }
        let low = |part, at: Affine3A| {
            direct_vertices(&world, &meshes, part)
                .into_iter()
                .map(|point| at.transform_point3(point).y)
                .fold(f32::INFINITY, f32::min)
        };
        for &band in &rig.segments {
            let y = low(band, transform(band).compute_affine());
            assert!(
                y > -0.015,
                "{model:?}: unposed segment sinks below the floor: {y}"
            );
        }
        let jaw = rig.joint(J::Jaw).unwrap();
        let fangs = rig.joint(J::Fangs).unwrap();
        let tongue = rig.joint(J::Snout).unwrap();
        assert!(
            low(fangs, transform(fangs).compute_affine())
                > low(jaw, transform(jaw).compute_affine()),
            "{model:?}: the neutral fangs must start tucked"
        );
        assert!(
            transform(tongue).scale.z < 0.02,
            "{model:?}: first frame should not show a full flick"
        );
    }
}

#[test]
fn serpent_skulls_remain_substantial_beside_the_supporting_neck() {
    for model in [CreatureModel::Snake, CreatureModel::Rattlesnake] {
        for build in 0..3 {
            let look = Look::with_choice(model, 42, Some(Variant { build, ..default() }));
            let (world, rig, _, meshes) = spawn_one(&look);
            let (low, high) = direct_bounds(&world, &meshes, rig.segments[0]);
            let (neck_low, neck_high) = direct_bounds(&world, &meshes, rig.segments[1]);
            let size = high - low;
            let breadth = size.x / (neck_high - neck_low).x;
            assert!(breadth > 1.45 && size.y > 0.18 && size.z > 0.34,
                "{model:?} build={build}: skull must read separately from neck: size={size:?}, breadth ratio={breadth:.3}");
        }
    }
}

#[test]
fn serpent_lower_jaws_are_separate_full_length_mouth_surfaces() {
    for model in [CreatureModel::Snake, CreatureModel::Rattlesnake] {
        let (world, rig, _, meshes) = spawn_one(&Look::of(model, 42));
        let jaw = rig
            .joint(J::Jaw)
            .expect("a biting snake needs a separate hinged lower jaw");
        assert!(world
            .get::<Children>(rig.segments[0])
            .unwrap()
            .contains(&jaw));
        let (low, high) = direct_bounds(&world, &meshes, jaw);
        let size = high - low;
        assert!(
            size.x > 0.19 && size.y > 0.035 && size.z > 0.28,
            "{model:?}: lower jaw must supply the broad lower mouth: {size:?}"
        );
    }
}

#[test]
fn serpent_bites_gape_in_preparation_close_at_impact_and_keep_the_root() {
    for model in [CreatureModel::Snake, CreatureModel::Rattlesnake] {
        let (mut app, root) = animate_look(Look::of(model, 42), 12, |_| Vec3::ZERO);
        let jaw = app
            .world()
            .get::<Rig>(root)
            .unwrap()
            .joint(J::Jaw)
            .expect("hinged bite");
        let angle = |app: &App| {
            app.world()
                .get::<Transform>(jaw)
                .unwrap()
                .rotation
                .angle_between(Quat::IDENTITY)
        };
        assert!(angle(&app) < 0.12, "closed resting mouth");
        let motion = if model == CreatureModel::Snake {
            Move::Strike
        } else {
            Move::Bite(Accent::Teal)
        };
        for (phase, open) in [(motion.impact() - 0.10, true), (motion.impact(), false)] {
            app.world_mut().get_mut::<Anim>(root).unwrap().action =
                Some((motion, phase - 0.016 / motion.duration()));
            app.update();
            let gape = angle(&app);
            assert!(
                if open { gape > 0.65 } else { gape < 0.20 },
                "{model:?} phase={phase}: gape={gape}"
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
    }
}

#[test]
fn serpent_fangs_fold_inside_the_closed_mouth_and_extend_with_the_bite() {
    for model in [CreatureModel::Snake, CreatureModel::Rattlesnake] {
        let (mut app, root) = animate_look(Look::of(model, 42), 12, |_| Vec3::ZERO);
        let rig = app.world().get::<Rig>(root).unwrap();
        let fangs = rig.joint(J::Fangs).expect("folding upper fangs");
        let head = rig.segments[0];
        let jaw = rig.joint(J::Jaw).unwrap();
        let motion = Move::Bite(Accent::Teal);
        for (phase, extended) in [
            (None, false),
            (Some(motion.impact() - 0.10), true),
            (Some(motion.impact()), false),
        ] {
            app.world_mut().get_mut::<Anim>(root).unwrap().action =
                phase.map(|t| (motion, t - 0.016 / motion.duration()));
            app.update();
            let rotation = app.world().get::<Transform>(fangs).unwrap().rotation;
            let angle = rotation.angle_between(Quat::IDENTITY);
            assert!(
                if extended { angle < 0.12 } else { angle > 1.4 },
                "{model:?}: folded/extended fangs angle={angle}"
            );
            if !extended {
                let world = app.world();
                let meshes = world.resource::<Assets<Mesh>>();
                let fang_transform = world.get::<Transform>(fangs).unwrap().compute_affine();
                let jaw_transform = world.get::<Transform>(jaw).unwrap().compute_affine();
                let low = |part, transform: Affine3A| {
                    direct_vertices(world, meshes, part)
                        .into_iter()
                        .map(|p| transform.transform_point3(p).y)
                        .fold(f32::INFINITY, f32::min)
                };
                assert!(
                    low(fangs, fang_transform) > low(jaw, jaw_transform),
                    "{model:?}: folded fangs must fit above the lower jaw exterior"
                );
                assert!(world.get::<Children>(head).unwrap().contains(&fangs));
            }
        }
    }
}

#[test]
fn serpent_emitted_heads_and_jaws_clear_the_floor_at_rest_bite_and_side_fall() {
    for model in [CreatureModel::Snake, CreatureModel::Rattlesnake] {
        let (mut app, root) = animate_look(Look::of(model, 42), 12, |_| Vec3::ZERO);
        for state in 0..4 {
            {
                let mut anim = app.world_mut().get_mut::<Anim>(root).unwrap();
                let motion = Move::Bite(Accent::Teal);
                anim.action = match state {
                    1 => Some((motion, motion.impact() - 0.10 - 0.016 / motion.duration())),
                    2 => Some((motion, motion.impact() - 0.016 / motion.duration())),
                    _ => None,
                };
                anim.dead = (state == 3).then_some(1.0);
            }
            app.update();
            let world = app.world();
            let rig = world.get::<Rig>(root).unwrap();
            let meshes = world.resource::<Assets<Mesh>>();
            let head = rig.segments[0];
            let head_transform = world.get::<Transform>(head).unwrap().compute_affine();
            for part in [
                head,
                rig.joint(J::Jaw).unwrap(),
                rig.joint(J::Fangs).unwrap(),
            ] {
                let transform = if part == head {
                    head_transform
                } else {
                    head_transform * world.get::<Transform>(part).unwrap().compute_affine()
                };
                let low = direct_vertices(world, meshes, part)
                    .into_iter()
                    .map(|p| transform.transform_point3(p).y)
                    .fold(f32::INFINITY, f32::min);
                assert!(
                    low >= -0.015,
                    "{model:?} state={state} part={part:?}: emitted mouth penetrates floor y={low}"
                );
            }
        }
    }
}

#[test]
fn raised_serpents_enter_and_leave_a_bite_without_resetting_the_neck() {
    // Quiet suppresses incidental rearing, but not a rattlesnake's warning.
    // The ordinary snake is sampled near the top of its separate idle rear.
    for (model, quiet) in [
        (CreatureModel::Rattlesnake, true),
        (CreatureModel::Snake, false),
    ] {
        let (mut app, actor) = animate_look(Look::of(model, 49), 12, |_| Vec3::ZERO);
        {
            let mut anim = app.world_mut().get_mut::<Anim>(actor).unwrap();
            anim.seed = 0.0;
            anim.time = 4.70;
            anim.quiet = quiet;
        }
        let head = app.world().get::<Rig>(actor).unwrap().segments[0];
        let at = |app: &App| app.world().get::<Transform>(head).unwrap().translation;
        app.update();
        let ready = at(&app);
        app.world_mut()
            .get_mut::<Anim>(actor)
            .unwrap()
            .act(Move::Strike);
        app.update();
        let start = at(&app);
        assert!(
            (start.y - ready.y).abs() < 0.060,
            "{model:?} head snaps on bite entry: {ready:?} to {start:?}"
        );
        let mut last = start;
        let mut front: f32 = last.z;
        for frame in 0..54 {
            let was_active = app.world().get::<Anim>(actor).unwrap().action.is_some();
            app.update();
            let at_now = at(&app);
            front = front.max(at_now.z);
            let active = app.world().get::<Anim>(actor).unwrap().action.is_some();
            if was_active && !active {
                assert!(
                    (at_now.y - last.y).abs() < 0.060,
                    "{model:?} head snaps on bite exit: {last:?} to {at_now:?}"
                );
            }
            assert!(at_now.is_finite(), "{model:?} at {frame}");
            last = at_now;
        }
        assert!(front > 0.80, "{model:?} still lunges into the bite");
        assert!(last.y > 0.14, "{model:?} resumes its raised rest shape");
    }
}

#[test]
fn rattlesnake_default_rest_has_an_extended_body_with_a_raised_head_and_rattle() {
    for quiet in [false, true] {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Rattlesnake,
                42,
                Some(Variant { build, ..default() }),
            );
            let (mut app, root) = animate_look(look, 12, |_| Vec3::ZERO);
            app.world_mut().get_mut::<Anim>(root).unwrap().quiet = quiet;
            app.update();
            let world = app.world();
            let rig = world.get::<Rig>(root).unwrap();
            let points: Vec<_> = rig
                .segments
                .iter()
                .map(|e| world.get::<Transform>(*e).unwrap().translation)
                .collect();
            let head = points[0];
            let tail = points[points.len() - 1];
            assert!(head.with_y(0.0).distance(tail.with_y(0.0))>0.85,
                "build={build} quiet={quiet}: default rest curls the body into a coil: head={head:?},tail={tail:?}");
            for segment in points.windows(2) {
                assert!(
                    (0.045..0.078).contains(&segment[0].distance(segment[1])),
                    "extended segments must remain connected"
                );
            }
            assert!(
                head.y > points[10].y + 0.09,
                "head stays raised over the extended body"
            );
            assert!(tail.y > points[14].y + 0.04, "warning rattle stays lifted");
        }
    }
}

#[test]
fn raised_snake_does_not_snap_down_at_the_start_of_death() {
    for motion in [
        DeathMotion::Collapse,
        DeathMotion::Stagger,
        DeathMotion::Dramatic,
    ] {
        let (mut app, actor) = animate_look(Look::of(CreatureModel::Snake, 23), 12, |_| Vec3::ZERO);
        {
            let mut anim = app.world_mut().get_mut::<Anim>(actor).unwrap();
            anim.seed = 0.0;
            anim.time = 4.72;
        }
        let head = app.world().get::<Rig>(actor).unwrap().segments[0];
        let at = |app: &App| app.world().get::<Transform>(head).unwrap().translation;
        app.update();
        let ready = at(&app);
        assert!(ready.y > 0.24, "fixture must begin raised: {ready:?}");
        app.world_mut()
            .get_mut::<Anim>(actor)
            .unwrap()
            .preview_death(motion);
        app.update();
        let entry = at(&app);
        assert!(
            (entry.y - ready.y).abs() < 0.05,
            "{motion:?}: raised head snapped at death entry: {ready:?} to {entry:?}"
        );
        let mut prior = entry;
        for _ in 0..8 {
            app.update();
            let head = at(&app);
            assert!(
                (head.y - prior.y).abs() < 0.05,
                "{motion:?}: abrupt neck step from {prior:?} to {head:?}"
            );
            prior = head;
        }
        for _ in 0..200 {
            app.update();
        }
        let settled = at(&app);
        assert!(settled.y < ready.y - 0.06);
        assert_eq!(app.world().get::<Anim>(actor).unwrap().dead, Some(1.0));
    }
}

#[test]
fn rattlesnake_releases_an_interrupted_bite_into_the_fall() {
    let bite = Move::for_ability("rogue.venomous_bite", None);
    for (motion, face_corridor) in [
        (DeathMotion::Collapse, false),
        (DeathMotion::Stagger, false),
        (DeathMotion::Dramatic, false),
        (DeathMotion::Collapse, true),
    ] {
        let (mut app, root) =
            animate_look(Look::of(CreatureModel::Rattlesnake, 42), 12, |_| Vec3::ZERO);
        app.world_mut().get_mut::<Anim>(root).unwrap().act(bite);
        for _ in 0..16 {
            app.update();
        }
        let rig = app.world().get::<Rig>(root).unwrap();
        let (head, tail) = (rig.segments[0], *rig.segments.last().unwrap());
        let at =
            |app: &App| [head, tail].map(|e| app.world().get::<Transform>(e).unwrap().translation);
        let mut prior = at(&app);
        assert!(prior[0].z > 0.81, "fixture must begin at bite contact");
        app.world_mut()
            .get_mut::<Anim>(root)
            .unwrap()
            .start_death(motion, face_corridor);
        for step in 0..28 {
            app.update();
            let points = at(&app);
            for (old, new) in prior.into_iter().zip(points) {
                assert!(
                    old.distance(new) < 0.052,
                    "{motion:?} facing={face_corridor} step {step}: {old:?} snapped to {new:?}"
                );
            }
            prior = points;
        }
        for _ in 0..230 {
            app.update();
        }
        assert_eq!(app.world().get::<Anim>(root).unwrap().dead, Some(1.0));
        assert!(
            (at(&app)[0].y - snake_ground_height(0, 2.6, Style::Rattlesnake)).abs() < 1.0e-6,
            "interrupted bite must settle at the normal rolled head height"
        );
    }
}
