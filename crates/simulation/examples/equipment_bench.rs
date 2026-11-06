//! Equipment-decision workload v1: complete AI choice, not ranking in isolation.
#[allow(dead_code)]
#[path = "../test_support/creatures.rs"]
mod subjects;
use std::{hint::black_box, num::NonZeroU64, time::Instant};
use tor_simulation::{
    ai::AiProfile,
    attributes::{Attributes, ByAttribute},
    behavior::IdleBehavior,
    rules::{DefenseKind, Modifier, ModifierOp, Scope, Stat},
    Action, EquipmentSlot, EquipmentSlotId, EquipmentSpec, Game, ItemClass, ItemId, ItemSpec,
};

use tor_world::Location;

fn main() {
    for count in [1_u64, 4, 16, 64] {
        let mut species = subjects::species();
        species.default_attributes = Attributes::new(ByAttribute {
            strength: 5,
            speed: 1,
            ..Default::default()
        })
        .unwrap();
        let mut game = Game::two_room_in_stone(42, subjects::content_with(species.clone(), vec![]));
        let at = Location {
            region: tor_world::RegionId(1),
            position: tor_world::Position { x: 1, y: 1, z: 0 },
        };
        let actor = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
        subjects::configure(&mut game, actor, "neutral", species);
        for index in 0..count {
            let id = 100 + index;
            let mut item = ItemSpec::ordinary(
                format!("item.test.gear-{id}").parse().unwrap(),
                format!("gear {id}"),
            );
            item.class = ItemClass::Armor;
            item.equipment = Some(EquipmentSpec {
                periodic_relocation: None,
                potency_modifiers: vec![],
                weapon_profile: None,
                armor: None,
                slot: EquipmentSlot::new("head_armor").unwrap(),
                attack: None,
                modifiers: vec![Modifier {
                    cancellable: false,
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::Defense {
                            defense: DefenseKind::Cognitive,
                        },
                        value: index as i64 + 1,
                    },
                }],
            });
            game.place_item_stack(id, at, Some(actor), 1, item).unwrap();
        }
        game.configure_ai(
            actor,
            AiProfile {
                idle: IdleBehavior::Wait,
                equipment: true,
                ..Default::default()
            },
        )
        .unwrap();
        game.refresh_navigation();
        let expected = Some((
            actor,
            Action::Equip {
                item: ItemId(99 + count),
                slot: EquipmentSlotId(3),
            },
        ));
        let before = game.clone();
        for _ in 0..3 {
            assert_eq!(game.next_ai_action(), expected);
        }
        for sample in 0..20 {
            let start_counts = tor_simulation::diagnostics::work_counts();
            let started = Instant::now();
            let action = black_box(game.next_ai_action());
            let decision_ms = started.elapsed().as_secs_f64() * 1000.;
            let end_counts = tor_simulation::diagnostics::work_counts();
            assert_eq!(action, expected);
            assert_eq!(game, before, "choice must not mutate authoritative state");
            println!(
                "{}",
                serde_json::json!({"version":1,"seed":42,"items":count,"sample":sample,
                "decision_ms":decision_ms,"selected_item":99+count,
                "gear_projections":end_counts.gear_projections-start_counts.gear_projections,
                "game_clones":end_counts.game_clones-start_counts.game_clones,
                "dynamic_folds":end_counts.dynamic_creature_folds-start_counts.dynamic_creature_folds})
            );
        }
    }
}
