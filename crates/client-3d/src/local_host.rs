//! Games played on this device alone: the scenarios installed beside the
//! client, the saves it has made, and a private server inside this process.
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tor_server::in_process::{scenario_summary, Host, HostConfig};

type Error = Box<dyn std::error::Error + Send + Sync>;

/// The file beside each save naming what it was started from.
const SIDECAR: &str = "launch.json";
const SAVE_FILE: &str = "game.db";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScenarioChoice {
    pub(crate) id: String,
    pub(crate) version: String,
    pub(crate) path: PathBuf,
}

impl ScenarioChoice {
    pub(crate) fn label(&self) -> String {
        format!("{} {}", self.id, self.version)
    }
}

/// Every package directly inside `root`, once per id. A directory without a
/// manifest is not a scenario; one whose manifest cannot be read is skipped.
pub(crate) fn discover_scenarios(root: &Path) -> Vec<ScenarioChoice> {
    let found = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join("scenario.toml").is_file())
        .filter_map(|path| {
            let summary = scenario_summary(&path).ok()?;
            let installed = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .map_or(0, seconds);
            Some((
                ScenarioChoice {
                    id: summary.id,
                    version: summary.version,
                    path,
                },
                installed,
            ))
        })
        .collect();
    newest_per_id(found)
}

/// Older installed copies of a package stay so their saves remain playable;
/// a new game uses the most recently installed copy of each id.
fn newest_per_id(mut found: Vec<(ScenarioChoice, u64)>) -> Vec<ScenarioChoice> {
    found.sort_by(|(a, a_time), (b, b_time)| {
        a.id.cmp(&b.id)
            .then(b_time.cmp(a_time))
            .then(b.path.cmp(&a.path))
    });
    found.dedup_by(|(later, _), (kept, _)| later.id == kept.id);
    found.into_iter().map(|(choice, _)| choice).collect()
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Sidecar {
    pub(crate) scenario: String,
    pub(crate) scenario_path: PathBuf,
    pub(crate) seed: u64,
    /// Seconds since the Unix epoch.
    pub(crate) created: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SavedGame {
    pub(crate) save: PathBuf,
    /// Missing for saves made by a launcher that kept no record.
    pub(crate) sidecar: Option<Sidecar>,
    /// Seconds since the Unix epoch the save was last written.
    pub(crate) modified: u64,
}

impl SavedGame {
    pub(crate) fn label(&self) -> String {
        let name = self
            .save
            .parent()
            .and_then(Path::file_name)
            .map_or_else(|| "game".into(), |n| n.to_string_lossy().into_owned());
        match &self.sidecar {
            Some(sidecar) => format!(
                "{}   seed {}   last played {}",
                sidecar.scenario,
                sidecar.seed,
                calendar(self.modified)
            ),
            None => format!("{name}   last played {}", calendar(self.modified)),
        }
    }
}

fn seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// A UTC date and minute, without a calendar dependency.
pub(crate) fn calendar(epoch_seconds: u64) -> String {
    let days = (epoch_seconds / 86_400) as i64;
    let minute = epoch_seconds % 86_400 / 60;
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        minute / 60,
        minute % 60
    )
}

/// Saves directly inside `root`, most recently played first.
pub(crate) fn list_saves(root: &Path) -> Vec<SavedGame> {
    let mut saves: Vec<_> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let dir = entry.path();
            let save = dir.join(SAVE_FILE);
            let modified = seconds(std::fs::metadata(&save).ok()?.modified().ok()?);
            let sidecar = std::fs::read(dir.join(SIDECAR))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok());
            Some(SavedGame {
                save,
                sidecar,
                modified,
            })
        })
        .collect();
    saves.sort_by(|a, b| b.modified.cmp(&a.modified).then(b.save.cmp(&a.save)));
    saves
}

/// A seed short enough to read back and type again.
pub(crate) fn random_seed() -> u64 {
    (uuid::Uuid::new_v4().as_u128() as u64) & 0xffff_ffff
}

/// A device may stop the app without warning, so play is written within a
/// couple of seconds instead of the server's half-minute default.
pub(crate) fn standalone_policy() -> tor_server::SavePolicy {
    use std::time::Duration;
    tor_server::SavePolicy {
        target_interval: Duration::from_secs(2),
        max_unsaved_age: Duration::from_secs(10),
        idle_interval: Duration::from_millis(500),
        ..Default::default()
    }
}

/// A running private game and the credential its player connects with.
pub(crate) struct Started {
    pub(crate) host: Host,
    pub(crate) token: String,
    pub(crate) save: PathBuf,
}

/// Starts a new game in a fresh directory; earlier saves are never reused.
pub(crate) fn new_game(
    scenario: &ScenarioChoice,
    seed: u64,
    save_root: &Path,
) -> Result<Started, Error> {
    let now = seconds(SystemTime::now());
    let save = crate::launch::reserve_save(save_root, &now.to_string())?;
    let sidecar = Sidecar {
        scenario: scenario.id.clone(),
        scenario_path: scenario.path.clone(),
        seed,
        created: now,
    };
    let directory = save.parent().ok_or("Save has no directory")?;
    std::fs::write(
        directory.join(SIDECAR),
        serde_json::to_vec_pretty(&sidecar)?,
    )?;
    start(&scenario.path, save, seed)
}

/// Resumes a save with the package it was started from.
pub(crate) fn resume(saved: &SavedGame, scenario_root: &Path) -> Result<Started, Error> {
    let sidecar = saved
        .sidecar
        .as_ref()
        .ok_or("This save has no record of its scenario, so it cannot be loaded here.")?;
    let package = if sidecar.scenario_path.join("scenario.toml").is_file() {
        sidecar.scenario_path.clone()
    } else {
        // The package moved, for example after reinstalling: use the
        // installed package with the same id. The save's pinned hash still
        // decides whether it matches.
        discover_scenarios(scenario_root)
            .into_iter()
            .find(|choice| choice.id == sidecar.scenario)
            .map(|choice| choice.path)
            .ok_or_else(|| format!("Scenario {} is not installed.", sidecar.scenario))?
    };
    start(&package, saved.save.clone(), sidecar.seed)
}

fn start(scenario: &Path, save: PathBuf, seed: u64) -> Result<Started, Error> {
    let credentials = crate::launch::Credentials::new();
    let mut config = HostConfig::new(
        scenario.to_path_buf(),
        save.clone(),
        seed,
        credentials.owner.clone(),
    );
    config.policy = standalone_policy();
    let host = Host::start(config)?;
    Ok(Started {
        host,
        token: credentials.owner,
        save,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let root = crate::launch::repo_root()
            .join(".local/3d-local-host-tests")
            .join(uuid::Uuid::new_v4().simple().to_string());
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn scenario_discovery_lists_only_packages_once_per_id() {
        let root = scratch();
        let shipped = crate::launch::repo_root().join("scenarios");
        std::fs::create_dir_all(root.join("notes")).unwrap();
        std::fs::write(root.join("notes/readme.txt"), "not a package").unwrap();
        for (directory, id) in [
            ("rogue-aaaa", "rogue"),
            ("rogue-bbbb", "rogue"),
            ("two", "two-room"),
        ] {
            let dir = root.join(directory);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("scenario.toml"),
                format!("format = 37\nid = \"{id}\"\nversion = \"1.0\"\n"),
            )
            .unwrap();
        }
        let found = discover_scenarios(&root);
        assert_eq!(
            found.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["rogue", "two-room"]
        );
        let choice = |dir: &str| ScenarioChoice {
            id: "rogue".into(),
            version: "1.0".into(),
            path: root.join(dir),
        };
        assert_eq!(
            newest_per_id(vec![(choice("rogue-new"), 20), (choice("rogue-old"), 10)]),
            [choice("rogue-new")],
            "the latest installation wins, whatever its directory is called"
        );
        assert_eq!(
            newest_per_id(vec![(choice("rogue-old"), 10), (choice("rogue-new"), 20)]),
            [choice("rogue-new")]
        );
        let shipped = discover_scenarios(&shipped);
        assert!(shipped.iter().any(|c| c.id == "rogue"));
        assert!(
            shipped.iter().all(|c| !c.path.ends_with("tests")),
            "the fixture directory is not a package"
        );
    }

    #[test]
    fn saves_are_listed_newest_first_with_or_without_a_record() {
        let root = scratch();
        let old = root.join("game-1-0");
        let new = root.join("game-2-0");
        let empty = root.join("game-3-0");
        for dir in [&old, &new, &empty] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(old.join(SAVE_FILE), b"old").unwrap();
        let record = Sidecar {
            scenario: "rogue".into(),
            scenario_path: root.join("rogue"),
            seed: 77,
            created: 2,
        };
        std::fs::write(new.join(SIDECAR), serde_json::to_vec(&record).unwrap()).unwrap();
        std::fs::write(new.join(SAVE_FILE), b"new").unwrap();
        let past = SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::File::options()
            .write(true)
            .open(old.join(SAVE_FILE))
            .unwrap()
            .set_modified(past)
            .unwrap();
        let saves = list_saves(&root);
        assert_eq!(saves.len(), 2, "a directory without a save is not listed");
        assert_eq!(saves[0].save, new.join(SAVE_FILE));
        assert_eq!(saves[0].sidecar.as_ref(), Some(&record));
        assert!(saves[0].label().contains("seed 77"));
        assert_eq!(saves[1].sidecar, None);
        assert!(saves[1].label().starts_with("game-1-0"));
    }

    #[test]
    fn calendar_dates_match_known_instants() {
        assert_eq!(calendar(0), "1970-01-01 00:00 UTC");
        assert_eq!(calendar(951_782_400), "2000-02-29 00:00 UTC");
        assert_eq!(calendar(1_794_787_260), "2026-11-16 00:01 UTC");
    }

    #[test]
    fn standalone_saves_are_frequent_and_valid() {
        let policy = standalone_policy();
        assert!(policy.validate().is_ok());
        assert!(policy.target_interval <= std::time::Duration::from_secs(2));
        assert!(policy.max_unsaved_age <= std::time::Duration::from_secs(10));
    }

    #[test]
    fn random_seeds_stay_short_enough_to_type() {
        for _ in 0..64 {
            assert!(random_seed() <= u64::from(u32::MAX));
        }
    }

    #[test]
    fn a_new_game_records_its_scenario_and_resumes_with_its_own_seed() {
        let root = scratch();
        let scenario = discover_scenarios(&crate::launch::repo_root().join("scenarios"))
            .into_iter()
            .find(|c| c.id == "two-room")
            .expect("shipped two-room package");
        let started = new_game(&scenario, 4242, &root).unwrap();
        let save = started.save.clone();
        started.host.shutdown().unwrap();
        let saves = list_saves(&root);
        assert_eq!(saves.len(), 1);
        let record = saves[0].sidecar.clone().unwrap();
        assert_eq!(record.seed, 4242);
        assert_eq!(record.scenario, "two-room");
        assert_eq!(saves[0].save, save);

        let again = new_game(&scenario, 4242, &root).unwrap();
        assert_ne!(again.save, save, "a new game never reuses a save");
        again.host.shutdown().unwrap();

        let resumed = resume(&saves[0], &root).unwrap();
        assert_eq!(resumed.save, save);
        resumed.host.shutdown().unwrap();
    }

    #[test]
    fn a_save_without_a_record_is_refused_with_a_reason() {
        let saved = SavedGame {
            save: PathBuf::from("missing/game.db"),
            sidecar: None,
            modified: 0,
        };
        let error = resume(&saved, Path::new("nowhere")).err().unwrap();
        assert!(error.to_string().contains("no record"));
    }
}
