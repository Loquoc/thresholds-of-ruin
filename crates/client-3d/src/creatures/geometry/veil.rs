//! A bordered veil is an independent head attachment, not part of a crown.
//! One shared sampled sheet supplies cloth, scalloped edging and its ornaments.
use super::*;
use crate::creatures::outfit::Veil;
use bevy::render::mesh::{
    skinning::{SkinnedMesh, SkinnedMeshInverseBindposes},
    VertexAttributeValues,
};

const ROWS: usize = 24;
const COLS: usize = 32;

/// The attachment follows the posed head. The lower skin ordinarily
/// stays in the torso frame and uses its own train bone when the wearer falls.
/// Use one continuous skin for the sheet and all its edging rather than
/// splitting the translucent fabric or deforming cached vertices per wearer.
pub(super) fn spawn(kit: &mut Kit, head: Entity, torso: Entity, body: HumanoidBody, recipe: Veil) {
    let roots = attachment_roots(body.head);
    let top = roots
        .iter()
        .map(|root| root.y)
        .fold(f32::INFINITY, f32::min)
        - 0.005;
    let model = kit.part_prepared(
        "veil",
        |p| emit(p, &roots, body.shoulders, recipe),
        |mesh| bind_drape(mesh, top),
    );
    let scale = storybook_part(Plan::Biped, J::Torso);
    let frame = kit
        .commands
        .spawn((Transform::from_scale(scale), Visibility::Inherited))
        .id();
    kit.commands.entity(torso).add_child(frame);
    let train = kit
        .commands
        .spawn((Transform::IDENTITY, Visibility::Inherited))
        .id();
    kit.commands.entity(frame).add_child(train);
    kit.rig.veil = Some(GroundDrape {
        train,
        pivot: roots[COLS / 2],
        samples: sheet(&roots, body.shoulders, recipe)
            .into_iter()
            .flatten()
            .filter_map(|p| {
                let [torso, head, train, _] = drape_weights(p.y, top);
                (train >= 0.70).then_some((p, [torso, head, train]))
            })
            .collect(),
    });
    let layers: Vec<_> = model
        .0
        .into_iter()
        .zip(model.2)
        .map(|((mesh, material), bounds)| {
            let mut entity =
                kit.commands
                    .spawn((Mesh3d(mesh), MeshMaterial3d(material), Transform::IDENTITY));
            if let Some(bounds) = bounds {
                entity.insert(bounds);
            }
            let e = entity.id();
            kit.commands.entity(frame).add_child(e);
            e
        })
        .collect();
    // The mesh is authored in torso presentation units; the unscaled head
    // joint needs the corresponding rest offset rebased into its own frame.
    let inverse = vec![
        Mat4::IDENTITY,
        Mat4::from_translation(-body.head) * Mat4::from_scale(scale),
        Mat4::IDENTITY,
    ];
    kit.commands.queue(move |world: &mut World| {
        let inverse_bindposes = world
            .get_resource_or_init::<Assets<SkinnedMeshInverseBindposes>>()
            .add(SkinnedMeshInverseBindposes::from(inverse));
        for layer in layers {
            if let Ok(mut entity) = world.get_entity_mut(layer) {
                entity.insert(SkinnedMesh {
                    inverse_bindposes: inverse_bindposes.clone(),
                    joints: vec![frame, head, train],
                });
            }
        }
    });
}

pub(in crate::creatures) struct GroundDrape {
    pub(in crate::creatures) train: Entity,
    pivot: Vec3,
    // Mesh-space points of the free sheet, with the same skin weights as
    // the renderer. Cover the actual scalloped shape without touching its
    // shared buffers when wearing, falling, or loading a second bride.
    samples: Box<[(Vec3, [f32; 3])]>,
}

impl GroundDrape {
    pub(in crate::creatures) fn pose(
        &self,
        torso: Affine3A,
        head: Affine3A,
        fall: f32,
    ) -> Transform {
        if fall <= 0.0 {
            return Transform::IDENTITY;
        }
        let folded = ease((fall - 0.48) / 0.52);
        // The free sheet trails 0.20 units back per unit of length. Align
        // that average plane with actor-local ground at the settled endpoint;
        // a vertical shift alone would leave most of the rim hovering.
        let normal = Vec3::new(0.0, -0.20, 1.0).normalize();
        let up = Vec3::new(
            torso.matrix3.x_axis.y,
            torso.matrix3.y_axis.y,
            torso.matrix3.z_axis.y,
        )
        .normalize_or(Vec3::Z);
        let flat = Quat::from_rotation_arc(normal, up);
        let rotation = Quat::IDENTITY.slerp(flat, folded);
        let hinge = self.pivot - rotation * self.pivot;
        // Loosen the veil slightly to the side as the body tips back. It
        // lands beside the dress rather than following the back underneath it.
        let lateral = torso
            .transform_vector3(Vec3::X)
            .with_y(0.0)
            .normalize_or_zero();
        let terrain_lift = self.samples.iter().fold(0.0f32, |needed, &(p, [a, b, c])| {
            let y = torso.transform_point3(p).y * a
                + head.transform_point3(p).y * b
                + torso.transform_point3(rotation * p + hinge).y * c;
            // A small allowance covers raised edging and flower settings.
            needed.max((0.012 - y) / c)
        });
        let world_delta = Vec3::Y * terrain_lift * ease(fall / 0.40) + lateral * (0.19 * folded);
        Transform::from_translation(hinge + torso.inverse().transform_vector3(world_delta))
            .with_rotation(rotation)
    }
}

fn drape_weights(y: f32, held_above: f32) -> [f32; 4] {
    const BLEND_DEPTH: f32 = 0.42;
    let head = ease((y - held_above + BLEND_DEPTH) / BLEND_DEPTH);
    let train = (1.0 - head) * ease((held_above - y) / BLEND_DEPTH);
    [1.0 - head - train, head, train, 0.0]
}

fn bind_drape(mesh: &mut Mesh, held_above: f32) {
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return;
    };
    let weights: Vec<_> = positions
        .iter()
        .map(|&[_, y, _]| drape_weights(y, held_above))
        .collect();
    let indices = vec![[0u16, 1, 2, 0]; weights.len()];
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_JOINT_INDEX,
        VertexAttributeValues::Uint16x4(indices),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, weights);
}

fn emit(p: &mut Parts, roots: &[Vec3], shoulders: f32, recipe: Veil) {
    let grid = sheet(roots, shoulders, recipe);
    for row in 0..ROWS {
        for column in 0..COLS {
            // Sheer material is already double-sided. No coincident reverse
            // triangles or thickness faces: they make translucent seams dark.
            let q = [
                grid[row][column],
                grid[row][column + 1],
                grid[row + 1][column + 1],
                grid[row + 1][column],
            ];
            p.sheer.tri([q[0], q[1], q[2]], recipe.color);
            p.sheer.tri([q[0], q[2], q[3]], recipe.color);
        }
    }
    let mut border = Vec::with_capacity(ROWS * 2 + COLS + 1);
    border.extend((0..=ROWS).map(|row| grid[row][0]));
    border.extend((1..=COLS).map(|column| grid[ROWS][column]));
    border.extend((0..ROWS).rev().map(|row| grid[row][COLS]));
    tube(
        &mut p.metal,
        &border,
        &vec![(0.0017, 0.0017); border.len()],
        recipe.trim,
    );
    // Sparse flower settings follow the very same border, rather than a second
    // independent curve that could float away when length/spread changes.
    for index in (4..border.len() - 4).step_by(8) {
        let at = border[index];
        let tangent = (border[index + 1] - border[index - 1]).normalize_or(Vec3::Y);
        let across = tangent.cross(Vec3::Z).normalize_or(Vec3::X);
        for petal in 0..5 {
            let angle = petal as f32 * TAU / 5.0;
            let direction = tangent * angle.cos() + across * angle.sin();
            p.metal.blob(
                at + direction * 0.006,
                Vec3::splat(0.0038),
                1,
                0.0,
                recipe.trim,
            );
        }
        p.satin.blob(
            at - Vec3::Z * 0.002,
            Vec3::new(0.0035, 0.0045, 0.0025),
            1,
            0.0,
            recipe.gem,
        );
    }
}

/// Neutral head attachment frame, deliberately independent of rendered crown
/// geometry. The ordinary scalp envelope includes a 35 mm hair allowance;
/// ornate hairstyles still require visual review rather than an expensive
/// second head mesh construction for every torso.
fn attachment_roots(head_origin: Vec3) -> Vec<Vec3> {
    let head_scale = storybook_part(Plan::Biped, J::Head);
    let torso_scale = storybook_part(Plan::Biped, J::Torso);
    (0..=COLS)
        .map(|column| {
            let u = column as f32 / COLS as f32 * 2.0 - 1.0;
            let angle = PI - u * PI * 0.42;
            let local = Vec3::new(
                angle.sin() * 0.145,
                0.247 - 0.006 * angle.cos(),
                -0.012 + angle.cos() * 0.155,
            );
            (local * head_scale + head_origin) / torso_scale
        })
        .collect()
}

fn sheet(roots: &[Vec3], shoulders: f32, recipe: Veil) -> Vec<Vec<Vec3>> {
    (0..=ROWS)
        .map(|row| {
            let t = row as f32 / ROWS as f32;
            (0..=COLS)
                .map(|column| {
                    let u = column as f32 / COLS as f32 * 2.0 - 1.0;
                    let root = roots[column];
                    let release = (t * 5.0).min(1.0);
                    let flare = shoulders * recipe.spread * t.powf(0.8);
                    let folds = 0.012 * (u * PI * 6.0).cos() * t;
                    let scallop = 0.007 * (t * PI * 8.0).sin().powi(2) * u.abs().powi(8);
                    Vec3::new(
                        root.x + u * (flare + scallop),
                        root.y - recipe.length * t + 0.018 * (u * PI * 8.0).cos() * t.powi(12),
                        root.z - release * 0.10 - t * recipe.length * 0.20 + folds,
                    )
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn recipe() -> Veil {
        Veil {
            color: Vec3::ONE,
            trim: GILT,
            gem: Vec3::Z,
            length: 0.85,
            spread: 1.0,
        }
    }
    fn roots() -> Vec<Vec3> {
        (0..=COLS)
            .map(|k| Vec3::new((k as f32 / COLS as f32 - 0.5) * 0.2, 0.25, -0.12))
            .collect()
    }
    #[test]
    fn bridal_veil_follows_the_head_at_its_root_but_not_at_the_hem() {
        use crate::creatures::{rogue_tests::spawn_one, CreatureModel, Equipment, Look, Wardrobe};
        let look = Look::preview_adjusted(CreatureModel::Human, 76, |v| {
            v.woman = true;
            v.hair = 4;
        })
        .composed(Wardrobe::RoyalBride, Equipment::default())
        .unwrap();
        let (mut world, rig, _, meshes) = spawn_one(&look);
        let head = rig.joint(J::Head).unwrap();
        let torso = rig.joint(J::Torso).unwrap();
        let body = rig.humanoid.unwrap();
        let roots = attachment_roots(body.head);
        let mut query = world.query::<(&Mesh3d, &SkinnedMesh, &ChildOf)>();
        let layers: Vec<_> = query
            .iter(&world)
            .filter(|(_, skin, _)| skin.joints.contains(&head))
            .collect();
        assert_eq!(
            layers.len(),
            3,
            "cloth, trim and gems need the same head binding"
        );

        let head_turn = Quat::from_rotation_y(0.63) * Quat::from_rotation_x(-0.22);
        let torso_scale = storybook_part(Plan::Biped, J::Torso);
        let mut root_coverage = vec![false; roots.len()];
        for (handle, skin, parent) in layers {
            let mesh = meshes.get(&handle.0).unwrap();
            let bindings = world.resource::<Assets<SkinnedMeshInverseBindposes>>();
            let bind = bindings.get(&skin.inverse_bindposes).unwrap();
            assert_eq!(skin.joints.len(), 3);
            let train = skin.joints[2];
            assert_eq!(
                world.get::<ChildOf>(train).unwrap().parent(),
                skin.joints[0]
            );
            assert_eq!(skin.joints[0], parent.parent());
            assert_eq!(skin.joints[1], head);
            assert_eq!(
                world.get::<ChildOf>(parent.parent()).unwrap().parent(),
                torso
            );
            let torso_frame = world
                .get::<Transform>(skin.joints[0])
                .unwrap()
                .compute_matrix();
            let head_frame = Mat4::from_rotation_translation(head_turn, body.head);
            let Some(VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("veil position buffer")
            };
            let Some(VertexAttributeValues::Float32x4(weights)) =
                mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT)
            else {
                panic!("veil weight buffer")
            };
            let Some(VertexAttributeValues::Uint16x4(indices)) =
                mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX)
            else {
                panic!("veil frame indices")
            };
            let mut saw_low_drape = false;
            let mut saw_transition = false;
            for ((&point, &[torso_w, head_w, train_w, v]), &index) in
                positions.iter().zip(weights).zip(indices)
            {
                let point = Vec3::from(point);
                assert_eq!(index, [0, 1, 2, 0]);
                assert!(head_w >= 0.0 && torso_w >= 0.0 && train_w >= 0.0);
                assert!((torso_w + head_w + train_w - 1.0).abs() < 1e-6);
                assert_eq!(v, 0.0);
                let posed = (torso_frame * bind[0]).transform_point3(point) * torso_w
                    + (head_frame * bind[1]).transform_point3(point) * head_w
                    + (torso_frame * bind[2]).transform_point3(point) * train_w;
                let rest = torso_scale * point;
                if point.y < roots[0].y - 0.75 {
                    saw_low_drape = true;
                    assert!(
                        (posed - rest).length() < 1e-5,
                        "head must not pull the train"
                    );
                }
                if (0.05..0.95).contains(&head_w) {
                    saw_transition = true;
                }
                for (i, &root) in roots.iter().enumerate() {
                    if (root - point).length_squared() < 1e-10 {
                        root_coverage[i] = true;
                        let pinned = body.head + head_turn * (rest - body.head);
                        assert!((pinned - posed).length() < 1e-5, "root {i} slipped");
                    }
                }
            }
            assert!(
                saw_low_drape && saw_transition,
                "each layer follows the same drape"
            );
        }
        assert!(
            root_coverage.into_iter().all(|seen| seen),
            "cover the real sheet attachment row"
        );
    }

    #[test]
    fn fallen_veil_train_rests_above_the_floor() {
        use crate::creatures::{
            animation::DeathMotion, rogue_tests::animate_look, CreatureModel, Equipment, Look,
            Wardrobe,
        };
        let look = Look::preview_adjusted(CreatureModel::Human, 42, |v| {
            v.woman = true;
            v.hair = 0;
        })
        .composed(Wardrobe::RoyalBride, Equipment::default())
        .unwrap();
        let (mut app, root) = animate_look(look, 2, |_| Vec3::ZERO);
        let rig = app.world().get::<Rig>(root).unwrap();
        let head = rig.joint(J::Head).unwrap();
        let mut query = app.world_mut().query::<(&Mesh3d, &SkinnedMesh)>();
        let frame = |world: &World, mut entity: Entity| {
            let mut matrix = Mat4::IDENTITY;
            while entity != root {
                matrix = world.get::<Transform>(entity).unwrap().compute_matrix() * matrix;
                entity = world.get::<ChildOf>(entity).unwrap().parent();
            }
            matrix
        };
        for motion in [
            DeathMotion::Collapse,
            DeathMotion::Stagger,
            DeathMotion::Dramatic,
        ] {
            app.world_mut()
                .get_mut::<Anim>(root)
                .unwrap()
                .preview_death(motion);
            for phase in [0.75, 0.86, 0.94, 1.0] {
                app.world_mut().get_mut::<Anim>(root).unwrap().dead = Some(phase);
                app.update();
                let world = app.world();
                let meshes = world.resource::<Assets<Mesh>>();
                let bindings = world.resource::<Assets<SkinnedMeshInverseBindposes>>();
                let mut layer_count = 0;
                for (handle, skin) in query
                    .iter(world)
                    .filter(|(_, skin)| skin.joints.contains(&head))
                {
                    layer_count += 1;
                    let mesh = meshes.get(&handle.0).unwrap();
                    let bind = bindings.get(&skin.inverse_bindposes).unwrap();
                    let joint_frames: Vec<_> = skin
                        .joints
                        .iter()
                        .zip(bind.iter())
                        .map(|(&e, &b)| frame(world, e) * b)
                        .collect();
                    let Some(VertexAttributeValues::Float32x3(positions)) =
                        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                    else {
                        panic!("veil positions");
                    };
                    let Some(VertexAttributeValues::Float32x4(weights)) =
                        mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT)
                    else {
                        panic!("veil weights");
                    };
                    let Some(VertexAttributeValues::Uint16x4(indices)) =
                        mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX)
                    else {
                        panic!("veil indices");
                    };
                    let mut floor: f32 = f32::INFINITY;
                    let mut low = 0;
                    let mut train = 0;
                    for ((&p, &w), &i) in positions.iter().zip(weights).zip(indices) {
                        if p[1] > -0.56 {
                            continue;
                        }
                        let at = (0..4)
                            .filter(|&k| w[k] > 0.0)
                            .map(|k| joint_frames[i[k] as usize].transform_point3(p.into()) * w[k])
                            .sum::<Vec3>();
                        floor = floor.min(at.y);
                        low += usize::from(at.y < 0.085);
                        train += 1;
                    }
                    assert!(train > 16, "sample the real trailing mesh, including trim");
                    assert!(
                        floor >= -0.012,
                        "{motion:?} phase {phase}: veil train penetrates ground at {floor:.3}"
                    );
                    if phase == 1.0 {
                        assert!(
                            low * 10 >= train,
                            "{motion:?} phase {phase}: {low} of {train} trailing vertices near the floor (min {floor:.3})"
                        );
                    }
                }
                assert_eq!(layer_count, 3);
            }
        }
    }

    #[test]
    fn veil_roots_translate_through_torso_presentation_frame() {
        let a = attachment_roots(Vec3::ZERO);
        let offset = Vec3::new(0.0, 0.4, 0.02);
        let b = attachment_roots(offset);
        let expected = offset / storybook_part(Plan::Biped, J::Torso);
        for (a, b) in a.iter().zip(b) {
            assert!((b - *a - expected).length() < 1e-6);
            assert!(b.is_finite());
        }
    }
    #[test]
    fn veil_preserves_attachment_row_and_parameterized_drape() {
        let roots = roots();
        for length in [0.85, 1.55, 1.80] {
            for shoulders in [0.85, 1.0, 1.12] {
                let grid = sheet(&roots, shoulders, Veil { length, ..recipe() });
                assert_eq!(grid[0], roots);
                assert!(grid.iter().flatten().all(|v| v.is_finite()));
                assert!(grid[ROWS][COLS].x > roots[COLS].x + shoulders * 0.9);
                assert!((grid[ROWS][COLS / 2].y - (0.25 - length)).abs() <= 0.019);
                assert!(grid[ROWS][COLS / 2].z < -0.20);
            }
        }
    }
    #[test]
    fn veil_cells_are_non_degenerate_single_surface() {
        let grid = sheet(&roots(), 0.2, recipe());
        for row in 0..ROWS {
            for col in 0..COLS {
                let a = grid[row][col];
                let b = grid[row][col + 1];
                let c = grid[row + 1][col];
                assert!((b - a).cross(c - a).length() > 1e-7);
            }
        }
    }
}
