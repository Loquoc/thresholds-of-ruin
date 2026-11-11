//! Exact native short-skirt/tasset pose diagnostics using emitted triangle crossings.
use super::*;
use bevy::render::mesh::VertexAttributeValues;
use std::time::Duration;
fn triangles(world: &World, entity: Entity, inverse: Affine3A) -> Vec<[Vec3; 3]> {
    let meshes = world.resource::<Assets<Mesh>>();
    world
        .get::<Children>(entity)
        .unwrap()
        .iter()
        .flat_map(|child| {
            let Some(handle) = world.get::<Mesh3d>(child) else {
                return Vec::new();
            };
            let mesh = meshes.get(&handle.0).unwrap();
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("rendered points")
            };
            let frame = inverse * world.get::<GlobalTransform>(child).unwrap().affine();
            let indices: Vec<_> = mesh
                .indices()
                .map(|i| i.iter().collect())
                .unwrap_or_else(|| (0..points.len()).collect());
            indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|tri| {
                    std::array::from_fn(|i| {
                        frame.transform_point3(Vec3::from_array(points[tri[i]]))
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}
// Independent vertical ray oracle, not production collision queries.
fn fixture(chain: bool) -> (World, Entity) {
    let mut equipment = Equipment::default();
    if chain {
        equipment.select("chain-mail").unwrap();
    }
    let look = Look::preview_adjusted(CreatureModel::Human, 22, |v| {
        *v = Variant {
            woman: true,
            build: 0,
            bust: 2,
            figure: 2,
            ..default()
        };
    })
    .composed(Wardrobe::Native, equipment)
    .unwrap();
    let (mut world, rig, _, meshes) = rogue_tests::spawn_one(&look);
    let root = world
        .get::<ChildOf>(rig.joint(J::Lift).unwrap())
        .unwrap()
        .parent();
    let at = Vec3::new(0.0, -0.5, 0.0);
    let mut anim = Anim::new(22, at);
    anim.yaw = 0.0;
    anim.target_yaw = Some(0.0);
    *world.get_mut::<Transform>(root).unwrap() = Transform::from_translation(at);
    world.entity_mut(root).insert((rig, anim));
    world.insert_resource(meshes);
    world.insert_resource(Time::<()>::default());
    (world, root)
}

fn edge_hit(a: Vec3, b: Vec3, tri: [Vec3; 3]) -> Option<Vec3> {
    let d = b - a;
    let e1 = tri[1] - tri[0];
    let e2 = tri[2] - tri[0];
    let h = d.cross(e2);
    let det = e1.dot(h);
    if det.abs() < 1e-10 {
        return None;
    }
    let s = a - tri[0];
    let u = s.dot(h) / det;
    let q = s.cross(e1);
    let v = d.dot(q) / det;
    let t = e2.dot(q) / det;
    (u > 1e-5 && v > 1e-5 && u + v < 0.99999 && t > 1e-5 && t < 0.99999).then_some(a + d * t)
}
fn crossing(a: [Vec3; 3], b: [Vec3; 3]) -> Option<Vec3> {
    let bounds = |t: [Vec3; 3]| {
        (
            t.into_iter().fold(Vec3::splat(f32::INFINITY), Vec3::min),
            t.into_iter()
                .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max),
        )
    };
    let (al, ah) = bounds(a);
    let (bl, bh) = bounds(b);
    if ah.cmplt(bl).any() || bh.cmplt(al).any() {
        return None;
    }
    (0..3).find_map(|i| {
        edge_hit(a[i], a[(i + 1) % 3], b).or_else(|| edge_hit(b[i], b[(i + 1) % 3], a))
    })
}
#[test]
fn native_short_skirt_and_tassets_do_not_cross_actual_thigh_faces() {
    let mut failures = Vec::new();
    for chain in [false, true] {
        let (mut world, root) = fixture(chain);
        let mut advance = crate::review_script::animation_schedule();
        let mut refresh = Schedule::default();
        refresh.add_systems(
            (
                refresh_pose,
                bevy::transform::systems::mark_dirty_trees,
                bevy::transform::systems::sync_simple_transforms,
                bevy::transform::systems::propagate_parent_transforms,
            )
                .chain(),
        );
        let mut velocity = Vec3::ZERO;
        for tick in 0..=53 {
            world.resource_mut::<Time>().advance_by(if tick == 0 {
                Duration::ZERO
            } else {
                Duration::from_millis(20)
            });
            if tick > 0 {
                world.get_mut::<Transform>(root).unwrap().translation += velocity / 50.0;
            }
            advance.run(&mut world);
            match tick {
                1 => velocity = Vec3::Z * 0.5,
                21 => velocity = Vec3::ZERO,
                45 => {
                    world.get_mut::<Anim>(root).unwrap().act(Move::Strike);
                    world.entity_mut(root).insert(PoseRefresh);
                }
                _ => {}
            }
            refresh.run(&mut world);
            let Some(pose_index) = [0, 20, 53].iter().position(|&t| t == tick) else {
                continue;
            };
            let rig = world.get::<Rig>(root).unwrap();
            for (j, expected) in [J::Hips, J::LegL, J::LegR]
                .into_iter()
                .zip(EXPECTED[usize::from(chain)][pose_index])
            {
                let matrix = world
                    .get::<GlobalTransform>(rig.joint(j).unwrap())
                    .unwrap()
                    .compute_matrix()
                    .to_cols_array();
                assert!(
                    matrix
                        .iter()
                        .zip(expected)
                        .all(|(a, b)| (*a - f32::from_bits(b)).abs() < 1e-5),
                    "native pose parity chain={chain} tick={tick} joint={j:?} actual={matrix:?}"
                );
            }
            let inverse = world
                .get::<GlobalTransform>(rig.joint(J::Hips).unwrap())
                .unwrap()
                .affine()
                .inverse();
            let body: Vec<_> = [J::LegL, J::LegR]
                .into_iter()
                .flat_map(|j| triangles(&world, rig.joint(j).unwrap(), inverse))
                .collect();
            assert!(body.len() > 100, "actual emitted thighs");
            let layers = if chain { 2 } else { 1 };
            assert!(rig.skirt_layers.len() >= layers);
            for (layer, &entity) in rig.skirt_layers.iter().take(layers).enumerate() {
                let cloth = triangles(&world, entity, inverse);
                assert!(cloth.len() > 100, "actual emitted skirt faces");
                let mut hits = Vec::new();
                for (i, &face) in cloth.iter().enumerate() {
                    for (j, &support) in body.iter().enumerate() {
                        if let Some(p) = crossing(face, support).filter(|p| p.y < -0.055) {
                            hits.push((i, j, p));
                        }
                    }
                }
                println!(
                    "skirt crossings chain={chain} tick={tick} layer={layer} count={} first={:?}",
                    hits.len(),
                    &hits[..hits.len().min(6)]
                );
                if !hits.is_empty() {
                    failures.push((chain, tick, layer, hits.len()));
                }
            }
        }
    }
    assert!(failures.is_empty(),"actual skirt triangles cross actual thigh faces below waistband, not empty authored slits: {failures:?}");
}

// Exact captured matrices from armor-join-audit/{plain,chain-only} at0/20/53.
const EXPECTED: [[[[u32; 16]; 3]; 3]; 2] = [
    [
        [
            [
                1060197192, 0, 0, 0, 0, 1060197192, 0, 0, 0, 0, 1060197192, 0, 0, 1042404880, 0,
                1065353216,
            ],
            [
                1060197192, 0, 0, 0, 0, 1060197192, 0, 0, 0, 0, 1060197192, 0, 1032604474,
                1041475185, 0, 1065353216,
            ],
            [
                1060197192, 0, 0, 0, 0, 1060197192, 0, 0, 0, 0, 1060197192, 0, 3180088122,
                1041475185, 0, 1065353216,
            ],
        ],
        [
            [
                1060194748, 3142570258, 3160597572, 0, 998435183, 1060176158, 1026152121, 0,
                1012883190, 3173654869, 1060174025, 0, 0, 1038088640, 1044549469, 1065353216,
            ],
            [
                1060194250, 1006589755, 3160222979, 0, 3153968586, 1060196386, 992536976, 0,
                1012769457, 3139369648, 1060194904, 0, 1032591817, 1036190524, 1044401908,
                1065353216,
            ],
            [
                1060194749, 3145736802, 3160390344, 0, 3146113136, 1058356008, 3200224740, 0,
                1012851501, 1052741154, 1058353565, 0, 3180096913, 1036274706, 1044585725,
                1065353216,
            ],
        ],
        [
            [
                1060193979, 1015377373, 0, 0, 3162845286, 1060173072, 1026171219, 0, 981468128,
                3173651783, 1060176280, 0, 997726192, 1041971488, 1043108711, 1065353216,
            ],
            [
                1060196573, 1004834765, 986414195, 0, 3149923361, 1059322667, 3196514475, 0,
                3146121754, 1049029914, 1059323085, 0, 1033153670, 1041151820, 1043052963,
                1065353216,
            ],
            [
                1060196922, 3146948976, 986073770, 0, 999926023, 1059952520, 3188765648, 0,
                3121933469, 1041282466, 1059952763, 0, 3179533842, 1040935624, 1043052963,
                1065353216,
            ],
        ],
    ],
    [
        [
            [
                1060197192, 0, 0, 0, 0, 1060197192, 0, 0, 0, 0, 1060197192, 0, 0, 1042404880, 0,
                1065353216,
            ],
            [
                1060197192, 0, 0, 0, 0, 1060197192, 0, 0, 0, 0, 1060197192, 0, 1032604474,
                1041475185, 0, 1065353216,
            ],
            [
                1060197192, 0, 0, 0, 0, 1060197192, 0, 0, 0, 0, 1060197192, 0, 3180088122,
                1041475185, 0, 1065353216,
            ],
        ],
        [
            [
                1060194748, 3142570258, 3160597572, 0, 998435183, 1060176158, 1026152121, 0,
                1012883190, 3173654869, 1060174025, 0, 0, 1038088640, 1044549469, 1065353216,
            ],
            [
                1060194250, 1006589755, 3160222979, 0, 3153968586, 1060196386, 992536976, 0,
                1012769457, 3139369648, 1060194904, 0, 1032591817, 1036190524, 1044401908,
                1065353216,
            ],
            [
                1060194749, 3145736802, 3160390344, 0, 3146113136, 1058356008, 3200224740, 0,
                1012851501, 1052741154, 1058353565, 0, 3180096913, 1036274706, 1044585725,
                1065353216,
            ],
        ],
        [
            [
                1060193979, 1015377373, 0, 0, 3162845286, 1060173072, 1026171219, 0, 981468128,
                3173651783, 1060176280, 0, 997726192, 1041971488, 1043108711, 1065353216,
            ],
            [
                1060196573, 1004834765, 986414195, 0, 3149923361, 1059322667, 3196514475, 0,
                3146121754, 1049029914, 1059323085, 0, 1033153670, 1041151820, 1043052963,
                1065353216,
            ],
            [
                1060196922, 3146948976, 986073770, 0, 999926023, 1059952520, 3188765648, 0,
                3121933469, 1041282466, 1059952763, 0, 3179533842, 1040935624, 1043052963,
                1065353216,
            ],
        ],
    ],
];
