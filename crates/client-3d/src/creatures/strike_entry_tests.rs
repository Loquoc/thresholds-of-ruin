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

#[test]
fn aquator_keeps_its_four_solved_feet_when_corrosion_interrupts_a_step() {
    fn ankles(app: &App, root: Entity) -> [(Vec3, Quat); 4] {
        let world = app.world();
        let rig = world.get::<Rig>(root).unwrap();
        [J::HoofFL, J::HoofFR, J::HoofBL, J::HoofBR].map(|joint| {
            let mut at = Vec3::ZERO;
            let mut facing = Quat::IDENTITY;
            let mut entity = rig.joint(joint).expect("four Aquator feet");
            loop {
                let local = world.get::<Transform>(entity).unwrap();
                at = local.transform_point(at);
                facing = local.rotation * facing;
                if entity == root {
                    return (at, facing);
                }
                entity = world.get::<ChildOf>(entity).unwrap().parent();
            }
        })
    }

    let corrode = Move::for_ability("rogue.corrode", None);
    let mut worst = (0.0f32, 0, 0);
    let mut worst_twist = (0.0f32, 0, 0);
    for cue_frame in [26, 29, 32, 35, 38, 41, 44, 47] {
        let moving = || {
            animate_look(Look::of(CreatureModel::Aquator, 42), cue_frame, |f| {
                Vec3::Z * f.min(25) as f32 * 0.026 + Vec3::X * f.saturating_sub(25) as f32 * 0.026
            })
        };
        let (mut attack, root) = moving();
        let (mut control, control_root) = moving();
        let before = ankles(&attack, root);
        assert_eq!(before, ankles(&control, control_root));
        attack
            .world_mut()
            .get_mut::<Anim>(root)
            .unwrap()
            .act(corrode);
        control
            .world_mut()
            .get_mut::<Anim>(control_root)
            .unwrap()
            .contacts
            .strike_entry = None;
        control
            .world_mut()
            .get_mut::<Anim>(control_root)
            .unwrap()
            .act(corrode);
        attack.update();
        control.update();

        for (i, ((a, turned), (b, old))) in
            ankles(&attack, root).into_iter().zip(before).enumerate()
        {
            let jump = a.distance(b);
            let twist = turned.angle_between(old);
            if twist > worst_twist.0 {
                worst_twist = (twist, cue_frame, i);
            }
            if jump > worst.0 {
                worst = (jump, cue_frame, i);
            }
        }
        for _ in 0..12 {
            attack.update();
            control.update();
        }
        assert_eq!(
            ankles(&attack, root),
            ankles(&control, control_root),
            "transferred support rejoins the unchanged authored attack before impact"
        );
    }
    eprintln!("Worst foot displacement {worst:?}, worst turn {worst_twist:?}");
    assert!(
        worst_twist.0 < 0.32,
        "first corrosion frame twisted a foot {worst_twist:?} (radians, cue frame, leg)"
    );
    assert!(
        worst.0 < 0.052,
        "first corrosion frame moved a grounded or swinging foot {worst:?} (distance, cue frame, leg)"
    );
}

fn ankles(app: &App, root: Entity) -> [Vec3; 2] {
    let world = app.world();
    let rig = world.get::<Rig>(root).unwrap();
    [J::FootL, J::FootR].map(|joint| {
        let mut entity = rig.joint(joint).unwrap();
        let mut point = Vec3::ZERO;
        loop {
            point = world
                .get::<Transform>(entity)
                .unwrap()
                .transform_point(point);
            if entity == root {
                return point;
            }
            entity = world.get::<ChildOf>(entity).unwrap().parent();
        }
    })
}

#[test]
fn hobgoblin_keeps_its_footing_when_a_strike_interrupts_braking() {
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    let look = Look::preview_adjusted(CreatureModel::Hobgoblin, 31, |variant| {
        *variant = Variant::default();
    });
    let braking = || {
        let (mut app, root) = animate_look(look.clone(), 1, |_| Vec3::ZERO);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            20,
        )));
        {
            let mut anim = app.world_mut().get_mut::<Anim>(root).unwrap();
            *anim = Anim::new(31, Vec3::ZERO);
            anim.yaw = 0.0;
            anim.target_yaw = Some(0.0);
        }
        for tick in 0..=81_u32 {
            app.world_mut()
                .get_mut::<Transform>(root)
                .unwrap()
                .translation
                .z = 0.011 * tick.clamp(4, 72).saturating_sub(4) as f32;
            app.update();
        }
        (app, root)
    };

    let (mut attack, root) = braking();
    let (mut stop, stop_root) = braking();
    let before = ankles(&attack, root);
    assert_eq!(before, ankles(&stop, stop_root));
    assert!(
        attack
            .world()
            .get::<Anim>(root)
            .unwrap()
            .contacts
            .settle
            .is_some(),
        "strike must interrupt an unfinished, supported stop"
    );
    attack
        .world_mut()
        .get_mut::<Anim>(root)
        .unwrap()
        .act(Move::Strike);
    attack.update();
    stop.update();
    let Some((Move::Strike, phase)) = attack.world().get::<Anim>(root).unwrap().action else {
        panic!("strike cue must present on the first gameplay frame");
    };
    assert!((phase * Move::Strike.duration() - 0.02).abs() < 1e-5);
    let walking = ankles(&stop, stop_root);
    let strike = ankles(&attack, root);
    for i in 0..2 {
        let control = before[i].distance(walking[i]);
        assert!(
            control < 0.055,
            "ordinary brake moved foot {i} by {control:.3}"
        );
        let jump = before[i].distance(strike[i]);
        assert!(
            jump < 0.065,
            "strike ripped foot {i} by {jump:.3} units in 20 ms from its braking pose (stop control: {control:.3})"
        );
    }
}

#[test]
fn hobgoblin_resumes_walking_during_strike_recovery_without_an_ankle_jump() {
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn run(strike: bool, restart: u32) -> (f32, u32) {
        let look = Look::preview_adjusted(CreatureModel::Hobgoblin, 31, |variant| {
            *variant = Variant::default();
        });
        let (mut app, root) = animate_look(look, 1, |_| Vec3::ZERO);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            20,
        )));
        let mut initial = Anim::new(31, Vec3::ZERO);
        initial.yaw = 0.0;
        initial.target_yaw = Some(0.0);
        *app.world_mut().get_mut::<Anim>(root).unwrap() = initial;
        let mut last = ankles(&app, root);
        let mut worst = (0.0, 0);
        for tick in 0..=134_u32 {
            app.world_mut()
                .get_mut::<Transform>(root)
                .unwrap()
                .translation
                .z =
                0.011 * (tick.clamp(4, 72).saturating_sub(4) + tick.saturating_sub(restart)) as f32;
            if strike && tick == 82 {
                app.world_mut()
                    .get_mut::<Anim>(root)
                    .unwrap()
                    .act(Move::Strike);
            }
            app.update();
            if tick % 2 == 0 {
                let now = ankles(&app, root);
                if (104..=132).contains(&tick) {
                    let jump = now[0].distance(last[0]).max(now[1].distance(last[1]));
                    if jump > worst.0 {
                        worst = (jump, tick);
                    }
                }
                last = now;
            }
        }
        worst
    }

    let ordinary = run(false, 103);
    let after_strike = run(true, 116);
    assert!(
        ordinary.0 < 0.105 && after_strike.0 < 0.105,
        "control: {ordinary:?} {after_strike:?}"
    );
    let overlap = run(true, 103);
    assert!(
        overlap.0 < 0.105,
        "restarting during recovery moved an ankle by {:.3} units in 40 ms at tick {}; controls: {ordinary:?} {after_strike:?}",
        overlap.0, overlap.1
    );
}
