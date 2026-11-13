use super::animation::ease;
use super::*;

// ---------------------------------------------------------------------------
// Effects: bolts, flashes, shockwaves and wards.

#[derive(Component)]
pub struct Bolt {
    from: Vec3,
    to: Vec3,
    delay: f32,
    t: f32,
    wobble: f32,
    accent: Accent,
    duration: f32,
    arc: bool,
    impact: bool,
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
                delay,
                t: 0.0,
                wobble,
                accent,
                duration: 0.32,
                arc: true,
                impact: true,
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
    let dt = time.delta_secs();
    for (e, mut bolt, mut transform) in &mut bolts {
        if bolt.delay > 0.0 {
            bolt.delay -= dt;
            continue;
        }
        bolt.t += dt / bolt.duration;
        let t = bolt.t.min(1.0);
        let side = (bolt.to - bolt.from).cross(Vec3::Y).normalize_or_zero();
        let arc = if bolt.arc {
            Vec3::Y * (t * PI).sin() * 0.25 + side * (t * TAU * 1.5).sin() * bolt.wobble
        } else {
            Vec3::ZERO
        };
        transform.translation = bolt.from.lerp(bolt.to, t) + arc;
        transform.scale = Vec3::splat(0.09 + 0.02 * (time.elapsed_secs() * 40.0).sin());
        if bolt.t >= 1.0 {
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
        Move::Gesture => return None,
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

fn effect_accent(cue: tor_protocol::EffectCue) -> Accent {
    use tor_protocol::EffectCue;
    match cue {
        EffectCue::Neutral => Accent::White,
        EffectCue::Physical => Accent::Amber,
        EffectCue::Fire => Accent::Ember,
        EffectCue::Cold => Accent::Teal,
        EffectCue::Lightning => Accent::Violet,
        EffectCue::Poison => Accent::Danger,
        EffectCue::Healing => Accent::Gold,
        EffectCue::Status => Accent::Abyss,
    }
}

/// A snapshot-owned marker, deliberately without a client countdown component.
/// The next authoritative observation updates or removes its root entity.
pub fn public_effect(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    appearance: tor_protocol::EffectAppearance,
    cue: tor_protocol::EffectCue,
    points: &[Vec<Vec3>],
) -> Entity {
    let accent = effect_accent(cue);
    let model = if appearance == tor_protocol::EffectAppearance::Halo {
        ring(library, meshes, accent)
    } else {
        sphere(library, meshes, accent)
    };
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

/// Present a server-resolved instant. Finite playback is cosmetic, not effect lifetime.
/// Present only server-disclosed cell paths. Styling and playback speed are cosmetic;
/// neither interpolation across segment boundaries nor invented impact flashes are allowed.
#[expect(
    clippy::too_many_arguments,
    reason = "authoritative descriptor and geometry are independent inputs"
)]
pub fn stage_authoritative(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    timing: tor_protocol::EffectTiming,
    appearance: tor_protocol::EffectAppearance,
    delivery: tor_protocol::EffectDelivery,
    cue: tor_protocol::EffectCue,
    segments: &[Vec<Vec3>],
) {
    use tor_protocol::{EffectAppearance, EffectDelivery, EffectTiming};
    let EffectTiming::Instant = timing;
    // Private status belongs in the HUD, never an implied world-visible aura.
    if appearance == EffectAppearance::StatusIndicator {
        return;
    }
    let accent = effect_accent(cue);
    let path = matches!(delivery, EffectDelivery::Ray | EffectDelivery::Projectile)
        && matches!(
            appearance,
            EffectAppearance::Bolt | EffectAppearance::Missile
        );
    for segment in segments {
        if !path || segment.len() == 1 {
            for point in segment {
                let model = if appearance == EffectAppearance::Halo {
                    ring(library, meshes, accent)
                } else {
                    sphere(library, meshes, accent)
                };
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
            continue;
        }
        for (index, pair) in segment.windows(2).enumerate() {
            let model = sphere(library, meshes, accent);
            commands
                .spawn((
                    Bolt {
                        from: pair[0],
                        to: pair[1],
                        delay: index as f32 * 0.08,
                        t: 0.0,
                        wobble: 0.0,
                        accent,
                        duration: 0.08,
                        arc: false,
                        impact: false,
                    },
                    Transform::from_translation(pair[0]).with_scale(Vec3::ZERO),
                    Visibility::default(),
                ))
                .with_children(|p| model.attach(p));
        }
    }
}

#[cfg(test)]
mod authoritative_tests {
    use super::*;
    use crate::{props::PropMaterials, Game};
    use tor_client_3d_model::Happening;
    use tor_protocol::{EffectAppearance, EffectCue, EffectDelivery, EffectTiming, Position};

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
                    appearance,
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
                appearance: EffectAppearance::Bolt,
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
}
