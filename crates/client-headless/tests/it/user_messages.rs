//! Synthetic server user message -> production WebSocket protocol -> actual
//! headless executable. The fixture authors the message; the real client
//! validates it and reports it as JSON without inventing key bindings.
use super::effects::{context, cursor, state, Client};
use serde_json::json;
use tor_protocol::*;

#[tokio::test]
async fn outcome_user_messages_reach_real_headless_as_json_with_neutral_control_names() {
    let mut client = Client::start_with_deferred_map(state(0, vec![]), true).await;
    client
        .send
        .send(ServerMessage::Update {
            update: Box::new(StreamUpdate {
                context: context(),
                actor: ActorId(1),
                branch: BranchId("fixture".into()),
                cursor: cursor(1),
                body: UpdateBody::Outcome {
                    visual_changed: false,
                    event: None,
                    facts: OutcomeFacts {
                        self_target: ActorTarget::from_digest([1; 32]),
                        status: None,
                        actor_names: vec![],
                        facts: vec![
                            OutcomeFact::UserMessage {
                                style: UserMessageStyle::Tutorial,
                                segments: vec![
                                    UserMessageSegment::Text("Press ".into()),
                                    UserMessageSegment::Control(ControlHint::PickUp),
                                    UserMessageSegment::Text(" to take it.".into()),
                                ],
                            },
                            OutcomeFact::UserMessage {
                                style: UserMessageStyle::Narration,
                                segments: vec![UserMessageSegment::Text("Rain falls.".into())],
                            },
                        ],
                    },
                },
            }),
        })
        .unwrap();
    let frame = client
        .until(|v| v["message"]["update"]["body"]["type"] == "outcome")
        .await;
    assert_eq!(
        frame["user_messages"],
        json!([
            {
                "style": "tutorial",
                "text": "Press [pick up] to take it.",
                "segments": [{"text": "Press "}, {"control": "pick_up"}, {"text": " to take it."}],
            },
            {
                "style": "narration",
                "text": "Rain falls.",
                "segments": [{"text": "Rain falls."}],
            },
        ])
    );
    assert_eq!(
        frame["narration"],
        json!(["Press [pick up] to take it.", "Rain falls."])
    );
    let map = client.until(|v| v["message"]["type"] == "map_page").await;
    assert_eq!(map["message"]["context"]["cursor"]["sequence"], "1");
    // Messages belong to the outcome that carried them, not later frames.
    let inspected = client.inspect().await;
    assert_eq!(inspected["user_messages"], json!([]));
}
