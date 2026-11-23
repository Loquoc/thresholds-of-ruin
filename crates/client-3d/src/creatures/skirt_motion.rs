//! Thigh-following motion for short, layered hip garments.
//!
//! All layers use the same continuous deformation in hip space. The waist stays
//! attached; the lower sides follow their respective thighs. Long skirts and
//! robes need their own lower-leg/contact solve and do not enter this owner.
use super::*;
use bevy::render::mesh::VertexAttributeValues;

pub(super) struct Panel {
    entity: Entity,
    mesh: Handle<Mesh>,
    scale: Vec3,
    rest: Vec<Vec3>,
    normals: Vec<[f32; 3]>,
    last: Option<[Affine3A; 4]>,
    follow_belt: bool,
    front_swing: Option<Vec<f32>>,
}
impl Panel {
    pub fn new(entity: Entity, mesh: Handle<Mesh>, scale: Vec3, meshes: &Assets<Mesh>) -> Self {
        let source = meshes.get(&mesh).expect("owned short garment");
        let Some(VertexAttributeValues::Float32x3(points)) =
            source.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("garment xyz");
        };
        let Some(VertexAttributeValues::Float32x3(normals)) =
            source.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("garment normals");
        };
        Self {
            entity,
            mesh,
            scale,
            rest: points.iter().map(|p| Vec3::from_array(*p)).collect(),
            normals: normals.clone(),
            last: None,
            follow_belt: false,
            front_swing: None,
        }
    }
    pub fn with_belt_anchor(mut self, follow_belt: bool) -> Self {
        self.follow_belt = follow_belt;
        self
    }
    pub fn with_front_swing(mut self, kind: Drape, amount: f32) -> Self {
        let Drape::Skirt(length) = kind else {
            return self;
        };
        if amount > 0.0 {
            self.front_swing = Some(
                self.rest
                    .iter()
                    .map(|p| {
                        // Retain waist seam and hem, and release only the
                        // upper panels in front of the thighs.
                        let t = (0.04 - p.y) / (0.04 + length);
                        let front = Vec2::new(p.x * 0.8, p.z).normalize_or_zero().y.max(0.0);
                        amount
                            * front.sqrt()
                            * animation::ease((t - 0.045) / 0.095)
                            * animation::ease((0.62 - t) / 0.35)
                    })
                    .collect(),
            );
        }
        self
    }
    pub fn invalidate(&mut self) {
        self.last = None;
    }
    pub fn map_references(
        &mut self,
        entity: &impl Fn(Entity) -> Entity,
        mesh: &mut impl FnMut(&Handle<Mesh>) -> Handle<Mesh>,
    ) {
        self.entity = entity(self.entity);
        self.mesh = mesh(&self.mesh);
        self.invalidate();
    }
}

fn follow(point: Vec3, roots: [Vec3; 2], legs: [Affine3A; 2]) -> Vec3 {
    let side = animation::ease((point.x + 0.04) / 0.08);
    let weight = animation::ease(-point.y / 0.08);
    let left = legs[0].transform_point3(point - roots[0]);
    let right = legs[1].transform_point3(point - roots[1]);
    let mut moved = point.lerp(right.lerp(left, side), weight);
    // The thigh may lift the free hem, not drag the upper attachment through
    // the fixed belt. Retain horizontal clearance while easing vertical lift
    // in only below the attachment region. Downward drape remains free.
    if moved.y > point.y {
        moved.y = point.y + (moved.y - point.y) * animation::ease((-0.04 - point.y) / 0.08);
    }
    moved
}

pub(super) fn place(
    panels: &mut [Panel],
    roots: [Vec3; 2],
    legs: [Affine3A; 2],
    torso_delta: Affine3A,
    frame: impl Fn(Entity) -> Option<Affine3A>,
    meshes: &mut Assets<Mesh>,
) {
    for panel in panels {
        let Some(transform) = frame(panel.entity) else {
            continue;
        };
        let anchor = if panel.follow_belt {
            torso_delta
        } else {
            Affine3A::IDENTITY
        };
        let key = [transform, legs[0], legs[1], anchor];
        if panel.last == Some(key) {
            continue;
        }
        let inverse = (transform * Affine3A::from_scale(panel.scale)).inverse();
        let [left_swing, right_swing] = panel.front_swing.as_ref().map_or([0.0; 2], |_| {
            legs.map(|leg| {
                let forward = leg.transform_vector3(Vec3::NEG_Y).z;
                animation::ease((forward - 0.04) / 0.43)
            })
        });
        let points: Vec<_> = panel
            .rest
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let rest = *p * panel.scale;
                let mut worn = follow(rest, roots, legs);
                if let Some(weights) = &panel.front_swing {
                    let side = animation::ease((rest.x + 0.04) / 0.08);
                    worn.z += weights[i] * (right_swing + (left_swing - right_swing) * side);
                }
                if panel.follow_belt {
                    let attach = animation::ease((p.y + 0.025) / 0.072);
                    worn = worn.lerp(anchor.transform_point3(rest), attach);
                }
                inverse.transform_point3(worn).to_array()
            })
            .collect();
        let Some(mesh) = meshes.get_mut(&panel.mesh) else {
            continue;
        };
        let rest = points
            .iter()
            .zip(&panel.rest)
            .all(|(p, r)| Vec3::from_array(*p).distance_squared(*r) < 1e-14);
        if rest {
            mesh.insert_attribute(
                Mesh::ATTRIBUTE_POSITION,
                panel.rest.iter().map(|p| p.to_array()).collect::<Vec<_>>(),
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, panel.normals.clone());
        } else {
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, points);
            mesh.compute_smooth_normals();
        }
        panel.last = Some(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look() -> Look {
        Look::preview_adjusted(CreatureModel::Human, 22, |v| {
            *v = Variant {
                woman: true,
                build: 0,
                bust: 2,
                figure: 2,
                ..default()
            };
        })
        .composed(Wardrobe::Native, Equipment::default())
        .unwrap()
    }
    fn positions(meshes: &Assets<Mesh>, handle: &Handle<Mesh>) -> Vec<[f32; 3]> {
        let Some(VertexAttributeValues::Float32x3(points)) = meshes
            .get(handle)
            .unwrap()
            .attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions");
        };
        points.clone()
    }
    #[test]
    fn upper_attachment_does_not_rise_through_fixed_belt() {
        let roots = [Vec3::new(0.1, -0.02, 0.0), Vec3::new(-0.1, -0.02, 0.0)];
        let legs = [
            Affine3A::from_rotation_translation(Quat::from_rotation_x(-0.6), roots[0]),
            Affine3A::from_rotation_translation(Quat::from_rotation_x(0.6), roots[1]),
        ];
        for y in [-0.04, -0.02, 0.0, 0.02, 0.04] {
            for x in [-0.18, 0.18] {
                let point = Vec3::new(x, y, 0.13);
                if y >= 0.0 {
                    assert_eq!(
                        follow(point, roots, legs),
                        point,
                        "upper attachment slides away from belt"
                    );
                }
                assert!(
                    follow(point, roots, legs).y <= y + 1e-6,
                    "upper attachment lifted at {point:?}"
                );
            }
        }
    }

    #[test]
    fn pinned_waist_and_same_side_layer_thickness_survive_motion() {
        let roots = [Vec3::new(0.1, -0.02, 0.0), Vec3::new(-0.1, -0.02, 0.0)];
        let legs = [
            Affine3A::from_rotation_translation(Quat::from_rotation_x(0.7), roots[0]),
            Affine3A::from_rotation_translation(Quat::from_rotation_x(-0.5), roots[1]),
        ];
        for x in [-0.2, -0.02, 0.0, 0.02, 0.2] {
            let p = Vec3::new(x, 0.04, 0.12);
            assert_eq!(follow(p, roots, legs), p);
        }
        for x in [-0.2, 0.2] {
            let inner = Vec3::new(x, -0.12, 0.12);
            let outer = inner + Vec3::Z * 0.007;
            let a = follow(inner, roots, legs);
            let b = follow(outer, roots, legs);
            assert!((a.distance(b) - 0.007).abs() < 1e-6);
            assert!(a.distance(inner) > 0.02);
        }
    }
    #[test]
    fn long_garments_and_aprons_keep_their_existing_motion_owners() {
        let princess = Look::preview_adjusted(CreatureModel::Human, 22, |v| v.woman = true)
            .dressed(Wardrobe::RoyalPrincess);
        let (_, rig, _, _) = rogue_tests::spawn_one(&princess);
        assert!(!rig.skirt_layers.is_empty());
        assert!(rig.short_garments.is_empty());
        let arcanist = Look::preview_adjusted(CreatureModel::RedMage, 22, |v| v.woman = true)
            .dressed(Wardrobe::Native);
        let (_, rig, _, _) = rogue_tests::spawn_one(&arcanist);
        assert!(
            rig.robe.is_some(),
            "bestiary Red Arcanist retains its fitted robe"
        );
        assert!(rig.short_garments.is_empty());
        let mut equipment = Equipment::default();
        equipment.select("apron").unwrap();
        let apron = look().composed(Wardrobe::Native, equipment).unwrap();
        let (_, rig, _, _) = rogue_tests::spawn_one(&apron);
        assert!(!rig.aprons.is_empty());
        assert!(rig.short_garments.is_empty());
    }

    #[test]
    fn separately_fitted_hip_decorations_are_not_left_on_a_rigid_baseline() {
        for model in [CreatureModel::Orc, CreatureModel::Troll] {
            let look =
                Look::preview_adjusted(model, 22, |v| v.woman = true).dressed(Wardrobe::Native);
            let (_, rig, _, _) = rogue_tests::spawn_one(&look);
            assert!(!rig.skirt_layers.is_empty());
            assert!(
                rig.short_garments.is_empty(),
                "{model:?} hip decorations still use their original retained surface"
            );
        }
    }

    #[test]
    fn instances_own_meshes_and_restore_immutable_rest_after_reset() {
        let look = look();
        let (mut world, mut rig, mut library, mut meshes) = rogue_tests::spawn_one(&look);
        assert!(!rig.short_garments.is_empty());
        let topology: Vec<Vec<usize>> = rig
            .short_garments
            .iter()
            .map(|panel| {
                meshes
                    .get(&panel.mesh)
                    .unwrap()
                    .indices()
                    .unwrap()
                    .iter()
                    .collect()
            })
            .collect();
        let material_ids = |rig: &Rig, world: &World| {
            rig.short_garments
                .iter()
                .flat_map(|panel| {
                    world
                        .get::<Children>(panel.entity)
                        .unwrap()
                        .iter()
                        .map(|child| {
                            world
                                .get::<MeshMaterial3d<StandardMaterial>>(child)
                                .unwrap()
                                .0
                                .id()
                        })
                })
                .collect::<Vec<_>>()
        };
        let materials = material_ids(&rig, &world);
        let before: std::collections::HashMap<_, _> = meshes
            .iter()
            .filter_map(|(id, mesh)| {
                let VertexAttributeValues::Float32x3(p) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)?
                else {
                    return None;
                };
                Some((id, p.clone()))
            })
            .collect();
        let parent = world.spawn(Transform::default()).id();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let other = spawn(
            &mut Commands::new(&mut queue, &world),
            &mut library,
            &mut meshes,
            parent,
            &look,
        );
        queue.apply(&mut world);
        assert_eq!(
            material_ids(&other, &world),
            materials,
            "materials remain shared between private geometry owners"
        );
        assert_eq!(other.short_garments.len(), rig.short_garments.len());
        for (a, b) in rig.short_garments.iter().zip(&other.short_garments) {
            assert_eq!(
                positions(&meshes, &a.mesh),
                positions(&meshes, &b.mesh),
                "ownership must not change emitted geometry"
            );
        }
        for panel in &other.short_garments {
            assert!(rig
                .short_garments
                .iter()
                .all(|first| first.mesh.id() != panel.mesh.id()));
        }
        let roots = [rig.rest[J::LegL as usize], rig.rest[J::LegR as usize]];
        let neutral = roots.map(Affine3A::from_translation);
        let moving = [
            Affine3A::from_rotation_translation(Quat::from_rotation_x(0.6), roots[0]),
            Affine3A::from_rotation_translation(Quat::from_rotation_x(-0.4), roots[1]),
        ];
        place(
            &mut rig.short_garments,
            roots,
            moving,
            Affine3A::IDENTITY,
            |_| Some(Affine3A::IDENTITY),
            &mut meshes,
        );
        assert!(rig
            .short_garments
            .iter()
            .any(|panel| positions(&meshes, &panel.mesh) != before[&panel.mesh.id()]));
        for (id, points) in &before {
            if !rig
                .short_garments
                .iter()
                .any(|panel| panel.mesh.id() == *id)
            {
                let VertexAttributeValues::Float32x3(after) = meshes
                    .get(*id)
                    .unwrap()
                    .attribute(Mesh::ATTRIBUTE_POSITION)
                    .unwrap()
                else {
                    panic!();
                };
                assert_eq!(after, points, "cached and unrelated meshes stay immutable");
            }
        }
        for (panel, expected) in rig.short_garments.iter().zip(&topology) {
            assert_eq!(
                &meshes
                    .get(&panel.mesh)
                    .unwrap()
                    .indices()
                    .unwrap()
                    .iter()
                    .collect::<Vec<_>>(),
                expected,
                "motion does not remove faces or introduce openings"
            );
        }
        rig.reset_presentation();
        assert!(rig.short_garments.iter().all(|panel| panel.last.is_none()));
        place(
            &mut rig.short_garments,
            roots,
            neutral,
            Affine3A::IDENTITY,
            |_| Some(Affine3A::IDENTITY),
            &mut meshes,
        );
        for panel in &rig.short_garments {
            assert_eq!(positions(&meshes, &panel.mesh), before[&panel.mesh.id()]);
            let Some(VertexAttributeValues::Float32x3(normals)) = meshes
                .get(&panel.mesh)
                .unwrap()
                .attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                panic!("normals");
            };
            assert_eq!(normals, &panel.normals);
        }
        // Scene preload remaps every private handle and invalidates its pose key.
        let old: Vec<_> = rig.short_garments.iter().map(|p| p.mesh.id()).collect();
        rig.map_render_references(
            |e| e,
            |h| {
                let mesh = meshes.get(h).unwrap().clone();
                meshes.add(mesh)
            },
        );
        for panel in &rig.short_garments {
            assert!(!old.contains(&panel.mesh.id()));
            assert!(panel.last.is_none());
        }
    }
}
