use super::rogue_tests::{animate_look, points_under};
use super::*;

#[test]
fn moving_hero_strike_keeps_support_on_first_gameplay_frame() {
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn foot_poses(app: &App, root: Entity) -> [(Vec3, f32); 2] {
        let world = app.world();
        let rig = world.get::<Rig>(root).unwrap();
        [J::FootL, J::FootR].map(|joint| {
            let foot = rig.joint(joint).unwrap();
            let mut entity = foot;
            let mut frame = Affine3A::IDENTITY;
            loop {
                frame = world.get::<Transform>(entity).unwrap().compute_affine() * frame;
                if entity == root {
                    break;
                }
                entity = world.get::<ChildOf>(entity).unwrap().parent();
            }
            let sole = points_under(world, world.resource::<Assets<Mesh>>(), foot)
                .into_iter()
                .map(|point| frame.transform_point3(point).y)
                .reduce(f32::min)
                .expect("visible foot mesh");
            (frame.transform_point3(Vec3::ZERO), sole)
        })
    }

    fn walking_hero(stance: usize) -> (App, Entity) {
        let (mut app, root) = animate_look(Look::of(CreatureModel::Human, 9), 45, |f| {
            Vec3::Z * f as f32 * 0.025
        });
        for _ in 0..80 {
            if app
                .world()
                .get::<Anim>(root)
                .unwrap()
                .contacts
                .strike_entry
                .is_some_and(|seed| seed.feet[stance].planted && !seed.feet[1 - stance].planted)
            {
                return (app, root);
            }
            app.world_mut()
                .get_mut::<Transform>(root)
                .unwrap()
                .translation
                .z += 0.025;
            app.update();
        }
        panic!("moving Human did not reach one-sided support on foot {stance}");
    }

    for millis in [20, 50] {
        let dt = Duration::from_millis(millis);
        for stance in 0..2 {
            let (mut transferred, root) = walking_hero(stance);
            let (mut ordinary, control) = walking_hero(stance);
            let before = foot_poses(&transferred, root);
            assert_eq!(before, foot_poses(&ordinary, control));

            for (app, entity, allow_transfer) in [
                (&mut transferred, root, true),
                (&mut ordinary, control, false),
            ] {
                app.insert_resource(TimeUpdateStrategy::ManualDuration(dt));
                let mut anim = app.world_mut().get_mut::<Anim>(entity).unwrap();
                if !allow_transfer {
                    anim.contacts.strike_entry = None;
                }
                anim.act(Move::Strike);
                // Gameplay consumes the cue before the small follow-actor step.
                // No zero-delta pose refresh is inserted before the first update.
                app.world_mut()
                    .get_mut::<Transform>(entity)
                    .unwrap()
                    .translation
                    .z += 0.01;
                app.update();
                let anim = app.world().get::<Anim>(entity).unwrap();
                let Some((Move::Strike, phase)) = anim.action else {
                    panic!("first gameplay frame did not present the strike");
                };
                assert!(
                    (phase * Move::Strike.duration() - dt.as_secs_f32()).abs() < 1e-6,
                    "expected first strike frame at {dt:?}, got phase {phase}"
                );
            }

            let actual = foot_poses(&transferred, root);
            let without = foot_poses(&ordinary, control);
            let supported_travel = actual[stance].0.distance(before[stance].0);
            let ordinary_travel = without[stance].0.distance(before[stance].0);
            assert!(
                ordinary_travel > 0.025 && supported_travel < ordinary_travel * 0.9,
                "{millis} ms foot {stance}: support moved {supported_travel}, control moved {ordinary_travel}; sole heights {actual:?} vs {without:?}"
            );
            assert!(
                actual[stance].1 >= -0.002,
                "{millis} ms foot {stance}: supported sole went below floor: {}",
                actual[stance].1
            );

            let mut elapsed = dt;
            while elapsed < Duration::from_millis(200) {
                transferred.update();
                ordinary.update();
                elapsed += dt;
            }
            assert_eq!(
                foot_poses(&transferred, root),
                foot_poses(&ordinary, control),
                "{millis} ms foot {stance}: brief handoff must rejoin the ordinary strike"
            );
        }
    }
}
