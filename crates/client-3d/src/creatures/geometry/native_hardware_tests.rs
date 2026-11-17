use super::*;

#[test]
fn supported_native_hardware_preserves_centers_counts_and_style() {
    for model in [CreatureModel::Vampire, CreatureModel::Hobgoblin] {
        for woman in [false, true] {
            for build in 0..3 {
                let look = Look::with_choice(
                    model,
                    42,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                );
                let body = HumanoidBody::resolve(&look);
                let figure = Figure::resolve(body, &look);
                let skin = figure.body_support();
                let garment = torso_layer_support(
                    &skin,
                    figure
                        .native_plate
                        .as_ref()
                        .or(figure.shell_shape.as_ref())
                        .unwrap_or(&skin),
                );
                let ornament = figure.outfit.torso_ornament.expect("native torso ornament");
                for smooth in [false, true] {
                    let mut parts = Parts::default();
                    parts.metal.smooth = smooth;
                    ornament_parts::emit(&mut parts, ornament, body, &garment, &skin, None);
                    let mut expected = Builder::default();
                    expected.smooth = smooth;
                    let mut stud = |x, y, radius, color| {
                        assert!(
                            torso_surface_hit(&garment, x, y, true).is_some(),
                            "native station needs no fallback"
                        );
                        expected.blob(
                            torso_surface(&garment, x, y, true),
                            Vec3::splat(radius),
                            0,
                            0.0,
                            color,
                        );
                    };
                    match ornament {
                        TorsoOrnament::RivetedVest { rivets, .. } => {
                            for k in 0..3 {
                                stud(-0.1 + k as f32 * 0.1, 0.16, 0.012, rivets);
                            }
                        }
                        TorsoOrnament::HighCollar { buttons, .. } => {
                            for side in [-1.0, 1.0] {
                                for k in 0..3 {
                                    stud(side * 0.055, 0.13 + k as f32 * 0.08, 0.011, buttons);
                                }
                            }
                        }
                        _ => panic!("unexpected native ornament"),
                    }
                    parts.metal.assert_same_recipe(&expected);
                }
            }
        }
    }
}

#[test]
fn leaf_bodice_pins_contact_the_composed_support_and_remain_exposed() {
    for model in [CreatureModel::Nymph, CreatureModel::Human] {
        for woman in [false, true] {
            // Male Druid deliberately substitutes a different torso recipe.
            if model == CreatureModel::Human && !woman {
                continue;
            }
            for build in 0..3 {
                let mut look = Look::with_choice(
                    model,
                    42,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                );
                if model == CreatureModel::Human {
                    look = look.dressed(Wardrobe::Druid);
                }
                let body = HumanoidBody::resolve(&look);
                let figure = Figure::resolve(body, &look);
                let skin = figure.body_support();
                let garment = torso_layer_support(
                    &skin,
                    figure
                        .native_plate
                        .as_ref()
                        .or(figure.shell_shape.as_ref())
                        .unwrap_or(&skin),
                );
                let ornament = figure.outfit.torso_ornament.expect("leaf ornament");
                assert!(matches!(ornament, TorsoOrnament::LeafBodice { .. }));
                let mut parts = Parts::default();
                ornament_parts::emit(&mut parts, ornament, body, &garment, &skin, None);
                let points: Vec<_> = parts.metal.surface_triangles().flatten().collect();
                assert!(!points.is_empty(), "pin remains present");
                let min = points
                    .iter()
                    .map(|p| garment.distance(*p))
                    .fold(f32::INFINITY, f32::min);
                let max = points
                    .iter()
                    .map(|p| garment.distance(*p))
                    .fold(f32::NEG_INFINITY, f32::max);
                assert!(min < 0.001, "{model:?}/{woman}/{build} pin floats by {min}");
                assert!(
                    max > 0.008,
                    "{model:?}/{woman}/{build} pin must remain exposed"
                );
            }
        }
    }
}
