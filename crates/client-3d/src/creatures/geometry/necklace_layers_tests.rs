//! Production caller witness: native plate and shell without equipped armor.
use super::*;

#[test]
fn torso_necklaces_use_native_outer_layers_without_equipped_plate() {
    use crate::creatures::outfit::Neck;
    let point_key = |p: Vec3| p.to_array().map(f32::to_bits);
    let triangles = |p: &Parts| {
        [&p.metal, &p.satin, &p.matte].map(|b| {
            let mut m = std::collections::BTreeMap::new();
            for triangle in b.surface_triangles() {
                *m.entry(triangle.map(point_key)).or_insert(0usize) += 1;
            }
            m
        })
    };
    for outer in [
        Outer::Cuirass,
        Outer::Shell {
            color: Vec3::Y,
            top: 0.55,
            sleeveless: true,
            leather: true,
            quilted: false,
            underbust: false,
            opening: ShellOpening::Closed,
            peplum: None,
        },
    ] {
        for neck in [
            Neck::Pendant {
                metal: Vec3::ONE,
                gem: Vec3::X,
            },
            Neck::Teeth { cord: Vec3::Z },
            Neck::Collar {
                metal: Vec3::ONE,
                beads: Vec3::Y,
            },
        ] {
            let mut look = Look::with_choice(
                CreatureModel::Human,
                22,
                Some(Variant {
                    woman: true,
                    ..default()
                }),
            );
            let mut outfit = Outfit::base(&look);
            outfit.outer = Some(outer);
            outfit.bodice.support = Support::None;
            outfit.trimmings.neck = Some(neck);
            look.outfit = Some(outfit);
            let body = HumanoidBody::resolve(&look);
            let figure = Figure::resolve(body, &look);
            assert!(figure.equipped_plate.is_none());
            let skin = torso_shape(body, None, figure.trim_neck);
            let layer = figure
                .native_plate
                .as_ref()
                .or(figure.shell_shape.as_ref())
                .expect("actual native layer");
            let support = torso_layer_support(&skin, layer);
            let at = Vec3::new(0.0, 0.48, 0.0);
            let bare = surface_ray_hit(&skin, at, Vec3::Z, 0.0, 0.45).unwrap();
            let dressed = surface_ray_hit(&support, at, Vec3::Z, 0.0, 0.45).unwrap();
            assert!(
                dressed.z - bare.z > 0.01,
                "fixture must distinguish native layer support"
            );
            let mut expected = Parts::default();
            accessory_parts::neck_collar(&mut expected, neck, body, &support, &look);
            assert!(
                expected.metal.surface_triangles().count()
                    + expected.satin.surface_triangles().count()
                    > 0
            );
            let mut actual = Parts::default();
            torso_parts(&mut actual, &figure);
            outfit.trimmings.neck = None;
            look.outfit = Some(outfit);
            let mut without = Parts::default();
            torso_parts(&mut without, &Figure::resolve(body, &look));
            let mut difference = triangles(&actual);
            for (after, before) in difference.iter_mut().zip(triangles(&without)) {
                for (key, count) in before {
                    let remaining = after
                        .get_mut(&key)
                        .expect("necklace must preserve underlying meshes");
                    assert!(*remaining >= count);
                    *remaining -= count;
                }
                after.retain(|_, count| *count > 0);
            }
            assert_eq!(
                difference,
                triangles(&expected),
                "actual torso caller must pass its native outer support"
            );
            for b in [&expected.metal, &expected.satin, &expected.matte] {
                for t in b.surface_triangles() {
                    for p in [t[0], t[1], t[2], (t[0] + t[1] + t[2]) / 3.0] {
                        assert!(
                            support.distance(p) >= 0.001,
                            "native layer clearance {}",
                            support.distance(p)
                        );
                    }
                }
            }
        }
    }
}
