//! Synthetic effect producer -> production WebSocket protocol -> actual headless
//! executable. These are NOT Rogue mechanics or simulation visibility tests:
//! the fixture chooses disclosed cells, while the real client validates them.
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    time::Duration,
};
use tokio::{net::TcpListener, sync::mpsc, time::timeout};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tor_protocol::*;

const WAIT: Duration = Duration::from_secs(15);
fn context() -> StreamContext {
    StreamContext {
        stream: StreamId("synthetic-effects".into()),
        epoch: 0,
    }
}
fn cursor(tick: u64) -> StreamCursor {
    StreamCursor {
        sequence: tick,
        tick,
    }
}
fn point(x: i32, y: i32) -> Position {
    Position { x, y, z: 0 }
}
fn key(p: Position) -> String {
    format!("{:040x}", (p.y + 10) * 100 + p.x + 10)
}
fn effect(id: u8, since: u64, expiry: Option<u64>, points: &[(i32, i32)]) -> PublicEffectView {
    let points: Vec<_> = points.iter().map(|&(x, y)| point(x, y)).collect();
    PublicEffectView {
        id: format!("{id:040x}"),
        observed_since: since,
        expires_at: expiry,
        delivery: EffectDelivery::Area,
        appearance: EffectAppearance::Cloud,
        cue: EffectCue::Poison,
        anchors: points
            .iter()
            .map(|&position| EffectAnchor {
                key: key(position),
                position,
            })
            .collect(),
        segments: vec![points],
    }
}
fn state(tick: u64, effects: Vec<PublicEffectView>) -> StateView {
    let cells: Vec<_> = (-2..=2).flat_map(|y| (-2..=2).map(move |x| {
        let p=point(x,y);
        json!({"key":key(p),"position":p,"trap":null,"object_presentation":null,
            "stairs_up":false,"stairs_down":false,"wall":false,"lit":true,"place_hint":false,"asset":"terrain.floor.cave"})
    })).collect();
    let mut state: StateView=serde_json::from_value(json!({"snapshot":format!("effects-{tick}"),
        "wizard_game":false,"revision":tick.to_string(),"observation":{
            "actor":"1","self_target":ActorTarget::from_digest([1;32]),"tick":tick.to_string(),
            "position":point(0,0),"places":[],"visible_cells":cells,"ground_items":[],"inventory":[],
            "charts":[],"linked_holds":[],"public_effects":effects,"visible_actors":[],"ready":true}})).unwrap();
    state
        .observation
        .visible_cells
        .sort_by_key(|cell| cell.position);
    state.validate().unwrap();
    state
}
fn update(next: &StateView, base: Option<&StateView>) -> ServerMessage {
    let body = if let Some(base) = base {
        UpdateBody::ObservationDelta {
            base: ObservationBase {
                snapshot: base.snapshot.clone(),
                cursor: cursor(base.revision),
                revision: base.revision,
            },
            state: Box::new(StateDelta::between(base, next).unwrap()),
            event: None,
        }
    } else {
        UpdateBody::Observation {
            state: next.clone().into(),
            event: None,
        }
    };
    ServerMessage::Update {
        update: Box::new(StreamUpdate {
            context: context(),
            actor: ActorId(1),
            branch: BranchId("fixture".into()),
            cursor: cursor(next.revision),
            body,
        }),
    }
}
struct Client {
    child: Child,
    lines: mpsc::UnboundedReceiver<Value>,
    send: mpsc::UnboundedSender<ServerMessage>,
    peer: tokio::task::JoinHandle<()>,
    recovery: mpsc::UnboundedReceiver<()>,
}
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.peer.abort();
    }
}
impl Client {
    async fn start(initial: StateView) -> Self {
        Self::start_with_deferred_map(initial, false).await
    }
    async fn start_with_deferred_map(initial: StateView, defer_map: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (send, mut incoming) = mpsc::unbounded_channel::<ServerMessage>();
        let (recovery_send, recovery) = mpsc::unbounded_channel();
        let peer = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(socket).await.unwrap();
            let hello = ws.next().await.unwrap().unwrap().into_text().unwrap();
            assert!(matches!(
                serde_json::from_str::<ClientMessage>(&hello).unwrap(),
                ClientMessage::Hello { .. }
            ));
            let welcome = ServerMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                user: "fixture".into(),
                actors: vec![ActorId(1)],
                role: AccessRole::Spectator,
            };
            ws.send(Message::Text(
                serde_json::to_string(&welcome).unwrap().into(),
            ))
            .await
            .unwrap();
            let request = ws.next().await.unwrap().unwrap().into_text().unwrap();
            let ClientMessage::Request { request_id, .. } = serde_json::from_str(&request).unwrap()
            else {
                panic!("expected attach")
            };
            let initial_cursor = cursor(initial.revision);
            let mut disclosed_cursor = initial_cursor;
            let snapshot = ServerMessage::Snapshot {
                request_id,
                snapshot: Box::new(Snapshot {
                    readiness: Readiness {
                        revision: 0,
                        admission: false,
                        resume: vec![],
                        cancel: vec![],
                    },
                    context: context(),
                    intentions: vec![],
                    actor: ActorId(1),
                    branch: BranchId("fixture".into()),
                    cursor: cursor(initial.revision),
                    has_control: false,
                    history: HistoryPage {
                        entries: vec![],
                        older_before: None,
                    },
                    state: initial.into(),
                    travel: None,
                }),
            };
            ws.send(Message::Text(
                serde_json::to_string(&snapshot).unwrap().into(),
            ))
            .await
            .unwrap();
            loop {
                tokio::select! {
                    message=incoming.recv()=>{let Some(message)=message else {break};
                        // Reply context follows every disclosed update, including Outcomes
                        // that leave the observation base unchanged.
                        if let ServerMessage::Update { update } = &message {
                            disclosed_cursor = update.cursor;
                        }
                        ws.send(Message::Text(serde_json::to_string(&message).unwrap().into())).await.unwrap();},
                    request=ws.next()=>{let Some(Ok(Message::Text(text)))=request else {break};
                        let ClientMessage::Request{request_id,request}=serde_json::from_str(&text).unwrap() else {continue};
                        // Force the transient-effect regression to exercise a map reply
                        // after an Outcome but against the unchanged observation base.
                        if defer_map && matches!(&*request, Request::Map { .. })
                            && disclosed_cursor == initial_cursor {
                            let message = incoming.recv().await.unwrap();
                            if let ServerMessage::Update { update } = &message {
                                disclosed_cursor = update.cursor;
                            }
                            ws.send(Message::Text(serde_json::to_string(&message).unwrap().into())).await.unwrap();
                        }
                        let reply=match *request {
                            Request::Map{query}=>ServerMessage::MapPage { context:ReplyContext {
                                snapshot:query.base.snapshot.clone(),input:InputContext{stream:query.context.clone(),readiness_revision:0},
                                actor:ActorId(1),branch:query.branch.clone(),cursor:disclosed_cursor,revision:query.base.revision},
                                request_id:request_id.clone(),page:Box::new(MapPage{transfer:request_id,actor:ActorId(1),branch:query.branch,
                                base:query.base,window:query.window,start:0,page:0,total_cells:0,complete:true,ambiguous:false,
                                limited:false,cells:vec![],next:None})},
                            Request::Snapshot=>{recovery_send.send(()).unwrap();continue},
                            // No presentation lookup is required to preserve effect semantics.
                            _=>continue,
                        };ws.send(Message::Text(serde_json::to_string(&reply).unwrap().into())).await.unwrap();
                    }
                }
            }
        });
        let mut child = Command::new(env!("CARGO_BIN_EXE_tor-client-headless"))
            .args([
                "--connect",
                &address.to_string(),
                "--observe",
                "--omit-memory",
            ])
            .env("TOR_SERVER_TOKEN", "synthetic-test-token")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (out, lines) = mpsc::unbounded_channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let value = serde_json::from_str(&line.unwrap()).unwrap();
                if out.send(value).is_err() {
                    break;
                }
            }
        });
        let mut client = Self {
            child,
            lines,
            send,
            peer,
            recovery,
        };
        client.until(|v| v["type"] == "ready").await;
        client
    }
    async fn until(&mut self, predicate: impl Fn(&Value) -> bool) -> Value {
        timeout(WAIT, async {
            loop {
                let value = self
                    .lines
                    .recv()
                    .await
                    .expect("headless exited before expected frame");
                assert!(value["error"].is_null(), "headless error: {value}");
                if predicate(&value) {
                    return value;
                }
            }
        })
        .await
        .expect("headless effect contract timed out")
    }
    async fn observe(&mut self, next: &StateView, base: Option<&StateView>) -> Value {
        self.send.send(update(next, base)).unwrap();
        let revision = next.revision.to_string();
        self.until(|v| v["state"]["revision"].as_str() == Some(revision.as_str()))
            .await
    }
    async fn inspect(&mut self) -> Value {
        writeln!(
            self.child.stdin.as_mut().unwrap(),
            "{{\"type\":\"inspect\"}}"
        )
        .unwrap();
        self.until(|v| v["type"] == "ready").await
    }
}
fn observed(value: &Value) -> &Value {
    &value["state"]["observation"]["public_effects"]
}

#[tokio::test]
async fn burst_and_cone_preserve_exact_authoritative_footprints_in_real_headless() {
    let mut client = Client::start(state(0, vec![])).await;
    let mut burst = effect(1, 1, Some(2), &[(0, -1), (-1, 0), (0, 0), (1, 0), (0, 1)]);
    burst.appearance = EffectAppearance::Flash;
    burst.cue = EffectCue::Fire;
    let first = state(1, vec![burst]);
    let frame = client.observe(&first, None).await;
    assert_eq!(
        observed(&frame)[0]["segments"],
        json!([[{"x":0,"y":-1,"z":0},{"x":-1,"y":0,"z":0},
        {"x":0,"y":0,"z":0},{"x":1,"y":0,"z":0},{"x":0,"y":1,"z":0}]])
    );
    let mut cone = effect(2, 2, None, &[(1, 0), (2, -1), (2, 0), (2, 1)]);
    cone.appearance = EffectAppearance::Flash;
    cone.cue = EffectCue::Cold;
    let second = state(2, vec![cone]);
    let frame = client.observe(&second, Some(&first)).await;
    assert_eq!(
        observed(&frame)[0]["segments"],
        json!([[{"x":1,"y":0,"z":0},{"x":2,"y":-1,"z":0},
        {"x":2,"y":0,"z":0},{"x":2,"y":1,"z":0}]])
    );
    assert_eq!(observed(&frame)[0]["expires_at"], Value::Null);
}

#[tokio::test]
async fn cloud_replaces_updates_loses_visibility_and_reenters_without_hidden_history() {
    let first = state(5, vec![effect(1, 5, Some(10), &[(0, 0), (1, 0)])]);
    let mut client = Client::start(first.clone()).await;
    let frame = client.inspect().await;
    assert_eq!(observed(&frame)[0]["observed_since"], "5");
    assert_eq!(observed(&frame)[0]["expires_at"], "10");
    let moved = state(6, vec![effect(1, 5, Some(10), &[(1, 0), (1, 1)])]);
    let frame = client.observe(&moved, Some(&first)).await;
    assert_eq!(observed(&frame).as_array().unwrap().len(), 1);
    assert_eq!(
        observed(&frame)[0]["segments"],
        json!([[{"x":1,"y":0,"z":0},{"x":1,"y":1,"z":0}]])
    );
    let hidden = state(7, vec![]);
    assert_eq!(
        observed(&client.observe(&hidden, Some(&moved)).await),
        &json!([])
    );
    // Omission does not assert world expiration: same synthetic cloud reappears
    // under a fresh observer identity, with no hidden creation time exposed.
    let reentry = state(8, vec![effect(2, 8, None, &[(1, 1)])]);
    let frame = client.observe(&reentry, Some(&hidden)).await;
    assert_eq!(observed(&frame)[0]["id"], format!("{:040x}", 2));
    assert_eq!(observed(&frame)[0]["observed_since"], "8");
    drop(client);
    let mut reconnected = Client::start(reentry).await;
    let recovered = reconnected.inspect().await;
    assert_eq!(observed(&recovered), observed(&frame));
}

#[tokio::test]
async fn instantaneous_area_event_is_not_a_persistent_cloud() {
    let mut client = Client::start(state(0, vec![])).await;
    let mut next = state(1, vec![]);
    let geometry = effect(1, 1, None, &[(0, 0), (1, 0), (0, 1)]);
    let mut combat = tor_client_common::test_support::observation()
        .combat
        .unwrap();
    combat.own_stats = None;
    combat.events = vec![CombatEventView::Effect {
        caster: Some(ActorTarget::from_digest([1; 32])),
        ability: Some(tor_client_common::test_support::content("fixture_fire")),
        tick: 1,
        timing: EffectTiming::Instant,
        appearance: EffectAppearance::Flash,
        anchors: geometry.anchors,
        delivery: EffectDelivery::Area,
        cue: EffectCue::Fire,
        segments: geometry.segments,
    }];
    next.observation.combat = Some(combat);
    next.validate().unwrap();
    let frame = client.observe(&next, None).await;
    let event = &frame["state"]["observation"]["combat"]["events"][0];
    assert_eq!(event["timing"], "instant");
    assert_eq!(event["tick"], "1");
    assert_eq!(
        event["segments"],
        json!([[{"x":0,"y":0,"z":0},{"x":1,"y":0,"z":0},{"x":0,"y":1,"z":0}]])
    );
    assert_eq!(observed(&frame), &json!([]));
    let cleared = state(2, vec![]);
    let frame = client.observe(&cleared, Some(&next)).await;
    assert!(frame["state"]["observation"]["combat"].is_null());
}

#[tokio::test]
async fn hidden_anchor_and_invalid_world_deadlines_require_repair_without_publication() {
    for mutation in 0..3 {
        let initial = state(0, vec![]);
        let mut client = Client::start(initial.clone()).await;
        // Begin with a positively validated fixture; corrupt one invariant only.
        let mut bad = state(1, vec![effect(1, 1, Some(3), &[(0, 0)])]);
        match mutation {
            0 => bad.observation.public_effects[0].anchors[0].key = format!("{:040x}", 9999),
            1 => bad.observation.public_effects[0].observed_since = 2,
            _ => bad.observation.public_effects[0].expires_at = Some(1),
        }
        assert!(bad.validate().is_err());
        client.send.send(update(&bad, None)).unwrap();
        timeout(WAIT, client.recovery.recv())
            .await
            .expect("invalid effect did not trigger snapshot repair")
            .unwrap();
        let frame = client.inspect().await;
        assert_eq!(frame["synchronized"], false);
        assert_eq!(frame["state"]["revision"], "0");
        assert_eq!(observed(&frame), &json!([]));
    }
}

#[tokio::test]
async fn disclosed_deadline_is_world_time_and_removal_is_authoritative() {
    let initial = state(4, vec![effect(1, 4, Some(6), &[(0, 0)])]);
    let mut client = Client::start(initial.clone()).await;
    // Inspecting/rendering does not advance the authoritative clock or expire it.
    for _ in 0..2 {
        let frame = client.inspect().await;
        assert_eq!(frame["state"]["observation"]["tick"], "4");
        assert_eq!(observed(&frame)[0]["expires_at"], "6");
    }
    let last = state(5, initial.observation.public_effects.clone());
    assert_eq!(
        observed(&client.observe(&last, Some(&initial)).await)
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let ended = state(6, vec![]);
    assert_eq!(
        observed(&client.observe(&ended, Some(&last)).await),
        &json!([])
    );
}

#[tokio::test]
async fn transient_area_outcome_reaches_real_headless_once_without_sustained_state() {
    let mut client = Client::start_with_deferred_map(state(0, vec![]), true).await;
    let geometry = effect(1, 1, None, &[(0, 0), (1, 0), (0, 1)]);
    let event = CombatEventView::Effect {
        caster: Some(ActorTarget::from_digest([1; 32])),
        ability: Some(tor_client_common::test_support::content("fixture_fire")),
        tick: 1,
        timing: EffectTiming::Instant,
        appearance: EffectAppearance::Flash,
        anchors: geometry.anchors,
        delivery: EffectDelivery::Area,
        cue: EffectCue::Fire,
        segments: geometry.segments,
    };
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
                        facts: vec![OutcomeFact::Combat {
                            event,
                            basic_attack: false,
                        }],
                    },
                },
            }),
        })
        .unwrap();
    let frame = client
        .until(|v| v["message"]["update"]["body"]["type"] == "outcome")
        .await;
    let effect = &frame["message"]["update"]["body"]["facts"]["facts"][0]["event"];
    assert_eq!(effect["timing"], "instant");
    assert_eq!(
        effect["segments"],
        json!([[{"x":0,"y":0,"z":0},{"x":1,"y":0,"z":0},{"x":0,"y":1,"z":0}]])
    );
    assert!(!frame["narration"].as_array().unwrap().is_empty());
    assert_eq!(observed(&frame), &json!([]));
    // A map reply retains observation base 0 but must name stream cursor 1.
    // Wait for its acceptance so an early process teardown cannot hide rejection.
    let map = client.until(|v| v["message"]["type"] == "map_page").await;
    assert_eq!(map["message"]["context"]["cursor"]["sequence"], "1");
    assert_eq!(map["message"]["page"]["base"]["cursor"]["sequence"], "0");
    let inspected = client.inspect().await;
    assert_eq!(inspected["narration"], json!([]));
    assert_eq!(observed(&inspected), &json!([]));
}
