//! Complete disclosed Gold decisions: candidate density and remembered topology.
#[allow(dead_code)]
#[path = "../test_support/creatures.rs"]
mod subjects;
use std::{hint::black_box, num::NonZeroU64, time::Instant};
use tor_simulation::{
    ai::AiProfile, behavior::IdleBehavior, Action, CurrencyKind, Game, ItemClass, ItemSpec,
};
use tor_world::{Extent, Location, Position, Region, RegionId, World};
fn main() {
    for width in [8, 32] {
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
                let species = subjects::species();
                let mut game =
                    Game::new(world, 42, subjects::content_with(species.clone(), vec![]));
                let at = Location {
                    region: RegionId(1),
                    position: Position { x: 1, y: 4, z: 0 },
                };
                let actor = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
                subjects::configure(&mut game, actor, "neutral", species);
                let goal = Location {
                    position: Position {
                        x: 1 + distance,
                        ..at.position
                    },
                    ..at
                };
                for index in 0..count {
                    let mut item = ItemSpec::ordinary(
                        format!("item.test.coin-{index}").parse().unwrap(),
                        format!("coin {index}"),
                    );
                    item.class = ItemClass::Coin;
                    item.stackable = true;
                    item.currency = Some(CurrencyKind::Gold);
                    game.place_item_stack(100 + index, goal, None, 7, item)
                        .unwrap();
                }
                game.configure_ai(
                    actor,
                    AiProfile {
                        idle: IdleBehavior::Explore,
                        gold: true,
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
                        serde_json::json!({"version":1,"seed":42,"width":width,"distance":distance,"items":count,"sample":sample,"decision_ms":decision_ms,
                        "route_searches":end.route_searches-start_counts.route_searches,
                        "game_clones":end.game_clones-start_counts.game_clones,
                        "observation_cells_copied":end.observation_cells_copied-start_counts.observation_cells_copied})
                    );
                }
            }
        }
    }
}
