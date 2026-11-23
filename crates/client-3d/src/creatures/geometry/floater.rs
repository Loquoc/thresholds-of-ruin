use super::*;

pub(in crate::creatures) const FLOATER_HOVER: f32 = 1.1;

/// Shared aura layout for the generated neutral pose and the moving core.
pub(in crate::creatures) fn floater_mote(
    ice: bool,
    live_core: Vec3,
    phase: f32,
    flare: f32,
    f: f32,
    count: f32,
) -> (Vec3, f32) {
    let angle = phase * if ice { 0.55 } else { 0.9 } + f * TAU / count;
    if ice {
        let radius = 0.32 + 0.14 * flare;
        (
            live_core
                + Vec3::new(
                    angle.cos() * radius,
                    angle.sin() * 0.13,
                    angle.sin() * radius,
                ),
            0.8,
        )
    } else {
        // Embers rise in a loose column and fade at both ends of
        // their life, so recycling has no visible positional pop.
        let life = (phase * 0.28 + f / count).rem_euclid(1.0);
        let radius = 0.13 + life * 0.16 + flare * 0.1;
        (
            live_core
                + Vec3::new(
                    angle.cos() * radius,
                    -0.1 + life * 0.65,
                    angle.sin() * radius,
                ),
            (PI * life).sin().powi(2),
        )
    }
}

/// Things that float: a burning wisp, a drifting knot of ice.
pub(super) fn floater(kit: &mut Kit, lift: Entity, look: &Look) {
    let l = look.clone();
    let aura = l.aura.unwrap_or(Accent::Ember);
    let core = kit.joint(J::Core, lift, Vec3::Y * FLOATER_HOVER, "core", |p| {
        p.accent = Some(aura);
        if l.style == Style::Ice {
            // Dark blue recesses and pale cyan planes keep the cluster
            // readable in daylight as well as against the dark dungeon.
            p.satin.smooth = false;
            p.satin.sides_mul = 1;
            p.satin.res = (5, 8);
            p.glow.smooth = false;
            p.glow.sides_mul = 1;
            p.satin.blob(
                Vec3::ZERO,
                Vec3::new(0.17, 0.205, 0.15),
                4,
                0.13,
                Vec3::new(0.12, 0.27, 0.4),
            );
            for k in 0..9 {
                let a = k as f32 * TAU / 9.0;
                let dir = Vec3::new(a.cos(), (k as f32 * 1.3).sin() * 0.8, a.sin()).normalize();
                let color = Vec3::new(0.16, 0.43, 0.61).lerp(l.skin, 0.15 + 0.5 * rand(17, k));
                ice_shard(
                    p,
                    dir * 0.06,
                    dir * (0.3 + rand(k as u64, 3) * 0.15),
                    0.055 + 0.025 * rand(19, k),
                    color,
                );
            }
            eyes(
                p,
                Some(Accent::White),
                Vec3::new(0.0, 0.04, 0.15),
                0.06,
                0.02,
            );
            p.accent = Some(aura);
        } else {
            // Shaded outer petals frame the luminous heart and curling tongues.
            for k in 0..6 {
                let a = k as f32 * TAU / 6.0 + 0.3;
                let out = Vec3::new(a.cos(), 0.0, a.sin());
                tube(
                    &mut p.matte,
                    &[
                        out * 0.05 - Vec3::Y * 0.09,
                        out * 0.19 + Vec3::Y * 0.02,
                        out * 0.12 + Vec3::Y * 0.17,
                    ],
                    &[(0.05, 0.02), (0.06, 0.025), (0.004, 0.004)],
                    Vec3::new(0.85, 0.22, 0.035),
                );
            }
            for (k, h) in [(0, 0.52f32), (1, 0.36), (2, 0.4)] {
                let a = k as f32 * 2.2;
                let out = Vec3::new(a.cos(), 0.0, a.sin()) * 0.04;
                let curl = Vec3::new(-a.sin(), 0.0, a.cos()) * 0.06;
                tube(
                    &mut p.glow,
                    &[
                        out,
                        out * 1.4 + Vec3::Y * h * 0.4 + curl * 0.5,
                        out + Vec3::Y * h * 0.75 - curl * 0.3,
                        Vec3::Y * h + curl,
                    ],
                    &[(0.075, 0.05), (0.06, 0.04), (0.03, 0.02), (0.003, 0.003)],
                    Vec3::ONE,
                );
            }
        }
    });
    if l.style == Style::Wisp {
        // A brighter golden heart inside the flames.
        kit.attach(core, Transform::IDENTITY, "heart", |p| {
            p.accent = Some(Accent::Amber);
            p.glow
                .blob(Vec3::Y * 0.02, Vec3::splat(0.085), 1, 0.0, Vec3::ONE);
        });
    }
    if l.style == Style::Ice {
        for (side, arm) in [(1.0f32, J::ArmL), (-1.0, J::ArmR)] {
            kit.joint(
                arm,
                core,
                Vec3::new(side * 0.42, -0.1, 0.0),
                &format!("shard{side}"),
                |p| {
                    p.accent = Some(aura);
                    p.satin.smooth = false;
                    p.satin.sides_mul = 1;
                    p.glow.smooth = false;
                    p.glow.sides_mul = 1;
                    for k in 0..4 {
                        let dir = Vec3::new(side * 0.3, -1.0, 0.2 * (k as f32 - 1.5)).normalize();
                        ice_shard(
                            p,
                            Vec3::new(0.0, 0.0, (k as f32 - 1.5) * 0.022),
                            dir * (0.32 + k as f32 * 0.04),
                            0.045 + k as f32 * 0.004,
                            Vec3::new(0.22, 0.5, 0.69).lerp(l.skin, k as f32 * 0.18),
                        );
                    }
                },
            );
        }
    }
    let mote = kit.part("mote", |p| {
        p.accent = Some(aura);
        p.glow
            .blob(Vec3::ZERO, Vec3::splat(0.035), 0, 0.0, Vec3::ONE);
    });
    let count = 6;
    for index in 0..count {
        let (at, size) = floater_mote(
            l.style == Style::Ice,
            Vec3::Y * FLOATER_HOVER,
            0.0,
            0.0,
            index as f32,
            count as f32,
        );
        let e = kit
            .commands
            .spawn((
                Transform::from_translation(at).with_scale(Vec3::splat(size)),
                Visibility::default(),
            ))
            .id();
        kit.commands.entity(e).with_children(|p| mote.attach(p));
        kit.commands.entity(lift).add_child(e);
        kit.rig.segments.push(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creatures::rogue_tests::spawn_one;

    #[test]
    fn floater_motes_form_an_aura_before_the_first_update() {
        for model in [CreatureModel::Ice, CreatureModel::Wisp] {
            let (world, rig, _, _) = spawn_one(&Look::of(model, 42));
            let local = |joint| world.get::<Transform>(joint).unwrap();
            let center = local(rig.joint(J::Core).unwrap()).translation;
            let motes: Vec<_> = rig.segments.iter().copied().map(local).collect();
            assert_eq!(motes.len(), 6);
            for mote in &motes {
                let offset = mote.translation - center;
                let radius = offset.with_y(0.0).length();
                assert!(
                    mote.translation.y > 0.9 && (0.12..0.37).contains(&radius),
                    "{model:?}: mote at {:?} is outside the core aura",
                    mote.translation
                );
                assert!(mote.scale.is_finite() && mote.scale.x >= 0.0 && mote.scale.x <= 1.0);
            }
            for (i, a) in motes.iter().enumerate() {
                for b in &motes[i + 1..] {
                    assert!(
                        a.translation.distance(b.translation) > 0.10,
                        "{model:?}: overlapping motes at spawn"
                    );
                }
            }
            if model == CreatureModel::Wisp {
                let smallest = motes.iter().map(|m| m.scale.x).fold(1.0, f32::min);
                let largest = motes.iter().map(|m| m.scale.x).fold(0.0, f32::max);
                assert!(
                    largest - smallest > 0.7,
                    "embers must start at staggered ages"
                );
            }
        }
    }
}
