use super::*;
use crate::{props::PropMaterials, ActorBody, Game};
use tor_client_3d_model::{Actor, Happening};
use tor_protocol::{AbilityOutcome, ActorTarget, Position};

fn body(app: &mut App, at: Vec3) -> Entity {
    let label = app.world_mut().spawn_empty().id();
    let ring = app.world_mut().spawn_empty().id();
    app.world_mut()
        .spawn((
            ActorBody {
                appearance_seed: None,
                appearance_key: "fixture".into(),
                model: CreatureModel::Placeholder,
                wearing: Default::default(),
                hostile: false,
                goal: at,
                walk: default(),
                own: false,
                label,
                ring,
                name: String::new(),
                height: 1.0,
            },
            Anim::new(0, at),
        ))
        .id()
}

fn scene(id: &str, name: &str, show_target: bool, disclose_target: bool) -> (App, Entity, Entity) {
    let mut app = App::new();
    let mut materials = Assets::<StandardMaterial>::default();
    let library = PropLibrary::new(PropMaterials::new(&mut materials));
    app.init_resource::<Game>()
        .init_resource::<Assets<Mesh>>()
        .insert_resource(materials)
        .insert_resource(library)
        .add_systems(Update, crate::scene::react);
    let caster = ActorTarget::from_digest([11; 32]);
    let target = ActorTarget::from_digest([12; 32]);
    let caster_entity = body(&mut app, Vec3::ZERO);
    let target_entity = body(&mut app, Vec3::X);
    let mut view = crate::test_support::view(1);
    view.hp = Some((12, 12));
    view.actors = [(caster, 0), (target, 1)]
        .into_iter()
        .filter(|(id, _)| show_target || *id == caster)
        .map(|(id, x)| Actor {
            appearance_seed: None,
            model: CreatureModel::Placeholder,
            wearing: Default::default(),
            attire: Default::default(),
            id,
            name: String::new(),
            description: String::new(),
            position: Position { x, y: 0, z: 0 },
            hostile: false,
            injury: None,
            is_self: id == caster,
        })
        .collect();
    {
        let mut game = app.world_mut().resource_mut::<Game>();
        game.view = Some(view);
        game.actors.insert(caster, caster_entity);
        if show_target {
            game.actors.insert(target, target_entity);
        }
        game.pending.push(Happening::Ability {
            caster: Some(caster),
            target: disclose_target.then_some(target),
            id: id.into(),
            name: name.into(),
            outcome: AbilityOutcome::Applied,
            strike: None,
        });
    }
    (app, caster_entity, target_entity)
}

#[test]
fn an_applied_effect_without_a_harm_witness_does_not_invent_a_flinch() {
    let (mut app, caster_entity, target_entity) =
        scene("fixture_support", "Gentle benediction", true, true);
    app.update();
    assert!(
        app.world()
            .get::<Anim>(caster_entity)
            .unwrap()
            .action
            .is_some(),
        "the applied effect must still animate its caster"
    );
    assert!(
        app.world()
            .get::<Anim>(target_entity)
            .unwrap()
            .hurt
            .is_none(),
        "applied is not evidence of damage or an injury change"
    );
}

#[test]
fn an_undisclosed_target_does_not_get_a_fabricated_projectile_endpoint() {
    let (mut app, caster, _) = scene("spark_lance_fire", "Spark Lance (Fire)", false, false);
    app.update();
    assert!(app.world().get::<Anim>(caster).unwrap().action.is_some());
    let mut bolts = app.world_mut().query::<&Bolt>();
    assert_eq!(
        bolts.iter(app.world()).count(),
        0,
        "no target position was disclosed"
    );
}

fn pose(id: &str, name: &str) -> Move {
    let (mut app, caster, _) = scene(id, name, true, true);
    app.update();
    app.world().get::<Anim>(caster).unwrap().action.unwrap().0
}

#[test]
fn renaming_a_disclosed_ability_does_not_change_its_motion_recipe() {
    assert_eq!(
        pose("basic_melee", "Spark Lance (Fire)"),
        pose("basic_melee", "Bind Wounds")
    );
}

#[test]
fn known_techniques_keep_distinct_motions_even_with_the_same_caption() {
    assert_ne!(
        pose("basic_melee", "A technique"),
        pose("bind_wounds", "A technique")
    );
}

#[test]
fn a_disclosed_harm_witness_animates_at_the_effects_arrival() {
    let (mut app, _, target_entity) = scene("spark_lance_fire", "A technique", true, true);
    let target = ActorTarget::from_digest([12; 32]);
    app.world_mut()
        .resource_mut::<Game>()
        .pending
        .push(Happening::Hurt(target));
    app.update();
    assert!(app
        .world()
        .get::<Anim>(target_entity)
        .unwrap()
        .hurt
        .is_some_and(|t| t < -0.32));
    let mut bolts = app.world_mut().query::<&Bolt>();
    assert_eq!(
        bolts.iter(app.world()).count(),
        1,
        "known target keeps its projectile"
    );
}

#[test]
fn an_unfamiliar_technique_keeps_a_neutral_motion_regardless_of_caption() {
    assert_eq!(pose("custom_support", "Spark Lance (Fire)"), Move::Gesture);
    assert_eq!(pose("custom_support", "Bind Wounds"), Move::Gesture);
}

#[test]
fn fey_theft_uses_the_filch_pose_without_inventing_an_item_or_hit_cue() {
    for id in ["filch_charm", "filch_gold"] {
        let (mut app, caster, target) = scene(id, "A technique", true, true);
        app.update();
        assert_eq!(
            app.world().get::<Anim>(caster).unwrap().action.unwrap().0,
            Move::Filch
        );
        assert!(app.world().get::<Anim>(target).unwrap().hurt.is_none());
        let mut cues = app
            .world_mut()
            .query_filtered::<Entity, Or<(With<Fade>, With<Bolt>, With<Mesh3d>)>>();
        assert_eq!(cues.iter(app.world()).count(), 0);
    }
}

#[test]
fn live_breath_does_not_fabricate_a_path_without_authoritative_geometry() {
    let (mut app, caster, _) = scene("rogue.breath_weapon", "Breath Weapon", true, true);
    app.update();
    assert!(app.world().get::<Anim>(caster).unwrap().action.is_some());
    let mut bolts = app.world_mut().query::<&Bolt>();
    assert_eq!(bolts.iter(app.world()).count(), 0);
}

#[test]
fn authoritative_geometry_renders_without_an_actor_or_a_hidden_gap_connector() {
    use tor_protocol::{EffectAppearance, EffectCue, EffectDelivery, EffectTiming};
    let (mut app, _, _) = scene("fixture", "Fixture", false, false);
    app.world_mut().resource_mut::<Game>().pending = vec![Happening::Effect {
        caster: None,
        ability: None,
        tick: 1,
        timing: EffectTiming::Instant,
        presentation: tor_protocol::EffectPresentation::neutral(EffectAppearance::Bolt),
        object_model: None,
        delivery: EffectDelivery::Ray,
        cue: EffectCue::Neutral,
        segments: vec![
            vec![Position { x: 0, y: 0, z: 0 }, Position { x: 1, y: 0, z: 0 }],
            vec![Position { x: 5, y: 0, z: 0 }, Position { x: 6, y: 0, z: 0 }],
        ],
    }];
    app.update();
    let mut bolts = app.world_mut().query::<&Bolt>();
    assert_eq!(
        bolts.iter(app.world()).count(),
        2,
        "never connect hidden gap"
    );
}

#[test]
fn authoritative_target_effect_replaces_canned_visual_but_keeps_actor_motion() {
    use tor_protocol::{ContentRef, EffectAppearance, EffectCue, EffectDelivery, EffectTiming};
    let (mut app, caster_entity, _) = scene("spark_lance_fire", "Spark Lance", true, true);
    app.world_mut()
        .resource_mut::<Game>()
        .pending
        .push(Happening::Effect {
            caster: Some(ActorTarget::from_digest([11; 32])),
            ability: Some(ContentRef {
                id: "spark_lance_fire".into(),
                name: "Spark Lance".into(),
            }),
            tick: 1,
            timing: EffectTiming::Instant,
            presentation: tor_protocol::EffectPresentation::neutral(EffectAppearance::Flash),
            object_model: None,
            delivery: EffectDelivery::Target,
            cue: EffectCue::Fire,
            segments: vec![vec![Position { x: 1, y: 0, z: 0 }]],
        });
    app.update();
    assert!(app
        .world()
        .get::<Anim>(caster_entity)
        .unwrap()
        .action
        .is_some());
    let mut bolts = app.world_mut().query::<&Bolt>();
    assert_eq!(
        bolts.iter(app.world()).count(),
        0,
        "no guessed caster-to-target path"
    );
    let mut fades = app.world_mut().query::<&Fade>();
    assert_eq!(
        fades.iter(app.world()).count(),
        1,
        "only authoritative target effect"
    );
}

#[test]
fn a_death_waits_for_the_disclosed_flight_that_caused_it() {
    // A fire bolt flies four cells to a kestrel that dies at once: the fall
    // must wait for the bolt, never precede it.
    let mut app = App::new();
    let mut materials = Assets::<StandardMaterial>::default();
    let library = PropLibrary::new(PropMaterials::new(&mut materials));
    app.init_resource::<Game>()
        .init_resource::<Assets<Mesh>>()
        .insert_resource(materials)
        .insert_resource(library)
        .add_systems(Update, crate::scene::react);
    let caster = ActorTarget::from_digest([11; 32]);
    let kestrel = ActorTarget::from_digest([13; 32]);
    let caster_entity = body(&mut app, crate::scene::stand(IVec3::ZERO));
    let kestrel_entity = body(&mut app, crate::scene::stand(IVec3::new(4, 0, 0)));
    {
        let mut game = app.world_mut().resource_mut::<Game>();
        game.actors.insert(caster, caster_entity);
        game.actors.insert(kestrel, kestrel_entity);
        game.pending.push(Happening::Effect {
            caster: Some(caster),
            ability: Some(tor_protocol::ContentRef {
                id: "fire_bolt".into(),
                name: "Fire bolt".into(),
            }),
            tick: 10,
            timing: tor_protocol::EffectTiming::Instant,
            presentation: tor_protocol::EffectPresentation::neutral(
                tor_protocol::EffectAppearance::Bolt,
            ),
            object_model: None,
            delivery: tor_protocol::EffectDelivery::Ray,
            cue: tor_protocol::EffectCue::Fire,
            segments: vec![(0..=4).map(|x| Position { x, y: 0, z: 0 }).collect()],
        });
        game.pending.push(Happening::Died(kestrel));
    }
    app.update();
    let travel = 4.0 / MISSILE_CELLS_PER_SECOND as f32;
    let dead = app
        .world()
        .get::<Anim>(kestrel_entity)
        .unwrap()
        .dead
        .unwrap();
    assert!(
        (dead - (-0.35 - travel)).abs() < 1e-5,
        "death phase {dead} must wait {travel}s for the bolt"
    );
    // A death that follows in a later batch still waits for the flight.
    app.world_mut()
        .resource_mut::<Game>()
        .pending
        .push(Happening::Hurt(kestrel));
    app.update();
    let hurt = app
        .world()
        .get::<Anim>(kestrel_entity)
        .unwrap()
        .hurt
        .unwrap();
    assert!(hurt < 0.0, "a later hurt still waits for the bolt");
}

#[test]
fn disclosed_bolt_and_griffin_reaction_stay_aligned_across_a_long_frame() {
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;
    use tor_protocol::{
        EffectAppearance, EffectCue, EffectDelivery, EffectPresentation, EffectTiming,
    };

    let run = |steps_after_approach: &[u64]| {
        let at = crate::scene::stand(IVec3::new(4, 0, 0));
        let (mut app, griffin) =
            rogue_tests::animate_look(Look::of(CreatureModel::Griffin, 42), 12, |_| at);
        let mut materials = Assets::<StandardMaterial>::default();
        let library = PropLibrary::new(PropMaterials::new(&mut materials));
        app.insert_resource(materials)
            .insert_resource(library)
            .init_resource::<Game>()
            .add_systems(
                Update,
                (crate::scene::react.before(animate), effects.after(animate)),
            );
        let target = ActorTarget::from_digest([13; 32]);
        let caster = ActorTarget::from_digest([11; 32]);
        let caster_entity = body(&mut app, crate::scene::stand(IVec3::ZERO));
        let template = body(&mut app, at);
        let target_body = app
            .world_mut()
            .entity_mut(template)
            .take::<ActorBody>()
            .unwrap();
        app.world_mut().despawn(template);
        app.world_mut().entity_mut(griffin).insert(target_body);
        let mut game = app.world_mut().resource_mut::<Game>();
        game.actors.insert(caster, caster_entity);
        game.actors.insert(target, griffin);
        game.pending.push(Happening::Effect {
            caster: Some(caster),
            ability: None,
            tick: 10,
            timing: EffectTiming::Instant,
            presentation: EffectPresentation::neutral(EffectAppearance::Bolt),
            object_model: None,
            delivery: EffectDelivery::Ray,
            cue: EffectCue::Fire,
            segments: vec![(0..=4).map(|x| Position { x, y: 0, z: 0 }).collect()],
        });
        game.pending.push(Happening::Hurt(target));
        game.pending.push(Happening::Died(target));
        let motion = Move::for_ability("rogue.fear", None);
        app.world_mut().get_mut::<Anim>(griffin).unwrap().action = Some((motion, 0.28));
        let update = |app: &mut App, ms| {
            *app.world_mut().resource_mut::<TimeUpdateStrategy>() =
                TimeUpdateStrategy::ManualDuration(Duration::from_millis(ms));
            app.update();
        };
        update(&mut app, 0);
        let bolt = {
            let world = app.world_mut();
            world
                .query_filtered::<Entity, With<Bolt>>()
                .single(world)
                .unwrap()
        };
        for _ in 0..14 {
            update(&mut app, 16);
        }
        let before = app.world().get::<Anim>(griffin).unwrap();
        assert!(app.world().get::<Bolt>(bolt).is_some());
        assert!(before.hurt.unwrap() < 0.0, "no hurt before contact");
        let before_death = before.dead.unwrap();
        for &ms in steps_after_approach {
            update(&mut app, ms);
        }
        assert!(
            app.world().get::<Bolt>(bolt).is_none(),
            "the staged projectile reached the Griffin"
        );
        let anim = app.world().get::<Anim>(griffin).unwrap();
        let wing = app
            .world()
            .get::<Rig>(griffin)
            .unwrap()
            .joint(J::WingL)
            .unwrap();
        let rotation = app.world().get::<Transform>(wing).unwrap().rotation;
        (
            anim.hurt.unwrap(),
            anim.dead.unwrap() - before_death,
            rotation,
        )
    };
    let steady = run(&[16, 16, 16, 16, 16, 16, 16, 8]);
    let hitch = run(&[120]);
    assert!(
        (steady.0 - 0.024).abs() < 0.002,
        "steady impact age {}",
        steady.0
    );
    assert!(
        (hitch.0 - steady.0).abs() < 0.002,
        "at bolt arrival the Griffin's hurt clock was {}s, expected {}s",
        hitch.0,
        steady.0
    );
    assert!(
        (hitch.1 - steady.1).abs() < 0.002,
        "death pre-roll follows the same clock as impact"
    );
    assert!(
        hitch.2.angle_between(steady.2) < 0.002,
        "wing release on impact stays in phase"
    );
}

#[test]
fn rattling_blow_lands_one_contact_ring_at_the_end_of_the_windup() {
    let (mut app, caster, target) = scene("rattling_blow", "Rattling Blow", true, true);
    app.world_mut()
        .resource_mut::<Game>()
        .pending
        .push(Happening::Hurt(ActorTarget::from_digest([12; 32])));
    app.update();
    let world = app.world_mut();
    assert_eq!(
        world.get::<Anim>(caster).unwrap().action.unwrap().0,
        Move::Heavy
    );
    assert!((world.get::<Anim>(target).unwrap().hurt.unwrap() + 0.495).abs() < 1e-5);
    assert_eq!(
        world.query::<&Fade>().iter(world).count(),
        1,
        "only the low impact ring, not a chest flash, is staged"
    );
}

#[test]
fn rattling_blow_does_not_stage_contact_on_a_miss_or_at_an_undisclosed_target() {
    for disclose in [true, false] {
        let (mut app, caster, target) = scene("rattling_blow", "Rattling Blow", true, disclose);
        if disclose {
            let mut game = app.world_mut().resource_mut::<Game>();
            let Happening::Ability { outcome, .. } = &mut game.pending[0] else {
                panic!("expected technique");
            };
            *outcome = AbilityOutcome::Miss;
        }
        app.update();
        let world = app.world_mut();
        assert_eq!(
            world.get::<Anim>(caster).unwrap().action.unwrap().0,
            Move::Heavy
        );
        assert!(world.get::<Anim>(target).unwrap().hurt.is_none());
        assert_eq!(world.query::<&Fade>().iter(world).count(), 0);
    }
}
