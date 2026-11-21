//! Adapter boundaries: hidden, duplicate, own and item-handling activity.
use super::*;
use crate::journal::{Command, Direction};
use crate::public_plan_test_fixture as fixture;
use tor_protocol::{ActivityKind, ActorTarget, ItemWorkKind, OutcomeFact, OutcomeObservation};

const OBSERVER: ActorId = ActorId(1);
const SUBJECT: ActorId = ActorId(2);

fn act(engine: &mut Engine, actor: ActorId, action: Action) -> CommandResult {
    engine
        .command(
            "player",
            "test",
            actor,
            &uuid::Uuid::new_v4().to_string(),
            &engine.branch().clone(),
            Command::Act {
                expected_boundary: engine.causal_boundary(actor).unwrap(),
                action,
            },
        )
        .unwrap()
}

fn view(engine: &Engine) -> OutcomeObservation {
    engine.semantic_observation(OBSERVER).unwrap().facts
}

fn handle(engine: &Engine) -> ActorTarget {
    engine.target_scope(OBSERVER).actor(SimActor(SUBJECT.0))
}

fn hide(engine: &Engine, view: &OutcomeObservation) -> OutcomeObservation {
    let handle = handle(engine);
    let mut view = view.clone();
    view.visible_actors.retain(|row| row.id != handle);
    view
}

fn ready_engine() -> Engine {
    let mut engine = Engine::memory(fixture::scenario(false)).unwrap();
    act(&mut engine, OBSERVER, Action::Wait);
    engine
}

#[test]
fn hidden_duplicate_and_own_activities_disclose_nothing() {
    let mut engine = ready_engine();
    let before = view(&engine);
    let result = act(
        &mut engine,
        SUBJECT,
        Action::Move {
            direction: Direction::East,
        },
    );
    let after = view(&engine);
    assert_eq!(
        engine
            .outcome_activities(&result, OBSERVER, &before, &after)
            .len(),
        1,
        "positive witness"
    );
    assert!(engine
        .outcome_activities(
            &result,
            OBSERVER,
            &hide(&engine, &before),
            &hide(&engine, &after)
        )
        .is_empty());
    // Leaving sight keeps the disclosed start and a null destination.
    assert!(matches!(
        engine
            .outcome_activities(&result, OBSERVER, &before, &hide(&engine, &after))
            .as_slice(),
        [OutcomeFact::Activity {
            activity: ActivityKind::Move {
                from: Some(_),
                to: None
            },
            ..
        }]
    ));
    let duplicate = CommandResult {
        duplicate: true,
        ..result.clone()
    };
    assert!(engine
        .outcome_activities(&duplicate, OBSERVER, &before, &after)
        .is_empty());
    // The acting observer itself never receives an activity fact.
    assert!(engine
        .outcome_activities(&result, SUBJECT, &before, &after)
        .is_empty());
}

#[test]
fn wind_up_needs_the_creature_sighting_already_in_the_observers_view() {
    let mut engine = ready_engine();
    let before = view(&engine);
    let result = act(
        &mut engine,
        SUBJECT,
        Action::UseAbility {
            id: tor_simulation::content_ids::AbilityId::new("slow_plan").unwrap(),
            invocation: crate::journal::AbilityInvocation {
                aim: crate::journal::AbilityAim::ActorTarget {
                    target: SimActor(OBSERVER.0),
                },
                weapon: crate::journal::WeaponSelection::Default,
            },
            supplies: Default::default(),
        },
    );
    let after = view(&engine);
    assert!(matches!(
        engine
            .outcome_activities(&result, OBSERVER, &before, &after)
            .as_slice(),
        [OutcomeFact::Activity {
            activity: ActivityKind::WindUp { .. },
            ..
        }]
    ));
    // The same body perceived only as an anonymous contact discloses no plan.
    let contact = engine.target_scope(OBSERVER).contact(SimActor(SUBJECT.0));
    let creature = handle(&engine);
    let as_contact = |view: &OutcomeObservation| {
        let mut view = view.clone();
        for row in &mut view.visible_actors {
            if row.id == creature {
                row.id = contact;
            }
        }
        view
    };
    assert!(engine
        .outcome_activities(&result, OBSERVER, &as_contact(&before), &as_contact(&after))
        .is_empty());
    assert!(engine
        .outcome_activities(&result, OBSERVER, &before, &hide(&engine, &after))
        .is_empty());
}

#[test]
fn visible_item_handling_uses_the_observers_own_item_name() {
    let mut engine = ready_engine();
    let before = view(&engine);
    let result = act(&mut engine, SUBJECT, Action::Wait);
    let after = view(&engine);
    // Independent oracle: the observer's ordinary projection of the floor token.
    let (scene, _) = engine.game.observe_scene(SimActor(OBSERVER.0)).unwrap();
    let token = scene.ground_items.first().expect("fixture token");
    let JournalContent::Action { event, .. } = &result.entry.content else {
        panic!("direct action entry")
    };
    let mut drink = result.clone();
    drink.entry.content = JournalContent::Action {
        action: Action::Drink { item: token.id.0 },
        event: event.clone(),
    };
    assert_eq!(
        engine.outcome_activities(&drink, OBSERVER, &before, &after),
        [OutcomeFact::Activity {
            actor: handle(&engine),
            started_at: result.entry.tick,
            activity: ActivityKind::ItemHandling {
                work: ItemWorkKind::Drink,
                name: Some(token.name.clone()),
            },
        }]
    );
    assert!(engine
        .outcome_activities(&drink, OBSERVER, &before, &hide(&engine, &after))
        .is_empty());
}
