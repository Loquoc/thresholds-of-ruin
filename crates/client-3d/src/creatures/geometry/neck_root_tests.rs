//! Neck/torso visibility at the real joint boundary, after child transforms.
use super::*;
use bevy::render::mesh::VertexAttributeValues;

fn frame(world: &World, entity: Entity) -> Affine3A {
    let local = world
        .get::<Transform>(entity)
        .map_or(Affine3A::IDENTITY, Transform::compute_affine);
    world
        .get::<ChildOf>(entity)
        .map_or(local, |parent| frame(world, parent.parent()) * local)
}

fn joint_triangles(
    world: &World,
    rig: &Rig,
    meshes: &Assets<Mesh>,
    joint: J,
    reference: Affine3A,
) -> Vec<[Vec3; 3]> {
    let entity = rig.joint(joint).expect("rendered biped joint");
    test_support::joint_meshes(world, rig, meshes, entity)
        .into_iter()
        .flat_map(|rendered| {
            let Some(VertexAttributeValues::Float32x3(points)) =
                rendered.mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("positions");
            };
            let indices: Vec<_> = rendered
                .mesh
                .indices()
                .map(|i| i.iter().collect())
                .unwrap_or_else(|| (0..points.len()).collect());
            let transform = reference * frame(world, entity) * rendered.transform;
            indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|t| {
                    std::array::from_fn(|i| {
                        transform.transform_point3(Vec3::from_array(points[t[i]]))
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn front(triangles: &[[Vec3; 3]], xy: Vec2) -> Option<f32> {
    triangles
        .iter()
        .filter_map(|[a, b, c]| {
            let v = b.truncate() - a.truncate();
            let w = c.truncate() - a.truncate();
            let q = xy - a.truncate();
            let det = v.perp_dot(w);
            if det.abs() < 1e-10 {
                return None;
            }
            let u = q.perp_dot(w) / det;
            let t = v.perp_dot(q) / det;
            (u >= -1e-6 && t >= -1e-6 && u + t <= 1.000001)
                .then_some(a.z + u * (b.z - a.z) + t * (c.z - a.z))
        })
        .reduce(f32::max)
}

#[test]
fn native_closed_bib_occludes_actual_hobgoblin_head_neck_root() {
    check_native_closed_bib(2, &NATIVE_NEUTRAL);
}

#[test]
fn walking_closed_bib_occludes_actual_hobgoblin_head_neck_root() {
    check_native_closed_bib(18, &NATIVE_WALK);
}

fn check_native_closed_bib(capture_tick: u32, expected_frames: &[[[[u32; 16]; 2]; 3]; 2]) {
    let mut failures = Vec::new();
    for woman in [false, true] {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Hobgoblin,
                17,
                Some(Variant {
                    woman,
                    build,
                    bust_lean: 1,
                    ..default()
                }),
            );
            let look = look
                .composed(Wardrobe::Native, Equipment::default())
                .unwrap();
            let (mut world, rig, _, meshes) = super::super::rogue_tests::spawn_one(&look);
            let root = world
                .get::<ChildOf>(rig.joint(J::Lift).unwrap())
                .unwrap()
                .parent();
            let mut anim = Anim::new(17, Vec3::ZERO);
            anim.yaw = 0.0;
            anim.target_yaw = Some(0.0);
            world.entity_mut(root).insert((rig, anim));
            world.insert_resource(meshes);
            world.insert_resource(Time::<()>::default());
            // Review neutral is tick 2 and walk is tick 18 at 50 Hz, not
            // bind pose. Use real pose + soften, including owned skin
            // child frames, rather than rotating a proxy neck by hand.
            let mut advance = crate::review_script::animation_schedule();
            for tick in 0..=capture_tick {
                world
                    .resource_mut::<Time>()
                    .advance_by(std::time::Duration::from_millis(if tick == 0 {
                        0
                    } else {
                        20
                    }));
                // The script applies velocity +.2Z after animation on tick 3;
                // integration begins on tick 4, before that tick's animation.
                if tick > 3 {
                    world.get_mut::<Transform>(root).unwrap().translation.z += 0.2 / 50.0;
                }
                advance.run(&mut world);
            }
            let rig = world.get::<Rig>(root).unwrap();
            let meshes = world.resource::<Assets<Mesh>>();
            // Retained native capture matrices independently qualify this
            // fixture before interpreting any skin/cloth depth result.
            for (joint, expected) in [J::Torso, J::Head]
                .into_iter()
                .zip(expected_frames[usize::from(woman)][build as usize])
            {
                let actual = Mat4::from(frame(&world, rig.joint(joint).unwrap())).to_cols_array();
                assert!(
                    actual
                        .iter()
                        .zip(expected)
                        .all(|(a, e)| (*a - f32::from_bits(e)).abs() < 1e-5),
                    "native tick{capture_tick} parity woman={woman} build={build} joint={joint:?}: {actual:?}"
                );
            }
            // Geometry uses differently scaled mesh children. Bring both joints
            // into the torso joint frame (the head joint's parent), not either
            // mesh child's scaled frame. The torso itself is .06 above hips.
            let torso_entity = rig.joint(J::Torso).unwrap();
            let reference = frame(&world, torso_entity).inverse();
            let head = joint_triangles(&world, rig, meshes, J::Head, reference);
            let torso = joint_triangles(&world, rig, meshes, J::Torso, reference);
            let mut witnesses = 0;
            let mut breaches = 0;
            let mut worst = (0.0_f32, Vec2::ZERO);
            // Interior of the authored closed bib, below the exposed neck.
            // An independent raster samples actual triangles, never an SDF.
            for row in 0..=20 {
                for col in -20..=20 {
                    let xy = Vec2::new(col as f32 * 0.002, 0.462 + row as f32 * 0.002);
                    let Some(skin) = front(&head, xy) else {
                        continue;
                    };
                    witnesses += 1;
                    let cloth = front(&torso, xy).expect("closed bib has no hole at neck root");
                    let gap = skin - cloth;
                    if gap > 0.00001 {
                        breaches += 1;
                        if gap > worst.0 {
                            worst = (gap, xy);
                        }
                    }
                }
            }
            assert!(
                witnesses >= 100,
                "woman={woman} build={build}: actual root must remain present, {witnesses} rays"
            );
            if breaches > 0 {
                failures.push(format!("woman={woman} build={build}: {breaches}/{witnesses} skin breakthroughs, maximum {} at {:?}", worst.0, worst.1));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "actual head skin must be behind the closed torso bib:\n{}",
        failures.join("\n")
    );
}

// Torso and Head column-major global matrices from independently rendered
// seed17 native neutral tick2 captures, indexed by sex then build.
const NATIVE_NEUTRAL: [[[[u32; 16]; 2]; 3]; 2] = [
    [
        [
            [
                1060016003, 3151673062, 0, 0, 1004051597, 1059906009, 1036071472, 0, 3128036688,
                3183554513, 1059906552, 0, 3156645164, 1060129477, 0, 1065353216,
            ],
            [
                1059998042, 1015214721, 1024504177, 0, 3163031488, 1060011504, 1010694448, 0,
                3171910682, 3159081592, 1059999534, 0, 3151578938, 1065772124, 1028569155,
                1065353216,
            ],
        ],
        [
            [
                1060369818, 3152116372, 0, 0, 1004490645, 1060256422, 1036463058, 0, 3128527300,
                3183946081, 1060256981, 0, 3156982808, 1060486801, 0, 1065353216,
            ],
            [
                1060351301, 1015480138, 1024797454, 0, 3163307206, 1060365181, 1011079496, 0,
                3172201572, 3159494576, 1060352839, 0, 3152019336, 1066044522, 1028988152,
                1065353216,
            ],
        ],
        [
            [
                1060723631, 3152559678, 0, 0, 1004929689, 1060606833, 1036854644, 0, 3128984346,
                3184337647, 1060607410, 0, 3157320451, 1060844125, 0, 1065353216,
            ],
            [
                1060704560, 1015745553, 1025090730, 0, 3163582923, 1060718855, 1011464544, 0,
                3172492461, 3159907576, 1060706144, 0, 3152459733, 1066316918, 1029407149,
                1065353216,
            ],
        ],
    ],
    [
        [
            [
                1059616006, 943497212, 995010656, 0, 3115987678, 1059562518, 1032103685, 0,
                3142409519, 3179587461, 1059562393, 0, 3152493664, 1059721842, 991635617,
                1065353216,
            ],
            [
                1059601019, 3155872007, 1023906359, 0, 1007798250, 1059613545, 1010189280, 0,
                3171430798, 3157176032, 1059600578, 0, 3152901692, 1065476423, 1024959153,
                1065353216,
            ],
        ],
        [
            [
                1059957448, 943881212, 995429528, 0, 3116365198, 1059902307, 1032372556, 0,
                3142825766, 3179856336, 1059902177, 0, 3152962352, 1060066558, 991950106,
                1065353216,
            ],
            [
                1059941998, 3156185732, 1024181145, 0, 1008093727, 1059954912, 1010558712, 0,
                3171706845, 3157530092, 1059941544, 0, 3153383004, 1065739674, 1025266501,
                1065353216,
            ],
        ],
        [
            [
                1060298891, 944263164, 995848399, 0, 3116742592, 1060242096, 1032641427, 0,
                3143242015, 3180125211, 1060241962, 0, 3153431041, 1060411275, 992264595,
                1065353216,
            ],
            [
                1060282977, 3156499465, 1024455932, 0, 1008389209, 1060296279, 1010928144, 0,
                3171982894, 3157884148, 1060282509, 0, 3153864312, 1066002926, 1025573848,
                1065353216,
            ],
        ],
    ],
];

#[test]
fn anatomy_supported_neck_preserves_upper_shape_and_continuous_root_for_general_bipeds() {
    for model in [
        CreatureModel::Hobgoblin,
        CreatureModel::Human,
        CreatureModel::Nymph,
        CreatureModel::Medusa,
        CreatureModel::Troll,
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                let look = Look::with_choice(
                    model,
                    17,
                    Some(Variant {
                        woman,
                        build,
                        bust_lean: 1,
                        ..default()
                    }),
                );
                let body = HumanoidBody::resolve(&look);
                let actual = supported_neck_profile(body);
                let scale = body.build * if woman { 0.9 } else { 1.0 };
                // Independent authored upper-neck contract, not a call back to
                // the profile under test. Attachments use these same sections.
                let expected = [
                    (
                        Vec3::new(0.0, -0.022, -0.007),
                        Vec2::new(0.05, 0.047) * scale,
                    ),
                    (
                        Vec3::new(0.0, 0.033, -0.008),
                        Vec2::new(0.045, 0.043) * scale,
                    ),
                    (
                        Vec3::new(0.0, 0.085, -0.01),
                        Vec2::new(0.045, 0.045) * scale,
                    ),
                ];
                assert_eq!(
                    actual[1..],
                    expected,
                    "{model:?} woman={woman} build={build}: exposed neck changes"
                );
                assert_eq!(actual[0].0, Vec3::new(0.0, -0.065, -0.008));
                assert!(
                    actual[0].1.cmpgt(Vec2::splat(0.005)).all(),
                    "root must retain volume"
                );
                assert!(
                    actual[0].1.cmple(Vec2::new(0.056, 0.052) * scale).all(),
                    "burial cannot enlarge neck"
                );
                let attachment = NeckAttachment::new(body.build, neck_slim(woman));
                for (center, radii) in expected {
                    let (at, r) = attachment.section(center.y);
                    assert!(at.abs_diff_eq(center, 1e-7) && r.abs_diff_eq(radii, 1e-7));
                }
                let mut old = Builder::default();
                biped_neck(&mut old, body.build, neck_slim(woman), Vec3::ONE);
                let mut fitted = Builder::default();
                fitted.smooth = true;
                biped_neck_on_body(&mut fitted, body, Vec3::ONE);
                assert!(fitted.smooth, "emission restores builder smoothing");
                let upper = |b: &Builder| {
                    b.surface_triangles()
                        .filter(|t| t.iter().all(|p| p.y >= 0.0))
                        .collect::<Vec<_>>()
                };
                assert!(!upper(&old).is_empty());
                assert_eq!(
                    upper(&fitted),
                    upper(&old),
                    "upper emitted skin remains exact"
                );
                assert_eq!(
                    fitted.surface_triangles().count(),
                    old.surface_triangles().count(),
                    "root fitting must not remove triangles or introduce remeshing"
                );
                // Every azimuth must have a finite actual skin wall through
                // the lower transition and exposed neck, including open outfits.
                for row in 0..=24 {
                    let y = -0.063 + row as f32 * 0.006;
                    for sample in 0..64 {
                        let angle = (sample as f32 + 0.37) * TAU / 64.0;
                        let origin = Vec3::new(0.0, y, -0.008);
                        let direction = Vec3::new(angle.sin(), 0.0, angle.cos());
                        let hit = fitted
                            .inner_surface(origin, direction)
                            .expect("actual neck has no circumferential hole at collar");
                        assert!(
                            (0.005..0.2).contains(&hit.distance(origin)),
                            "finite noncollapsed root wall"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn fitted_neck_root_responds_to_anatomy_and_keeps_already_buried_roots_exact() {
    let look = Look::with_choice(
        CreatureModel::Hobgoblin,
        17,
        Some(Variant {
            woman: true,
            build: 2,
            bust_lean: 1,
            ..default()
        }),
    );
    let body = HumanoidBody::resolve(&look);
    // Same neck and species, different height inside the chest support. A
    // species patch or unconditional radius multiplier cannot satisfy both.
    let mut buried = body;
    buried.head.y -= 0.06;
    let original = neck_profile(body.build, neck_slim(body.woman));
    let support = torso_shape(buried, None, false);
    for sample in 0..97 {
        let angle = (sample as f32 + 0.41) * TAU / 97.0;
        let local = original[0].0
            + Vec3::new(
                angle.sin() * original[0].1.x,
                0.0,
                angle.cos() * original[0].1.y,
            );
        let point = (local * storybook_part(Plan::Biped, J::Head) + buried.head)
            / storybook_part(Plan::Biped, J::Torso);
        assert!(
            support.distance(point) <= -0.008,
            "transfer fixture must already have full root overlap, distance={}",
            support.distance(point)
        );
    }
    assert_eq!(
        supported_neck_profile(buried),
        original,
        "already supported root is untouched"
    );
    let fitted = supported_neck_profile(body);
    assert!(
        fitted[0].1.cmplt(original[0].1).any(),
        "rendered-pose regression root must acquire burial reserve"
    );
    assert_eq!(fitted[1..], original[1..]);
}

#[test]
fn open_neck_outfits_keep_the_same_anatomical_head_and_neck_band() {
    use super::super::outfit::Neck;
    for model in [
        CreatureModel::Human,
        CreatureModel::Nymph,
        CreatureModel::Medusa,
        CreatureModel::Troll,
        CreatureModel::Hobgoblin,
    ] {
        for woman in [false, true] {
            let mut look = Look::with_choice(model, 17, Some(Variant { woman, ..default() }));
            let body = HumanoidBody::resolve(&look);
            let mut outfit = Outfit::worn(&look);
            outfit.outer = None;
            outfit.bodice.cover = Cover::Full;
            outfit.bodice.cut = Neckline::Closed;
            outfit.bodice.male_cut = Some(Neckline::Closed);
            outfit.trimmings.neck = Some(Neck::Jewel {
                band: Vec3::Y,
                gem: Vec3::X,
            });
            look.outfit = Some(outfit);
            let closed_recipe = head::Recipe::resolve(&look);
            let mut closed = Parts::default();
            closed_recipe.emit(&mut closed, &look, body);
            outfit.bodice.cover = Cover::Bare;
            outfit.bodice.cut = Neckline::DEEP;
            outfit.bodice.male_cut = Some(Neckline::DEEP);
            look.outfit = Some(outfit);
            let open_recipe = head::Recipe::resolve(&look);
            assert_eq!(
                closed_recipe.key(&look),
                open_recipe.key(&look),
                "neck anatomy must not introduce a torso-garment cache dependency"
            );
            let mut open = Parts::default();
            open_recipe.emit(&mut open, &look, body);
            for (actual, expected) in [
                (&open.matte, &closed.matte),
                (&open.satin, &closed.satin),
                (&open.metal, &closed.metal),
                (&open.glow, &closed.glow),
                (&open.ghost, &closed.ghost),
                (&open.sheer, &closed.sheer),
            ] {
                actual.assert_same_recipe(expected);
            }
        }
    }
}

// Exact retained native walk tick18 matrices, same six actors and camera run.
const NATIVE_WALK: [[[[u32; 16]; 2]; 3]; 2] = [
    [
        [
            [
                1060015976, 0, 1004537802, 0, 3128650148, 1059903429, 1036248410, 0, 3151876276,
                3183732705, 1059902860, 0, 0, 1059887861, 1031127697, 1065353216,
            ],
            [
                1060015976, 0, 1004537802, 0, 3102010624, 1060015094, 1009934312, 0, 3152019578,
                3157418560, 1060014519, 0, 3120988539, 1065650626, 1038331696, 1065353216,
            ],
        ],
        [
            [
                1060369880, 0, 1003787808, 0, 3127790034, 1060254038, 1036629948, 0, 3151134024,
                3184114161, 1060253541, 0, 0, 1060242177, 1031127697, 1065353216,
            ],
            [
                1060369880, 0, 1003787808, 0, 3101095176, 1060368911, 1010169744, 0, 3151269717,
                3157653912, 1060368409, 0, 3120494579, 1065921572, 1038535819, 1065353216,
            ],
        ],
        [
            [
                1060723779, 0, 1003002393, 0, 3126893260, 1060604646, 1037011483, 0, 3150356646,
                3184495617, 1060604216, 0, 0, 1060596732, 1031127697, 1065353216,
            ],
            [
                1060723779, 0, 1003002393, 0, 3100168512, 1060722727, 1010405200, 0, 3150484431,
                3157889280, 1060722292, 0, 3119535031, 1066192637, 1038739940, 1065353216,
            ],
        ],
    ],
    [
        [
            [
                1059615480, 3113027904, 1005188892, 0, 3119320966, 1059559892, 1032313531, 0,
                3152652664, 3179797313, 1059559240, 0, 3100017925, 1059484185, 1031763359,
                1065353216,
            ],
            [
                1059615485, 3124840919, 1005046157, 0, 975385728, 1059614677, 1009701332, 0,
                3152553475, 3157177120, 1059614042, 0, 3115349269, 1065356892, 1036544366,
                1065353216,
            ],
        ],
        [
            [
                1059957003, 3098539776, 1004506184, 0, 3122535936, 1059899783, 1032574233, 0,
                3151933569, 3180058232, 1059899209, 0, 3105171965, 1059827613, 1031783061,
                1065353216,
            ],
            [
                1059957013, 3125221673, 1004300519, 0, 975881641, 1059956108, 1009938940, 0,
                3151808481, 3157415076, 1059955553, 0, 3119760500, 1065619526, 1036693693,
                1065353216,
            ],
        ],
        [
            [
                1060298523, 951487080, 1003762801, 0, 3124842130, 1060239679, 1032834597, 0,
                3151157259, 3180318758, 1060239179, 0, 3108103441, 1060169423, 1031800766,
                1065353216,
            ],
            [
                1060298534, 3125468930, 1003501006, 0, 976248462, 1060297541, 1010173276, 0,
                3151009345, 3157649860, 1060297060, 0, 3122128021, 1065881354, 1036842832,
                1065353216,
            ],
        ],
    ],
];
