//! Newly reconstructed content compilation checks; shared status runtime is reused.
use tor_simulation::content_ids::{AbilityId, SpeciesId};

#[test]
fn rogue_chill_is_a_purchased_checked_status_signature_with_no_fake_damage() {
    let (catalog, manifest) = super::rogue_roster::catalog();
    let definition =
        &catalog.content().catalog().species[&SpeciesId::new("rogue.ice_monster").unwrap()];
    let strike = &definition.melee.strikes()[0];
    assert!(strike.check.is_some());
    assert!(strike.on_success.is_empty());
    assert!(strike.on_failure.is_empty());
    let build = manifest.archetypes["rogue.ice_monster"]
        .creature
        .as_ref()
        .unwrap();
    let trained = catalog.prepare(build).unwrap().instantiate(42).unwrap();
    let ability = AbilityId::new("rogue.freezing_touch").unwrap();
    assert!(trained.derive().unwrap().abilities.contains(&ability));
    let mut untrained = build.clone();
    untrained.hit_dice[0].purchases.clear();
    // The package owns its innate purchase. Author a matching unpaid package
    // variant rather than editing a locked purchase under the original package.
    assert!(catalog.prepare(&untrained).is_err());
    let untrained = super::rogue_roster::prepare_roster_variant(&catalog, &manifest, &untrained)
        .unwrap()
        .instantiate(42)
        .unwrap();
    assert!(!untrained.derive().unwrap().abilities.contains(&ability));
    let signature =
        serde_json::to_value(catalog.content().rules().ability(&ability).unwrap()).unwrap();
    assert_eq!(signature["strikes"]["kind"], "natural_melee");
    let rider = &signature["strikes"]["additional_riders"][0];
    assert_eq!(rider["trigger"], "success");
    assert_eq!(rider["effects"][0]["duration"], 200);
}

#[test]
fn production_ice_monster_waits_unprovoked_then_pursues_a_disclosed_attacker() {
    use std::{collections::BTreeSet, num::NonZeroU64};
    use tor_simulation::{
        abilities::AbilityInvocation, ai::AiProfile, Action, ActorId, CreatureIdentity, Game,
    };
    use tor_world::{Extent, Location, Position, Region, RegionId, World};
    let (catalog, manifest) = super::rogue_roster::catalog();
    let mut game = Game::new(
        World::new(
            vec![Region {
                id: RegionId(1),
                name: "Ice pursuit witness".into(),
                bounds: Extent::new(13, 7, 3).unwrap(),
            }],
            vec![],
        )
        .unwrap(),
        42,
        catalog.content().clone(),
    );
    let at = |x| Location {
        region: RegionId(1),
        position: Position { x, y: 3, z: 0 },
    };
    let hero = game
        .spawn_actor(at(4), NonZeroU64::new(100).unwrap())
        .unwrap();
    let ice = game
        .spawn_actor(at(3), NonZeroU64::new(100).unwrap())
        .unwrap();
    let ice_recipe = &manifest.archetypes["rogue.ice_monster"];
    for (actor, spec, body) in [
        (
            hero,
            manifest.characters[0].creature.as_ref().unwrap(),
            manifest.characters[0].body.clone().unwrap(),
        ),
        (
            ice,
            ice_recipe.creature.as_ref().unwrap(),
            ice_recipe.body.clone().unwrap(),
        ),
    ] {
        game.set_body(actor, body.into()).unwrap();
        game.configure_creature(
            actor,
            CreatureIdentity {
                corpse_identity: spec.corpse_identity.clone(),
                name: spec.name.clone(),
                faction: spec.faction.clone(),
            },
            catalog
                .prepare(spec)
                .unwrap()
                .instantiate(tor_server::creature_authoring::actor_health_seed(
                    42, actor.0,
                ))
                .unwrap(),
        )
        .unwrap();
    }
    let mut profile = AiProfile::from(game.creature(ice).unwrap().build().species().behavior);
    // The live rogue_hostile profile overrides only this field.
    profile.flee_percent = 0;
    assert_eq!(profile.idle, tor_simulation::behavior::IdleBehavior::Wait);
    game.configure_ai(ice, profile).unwrap();
    game.configure_run(hero, BTreeSet::from([hero]), manifest.factions.clone())
        .unwrap();
    game.refresh_navigation();
    assert!(game
        .observe(ice)
        .unwrap()
        .visible_actors
        .iter()
        .any(|a| a.id == hero));
    game.act(hero, Action::Wait).unwrap();
    assert_eq!(
        game.next_ai_action(),
        Some((ice, Action::Wait)),
        "visible but unprovoked must remain passive"
    );
    game.admit_ai_intention(ice).unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    for _ in 0..32 {
        if game.next_actor() == Some(hero) {
            break;
        }
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert_eq!(game.next_actor(), Some(hero));
    // The ordinary player build is unarmed: no test-only attack or free provocation.
    game.act(
        hero,
        Action::UseAbility {
            id: AbilityId::new("basic_melee").unwrap(),
            invocation: AbilityInvocation::actor_target(ice),
            supplies: Default::default(),
        },
    )
    .unwrap();
    for _ in 0..32 {
        if game.preparation(hero).is_none() && game.next_actor() == Some(hero) {
            break;
        }
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert!(game.preparation(hero).is_none());
    assert!(
        game.alive(ice),
        "the production ice monster must survive this fixed provocation fixture"
    );
    game.teleport(hero, at(8)).unwrap();
    game.refresh_navigation();
    assert!(game
        .observe(ice)
        .unwrap()
        .visible_actors
        .iter()
        .any(|a| a.id == hero));
    for _ in 0..32 {
        if game.next_actor() == Some(ice) {
            break;
        }
        assert_eq!(game.next_actor(), Some(hero));
        game.act(hero, Action::Wait).unwrap();
    }
    let before = game.observe(ice).unwrap().location;
    let choice = game.next_ai_action().unwrap();
    assert_eq!(choice.0, ice);
    assert!(
        matches!(choice.1, Action::Move(_)),
        "provoked ice monster must pursue: {:?}",
        choice.1
    );
    game.admit_ai_intention(ice).unwrap();
    let committed = game.execute_next_intention().unwrap();
    assert_eq!(committed.action, Some(choice.1));
    committed.outcome.unwrap();
    let after = game.observe(ice).unwrap().location;
    assert!(
        after.position.x > before.position.x,
        "pursuit must make real progress toward the disclosed attacker"
    );
    assert_eq!(hero, ActorId(1));
}
