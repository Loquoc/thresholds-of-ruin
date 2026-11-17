//! The in-process host: the startup `tor-server` performs, as a library call.
use std::path::PathBuf;
use tor_client_common::Connection;
use tor_protocol::ActorId;
use tor_server::in_process::{scenario_summary, Host, HostConfig};

const PLAYER: &str = "host-player-token-0001";

fn package(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios")
        .join(name)
}

async fn attach(host: &Host, token: &str) -> Result<Connection, String> {
    Connection::connect(host.address(), token.into(), ActorId(1), "test")
        .await
        .map_err(|error| error.to_string())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hosted_game_is_playable_durable_and_keeps_its_seed() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("game.db");
    let host = tokio::task::spawn_blocking({
        let save = save.clone();
        move || {
            Host::start(HostConfig::new(
                package("two-room"),
                save,
                4242,
                PLAYER.into(),
            ))
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert!(host.address().ip().is_loopback());
    let mut client = attach(&host, PLAYER).await.unwrap();
    assert!(
        attach(&host, "not-the-player-token").await.is_err(),
        "only the configured credential is accepted"
    );
    client.close().await.unwrap();
    tokio::task::spawn_blocking(move || host.shutdown())
        .await
        .unwrap()
        .unwrap();
    let archive = tor_server::inspect_save(&save).unwrap();
    assert_eq!(archive["scenario"]["seed"], 4242);

    // Resuming with another seed keeps the save's own.
    let host = tokio::task::spawn_blocking({
        let save = save.clone();
        move || Host::start(HostConfig::new(package("two-room"), save, 7, PLAYER.into()))
    })
    .await
    .unwrap()
    .unwrap();
    let mut client = attach(&host, PLAYER).await.unwrap();
    client.close().await.unwrap();
    tokio::task::spawn_blocking(move || host.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        tor_server::inspect_save(&save).unwrap()["scenario"]["seed"],
        4242
    );
}

#[test]
fn a_host_refuses_what_the_command_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let short = Host::start(HostConfig::new(
        package("two-room"),
        dir.path().join("a.db"),
        1,
        "short".into(),
    ));
    assert!(short.err().unwrap().to_string().contains("16-1024"));
    let mut same = HostConfig::new(
        package("two-room"),
        dir.path().join("b.db"),
        1,
        PLAYER.into(),
    );
    same.spectator_token = Some(PLAYER.into());
    assert!(
        Host::start(same).is_err(),
        "player and spectator must differ"
    );
    let missing = Host::start(HostConfig::new(
        dir.path().join("no-such-package"),
        dir.path().join("c.db"),
        1,
        PLAYER.into(),
    ));
    assert!(missing.is_err());
    assert!(
        !dir.path().join("c.db").exists(),
        "a failed start makes no save"
    );
}

#[test]
fn package_summaries_name_shipped_packages() {
    let summary = scenario_summary(&package("rogue")).unwrap();
    assert_eq!(summary.id, "rogue");
    assert_eq!(summary.version, "1.0");
    assert_eq!(summary.content_hash.as_deref().map(str::len), Some(64));
    assert!(scenario_summary(&package("does-not-exist")).is_err());
}
