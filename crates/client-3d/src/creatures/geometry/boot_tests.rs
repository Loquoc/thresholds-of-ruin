//! Boot silhouette contracts measure the emitted triangle surface, including swaps.
use super::*;
use crate::creatures::rogue_tests::spawn_one;
use bevy::render::mesh::VertexAttributeValues;

fn section_floor(world: &World, meshes: &Assets<Mesh>, joint: Entity, z: f32) -> f32 {
    let mut cuts = Vec::new();
    for child in world.get::<Children>(joint).unwrap().iter() {
        let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0)) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions")
        };
        let at = world.get::<Transform>(child).unwrap().compute_affine();
        let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
        for tri in indices.as_chunks::<3>().0 {
            let p = tri.map(|i| at.transform_point3(points[i].into()));
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                let da = p[a].z - z;
                let db = p[b].z - z;
                if da.abs() < 1e-6 {
                    cuts.push(p[a].y);
                }
                if da * db < 0.0 {
                    cuts.push(p[a].lerp(p[b], da / (da - db)).y);
                }
            }
        }
    }
    assert!(!cuts.is_empty());
    cuts.into_iter().fold(f32::INFINITY, f32::min)
}

#[test]
fn constructed_boots_have_a_raised_arch_only_when_the_outfit_has_a_heel() {
    for (model, wardrobe) in [
        (CreatureModel::Vampire, Wardrobe::Native),
        (CreatureModel::Medusa, Wardrobe::Native),
        (CreatureModel::Orc, Wardrobe::Ranger),
        (CreatureModel::Vampire, Wardrobe::Courtier),
    ] {
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
                )
                .dressed(wardrobe);
                let Feet::Boots(boots) = Outfit::worn(&look).feet else {
                    panic!("boot fixture")
                };
                let (world, rig, _, meshes) = spawn_one(&look);
                let body = rig.humanoid.unwrap();
                for foot in [J::FootL, J::FootR] {
                    let arch = section_floor(
                        &world,
                        &meshes,
                        rig.joint(foot).unwrap(),
                        0.03 * body.build * super::super::anatomy::FOOT_SCALE.z,
                    );
                    if boots.heel {
                        assert!(arch> -0.054,"{model:?} {wardrobe:?} woman {woman} build {build}: arch filled to {arch}");
                    } else {
                        assert!(arch < -0.073, "flat boots keep a planted midsole: {arch}");
                    }
                }
            }
        }
    }
}
// Append to geometry/boot_tests.rs before applying production changes.
// Actual emitted material surfaces are the oracle, not the proposed fitter.
#[test]
fn native_boot_shafts_enclose_rendered_trouser_calves() {
    let mut worst = 0.0f32;
    let mut samples = 0;
    for woman in [true, false] {
        for build in 0..3 {
            let look = Look::preview_adjusted(CreatureModel::Human, 22, |v| {
                *v = Variant {
                    woman,
                    build,
                    bust: 2,
                    figure: 2,
                    ..default()
                };
            })
            .composed(Wardrobe::Native, Equipment::default())
            .unwrap();
            let (world, rig, library, meshes) = spawn_one(&look);
            let cloth_material = world
                .get::<Children>(rig.joint(J::LegL).unwrap())
                .unwrap()
                .iter()
                .find_map(|child| world.get::<MeshMaterial3d<StandardMaterial>>(child))
                .expect("native trouser thigh material")
                .0
                .clone();
            assert_ne!(cloth_material, library.materials.satin());
            let body = rig.humanoid.unwrap();
            let figure = biped::Figure::resolve(body, &look);
            assert!(figure.boots.is_some() && figure.trousered);
            for joint in [J::ShinL, J::ShinR] {
                let mut child_frame = None;
                let mut cloth = Builder::default();
                let mut leather = Builder::default();
                // Both models use the same child scale. Read authored triangle
                // positions to avoid introducing pose/scale assumptions here.
                for child in world
                    .get::<Children>(rig.joint(joint).unwrap())
                    .unwrap()
                    .iter()
                {
                    let Some(material) = world.get::<MeshMaterial3d<StandardMaterial>>(child)
                    else {
                        continue;
                    };
                    let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0))
                    else {
                        continue;
                    };
                    let Some(VertexAttributeValues::Float32x3(points)) =
                        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                    else {
                        panic!("positions");
                    };
                    let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                    let target = if material.0 == library.materials.satin() {
                        &mut leather
                    } else if material.0 == cloth_material {
                        &mut cloth
                    } else {
                        continue;
                    };
                    let frame = world.get::<Transform>(child).unwrap().compute_affine();
                    if let Some(expected) = child_frame {
                        assert_eq!(frame, expected, "cloth and leather authored frames agree");
                    }
                    child_frame = Some(frame);
                    for tri in indices.as_chunks::<3>().0 {
                        target.tri_out(
                            tri.map(|i| Vec3::from_array(points[i])),
                            Vec3::ZERO,
                            Vec3::ONE,
                        );
                    }
                }
                let shape = figure.boot_shape;
                for row in 1..32 {
                    let y = shape.bottom + (shape.top - shape.bottom) * row as f32 / 32.0;
                    for angle in 0..96 {
                        let theta = (angle as f32 + 0.37) * TAU / 96.0;
                        let direction = Vec3::new(theta.cos(), 0.0, theta.sin());
                        let origin = Vec3::Y * y;
                        let Some(support) = cloth.outer_surface(origin, direction) else {
                            panic!("clothed calf witness");
                        };
                        let Some(surface) = leather.outer_surface(origin, direction) else {
                            panic!("shaft witness");
                        };
                        worst = worst.max((support - surface).dot(direction));
                        samples += 1;
                    }
                }
            }
        }
    }
    assert!(samples > 30_000, "actual emitted surface coverage");
    assert!(
        worst <= 0.0001,
        "native boot exposes {worst} m of rendered trouser calf"
    );
}

#[test]
fn barbarian_boots_enclose_rendered_trousers_below_the_fringe() {
    let mut worst = 0.0f32;
    let mut samples = 0;
    for woman in [false] {
        for build in 0..3 {
            let look = Look::preview_adjusted(CreatureModel::Human, 22, |v| {
                *v = Variant {
                    woman,
                    build,
                    bust: 2,
                    figure: 2,
                    ..default()
                };
            })
            .composed(Wardrobe::Barbarian, Equipment::default())
            .unwrap();
            let (world, rig, library, meshes) = spawn_one(&look);
            let cloth_material = world
                .get::<Children>(rig.joint(J::LegL).unwrap())
                .unwrap()
                .iter()
                .find_map(|child| world.get::<MeshMaterial3d<StandardMaterial>>(child))
                .expect("native trouser thigh material")
                .0
                .clone();
            assert_ne!(cloth_material, library.materials.satin());
            let body = rig.humanoid.unwrap();
            let figure = biped::Figure::resolve(body, &look);
            assert!(figure.boots.is_some() && figure.trousered);
            for joint in [J::ShinL, J::ShinR] {
                let mut child_frame = None;
                let mut cloth = Builder::default();
                let mut leather = Builder::default();
                // Both models use the same child scale. Read authored triangle
                // positions to avoid introducing pose/scale assumptions here.
                for child in world
                    .get::<Children>(rig.joint(joint).unwrap())
                    .unwrap()
                    .iter()
                {
                    let Some(material) = world.get::<MeshMaterial3d<StandardMaterial>>(child)
                    else {
                        continue;
                    };
                    let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0))
                    else {
                        continue;
                    };
                    let Some(VertexAttributeValues::Float32x3(points)) =
                        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                    else {
                        panic!("positions");
                    };
                    let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                    let target = if material.0 == library.materials.satin() {
                        &mut leather
                    } else if material.0 == cloth_material {
                        &mut cloth
                    } else {
                        continue;
                    };
                    let frame = world.get::<Transform>(child).unwrap().compute_affine();
                    if let Some(expected) = child_frame {
                        assert_eq!(frame, expected, "cloth and leather authored frames agree");
                    }
                    child_frame = Some(frame);
                    for tri in indices.as_chunks::<3>().0 {
                        target.tri_out(
                            tri.map(|i| Vec3::from_array(points[i])),
                            Vec3::ZERO,
                            Vec3::ONE,
                        );
                    }
                }
                let shape = figure.boot_shape;
                for row in 1..32 {
                    let y = shape.bottom + (shape.top - 0.16 - shape.bottom) * row as f32 / 32.0;
                    for angle in 0..96 {
                        let theta = (angle as f32 + 0.37) * TAU / 96.0;
                        let direction = Vec3::new(theta.cos(), 0.0, theta.sin());
                        let origin = Vec3::Y * y;
                        let Some(support) = cloth.outer_surface(origin, direction) else {
                            panic!("clothed calf witness");
                        };
                        let Some(surface) = leather.outer_surface(origin, direction) else {
                            panic!("shaft witness");
                        };
                        worst = worst.max((support - surface).dot(direction));
                        samples += 1;
                    }
                }
            }
        }
    }
    assert!(samples > 15_000, "actual emitted surface coverage");
    assert!(
        worst <= 0.0001,
        "Barbarian boot below fringe exposes {worst} m of rendered trouser calf"
    );
}
