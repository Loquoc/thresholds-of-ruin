//! Activity facts disclose only what watching another actor would show.
use super::*;
use crate::journal::{AbilityAim, AbilityInvocation, Action, Command, Direction, WeaponSelection};
use crate::public_plan_test_fixture as fixture;
use crate::CommandResult;
use tor_protocol::{ActivityKind, ActivityPosture, ActorTarget, Position};

const OBSERVER: ActorId = ActorId(1);
const SUBJECT: ActorId = ActorId(2);

fn attached(role: AccessRole) -> (Service, Connection) {
    let mut service = Service::new(Engine::memory(fixture::scenario(false)).unwrap());
    let account = Account {
        user: "observer".into(),
        token: "test-only".into(),
        role,
        actors: BTreeSet::from([OBSERVER]),
    };
    let mut client = service.connect(&account, "headless".into()).unwrap();
    service.handle(
        client.id,
        "attach".into(),
        Request::Attach { actor: OBSERVER },
    );
    while client.messages.try_recv().is_ok() {}
    (service, client)
}

/// Commit one direct action and publish it as the live service does.
fn act(service: &mut Service, actor: ActorId, action: Action) -> CommandResult {
    let before = service.semantic_states().unwrap();
    let result = service
        .engine
        .command(
            "player",
            "test",
            actor,
            &uuid::Uuid::new_v4().to_string(),
            &service.engine.branch().clone(),
            Command::Act {
                expected_boundary: service.engine.causal_boundary(actor).unwrap(),
                action,
            },
        )
        .unwrap();
    service.action_result_update(&before, &result).unwrap();
    result
}

fn activities(client: &mut Connection) -> Vec<(ActorTarget, u64, ActivityKind, u64)> {
    let mut found = Vec::new();
    while let Ok(message) = client.messages.try_recv() {
        let ServerMessage::Update { update } = message else {
            continue;
        };
        let UpdateBody::Outcome { facts, .. } = update.body else {
            continue;
        };
        for fact in facts.facts {
            if let OutcomeFact::Activity {
                actor,
                started_at,
                activity,
            } = fact
            {
                found.push((actor, started_at, activity, update.cursor.tick));
            }
        }
    }
    found
}

fn subject_handle(service: &Service) -> ActorTarget {
    service
        .engine
        .target_scope(OBSERVER)
        .actor(tor_simulation::ActorId(SUBJECT.0))
}

fn subject_offset(service: &Service) -> Position {
    let handle = subject_handle(service);
    service
        .engine
        .state(OBSERVER)
        .unwrap()
        .observation
        .visible_actors
        .iter()
        .filter(|row| row.id == handle)
        .map(|row| row.position)
        .min_by_key(|p| {
            (
                p.z,
                p.x.unsigned_abs() + p.y.unsigned_abs() + p.z.unsigned_abs(),
            )
        })
        .expect("subject is visible")
}

fn slow_plan() -> Action {
    Action::UseAbility {
        id: tor_simulation::content_ids::AbilityId::new("slow_plan").unwrap(),
        invocation: AbilityInvocation {
            aim: AbilityAim::ActorTarget {
                target: tor_simulation::ActorId(OBSERVER.0),
            },
            weapon: WeaponSelection::Default,
        },
        supplies: Default::default(),
    }
}

#[test]
fn watched_step_is_disclosed_with_both_event_time_cells_and_own_actions_are_not() {
    for role in [AccessRole::Player, AccessRole::Spectator] {
        let (mut service, mut client) = attached(role);
        // The observer's own wait and step keep their own-action facts only.
        act(&mut service, OBSERVER, Action::Wait);
        assert!(activities(&mut client).is_empty(), "{role:?}");
        act(&mut service, SUBJECT, Action::Wait);
        assert!(activities(&mut client).is_empty(), "waiting is not public");
        act(&mut service, OBSERVER, Action::Wait);
        activities(&mut client);
        let from = subject_offset(&service);
        let result = act(
            &mut service,
            SUBJECT,
            Action::Move {
                direction: Direction::East,
            },
        );
        let to = subject_offset(&service);
        assert_eq!(
            to,
            Position {
                x: from.x + 1,
                ..from
            },
            "east step"
        );
        let found = activities(&mut client);
        assert_eq!(
            found
                .iter()
                .map(|(actor, start, kind, _)| (*actor, *start, kind.clone()))
                .collect::<Vec<_>>(),
            [(
                subject_handle(&service),
                result.entry.tick,
                ActivityKind::Move {
                    from: Some(from),
                    to: Some(to),
                },
            )],
            "{role:?}"
        );
        // Publication follows the walk to the next decision, so the update
        // tick may be later: the explicit start is what times the step.
        assert!(found[0].3 >= found[0].1, "{role:?}");
    }
}

#[test]
fn public_wind_up_discloses_only_its_category() {
    let (mut service, mut client) = attached(AccessRole::Player);
    act(&mut service, OBSERVER, Action::Wait);
    activities(&mut client);
    let before = service.semantic_states().unwrap();
    let result = service
        .engine
        .command(
            "player",
            "test",
            SUBJECT,
            "wind-up",
            &service.engine.branch().clone(),
            Command::Act {
                expected_boundary: service.engine.causal_boundary(SUBJECT).unwrap(),
                action: slow_plan(),
            },
        )
        .unwrap();
    service.action_result_update(&before, &result).unwrap();
    let mut outcomes = Vec::new();
    while let Ok(message) = client.messages.try_recv() {
        if let ServerMessage::Update { update } = message {
            if let UpdateBody::Outcome { facts, .. } = update.body {
                outcomes.push(facts);
            }
        }
    }
    let found: Vec<_> = outcomes
        .iter()
        .flat_map(|facts| &facts.facts)
        .filter_map(|fact| match fact {
            OutcomeFact::Activity {
                actor,
                started_at,
                activity,
            } => Some((*actor, *started_at, activity.clone())),
            _ => None,
        })
        .collect();
    // slow_plan has sight reach, so its stance is casting.
    assert_eq!(
        found,
        [(
            subject_handle(&service),
            result.entry.tick,
            ActivityKind::WindUp {
                posture: ActivityPosture::Casting,
            },
        )]
    );
    // No outcome names the ability; the activity itself carries no timing.
    let encoded = serde_json::to_string(&outcomes).unwrap();
    for private in ["slow_plan", "Slow Plan"] {
        assert!(!encoded.contains(private), "{private}");
    }
    let activity = serde_json::to_value(
        outcomes
            .iter()
            .flat_map(|facts| &facts.facts)
            .find(|fact| matches!(fact, OutcomeFact::Activity { .. }))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        activity,
        serde_json::json!({
            "kind": "activity",
            "actor": subject_handle(&service),
            "started_at": result.entry.tick.to_string(),
            "activity": {"type": "wind_up", "posture": "casting"},
        })
    );
}
