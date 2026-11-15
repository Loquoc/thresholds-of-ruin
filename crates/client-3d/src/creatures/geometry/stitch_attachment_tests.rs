use super::*;

#[test]
fn native_zombie_stitches_follow_the_full_torso_span() {
    for woman in [false, true] {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Zombie,
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
            let ornament = figure.outfit.torso_ornament.expect("native sewn stitches");
            assert!(matches!(ornament, TorsoOrnament::StitchedFront { .. }));
            for smooth in [false, true] {
                let mut parts = Parts::default();
                parts.matte.smooth = smooth;
                ornament_parts::emit(&mut parts, ornament, body, &garment, &skin, None);
                let triangles: Vec<_> = parts.matte.surface_triangles().collect();
                assert!(triangles.len() >= 36, "all three stitches remain present");
                for height in [0.2_f32, 0.24, 0.28] {
                    for x in [0.02, 0.055, 0.09] {
                        assert!(
                            parts
                                .matte
                                .outer_surface(Vec3::new(x * body.build, height, 0.0), Vec3::Z)
                                .is_some(),
                            "positive full-width stitch witness {woman}/{build}/{smooth}/{height}/{x}"
                        );
                    }
                }
                // Inspect the emitted surface, including face/edge interiors, not
                // the production projection or proposed new fitting locations.
                for tri in triangles {
                    for at in [
                        tri[0],
                        tri[1],
                        tri[2],
                        (tri[0] + tri[1]) * 0.5,
                        (tri[1] + tri[2]) * 0.5,
                        (tri[2] + tri[0]) * 0.5,
                        (tri[0] + tri[1] + tri[2]) / 3.0,
                    ] {
                        let gap = garment.distance(at);
                        assert!((-0.002..=0.016).contains(&gap),
                            "sewn stitch must not bury or float across torso: gap={gap}, {woman}/{build}/{smooth}, at={at:?}");
                    }
                }
            }
        }
    }
}
