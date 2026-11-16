//! CPU-only, bounded construction workload for the shared performance harness.
use crate::{
    creatures::{self, Equipment, Look, Wardrobe},
    props::{PropLibrary, PropMaterials},
};
use bevy::prelude::*;
use serde_json::json;
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};
use tor_client_3d_model::CreatureModel;

fn spawn(look: &Look, library: &mut PropLibrary, meshes: &mut Assets<Mesh>) -> World {
    let mut world = World::new();
    let parent = world.spawn(Transform::default()).id();
    let mut queue = bevy::ecs::world::CommandQueue::default();
    let rig = creatures::spawn(
        &mut Commands::new(&mut queue, &world),
        library,
        meshes,
        parent,
        look,
    );
    queue.apply(&mut world);
    world.entity_mut(parent).insert(rig);
    world
}

fn payload(meshes: &Assets<Mesh>) -> usize {
    meshes
        .iter()
        .map(|(_, mesh)| {
            mesh.get_vertex_buffer_size() + mesh.get_index_buffer_bytes().map_or(0, <[u8]>::len)
        })
        .sum()
}

struct MotionSample {
    times: Vec<f64>,
    phases: [Vec<f64>; 4],
    hair_phases: [Vec<f64>; 4],
    changed: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum MotionCase {
    Jump,
    Walk,
    Strike,
}
impl MotionCase {
    fn name(self) -> &'static str {
        match self {
            Self::Jump => "jump",
            Self::Walk => "walk",
            Self::Strike => "strike",
        }
    }
    fn velocity(self) -> Vec3 {
        if self == Self::Walk {
            Vec3::Z * 1.5
        } else {
            Vec3::ZERO
        }
    }
    fn joint(self) -> creatures::J {
        if self == Self::Strike {
            creatures::J::HandR
        } else {
            creatures::J::Lift
        }
    }
}

fn motion(mut world: World, meshes: Assets<Mesh>, recipe: MotionCase) -> MotionSample {
    let root = world
        .query_filtered::<Entity, With<creatures::Rig>>()
        .single(&world)
        .unwrap();
    let lift = world
        .get::<creatures::Rig>(root)
        .unwrap()
        .joint(recipe.joint())
        .unwrap();
    world
        .entity_mut(root)
        .insert(creatures::Anim::new(7, Vec3::ZERO));
    world.insert_resource(meshes);
    world.insert_resource(Time::<()>::default());
    world.insert_resource(creatures::MotionProfile::default());
    let mut schedule = crate::review_script::animation_schedule();
    // Initialize systems and settle the starting pose outside the measured loop.
    for _ in 0..2 {
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(20));
        schedule.run(&mut world);
    }
    match recipe {
        MotionCase::Jump => world
            .get_mut::<creatures::Anim>(root)
            .unwrap()
            .act(creatures::Move::Jump),
        MotionCase::Strike => world
            .get_mut::<creatures::Anim>(root)
            .unwrap()
            .act(creatures::Move::Strike),
        MotionCase::Walk => {}
    }
    let mut times = Vec::with_capacity(30);
    let mut changed = 0;
    let mut phases: [Vec<f64>; 4] = std::array::from_fn(|_| Vec::with_capacity(30));
    let mut hair_phases: [Vec<f64>; 4] = std::array::from_fn(|_| Vec::with_capacity(30));
    for _ in 0..30 {
        let before = *world.get::<Transform>(lift).unwrap();
        // Match review playback: move the actor before the animation schedule.
        if recipe == MotionCase::Walk {
            world.get_mut::<Transform>(root).unwrap().translation += recipe.velocity() / 50.0;
        }
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(20));
        let start = Instant::now();
        schedule.run(&mut world);
        times.push(start.elapsed().as_secs_f64() * 1000.0);
        let profile = world.resource::<creatures::MotionProfile>();
        assert_eq!(profile.rigs, 1, "the workload must profile its actual rig");
        for (values, elapsed) in phases.iter_mut().zip(profile.phases) {
            values.push(elapsed.as_secs_f64() * 1000.0);
        }
        for (values, elapsed) in hair_phases.iter_mut().zip(profile.hair_phases) {
            values.push(elapsed.as_secs_f64() * 1000.0);
        }
        let after = world.get::<Transform>(lift).unwrap();
        assert!(
            after.translation.is_finite() && after.rotation.is_finite() && after.scale.is_finite()
        );
        changed += usize::from(*after != before);
    }
    MotionSample {
        times,
        phases,
        hair_phases,
        changed,
    }
}

/// Emit versioned CPU construction and fixed-step motion samples, without a GPU.
/// Setup and mesh accounting are outside the spawn timers. Both worlds stay alive
/// through measurement: cached geometry is shared, mutable hair/cloth is not.
#[derive(Clone, Copy)]
struct ModelCase {
    name: &'static str,
    model: CreatureModel,
    wardrobe: Wardrobe,
    item: &'static str,
    woman: bool,
    build: Option<u8>,
    motion: MotionCase,
}
impl ModelCase {
    fn look(self) -> Result<Look, String> {
        let mut equipment = Equipment::default();
        equipment.select(self.item)?;
        Look::preview_adjusted(self.model, 7, |body| {
            body.woman = self.woman;
            if let Some(build) = self.build {
                body.build = build;
            }
        })
        .composed(self.wardrobe, equipment)
    }
}
const LEGACY: [ModelCase; 3] = [
    ModelCase {
        name: "human-sylvan-plate",
        model: CreatureModel::Human,
        wardrobe: Wardrobe::SylvanDuelist,
        item: "breastplate",
        woman: true,
        build: None,
        motion: MotionCase::Jump,
    },
    ModelCase {
        name: "nymph-princess-cloak",
        model: CreatureModel::Nymph,
        wardrobe: Wardrobe::RoyalPrincess,
        item: "cloak",
        woman: true,
        build: None,
        motion: MotionCase::Jump,
    },
    ModelCase {
        name: "mage-native-boots",
        model: CreatureModel::Mage,
        wardrobe: Wardrobe::Native,
        item: "boots",
        woman: true,
        build: None,
        motion: MotionCase::Jump,
    },
];
const TARGETED: [ModelCase; 3] = [
    ModelCase {
        name: "nymph-slender-walk",
        build: Some(0),
        motion: MotionCase::Walk,
        ..LEGACY[1]
    },
    ModelCase {
        name: "nymph-sturdy-strike",
        build: Some(2),
        motion: MotionCase::Strike,
        ..LEGACY[1]
    },
    ModelCase {
        name: "mage-male-walk",
        woman: false,
        build: Some(1),
        motion: MotionCase::Walk,
        ..LEGACY[2]
    },
];

const EQUIPMENT: [ModelCase; 3] = [
    ModelCase {
        name: "human-plumed-helmet",
        model: CreatureModel::Human,
        wardrobe: Wardrobe::Native,
        item: "plumed-helmet",
        woman: false,
        build: Some(1),
        motion: MotionCase::Walk,
    },
    ModelCase {
        name: "human-padded-gloves",
        model: CreatureModel::Human,
        wardrobe: Wardrobe::Native,
        item: "padded-gloves",
        woman: true,
        build: Some(0),
        motion: MotionCase::Strike,
    },
    ModelCase {
        name: "human-hiking-boots",
        model: CreatureModel::Human,
        wardrobe: Wardrobe::Native,
        item: "hiking-boots",
        woman: false,
        build: Some(2),
        motion: MotionCase::Walk,
    },
];
#[derive(Clone, Copy)]
enum Suite {
    Legacy,
    Targeted,
    Equipment,
}

/// Separate bounded equipment diagnostic, not a replacement for v4 or v5.
pub fn run_equipment() -> Result<(), Box<dyn std::error::Error>> {
    run_cases(&EQUIPMENT, Suite::Equipment)
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    run_cases(&LEGACY, Suite::Legacy)
}

/// Separate bounded diagnostic suite. Does not replace the version 4 workload.
pub fn run_targeted() -> Result<(), Box<dyn std::error::Error>> {
    run_cases(&TARGETED, Suite::Targeted)
}

fn run_cases(cases: &[ModelCase], suite: Suite) -> Result<(), Box<dyn std::error::Error>> {
    let mut output = io::BufWriter::new(io::stdout().lock());
    for recipe in cases {
        let case = recipe.name;
        let look = recipe.look()?;
        for sample in 0..3 {
            let mut materials = Assets::<StandardMaterial>::default();
            let mut library = PropLibrary::new(PropMaterials::new(&mut materials));
            let mut meshes = Assets::<Mesh>::default();
            let start = Instant::now();
            let cold = spawn(&look, &mut library, &mut meshes);
            let cold_spawn_ms = start.elapsed().as_secs_f64() * 1000.0;
            let cold_meshes = meshes.len();
            let cold_mesh_bytes = payload(&meshes);
            let start = Instant::now();
            let warm = spawn(&look, &mut library, &mut meshes);
            let warm_spawn_ms = start.elapsed().as_secs_f64() * 1000.0;
            let warm_added_meshes = meshes.len() - cold_meshes;
            let warm_added_mesh_bytes = payload(&meshes) - cold_mesh_bytes;
            let warm_entities = warm.entities().len();
            let motion = motion(warm, meshes, recipe.motion);
            assert!(motion.changed > 0, "animated joint did not change: {case}");
            let mut row = json!({"version":4,"case":case,"sample":sample,"seed":7,
                "cold_spawn_ms":cold_spawn_ms,"warm_spawn_ms":warm_spawn_ms,
                "cold_meshes":cold_meshes,"warm_added_meshes":warm_added_meshes,
                "cold_mesh_bytes":cold_mesh_bytes,"warm_added_mesh_bytes":warm_added_mesh_bytes,
                "cold_entities":cold.entities().len(),"warm_entities":warm_entities,
                "pose_update_ms":motion.times,"motion_frames":30,"hz":50,"changed_pose_frames":motion.changed,
                "pose_joints_ms":motion.phases[0],"robe_fit_ms":motion.phases[1],
                "drape_update_ms":motion.phases[2],"hair_fit_ms":motion.phases[3],
                "hair_controls_ms":motion.hair_phases[0],"hair_deform_ms":motion.hair_phases[1],
                "hair_constraints_ms":motion.hair_phases[2],"hair_mesh_ms":motion.hair_phases[3]});
            if !matches!(suite, Suite::Legacy) {
                row["version"] = json!(5);
                row["suite"] = json!("targeted-contact-v1");
                row["woman"] = json!(recipe.woman);
                row["build"] = json!(recipe.build.unwrap());
                row["motion"] = json!(recipe.motion.name());
                row["velocity"] = json!(recipe.motion.velocity().to_array());
            }
            if matches!(suite, Suite::Equipment) {
                row["version"] = json!(6);
                row["suite"] = json!("equipment-construction-v1");
                row["equipment"] = json!(recipe.item);
                row["model"] = json!("human");
                row["wardrobe"] = json!("native");
            }
            writeln!(output, "{row}")?;
        }
    }
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_recipes_preserve_legacy_and_target_body_motion_variation() {
        assert_eq!(
            LEGACY.map(|x| x.name),
            [
                "human-sylvan-plate",
                "nymph-princess-cloak",
                "mage-native-boots"
            ]
        );
        assert!(LEGACY
            .iter()
            .all(|x| x.woman && x.build.is_none() && x.motion == MotionCase::Jump));
        assert_eq!(TARGETED.map(|x| x.build), [Some(0), Some(2), Some(1)]);
        assert_eq!(
            TARGETED.map(|x| x.motion),
            [MotionCase::Walk, MotionCase::Strike, MotionCase::Walk]
        );
        assert!(!TARGETED[2].woman);
        assert_eq!(
            EQUIPMENT.map(|x| x.item),
            ["plumed-helmet", "padded-gloves", "hiking-boots"]
        );
        assert_eq!(EQUIPMENT.map(|x| x.build), [Some(1), Some(0), Some(2)]);
        assert_eq!(EQUIPMENT.map(|x| x.woman), [false, true, false]);
        assert_eq!(
            EQUIPMENT.map(|x| x.motion),
            [MotionCase::Walk, MotionCase::Strike, MotionCase::Walk]
        );
        assert!(EQUIPMENT
            .iter()
            .all(|x| x.model == CreatureModel::Human && x.wardrobe == Wardrobe::Native));
        for case in LEGACY.iter().chain(TARGETED.iter()).chain(EQUIPMENT.iter()) {
            assert!(case.look().is_ok());
        }
    }
}
