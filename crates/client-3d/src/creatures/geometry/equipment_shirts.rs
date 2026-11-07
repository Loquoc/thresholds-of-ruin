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

/// Clothing support is independent of anatomy. Additive unions preserve the
/// original bust extrema while spanning the concave folds beneath them.
/// The caller emits only covered cloth; the unchanged skin remains underneath.
pub(super) fn torso_envelope(
    kind: ShirtAppearance,
    body: HumanoidBody,
    skin: &crate::sdf::Sculpt,
    neckline: Option<NecklineCut>,
) -> crate::sdf::Sculpt {
    use crate::sdf::Shape;
    let mut cloth = skin.clone();
    if let Some(bust) = body.bust {
        bust_underbridge(&mut cloth, &bust, 0.90, 0.002 * body.build, 0.002);
        // A soft tension panel joins the two lower folds. It does not replace
        // the separate upper fullness or change the wearer's anatomy.
        let (left, lr) = bust.mass(-1.0, 0.0);
        let (right, rr) = bust.mass(1.0, 0.0);
        cloth.add(
            Shape::Capsule(left, right, lr.z * 0.55, rr.z * 0.55),
            0.025 * body.build,
        );
    } else if kind == ShirtAppearance::HawaiianShirt {
        // Woven vacation shirts hang straighter than a fitted knit tee.
        // Room is local to the waist and ribs, not the shoulder or neck.
        cloth.add(
            Shape::Ellipsoid(
                Vec3::new(0.0, 0.20, -0.005),
                Vec3::new(
                    0.179 * body.shoulders.max(body.waist) * body.build,
                    0.205,
                    0.127 * body.build,
                ),
            ),
            0.018 * body.build,
        );
    }
    cloth.inflate(0.0025 * body.build);
    // Extra tension/ease belongs below the opening. Above its lowest point
    // retain only a 2.5 mm cloth allowance on the exact skin, so conservative
    // outer-layer fitting cannot turn an exposed neckline into a padded shelf.
    cloth.carve(
        Shape::Beyond(Vec3::Y, neckline.map_or(0.53, |cut| cut.bottom)),
        0.012 * body.build,
    );
    let mut upper = skin.clone();
    upper.inflate(0.0025 * body.build);
    cloth.union(upper, 0.0);
    cloth
}

/// Clip the retained layer exactly at the crop. The neckline reserve is
/// conservative rather than using a 3D capsule that could cut away visible
/// scoop cloth. Its allowance is bounded by torso_envelope above the opening.
pub(super) fn retained_support(
    body: HumanoidBody,
    bodice: Bodice,
    envelope: &crate::sdf::Sculpt,
) -> crate::sdf::Sculpt {
    use crate::sdf::Shape;
    let mut support = envelope.clone();
    if let Cover::Crop(Hem::Even) = bodice.cover {
        let bottom = body.bust.map_or(0.27, |bust| bust.fold) - 0.03;
        support.carve(Shape::Beyond(Vec3::NEG_Y, -bottom), 0.0);
    }
    support
}

/// Parameterize the scoop by width instead of height: equal-height samples
/// crowd the high sides and leave a sharp-looking two-segment bottom.
pub(super) fn scoop_binding(cut: NecklineCut) -> Vec<(Vec2, f32)> {
    (0..=32)
        .map(|i| {
            let t = i as f32 / 16.0 - 1.0;
            let x = t * cut.wide;
            let u = ((x.abs() - cut.point) / (cut.wide - cut.point)).max(0.0);
            let y = cut.bottom + (cut.collar - cut.bottom) * u * u;
            (Vec2::new(x, y - 0.0015), 0.006)
        })
        .collect()
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
    fn test_envelope(
        kind: ShirtAppearance,
        body: HumanoidBody,
        skin: &crate::sdf::Sculpt,
    ) -> crate::sdf::Sculpt {
        let look = Look::of(CreatureModel::Human, 7);
        let bodice = kind.compose(Outfit::native(&look), body.woman).bodice;
        torso_envelope(kind, body, skin, NecklineCut::resolve(bodice.cut, body))
    }

    #[test]
    fn emitted_neckline_and_shoulders_are_retained_without_an_inflated_opening() {
        use bevy::render::mesh::VertexAttributeValues;
        for woman in [false, true] {
            let look = Look::with_choice(
                CreatureModel::Human,
                7,
                Some(Variant { woman, ..default() }),
            );
            let body = HumanoidBody::resolve(&look);
            let skin = torso_shape(body, None, true);
            for kind in ShirtAppearance::ALL {
                let bodice = kind.compose(Outfit::native(&look), woman).bodice;
                let cut = NecklineCut::resolve(bodice.cut, body).unwrap();
                let envelope = torso_envelope(kind, body, &skin, Some(cut));
                let support = retained_support(body, bodice, &envelope);
                let mut actual = Parts::default();
                envelope.mesh_region(
                    &mut actual.matte,
                    Vec3::new(-0.38, -0.06, -0.21) * body.build.max(1.0),
                    Vec3::new(0.38, 0.64, 0.28) * body.build.max(1.0),
                    0.0085,
                    &|_, _| Vec3::ONE,
                    &|at| bodice_covers(body, bodice, false, at) && !cut.open(at),
                    true,
                );
                for fraction in [0.25, 0.50, 0.75] {
                    let y = cut.bottom + (cut.collar - cut.bottom) * fraction;
                    for x in [0.0, cut.edge(y) * 0.5] {
                        assert!(actual.matte.outer_surface(Vec3::new(x, y, 0.0), Vec3::Z).is_none(),
                            "no invented cloth across exposed neckline: {kind:?}, woman={woman}, {x}/{y}");
                        let reserved = torso_surface_hit(&support, x, y, true).unwrap();
                        assert!(skin.distance(reserved) <= 0.0026 * body.build,
                            "neckline reserve exceeds 2.5 mm skin allowance (0.1 mm numerical tolerance): {kind:?}, woman={woman}");
                    }
                }
                let mut meshes = Assets::<Mesh>::default();
                let mut materials = Assets::<StandardMaterial>::default();
                let model = actual.finish(
                    &mut meshes,
                    &crate::props::PropMaterials::new(&mut materials),
                );
                let mut neckline_vertices = 0;
                let mut shoulder_vertices = 0;
                for (handle, _) in model.0 {
                    let Some(VertexAttributeValues::Float32x3(points)) = meshes
                        .get(&handle)
                        .unwrap()
                        .attribute(Mesh::ATTRIBUTE_POSITION)
                    else {
                        panic!("cloth positions");
                    };
                    for point in points {
                        let at = Vec3::from_array(*point);
                        if at.z > 0.0 && at.y > cut.bottom + 0.008 {
                            assert!(
                                support.distance(at) <= 0.0045,
                                "retained support lost emitted neckline/shoulder cloth at {at:?}"
                            );
                            if (at.x.abs() - cut.edge(at.y)).abs() < 0.02 {
                                neckline_vertices += 1;
                            }
                            if at.x.abs() > 0.13 * body.shoulders * body.build && at.y > 0.46 {
                                shoulder_vertices += 1;
                            }
                        }
                    }
                }
                assert!(neckline_vertices > 10 && shoulder_vertices > 10,
                    "positive coverage witnesses must exercise both edges: {neckline_vertices}/{shoulder_vertices}");
            }
        }
    }

    #[test]
    fn cropped_shirt_preserves_rendered_bare_skin_and_reserves_only_retained_cloth() {
        use super::super::{biped::Figure, test_support};
        use crate::creatures::rogue_tests::spawn_one;
        use bevy::render::mesh::VertexAttributeValues;
        let mut equipment = Equipment::default();
        equipment.select("hawaiian-shirt").unwrap();
        let look = Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                ..default()
            }),
        )
        .equipped(equipment);
        let body = HumanoidBody::resolve(&look);
        let figure = Figure::resolve(body, &look);
        let skin = torso_shape(body, None, figure.trim_neck);
        let mut reference = Parts::default();
        skin.mesh(
            &mut reference.matte,
            Vec3::new(-0.38, -0.06, -0.21) * body.build.max(1.0),
            Vec3::new(0.38, 0.64, 0.24) * body.build.max(1.0),
            0.0085,
            &|_, _| look.skin,
        );
        let (world, rig, _, meshes) = spawn_one(&look);
        let torso = rig.joints[J::Torso as usize].unwrap();
        let mut rendered = Parts::default();
        for surface in test_support::joint_meshes(&world, &rig, &meshes, torso) {
            let Some(VertexAttributeValues::Float32x3(points)) =
                surface.mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("torso positions");
            };
            let indices: Vec<_> = surface.mesh.indices().unwrap().iter().collect();
            for tri in indices.as_chunks::<3>().0 {
                rendered.matte.tri(
                    std::array::from_fn(|i| {
                        surface
                            .transform
                            .transform_point3(Vec3::from_array(points[tri[i]]))
                    }),
                    Vec3::ONE,
                );
            }
        }
        for x in [-0.025, 0.0, 0.025] {
            for y in [0.12, 0.16, 0.49] {
                let origin = Vec3::new(x, y, 0.0);
                let before = reference.matte.outer_surface(origin, Vec3::Z).unwrap();
                let after = rendered.matte.outer_surface(origin, Vec3::Z).unwrap();
                assert!(
                    before.distance(after) < 0.0002,
                    "shirt must not reshape exposed crop/neckline skin: {before:?} -> {after:?}"
                );
            }
        }
        let envelope = figure.shirt_shape.as_ref().unwrap();
        let retained = figure.shirt_support.as_ref().unwrap();
        assert!(
            retained.distance(Vec3::new(0.0, 0.12, 0.0)) > 0.03,
            "invisible cropped abdomen cannot inflate outer-layer fitting"
        );
        let bust = body.bust.unwrap();
        for side in [-1.0, 1.0] {
            let x = bust.apex(side).x;
            let at = torso_surface_hit(envelope, x, bust.fold + 0.015, true).unwrap();
            assert!(
                retained.distance(at).abs() < 0.002,
                "visible lower-bust cloth must remain in retained support"
            );
            assert!(
                figure.waist_support.distance(at) < 0.002,
                "layer fitting must see the actual rendered shirt envelope"
            );
        }
    }

    #[test]
    fn independent_cloth_bridges_lower_folds_without_flattening_the_body() {
        let body = HumanoidBody::resolve(&Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                ..default()
            }),
        ));
        let skin = torso_shape(body, None, false);
        let bust = body.bust.unwrap();
        let mut old = skin.clone();
        bust_underbridge(&mut old, &bust, 0.55, 0.0, 0.0);
        for kind in ShirtAppearance::ALL {
            let cloth = test_envelope(kind, body, &skin);
            let mut bridged = 0.0f32;
            for side in [-1.0, 1.0] {
                let apex = bust.apex(side);
                let actual_skin = torso_surface_hit(&skin, apex.x, apex.y, true).unwrap();
                let fitted = torso_surface_hit(&cloth, apex.x, apex.y, true).unwrap();
                assert!(fitted.z >= actual_skin.z, "cloth cannot flatten the wearer");
                assert!(
                    fitted.z - actual_skin.z < 0.035 * body.build,
                    "soft fitted fullness must not become a rigid chest plate"
                );
                for offset in [-0.025, -0.010, 0.005, 0.020, 0.035] {
                    let y = bust.fold + offset;
                    let before = torso_surface_hit(&old, apex.x, y, true).unwrap();
                    let after = torso_surface_hit(&cloth, apex.x, y, true).unwrap();
                    bridged = bridged.max(after.z - before.z);
                    assert!(
                        after.z - before.z < 0.065 * body.build,
                        "bounded cloth stand-off, not inflated anatomy"
                    );
                }
            }
            assert!(
                bridged > 0.006 * body.build,
                "must bridge the old under-fold, not just add a thin skin coating: {bridged}"
            );
            let waist_skin = torso_surface_hit(&skin, 0.0, 0.12, true).unwrap();
            let waist_cloth = torso_surface_hit(&cloth, 0.0, 0.12, true).unwrap();
            assert!(
                waist_cloth.z - waist_skin.z < 0.015 * body.build,
                "retain a fitted feminine waist"
            );
        }
    }

    #[test]
    fn woven_male_shirt_has_bounded_ease_and_armor_keeps_its_original_support() {
        use super::super::biped::Figure;
        let base = Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: false,
                ..default()
            }),
        );
        let body = HumanoidBody::resolve(&base);
        let skin = torso_shape(body, None, false);
        let tee = test_envelope(ShirtAppearance::TShirt, body, &skin);
        let woven = test_envelope(ShirtAppearance::HawaiianShirt, body, &skin);
        let mut extra = 0.0f32;
        for y in [0.08, 0.12, 0.16, 0.20] {
            let tight = torso_surface_hit(&tee, 0.0, y, true).unwrap();
            let loose = torso_surface_hit(&woven, 0.0, y, true).unwrap();
            extra = extra.max(loose.z - tight.z);
            assert!(loose.z - tight.z < 0.035 * body.build);
        }
        assert!(
            extra > 0.004 * body.build,
            "woven waist must have real ease: {extra}"
        );
        let figures: Vec<_> = ShirtAppearance::ALL
            .into_iter()
            .map(|kind| {
                let mut equipment = Equipment::default();
                equipment.select(kind.name()).unwrap();
                equipment.select("plate-mail").unwrap();
                Figure::resolve(body, &base.clone().equipped(equipment))
            })
            .collect();
        let retained_skin = torso_shape(body, None, figures[1].trim_neck);
        for x in [-0.18, -0.09, 0.0, 0.09, 0.18] {
            for y in [0.08, 0.20, 0.35, 0.48] {
                for z in [-0.12, 0.0, 0.12, 0.20] {
                    let at = Vec3::new(x, y, z);
                    assert_eq!(
                        figures[0].equipped_plate.as_ref().unwrap().distance(at),
                        figures[1].equipped_plate.as_ref().unwrap().distance(at),
                        "inner shirt choice cannot reshape plate armor"
                    );
                    assert_eq!(
                        figures[1].shirt_shape.as_ref().unwrap().distance(at),
                        retained_skin.distance(at),
                        "hidden relaxed envelope must not participate in armor fitting"
                    );
                }
            }
        }
    }

    #[test]
    fn scoop_binding_has_a_joined_round_bottom_and_narrow_knit_edge() {
        let body = HumanoidBody::resolve(&Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                ..default()
            }),
        ));
        let cut = NecklineCut::resolve(
            Neckline::Scoop {
                depth: 0.28,
                width: 0.18,
            },
            body,
        )
        .unwrap();
        let path = scoop_binding(cut);
        assert_eq!(path.len(), 33);
        assert_eq!(
            path[16].0.x, 0.0,
            "one shared bottom, not two separated binding tips"
        );
        assert!(path.iter().all(|(_, width)| *width <= 0.006));
        let near = path[17].0 - path[16].0;
        assert!(
            near.y.abs() < near.x * 0.20,
            "bottom tangent must be almost horizontal"
        );
        for pair in path.windows(2) {
            assert!(pair[1].0.x > pair[0].0.x);
            assert!(pair[0].0.distance(pair[1].0) < 0.025);
        }
    }

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
            let neckline = NecklineCut::resolve(
                Neckline::Plunge {
                    depth: if woman { 0.65 } else { 0.15 },
                    width: if woman { 0.16 } else { 0.12 },
                    wrap: false,
                },
                body,
            )
            .unwrap();
            for (index, side) in [-1.0, 1.0].into_iter().enumerate() {
                let sh = body.shoulders * body.build;
                let corners = [
                    Vec2::new(side * 0.040 * sh, 0.523),
                    Vec2::new(side * 0.10 * sh, 0.500),
                    Vec2::new(side * 0.142 * sh, 0.434),
                    Vec2::new(side * (neckline.edge(0.48) + 0.006), 0.465),
                ];
                // Witness the interior of an emitted triangle, not a fixed
                // chest point which lies outside the female collar leaf.
                // Evaluate its three supporting torso points independently so
                // chest curvature cannot masquerade as a raised cloth fold.
                let samples = [(0.5, 0.5), (2.0 / 3.0, 0.5), (2.0 / 3.0, 2.0 / 3.0)];
                let mut under = Vec3::ZERO;
                let mut expected_turn = 0.0;
                for (u, v) in samples {
                    let at = corners[0]
                        .lerp(corners[1], u)
                        .lerp(corners[3].lerp(corners[2], u), v);
                    under += torso_surface_hit(&shirt, at.x, at.y, true)
                        .expect("collar triangle must have torso support")
                        / 3.0;
                    expected_turn += 0.012 * body.build * (0.2 + 0.8 * (PI * u).sin()) * v / 3.0;
                }
                let origin = Vec3::new(under.x, under.y, 0.0);
                let leaf = p
                    .matte
                    .outer_surface(origin, Vec3::Z)
                    .expect("interior of folded collar triangle");
                // The emission offset alone (0.004 plus patch layering) is
                // insufficient: removing the actual fold must fail this test.
                let base_offset = 0.004 + (index + 1) as f32 * 0.0006;
                let observed_turn = leaf.z - under.z - base_offset;
                assert!(expected_turn > 0.005 * body.build);
                assert!(
                    (observed_turn - expected_turn).abs() < 0.00002,
                    "woman={woman}, side={side}: actual fold {observed_turn}, expected {expected_turn}"
                );
            }
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
