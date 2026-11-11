//! Localized posed support for front-only apron panels.
use super::*;
use bevy::render::mesh::VertexAttributeValues;
use std::collections::BTreeMap;

const GAP: f32 = 0.004;
const PIN_Y: f32 = 0.075;
const CELL: f32 = 0.04;

/// An instance-owned front panel, distinct from a surrounding robe solve.
pub(super) struct Panel {
    pub entity: Entity,
    mesh: Handle<Mesh>,
    rest: Vec<Vec3>,
    rest_normals: Vec<[f32; 3]>,
    triangles: Vec<[usize; 3]>,
    from_robe: bool,
    rejected: u64,
}
impl Panel {
    pub fn new(entity: Entity, mesh: Handle<Mesh>, meshes: &Assets<Mesh>, from_robe: bool) -> Self {
        let source = meshes.get(&mesh).expect("owned apron mesh");
        let VertexAttributeValues::Float32x3(points) = source
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .expect("apron positions")
        else {
            panic!("apron xyz")
        };
        let VertexAttributeValues::Float32x3(normals) = source
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .expect("apron normals")
        else {
            panic!("apron normal xyz")
        };
        let indices: Vec<_> = source
            .indices()
            .map(|i| i.iter().collect())
            .unwrap_or_else(|| (0..points.len()).collect());
        Self {
            entity,
            mesh,
            rest_normals: normals.clone(),
            rest: points.iter().map(|p| Vec3::from_array(*p)).collect(),
            triangles: indices.as_chunks::<3>().0.to_vec(),
            from_robe,
            rejected: 0,
        }
    }
    pub fn rejected(&self) -> u64 {
        self.rejected
    }
    pub fn map_references(
        &mut self,
        entity: &impl Fn(Entity) -> Entity,
        mesh: &mut impl FnMut(&Handle<Mesh>) -> Handle<Mesh>,
    ) {
        self.entity = entity(self.entity);
        self.mesh = mesh(&self.mesh);
    }
}

/// Clip an actual support face to one projected cloth face. Interpolated Z is
/// retained: a distant vertex of a crossing knee triangle cannot inflate cloth.
fn intersection(mut polygon: Vec<Vec3>, cloth: [Vec3; 3]) -> Vec<Vec3> {
    let winding = (cloth[1] - cloth[0])
        .truncate()
        .perp_dot((cloth[2] - cloth[0]).truncate())
        .signum();
    if winding == 0.0 {
        return Vec::new();
    }
    for edge in 0..3 {
        let a = cloth[edge].truncate();
        let b = cloth[(edge + 1) % 3].truncate();
        let distance = |p: Vec3| (b - a).perp_dot(p.truncate() - a) * winding;
        let mut out = Vec::new();
        for i in 0..polygon.len() {
            let p = polygon[i];
            let q = polygon[(i + 1) % polygon.len()];
            let dp = distance(p);
            let dq = distance(q);
            if dp >= 0.0 {
                out.push(p);
            }
            if (dp >= 0.0) != (dq >= 0.0) {
                out.push(p.lerp(q, dp / (dp - dq)));
            }
        }
        polygon = out;
        if polygon.is_empty() {
            break;
        }
    }
    polygon
}
fn plane_depth(tri: [Vec3; 3], xy: Vec2) -> Option<f32> {
    let [a, b, c] = tri;
    let v = (b - a).truncate();
    let w = (c - a).truncate();
    let q = xy - a.truncate();
    let d = v.perp_dot(w);
    (d.abs() > 1e-10)
        .then(|| a.z + q.perp_dot(w) / d * (b.z - a.z) + v.perp_dot(q) / d * (c.z - a.z))
}
fn cells(tri: [Vec3; 3]) -> impl Iterator<Item = (i32, i32)> {
    let lo = tri
        .into_iter()
        .fold(Vec2::splat(f32::INFINITY), |a, p| a.min(p.truncate()));
    let hi = tri
        .into_iter()
        .fold(Vec2::splat(f32::NEG_INFINITY), |a, p| a.max(p.truncate()));
    let x0 = (lo.x / CELL).floor() as i32;
    let x1 = (hi.x / CELL).floor() as i32;
    let y0 = (lo.y / CELL).floor() as i32;
    let y1 = (hi.y / CELL).floor() as i32;
    (x0..=x1).flat_map(move |x| (y0..=y1).map(move |y| (x, y)))
}
fn solve(
    base: &[Vec3],
    faces: &[[usize; 3]],
    support: &[[Vec3; 3]],
    reference: &[Vec3],
) -> Result<Vec<Vec3>, &'static str> {
    if base.len() != reference.len() {
        return Err("apron rest topology changed");
    }
    let mut keys = BTreeMap::new();
    let mut xy = Vec::new();
    let mut pinned = Vec::new();
    let mut group = Vec::with_capacity(base.len());
    // Pair from immutable rest, not roundoff-perturbed robe-pose coordinates.
    for (p, rest) in base.iter().zip(reference) {
        let key = [
            if rest.x == 0.0 { 0 } else { rest.x.to_bits() },
            if rest.y == 0.0 { 0 } else { rest.y.to_bits() },
        ];
        let index = *keys.entry(key).or_insert_with(|| {
            let i = xy.len();
            xy.push(p.truncate());
            pinned.push(rest.y >= PIN_Y);
            i
        });
        group.push(index);
    }
    let mut bins: BTreeMap<(i32, i32), Vec<usize>> = BTreeMap::new();
    for (i, tri) in support.iter().enumerate() {
        for cell in cells(*tri) {
            bins.entry(cell).or_default().push(i);
        }
    }
    let mut required = vec![0.0f32; xy.len()];
    for face in faces {
        let tri = face.map(|i| base[i]);
        if plane_depth(tri, tri[0].truncate()).is_none() {
            continue;
        }
        let mut candidates = Vec::new();
        for cell in cells(tri) {
            if let Some(v) = bins.get(&cell) {
                candidates.extend(v.iter().copied());
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        let mut shift = 0.0f32;
        for index in candidates {
            for p in intersection(support[index].to_vec(), tri) {
                shift = shift
                    .max(p.z + GAP - plane_depth(tri, p.truncate()).expect("nondegenerate face"));
            }
        }
        if shift > 0.0 {
            for i in face {
                required[group[*i]] = required[group[*i]].max(shift);
            }
        }
    }
    // An outward-only slope-limited majorant spreads a local knee demand
    // without averaging below contact, translating the whole apron or moving
    // its waist/hem. Shared XY nodes preserve seams and front/back thickness.
    let smooth: Vec<_> = xy
        .iter()
        .map(|p| {
            xy.iter()
                .zip(&required)
                .map(|(q, d)| d - 1.25 * p.distance(*q))
                .fold(0.0f32, f32::max)
        })
        .collect();
    if pinned.iter().zip(&smooth).any(|(pin, d)| *pin && *d > 1e-6) {
        return Err("posed support conflicts with pinned apron waist");
    }
    if smooth.iter().any(|d| !d.is_finite() || *d > 0.25) {
        return Err("apron contact exceeds local shape budget");
    }
    Ok(base
        .iter()
        .enumerate()
        .map(|(i, p)| *p + Vec3::Z * smooth[group[i]])
        .collect())
}

pub(super) fn place(
    panels: &mut [Panel],
    support: &[visual_contact::PosedTriangles],
    frame: impl Fn(Entity) -> Option<Affine3A>,
    meshes: &mut Assets<Mesh>,
) {
    for panel in panels {
        let Some(transform) = frame(panel.entity) else {
            continue;
        };
        let inverse = transform.inverse();
        let posed: Vec<_> = support
            .iter()
            .flat_map(|part| {
                let xf = inverse * part.transform;
                part.triangles
                    .iter()
                    .map(move |tri| tri.map(|p| xf.transform_point3(p)))
            })
            .collect();
        let Some(mesh) = meshes.get_mut(&panel.mesh) else {
            continue;
        };
        // A preceding robe solve publishes its own fresh baseline each frame.
        // Ordinary aprons always start from immutable rest, never prior shifts.
        let baseline = if panel.from_robe {
            let Some(VertexAttributeValues::Float32x3(p)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                continue;
            };
            p.iter().map(|p| Vec3::from_array(*p)).collect()
        } else {
            panel.rest.clone()
        };
        match solve(&baseline, &panel.triangles, &posed, &panel.rest) {
            Ok(points) => {
                let deformed = points != baseline;
                mesh.insert_attribute(
                    Mesh::ATTRIBUTE_POSITION,
                    points.iter().map(|p| p.to_array()).collect::<Vec<_>>(),
                );
                if deformed {
                    mesh.compute_smooth_normals();
                } else if !panel.from_robe {
                    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, panel.rest_normals.clone());
                }
            }
            Err(reason) => {
                panel.rejected += 1;
                warn!("apron contact rejected: {reason}");
                mesh.insert_attribute(
                    Mesh::ATTRIBUTE_POSITION,
                    baseline.iter().map(|p| p.to_array()).collect::<Vec<_>>(),
                );
                if !panel.from_robe {
                    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, panel.rest_normals.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::creatures::*;
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
    fn depth(triangle: [Vec3; 3], xy: Vec2) -> Option<f32> {
        let [a, b, c] = triangle;
        let v = b.truncate() - a.truncate();
        let w = c.truncate() - a.truncate();
        let q = xy - a.truncate();
        let d = v.perp_dot(w);
        if d.abs() < 1e-10 {
            return None;
        }
        let u = q.perp_dot(w) / d;
        let t = v.perp_dot(q) / d;
        (u >= -1e-5 && t >= -1e-5 && u + t <= 1.00001)
            .then_some(a.z + u * (b.z - a.z) + t * (c.z - a.z))
    }
    fn fixture() -> (World, Entity) {
        let mut equipment = Equipment::default();
        equipment.select("apron").unwrap();
        equipment.select("hawaiian-shirt").unwrap();
        let look = Look::preview_adjusted(CreatureModel::Human, 22, |v| {
            *v = Variant {
                woman: false,
                build: 2,
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
    #[test]
    fn native_apron_clears_rendered_knees_at_review_ticks_20_and_53() {
        let (mut world, root) = fixture();
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
        let mut results = Vec::new();
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
            if ![20, 53].contains(&tick) {
                continue;
            }
            let rig = world.get::<Rig>(root).unwrap();
            let expected = if tick == 20 {
                EXPECTED_WALK
            } else {
                EXPECTED_STRIKE
            };
            let actual = world
                .get::<GlobalTransform>(rig.joint(J::LegL).unwrap())
                .unwrap()
                .compute_matrix()
                .to_cols_array();
            assert!(
                actual
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| (*a - b).abs() < 0.00001),
                "exact native captured pose mismatch tick{tick}: {actual:?}"
            );
            let panel = rig.skirt_layers[0];
            let inverse = world
                .get::<GlobalTransform>(panel)
                .unwrap()
                .affine()
                .inverse();
            let cloth = triangles(&world, panel, inverse);
            let mut worst = f32::NEG_INFINITY;
            let mut count = 0;
            for joint in [J::LegL, J::LegR, J::ShinL, J::ShinR] {
                for tri in triangles(&world, rig.joint(joint).unwrap(), inverse) {
                    for i in 0..=4 {
                        for j in 0..=4 - i {
                            let u = i as f32 / 4.0;
                            let v = j as f32 / 4.0;
                            let p = tri[0] * (1.0 - u - v) + tri[1] * u + tri[2] * v;
                            if let Some(z) = cloth
                                .iter()
                                .filter_map(|t| depth(*t, p.truncate()))
                                .reduce(f32::min)
                            {
                                count += 1;
                                worst = worst.max(p.z - z);
                            }
                        }
                    }
                }
            }
            assert!(count > 500, "positive actual knee overlap fixture");
            results.push((tick, worst, count));
        }
        assert!(results.iter().all(|(_,gap,_)| *gap<=-0.003),"rendered knee must remain behind apron back face with3mm directional clearance: {results:?}");
    }
    // Captured native50Hz scene, Human seed22 build2, tick20.
    const EXPECTED_WALK: [f32; 16] = [
        f32::from_bits(0x3f43155e),
        f32::from_bits(0x3c014716),
        f32::from_bits(0xbc585ed3),
        f32::from_bits(0x00000000),
        f32::from_bits(0xbbeeb0ac),
        f32::from_bits(0x3f42e9cf),
        f32::from_bits(0x3d0da90c),
        f32::from_bits(0x00000000),
        f32::from_bits(0x3c5e00ef),
        f32::from_bits(0xbd0d1d51),
        f32::from_bits(0x3f42e495),
        f32::from_bits(0x00000000),
        f32::from_bits(0x3da9c156),
        f32::from_bits(0x3e24f600),
        f32::from_bits(0x3e4111b8),
        f32::from_bits(0x3f800000),
    ];
    // Captured native50Hz scene, Human seed22 build2, tick53.
    const EXPECTED_STRIKE: [f32; 16] = [
        f32::from_bits(0x3f431d76),
        f32::from_bits(0x3be1e8f6),
        f32::from_bits(0xba5c1032),
        f32::from_bits(0x00000000),
        f32::from_bits(0xbbdb798c),
        f32::from_bits(0x3f348cc7),
        f32::from_bits(0xbe93f1a2),
        f32::from_bits(0x00000000),
        f32::from_bits(0xbaf0c27c),
        f32::from_bits(0x3e93f107),
        f32::from_bits(0x3f348ed5),
        f32::from_bits(0x00000000),
        f32::from_bits(0x3db26b52),
        f32::from_bits(0x3e50b0e2),
        f32::from_bits(0x3e29576c),
        f32::from_bits(0x3f800000),
    ];
    #[test]
    fn face_interior_support_preserves_duplicates_back_thickness_and_pins() {
        let base = vec![
            Vec3::new(-0.1, -0.2, 0.2),
            Vec3::new(0.1, -0.2, 0.2),
            Vec3::new(0.1, -0.4, 0.2),
            Vec3::new(-0.1, -0.4, 0.2),
            Vec3::new(-0.1, -0.2, 0.196),
            Vec3::new(0.1, -0.2, 0.196),
            Vec3::new(0.1, -0.4, 0.196),
            Vec3::new(-0.1, -0.4, 0.196),
            Vec3::new(-0.1, -0.2, 0.2),
            Vec3::new(0.0, 0.12, 0.2),
        ];
        let faces = [[0, 1, 2], [0, 2, 3], [4, 6, 5], [4, 7, 6]];
        // A triangle wholly inside the cloth face, missed by corner-only rays.
        let obstacle = [[
            Vec3::new(-0.03, -0.31, 0.245),
            Vec3::new(-0.01, -0.31, 0.245),
            Vec3::new(-0.02, -0.33, 0.245),
        ]];
        let result = super::solve(&base, &faces, &obstacle, &base).unwrap();
        for (a, b) in base.iter().zip(&result) {
            assert_eq!(a.truncate(), b.truncate());
            assert!(b.z >= a.z);
        }
        assert_eq!(
            result[0], result[8],
            "duplicate render corners move together"
        );
        for i in 0..4 {
            assert!(((result[i].z - result[i + 4].z) - 0.004).abs() < 1e-6);
        }
        assert_eq!(result[9], base[9], "waist pin never moves");
        for point in obstacle[0] {
            let back = faces[2..]
                .iter()
                .filter_map(|f| depth(f.map(|i| result[i]), point.truncate()))
                .reduce(f32::min)
                .unwrap();
            assert!(
                back >= 0.2489,
                "independent back-face depth at interior support"
            );
        }
        assert_eq!(
            super::solve(&base, &faces, &[], &base).unwrap(),
            base,
            "no accumulated previous-frame displacement"
        );
        let pinned = [
            Vec3::new(-0.1, 0.1, 0.2),
            Vec3::new(0.1, 0.1, 0.2),
            Vec3::new(0.0, 0.2, 0.2),
        ];
        assert!(
            super::solve(
                &pinned,
                &[[0, 1, 2]],
                &[pinned.map(|p| p + Vec3::Z * 0.01)],
                &pinned
            )
            .is_err(),
            "pin conflict must reject rather than move attachment"
        );
        assert!(
            super::solve(&base, &faces, &[obstacle[0].map(|p| p + Vec3::Z)], &base).is_err(),
            "unbounded shape demand must reject"
        );
    }

    #[test]
    fn warmed_native_aprons_have_private_meshes_and_contact_does_not_mutate_neighbor() {
        use bevy::asset::RenderAssetUsages;
        let mut equipment = Equipment::default();
        equipment.select("apron").unwrap();
        let look = Look::of(CreatureModel::Human, 22).equipped(equipment);
        let (mut world, mut first, mut library, mut meshes) = rogue_tests::spawn_one(&look);
        let parent = world.spawn(Transform::default()).id();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let second = spawn(
            &mut Commands::new(&mut queue, &world),
            &mut library,
            &mut meshes,
            parent,
            &look,
        );
        queue.apply(&mut world);
        assert!(first.robe.is_none() && second.robe.is_none());
        assert_eq!(first.aprons.len(), 1);
        assert_eq!(second.aprons.len(), 1);
        let a = &first.aprons[0];
        let b = &second.aprons[0];
        assert_ne!(a.mesh, b.mesh);
        let neighbor = b.rest.clone();
        let baseline = a.rest.clone();
        let entity = a.entity;
        assert!(meshes
            .get(&a.mesh)
            .unwrap()
            .asset_usage
            .contains(RenderAssetUsages::MAIN_WORLD));
        assert!(meshes
            .get(&b.mesh)
            .unwrap()
            .asset_usage
            .contains(RenderAssetUsages::MAIN_WORLD));
        let support = visual_contact::PosedTriangles::new(
            std::sync::Arc::new(vec![[
                Vec3::new(-0.05, -0.3, 0.36),
                Vec3::new(0.05, -0.3, 0.36),
                Vec3::new(0.0, -0.4, 0.36),
            ]]),
            Affine3A::IDENTITY,
        );
        super::place(
            &mut first.aprons,
            std::slice::from_ref(&support),
            |e| (e == entity).then_some(Affine3A::IDENTITY),
            &mut meshes,
        );
        let points = |meshes: &Assets<Mesh>, handle: &Handle<Mesh>| {
            let VertexAttributeValues::Float32x3(p) = meshes
                .get(handle)
                .unwrap()
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .unwrap()
            else {
                panic!("xyz")
            };
            p.iter().map(|p| Vec3::from_array(*p)).collect::<Vec<_>>()
        };
        assert_eq!(points(&meshes, &second.aprons[0].mesh), neighbor);
        assert_ne!(
            points(&meshes, &first.aprons[0].mesh),
            baseline,
            "fixture actually deforms first wearer"
        );
        let first_pose = points(&meshes, &first.aprons[0].mesh);
        super::place(
            &mut first.aprons,
            std::slice::from_ref(&support),
            |e| (e == entity).then_some(Affine3A::IDENTITY),
            &mut meshes,
        );
        assert_eq!(
            points(&meshes, &first.aprons[0].mesh),
            first_pose,
            "repeated identical pose has no accumulation"
        );
        super::place(
            &mut first.aprons,
            std::slice::from_ref(&support),
            |e| (e == entity).then_some(Affine3A::from_translation(Vec3::NEG_Z * 0.03)),
            &mut meshes,
        );
        assert_ne!(
            points(&meshes, &first.aprons[0].mesh),
            first_pose,
            "drape frame change is observed with unchanged leg geometry"
        );
        super::place(
            &mut first.aprons,
            &[],
            |e| (e == entity).then_some(Affine3A::IDENTITY),
            &mut meshes,
        );
        assert_eq!(
            points(&meshes, &first.aprons[0].mesh),
            baseline,
            "rest restored when support leaves"
        );
        assert_eq!(points(&meshes, &second.aprons[0].mesh), neighbor);
        assert_eq!(first.aprons[0].rejected, 0);
    }
    #[test]
    fn robe_cached_pose_republishes_baseline_and_rotated_pairs_keep_thickness() {
        let mut equipment = Equipment::default();
        equipment.select("apron").unwrap();
        let look = Look::of(CreatureModel::Mage, 22).equipped(equipment);
        let (_world, mut rig, _, mut meshes) = rogue_tests::spawn_one(&look);
        assert_eq!(rig.aprons.len(), 1);
        let entity = rig.aprons[0].entity;
        let handle = rig.aprons[0].mesh.clone();
        let frame = Affine3A::from_rotation_translation(
            Quat::from_euler(EulerRot::XYZ, 0.09, 0.17, -0.04),
            Vec3::ZERO,
        );
        let pose = |rig: &mut Rig, meshes: &mut Assets<Mesh>| {
            let robe = rig.robe.as_mut().unwrap();
            robe.update_layer_transforms(&|e| (e == entity).then_some(frame));
            crate::creatures::robe::place(
                robe,
                &rig.rest,
                &[Quat::IDENTITY; JOINTS],
                &[Vec3::ZERO; JOINTS],
                meshes,
            );
        };
        let points = |meshes: &Assets<Mesh>| {
            let VertexAttributeValues::Float32x3(p) = meshes
                .get(&handle)
                .unwrap()
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .unwrap()
            else {
                panic!("xyz")
            };
            p.iter().map(|p| Vec3::from_array(*p)).collect::<Vec<_>>()
        };
        pose(&mut rig, &mut meshes);
        rig.robe
            .as_ref()
            .unwrap()
            .publish_overlay_baselines(&mut meshes);
        let baseline = points(&meshes);
        let z = rig.aprons[0]
            .triangles
            .iter()
            .filter_map(|f| depth(f.map(|i| baseline[i]), Vec2::new(0.0, -0.3)))
            .reduce(f32::max)
            .expect("fixture inside apron");
        let support = visual_contact::PosedTriangles::new(
            std::sync::Arc::new(vec![[
                Vec3::new(-0.015, -0.30, z + 0.025),
                Vec3::new(0.015, -0.30, z + 0.025),
                Vec3::new(0.0, -0.33, z + 0.025),
            ]
            .map(|p| frame.transform_point3(p))]),
            Affine3A::IDENTITY,
        );
        super::place(
            &mut rig.aprons,
            std::slice::from_ref(&support),
            |e| (e == entity).then_some(frame),
            &mut meshes,
        );
        let deformed = points(&meshes);
        assert_ne!(deformed, baseline);
        assert_eq!(rig.aprons[0].rejected, 0);
        let mut groups = std::collections::BTreeMap::<[u32; 2], Vec<usize>>::new();
        for (i, p) in rig.aprons[0].rest.iter().enumerate() {
            groups
                .entry([p.x.to_bits(), p.y.to_bits()])
                .or_default()
                .push(i);
        }
        let mut pairs = 0;
        let mut perturbed = 0;
        for indices in groups.values() {
            for &i in indices {
                for &j in indices {
                    if (rig.aprons[0].rest[i].z - rig.aprons[0].rest[j].z).abs() < 0.003 {
                        continue;
                    }
                    pairs += 1;
                    perturbed += usize::from(baseline[i].truncate() != baseline[j].truncate());
                    assert!(
                        (deformed[i] - baseline[i]).distance(deformed[j] - baseline[j]) < 1e-6,
                        "paired cloth sides use the same displacement after rotated robe roundtrip"
                    );
                }
            }
        }
        assert!(
            pairs > 100 && perturbed > 0,
            "exercise actual front/back roundoff, not only synthetic coincident nodes"
        );
        pose(&mut rig, &mut meshes);
        assert_eq!(
            points(&meshes),
            deformed,
            "unchanged robe pose really exercised its cache-hit path"
        );
        rig.robe
            .as_ref()
            .unwrap()
            .publish_overlay_baselines(&mut meshes);
        super::place(
            &mut rig.aprons,
            &[],
            |e| (e == entity).then_some(frame),
            &mut meshes,
        );
        assert_eq!(
            points(&meshes),
            baseline,
            "removed knee support restores robe-only baseline, not stale apron output"
        );
        let VertexAttributeValues::Float32x3(actual_normals) = meshes
            .get(&handle)
            .unwrap()
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .unwrap()
        else {
            panic!("normals")
        };
        assert!(actual_normals
            .iter()
            .all(|n| Vec3::from_array(*n).is_finite()));
    }
}
