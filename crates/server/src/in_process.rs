//! A private game server inside another program. This is the startup the
//! `tor-server` command performs, for hosts such as the Android app that
//! cannot start a second process. Clients still reach it over its loopback
//! WebSocket, exactly as they reach the command.
use crate::{
    scenario_package, serve, Account, Engine, OutboundLimits, SavePolicy, Service, Simulation,
};
use std::{
    collections::BTreeSet,
    net::SocketAddr,
    path::{Path, PathBuf},
    thread::JoinHandle,
};
use tokio::{net::TcpListener, sync::oneshot};

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// Rejects a credential the `tor-server` command would also reject.
pub fn check_token(name: &str, token: &str) -> Result<(), String> {
    if token.trim().len() < 16 || token.len() > 1024 || token.chars().any(char::is_control) {
        return Err(format!(
            "{name} must contain 16-1024 non-control characters"
        ));
    }
    Ok(())
}

/// The player account, then optional spectator and wizard accounts, all for
/// every actor of `engine`. Tokens must already be distinct and valid.
pub fn accounts(
    engine: &Engine,
    player: String,
    spectator: Option<String>,
    wizard: Option<String>,
) -> Vec<Account> {
    let actors = engine.actors().into_iter().collect::<BTreeSet<_>>();
    let mut accounts = vec![Account {
        role: tor_protocol::AccessRole::Player,
        user: "local".into(),
        token: player,
        actors: actors.clone(),
    }];
    if let Some(token) = spectator {
        accounts.push(Account {
            role: tor_protocol::AccessRole::Spectator,
            user: "spectator".into(),
            token,
            actors: actors.clone(),
        });
    }
    if let Some(token) = wizard {
        accounts.push(Account {
            role: tor_protocol::AccessRole::Wizard,
            user: "wizard".into(),
            token,
            actors,
        });
    }
    accounts
}

/// What one private game needs. An existing save keeps its own seed and its
/// pinned package; `scenario` is still read for regions it has not copied.
pub struct HostConfig {
    pub scenario: PathBuf,
    pub save: PathBuf,
    pub seed: u64,
    pub character: Option<u64>,
    pub policy: SavePolicy,
    pub outbound: OutboundLimits,
    pub player_token: String,
    pub spectator_token: Option<String>,
}

impl HostConfig {
    pub fn new(scenario: PathBuf, save: PathBuf, seed: u64, player_token: String) -> Self {
        Self {
            scenario,
            save,
            seed,
            character: None,
            policy: SavePolicy::default(),
            outbound: OutboundLimits::default(),
            player_token,
            spectator_token: None,
        }
    }
}

/// A running private server. It never grants wizard access. Shut it down
/// with [`Host::shutdown`] to learn whether the save was flushed; dropping
/// it also stops it, discarding that answer.
pub struct Host {
    address: SocketAddr,
    stop: Option<oneshot::Sender<()>>,
    worker: Option<JoinHandle<Result<(), String>>>,
}

impl Host {
    /// Opens or creates the save and listens on a fresh loopback port.
    /// Returns once clients can connect.
    pub fn start(config: HostConfig) -> Result<Self, Error> {
        check_token("The player token", &config.player_token)?;
        if let Some(spectator) = &config.spectator_token {
            check_token("The spectator token", spectator)?;
            if *spectator == config.player_token {
                return Err("Player and spectator tokens must differ".into());
            }
        }
        if !config.outbound.is_valid() {
            return Err("Invalid outbound byte limits".into());
        }
        config.policy.validate()?;
        let scenario =
            scenario_package::load(&config.scenario, config.seed, config.character, false)?;
        let mut engine = Engine::open_with_policy(&config.save, scenario, config.policy)?;
        engine.start_preloading();
        let accounts = accounts(&engine, config.player_token, config.spectator_token, None);
        let service = Service::with_outbound_limits(engine, config.outbound)?;
        let simulation = Simulation::start(service);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("tor-host")
            .enable_all()
            .build()?;
        let listener = runtime.block_on(TcpListener::bind("127.0.0.1:0"))?;
        let address = listener.local_addr()?;
        let (stop, stopped) = oneshot::channel::<()>();
        let worker = std::thread::Builder::new()
            .name("tor-host".into())
            .spawn(move || {
                runtime
                    .block_on(serve(listener, simulation, accounts, async {
                        let _ = stopped.await;
                    }))
                    .map(drop)
                    .map_err(|error| error.to_string())
            })?;
        Ok(Self {
            address,
            stop: Some(stop),
            worker: Some(worker),
        })
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Stops accepting clients, closes their sessions and waits until the
    /// save is durable.
    pub fn shutdown(mut self) -> Result<(), String> {
        self.stop_and_wait()
    }

    fn stop_and_wait(&mut self) -> Result<(), String> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        match self.worker.take() {
            Some(worker) => worker
                .join()
                .unwrap_or_else(|_| Err("The private server panicked".into())),
            None => Ok(()),
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.stop_and_wait();
    }
}

/// What a menu can say about a package without loading it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScenarioSummary {
    pub id: String,
    pub version: String,
    /// From its validation certificate; `None` for an unvalidated package.
    pub content_hash: Option<String>,
}

/// Reads a package's identity from its manifest and certificate.
pub fn scenario_summary(root: &Path) -> Result<ScenarioSummary, Error> {
    #[derive(serde::Deserialize)]
    struct Identity {
        id: String,
        version: String,
    }
    #[derive(serde::Deserialize)]
    struct Certified {
        content_hash: String,
    }
    let identity: Identity = toml::from_str(&std::fs::read_to_string(root.join("scenario.toml"))?)?;
    let content_hash = match std::fs::read_to_string(root.join("validation.json")) {
        Ok(text) => Some(serde_json::from_str::<Certified>(&text)?.content_hash),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    Ok(ScenarioSummary {
        id: identity.id,
        version: identity.version,
        content_hash,
    })
}
