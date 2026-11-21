//! Player settings, and the interruption rules they control.
//!
//! A mob entering view interrupts the player's errands and travel unless a
//! setting that matches it is off: peaceful mobs, or mobs this character has
//! never seen before. A hostile mob seen before always interrupts. These
//! settings change only the client's own decisions. The server still ends a
//! journey when anything new appears; the client then reissues it when every
//! newcomer is excused.
use crate::{Game, Net};
use bevy::prelude::*;
use std::collections::HashSet;
use std::path::PathBuf;
use tor_client_3d_model::View;
use tor_protocol::{ActorTarget, TravelPhase};

/// Saved per user beside the client's other local data, outside saves and Git.
#[derive(Resource, Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    /// A peaceful mob entering view interrupts.
    pub interrupt_on_peaceful: bool,
    /// A mob this character has never seen before entering view interrupts.
    pub interrupt_on_unseen: bool,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            interrupt_on_peaceful: true,
            interrupt_on_unseen: true,
            path: None,
        }
    }
}

impl Settings {
    /// Load the saved settings, or the defaults when none are saved or readable.
    pub fn load(path: PathBuf) -> Self {
        let mut settings = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .unwrap_or_default();
        settings.path = Some(path);
        settings
    }

    /// The ordinary location, under the checkout's ignored local data.
    pub fn default_path() -> PathBuf {
        crate::launch::repo_root().join(".local/client-3d/settings.json")
    }

    /// Save, reporting a failure for the status line rather than stopping play.
    pub fn save(&self) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(path, bytes).map_err(|error| error.to_string())
    }

    /// Whether a mob entering view interrupts.
    pub fn interrupts(&self, hostile: bool, seen_before: bool) -> bool {
        (hostile || self.interrupt_on_peaceful) && (seen_before || self.interrupt_on_unseen)
    }
}

/// Mobs that entered view in `now` and were absent from `before`.
pub(crate) fn newcomers<'a>(
    before: &'a HashSet<ActorTarget>,
    now: &'a View,
) -> impl Iterator<Item = &'a tor_client_3d_model::Actor> {
    now.actors
        .iter()
        .filter(move |actor| !actor.is_self && !before.contains(&actor.id))
}

/// Every mob currently in view, other than the observer.
pub(crate) fn in_view(view: &View) -> HashSet<ActorTarget> {
    view.actors
        .iter()
        .filter(|actor| !actor.is_self)
        .map(|actor| actor.id)
        .collect()
}

/// A journey the player asked for, kept so an excused interruption can resume it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Journey {
    pub destination: String,
    /// Mobs in view when it was last sent.
    pub started_with: HashSet<ActorTarget>,
    pub hp: Option<u64>,
    /// The view serial it was sent from; the next ending must be newer.
    pub sent_at: u64,
    pub resumed: u32,
}

/// Resumptions of one journey before the client stops asking.
const MAX_RESUMES: u32 = 50;

/// What to do when the server reports a journey's end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum JourneyEnd {
    /// Still under way, or not ended by a newcomer: nothing to decide.
    Keep,
    /// Every newcomer is excused and nothing hurt: send it again.
    Resume,
    /// Something needs the player.
    Stop,
}

/// Decide a journey's end from the view that reports it.
pub(crate) fn judge(
    settings: &Settings,
    journey: &Journey,
    view: &View,
    seen: &HashSet<ActorTarget>,
) -> JourneyEnd {
    let Some(travel) = &view.travel else {
        return JourneyEnd::Keep;
    };
    match travel.phase {
        TravelPhase::Active => JourneyEnd::Keep,
        TravelPhase::Hazard => {
            let hurt =
                matches!((journey.hp, view.hp), (Some(before), Some((now, _))) if now < before);
            let interrupting = newcomers(&journey.started_with, view)
                .any(|actor| settings.interrupts(actor.hostile, seen.contains(&actor.id)));
            if hurt || interrupting || journey.resumed >= MAX_RESUMES {
                JourneyEnd::Stop
            } else {
                JourneyEnd::Resume
            }
        }
        _ => JourneyEnd::Stop,
    }
}

/// After each view: resume a journey ended only by excused newcomers.
pub(crate) fn resume_travel(
    net: Res<Net>,
    mut game: ResMut<Game>,
    settings: Res<Settings>,
    pursuit: Option<Res<crate::interact::Pursuit>>,
) {
    // A clicked errand re-asks its own travel; only plain journeys resume here.
    if pursuit.is_some_and(|pursuit| pursuit.active()) {
        return;
    }
    let (Some(journey), Some(view)) = (game.journey.clone(), game.view.clone()) else {
        return;
    };
    if game.view_serial == journey.sent_at || game.busy {
        return;
    }
    match judge(&settings, &journey, &view, &game.seen) {
        JourneyEnd::Keep => {}
        JourneyEnd::Stop => game.journey = None,
        JourneyEnd::Resume => {
            if game.send_planned(
                &net,
                tor_client_3d_model::Cmd::Travel(journey.destination.clone()),
            ) {
                let serial = game.view_serial;
                if let Some(journey) = game.journey.as_mut() {
                    journey.started_with = in_view(&view);
                    journey.hp = view.hp.map(|(hp, _)| hp);
                    journey.sent_at = serial;
                    journey.resumed += 1;
                }
            }
        }
    }
}

/// One toggle in the menu's Settings section.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Setting {
    Peaceful,
    Unseen,
}

/// The text of a toggle, refreshed whenever the settings change.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SettingText(pub Setting);

impl Setting {
    fn enabled(self, settings: &Settings) -> bool {
        match self {
            Self::Peaceful => settings.interrupt_on_peaceful,
            Self::Unseen => settings.interrupt_on_unseen,
        }
    }

    pub fn label(self, settings: &Settings) -> String {
        let what = match self {
            Self::Peaceful => "Peaceful mobs interrupt",
            Self::Unseen => "New mobs interrupt",
        };
        format!(
            "{what}: {}",
            if self.enabled(settings) { "On" } else { "Off" }
        )
    }
}

/// A menu button for one setting; it is not a HUD action.
pub(crate) fn toggle_button(setting: Setting, settings: &Settings) -> impl Bundle {
    (
        Button,
        setting,
        crate::chrome::BlockWorld,
        BackgroundColor(crate::chrome::BUTTON),
        BorderColor(crate::chrome::GOLD),
        Node {
            padding: UiRect::axes(Val::Px(12.0), Val::Px(5.0)),
            border: UiRect::all(Val::Px(1.0)),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            Text::new(setting.label(settings)),
            crate::chrome::text(16.0, crate::chrome::INK),
            TextLayout::new_with_justify(JustifyText::Center),
            SettingText(setting),
        )],
    )
}

/// Flip a setting when its button is pressed, save it, and refresh the labels.
pub(crate) fn toggles(
    mut settings: ResMut<Settings>,
    mut game: ResMut<Game>,
    mut buttons: Query<(&Interaction, &Setting, &mut BackgroundColor), Changed<Interaction>>,
    mut texts: Query<(&SettingText, &mut Text)>,
) {
    for (interaction, setting, mut color) in &mut buttons {
        color.0 = if *interaction == Interaction::None {
            crate::chrome::BUTTON
        } else {
            crate::chrome::HOT
        };
        if *interaction != Interaction::Pressed {
            continue;
        }
        match setting {
            Setting::Peaceful => settings.interrupt_on_peaceful = !settings.interrupt_on_peaceful,
            Setting::Unseen => settings.interrupt_on_unseen = !settings.interrupt_on_unseen,
        }
        if let Err(error) = settings.save() {
            game.status = format!("Settings were not saved: {error}");
        }
    }
    if settings.is_changed() {
        for (text, mut label) in &mut texts {
            let value = text.0.label(&settings);
            if label.0 != value {
                label.0 = value;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tor_client_3d_model::Actor;
    use tor_protocol::{EntryId, Position, TravelStatus};

    fn mob(id: u8, hostile: bool) -> Actor {
        Actor {
            appearance_seed: None,
            model: tor_client_3d_model::CreatureModel::Placeholder,
            wearing: Default::default(),
            attire: Default::default(),
            id: ActorTarget::from_digest([id; 32]),
            name: String::new(),
            description: String::new(),
            position: Position {
                x: id.into(),
                y: 0,
                z: 0,
            },
            hostile,
            injury: None,
            is_self: false,
        }
    }

    fn ended(phase: TravelPhase, actors: Vec<Actor>, hp: u64) -> View {
        let mut view = crate::test_support::view(2);
        view.actors = actors;
        view.hp = Some((hp, 20));
        view.travel = Some(TravelStatus {
            id: EntryId("journey".into()),
            destination: "hall".into(),
            completed_steps: 3,
            phase,
        });
        view
    }

    fn journey(started_with: &[u8]) -> Journey {
        Journey {
            destination: "cell:9".into(),
            started_with: started_with
                .iter()
                .map(|id| ActorTarget::from_digest([*id; 32]))
                .collect(),
            hp: Some(20),
            sent_at: 1,
            resumed: 0,
        }
    }

    #[test]
    fn a_newcomer_interrupts_unless_a_matching_setting_is_off() {
        let all = Settings::default();
        let calm = Settings {
            interrupt_on_peaceful: false,
            ..Settings::default()
        };
        let familiar = Settings {
            interrupt_on_unseen: false,
            ..Settings::default()
        };
        // (hostile, seen before) under each setting.
        for (settings, expected) in [
            (&all, [true, true, true, true]),
            (&calm, [false, false, true, true]),
            (&familiar, [false, true, false, true]),
        ] {
            let got = [(false, false), (false, true), (true, false), (true, true)]
                .map(|(hostile, seen)| settings.interrupts(hostile, seen));
            assert_eq!(got, expected, "{settings:?}");
        }
    }

    #[test]
    fn an_excused_newcomer_resumes_travel_and_anything_else_stops_it() {
        let calm = Settings {
            interrupt_on_peaceful: false,
            ..Settings::default()
        };
        let none = HashSet::new();
        // A wandering peaceful mob ended the journey: resume.
        let view = ended(TravelPhase::Hazard, vec![mob(3, false)], 20);
        assert_eq!(
            judge(&calm, &journey(&[]), &view, &none),
            JourneyEnd::Resume
        );
        // The same with default settings stops for the player.
        assert_eq!(
            judge(&Settings::default(), &journey(&[]), &view, &none),
            JourneyEnd::Stop
        );
        // A hostile newcomer, a wound, or arrival always stop.
        let hostile = ended(TravelPhase::Hazard, vec![mob(4, true)], 20);
        assert_eq!(
            judge(&calm, &journey(&[]), &hostile, &none),
            JourneyEnd::Stop
        );
        let hurt = ended(TravelPhase::Hazard, vec![mob(3, false)], 15);
        assert_eq!(judge(&calm, &journey(&[]), &hurt, &none), JourneyEnd::Stop);
        let arrived = ended(TravelPhase::Arrived, vec![], 20);
        assert_eq!(
            judge(&calm, &journey(&[]), &arrived, &none),
            JourneyEnd::Stop
        );
        // A hostile mob already in view when it was sent is not a newcomer.
        let present = ended(TravelPhase::Hazard, vec![mob(4, true), mob(3, false)], 20);
        assert_eq!(
            judge(&calm, &journey(&[4]), &present, &none),
            JourneyEnd::Resume
        );
        let active = ended(TravelPhase::Active, vec![mob(4, true)], 20);
        assert_eq!(
            judge(&calm, &journey(&[]), &active, &none),
            JourneyEnd::Keep
        );
        let mut tired = journey(&[]);
        tired.resumed = MAX_RESUMES;
        assert_eq!(judge(&calm, &tired, &view, &none), JourneyEnd::Stop);
    }

    #[test]
    fn settings_persist_and_unreadable_files_fall_back_to_defaults() {
        // Temporary files stay inside the workspace on the maintainer's machine.
        let directory = crate::launch::repo_root()
            .join(format!(".local/tor-3d-settings-{}", std::process::id()));
        let path = directory.join("settings.json");
        let mut settings = Settings::load(path.clone());
        assert_eq!(
            settings,
            Settings {
                path: Some(path.clone()),
                ..Settings::default()
            }
        );
        settings.interrupt_on_unseen = false;
        settings.save().unwrap();
        assert!(!Settings::load(path.clone()).interrupt_on_unseen);
        std::fs::write(&path, b"not json").unwrap();
        assert_eq!(
            Settings::load(path.clone()),
            Settings {
                path: Some(path),
                ..Settings::default()
            }
        );
        let _ = std::fs::remove_dir_all(directory);
    }
}
