//! Visible shirt construction details, clipped by actual outer armor coverage.
use super::super::outfit::shirts::ShirtAppearance;
use super::*;

/// A shirt print or collar must not reappear on the outside of armor.
fn visible_at(
    surface: &crate::sdf::Sculpt,
    armor: Option<&crate::sdf::Sculpt>,
    at: Vec2,
) -> Option<Vec3> {
    let point = torso_surface_hit(surface, at.x, at.y, true)?;
    if armor
        .and_then(|outer| torso_surface_hit(outer, at.x, at.y, true))
        .is_some_and(|outer| outer.z > point.z + 0.002)
    {
        return None;
    }
    Some(point + Vec3::Z * 0.004)
}

pub(super) fn emit(
    p: &mut Parts,
    kind: ShirtAppearance,
    body: HumanoidBody,
    surface: &crate::sdf::Sculpt,
    armor: Option<&crate::sdf::Sculpt>,
) {
    let mut depth = 0.0;
    let mut patch = |points: [Vec2; 4], color: Vec3| {
        depth += 0.0003;
        let fitted = points.map(|at| visible_at(surface, armor, at).map(|p| p + Vec3::Z * depth));
        if let [Some(a), Some(b), Some(c), Some(d)] = fitted {
            p.matte
                .quad_out([a, b, c, d], (a + b + c + d) * 0.25 - Vec3::Z * 0.02, color);
        }
    };
    match kind {
        ShirtAppearance::TShirt => {
            // Tiny mimic-chest emblem: wood, open dark mouth, teeth and latch.
            let x = 0.105 * body.build;
            let y = 0.405;
            let quad = |left: f32, low: f32, right: f32, high: f32| {
                [
                    Vec2::new(x + left, y + low),
                    Vec2::new(x + right, y + low),
                    Vec2::new(x + right, y + high),
                    Vec2::new(x + left, y + high),
                ]
            };
            patch(
                quad(-0.025, -0.024, 0.025, 0.022),
                Vec3::new(0.27, 0.13, 0.06),
            );
            patch(quad(-0.021, -0.002, 0.021, 0.010), DARK);
            patch(
                quad(-0.020, -0.019, 0.020, -0.006),
                Vec3::new(0.60, 0.31, 0.10),
            );
            for dx in [-0.015, -0.005, 0.005, 0.015] {
                patch(quad(dx, 0.005, dx + 0.005, 0.010), CREAM);
            }
            patch(quad(-0.003, -0.019, 0.003, -0.010), GILT);
        }
        ShirtAppearance::HawaiianShirt => {
            let color = kind.color(body.woman);
            for side in [-1.0, 1.0] {
                let s = body.shoulders * body.build;
                patch(
                    [
                        Vec2::new(side * 0.028 * s, 0.52),
                        Vec2::new(side * 0.085 * s, 0.50),
                        Vec2::new(side * 0.115 * s, 0.435),
                        Vec2::new(side * 0.060 * s, 0.465),
                    ],
                    color * 1.12,
                );
            }
            // Buttons run only below the shirt's open collar.
            let neckline = NecklineCut::resolve(
                if body.woman {
                    Neckline::Plunge {
                        depth: 0.65,
                        width: 0.16,
                        wrap: false,
                    }
                } else {
                    Neckline::Plunge {
                        depth: 0.15,
                        width: 0.12,
                        wrap: false,
                    }
                },
                body,
            )
            .unwrap();
            for i in 0..7 {
                let y = 0.075 + i as f32 * 0.045;
                if body.woman || y >= neckline.bottom - 0.01 {
                    continue;
                }
                patch(
                    [
                        Vec2::new(-0.003, y - 0.004),
                        Vec2::new(0.003, y - 0.004),
                        Vec2::new(0.003, y + 0.004),
                        Vec2::new(-0.003, y + 0.004),
                    ],
                    Vec3::new(0.58, 0.37, 0.19),
                );
            }
            if body.woman {
                // Knot/tails follow the actual cropped shirt hem, not the
                // belt below the bare midriff.
                let hem = body.bust.map_or(0.27, |bust| bust.fold) - 0.03;
                for side in [-1.0, 1.0] {
                    patch(
                        [
                            Vec2::new(side * 0.005, hem),
                            Vec2::new(side * 0.055, hem - 0.055),
                            Vec2::new(side * 0.060, hem - 0.010),
                            Vec2::new(side * 0.012, hem + 0.018),
                        ],
                        color * 1.08,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_sleeved_shirts_cover_shoulders_beyond_their_neckline() {
        for woman in [false, true] {
            for shirt in ShirtAppearance::ALL {
                let mut look = Look::with_choice(
                    CreatureModel::Human,
                    7,
                    Some(Variant { woman, ..default() }),
                );
                look.equipment.shirt = Some(shirt);
                let body = HumanoidBody::resolve(&look);
                let figure = super::super::biped::Figure::resolve(body, &look);
                assert!(figure.bare_arms, "short sleeves retain exposed lower arms");
                assert!(!figure.bare_shoulders());
                let cut = NecklineCut::resolve(figure.outfit.bodice.cut, body).unwrap();
                let at = Vec3::new(cut.edge(0.525) + 0.025, 0.525, 0.09);
                assert!(!cut.open(at));
                assert!(bodice_covers(
                    body,
                    figure.outfit.bodice,
                    figure.bare_shoulders(),
                    at
                ));
            }
        }
    }

    #[test]
    fn actual_outer_armor_hides_shirt_prints_but_an_open_cloak_does_not() {
        let body = HumanoidBody::resolve(&Look::of(CreatureModel::Human, 7));
        let shirt = torso_shape(body, None, false);
        let mut armor = shirt.clone();
        armor.inflate(0.03);
        let at = Vec2::new(0.04, 0.35);
        assert!(visible_at(&shirt, None, at).is_some());
        assert!(visible_at(&shirt, Some(&armor), at).is_none());
    }
}
