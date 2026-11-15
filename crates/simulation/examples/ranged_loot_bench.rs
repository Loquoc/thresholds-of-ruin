//! Complete ranged acquisition decisions; compatible payload and unmatched launcher density.
#[allow(dead_code)]
#[path = "../test_support/creatures.rs"]
mod subjects;
use std::{hint::black_box, num::NonZeroU64, time::Instant};
use tor_simulation::{ai::AiProfile, behavior::IdleBehavior, Action, Game, ItemClass, ItemSpec};
use tor_world::{Extent, Location, Position, Region, RegionId, World};
fn main() {
    for mode in ["payload", "unmatched"] {
        let width = 16;
        for distance in [4, 6] {
            for count in [1_u64, 16, 128] {
                let mut world = World::new(vec![], vec![]).unwrap();
                world
                    .add_region(Region {
                        id: RegionId(1),
                        name: "loot measurement".into(),
                        bounds: Extent::new(width, 9, 1).unwrap(),
                    })
                    .unwrap();
                let content = fixture_content();
                let mut game = Game::new(world, 42, content.clone());
                let at = Location {
                    region: RegionId(1),
                    position: Position { x: 1, y: 4, z: 0 },
                };
                let actor = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
                let build = tor_simulation::creatures::CreatureBuild::new(
                    content,
                    tor_simulation::content_ids::SpeciesId::new("unit_subject").unwrap(),
                    tor_simulation::progression::HdLedger::new(vec![
                        tor_simulation::progression::HitDie::new(
                            tor_simulation::progression::HdSource::Racial,
                            42,
                        ),
                    ])
                    .unwrap(),
                    tor_simulation::attributes::ManaBinding::Intellect,
                    [
                        tor_simulation::attributes::Skill::HeavyWeaponry,
                        tor_simulation::attributes::Skill::LightWeaponry,
                    ],
                )
                .unwrap();
                game.configure_creature(
                    actor,
                    tor_simulation::CreatureIdentity {
                        corpse_identity: "item.corpse.fixture".parse().unwrap(),
                        name: "fixture".into(),
                        faction: "neutral".into(),
                    },
                    build,
                )
                .unwrap();
                let goal = Location {
                    position: Position {
                        x: 1 + distance,
                        ..at.position
                    },
                    ..at
                };
                if mode == "payload" {
                    let owned = game
                        .spawn_item_stack(at, Some(actor), 1, bow(999, "fixture.owned"))
                        .unwrap();
                    game.equip_authored(actor, owned, tor_simulation::EquipmentSlotId(0))
                        .unwrap();
                    game.spawn_item_stack(at, Some(actor), 2, payload(999, "fixture.owned"))
                        .unwrap();
                }
                for index in 0..count {
                    let item = if mode == "payload" {
                        payload(index, "fixture.owned")
                    } else {
                        bow(index, &format!("fixture.unmatched-{index}"))
                    };
                    game.place_item_stack(
                        100 + index,
                        goal,
                        None,
                        if mode == "payload" { 7 } else { 1 },
                        item,
                    )
                    .unwrap();
                }
                game.configure_ai(
                    actor,
                    AiProfile {
                        idle: IdleBehavior::Explore,
                        equipment: true,
                        loot: true,
                        ..Default::default()
                    },
                )
                .unwrap();
                game.refresh_navigation();
                let view = game.observe(actor).unwrap();
                assert_eq!(
                    view.ground_items.len(),
                    count as usize,
                    "all candidates genuinely disclosed"
                );
                let before = game.clone();
                for _ in 0..3 {
                    assert!(
                        matches!(game.next_ai_action(), Some((id, Action::Move(_))) if id == actor)
                    );
                }
                for sample in 0..20 {
                    let start_counts = tor_simulation::diagnostics::work_counts();
                    let started = Instant::now();
                    let action = black_box(game.next_ai_action());
                    let decision_ms = started.elapsed().as_secs_f64() * 1000.;
                    let end = tor_simulation::diagnostics::work_counts();
                    assert!(matches!(action, Some((id, Action::Move(_))) if id == actor));
                    assert_eq!(game, before);
                    println!(
                        "{}",
                        serde_json::json!({"version":1,"seed":42,"mode":mode,"distance":distance,"items":count,"sample":sample,"decision_ms":decision_ms,
                        "route_searches":end.route_searches-start_counts.route_searches,
                        "game_clones":end.game_clones-start_counts.game_clones,
                        "observation_cells_copied":end.observation_cells_copied-start_counts.observation_cells_copied})
                    );
                }
            }
        }
    }
}
fn fixture_content() -> std::sync::Arc<tor_simulation::creatures::Content> {
    use tor_simulation::{resources::Resource, rules::*};
    let mut document = subjects::ruleset().document().clone();
    let mut launch = document
        .abilities
        .iter()
        .find(|a| a.basic_attack)
        .unwrap()
        .clone();
    launch.id = tor_simulation::content_ids::AbilityId::new("fixture.launch").unwrap();
    launch.basic_attack = false;
    launch.invocation = tor_simulation::abilities::InvocationKind::DirectionalProjectile;
    launch.reach = Reach::Projectile;
    launch.strikes = StrikeSource::Projectile {
        delivery: tor_simulation::projectile_profiles::ProjectileDeliveryKind::Launch,
    };
    document.constants.default_abilities.push(launch.id.clone());
    document.abilities.push(launch);
    document
        .expertises
        .push(tor_simulation::expertise::ExpertiseDef {
            id: tor_simulation::content_ids::ExpertiseId::new("fixture.bow").unwrap(),
            name: "fixture bow".into(),
            category: tor_simulation::expertise::ExpertiseCategory::Equipment,
        });
    for definition in &mut document.resources {
        definition.capacity.required_flag = None;
    }
    let mut species = subjects::species();
    for resource in Resource::ALL {
        species.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Set {
                stat: Stat::ResourceMax { resource },
                value: 20,
            },
        });
    }
    tor_simulation::creatures::Content::new(
        Ruleset::validate(document).unwrap(),
        subjects::content_with(species, vec![]).catalog().clone(),
    )
    .unwrap()
}
fn payload(index: u64, family: &str) -> ItemSpec {
    let mut item = ItemSpec::ordinary(
        format!("item.test.payload-{index}").parse().unwrap(),
        "payload".into(),
    );
    item.class = ItemClass::Weapon;
    item.stackable = true;
    let damage = tor_simulation::damage::DamageComponent::fixed(
        tor_simulation::combat::DamageType::Keen,
        None,
        3,
    );
    item.projectile = Some(tor_simulation::projectile_profiles::ProjectileSpec {
        family: tor_simulation::content_ids::LauncherFamilyId::new(family).unwrap(),
        damage: tor_simulation::damage::DamageSpec::new(vec![damage.clone()], Some(damage.key()))
            .unwrap(),
        recovery: tor_simulation::projectile_profiles::HitRecovery::ConsumedOnHit,
    });
    item
}
fn bow(index: u64, family: &str) -> ItemSpec {
    use tor_simulation::weapon_profiles::*;
    let mut item = ItemSpec::ordinary(
        format!("item.test.bow-{index}").parse().unwrap(),
        "bow".into(),
    );
    item.class = ItemClass::Weapon;
    item.equipment = Some(tor_simulation::EquipmentSpec {
        periodic_relocation: None,
        potency_modifiers: vec![],
        slot: tor_simulation::EquipmentSlot::new("weapon").unwrap(),
        armor: None,
        attack: Some(subjects::melee(
            0,
            60,
            tor_simulation::combat::DamageType::Impact,
            1,
        )),
        modifiers: vec![],
        weapon_profile: Some(WeaponProfileSpec {
            expertise: tor_simulation::content_ids::ExpertiseId::new("fixture.bow").unwrap(),
            launcher: Some(LauncherProfile {
                family: tor_simulation::content_ids::LauncherFamilyId::new(family).unwrap(),
                range: RangeBands {
                    short_feet: 80,
                    long_feet: 320,
                },
            }),
            thrown_payload: None,
            base_traits: vec![],
            expert_traits: WeaponTraitChanges {
                add: vec![],
                remove: vec![],
            },
        }),
    });
    item
}
