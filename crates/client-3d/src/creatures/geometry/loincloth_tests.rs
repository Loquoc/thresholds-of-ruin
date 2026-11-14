//! Standing attachment regression against emitted meshes, not fitter samples.
use super::*;
use bevy::render::mesh::VertexAttributeValues;
use std::collections::HashSet;

fn mesh_triangles(mesh: &Mesh, frame: Affine3A) -> Vec<[Vec3; 3]> {
    let Some(VertexAttributeValues::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("rendered positions");
    };
    let indices: Vec<_> = mesh
        .indices()
        .map(|i| i.iter().collect())
        .unwrap_or_else(|| (0..points.len()).collect());
    indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| std::array::from_fn(|i| frame.transform_point3(Vec3::from_array(points[t[i]]))))
        .collect()
}

fn frame(world: &World, entity: Entity) -> Affine3A {
    let local = world
        .get::<Transform>(entity)
        .map_or(Affine3A::IDENTITY, Transform::compute_affine);
    world
        .get::<ChildOf>(entity)
        .map_or(local, |parent| frame(world, parent.parent()) * local)
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
    let bounds = |tri: [Vec3; 3]| {
        (
            tri.into_iter().fold(Vec3::splat(f32::INFINITY), Vec3::min),
            tri.into_iter()
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

fn depth([a, b, c]: [Vec3; 3], xy: Vec2) -> Option<f32> {
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
}

#[test]
fn native_orc_standing_loincloth_clears_actual_hips_thighs_and_lower_garments() {
    check_native_orc_loincloth(ClothCheck::Clearance);
}

#[test]
fn native_orc_loincloth_fold_preserves_authored_shape() {
    check_native_orc_loincloth(ClothCheck::Shape);
}

#[test]
fn native_orc_loincloth_ring_attaches_to_folded_panel_and_belt() {
    check_native_orc_loincloth(ClothCheck::Mount);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ClothCheck {
    Clearance,
    Shape,
    Mount,
}

fn check_native_orc_loincloth(mode: ClothCheck) {
    let mut failures = Vec::new();
    for woman in [false, true] {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Orc,
                17,
                Some(Variant {
                    woman,
                    build,
                    bust_lean: 1,
                    ..default()
                }),
            );
            let (world, rig, library, meshes) = super::super::rogue_tests::spawn_one(&look);
            let used: HashSet<_> = meshes.iter().map(|(id, _)| id).collect();
            let models = library.prepared_models(&used);
            let loin = models
                .iter()
                .find(|(key, _)| key.ends_with("/loincloth"))
                .expect("native Orc cached loincloth");
            let loin_ids: HashSet<_> = loin.1 .0.iter().map(|(h, _)| h.id()).collect();
            let drape = rig
                .drapes
                .iter()
                .map(|(entity, _)| *entity)
                .find(|entity| {
                    world.get::<Children>(*entity).is_some_and(|children| {
                        children.iter().any(|child| {
                            world
                                .get::<Mesh3d>(child)
                                .is_some_and(|m| loin_ids.contains(&m.0.id()))
                        })
                    })
                })
                .expect("actual cached loincloth attached as a drape");
            let hips = rig.joint(J::Hips).unwrap();
            assert_eq!(world.get::<ChildOf>(drape).unwrap().parent(), hips);
            assert_eq!(
                world.get::<Transform>(drape).unwrap().translation,
                Vec3::ZERO,
                "mount frame must stay on hips, rather than translating the entire accessory"
            );
            let inverse = frame(&world, hips).inverse();
            let mut cloth = Vec::new();
            let mut ring = Vec::new();
            for child in world.get::<Children>(drape).unwrap().iter() {
                let Some(handle) = world.get::<Mesh3d>(child) else {
                    continue;
                };
                let triangles = mesh_triangles(
                    meshes.get(&handle.0).unwrap(),
                    inverse * frame(&world, child),
                );
                let material = &loin
                    .1
                     .0
                    .iter()
                    .find(|(h, _)| h.id() == handle.0.id())
                    .unwrap()
                    .1;
                if material == library.materials.metal() {
                    ring.extend(triangles);
                } else {
                    cloth.extend(triangles);
                }
            }
            assert!(
                (72..=4096).contains(&cloth.len()),
                "static panel/stripe subdivision stays bounded: {} triangles",
                cloth.len()
            );
            assert!(ring.len() >= 100, "worked mounting ring remains present");
            if mode == ClothCheck::Shape {
                println!("loincloth budget woman={woman} build={build} cloth_triangles={} ring_triangles={}", cloth.len(), ring.len());
                assert_folded_cloth(&look, &loin.1, &library, &meshes);
                assert_original_belt(&look, &world, &rig, &meshes);
                continue;
            }
            if mode == ClothCheck::Mount {
                assert_rigid_ring(&ring);
                // The belt is emitted with the torso. Read actual waist meshes
                // from both owning joints; woven shading/pigment changes must not
                // change the attachment oracle.
                let mut belt = Vec::new();
                for joint in [J::Torso, J::Hips] {
                    let entity = rig.joint(joint).unwrap();
                    for rendered in test_support::joint_meshes(&world, &rig, &meshes, entity) {
                        belt.extend(
                            mesh_triangles(
                                rendered.mesh,
                                inverse * frame(&world, entity) * rendered.transform,
                            )
                            .into_iter()
                            .filter(|t| {
                                t.iter().any(|p| p.y > -0.01 && p.y < 0.16)
                                    && t.iter().any(|p| p.z > 0.0)
                            }),
                        );
                    }
                }
                assert!(belt.len() > 20, "actual waist mounting surface is present");
                // Test visibility against actual lower garments as well as the
                // corrected panel and the unchanged waist mounting surface.
                let mut lower_occluders = Vec::new();
                for (entity, _) in &rig.drapes {
                    if *entity == drape {
                        continue;
                    }
                    let Some(children) = world.get::<Children>(*entity) else {
                        continue;
                    };
                    for child in children.iter() {
                        let Some(handle) = world.get::<Mesh3d>(child) else {
                            continue;
                        };
                        let Some((key, _)) = models.iter().find(|(_, model)| {
                            model.0.iter().any(|(h, _)| h.id() == handle.0.id())
                        }) else {
                            continue;
                        };
                        let part = key.rsplit('/').next().unwrap();
                        if part.starts_with("skirt") || part == "peplum" {
                            lower_occluders.extend(mesh_triangles(
                                meshes.get(&handle.0).unwrap(),
                                inverse * frame(&world, child),
                            ));
                        }
                    }
                }
                assert!(
                    !lower_occluders.is_empty(),
                    "actual native lower garments occlude the ring"
                );
                let front: Vec<_> = cloth
                    .iter()
                    .copied()
                    .filter(|t| t.iter().all(|p| p.z > 0.0))
                    .collect();
                let panel_top = front
                    .iter()
                    .flatten()
                    .map(|p| p.y)
                    .reduce(f32::max)
                    .unwrap();
                let ring_low = ring
                    .iter()
                    .flatten()
                    .copied()
                    .fold(Vec3::splat(f32::INFINITY), Vec3::min);
                let ring_high = ring
                    .iter()
                    .flatten()
                    .copied()
                    .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
                let ring_middle = (ring_low.y + ring_high.y) * 0.5;
                for (label, target, bottom, top) in [
                    ("panel top", &front, panel_top - 0.02, panel_top),
                    ("belt", &belt, ring_middle, ring_high.y),
                ] {
                    let mut projected_ring = 0;
                    let mut visible_ring = 0;
                    let mut overlap = 0;
                    let mut contacts = 0;
                    let mut visible_contacts = 0;
                    let mut closest = f32::INFINITY;
                    let mut farthest = f32::NEG_INFINITY;
                    for iy in 0..=60 {
                        for ix in 0..=100 {
                            let xy = Vec2::new(
                                ring_low.x + (ring_high.x - ring_low.x) * ix as f32 / 100.0,
                                bottom + (top - bottom) * iy as f32 / 60.0,
                            );
                            let Some(back) =
                                ring.iter().filter_map(|t| depth(*t, xy)).reduce(f32::min)
                            else {
                                continue;
                            };
                            // Count every projected ring ray, including rays
                            // with no target overlap or no mounting contact.
                            projected_ring += 1;
                            let face = ring
                                .iter()
                                .filter_map(|t| depth(*t, xy))
                                .reduce(f32::max)
                                .unwrap();
                            let occluder = belt
                                .iter()
                                .chain(&front)
                                .chain(&lower_occluders)
                                .filter_map(|t| depth(*t, xy))
                                .reduce(f32::max);
                            let visible = occluder.is_none_or(|z| face > z + 0.0005);
                            if visible {
                                visible_ring += 1;
                            }
                            let Some(surface) =
                                target.iter().filter_map(|t| depth(*t, xy)).reduce(f32::max)
                            else {
                                continue;
                            };
                            overlap += 1;
                            let gap = back - surface;
                            closest = closest.min(gap);
                            farthest = farthest.max(gap);
                            // Mounting overlap is intentional. Only visible air
                            // between the hardware and its attachment is a fault.
                            if gap <= 0.004 {
                                contacts += 1;
                                if visible {
                                    visible_contacts += 1;
                                }
                            }
                        }
                    }
                    let visible_fraction = visible_ring as f32 / projected_ring.max(1) as f32;
                    println!("loincloth mount woman={woman} build={build} target={label} all_ring_samples={projected_ring} visible_ring_samples={visible_ring} visible_fraction={visible_fraction:.4} projected_samples={overlap} contact_samples={contacts} visible_contact_samples={visible_contacts} gap_range=[{closest:.6},{farthest:.6}]");
                    if overlap < 20
                        || contacts < 20
                        || visible_contacts < 20
                        || projected_ring == 0
                        || visible_ring * 10 < projected_ring * 9
                    {
                        failures.push(format!("{woman}/{build}/ring-{label}: floating or obscured mount, all_ring_samples={projected_ring}, visible_ring_samples={visible_ring}, visible_fraction={visible_fraction:.4}, overlap={overlap}, contacts={contacts}, visible_contacts={visible_contacts}, gap_range=[{closest:.6},{farthest:.6}]"));
                    }
                }
                continue;
            }
            let mut supports = Vec::new();
            for joint in [J::Hips, J::LegL, J::LegR] {
                let entity = rig.joint(joint).unwrap();
                let triangles: Vec<_> = test_support::joint_meshes(&world, &rig, &meshes, entity)
                    .into_iter()
                    .flat_map(|m| {
                        mesh_triangles(m.mesh, inverse * frame(&world, entity) * m.transform)
                    })
                    .collect();
                assert!(triangles.len() > 50, "actual {joint:?} geometry is present");
                supports.push((format!("{joint:?}"), triangles));
            }
            for (entity, _) in &rig.drapes {
                if *entity == drape {
                    continue;
                }
                let Some(children) = world.get::<Children>(*entity) else {
                    continue;
                };
                for child in children.iter() {
                    let Some(handle) = world.get::<Mesh3d>(child) else {
                        continue;
                    };
                    let Some((key, _)) = models
                        .iter()
                        .find(|(_, model)| model.0.iter().any(|(h, _)| h.id() == handle.0.id()))
                    else {
                        continue;
                    };
                    let part = key.rsplit('/').next().unwrap();
                    if part.starts_with("skirt") || part == "peplum" {
                        supports.push((
                            part.to_owned(),
                            mesh_triangles(
                                meshes.get(&handle.0).unwrap(),
                                inverse * frame(&world, child),
                            ),
                        ));
                    }
                }
            }
            assert!(
                supports
                    .iter()
                    .any(|(name, _)| name.starts_with("skirt") || name == "peplum"),
                "native lower garment must participate, including its waistband"
            );
            for (name, support) in &supports {
                let hits: Vec<_> = cloth
                    .iter()
                    .enumerate()
                    .flat_map(|(i, a)| {
                        support
                            .iter()
                            .enumerate()
                            .filter_map(move |(j, b)| crossing(*a, *b).map(|p| (i, j, p)))
                    })
                    .collect();
                println!(
                    "loincloth woman={woman} build={build} support={name} crossings={} first={:?}",
                    hits.len(),
                    &hits[..hits.len().min(4)]
                );
                if !hits.is_empty() {
                    failures.push(format!("{woman}/{build}/{name}: {} crossings", hits.len()));
                }
            }
            // Independent dense orthographic rays also catch a panel completely
            // buried inside another closed mesh. Include the full waistband;
            // do not waive precisely the area where the baseline clips.
            for side in [-1.0f32, 1.0] {
                let panel: Vec<_> = cloth
                    .iter()
                    .copied()
                    .filter(|t| t.iter().all(|p| p.z * side > 0.0))
                    .collect();
                let low = panel
                    .iter()
                    .flatten()
                    .copied()
                    .fold(Vec3::splat(f32::INFINITY), Vec3::min);
                let high = panel
                    .iter()
                    .flatten()
                    .copied()
                    .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
                let mut sampled = 0;
                let mut worst = f32::NEG_INFINITY;
                let mut witness = Vec3::ZERO;
                for iy in 0..=160 {
                    for ix in 0..=64 {
                        let xy = Vec2::new(
                            low.x + (high.x - low.x) * ix as f32 / 64.0,
                            low.y + (high.y - low.y) * iy as f32 / 160.0,
                        );
                        let Some(back) = panel
                            .iter()
                            .filter_map(|t| depth(*t, xy).map(|z| z * side))
                            .reduce(f32::min)
                        else {
                            continue;
                        };
                        let Some(body) = supports
                            .iter()
                            .flat_map(|(_, s)| s)
                            .filter_map(|t| depth(*t, xy).map(|z| z * side))
                            .reduce(f32::max)
                        else {
                            continue;
                        };
                        sampled += 1;
                        if body - back > worst {
                            worst = body - back;
                            witness = Vec3::new(xy.x, xy.y, body * side);
                        }
                    }
                }
                assert!(
                    sampled > 100,
                    "dense rays overlap the actual wearer on side {side}"
                );
                println!("loincloth woman={woman} build={build} side={side} samples={sampled} worst_penetration={worst:.6} at={witness:?}");
                if worst > 0.0001 {
                    failures.push(format!(
                        "{woman}/{build}/side{side}: penetration={worst:.6} at {witness:?}"
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "loincloth {mode} failed: {failures:#?}",
        mode = match mode {
            ClothCheck::Mount => "ring attachment",
            ClothCheck::Shape => "authored shape",
            ClothCheck::Clearance => "standing cloth clearance",
        }
    );
}

// Frozen authored recipe: independent projection, thickness, pigment and
// fallback oracle. Do not update this recipe to follow production fit edits.
fn original_cloth(look: &Look) -> Parts {
    let body = HumanoidBody::resolve(look);
    let b = body.build;
    let pelvis = body.pelvis_radii();
    let red = look.trim;
    let edge = GILT * 0.8;
    let mut p = Parts::default();
    p.soften();
    for front in [1.0f32, -1.0] {
        let down = Vec3::new(0.0, 1.0, -0.12 * front).normalize();
        p.matte.extrude(
            &[
                Vec2::new(-0.075 * b, 0.0),
                Vec2::new(0.075 * b, 0.0),
                Vec2::new(0.09 * b, -0.4),
                Vec2::new(0.0, -0.44),
                Vec2::new(-0.09 * b, -0.4),
            ],
            Vec3::new(0.0, 0.04, front * (pelvis.z + 0.03)),
            Vec3::X,
            down,
            0.012,
            red,
        );
        // A worked border near the hem.
        p.matte.extrude(
            &[
                Vec2::new(-0.088 * b, -0.33),
                Vec2::new(0.088 * b, -0.33),
                Vec2::new(0.09 * b, -0.36),
                Vec2::new(-0.09 * b, -0.36),
            ],
            Vec3::new(0.0, 0.04, front * (pelvis.z + 0.037)),
            Vec3::X,
            down,
            0.004,
            edge,
        );
    }

    p
}

fn finish_parts(parts: Parts) -> (crate::props::Model, Assets<Mesh>) {
    let mut materials = Assets::<StandardMaterial>::default();
    let mut meshes = Assets::<Mesh>::default();
    let model = parts.finish(
        &mut meshes,
        &crate::props::PropMaterials::new(&mut materials),
    );
    (model, meshes)
}

fn colored_triangles(mesh: &Mesh) -> Vec<([Vec3; 3], [u32; 4])> {
    let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
    else {
        panic!("cloth pigments")
    };
    let triangles = mesh_triangles(mesh, Affine3A::IDENTITY);
    let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
    triangles
        .into_iter()
        .zip(indices.as_chunks::<3>().0.iter())
        .map(|(t, ids)| {
            let pigment = colors[ids[0]].map(f32::to_bits);
            assert!(ids.iter().all(|i| colors[*i].map(f32::to_bits) == pigment));
            (t, pigment)
        })
        .collect()
}

fn outer_depth(triangles: &[[Vec3; 3]], xy: Vec2, side: f32) -> f32 {
    triangles
        .iter()
        .filter_map(|t| depth(*t, xy).map(|z| z * side))
        .reduce(f32::max)
        .expect("authored face interior remains covered")
}

fn assert_folded_cloth(
    look: &Look,
    actual: &crate::props::Model,
    library: &PropLibrary,
    meshes: &Assets<Mesh>,
) {
    let (original, original_meshes) = finish_parts(original_cloth(look));
    let reference: Vec<_> = original
        .0
        .iter()
        .flat_map(|(h, _)| colored_triangles(original_meshes.get(h).unwrap()))
        .collect();
    let fitted: Vec<_> = actual
        .0
        .iter()
        .filter(|(_, m)| m != library.materials.metal())
        .flat_map(|(h, _)| colored_triangles(meshes.get(h).unwrap()))
        .collect();
    let pigments: HashSet<_> = reference.iter().map(|(_, c)| *c).collect();
    assert_eq!(
        fitted.iter().map(|(_, c)| *c).collect::<HashSet<_>>(),
        pigments,
        "authored cloth and stripe pigments"
    );
    let body = HumanoidBody::resolve(look);
    let k = 1.0 / 1.0144_f32.sqrt();
    for side in [-1.0_f32, 1.0] {
        let mut fields = Vec::new();
        // Independently classify the hem stripe by its authored vertical extent.
        for color in &pigments {
            let select = |source: &Vec<([Vec3; 3], [u32; 4])>| {
                source
                    .iter()
                    .filter(|(t, c)| c == color && t.iter().all(|p| p.z * side > 0.0))
                    .map(|(t, _)| *t)
                    .collect::<Vec<_>>()
            };
            let old = select(&reference);
            let new = select(&fitted);
            assert!(!old.is_empty() && !new.is_empty());
            let stripe = old.iter().flatten().all(|p| p.y < -0.25);
            let half = if stripe { 0.002 } else { 0.006 };
            // Subdivision may add vertices, but no projected point may leave the
            // original face and no authored corner/hem may disappear.
            for (points, faces) in [(&old, &new), (&new, &old)] {
                for point in points.iter().flatten() {
                    assert!(faces.iter().any(|t| depth(*t, point.truncate()).is_some()),
                        "authored X/Y outline and hem changed: side={side}, stripe={stripe}, point={point:?}");
                }
            }
            let start = if stripe { 0.331 } else { 0.005 };
            let end = if stripe { 0.359 } else { 0.435 };
            for iy in 0..=100 {
                let station = start + (end - start) * iy as f32 / 100.0;
                let width = if station <= 0.4 {
                    (0.075 + 0.015 * station / 0.4) * body.build
                } else {
                    0.09 * (0.44 - station) / 0.04 * body.build
                };
                for ix in [-0.7_f32, 0.0, 0.7] {
                    let x = width * ix;
                    let outward_xy = Vec2::new(x, 0.04 - station * k + 0.12 * k * half);
                    let inward_xy = Vec2::new(x, 0.04 - station * k - 0.12 * k * half);
                    let outside = outer_depth(&new, outward_xy, side);
                    let inside = new
                        .iter()
                        .filter_map(|t| depth(*t, inward_xy).map(|z| z * side))
                        .reduce(f32::min)
                        .expect("paired inner shell face");
                    if (outside - inside - 2.0 * half * k).abs() >= 2e-5 {
                        let hits = |xy: Vec2| {
                            new.iter()
                                .enumerate()
                                .filter_map(|(i, t)| depth(*t, xy).map(|z| (i, z * side, *t)))
                                .collect::<Vec<_>>()
                        };
                        println!("paired thickness witness body_build={} side={side} stripe={stripe} station={station} x={x} outward_xy={outward_xy:?} inward_xy={inward_xy:?} outside={outside} inside={inside} original_outside={} original_inside={} outer_hits={:?} inner_hits={:?}",
                            body.build,
                            outer_depth(&old, outward_xy, side),
                            old.iter().filter_map(|t| depth(*t, inward_xy).map(|z| z*side)).reduce(f32::min).unwrap(),
                            hits(outward_xy), hits(inward_xy));
                    }
                    assert!((outside - inside - 2.0 * half * k).abs() < 2e-5,
                        "paired authored shell thickness: side={side}, stripe={stripe}, station={station}, difference={}", outside - inside);
                    let shift = outside - outer_depth(&old, outward_xy, side);
                    assert!(shift >= -2e-6, "fold must only displace outward");
                }
            }
            fields.push((stripe, old, new));
        }
        let (_, old_panel, panel) = fields.iter().find(|(stripe, _, _)| !stripe).unwrap();
        let (_, old_stripe, stripe) = fields.iter().find(|(stripe, _, _)| *stripe).unwrap();
        for iy in 0..=60 {
            let station = 0.331 + 0.028 * iy as f32 / 60.0;
            for x in [-0.04 * body.build, 0.0, 0.04 * body.build] {
                let shift = |new: &[[Vec3; 3]], old: &[[Vec3; 3]], half: f32| {
                    let xy = Vec2::new(x, 0.04 - station * k + 0.12 * k * half);
                    outer_depth(new, xy, side) - outer_depth(old, xy, side)
                };
                assert!(
                    (shift(panel, old_panel, 0.006) - shift(stripe, old_stripe, 0.002)).abs()
                        < 2e-5,
                    "hem stripe follows the same neutral-station fold as its panel"
                );
            }
        }
    }
}

fn assert_rigid_ring(actual: &[[Vec3; 3]]) {
    let mut p = Parts::default();
    p.metal.torus(Vec3::ZERO, Vec3::Z, 0.035, 0.01, 14, 4, GILT);
    let (reference, meshes) = finish_parts(p);
    let expected = mesh_triangles(meshes.get(&reference.0[0].0).unwrap(), Affine3A::IDENTITY);
    assert_eq!(actual.len(), expected.len(), "unchanged ring tessellation");
    // Sorted all-pairs distances are independent of translation, tilt, vertex
    // order and winding. They detect both tube and major-radius deformation.
    let distances = |triangles: &[[Vec3; 3]]| {
        let points: Vec<_> = triangles.iter().flatten().copied().collect();
        let mut values: Vec<_> = points
            .iter()
            .enumerate()
            .flat_map(|(i, p)| points[i + 1..].iter().map(move |q| p.distance(*q)))
            .collect();
        values.sort_by(f32::total_cmp);
        values
    };
    for (a, b) in distances(actual).into_iter().zip(distances(&expected)) {
        assert!(
            (a - b).abs() < 2e-6,
            "ring must remain rigid with 35mm major / 10mm tube radii"
        );
    }
}

fn assert_original_belt(look: &Look, world: &World, rig: &Rig, meshes: &Assets<Mesh>) {
    let body = HumanoidBody::resolve(look);
    let figure = biped::Figure::resolve(body, look);
    let mut p = Parts::default();
    // Frozen belt-band recipe, independent of attachment and buckle selection.
    radial_parts::RadialBandProfile {
        heights: Vec2::new(0.045, 0.095),
        support_to_output_y: body.torso.y,
        tilt: 0.0,
        clearance: 0.004,
        thickness: 0.003,
        rows: 4,
        sides: 48,
    }
    .emit(&mut p.satin, &figure.waist_support, LEATHER * 0.8);
    let (expected, expected_meshes) = finish_parts(p);
    let expected_frame = Affine3A::from_scale(storybook_part(Plan::Biped, J::Hips));
    let expected: Vec<_> = expected
        .0
        .iter()
        .flat_map(|(h, _)| mesh_triangles(expected_meshes.get(h).unwrap(), expected_frame))
        .collect();
    let hips = rig.joint(J::Hips).unwrap();
    let inverse = frame(world, hips).inverse();
    let actual: Vec<_> = [J::Hips, J::Torso]
        .into_iter()
        .flat_map(|joint| {
            let entity = rig.joint(joint).unwrap();
            test_support::joint_meshes(world, rig, meshes, entity)
                .into_iter()
                .flat_map(move |m| {
                    mesh_triangles(m.mesh, inverse * frame(world, entity) * m.transform)
                })
        })
        .collect();
    assert!(!expected.is_empty());
    for t in expected {
        assert!(
            actual
                .iter()
                .any(|a| t.iter().all(|p| a.iter().any(|q| p.distance(*q) < 2e-6))),
            "static fold must not move or rebuild a differently shaped waist band: {t:?}"
        );
    }
}

#[test]
fn loincloth_no_support_preserves_exact_original_recipe() {
    for woman in [false, true] {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Orc,
                17,
                Some(Variant {
                    woman,
                    build,
                    bust_lean: 1,
                    ..default()
                }),
            );
            let body = HumanoidBody::resolve(&look);
            for support in [
                Vec::new(),
                vec![[Vec3::splat(10.0), Vec3::splat(11.0), Vec3::splat(12.0)]],
            ] {
                let mut actual = Parts::default();
                actual.soften();
                for side in [1.0_f32, -1.0] {
                    let fold = garment_details::panel_fold(&support, body, side);
                    garment_details::fitted_panel(
                        &mut actual.matte,
                        body,
                        side,
                        &fold,
                        look.trim,
                        GILT * 0.8,
                    );
                }
                original_cloth(&look)
                    .matte
                    .assert_same_recipe(&actual.matte);
            }
        }
    }
}

#[test]
fn native_orc_warm_loincloth_reuses_shared_meshes_without_mutation() {
    let look = Look::with_choice(
        CreatureModel::Orc,
        17,
        Some(Variant {
            build: 2,
            ..default()
        }),
    );
    let (mut world, _, mut library, mut meshes) = super::super::rogue_tests::spawn_one(&look);
    let used = meshes.iter().map(|(id, _)| id).collect();
    let models = library.prepared_models(&used);
    let (_, model) = models
        .iter()
        .find(|(key, _)| key.ends_with("/loincloth"))
        .unwrap();
    let handles: HashSet<_> = model.0.iter().map(|(h, _)| h.id()).collect();
    let snapshot: Vec<_> = model
        .0
        .iter()
        .map(|(h, _)| (h.clone(), format!("{:?}", meshes.get(h).unwrap())))
        .collect();
    let count = meshes.len();
    let parent = world.spawn(Transform::default()).id();
    let mut queue = bevy::ecs::world::CommandQueue::default();
    let rig = super::super::spawn(
        &mut Commands::new(&mut queue, &world),
        &mut library,
        &mut meshes,
        parent,
        &look,
    );
    queue.apply(&mut world);
    let attached: HashSet<_> = rig
        .drapes
        .iter()
        .flat_map(|(entity, _)| {
            world
                .get::<Children>(*entity)
                .into_iter()
                .flat_map(|children| children.iter())
        })
        .filter_map(|child| world.get::<Mesh3d>(child).map(|mesh| mesh.0.id()))
        .filter(|id| handles.contains(id))
        .collect();
    assert_eq!(
        attached, handles,
        "warm wearer attaches the same cached cloth and hardware handles"
    );
    assert_eq!(
        meshes.len(),
        count,
        "static fit allocates no per-wearer meshes"
    );
    for (handle, before) in snapshot {
        assert_eq!(
            format!("{:?}", meshes.get(&handle).unwrap()),
            before,
            "warm spawn must not mutate any cached cloth attributes or indices"
        );
    }
}

#[test]
fn native_troll_apron_clears_actual_lower_garments() {
    let mut failures = Vec::new();
    for (model, woman, build) in [
        CreatureModel::Troll,
        CreatureModel::Human,
        CreatureModel::Orc,
    ]
    .into_iter()
    .flat_map(|model| {
        [false, true]
            .into_iter()
            .flat_map(move |woman| (0..3).map(move |build| (model, woman, build)))
    }) {
        let mut look = Look::with_choice(
            model,
            42,
            Some(Variant {
                woman,
                build,
                bust_lean: 1,
                ..default()
            }),
        );
        if model != CreatureModel::Troll {
            let donor = Look::with_choice(
                CreatureModel::Troll,
                42,
                Some(Variant {
                    woman,
                    build,
                    bust_lean: 1,
                    ..default()
                }),
            );
            look.outfit = Some(crate::creatures::outfit::Outfit::native(&donor));
        }
        let (world, rig, library, meshes) = super::super::rogue_tests::spawn_one(&look);
        let used: HashSet<_> = meshes.iter().map(|(id, _)| id).collect();
        let models = library.prepared_models(&used);
        let inverse = frame(&world, rig.joint(J::Hips).unwrap()).inverse();
        let mut apron = Vec::new();
        let mut support = Vec::new();
        for (entity, _) in &rig.drapes {
            for child in world.get::<Children>(*entity).unwrap().iter() {
                let Some(handle) = world.get::<Mesh3d>(child) else {
                    continue;
                };
                let Some((key, model)) = models
                    .iter()
                    .find(|(_, m)| m.0.iter().any(|(h, _)| h.id() == handle.0.id()))
                else {
                    continue;
                };
                let material = &model
                    .0
                    .iter()
                    .find(|(h, _)| h.id() == handle.0.id())
                    .unwrap()
                    .1;
                let part = key.rsplit('/').next().unwrap();
                let triangles = mesh_triangles(
                    meshes.get(&handle.0).unwrap(),
                    inverse * frame(&world, child),
                );
                if part == "rags" && *material == library.materials.satin() {
                    apron.extend(triangles);
                } else if part.starts_with("skirt") || part == "peplum" || part == "rags" {
                    support.extend(triangles);
                }
            }
        }
        assert!(
            apron.len() >= 12 && support.len() > 100,
            "actual apron and lower garment witnesses"
        );
        let mut samples = 0;
        let mut worst = f32::NEG_INFINITY;
        let bounds = apron.iter().flatten().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(low, high), &p| (low.min(p), high.max(p)),
        );
        for y in 0..=160 {
            for x in 0..=100 {
                let xy = Vec2::new(
                    bounds.0.x + (bounds.1.x - bounds.0.x) * x as f32 / 100.0,
                    bounds.0.y + (bounds.1.y - bounds.0.y) * y as f32 / 160.0,
                );
                let Some(back) = apron.iter().filter_map(|t| depth(*t, xy)).reduce(f32::min) else {
                    continue;
                };
                let Some(front) = support
                    .iter()
                    .filter_map(|t| depth(*t, xy))
                    .reduce(f32::max)
                else {
                    continue;
                };
                samples += 1;
                worst = worst.max(front - back);
            }
        }
        assert!(samples > 100, "positive actual overlap");
        if worst > 0.0001 {
            failures.push(format!("{model:?}/{woman}/{build}: penetration={worst}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn fitted_troll_apron_keeps_lower_layer_clearance_during_motion() {
    let mut failures = Vec::new();
    for woman in [false, true] {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Troll,
                42,
                Some(Variant {
                    woman,
                    build,
                    bust_lean: 1,
                    ..default()
                }),
            );
            let (mut world, rig, library, meshes) = super::super::rogue_tests::spawn_one(&look);
            let used: HashSet<_> = meshes.iter().map(|(id, _)| id).collect();
            let models = library.prepared_models(&used);
            let mut apron = Vec::new();
            let mut support = Vec::new();
            for (entity, _) in &rig.drapes {
                for child in world.get::<Children>(*entity).unwrap().iter() {
                    let Some(handle) = world.get::<Mesh3d>(child) else {
                        continue;
                    };
                    let Some((key, model)) = models
                        .iter()
                        .find(|(_, m)| m.0.iter().any(|(h, _)| h.id() == handle.0.id()))
                    else {
                        continue;
                    };
                    let material = &model
                        .0
                        .iter()
                        .find(|(h, _)| h.id() == handle.0.id())
                        .unwrap()
                        .1;
                    let part = key.rsplit('/').next().unwrap();
                    let entry = (child, handle.0.clone());
                    if part == "rags" && *material == library.materials.satin() {
                        apron.push(entry);
                    } else if part.starts_with("skirt") || part == "peplum" || part == "rags" {
                        support.push(entry);
                    }
                }
            }
            assert!(
                !apron.is_empty() && support.len() >= 2,
                "actual lower layer witnesses"
            );
            let hips = rig.joint(J::Hips).unwrap();
            let root = world
                .get::<ChildOf>(rig.joint(J::Lift).unwrap())
                .unwrap()
                .parent();
            world
                .entity_mut(root)
                .insert((rig, Anim::new(42, Vec3::ZERO)));
            world.insert_resource(meshes);
            world.insert_resource(Time::<()>::default());
            let mut advance = crate::review_script::animation_schedule();
            for tick in 0..160 {
                world
                    .resource_mut::<Time>()
                    .advance_by(std::time::Duration::from_millis(16));
                let delta = if tick < 45 {
                    Vec3::Z * 0.03
                } else if tick < 90 {
                    Vec3::X * 0.03
                } else {
                    Vec3::ZERO
                };
                world.get_mut::<Transform>(root).unwrap().translation += delta;
                advance.run(&mut world);
                if tick % 8 != 0 {
                    continue;
                }
                let inverse = frame(&world, hips).inverse();
                let meshes = world.resource::<Assets<Mesh>>();
                let faces = |entries: &[(Entity, Handle<Mesh>)]| {
                    entries
                        .iter()
                        .flat_map(|(e, h)| {
                            mesh_triangles(meshes.get(h).unwrap(), inverse * frame(&world, *e))
                        })
                        .collect::<Vec<_>>()
                };
                let a = faces(&apron);
                let s = faces(&support);
                let hits = a
                    .iter()
                    .flat_map(|&a| s.iter().filter_map(move |&b| crossing(a, b)))
                    .count();
                if hits > 0 {
                    failures.push(format!("{woman}/{build}/tick{tick}: {hits} crossings"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
