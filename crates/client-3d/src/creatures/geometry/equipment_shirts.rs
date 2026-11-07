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

/// A single thin cloth surface follows the deltoid into a relaxed short
/// sleeve. Unlike the costume cap, it has no separate shoulder ball or thick
/// torus cuff. The anatomy stays untouched underneath.
fn short_sleeve_form(body: HumanoidBody, side: f32, arm_length: f32) -> crate::sdf::Sculpt {
    use crate::sdf::Shape;
    let mut cloth = upper_arm_form(side, body.build, arm_length, body.profile.muscle, 0.94);
    cloth.inflate(0.006 * body.build);
    cloth.carve(
        Shape::Beyond(Vec3::NEG_Y, 0.155 * arm_length),
        0.001 * body.build,
    );
    cloth
}

pub(super) fn emit_short_sleeve(
    p: &mut Parts,
    kind: ShirtAppearance,
    body: HumanoidBody,
    side: f32,
    arm_length: f32,
) {
    let build = body.build;
    let cloth = short_sleeve_form(body, side, arm_length);
    let hem = -0.155 * arm_length;
    let color = kind.color(body.woman);
    cloth.mesh_projected(
        &mut p.matte,
        Vec3::new(-0.15 * build, hem - 0.012 * build, -0.115 * build),
        Vec3::new(0.15 * build, 0.09 * build.max(1.0), 0.115 * build),
        closed_surface_step(0.009 * build),
        &|at, _| {
            // Continuous print scale across the torso and shoulder, rather
            // than a plain-colored shoulder pad attached to a floral vest.
            let pigment = if kind == ShirtAppearance::HawaiianShirt {
                Pattern::Floral.paint(color, at + body.shoulder(side))
            } else {
                color
            };
            let folded_hem = if at.y < hem + 0.007 * arm_length {
                0.90
            } else {
                1.0
            };
            pigment * folded_hem * crate::sdf::Sculpt::weave(at)
        },
    );
}

pub(super) fn emit(
    p: &mut Parts,
    kind: ShirtAppearance,
    body: HumanoidBody,
    surface: &crate::sdf::Sculpt,
    armor: Option<&crate::sdf::Sculpt>,
) {
    let mut depth = 0.0;
    let mut patch = |points: [Vec2; 4], color: Vec3, fold: f32| {
        depth += 0.0003;
        // Small fitted cells follow the curved chest rather than cutting
        // through its center with one flat quad. Occlusion is cell-local.
        let at = |u: f32, v: f32| {
            points[0]
                .lerp(points[1], u)
                .lerp(points[3].lerp(points[2], u), v)
        };
        for row in 0..6 {
            for column in 0..6 {
                let u = column as f32 / 6.0;
                let v = row as f32 / 6.0;
                let fitted = [
                    (u, v),
                    (u + 1.0 / 6.0, v),
                    (u + 1.0 / 6.0, v + 1.0 / 6.0),
                    (u, v + 1.0 / 6.0),
                ]
                .map(|(u, v)| {
                    let turn = fold * (0.2 + 0.8 * (PI * u).sin()) * v;
                    visible_at(surface, armor, at(u, v)).map(|p| p + Vec3::Z * (depth + turn))
                });
                if let [Some(a), Some(b), Some(c), Some(d)] = fitted {
                    p.matte
                        .quad_out([a, b, c, d], (a + b + c + d) * 0.25 - Vec3::Z * 0.02, color);
                }
            }
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
                0.0,
            );
            patch(quad(-0.021, -0.002, 0.021, 0.010), DARK, 0.0);
            patch(
                quad(-0.020, -0.019, 0.020, -0.006),
                Vec3::new(0.60, 0.31, 0.10),
                0.0,
            );
            for dx in [-0.015, -0.005, 0.005, 0.015] {
                patch(quad(dx, 0.005, dx + 0.005, 0.010), CREAM, 0.0);
            }
            patch(quad(-0.003, -0.019, 0.003, -0.010), GILT, 0.0);
        }
        ShirtAppearance::HawaiianShirt => {
            let color = kind.color(body.woman);
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
            let sh = body.shoulders * body.build;
            // Broad, turned camp-collar leaves, with a darker fold line and a
            // lighter underside. Keep their tips beside the opening rather
            // than laying a pair of arbitrary triangles across bare sternum.
            for side in [-1.0, 1.0] {
                let inner = side * neckline.edge(0.48);
                patch(
                    [
                        Vec2::new(side * 0.035 * sh, 0.525),
                        Vec2::new(side * 0.10 * sh, 0.505),
                        Vec2::new(side * 0.15 * sh, 0.425),
                        Vec2::new(inner, 0.465),
                    ],
                    color * 0.83,
                    0.004 * body.build,
                );
                patch(
                    [
                        Vec2::new(side * 0.040 * sh, 0.523),
                        Vec2::new(side * 0.10 * sh, 0.500),
                        Vec2::new(side * 0.142 * sh, 0.434),
                        Vec2::new(inner + side * 0.006, 0.465),
                    ],
                    color * 1.14,
                    0.012 * body.build,
                );
            }
            let bottom = if body.woman {
                body.bust.map_or(0.27, |bust| bust.fold) - 0.025
            } else {
                0.045
            };
            let top = neckline.bottom - 0.008;
            if top > bottom {
                // Actual narrow front opening, two sewn edges and contrasting
                // coconut buttons. This makes the loose shirt read as woven
                // button-up clothing instead of a printed bodice.
                patch(
                    [
                        Vec2::new(-0.012 * body.build, bottom),
                        Vec2::new(0.012 * body.build, bottom),
                        Vec2::new(0.012 * body.build, top),
                        Vec2::new(-0.012 * body.build, top),
                    ],
                    color * 0.84,
                    0.0,
                );
                patch(
                    [
                        Vec2::new(-0.009 * body.build, bottom),
                        Vec2::new(0.009 * body.build, bottom),
                        Vec2::new(0.009 * body.build, top),
                        Vec2::new(-0.009 * body.build, top),
                    ],
                    color * 1.06,
                    0.0,
                );
                let count = ((top - bottom) / 0.055).floor() as usize;
                for i in 0..=count {
                    let y = bottom + 0.018 + i as f32 * 0.055;
                    if y >= top - 0.008 {
                        continue;
                    }
                    patch(
                        [
                            Vec2::new(-0.0055, y - 0.0055),
                            Vec2::new(0.0055, y - 0.0055),
                            Vec2::new(0.0055, y + 0.0055),
                            Vec2::new(-0.0055, y + 0.0055),
                        ],
                        Vec3::new(0.48, 0.27, 0.11),
                        0.0,
                    );
                    patch(
                        [
                            Vec2::new(-0.0015, y - 0.0015),
                            Vec2::new(0.0015, y - 0.0015),
                            Vec2::new(0.0015, y + 0.0015),
                            Vec2::new(-0.0015, y + 0.0015),
                        ],
                        DARK,
                        0.0,
                    );
                }
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
                        0.0,
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
    fn ordinary_shirt_sleeve_is_thin_continuous_cloth_not_a_padded_cap() {
        for woman in [false, true] {
            let body = HumanoidBody::resolve(&Look::with_choice(
                CreatureModel::Human,
                7,
                Some(Variant { woman, ..default() }),
            ));
            for side in [-1.0, 1.0] {
                let mut p = Parts::default();
                emit_short_sleeve(&mut p, ShirtAppearance::TShirt, body, side, body.arm_length);
                let skin =
                    upper_arm_form(side, body.build, body.arm_length, body.profile.muscle, 0.94);
                for y in [-0.018, -0.065, -0.11, -0.145] {
                    for direction in [Vec3::X * side, Vec3::Z, Vec3::NEG_Z] {
                        let hit = p
                            .matte
                            .outer_surface(Vec3::Y * (y * body.arm_length), direction)
                            .expect("continuous sleeve over the upper arm");
                        let ease = skin.distance(hit);
                        assert!(
                            ease > 0.001 * body.build && ease < 0.013 * body.build,
                            "thin cloth clearance, not padded sphere: {woman} {side} {y} {ease}"
                        );
                    }
                }
                assert!(
                    p.matte
                        .outer_surface(Vec3::Y * (-0.18 * body.arm_length), Vec3::X * side)
                        .is_none(),
                    "short sleeve leaves lower upper-arm exposed"
                );
                assert!(p.metal.is_empty());
            }
        }
    }

    #[test]
    fn tropical_sleeves_carry_petals_and_leaves_instead_of_plain_shoulder_pads() {
        use bevy::render::mesh::VertexAttributeValues;
        let body = HumanoidBody::resolve(&Look::of(CreatureModel::Human, 7));
        let mut p = Parts::default();
        emit_short_sleeve(
            &mut p,
            ShirtAppearance::HawaiianShirt,
            body,
            1.0,
            body.arm_length,
        );
        let mut materials = Assets::<StandardMaterial>::default();
        let materials = crate::props::PropMaterials::new(&mut materials);
        let mut meshes = Assets::<Mesh>::default();
        let model = p.finish(&mut meshes, &materials);
        let mut leaves = 0;
        let mut petals = 0;
        for (handle, _) in model.0 {
            let Some(VertexAttributeValues::Float32x4(colors)) = meshes
                .get(&handle)
                .unwrap()
                .attribute(Mesh::ATTRIBUTE_COLOR)
            else {
                panic!("sleeve vertex colors");
            };
            for color in colors {
                if color[1] > color[0] * 1.2 {
                    leaves += 1;
                }
                if color[0] > 0.5 && color[0] > color[1] * 2.0 {
                    petals += 1;
                }
            }
        }
        assert!(
            leaves > 10 && petals > 10,
            "readable floral sleeve colors: {leaves} leaves, {petals} petals"
        );
    }

    #[test]
    fn camp_collar_has_fold_relief_and_remains_hidden_under_armor() {
        for woman in [false, true] {
            let body = HumanoidBody::resolve(&Look::with_choice(
                CreatureModel::Human,
                7,
                Some(Variant { woman, ..default() }),
            ));
            let shirt = torso_shape(body, None, false);
            let mut p = Parts::default();
            emit(&mut p, ShirtAppearance::HawaiianShirt, body, &shirt, None);
            let x = 0.10 * body.shoulders * body.build;
            let origin = Vec3::new(x, 0.465, 0.0);
            let leaf = p
                .matte
                .outer_surface(origin, Vec3::Z)
                .expect("folded collar leaf");
            let under = torso_surface_hit(&shirt, x, origin.y, true).unwrap();
            assert!(
                leaf.z - under.z > 0.005,
                "collar turn must have relief, not be painted on"
            );
            let mut armor = shirt.clone();
            armor.inflate(0.05);
            let mut covered = Parts::default();
            emit(
                &mut covered,
                ShirtAppearance::HawaiianShirt,
                body,
                &shirt,
                Some(&armor),
            );
            assert!(
                covered.matte.is_empty(),
                "collar/buttons cannot be projected on top of enclosing armor"
            );
        }
    }

    #[test]
    fn replacing_inner_shirt_does_not_replace_plate_shoulder_geometry() {
        use crate::creatures::rogue_tests::spawn_one;
        use bevy::render::mesh::VertexAttributeValues;
        let arm_points = |shirt: ShirtAppearance| {
            let mut equipment = Equipment::default();
            equipment.select(shirt.name()).unwrap();
            equipment.select("plate-mail").unwrap();
            let look = Look::of(CreatureModel::Human, 7).equipped(equipment);
            let (world, rig, _, meshes) = spawn_one(&look);
            let joint = rig.joints[J::ArmL as usize].unwrap();
            super::super::test_support::joint_meshes(&world, &rig, &meshes, joint)
                .into_iter()
                .flat_map(|surface| {
                    let Some(VertexAttributeValues::Float32x3(points)) =
                        surface.mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                    else {
                        panic!("arm positions");
                    };
                    points
                        .iter()
                        .map(|p| {
                            surface
                                .transform
                                .transform_point3(Vec3::from_array(*p))
                                .to_array()
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        let tee = arm_points(ShirtAppearance::TShirt);
        let floral = arm_points(ShirtAppearance::HawaiianShirt);
        assert!(!tee.is_empty());
        assert_eq!(
            tee, floral,
            "armor pauldrons own the arm silhouette over either shirt"
        );
    }

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
