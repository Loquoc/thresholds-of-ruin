use super::animation::ease;
use super::*;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Effects: bolts, flashes, shockwaves and wards.

#[derive(Component)]
pub struct Bolt {
    from: Vec3,
    to: Vec3,
    delay: Duration,
    elapsed: Duration,
    wobble: f32,
    accent: Accent,
    duration: Duration,
    arc: bool,
    impact: bool,
    physical: bool,
}

#[derive(Component)]
pub struct Fade {
    t: f32,
    life: f32,
    grow: f32,
    base: f32,
}

fn sphere(library: &mut PropLibrary, meshes: &mut Assets<Mesh>, accent: Accent) -> Model {
    library.get(format!("fx-sphere:{accent:?}"), meshes, move || {
        let mut p = Parts {
            accent: Some(accent),
            ..default()
        };
        p.glow.blob(Vec3::ZERO, Vec3::ONE, 0, 0.0, Vec3::ONE);
        p
    })
}

fn ring(library: &mut PropLibrary, meshes: &mut Assets<Mesh>, accent: Accent) -> Model {
    library.get(format!("fx-ring:{accent:?}"), meshes, move || {
        let mut p = Parts {
            accent: Some(accent),
            ..default()
        };
        p.glow
            .torus(Vec3::ZERO, Vec3::Y, 1.0, 0.035, 32, 3, Vec3::ONE);
        p
    })
}

/// Small cached silhouettes; no per-frame mesh allocation or extra lights.
fn effect_form(
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    form: tor_protocol::EffectAppearance,
    accent: Accent,
) -> Model {
    use tor_protocol::EffectAppearance;
    library.get(format!("fx-form:{form:?}:{accent:?}"), meshes, move || {
        let mut parts = Parts {
            accent: Some(accent),
            ..default()
        };
        match form {
            EffectAppearance::Bolt => {
                parts
                    .glow
                    .blob(Vec3::ZERO, Vec3::new(3.5, 0.45, 0.45), 0, 0.0, Vec3::ONE);
            }
            EffectAppearance::Missile => {
                parts
                    .metal
                    .blob(Vec3::ZERO, Vec3::new(2.8, 0.3, 0.3), 0, 0.0, Vec3::ONE);
            }
            EffectAppearance::Halo => {
                for radius in [0.7, 1.0] {
                    parts
                        .glow
                        .torus(Vec3::ZERO, Vec3::Y, radius, 0.035, 24, 3, Vec3::ONE);
                }
            }
            EffectAppearance::Cloud => {
                for point in [
                    Vec3::ZERO,
                    Vec3::new(-0.4, 0.2, 0.0),
                    Vec3::new(0.35, 0.0, 0.3),
                ] {
                    parts.glow.blob(point, Vec3::splat(0.65), 0, 0.0, Vec3::ONE);
                }
            }
            EffectAppearance::Flash | EffectAppearance::StatusIndicator => {
                for scale in [
                    Vec3::new(1.0, 0.18, 0.18),
                    Vec3::new(0.18, 1.0, 0.18),
                    Vec3::new(0.18, 0.18, 1.0),
                ] {
                    parts.glow.blob(Vec3::ZERO, scale, 0, 0.0, Vec3::ONE);
                }
            }
        }
        parts
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "effect construction receives its asset stores and trajectory together"
)]
pub fn bolt(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    from: Vec3,
    to: Vec3,
    accent: Accent,
    delay: f32,
    wobble: f32,
) {
    let model = sphere(library, meshes, accent);
    commands
        .spawn((
            Bolt {
                from,
                to,
                delay: Duration::from_secs_f32(delay.max(0.0)),
                elapsed: Duration::ZERO,
                wobble,
                accent,
                duration: Duration::from_millis(320),
                arc: true,
                impact: true,
                physical: false,
            },
            Transform::from_translation(from).with_scale(Vec3::ZERO),
            Visibility::default(),
        ))
        .with_children(|p| {
            model.attach(p);
            p.spawn(PointLight {
                color: accent.light(),
                intensity: 12_000.0,
                range: 4.0,
                shadows_enabled: false,
                ..default()
            });
        });
}

pub fn flash(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    at: Vec3,
    accent: Accent,
    size: f32,
    delay: f32,
) {
    let model = sphere(library, meshes, accent);
    commands
        .spawn((
            Fade {
                t: -delay,
                life: 0.3,
                grow: size,
                base: size * 0.2,
            },
            Transform::from_translation(at).with_scale(Vec3::ZERO),
            Visibility::default(),
        ))
        .with_children(|p| model.attach(p));
}

pub fn shockwave(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    at: Vec3,
    accent: Accent,
    delay: f32,
) {
    let model = ring(library, meshes, accent);
    commands
        .spawn((
            Fade {
                t: -delay,
                life: 0.5,
                grow: 1.4,
                base: 0.15,
            },
            Transform::from_translation(at).with_scale(Vec3::ZERO),
            Visibility::default(),
        ))
        .with_children(|p| model.attach(p));
}

/// A ward: rings of light rising around a figure.
pub fn ward(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    at: Vec3,
    accent: Accent,
) {
    for k in 0..3 {
        let model = ring(library, meshes, accent);
        commands
            .spawn((
                Fade {
                    t: -(k as f32) * 0.12,
                    life: 0.8,
                    grow: 0.25,
                    base: 0.5,
                },
                Transform::from_translation(at + Vec3::Y * (0.3 + k as f32 * 0.5))
                    .with_scale(Vec3::ZERO),
                Visibility::default(),
            ))
            .with_children(|p| model.attach(p));
    }
}

pub fn effects(
    mut commands: Commands,
    time: Res<Time>,
    mut library: ResMut<PropLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut bolts: Query<(Entity, &mut Bolt, &mut Transform), Without<Fade>>,
    mut fades: Query<(Entity, &mut Fade, &mut Transform), Without<Bolt>>,
) {
    let delta = time.delta();
    let dt = delta.as_secs_f32();
    for (e, mut bolt, mut transform) in &mut bolts {
        let travel_dt = if !bolt.delay.is_zero() {
            if delta < bolt.delay {
                bolt.delay -= delta;
                continue;
            }
            let remaining = delta - bolt.delay;
            bolt.delay = Duration::ZERO;
            remaining
        } else {
            delta
        };
        bolt.elapsed += travel_dt;
        let t = (bolt.elapsed.as_secs_f32() / bolt.duration.as_secs_f32()).min(1.0);
        let side = (bolt.to - bolt.from).cross(Vec3::Y).normalize_or_zero();
        let arc = if bolt.arc {
            Vec3::Y * (t * PI).sin() * 0.25 + side * (t * TAU * 1.5).sin() * bolt.wobble
        } else {
            Vec3::ZERO
        };
        transform.translation = bolt.from.lerp(bolt.to, t) + arc;
        if !bolt.arc {
            transform.rotation =
                Quat::from_rotation_arc(Vec3::X, (bolt.to - bolt.from).normalize_or(Vec3::X));
        }
        transform.scale = if bolt.physical {
            Vec3::ONE
        } else {
            Vec3::splat(0.09 + 0.02 * (time.elapsed_secs() * 40.0).sin())
        };
        if bolt.elapsed >= bolt.duration {
            commands.entity(e).despawn();
            if bolt.impact {
                flash(
                    &mut commands,
                    &mut library,
                    &mut meshes,
                    bolt.to,
                    bolt.accent,
                    0.45,
                    0.0,
                );
            }
        }
    }
    for (e, mut fade, mut transform) in &mut fades {
        fade.t += dt;
        if fade.t < 0.0 {
            continue;
        }
        let k = fade.t / fade.life;
        if k >= 1.0 {
            commands.entity(e).despawn();
            continue;
        }
        let size = fade.base + fade.grow * ease(k);
        let thin = 1.0 - k * k;
        transform.scale = Vec3::new(size, size * thin.max(0.05), size);
        if fade.base < 0.3 && fade.grow < 1.0 {
            transform.scale = Vec3::splat(size * thin.max(0.05));
        }
    }
}

/// A ring of light that hangs in the air and fades.
fn halo(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    at: Vec3,
    accent: Accent,
    size: f32,
    delay: f32,
) {
    let model = ring(library, meshes, accent);
    commands
        .spawn((
            Fade {
                t: -delay,
                life: 0.7,
                grow: size,
                base: size * 0.4,
            },
            Transform::from_translation(at).with_scale(Vec3::ZERO),
            Visibility::default(),
        ))
        .with_children(|p| model.attach(p));
}

/// Stage a move's effects between a caster and its target (feet positions
/// and heights). Returns, when the target is struck, the delay until it
/// reacts to the blow or the spell's arrival.
#[allow(clippy::too_many_arguments)]
pub fn stage(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    action: Move,
    from: (Vec3, f32),
    to: Option<(Vec3, f32)>,
    landed: bool,
) -> Option<f32> {
    let impact = action.impact() * action.duration();
    let (caster, height) = from;
    let chest = |(at, h): (Vec3, f32)| at + Vec3::Y * h * 0.55;
    let toward = to.map_or(Vec3::Z, |(at, _)| (at - caster).normalize_or(Vec3::Z));
    let hand = caster + Vec3::Y * height * 0.7 + toward * 0.35;
    match action {
        // A theft cue does not reveal the actual object, nor add a strike flash.
        Move::Gesture | Move::Filch => return None,
        Move::Guard(accent) => {
            ward(commands, library, meshes, caster, accent);
            return None;
        }
        Move::Focus => {
            halo(
                commands,
                library,
                meshes,
                caster + Vec3::Y * height * 0.92,
                Accent::Teal,
                0.25,
                0.2,
            );
            halo(
                commands,
                library,
                meshes,
                caster + Vec3::Y * height * 0.92,
                Accent::Teal,
                0.25,
                0.55,
            );
            return None;
        }
        Move::Step => {
            shockwave(
                commands,
                library,
                meshes,
                caster + Vec3::Y * 0.04,
                Accent::White,
                impact,
            );
            return None;
        }
        Move::Mend => {
            ward(commands, library, meshes, caster, Accent::Teal);
            for k in 0..3 {
                flash(
                    commands,
                    library,
                    meshes,
                    caster + Vec3::Y * (0.5 + 0.12 * k as f32),
                    Accent::Teal,
                    0.18,
                    impact + k as f32 * 0.2,
                );
            }
            return None;
        }
        Move::Fear => {
            for k in 0..3 {
                halo(
                    commands,
                    library,
                    meshes,
                    caster + Vec3::Y * height * 0.85,
                    Accent::Violet,
                    0.9 + k as f32 * 0.4,
                    impact + k as f32 * 0.1,
                );
            }
        }
        _ => {}
    }
    let target = to?;
    let travel = match action {
        Move::Cast(accent) => {
            bolt(
                commands,
                library,
                meshes,
                hand,
                chest(target),
                accent,
                impact,
                0.0,
            );
            0.32
        }
        Move::Hex => {
            bolt(
                commands,
                library,
                meshes,
                hand,
                chest(target),
                Accent::Violet,
                impact,
                0.2,
            );
            0.32
        }
        Move::Breath(accent) => {
            // A stream poured from the open jaw.
            let mouth = caster + Vec3::Y * height * 0.72 + toward * 0.55;
            for k in 0..5 {
                bolt(
                    commands,
                    library,
                    meshes,
                    mouth,
                    chest(target) + Vec3::Y * (k as f32 - 2.0) * 0.06,
                    accent,
                    impact + k as f32 * 0.07,
                    0.1,
                );
            }
            0.3
        }
        Move::Gaze => {
            // The eyes kindle, and the stare crosses to the target's eyes.
            let eyes = caster + Vec3::Y * height * 0.9 + toward * 0.12;
            halo(
                commands,
                library,
                meshes,
                eyes,
                Accent::Amber,
                0.3,
                impact * 0.6,
            );
            bolt(
                commands,
                library,
                meshes,
                eyes,
                target.0 + Vec3::Y * target.1 * 0.9,
                Accent::Violet,
                impact,
                0.0,
            );
            0.25
        }
        Move::Drain => {
            // The life leaves the target and flows back into the caster.
            for k in 0..3 {
                bolt(
                    commands,
                    library,
                    meshes,
                    chest(target),
                    hand,
                    Accent::Ember,
                    impact + k as f32 * 0.12,
                    0.12,
                );
            }
            0.0
        }
        _ => 0.0,
    };
    if landed {
        let (accent, size) = match action {
            Move::Touch(accent) => (accent, 0.55),
            Move::Bite(accent) => (accent, 0.4),
            Move::Breath(accent) => (accent, 0.5),
            Move::Heavy => (Accent::Gold, 0.45),
            Move::Bash | Move::Shove => (Accent::Gold, 0.35),
            Move::Cast(_) | Move::Hex | Move::Drain | Move::Fear | Move::Gaze => {
                (Accent::White, 0.0)
            }
            _ => (Accent::White, 0.32),
        };
        if size > 0.0 {
            flash(
                commands,
                library,
                meshes,
                chest(target),
                accent,
                size,
                impact,
            );
        }
        if action == Move::Heavy {
            shockwave(
                commands,
                library,
                meshes,
                target.0 + Vec3::Y * 0.05,
                Accent::Gold,
                impact,
            );
        }
    }
    Some(impact + travel)
}

fn effect_accent(visual: tor_protocol::EffectVisual) -> Accent {
    use tor_protocol::EffectVisual;
    match visual {
        EffectVisual::Neutral => Accent::White,
        EffectVisual::Flame => Accent::Ember,
        EffectVisual::Frost => Accent::Teal,
        EffectVisual::Sparks => Accent::Violet,
        EffectVisual::Vapor => Accent::Teal,
        EffectVisual::Radiance => Accent::Gold,
    }
}

/// A snapshot-owned marker, deliberately without a client countdown component.
/// The next authoritative observation updates or removes its root entity.
pub fn public_effect(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    appearance: tor_protocol::EffectAppearance,
    visual: tor_protocol::EffectVisual,
    points: &[Vec<Vec3>],
) -> Entity {
    let accent = effect_accent(visual);
    let model = effect_form(library, meshes, appearance, accent);
    let size = if appearance == tor_protocol::EffectAppearance::Cloud {
        0.4
    } else {
        0.18
    };
    commands
        .spawn((Transform::default(), Visibility::default()))
        .with_children(|root| {
            for point in points.iter().flatten() {
                root.spawn((
                    Transform::from_translation(*point).with_scale(Vec3::splat(size)),
                    Visibility::default(),
                ))
                .with_children(|child| model.attach(child));
            }
        })
        .id()
}

/// Cosmetic missile speed, in cells per second. Never derived from server data.
pub const MISSILE_CELLS_PER_SECOND: f64 = 12.5;

/// Present a server-resolved instant. Finite playback is cosmetic, not effect lifetime.
/// Present only server-disclosed cell paths. Within a segment a missile flies
/// straight along each run of its disclosed cells; it never extends past a
/// segment, never crosses a hidden gap (the gap only delays the next segment),
/// and never flies toward an undisclosed cell. Styling and speed are cosmetic,
/// and no impact flash is invented. `launch` delays the whole flight.
#[expect(
    clippy::too_many_arguments,
    reason = "authoritative descriptor and geometry are independent inputs"
)]
pub fn stage_authoritative(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    timing: tor_protocol::EffectTiming,
    presentation: &tor_protocol::EffectPresentation,
    object_model: Option<tor_client_3d_model::ItemModel>,
    delivery: tor_protocol::EffectDelivery,
    _cue: tor_protocol::EffectCue,
    segments: &[Vec<Vec3>],
    launch: Duration,
) {
    use tor_protocol::{EffectAppearance, EffectDelivery, EffectTiming};
    let EffectTiming::Instant = timing;
    let appearance = presentation.form;
    // Private status belongs in the HUD, never an implied world-visible aura.
    if appearance == EffectAppearance::StatusIndicator {
        return;
    }
    let accent = effect_accent(presentation.visual);
    let path = matches!(delivery, EffectDelivery::Ray | EffectDelivery::Projectile)
        && matches!(
            appearance,
            EffectAppearance::Bolt | EffectAppearance::Missile
        );
    for segment in segments {
        if !path || segment.len() == 1 {
            for point in segment {
                let model = effect_form(library, meshes, appearance, accent);
                let size = if appearance == EffectAppearance::Cloud {
                    0.4
                } else {
                    0.18
                };
                commands
                    .spawn((
                        Fade {
                            t: 0.0,
                            life: 0.3,
                            grow: size,
                            base: size * 0.2,
                        },
                        Transform::from_translation(*point).with_scale(Vec3::ZERO),
                        Visibility::default(),
                    ))
                    .with_children(|p| model.attach(p));
            }
        }
    }
    // Paths fly as straight runs, timed across every segment and gap.
    if !path {
        return;
    }
    let cells: Vec<Vec<[f32; 3]>> = segments
        .iter()
        .map(|segment| segment.iter().map(|point| point.to_array()).collect())
        .collect();
    for leg in tor_client_3d_model::playback::flight_legs(&cells, MISSILE_CELLS_PER_SECOND) {
        let model = match object_model {
            Some(kind) => library.item(kind, meshes),
            None => effect_form(library, meshes, appearance, accent),
        };
        let from = Vec3::from_array(leg.from);
        commands
            .spawn((
                Bolt {
                    from,
                    to: Vec3::from_array(leg.to),
                    delay: launch + Duration::from_secs_f64(leg.delay),
                    elapsed: Duration::ZERO,
                    wobble: 0.0,
                    accent,
                    duration: Duration::from_secs_f64(leg.duration.max(0.01)),
                    arc: false,
                    impact: false,
                    physical: object_model.is_some(),
                },
                Transform::from_translation(from).with_scale(Vec3::ZERO),
                Visibility::default(),
            ))
            .with_children(|p| model.attach(p));
    }
}

#[cfg(test)]
mod authoritative_tests {
    use super::*;
    use crate::{props::PropMaterials, Game};
    use tor_client_3d_model::Happening;
    use tor_protocol::{EffectAppearance, EffectCue, EffectDelivery, EffectTiming, Position};

    #[test]
    fn delayed_bolt_uses_the_remainder_of_its_starting_frame() {
        // The first two cases cross or exactly reach the delay boundary. The
        // final case remains hidden; none may discard elapsed travel time.
        for (delay, expected_x, visible) in
            [(0.03, 0.02, true), (0.05, 0.0, true), (0.08, 0.0, false)]
        {
            let mut app = App::new();
            let mut materials = Assets::<StandardMaterial>::default();
            let library = PropLibrary::new(PropMaterials::new(&mut materials));
            let mut time = Time::<()>::default();
            time.advance_by(std::time::Duration::from_millis(50));
            app.insert_resource(time)
                .init_resource::<Assets<Mesh>>()
                .insert_resource(library)
                .add_systems(Update, effects);
            let entity = app
                .world_mut()
                .spawn((
                    Bolt {
                        from: Vec3::ZERO,
                        to: Vec3::X,
                        delay: Duration::from_secs_f64(delay),
                        elapsed: Duration::ZERO,
                        wobble: 0.0,
                        accent: Accent::White,
                        duration: Duration::from_secs(1),
                        arc: false,
                        impact: false,
                        physical: false,
                    },
                    Transform::from_scale(Vec3::ZERO),
                ))
                .id();
            app.update();
            let transform = app.world().get::<Transform>(entity).unwrap();
            assert!((transform.translation.x - expected_x).abs() < 1e-6);
            assert_eq!(transform.scale.x > 0.0, visible);
        }
    }

    #[test]
    fn contiguous_hops_never_blank_at_fixed_tick_boundaries() {
        let mut app = App::new();
        let mut materials = Assets::<StandardMaterial>::default();
        app.insert_resource(Time::<()>::default())
            .insert_resource(PropLibrary::new(PropMaterials::new(&mut materials)))
            .init_resource::<Assets<Mesh>>()
            .add_systems(Update, effects);
        for index in 0..4 {
            app.world_mut().spawn((
                Bolt {
                    from: Vec3::X * index as f32,
                    to: Vec3::X * (index + 1) as f32,
                    delay: Duration::from_millis(index as u64 * 80),
                    elapsed: Duration::ZERO,
                    wobble: 0.0,
                    accent: Accent::White,
                    duration: Duration::from_millis(80),
                    arc: false,
                    impact: false,
                    physical: false,
                },
                Transform::from_scale(Vec3::ZERO),
            ));
        }
        for tick in 1..16 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_millis(20));
            app.update();
            let visible = app
                .world_mut()
                .query_filtered::<&Transform, With<Bolt>>()
                .iter(app.world())
                .filter(|transform| transform.scale.x > 0.0)
                .count();
            assert!(visible > 0, "blank at 50 Hz tick {tick}");
            // Capture holds are real zero-delta updates, not additional simulation time.
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::ZERO);
            app.update();
        }
    }

    #[test]
    fn cached_projectile_meshes_keep_bounds_after_cpu_extraction() {
        use bevy::render::primitives::Aabb;
        let mut materials = Assets::<StandardMaterial>::default();
        let mut library = PropLibrary::new(PropMaterials::new(&mut materials));
        let mut meshes = Assets::<Mesh>::default();
        let first = library.item(tor_client_3d_model::ItemModel::Arrow, &mut meshes);
        assert!(!first.0.is_empty());
        for (handle, _) in &first.0 {
            assert!(meshes.remove(handle).is_some());
        }
        // RENDER_WORLD-only CPU geometry has gone, but cached GPU handles remain reusable.
        let late = library.item(tor_client_3d_model::ItemModel::Arrow, &mut meshes);
        assert_eq!(first.0.len(), late.0.len());
        let mut world = World::new();
        world
            .commands()
            .spawn(Transform::default())
            .with_children(|p| late.attach(p));
        world.flush();
        let drawn: Vec<_> = world
            .query_filtered::<Option<&Aabb>, With<Mesh3d>>()
            .iter(&world)
            .collect();
        assert_eq!(drawn.len(), first.0.len());
        assert!(drawn
            .iter()
            .all(|bounds| bounds.is_some_and(|b| b.center.is_finite()
                && b.half_extents.is_finite()
                && b.half_extents.max_element() > 0.0)));
    }

    #[test]
    fn appearance_is_distinct_from_delivery_and_private_status_has_no_world_aura() {
        for (appearance, expected_flashes) in [
            (EffectAppearance::Flash, 2),
            (EffectAppearance::Halo, 2),
            (EffectAppearance::Cloud, 2),
            (EffectAppearance::StatusIndicator, 0),
        ] {
            let mut app = App::new();
            let mut materials = Assets::<StandardMaterial>::default();
            let library = PropLibrary::new(PropMaterials::new(&mut materials));
            app.init_resource::<Game>()
                .init_resource::<Assets<Mesh>>()
                .insert_resource(materials)
                .insert_resource(library)
                .add_systems(Update, crate::scene::react);
            app.world_mut()
                .resource_mut::<Game>()
                .pending
                .push(Happening::Effect {
                    caster: None,
                    ability: None,
                    tick: 4,
                    timing: EffectTiming::Instant,
                    presentation: tor_protocol::EffectPresentation::neutral(appearance),
                    object_model: None,
                    delivery: EffectDelivery::Ray,
                    cue: EffectCue::Neutral,
                    segments: vec![vec![
                        Position { x: 0, y: 0, z: 0 },
                        Position { x: 1, y: 0, z: 0 },
                    ]],
                });
            app.update();
            assert_eq!(
                app.world_mut().query::<&Bolt>().iter(app.world()).count(),
                0
            );
            assert_eq!(
                app.world_mut().query::<&Fade>().iter(app.world()).count(),
                expected_flashes
            );
        }
    }

    #[test]
    fn authoritative_rays_use_exact_segments_without_arcs_or_impact_claims() {
        let mut app = App::new();
        let mut materials = Assets::<StandardMaterial>::default();
        let library = PropLibrary::new(PropMaterials::new(&mut materials));
        app.init_resource::<Game>()
            .init_resource::<Assets<Mesh>>()
            .insert_resource(materials)
            .insert_resource(library)
            .add_systems(Update, crate::scene::react);
        app.world_mut()
            .resource_mut::<Game>()
            .pending
            .push(Happening::Effect {
                caster: None,
                ability: None,
                tick: 4,
                timing: EffectTiming::Instant,
                presentation: tor_protocol::EffectPresentation::neutral(EffectAppearance::Bolt),
                object_model: None,
                delivery: EffectDelivery::Ray,
                cue: EffectCue::Neutral,
                segments: vec![
                    vec![
                        Position { x: 0, y: 0, z: 0 },
                        Position { x: 1, y: 0, z: 0 },
                        Position { x: 0, y: 0, z: 0 },
                    ],
                    vec![Position { x: 8, y: 0, z: 0 }, Position { x: 8, y: 1, z: 0 }],
                ],
            });
        app.update();
        let mut query = app.world_mut().query::<&Bolt>();
        let bolts: Vec<_> = query.iter(app.world()).collect();
        assert_eq!(bolts.len(), 3);
        let paths: Vec<_> = bolts.iter().map(|b| (b.from, b.to)).collect();
        let point = |x, y| Vec3::new(x, 0.05, y);
        for (from, to) in [
            (point(0.0, 0.0), point(1.0, 0.0)),
            (point(1.0, 0.0), point(0.0, 0.0)),
            (point(8.0, 0.0), point(8.0, 1.0)),
        ] {
            assert!(paths
                .iter()
                .any(|(a, b)| a.distance(from) < 1e-6 && b.distance(to) < 1e-6));
        }
        assert!(bolts.iter().all(|b| !b.arc && !b.impact));
    }

    #[test]
    fn an_off_axis_missile_flies_one_straight_line_along_its_disclosed_cells() {
        let mut app = App::new();
        let mut materials = Assets::<StandardMaterial>::default();
        let library = PropLibrary::new(PropMaterials::new(&mut materials));
        app.init_resource::<Game>()
            .init_resource::<Assets<Mesh>>()
            .insert_resource(materials)
            .insert_resource(library)
            .add_systems(Update, crate::scene::react);
        // The server's per-axis stepper zigzags across a shallow diagonal.
        let cells = [(0, 0), (1, 0), (2, 1), (3, 1), (4, 2), (5, 2), (6, 3)];
        app.world_mut()
            .resource_mut::<Game>()
            .pending
            .push(Happening::Effect {
                caster: None,
                ability: None,
                tick: 4,
                timing: EffectTiming::Instant,
                presentation: tor_protocol::EffectPresentation::neutral(EffectAppearance::Missile),
                object_model: None,
                delivery: EffectDelivery::Projectile,
                cue: EffectCue::Neutral,
                segments: vec![cells
                    .iter()
                    .map(|&(x, y)| Position { x, y, z: 0 })
                    .collect()],
            });
        app.update();
        let mut query = app.world_mut().query::<&Bolt>();
        let bolts: Vec<_> = query.iter(app.world()).collect();
        let point = |x, y| Vec3::new(x, 0.05, y);
        assert_eq!(bolts.len(), 1, "one heading, not a wiggle per cell");
        assert!(bolts[0].from.distance(point(0.0, 0.0)) < 1e-6);
        assert!(bolts[0].to.distance(point(6.0, 3.0)) < 1e-6);
        // Fixed cosmetic speed over the straight length.
        let expected = 45f64.sqrt() / MISSILE_CELLS_PER_SECOND;
        assert!((bolts[0].duration.as_secs_f64() - expected).abs() < 1e-6);
        assert_eq!(bolts[0].delay, Duration::ZERO);
    }
}
