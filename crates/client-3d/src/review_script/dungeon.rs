//! Bounded review-only room using the production sculpted cave geometry/shader.
//! This is a rendering fixture, not a simulated or network-disclosed dungeon.
use super::*;
use crate::cave::{CaveExtension, CaveMaterial, CaveParams, CaveWorld, Solid};
use std::collections::{HashMap, HashSet};

pub(super) fn setup(
    commands: &mut Commands,
    script: &Script,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    cave_materials: &mut Assets<CaveMaterial>,
) {
    let (mut cave, dirty) = room(script);
    let floor = Vec3::from_array(script.floor_color);
    let params = CaveParams {
        floor_a: (floor * 0.8).extend(1.0),
        floor_b: (floor * 1.15).extend(1.0),
        camera: Vec3::from_array(script.cameras[0].position).extend(1.5),
        ..default()
    };
    let rock = cave_materials.add(CaveMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.88,
            ..default()
        },
        extension: CaveExtension { params },
    });
    let crystal = [
        (
            Color::srgb(0.35, 0.95, 0.85),
            LinearRgba::rgb(0.5, 2.6, 2.1),
        ),
        (Color::srgb(0.75, 0.5, 1.0), LinearRgba::rgb(1.7, 0.7, 2.8)),
    ]
    .map(|(base_color, emissive)| {
        materials.add(StandardMaterial {
            base_color,
            emissive,
            perceptual_roughness: 0.12,
            reflectance: 0.8,
            ..default()
        })
    });
    for chunk in dirty {
        let built = cave.build(chunk);
        if let Some(mesh) = built.rock {
            commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(rock.clone())));
        }
        for (mesh, material) in built.crystals.into_iter().zip(&crystal) {
            if let Some(mesh) = mesh {
                commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material.clone())));
            }
        }
        for (at, glow) in built.lights {
            commands.spawn((
                PointLight {
                    color: glow.light(),
                    intensity: 9000.0,
                    range: 4.5,
                    radius: 0.15,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_translation(at),
            ));
        }
    }
    crate::scene::studio_lights(commands);
}

fn room(script: &Script) -> (CaveWorld, HashSet<IVec3>) {
    let mut solids = HashMap::new();
    let mut open = HashSet::new();
    for x in -5i32..=5 {
        for z in -4i32..=4 {
            let key = ((x + 5) * 131 + (z + 4) * 53 + 17) as u64;
            solids.insert(
                IVec3::new(x, -1, z),
                Solid {
                    seen: true,
                    lit: true,
                    key,
                },
            );
            open.insert(IVec3::new(x, 0, z));
            // Back and side boundaries leave the normal play camera unobscured.
            if z == -4 || x.abs() == 5 {
                for y in 0..=1 {
                    solids.insert(
                        IVec3::new(x, y, z),
                        Solid {
                            seen: true,
                            lit: true,
                            key: key + y as u64,
                        },
                    );
                }
            }
        }
    }
    let blocked = script
        .ground_items
        .iter()
        .map(|spec| {
            let p = ground_cell(spec.position);
            IVec3::new(p.x, p.z, p.y)
        })
        .chain(script.actors.iter().map(|actor| {
            let p = actor.position;
            IVec3::new(
                p[0].round() as i32,
                (p[1] + 0.5).round() as i32,
                p[2].round() as i32,
            )
        }))
        .collect();
    let mut cave = CaveWorld::default();
    let dirty = cave.update(solids, open.clone(), open, blocked);
    (cave, dirty)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_room_uses_real_cave_and_keeps_loot_cells_clear_of_decoration() {
        let mut script =
            Script::parse(include_bytes!("../../review-scripts/rogue-armor.json")).unwrap();
        script.ground_items = vec![GroundProp {
            asset: None,
            model: "Coins".into(),
            position: [1.0, -0.5, 2.0],
            yaw: 0.0,
            disclosed_name: None,
            quantity: 1,
        }];
        let (mut cave, dirty) = room(&script);
        assert!(cave.blocked.contains(&IVec3::new(1, 0, 2)));
        assert!(cave.solids.contains_key(&IVec3::new(1, -1, 2)));
        assert!(cave.solids.contains_key(&IVec3::new(5, 1, 0)));
        assert!(!cave.solids.contains_key(&IVec3::new(0, 0, 0)));
        assert!(dirty
            .into_iter()
            .any(|chunk| cave.build(chunk).rock.is_some()));
    }
}
