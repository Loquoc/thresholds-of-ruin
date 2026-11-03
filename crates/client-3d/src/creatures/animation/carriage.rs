//! Cosmetic walking carriage, independent of clothes, body meshes and gameplay.
use super::*;

/// Explicit presentation choices. Native preserves each existing recipe.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WalkingCarriage {
    #[default]
    Native,
    Grounded,
    Poised,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Carriage {
    pub narrowing: f32,
    pub hips: f32,
    pub torso: f32,
    pub head: f32,
}

pub(super) fn resolve(rig: &Rig, selection: WalkingCarriage) -> Carriage {
    resolve_body(
        rig.style,
        rig.humanoid.is_some_and(|body| body.woman),
        rig.humanoid.is_some_and(|body| body.bust.is_some()),
        selection,
    )
}

fn resolve_body(style: Style, woman: bool, organic: bool, selection: WalkingCarriage) -> Carriage {
    // Specialized locomotion keeps its native recipe even if internal callers
    // bypass the stricter review-scene validation.
    if !matches!(style, Style::Zombie | Style::Guardian | Style::Yeti) {
        match selection {
            WalkingCarriage::Grounded => {
                return Carriage {
                    narrowing: 0.08,
                    hips: 0.03,
                    torso: -0.025,
                    head: -0.005,
                }
            }
            WalkingCarriage::Poised => {
                return Carriage {
                    narrowing: 0.3,
                    hips: 0.075,
                    torso: -0.06,
                    head: -0.015,
                }
            }
            WalkingCarriage::Native => {}
        }
    }
    let rolls = woman && !matches!(style, Style::Zombie | Style::Guardian);
    Carriage {
        narrowing: if organic && style != Style::Zombie {
            0.3
        } else {
            0.0
        },
        hips: if rolls { 0.075 } else { 0.0 },
        torso: if rolls { -0.06 } else { 0.0 },
        head: if rolls { -0.02 } else { 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_retains_existing_recipe_coefficients() {
        for style in [
            Style::Hero,
            Style::Caster,
            Style::Orc,
            Style::Zombie,
            Style::Guardian,
            Style::Yeti,
        ] {
            for woman in [false, true] {
                for organic in [false, true] {
                    let p = resolve_body(style, woman, organic, WalkingCarriage::Native);
                    assert_eq!(
                        p.narrowing,
                        if organic && style != Style::Zombie {
                            0.3
                        } else {
                            0.0
                        }
                    );
                    let roll = woman && !matches!(style, Style::Zombie | Style::Guardian);
                    assert_eq!(
                        (p.hips, p.torso, p.head),
                        if roll {
                            (0.075, -0.06, -0.02)
                        } else {
                            (0.0, 0.0, 0.0)
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn explicit_carriage_is_not_selected_by_sex_or_bust_geometry() {
        for (selection, expected) in [
            (
                WalkingCarriage::Grounded,
                Carriage {
                    narrowing: 0.08,
                    hips: 0.03,
                    torso: -0.025,
                    head: -0.005,
                },
            ),
            (
                WalkingCarriage::Poised,
                Carriage {
                    narrowing: 0.3,
                    hips: 0.075,
                    torso: -0.06,
                    head: -0.015,
                },
            ),
        ] {
            for style in [Style::Hero, Style::Caster, Style::Orc] {
                for woman in [false, true] {
                    for organic in [false, true] {
                        assert_eq!(resolve_body(style, woman, organic, selection), expected);
                    }
                }
            }
        }
    }
    #[test]
    fn explicit_carriage_changes_actual_pose_not_body_or_clock() {
        use crate::creatures::rogue_tests::spawn_one;
        let look = Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        let (_, rig, _, _) = spawn_one(&look);
        let mut grounded = Pose::new();
        let mut poised = Pose::new();
        biped_pose(
            &mut grounded,
            &rig,
            0.5,
            1.0,
            0.0,
            None,
            WalkingCarriage::Grounded,
        );
        biped_pose(
            &mut poised,
            &rig,
            0.5,
            1.0,
            0.0,
            None,
            WalkingCarriage::Poised,
        );
        assert_ne!(grounded.rot[J::Hips as usize], poised.rot[J::Hips as usize]);
        assert_eq!(
            grounded.rot[J::ArmL as usize],
            poised.rot[J::ArmL as usize],
            "carriage does not invent a new action cadence"
        );
        let mut anim = Anim::new(7, Vec3::ZERO);
        anim.speed = 0.5;
        anim.walk = 1.0;
        anim.carriage = WalkingCarriage::Grounded;
        let wide = biped_walk_contacts(&rig, &anim);
        anim.carriage = WalkingCarriage::Poised;
        let narrow = biped_walk_contacts(&rig, &anim);
        assert!(wide[0].x.abs() > narrow[0].x.abs());
        assert_eq!(anim.time, 0.0);
        assert_eq!(wide[0].y, narrow[0].y);
        assert_eq!(wide[0].z, narrow[0].z);
    }
}
