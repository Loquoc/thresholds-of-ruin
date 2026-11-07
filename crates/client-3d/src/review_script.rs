//! Offline script runner, sharing procedural composition and presentation animation.
mod dungeon;
pub(crate) mod live;
mod schema;
#[cfg(test)]
mod tests;
use crate::{
    creatures::{self, Anim, Look, Rig, J},
    props::{PropLibrary, PropMaterials},
};
use bevy::{
    app::ScheduleRunnerPlugin,
    ecs::schedule::ScheduleLabel,
    prelude::*,
    render::{
        camera::RenderTarget,
        mesh::RenderMesh,
        render_asset::{RenderAssetUsages, RenderAssets},
        render_resource::{
            CachedPipelineState, Extent3d, PipelineCache, TextureDimension, TextureFormat,
            TextureUsages,
        },
        renderer::RenderAdapterInfo,
        view::screenshot::{Screenshot, ScreenshotCaptured},
        Render, RenderApp, RenderSet,
    },
    time::TimeSystem,
    window::{ExitCondition, WindowResolution},
    winit::WinitPlugin,
};
use schema::*;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Apply,
    Pose,
    Camera,
    Pending,
    Advance,
    Finished,
    Failed,
}
/// Constant-memory CPU-side schedule wall-time attribution, enabled only by
/// the offline capture runner. These are durations, not simulation deadlines.
#[derive(Default)]
struct ScheduleCost {
    calls: u64,
    total: Duration,
    max: Duration,
    zero_delta_calls: u64,
    same_tick_calls: u64,
    last_tick: Option<u32>,
    phase_calls: [u64; 7],
    refresh_rigs: u64,
}
impl ScheduleCost {
    fn record(
        &mut self,
        elapsed: Duration,
        tick: u32,
        zero_delta: bool,
        phase: Phase,
        refresh_rigs: usize,
    ) {
        self.calls = self.calls.saturating_add(1);
        self.total = self.total.saturating_add(elapsed);
        self.max = self.max.max(elapsed);
        self.zero_delta_calls = self.zero_delta_calls.saturating_add(u64::from(zero_delta));
        self.same_tick_calls = self
            .same_tick_calls
            .saturating_add(u64::from(self.last_tick == Some(tick)));
        self.last_tick = Some(tick);
        let i = phase as usize;
        self.phase_calls[i] = self.phase_calls[i].saturating_add(1);
        self.refresh_rigs = self.refresh_rigs.saturating_add(refresh_rigs as u64);
    }
    fn report(&self) -> Value {
        json!({"count":self.calls,"total_ms":self.total.as_secs_f64()*1000.0,
            "max_ms":self.max.as_secs_f64()*1000.0,"zero_delta_count":self.zero_delta_calls,
            "same_tick_count":self.same_tick_calls,"refresh_rigs":self.refresh_rigs,
            "phase_counts": self.phase_calls})
    }
}
#[derive(Resource, Default)]
struct ReviewTiming {
    schedules: [ScheduleCost; 2],
    motion_rigs: u64,
    motion_phases: [Duration; 4],
}
impl ReviewTiming {
    fn report(&self) -> Value {
        json!({"version":1,"scope":"CPU-side schedule elapsed wall time; excludes render/GPU wait",
            "phase_order":["Apply","Pose","Camera","Pending","Advance","Finished","Failed"],
            "AdvanceReview":self.schedules[0].report(),"RefreshReview":self.schedules[1].report(),
            "motion_rig_steps":self.motion_rigs,
            "motion_phase_order":["pose_joints","robe","drapes_segments","hair_contacts"],
            "motion_phase_total_ms":self.motion_phases.map(|d|d.as_secs_f64()*1000.0)})
    }
}

#[derive(Resource)]
struct Run {
    script: Script,
    offscreen: bool,
    output: PathBuf,
    tick: u32,
    next_capture: usize,
    phase: Phase,
    clock: Time<Virtual>,
    deadline: Instant,
    manifest: Value,
    error: Option<String>,
}
#[derive(Resource, Clone, Default)]
struct Readiness(Arc<Mutex<ReadyState>>);
#[derive(Default)]
struct ReadyState {
    ready: bool,
    error: Option<String>,
    adapter: Value,
    meshes: Vec<AssetId<Mesh>>,
}
#[derive(Component)]
struct ActorState {
    spec: Actor,
    velocity: Vec3,
}
#[derive(Component)]
struct ReviewCamera;

/// The image used by both the camera and screenshot request in windowless mode.
#[derive(Resource)]
struct OffscreenTarget(Handle<Image>);

fn review_window(viewport: [u32; 2], offscreen: bool) -> WindowPlugin {
    if offscreen {
        WindowPlugin {
            primary_window: None,
            // The capture director owns the bounded exit, not window presence.
            exit_condition: ExitCondition::DontExit,
            ..default()
        }
    } else {
        WindowPlugin {
            primary_window: Some(Window {
                title: "Procedural model script review".into(),
                resolution: WindowResolution::new(viewport[0] as f32, viewport[1] as f32)
                    .with_scale_factor_override(1.0),
                ..default()
            }),
            ..default()
        }
    }
}

fn offscreen_image(viewport: [u32; 2]) -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: viewport[0],
            height: viewport[1],
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage |=
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC;
    image
}

fn run(path: &Path, output: &Path, offscreen: bool) -> Result<(), crate::Error> {
    if std::fs::metadata(path)?.len() > 1_048_576 {
        return Err("scene script exceeds 1 MiB".into());
    }
    let bytes = std::fs::read(path)?;
    let script = Script::parse(&bytes)?;
    // A new run never overwrites another run or follows an existing output symlink.
    std::fs::create_dir(output)?;
    std::fs::write(output.join("script.json"), &bytes)?;
    let manifest = json!({"version":1,"status":"running","script":script,
        "script_fingerprint_fnv1a64":fingerprint(&bytes),
        "executable":std::env::current_exe()?.to_string_lossy(),
        "capture_mode":if offscreen { "offscreen" } else { "window" },
        "captures":[],"provenance":"local executable; source revision not supplied"});
    std::fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    let viewport = script.viewport;
    let readiness = Readiness::default();
    let mut app = App::new();
    let plugins = crate::graphics::plugins().set(review_window(viewport, offscreen));
    if offscreen {
        app.add_plugins(plugins.disable::<WinitPlugin>())
            .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
                1.0 / 60.0,
            )));
    } else {
        app.add_plugins(plugins);
    }
    app.add_plugins(bevy::pbr::MaterialPlugin::<crate::cave::CaveMaterial>::default());
    bevy::asset::load_internal_asset!(app, crate::cave::SHADER, "cave.wgsl", Shader::from_wgsl);
    app.insert_resource(ClearColor(Color::srgb(0.08, 0.09, 0.12)))
        .insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 180.0,
            ..default()
        })
        .insert_resource(ReviewTiming::default())
        .insert_resource(creatures::MotionProfile::default())
        .insert_resource(readiness.clone())
        .insert_resource(Run {
            script,
            offscreen,
            output: output.to_path_buf(),
            tick: 0,
            next_capture: 0,
            phase: Phase::Apply,
            clock: Time::default(),
            deadline: Instant::now() + Duration::from_secs(180),
            manifest,
            error: None,
        })
        .add_systems(Startup, (crate::chrome::setup, setup).chain())
        .add_systems(First, clock.after(TimeSystem))
        .add_systems(
            Update,
            (
                advance_actors,
                advance_animation,
                apply,
                refresh_animation,
                creatures::snuff,
                creatures::effects,
                crate::scene::animate_props,
            )
                .chain(),
        )
        .add_systems(
            PostUpdate,
            ground_labels
                .after(bevy::render::camera::CameraUpdateSystem)
                .before(bevy::ui::UiSystem::Prepare),
        )
        .add_systems(Last, (track_drawn_meshes, capture).chain());
    configure_animation(&mut app);
    app.sub_app_mut(RenderApp)
        .insert_resource(readiness)
        .add_systems(Render, renderer_ready.in_set(RenderSet::Cleanup));
    let exit = app.run();
    let mut final_manifest: Value =
        serde_json::from_slice(&std::fs::read(output.join("manifest.json"))?)?;
    if exit.is_success() && final_manifest["status"] == "complete" {
        Ok(())
    } else {
        if final_manifest["status"] != "failed" {
            final_manifest["status"] = json!("failed");
            final_manifest["error"] = json!("renderer exited before every capture completed");
            std::fs::write(
                output.join("manifest.json"),
                serde_json::to_vec_pretty(&final_manifest)?,
            )?;
        }
        Err("scene script failed; see manifest.json".into())
    }
}
fn fingerprint(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    });
    format!("{hash:016x}")
}
fn renderer_ready(
    readiness: Res<Readiness>,
    pipelines: Res<PipelineCache>,
    meshes: Res<RenderAssets<RenderMesh>>,
    adapter: Res<RenderAdapterInfo>,
) {
    let error = pipelines.pipelines().find_map(|p| match &p.state {
        CachedPipelineState::Err(e) => Some(format!("{e:?}")),
        _ => None,
    });
    let mut state = readiness.0.lock().unwrap();
    let ready = error.is_none()
        && pipelines.waiting_pipelines().next().is_none()
        && !state.meshes.is_empty()
        && state.meshes.iter().all(|id| meshes.get(*id).is_some());
    state.ready = ready;
    state.error = error;
    state.adapter = json!({"name":adapter.name,"backend":format!("{:?}",adapter.backend),"driver":adapter.driver,"driver_info":adapter.driver_info});
}
fn clock(mut run: ResMut<Run>, mut virtual_time: ResMut<Time<Virtual>>, mut time: ResMut<Time>) {
    let delta = if run.phase == Phase::Advance {
        run.tick += 1;
        run.phase = Phase::Apply;
        Duration::from_secs_f64(1.0 / run.script.hz as f64)
    } else {
        Duration::ZERO
    };
    run.clock.advance_by(delta);
    *virtual_time = run.clock;
    *time = run.clock.as_generic();
}
fn setup(
    mut commands: Commands,
    run: Res<Run>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut cave_materials: ResMut<Assets<crate::cave::CaveMaterial>>,
) {
    let mut library = PropLibrary::new(PropMaterials::new(&mut materials));
    for spec in &run.script.actors {
        let at = Vec3::from_array(spec.position);
        let anim = spec.animation(at, spec.yaw);
        let entity = commands
            .spawn((
                ActorState {
                    spec: spec.clone(),
                    velocity: Vec3::ZERO,
                },
                anim,
                Transform::from_translation(at),
                Visibility::default(),
            ))
            .id();
        let rig = creatures::spawn(
            &mut commands,
            &mut library,
            &mut meshes,
            entity,
            &spec.look().expect("validated actor"),
        );
        if let Some(kind) = &spec.held_item {
            let hand = rig.joint(J::HandR).expect("validated Human hand");
            crate::scene::spawn_held_item(
                &mut commands,
                &mut library,
                &mut meshes,
                hand,
                item_model(kind).expect("validated held model"),
                tor_client_3d_model::item_surface(
                    item_model(kind).expect("validated held model"),
                    spec.held_asset.as_deref().unwrap_or(""),
                ),
            );
        }
        commands.entity(entity).insert(rig);
    }
    let mut groups = std::collections::BTreeMap::new();
    for spec in &run.script.ground_items {
        groups
            .entry(ground_cell(spec.position))
            .or_insert_with(Vec::new)
            .push(spec);
    }
    if run.script.pile_picker {
        let (cell, items) = groups
            .iter()
            .find(|(_, items)| items.len() > 4)
            .expect("validated pile");
        let rows = items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                (
                    tor_protocol::ItemTarget::from_digest([index as u8 + 1; 32]),
                    format!(
                        "{} ×{}",
                        item.disclosed_name
                            .as_deref()
                            .expect("validated disclosure"),
                        item.quantity
                    ),
                    true,
                )
            })
            .collect();
        crate::pile_picker::spawn(
            &mut commands,
            &crate::pile_picker::Drawing {
                at: *cell,
                rows,
                enabled: true,
            },
        );
    }
    for (cell, items) in groups {
        if items.len() > 4 {
            let models: Vec<_> = items
                .iter()
                .map(|spec| {
                    let kind = item_model(&spec.model).expect("validated ground model");
                    (
                        kind,
                        tor_client_3d_model::item_surface(
                            kind,
                            spec.asset.as_deref().unwrap_or(""),
                        ),
                    )
                })
                .collect();
            crate::scene::spawn_ground_pile(
                &mut commands,
                &mut library,
                &mut meshes,
                &models,
                crate::stand(crate::ivec(cell)),
            );
        } else {
            for spec in items {
                crate::scene::spawn_ground_item(
                    &mut commands,
                    &mut library,
                    &mut meshes,
                    item_model(&spec.model).expect("validated ground model"),
                    tor_client_3d_model::item_surface(
                        item_model(&spec.model).expect("validated ground model"),
                        spec.asset.as_deref().unwrap_or(""),
                    ),
                    Vec3::from_array(spec.position),
                    spec.yaw.to_radians(),
                );
            }
        }
    }
    commands.insert_resource(library);
    if run.script.dungeon {
        dungeon::setup(
            &mut commands,
            &run.script,
            &mut meshes,
            &mut materials,
            &mut cave_materials,
        );
    } else {
        commands.spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(100.0, 100.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb_from_array(run.script.floor_color),
                perceptual_roughness: 0.9,
                ..default()
            })),
            Transform::from_xyz(0.0, -0.5, 0.0),
        ));
        commands.spawn((
            DirectionalLight {
                illuminance: 8000.0,
                shadows_enabled: true,
                ..default()
            },
            Transform::from_xyz(3.0, 5.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
        ));
    }
    let target = if run.offscreen {
        let image = images.add(offscreen_image(run.script.viewport));
        commands.insert_resource(OffscreenTarget(image.clone()));
        RenderTarget::Image(image.into())
    } else {
        RenderTarget::default()
    };
    commands.spawn((
        ReviewCamera,
        bevy::ui::IsDefaultUiCamera,
        Camera {
            target,
            ..default()
        },
        Camera3d::default(),
        crate::graphics::camera_antialiasing(),
        Transform::from_xyz(0.0, 1.5, 4.0).looking_at(Vec3::Y * 0.5, Vec3::Y),
    ));
}
fn ground_cell([x, y, z]: [f32; 3]) -> tor_protocol::Position {
    tor_protocol::Position {
        x: x.round() as i32,
        y: z.round() as i32,
        z: (y + 0.5).round() as i32,
    }
}

/// Capture-only adapter: explicit fixture disclosure enters the same grouping,
/// layout and UI composition used by gameplay. This does not simulate live fog
/// or picking; those remain enforced by loot_labels::show in exploration.
fn ground_labels(
    mut commands: Commands,
    run: Res<Run>,
    camera: Query<(&Camera, &Transform), With<ReviewCamera>>,
    old: Query<Entity, With<crate::loot_labels::GroundLabel>>,
    mut previous: Local<Vec<crate::loot_labels::PlacedLabel>>,
) {
    if run.script.pile_picker {
        return;
    }
    let Ok((camera, transform)) = camera.single() else {
        return;
    };
    // ReviewCamera is an unparented root. Use its current transform before UI
    // layout rather than the previous frame's propagated GlobalTransform.
    let eye = GlobalTransform::from(*transform);
    let groups = crate::loot_labels::disclosed_groups(run.script.ground_items.iter().map(|item| {
        let name = item.disclosed_name.as_deref().unwrap_or("");
        let position = ground_cell(item.position);
        (position, name, item.quantity)
    }));
    let groups = if run.script.ground_labels {
        groups
    } else {
        crate::loot_labels::pile_counts(groups)
    };
    let anchors = groups
        .into_iter()
        .filter_map(|group| {
            let world = crate::stand(crate::ivec(group.position)) + Vec3::Y * 0.12;
            let at = camera.world_to_viewport(&eye, world).ok()?;
            Some((at, group))
        })
        .collect();
    let [w, h] = run.script.viewport;
    let next = crate::loot_labels::layout(anchors, Vec2::new(w as f32, h as f32));
    if *previous != next {
        for entity in &old {
            commands.entity(entity).despawn();
        }
        crate::loot_labels::spawn(&mut commands, &next);
        *previous = next;
    }
}

/// The meshes the scene draws now. A rebuild frees the old outfit's
/// wearer-owned meshes, which never reach the renderer again, so readiness
/// must not wait on every mesh asset that existed when it began.
fn track_drawn_meshes(drawn: Query<&Mesh3d>, readiness: Res<Readiness>) {
    readiness.0.lock().unwrap().meshes = drawn.iter().map(|mesh| mesh.id()).collect();
}
fn advance_actors(run: Res<Run>, mut actors: Query<(&ActorState, &mut Transform)>) {
    if run.phase == Phase::Apply && run.tick > 0 {
        for (actor, mut transform) in &mut actors {
            transform.translation += actor.velocity / run.script.hz as f32;
        }
    }
}

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct AdvanceReview;
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct RefreshReview;

/// Shared CPU pose/cloth schedule used by review playback and its benchmark.
pub(crate) fn animation_schedule() -> Schedule {
    let mut schedule = Schedule::new(AdvanceReview);
    schedule.add_systems(
        (
            creatures::animate,
            bevy::transform::systems::mark_dirty_trees,
            bevy::transform::systems::sync_simple_transforms,
            bevy::transform::systems::propagate_parent_transforms,
            creatures::soften,
        )
            .chain(),
    );
    schedule
}

fn configure_animation(app: &mut App) {
    app.add_schedule(animation_schedule());
    app.init_schedule(RefreshReview).add_systems(
        RefreshReview,
        (
            creatures::refresh_pose,
            bevy::transform::systems::mark_dirty_trees,
            bevy::transform::systems::sync_simple_transforms,
            bevy::transform::systems::propagate_parent_transforms,
        )
            .chain(),
    );
}
fn timed_animation(world: &mut World, label: impl ScheduleLabel, slot: usize) {
    // Live playback and ordinary gameplay do not opt into capture accounting.
    if !world.contains_resource::<ReviewTiming>() {
        world.run_schedule(label);
        return;
    }
    let (tick, phase) = {
        let run = world.resource::<Run>();
        (run.tick, run.phase)
    };
    let zero_delta = world.resource::<Time>().delta().is_zero();
    let refresh_rigs = if slot == 1 {
        world
            .query_filtered::<Entity, (With<Rig>, With<creatures::PoseRefresh>)>()
            .iter(world)
            .count()
    } else {
        0
    };
    let start = Instant::now();
    world.run_schedule(label);
    let elapsed = start.elapsed();
    let motion = (slot == 0).then(|| {
        let profile = world.resource::<creatures::MotionProfile>();
        (profile.rigs, profile.phases)
    });
    let mut timing = world.resource_mut::<ReviewTiming>();
    timing.schedules[slot].record(elapsed, tick, zero_delta, phase, refresh_rigs);
    if let Some((rigs, phases)) = motion {
        timing.motion_rigs = timing.motion_rigs.saturating_add(rigs as u64);
        for (total, elapsed) in timing.motion_phases.iter_mut().zip(phases) {
            *total = total.saturating_add(elapsed);
        }
    }
}
fn advance_animation(world: &mut World) {
    timed_animation(world, AdvanceReview, 0);
}
fn refresh_animation(world: &mut World) {
    timed_animation(world, RefreshReview, 1);
}

#[allow(clippy::too_many_arguments)]
fn apply(
    mut commands: Commands,
    mut run: ResMut<Run>,
    mut actors: Query<(Entity, &mut ActorState, &mut Anim, &mut Transform, &Rig)>,
    mut library: ResMut<PropLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    readiness: Res<Readiness>,
) {
    if run.phase != Phase::Apply {
        return;
    }
    for (entity, mut actor, mut anim, mut transform, rig) in &mut actors {
        let mut rebuild = false;
        for event in run.script.events.iter().filter(|e| e.tick == run.tick) {
            if actor.spec.id != event.actor {
                continue;
            }
            let ActorState { spec, velocity } = &mut *actor;
            rebuild |= apply_operation(spec, velocity, &mut anim, &mut transform, &event.operation);
            if matches!(
                event.operation,
                Operation::Action { .. } | Operation::Teleport { .. }
            ) {
                commands.entity(entity).insert(creatures::PoseRefresh);
            }
        }
        if rebuild {
            readiness.0.lock().unwrap().ready = false;
            if let Some(lift) = rig.joint(J::Lift) {
                commands.entity(lift).despawn();
            }
            let look: Look = actor.spec.look().expect("validated composition event");
            let rig = creatures::spawn(&mut commands, &mut library, &mut meshes, entity, &look);
            commands
                .entity(entity)
                .insert((rig, creatures::PoseRefresh));
        }
    }
    run.phase = Phase::Pose;
}
// Shared event semantics; rendering policies decide how to realize changed geometry.
fn apply_operation(
    spec: &mut Actor,
    velocity: &mut Vec3,
    anim: &mut Anim,
    transform: &mut Transform,
    operation: &Operation,
) -> bool {
    match operation {
        Operation::Action { name } => {
            if let Some(action) = schema::action(name).expect("validated action") {
                anim.act(action);
            } else {
                anim.action = None;
            }
        }
        Operation::Velocity { value } => *velocity = Vec3::from_array(*value),
        Operation::Teleport { position, yaw } => {
            transform.translation = Vec3::from_array(*position);
            *anim = spec.animation(transform.translation, *yaw);
        }
        Operation::Dress { outfit } => {
            if spec.outfit != *outfit {
                spec.outfit = outfit.clone();
                return true;
            }
        }
        Operation::Equipment { items } => {
            if spec.equipment != *items {
                spec.equipment = items.clone();
                return true;
            }
        }
    }
    false
}
fn save_manifest(run: &Run) -> std::io::Result<()> {
    let temp = run.output.join("manifest.pending.json");
    std::fs::write(&temp, serde_json::to_vec_pretty(&run.manifest)?)?;
    // Windows rename does not replace files. The manifest remains recoverable
    // as manifest.pending.json if replacement fails.
    let target = run.output.join("manifest.json");
    if target.exists() {
        std::fs::remove_file(&target)?;
    }
    std::fs::rename(temp, target)
}
fn cloth_contact_report(workloads: Vec<[usize; 3]>, fallbacks: u64) -> Value {
    json!({"shared_between_layers":true, "solver_fallbacks":fallbacks, "cloaks":workloads.into_iter().map(|[triangles,nodes,bin_refs]| {
        json!({"triangles":triangles,"bvh_nodes":nodes,"height_bin_references":bin_refs})
    }).collect::<Vec<_>>()})
}

#[allow(clippy::too_many_arguments)]
fn capture(
    mut commands: Commands,
    mut run: ResMut<Run>,
    timing: Option<Res<ReviewTiming>>,
    readiness: Res<Readiness>,
    offscreen: Option<Res<OffscreenTarget>>,
    mut camera: Query<(&mut Transform, &mut Projection), With<ReviewCamera>>,
    actors: Query<(&ActorState, &Anim, &Transform, &Rig, &Children), Without<ReviewCamera>>,
    joints: Query<&GlobalTransform>,
    mut exit: EventWriter<AppExit>,
) {
    if run.phase == Phase::Finished {
        return;
    }
    if let Some(timing) = timing.as_ref() {
        run.manifest["cpu_timing"] = timing.report();
    }
    let (ready, render_error, adapter) = {
        let state = readiness.0.lock().unwrap();
        (state.ready, state.error.clone(), state.adapter.clone())
    };
    if let Some(error) = render_error {
        run.error = Some(error);
        run.phase = Phase::Failed;
    }
    if Instant::now() > run.deadline {
        run.error = Some("scene or GPU readback timed out".into());
        run.phase = Phase::Failed;
    }
    if run.phase == Phase::Failed {
        run.manifest["status"] = json!("failed");
        run.manifest["error"] = json!(run.error);
        let _ = save_manifest(&run);
        run.phase = Phase::Finished;
        exit.write(AppExit::error());
        return;
    }
    if !ready || run.phase == Phase::Pending {
        return;
    }
    if run.next_capture == run.script.captures.len() {
        run.manifest["status"] = json!("complete");
        match save_manifest(&run) {
            Ok(()) => {
                run.phase = Phase::Finished;
                exit.write(AppExit::Success);
            }
            Err(e) => {
                run.error = Some(e.to_string());
                run.phase = Phase::Failed;
            }
        }
        return;
    }
    let shot = run.script.captures[run.next_capture].clone();
    if run.phase == Phase::Pose {
        if shot.tick > run.tick {
            run.phase = Phase::Advance;
            return;
        }
        let spec = run
            .script
            .cameras
            .iter()
            .find(|c| c.id == shot.camera)
            .unwrap();
        let Ok((mut transform, mut projection)) = camera.single_mut() else {
            return;
        };
        *transform = Transform::from_translation(Vec3::from_array(spec.position))
            .looking_at(Vec3::from_array(spec.target), Vec3::Y);
        *projection = Projection::Perspective(PerspectiveProjection {
            fov: spec.fov_degrees.to_radians(),
            near: 0.01,
            far: 2000.0,
            ..default()
        });
        // Camera changes in Last propagate next frame, while the pose is held.
        run.phase = Phase::Camera;
        return;
    }
    if run.phase != Phase::Camera {
        return;
    }
    let mut states:Vec<Value>=actors.iter().map(|(actor,anim,transform,rig,children)|json!({
        "id":actor.spec.id,"composition":actor.spec,"resolved_key":actor.spec.look().unwrap().key,
        "position":transform.translation.to_array(),"yaw":anim.yaw,"root_children":children.len(),
        "cloth_contact":cloth_contact_report(rig.cloth_contact_workloads(), rig.cloth_contact_fallbacks()),
        "joints": ([J::Hips,J::Torso,J::Head,J::HandL,J::HandR,J::LegL,J::LegR,J::ShinL,J::ShinR,J::FootL,J::FootR].into_iter().filter_map(|joint| {
            let entity=rig.joint(joint)?;
            let matrix=joints.get(entity).ok()?.compute_matrix().to_cols_array();
            Some((format!("{joint:?}"),json!(matrix)))
        }).collect::<serde_json::Map<String,Value>>()),
        "action":anim.action.map(|(a,t)|json!({"name":format!("{a:?}"),"phase":t}))
    })).collect();
    states.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    let camera_spec = run
        .script
        .cameras
        .iter()
        .find(|c| c.id == shot.camera)
        .unwrap()
        .clone();
    let report = json!({"name":shot.name,"tick":run.tick,"clock_ns":run.clock.elapsed().as_nanos().to_string(),
        "camera":camera_spec,"actors":states,"renderer":adapter,"cpu_timing":run.manifest.get("cpu_timing")});
    let output = run.output.clone();
    run.phase = Phase::Pending;
    let screenshot = match offscreen {
        Some(target) => Screenshot::image(target.0.clone()),
        None => Screenshot::primary_window(),
    };
    commands.spawn(screenshot).observe(
        move |trigger: Trigger<ScreenshotCaptured>, mut run: ResMut<Run>| {
            let result = (|| -> Result<Value, crate::Error> {
                let image = trigger.event().0.clone().try_into_dynamic()?;
                let temporary = output.join(format!("{}.pending.png", shot.name));
                let path = output.join(format!("{}.png", shot.name));
                image.save(&temporary)?;
                let bytes = std::fs::read(&temporary)?;
                std::fs::rename(&temporary, &path)?;
                let mut entry = report.clone();
                entry["width"] = image.width().into();
                entry["height"] = image.height().into();
                entry["image_fingerprint_fnv1a64"] = fingerprint(&bytes).into();
                Ok(entry)
            })();
            match result {
                Ok(entry) => {
                    run.manifest["captures"].as_array_mut().unwrap().push(entry);
                    run.next_capture += 1;
                    run.phase = Phase::Pose;
                    if let Err(e) = save_manifest(&run) {
                        run.error = Some(e.to_string());
                        run.phase = Phase::Failed;
                    }
                }
                Err(e) => {
                    run.error = Some(e.to_string());
                    run.phase = Phase::Failed;
                }
            }
        },
    );
}

#[derive(Debug, PartialEq, Eq)]
struct CaptureOptions {
    script: PathBuf,
    output: PathBuf,
    offscreen: bool,
}

fn parse_cli(arguments: &[String]) -> Result<CaptureOptions, crate::Error> {
    let mut script = None;
    let mut output = None;
    let mut offscreen = false;
    let mut args = arguments.iter();
    while let Some(arg) = args.next() {
        if arg == "--offscreen" {
            if offscreen {
                return Err("duplicate --offscreen".into());
            }
            offscreen = true;
            continue;
        }
        let slot = match arg.as_str() {
            "--scene-script" => &mut script,
            "--capture" => &mut output,
            _ => return Err(format!("{arg} cannot be combined with --scene-script").into()),
        };
        if slot.is_some() {
            return Err(format!("duplicate {arg}").into());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {arg}"))?;
        if value.starts_with("--") {
            return Err(format!("missing value for {arg}").into());
        }
        *slot = Some(PathBuf::from(value));
    }
    Ok(CaptureOptions {
        script: script.ok_or("--scene-script is required")?,
        output: output.ok_or("--scene-script requires --capture with a new output directory")?,
        offscreen,
    })
}

pub(crate) fn cli(arguments: &[String]) -> Result<(), crate::Error> {
    let options = parse_cli(arguments)?;
    run(&options.script, &options.output, options.offscreen)
}

#[cfg(test)]
mod timing_tests {
    use super::*;
    #[test]
    fn cloth_contact_counts_are_additive_capture_metadata() {
        let report = cloth_contact_report(vec![[123, 31, 246], [123, 31, 246]], 2);
        let manifest = json!({"status":"complete","captures":[{"actors":[{"id":"cape","cloth_contact":report}]}]});
        let decoded: Value =
            serde_json::from_slice(&serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert_eq!(decoded["status"], "complete");
        assert_eq!(
            decoded["captures"][0]["actors"][0]["cloth_contact"]["cloaks"][0]["triangles"],
            123
        );
        assert_eq!(
            decoded["captures"][0]["actors"][0]["cloth_contact"]["cloaks"][1]["bvh_nodes"],
            31
        );
        assert_eq!(report["solver_fallbacks"], json!(2));
        assert_eq!(cloth_contact_report(Vec::new(), 0)["cloaks"], json!([]));
    }

    #[test]
    fn bounded_schedule_counters_preserve_capture_and_failure_schema() {
        let mut timing = ReviewTiming::default();
        timing.schedules[0].record(Duration::from_millis(3), 7, false, Phase::Apply, 0);
        timing.schedules[0].record(Duration::from_millis(9), 7, true, Phase::Pending, 0);
        timing.schedules[0].record(Duration::from_millis(2), 8, false, Phase::Apply, 0);
        timing.schedules[1].record(Duration::from_millis(5), 8, true, Phase::Pose, 2);
        let report = timing.report();
        let advance = &report["AdvanceReview"];
        assert_eq!(advance["count"], 3);
        assert_eq!(advance["total_ms"], 14.0);
        assert_eq!(advance["max_ms"], 9.0);
        assert_eq!(advance["zero_delta_count"], 1);
        assert_eq!(advance["same_tick_count"], 1);
        assert_eq!(advance["phase_counts"][Phase::Apply as usize], 2);
        assert_eq!(advance["phase_counts"][Phase::Pending as usize], 1);
        assert_eq!(report["RefreshReview"]["refresh_rigs"], 2);
        assert_eq!(
            report["phase_order"].as_array().unwrap().len(),
            advance["phase_counts"].as_array().unwrap().len()
        );
        for status in ["running", "complete", "failed"] {
            let manifest = json!({"status":status,"cpu_timing":report});
            let decoded: Value =
                serde_json::from_slice(&serde_json::to_vec(&manifest).unwrap()).unwrap();
            assert_eq!(decoded["cpu_timing"], report);
        }
    }
}
