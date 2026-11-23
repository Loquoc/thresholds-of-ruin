//! Death timing and the performances that lead into the shared settled poses.
use super::*;

/// A client-only visual choice, made once when a living actor dies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeathMotion {
    #[default]
    Collapse,
    Stagger,
    Dramatic,
}

impl DeathMotion {
    pub(in crate::creatures) fn choose(percentile: u8) -> Self {
        match percentile {
            0..=4 => Self::Dramatic,
            5..=51 => Self::Collapse,
            _ => Self::Stagger,
        }
    }

    pub(in crate::creatures) fn duration(self) -> f32 {
        match self {
            Self::Collapse => 1.1,
            Self::Stagger => 1.52,
            Self::Dramatic => 3.18,
        }
    }

    /// Negative phases are the old, fixed-length corridor-facing pre-roll.
    pub(in crate::creatures) fn advance(self, phase: f32, dt: f32) -> f32 {
        let facing = (-phase).max(0.0) * Self::Collapse.duration();
        if dt < facing {
            phase + dt / Self::Collapse.duration()
        } else {
            (phase.max(0.0) + (dt - facing) / self.duration()).min(1.0)
        }
    }

    pub(in crate::creatures) fn fall(self, phase: f32) -> f32 {
        let start = match self {
            Self::Collapse => 0.0,
            Self::Stagger => 0.25,
            Self::Dramatic => 0.54,
        };
        ((phase - start) / (1.0 - start)).clamp(0.0, 1.0)
    }

    pub(in crate::creatures) fn beats(self, phase: f32) -> (f32, f32) {
        match self {
            Self::Collapse => (0.0, 0.0),
            Self::Stagger => (envelope(phase, 0.0, 0.12, 0.20, 0.40), 0.0),
            Self::Dramatic => (
                envelope(phase, 0.0, 0.12, 0.18, 0.28),
                envelope(phase, 0.23, 0.37, 0.50, 0.67),
            ),
        }
    }
}

pub(super) fn perform(p: &mut Pose, rig: &Rig, anim: &Anim, phase: f32) {
    let side = if anim.seed > 5.0 { 1.0 } else { -1.0 };
    if anim.death_motion == DeathMotion::Collapse && rig.plan == Plan::Biped {
        heavy_biped_collapse(p, rig.style, phase, side);
    }
    let (brace, flourish) = anim.death_motion.beats(phase);
    if brace == 0.0 && flourish == 0.0 {
        return;
    }
    let wobble = (phase * 42.0).sin() * flourish;
    let tilt = |angle| Quat::from_rotation_z(side * angle);
    let splay = |side: f32, angle| Quat::from_rotation_z(side * angle);
    match rig.plan {
        Plan::Biped => {
            // Buckle and grab at the chest. In the theatrical follow-up,
            // one arm flies up as the other trails before the grounded fall.
            p.r(
                J::Torso,
                bow(0.32 * brace - 0.24 * flourish) * tilt(0.16 * flourish + 0.09 * wobble),
            );
            p.r(
                J::Head,
                bow(0.14 * brace - 0.28 * flourish) * tilt(0.12 * wobble),
            );
            for (arm, fore, handed) in [(J::ArmL, J::ForeL, 1.0), (J::ArmR, J::ForeR, -1.0)] {
                let (throw, spread, bend) = if handed == side {
                    (0.48, 1.95, 0.52)
                } else {
                    (0.78, 0.62, 0.70)
                };
                p.r(
                    arm,
                    fwd(0.64 * brace + throw * flourish)
                        * splay(handed, -0.28 * brace + spread * flourish),
                );
                p.r(fore, fwd(0.95 * brace + bend * flourish));
            }
        }
        Plan::Bird if rig.style.flies() => {
            // One failed wing stroke, or several emphatic corrections.
            let beat = (phase * 68.0).sin() * flourish;
            p.r(J::WingL, splay(1.0, 0.66 * brace + 0.58 * beat));
            p.r(J::WingR, splay(-1.0, 0.28 * brace + 0.58 * beat));
            p.r(J::Lift, tilt(0.27 * brace + 0.30 * wobble));
            p.o(J::Lift, Vec3::Y * (0.05 * brace + 0.12 * flourish));
            p.r(J::Neck, bow(0.28 * brace - 0.26 * flourish));
        }
        Plan::Bird => {
            p.r(
                J::Neck,
                bow(0.42 * brace - 0.34 * flourish) * tilt(0.24 * wobble),
            );
            p.r(
                J::Head,
                bow(-0.22 * brace + 0.19 * flourish) * tilt(-0.16 * wobble),
            );
            p.r(J::Body, tilt(0.045 * brace + 0.07 * wobble));
        }
        Plan::Quadruped | Plan::Centaur => {
            p.r(
                J::Neck,
                bow(0.36 * brace - 0.37 * flourish) * tilt(0.13 * wobble),
            );
            p.r(
                J::Head,
                bow(-0.17 * brace + 0.16 * flourish) * tilt(-0.14 * wobble),
            );
            p.r(J::Jaw, bow(0.47 * flourish));
            p.r(
                J::Tail,
                Quat::from_rotation_y(side * (0.28 * brace - 0.42 * wobble)),
            );
            if rig.plan == Plan::Centaur {
                p.r(J::Torso, bow(0.29 * brace - 0.20 * flourish));
                p.r(J::ArmL, fwd(0.55 * brace) * splay(1.0, 0.98 * flourish));
                p.r(J::ArmR, fwd(0.40 * brace) * splay(-1.0, 0.83 * flourish));
            } else if matches!(rig.style, Style::Dragon | Style::Griffin) {
                p.r(J::WingL, splay(-1.0, 0.77 * flourish));
                p.r(J::WingR, splay(1.0, 0.77 * flourish));
            }
        }
        Plan::Rooted => {
            p.r(J::Neck, tilt(0.39 * brace - 0.46 * flourish));
            p.r(
                J::Torso,
                bow(-0.18 * brace + 0.35 * flourish) * tilt(0.16 * wobble),
            );
            p.r(J::Jaw, bow(0.18 * brace + 0.68 * flourish));
        }
        Plan::Serpent => {
            // The head rise travels down the segmented body in place_segments.
            p.o(J::Lift, Vec3::X * side * (0.09 * brace + 0.11 * wobble));
        }
        Plan::Floater => {
            p.o(
                J::Lift,
                Vec3::new(
                    side * (0.16 * brace + 0.24 * wobble),
                    0.10 * brace + 0.33 * flourish,
                    0.0,
                ),
            );
            p.r(
                J::Lift,
                tilt(0.15 * brace + 0.45 * wobble) * Quat::from_rotation_y(side * 0.64 * flourish),
            );
        }
        Plan::Marker => {
            p.r(J::Lift, tilt(0.23 * brace + 0.29 * wobble));
            p.o(J::Lift, Vec3::Y * (0.04 * brace + 0.20 * flourish));
        }
    }
}

/// Let the heavier bipeds lose their knees before the shoulders come down.
/// The downhill leg provides the last bit of support; the other leg folds
/// more sharply. Rejoin the shared corpse pose before the body settles.
fn heavy_biped_collapse(p: &mut Pose, style: Style, phase: f32, side: f32) {
    let character = match style {
        Style::Yeti => 1.0,
        Style::Troll => 0.82,
        _ => return,
    };
    let fold = character * envelope(phase, 0.0, 0.35, 0.51, 0.89);
    if fold == 0.0 {
        return;
    }
    let (support, loose) = if side > 0.0 {
        (BIPED_LEGS[1], BIPED_LEGS[0])
    } else {
        (BIPED_LEGS[0], BIPED_LEGS[1])
    };
    for ((thigh, shin, foot), reach, bend) in [(support, 0.18, 0.36), (loose, 0.43, 0.92)] {
        p.r(thigh, fwd(reach * fold));
        p.r(shin, fwd(-bend * fold));
        p.r(foot, fwd((bend - reach) * 0.55 * fold));
    }
    p.o(J::Lift, Vec3::NEG_Y * (0.045 * fold));
    p.r(J::Torso, bow(0.20 * fold));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn death_selection_weights_and_lengths_are_explicit() {
        use DeathMotion::*;
        let mut counts = [0; 3];
        for roll in 0..100 {
            counts[match DeathMotion::choose(roll) {
                Collapse => 0,
                Stagger => 1,
                Dramatic => 2,
            }] += 1;
        }
        assert_eq!(counts, [47, 48, 5]);
        assert!(Dramatic.duration() > Stagger.duration() * 2.0);
        assert!(Stagger.duration() > Collapse.duration());
        for motion in [Collapse, Stagger, Dramatic] {
            let before_facing = motion.advance(-0.35, 0.3);
            assert!(before_facing < 0.0);
            assert_eq!(motion.fall(before_facing), 0.0);
            let at = motion.advance(before_facing, 0.085 + motion.duration() * 0.5);
            assert!((at - 0.5).abs() < 1e-5);
            assert!(motion.advance(at, motion.duration() * 0.49) < 1.0);
            assert_eq!(motion.advance(at, motion.duration() * 0.51), 1.0);
            assert_eq!(motion.fall(1.0), 1.0);
            assert_eq!(motion.beats(1.0), (0.0, 0.0));
        }
        assert_eq!(Collapse.beats(0.42), (0.0, 0.0));
        assert_eq!(Stagger.beats(0.42), (0.0, 0.0));
        assert!(Dramatic.beats(0.42).1 > 0.9);
    }

    #[test]
    fn heavy_bipeds_fold_the_loose_knee_without_losing_the_shared_corpse_pose() {
        use crate::creatures::rogue_tests::{points_under, spawn_one};

        for model in [
            CreatureModel::Yeti,
            CreatureModel::Troll,
            CreatureModel::Orc,
        ] {
            for build in 0..3 {
                let look = Look::with_choice(model, 42, Some(Variant { build, ..default() }));
                let (world, rig, _, meshes) = spawn_one(&look);
                let sole_points = BIPED_LEGS.map(|(_, _, foot)| {
                    let points = points_under(&world, &meshes, rig.joint(foot).unwrap());
                    assert!(!points.is_empty());
                    points
                });
                let lowest = |pose: &Pose| {
                    BIPED_LEGS
                        .into_iter()
                        .enumerate()
                        .map(|(leg, (thigh, knee, foot))| {
                            let frame = [J::Lift, J::Hips, thigh, knee, foot].into_iter().fold(
                                Affine3A::IDENTITY,
                                |frame, joint| {
                                    let i = joint as usize;
                                    let at = pose.off[i]
                                        + if joint == J::Lift {
                                            Vec3::ZERO
                                        } else {
                                            rig.rest[i]
                                        };
                                    frame * Affine3A::from_rotation_translation(pose.rot[i], at)
                                },
                            );
                            sole_points[leg]
                                .iter()
                                .map(|&point| frame.transform_point3(point).y)
                                .fold(f32::INFINITY, f32::min)
                        })
                        .fold(f32::INFINITY, f32::min)
                };
                for seed in [2.0, 7.0] {
                    for phase in [0.0, 0.20, 0.35, 0.48, 0.62, 0.76, 0.9, 1.0] {
                        let mut anim = Anim::new(42, Vec3::ZERO);
                        anim.seed = seed;
                        anim.preview_death(DeathMotion::Collapse);
                        anim.dead = Some(phase);
                        let mut ordinary = Pose::new();
                        biped_pose(&mut ordinary, &rig, 0.0, 0.0, 0.0, None, anim.carriage);
                        death_pose(&mut ordinary, &rig, ease(phase), seed);
                        let shaped = pose(&rig, &mut anim, 0.0, 0.0, false, false);
                        if model == CreatureModel::Orc || phase == 0.0 || phase >= 0.9 {
                            assert_eq!(ordinary.off, shaped.off);
                            assert_eq!(ordinary.rot, shaped.rot);
                            continue;
                        }
                        let (base, actual) = (lowest(&ordinary), lowest(&shaped));
                        assert!(
                            actual >= base.min(0.0) - 0.04,
                            "{model:?} build {build} seed {seed} at {phase}: sole sinks from {base} to {actual}"
                        );
                        if phase == 0.48 {
                            let [left, right] = [J::ShinL, J::ShinR].map(|joint| {
                                shaped.rot[joint as usize]
                                    .angle_between(ordinary.rot[joint as usize])
                            });
                            let (support, loose) = if seed > 5.0 {
                                (right, left)
                            } else {
                                (left, right)
                            };
                            assert!(
                                loose > support + 0.35,
                                "{model:?} did not fold the loose knee first"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn duplicate_live_death_cue_does_not_reroll_or_restart() {
        let mut anim = Anim::new(21, Vec3::ZERO);
        anim.act(Move::Strike);
        assert!(anim.die(true));
        assert_eq!(anim.dead, Some(-0.35));
        assert!(anim.action.is_none());
        let chosen = anim.death_motion;
        anim.dead = Some(0.55);
        assert!(!anim.die(true));
        assert_eq!(anim.death_motion, chosen);
        assert_eq!(anim.dead, Some(0.55));
        anim.preview_death(DeathMotion::Dramatic);
        assert!(
            anim.death_linger()
                >= 0.35 * DeathMotion::Collapse.duration() + DeathMotion::Dramatic.duration()
        );
        let corpse = Anim::corpse(21);
        assert_eq!(corpse.death_fall(), 1.0);
        assert_eq!(corpse.death_linger(), 2.0);
    }
}
