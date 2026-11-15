//! Render profiles: how much the client spends drawing each frame.
//!
//! A profile is chosen once at start-up. `full`, the desktop default, keeps
//! every effect and draws exactly as the client did before profiles existed;
//! `low` is for hardware a long way below a desktop GPU, and is what the
//! experimental Android client uses. Each setting can also be changed alone,
//! which is how a saving is tried before a profile adopts it.
//!
//! There is no player-facing choice yet. Developers pick a profile, and may
//! override single settings, with `TOR_RENDER_PROFILE`, for example
//! `low,scale=0.67,shadows=on`; `TOR_RENDER_REPORT=5` logs frame timing every
//! five seconds.
use crate::cave::CaveMaterial;
use bevy::{
    core_pipeline::{
        bloom::Bloom,
        fxaa::Fxaa,
        prepass::{DepthPrepass, NormalPrepass},
    },
    ecs::schedule::{ExecutorKind, ScheduleLabel},
    pbr::ScreenSpaceAmbientOcclusion,
    prelude::*,
};
use std::time::Instant;

#[cfg(target_os = "android")]
mod android;
mod baked;
#[cfg(test)]
mod tests;

/// Where the rock's surface noise comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RockNoise {
    /// Hashed afresh for every pixel.
    Hashed,
    /// Read from textures baked at start-up: far fewer operations per pixel,
    /// with a pattern that repeats every 16 units, smoother strata and
    /// straighter floor cracks.
    Baked,
}

/// How the rock is lit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RockLighting {
    /// The engine's physically based model, highlights included.
    Full,
    /// A matte surface: ambient, point and directional light, no highlights.
    Diffuse,
}

/// How the engine runs its systems.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scheduling {
    /// Spread over the worker threads.
    Parallel,
    /// In turn, on one thread for the game and one for rendering. Handing
    /// hundreds of small systems between threads costs more than it saves.
    Serial,
}

#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub(crate) struct RenderProfile {
    /// The fraction of the screen's width and height that is drawn; the rest
    /// is made up by stretching. Applied on Android only so far.
    pub scale: f32,
    /// Lower the scale by up to a fifth while frames miss thirty a second, and
    /// restore it when they stop missing.
    pub adaptive_scale: bool,
    pub rock_noise: RockNoise,
    pub rock_lighting: RockLighting,
    pub ambient_occlusion: bool,
    pub bloom: bool,
    pub antialiasing: bool,
    pub shadows: bool,
    pub scheduling: Scheduling,
    /// Zero fits every cloak every frame; otherwise see
    /// [`CloakPacing`](crate::creatures::CloakPacing).
    pub cloak_interval: u32,
    /// Keep the game and render threads on the fastest cores, where the host
    /// can and the cores differ.
    pub pin_threads: bool,
}

impl RenderProfile {
    /// Every effect, and the rock as it was first written.
    pub(crate) const fn full() -> Self {
        Self {
            scale: 1.0,
            adaptive_scale: false,
            rock_noise: RockNoise::Hashed,
            rock_lighting: RockLighting::Full,
            ambient_occlusion: true,
            bloom: true,
            antialiasing: true,
            shadows: true,
            scheduling: Scheduling::Parallel,
            cloak_interval: 0,
            pin_threads: false,
        }
    }

    /// For hardware far below a desktop GPU.
    pub(crate) const fn low() -> Self {
        Self {
            scale: 0.5,
            adaptive_scale: true,
            rock_noise: RockNoise::Baked,
            rock_lighting: RockLighting::Diffuse,
            ambient_occlusion: false,
            bloom: false,
            antialiasing: true,
            shadows: false,
            scheduling: Scheduling::Serial,
            cloak_interval: 2,
            pin_threads: true,
        }
    }

    pub(crate) const fn for_platform(android: bool) -> Self {
        if android {
            Self::low()
        } else {
            Self::full()
        }
    }

    /// The platform's profile, changed by a comma-separated list of profile
    /// names and `key=value` settings.
    pub(crate) fn parse(android: bool, text: &str) -> Result<Self, String> {
        let mut profile = Self::for_platform(android);
        for entry in text.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            let Some((key, value)) = entry.split_once('=') else {
                profile = match entry {
                    "full" => Self::full(),
                    "low" => Self::low(),
                    _ => return Err(format!("Unknown render profile `{entry}`")),
                };
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            let invalid = || format!("Invalid render setting `{entry}`");
            let switch = || match value {
                "on" | "1" | "true" => Ok(true),
                "off" | "0" | "false" => Ok(false),
                _ => Err(invalid()),
            };
            match key {
                "scale" => {
                    profile.scale = value
                        .parse()
                        .ok()
                        .filter(|scale| (0.25..=1.0).contains(scale))
                        .ok_or_else(invalid)?;
                }
                "adaptive" => profile.adaptive_scale = switch()?,
                "noise" => {
                    profile.rock_noise = match value {
                        "hashed" => RockNoise::Hashed,
                        "baked" => RockNoise::Baked,
                        _ => return Err(invalid()),
                    };
                }
                "lighting" => {
                    profile.rock_lighting = match value {
                        "full" => RockLighting::Full,
                        "diffuse" => RockLighting::Diffuse,
                        _ => return Err(invalid()),
                    };
                }
                "occlusion" => profile.ambient_occlusion = switch()?,
                "bloom" => profile.bloom = switch()?,
                "antialiasing" => profile.antialiasing = switch()?,
                "shadows" => profile.shadows = switch()?,
                "scheduling" => {
                    profile.scheduling = match value {
                        "parallel" => Scheduling::Parallel,
                        "serial" => Scheduling::Serial,
                        _ => return Err(invalid()),
                    };
                }
                "cloaks" => profile.cloak_interval = value.parse().map_err(|_| invalid())?,
                "pin" => profile.pin_threads = switch()?,
                _ => return Err(format!("Unknown render setting `{key}`")),
            }
        }
        Ok(profile)
    }

    /// The platform's profile with any `TOR_RENDER_PROFILE` changes.
    pub(crate) fn from_environment() -> Result<Self, String> {
        Self::parse(
            cfg!(target_os = "android"),
            &std::env::var("TOR_RENDER_PROFILE").unwrap_or_default(),
        )
    }

    /// The two switches the rock shader reads from `CaveParams::light`.
    fn rock_switches(&self) -> Vec2 {
        Vec2::new(
            f32::from(u8::from(self.rock_noise == RockNoise::Baked)),
            f32::from(u8::from(self.rock_lighting == RockLighting::Diffuse)),
        )
    }
}

/// Lowers the drawn scale while frames miss thirty a second, and tries the
/// chosen scale again once they have kept pace for a while.
// Driven only where the scale is applied, which is Android so far.
#[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
#[derive(Resource, Debug)]
pub(crate) struct ScaleGovernor {
    /// Tenths taken off the chosen scale: none, one or two.
    step: u8,
    /// Seconds of steady pace needed before a larger picture is tried again.
    patience: f64,
    steady: f64,
    /// Whether the last change was a trial of a larger picture.
    trial: bool,
}

impl Default for ScaleGovernor {
    fn default() -> Self {
        Self {
            step: 0,
            patience: Self::PATIENCE,
            steady: 0.0,
            trial: false,
        }
    }
}

#[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
impl ScaleGovernor {
    const PATIENCE: f64 = 15.0;
    /// A window slower than this missed the pace of thirty frames a second.
    const MISSED_MS: f64 = 35.0;
    /// A stall this long, such as loading, says nothing about drawing.
    const STALLED_MS: f64 = 100.0;

    pub(crate) fn scale(&self, profile: &RenderProfile) -> f32 {
        profile.scale * (1.0 - 0.1 * f32::from(self.step))
    }

    /// Takes the mean frame time over a window of `seconds`; returns whether
    /// the scale changed.
    fn observe(&mut self, mean_ms: f64, seconds: f64) -> bool {
        if mean_ms > Self::STALLED_MS {
            return false;
        }
        if mean_ms > Self::MISSED_MS {
            if self.trial {
                // The larger picture did not hold; wait longer before retrying.
                self.patience = (self.patience * 2.0).min(120.0);
            }
            self.trial = false;
            self.steady = 0.0;
            if self.step < 2 {
                self.step += 1;
                return true;
            }
            return false;
        }
        self.steady += seconds;
        if self.trial && self.steady >= 6.0 {
            self.trial = false;
            self.patience = Self::PATIENCE;
        }
        if self.step > 0 && self.steady >= self.patience {
            self.step -= 1;
            self.trial = true;
            self.steady = 0.0;
            return true;
        }
        false
    }
}

/// A scene camera the profile has already adjusted.
#[derive(Component)]
struct Profiled;

/// The baked rock textures, once any material needs them.
#[derive(Resource, Default)]
struct Baked(Option<(Handle<Image>, Handle<Image>)>);

/// Seconds between frame-timing reports.
#[derive(Resource)]
struct Report(f64);

/// Frame timing gathered since the last report.
#[derive(Resource, Default)]
struct Meter {
    since: f64,
    frames: u32,
    slowest: f64,
    began: Option<Instant>,
    work: f64,
}

pub(crate) fn plugin(profile: RenderProfile) -> impl Plugin {
    ProfilePlugin(profile)
}

struct ProfilePlugin(RenderProfile);

impl Plugin for ProfilePlugin {
    fn build(&self, app: &mut App) {
        let profile = self.0;
        info!("Render profile: {profile:?}");
        app.insert_resource(profile)
            .init_resource::<ScaleGovernor>()
            .init_resource::<Baked>();
        if profile.scheduling == Scheduling::Serial {
            for schedule in [
                First.intern(),
                PreUpdate.intern(),
                Update.intern(),
                PostUpdate.intern(),
                Last.intern(),
            ] {
                app.edit_schedule(schedule, |schedule| {
                    schedule.set_executor_kind(ExecutorKind::SingleThreaded);
                });
            }
            if let Some(render) = app.get_sub_app_mut(bevy::render::RenderApp) {
                render.edit_schedule(bevy::render::Render, |schedule| {
                    schedule.set_executor_kind(ExecutorKind::SingleThreaded);
                });
                render.edit_schedule(ExtractSchedule, |schedule| {
                    schedule.set_executor_kind(ExecutorKind::SingleThreaded);
                });
            }
        }
        if profile.cloak_interval > 0 {
            app.insert_resource(crate::creatures::CloakPacing::every(profile.cloak_interval));
        }
        if !(profile.ambient_occlusion && profile.bloom && profile.antialiasing) {
            app.add_systems(Last, cameras);
        }
        if !profile.shadows {
            app.add_systems(Last, lights);
        }
        if profile.rock_switches() != Vec2::ZERO {
            app.add_systems(Last, rock);
        }
        let report = std::env::var("TOR_RENDER_REPORT")
            .ok()
            .and_then(|seconds| seconds.parse::<f64>().ok())
            .filter(|seconds| *seconds > 0.0);
        if let Some(seconds) = report {
            app.insert_resource(Report(seconds))
                .init_resource::<Meter>()
                .add_systems(First, |mut meter: ResMut<Meter>| {
                    meter.began = Some(Instant::now());
                })
                .add_systems(Last, self::report);
        }
        #[cfg(target_os = "android")]
        android::build(app, &profile);
        #[cfg(not(target_os = "android"))]
        if profile.scale < 1.0 {
            warn!("A render scale below one is applied on Android only so far");
        }
    }
}

/// Removes the effects the profile leaves out from each scene camera, once.
fn cameras(
    profile: Res<RenderProfile>,
    mut commands: Commands,
    found: Query<Entity, (With<Camera3d>, Without<Profiled>)>,
) {
    for entity in &found {
        let mut camera = commands.entity(entity);
        camera.insert(Profiled);
        if !profile.ambient_occlusion {
            camera.remove::<(ScreenSpaceAmbientOcclusion, DepthPrepass, NormalPrepass)>();
        }
        if !profile.bloom {
            camera.remove::<Bloom>();
        }
        if !profile.antialiasing {
            camera.remove::<Fxaa>();
        }
    }
}

/// Keeps every light from casting shadows.
fn lights(
    mut points: Query<&mut PointLight>,
    mut suns: Query<&mut DirectionalLight>,
    mut spots: Query<&mut SpotLight>,
) {
    for mut light in &mut points {
        if light.shadows_enabled {
            light.shadows_enabled = false;
        }
    }
    for mut light in &mut suns {
        if light.shadows_enabled {
            light.shadows_enabled = false;
        }
    }
    for mut light in &mut spots {
        if light.shadows_enabled {
            light.shadows_enabled = false;
        }
    }
}

/// Gives every cave material the profile's rock switches and, when it reads
/// them, the baked noise textures.
fn rock(
    profile: Res<RenderProfile>,
    mut baked: ResMut<Baked>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<CaveMaterial>>,
) {
    let switches = profile.rock_switches();
    let textures = (profile.rock_noise == RockNoise::Baked).then(|| {
        baked
            .0
            .get_or_insert_with(|| {
                (
                    images.add(baked::lattice_image()),
                    images.add(baked::sums_image()),
                )
            })
            .clone()
    });
    let stale: Vec<_> = materials
        .iter()
        .filter(|(_, material)| {
            let light = material.extension.params.light;
            Vec2::new(light.y, light.z) != switches
                || material.extension.lattice.is_none() != textures.is_none()
        })
        .map(|(id, _)| id)
        .collect();
    for id in stale {
        if let Some(material) = materials.get_mut(id) {
            material.extension.params.light.y = switches.x;
            material.extension.params.light.z = switches.y;
            let (lattice, sums) = textures.clone().unzip();
            material.extension.lattice = lattice;
            material.extension.sums = sums;
        }
    }
}

fn report(
    (profile, governor, every): (Res<RenderProfile>, Res<ScaleGovernor>, Res<Report>),
    time: Res<Time<Real>>,
    mut meter: ResMut<Meter>,
    drawn: Query<&ViewVisibility, With<Mesh3d>>,
    rigs: Query<(), With<crate::creatures::Rig>>,
    points: Query<(), With<PointLight>>,
) {
    let now = time.elapsed_secs_f64();
    meter.frames += 1;
    meter.slowest = meter.slowest.max(time.delta_secs_f64());
    if let Some(began) = meter.began {
        meter.work += began.elapsed().as_secs_f64();
    }
    let seconds = now - meter.since;
    if seconds < every.0 {
        return;
    }
    let frames = f64::from(meter.frames);
    info!(
        "render-report fps {:.1} mean-ms {:.1} slowest-ms {:.1} update-ms {:.1} scale {:.2} creatures {} meshes-drawn {} point-lights {}",
        frames / seconds,
        seconds * 1000.0 / frames,
        meter.slowest * 1000.0,
        meter.work * 1000.0 / frames,
        governor.scale(&profile),
        rigs.iter().count(),
        drawn.iter().filter(|visible| visible.get()).count(),
        points.iter().count(),
    );
    *meter = Meter {
        since: now,
        ..default()
    };
}
