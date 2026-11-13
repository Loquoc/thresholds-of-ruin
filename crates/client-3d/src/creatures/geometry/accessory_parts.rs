//! Jewellery and small pieces: chokers, collars, armlets, waist pieces,
//! pointed shoulders and garters. Each is built in the space of the joint it
//! rides on and fitted to the resolved wearer, so any outfit can wear it.
use super::super::outfit::{Earrings, Neck, Waist};
use super::*;

/// What is worn on the neck itself, in the head joint's space where the
/// neck is built: chokers and standing collars turn with the head.
#[cfg(test)]
pub(super) fn neck_band(p: &mut Parts, neck: Neck, build: f32, slim: f32) {
    neck_band_on_support(p, neck, build, slim, None);
}

pub(super) fn neck_band_on_support(
    p: &mut Parts,
    neck: Neck,
    build: f32,
    slim: f32,
    support: Option<&crate::sdf::Sculpt>,
) {
    let r = |y: f32| {
        let profile = neck_profile(build, slim);
        let (a, b) = (profile[0], profile[1]);
        let t = ((y - a.0.y) / (b.0.y - a.0.y)).clamp(0.0, 1.0);
        a.1.lerp(b.1, t).max_element()
    };
    match neck {
        Neck::Choker(band) => {
            fitted_neck_ring_on_support(&mut p.satin, build, slim, 0.009, band, support);
        }
        Neck::Jewel { band, gem } => {
            let front = fitted_neck_ring_on_support(&mut p.satin, build, slim, 0.01, band, support);
            p.metal.blob(
                front + Vec3::Z * (0.006 * build),
                Vec3::new(0.013, 0.016, 0.008) * build,
                1,
                0.0,
                gem,
            );
        }
        Neck::Stand(color) => {
            // A stiff collar rising round the neck, open in a V in front.
            for side in [-1.0f32, 1.0] {
                let path: Vec<Vec3> = (0..=6)
                    .map(|k| {
                        let a = side * (0.35 + k as f32 / 6.0 * (PI - 0.35));
                        Vec3::new(
                            a.sin() * (r(-0.05) + 0.012),
                            -0.04,
                            a.cos() * (r(-0.05) + 0.012) - 0.008,
                        )
                    })
                    .collect();
                for (i, w) in path.windows(2).enumerate() {
                    let rise = 0.05 + 0.015 * (i as f32 / 5.0);
                    p.satin.extrude(
                        &[
                            Vec2::new(0.0, 0.0),
                            Vec2::new(w[0].distance(w[1]), 0.0),
                            Vec2::new(w[0].distance(w[1]), rise),
                            Vec2::new(0.0, rise),
                        ],
                        w[0] - Vec3::Y * 0.025,
                        (w[1] - w[0]).normalize(),
                        Vec3::Y,
                        0.008,
                        color,
                    );
                }
            }
        }
        Neck::Collar { .. } | Neck::Teeth { .. } | Neck::Pendant { .. } => {}
    }
}

/// Closed elliptical band: keep authored height, with a thin radial section
/// rather than moving a bulky round torus farther away from the neck.
#[cfg(test)]
fn fitted_neck_ring(b: &mut Builder, build: f32, slim: f32, half_height: f32, color: Vec3) -> Vec3 {
    fitted_neck_ring_on_support(b, build, slim, half_height, color, None)
}

fn fitted_neck_ring_on_support(
    b: &mut Builder,
    build: f32,
    slim: f32,
    half_height: f32,
    color: Vec3,
    support: Option<&crate::sdf::Sculpt>,
) -> Vec3 {
    let attachment = super::torso::NeckAttachment::new(build, slim);
    let seat = attachment.band_seat(half_height * build);
    let ring = radial_parts::RoundedBandProfile {
        height: seat,
        half_height: half_height * build,
        clearance: 0.003 * build,
        radial_half: 0.0035 * build,
        sides: 24,
        sections: 8,
    }
    .resolve(
        |height, angle, gap| Some(attachment.supported_surface(height, angle, gap, support)),
        color,
    )
    .expect("anatomical neck support is complete");
    let front = ring
        .outer_surface(Vec3::new(0.0, seat, -0.008), Vec3::Z)
        .expect("own ring front mount");
    for triangle in ring.surface_triangles() {
        b.tri(triangle, color);
    }
    front
}

/// What lies over the collarbones, in the torso's space: a broad beaded
/// collar, or teeth strung on a cord.
pub(super) fn neck_collar(
    p: &mut Parts,
    neck: Neck,
    body: HumanoidBody,
    form: &crate::sdf::Sculpt,
    look: &Look,
) {
    if !matches!(
        neck,
        Neck::Pendant { .. } | Neck::Teeth { .. } | Neck::Collar { .. }
    ) {
        return;
    }
    let face = face_shape(look, !matches!(look.style, Style::Hobgoblin | Style::Troll)).form;
    let support = necklace_parts::NecklaceSupport::new(body, form, &face);
    let core = super::super::anatomy::TorsoCoreProfile::biped(body);
    match neck {
        Neck::Pendant { .. } | Neck::Teeth { .. } => {
            necklace_parts::emit(p, neck, core, &support);
        }
        Neck::Collar { metal, beads } => {
            necklace_parts::emit_collar(p, core, &support, metal, beads);
        }
        _ => {}
    }
}

/// A band round the upper arm, in the arm joint's space.
pub(super) fn armlet(p: &mut Parts, color: Vec3, build: f32, arm_len: f32) {
    let profile = radial_parts::RoundedBandProfile {
        height: -0.13 * arm_len,
        half_height: 0.011,
        clearance: 0.003 * build,
        // Like the authored height, metal thickness is a style dimension,
        // not an anatomical radius that grows with the wearer's build.
        radial_half: 0.0025,
        sides: 24,
        sections: 8,
    };
    let ring = profile.resolve_layers(p, color, None);
    if let Some(ring) = ring {
        for triangle in ring.surface_triangles() {
            p.metal.tri(triangle, color);
        }
    }
}

/// Fine wrist bracelet, in the forearm joint frame. Keep the authored seat.
pub(super) fn resolve_bracelet(
    support: &Parts,
    color: Vec3,
    build: f32,
    arm_len: f32,
) -> Option<Builder> {
    let profile = radial_parts::RoundedBandProfile {
        height: -0.255 * arm_len,
        half_height: 0.004 * build,
        clearance: 0.002 * build,
        radial_half: 0.004 * build,
        sides: 24,
        sections: 8,
    };
    // Resolve against only the support selected by the composition plan.
    profile.resolve_layers(support, color, Some(0.018))
}

pub(super) fn emit_bracelet(p: &mut Parts, ring: Builder, color: Vec3) {
    for triangle in ring.surface_triangles() {
        p.metal.tri(triangle, color);
    }
}

#[cfg(test)]
pub(super) fn bracelet(p: &mut Parts, color: Vec3, build: f32, arm_len: f32) {
    if let Some(ring) = resolve_bracelet(p, color, build, arm_len) {
        emit_bracelet(p, ring, color);
    }
}

/// Stiff pointed shoulders standing up off the arm, in its joint's space.
pub(super) fn peak(p: &mut Parts, color: Vec3, side: f32, build: f32) {
    p.satin.extrude(
        &[
            Vec2::new(-0.03, -0.03),
            Vec2::new(0.105, -0.03),
            Vec2::new(0.12, 0.07),
        ],
        Vec3::new(0.0, 0.0, -0.05 * build),
        Vec3::X * side,
        Vec3::Y,
        0.1 * build,
        color,
    );
}

/// What is worn round the waist over the garments, in the hips' space.
pub(super) fn waist(p: &mut Parts, waist: Waist, body: HumanoidBody, support: &crate::sdf::Sculpt) {
    let fit = |height| super::super::robe::belt_on(body, height, support);
    let belt = fit(BELT_CENTER_Y);
    let at = |a: f32, y: f32, out: f32| {
        if y >= BELT_BOTTOM_Y {
            waist_parts::point(body, support, y, a, out)
        } else {
            Vec3::new(a.sin() * (belt.x + out), y, a.cos() * (belt.y + out))
        }
    };
    match waist {
        Waist::Jeweled { metal, gem } => {
            // The framework is fitted to the body at every height, rather
            // than a horizontal ring that projects from a narrow waist.
            let path: Vec<_> = (0..=64)
                .map(|k| {
                    let a = k as f32 * TAU / 64.0;
                    waist_parts::point(
                        body,
                        support,
                        0.125 - 0.075 * a.cos().max(0.0).powi(4),
                        a,
                        0.009,
                    )
                })
                .collect();
            tube(
                &mut p.metal,
                &path,
                &vec![(0.0035, 0.0035); path.len()],
                metal,
            );
            for k in [-2i32, -1, 0, 1, 2] {
                let a = k as f32 * 0.42;
                let at = waist_parts::point(
                    body,
                    support,
                    0.125 - 0.075 * a.cos().max(0.0).powi(4),
                    a,
                    0.015,
                );
                let size = if k == 0 { 1.0 } else { 0.65 };
                p.metal
                    .blob(at, Vec3::new(0.018, 0.028, 0.008) * size, 1, 0.0, metal);
                p.satin.blob(
                    at + Vec3::Z * 0.007,
                    Vec3::new(0.012, 0.020, 0.007) * size,
                    1,
                    0.0,
                    gem,
                );
            }
        }
        Waist::Sash(color) => {
            // A broad sash round the waist, knotted at her left hip with
            // two tails hanging from the knot.
            waist_parts::band(&mut p.matte, body, support, Vec2::new(0.04, 0.11), color);
            let knot = at(0.9, 0.06, 0.012);
            p.matte
                .blob(knot, Vec3::new(0.028, 0.03, 0.022), 3, 0.1, color);
            for (k, swing) in [(0.0f32, -0.02f32), (1.0, 0.03)] {
                p.matte.extrude(
                    &[
                        Vec2::new(-0.022, 0.0),
                        Vec2::new(0.022, 0.0),
                        Vec2::new(0.03 + swing, 0.3 - 0.06 * k),
                        Vec2::new(-0.01 + swing, 0.32 - 0.06 * k),
                    ],
                    knot + Vec3::new(0.0, -0.01, 0.006),
                    Vec3::new(0.9f32.cos(), 0.0, -0.9f32.sin()),
                    Vec3::NEG_Y,
                    0.006,
                    color * (1.0 - 0.08 * k),
                );
            }
        }
        Waist::Coins(metal) => {
            // A belt low on the hips, dipping to a point in front, hung
            // with swags of coins.
            let path: Vec<Vec3> = (0..=16)
                .map(|k| {
                    let a = (k as f32 / 16.0 - 0.5) * TAU;
                    at(a, -0.005 - 0.05 * a.cos().max(0.0).powi(4), 0.012)
                })
                .collect();
            tube(
                &mut p.metal,
                &path,
                &vec![(0.012, 0.012); path.len()],
                metal,
            );
            for row in 0..2 {
                for k in -5i32..=5 {
                    let a = k as f32 * 0.24;
                    let sag = 0.035 * (1.0 - (k as f32 / 5.0).powi(2));
                    let coin = at(
                        a,
                        -0.03 - 0.05 * a.cos().powi(4) - sag - 0.035 * row as f32,
                        0.02,
                    );
                    p.metal
                        .blob(coin, Vec3::new(0.011, 0.011, 0.003), 1, 0.0, metal);
                }
            }
        }
        Waist::CrossedBelts(color) => {
            for (tilt, gap) in [(-0.04, 0.008), (0.04, 0.013)] {
                waist_parts::slanted_band(
                    &mut p.matte,
                    body,
                    support,
                    Vec2::new(0.075, 0.105),
                    Vec2::new(tilt, gap),
                    color,
                );
            }
        }
        Waist::Crossed(color) => {
            // Cords crossing round the waist above the hips, tied in front
            // with a bow and two hanging ends.
            let first_height = 0.1;
            let mut knot = None;
            for k in 0..3 {
                let y = first_height + 0.035 * k as f32;
                let path: Vec<Vec3> = (0..=16)
                    .map(|j| {
                        let a = (j as f32 / 16.0) * TAU;
                        at(a, y + 0.03 * (a * 2.0).sin(), 0.006)
                    })
                    .collect();
                let mut cord = p.metal.empty_like();
                tube(&mut cord, &path, &vec![(0.005, 0.005); path.len()], color);
                if k == 2 {
                    // Tie the upper cord, with short ends above the separate belt.
                    // Resolve against this cord only, never earlier ornaments.
                    knot = cord.outer_surface(Vec3::Y * y, Vec3::Z);
                }
                p.metal.absorb(cord);
            }
            let Some(knot) = knot else {
                return;
            };
            // Bend this small cloth tie over the same resolved waist surface.
            // Keep the actual upper-cord seat, authored pattern and thickness;
            // only depth follows the garment. Existing fitted append owns normals.
            let depth = |at: Vec3| {
                torso_surface_hit_offset(support, at.x, at.y - body.torso.y, true, 0.01)
                    .map(|point| point.z)
                    .unwrap_or(at.z)
            };
            let mut tie = Parts {
                metal: p.metal.empty_like(),
                ..default()
            };
            tie.set_transform(Affine3A::IDENTITY);
            for lobe in [-1.0f32, 1.0] {
                tie.metal.blob(
                    knot + Vec3::X * lobe * 0.018,
                    Vec3::new(0.016, 0.01, 0.006),
                    1,
                    0.0,
                    color,
                );
                tube(
                    &mut tie.metal,
                    &(0..=8)
                        .map(|step| {
                            knot + Vec3::new(lobe * 0.012, -0.06, 0.004) * (step as f32 / 8.0)
                        })
                        .collect::<Vec<_>>(),
                    &[(0.004, 0.004); 9],
                    color,
                );
            }
            p.metal.absorb_fitted(tie.metal, |at| {
                // Keep the central stitch on the cord. Outside its small join,
                // reserve the lobe half-thickness plus a surface margin.
                let join = (at.truncate() - knot.truncate()).length() / 0.008;
                at + Vec3::Z * ((depth(at) - knot.z) * join.min(1.0))
            });
        }
    }
}

/// A cloth tie knotted round her left thigh, in the leg joint's space.
pub(super) fn garter(p: &mut Parts, color: Vec3, build: f32) {
    let y = -0.2;
    let profile = radial_parts::RoundedBandProfile {
        height: y,
        half_height: 0.012,
        clearance: 0.003 * build,
        radial_half: 0.012,
        sides: 24,
        sections: 8,
    };
    let Some(ring) = profile.resolve_layers(p, color, None) else {
        return;
    };
    // The ring owns both tie mounts. Resolve them before emitting any part,
    // so neither the band nor its knot can become its own fitting support.
    let Some(outward) = Vec3::new(0.07 * build, 0.0, 0.03).try_normalize() else {
        return;
    };
    let Some(knot) = ring.outer_surface(Vec3::Y * y, outward) else {
        return;
    };
    let Some(root) = ring.outer_surface(Vec3::Y * (y - 0.01), outward) else {
        return;
    };
    for triangle in ring.surface_triangles() {
        p.matte.tri(triangle, color);
    }
    p.matte.blob(
        knot + outward * 0.002,
        Vec3::new(0.018, 0.02, 0.016),
        2,
        0.1,
        color,
    );
    p.matte.extrude(
        &[
            Vec2::new(-0.012, 0.0),
            Vec2::new(0.012, 0.0),
            Vec2::new(0.02, 0.14),
            Vec2::new(-0.004, 0.13),
        ],
        root + outward * 0.002,
        outward.cross(Vec3::Y),
        Vec3::NEG_Y,
        0.004,
        color,
    );
}

/// Emit the one resolved outfit's ear jewellery, preserving the native recipes.
/// The recipe keeps its design while its mount comes from shared ear anatomy.
pub(super) fn earrings(
    p: &mut Parts,
    recipe: Earrings,
    form: &crate::sdf::Sculpt,
    anatomy: EarAnatomy,
) {
    match recipe {
        Earrings::GemDrops { metal, gem } => {
            for side in [-1.0, 1.0] {
                let Some(mount) = anatomy.attachment(form, side) else {
                    continue;
                };
                let at = mount.surface + mount.outward * 0.002;
                p.metal
                    .prism(at, at - Vec3::Y * 0.035, 0.002, 0.002, 5, metal, 0.0);
                p.metal.blob(
                    at - Vec3::Y * 0.045,
                    Vec3::new(0.010, 0.017, 0.006),
                    1,
                    0.0,
                    metal,
                );
                p.satin.blob(
                    at + Vec3::new(0.0, -0.045, 0.005),
                    Vec3::new(0.007, 0.012, 0.005),
                    1,
                    0.0,
                    gem,
                );
            }
        }
        Earrings::Drops { metal } => {
            for side in [-1.0, 1.0] {
                let Some(mount) = anatomy.attachment(form, side) else {
                    continue;
                };
                p.metal.blob(
                    mount.surface + mount.outward * 0.002 - Vec3::Y * 0.014,
                    Vec3::new(0.006, 0.014, 0.004),
                    0,
                    0.0,
                    metal,
                );
            }
        }
        Earrings::Hoops { metal } => {
            for side in [-1.0, 1.0] {
                let Some(mount) = anatomy.attachment(form, side) else {
                    continue;
                };
                p.metal.torus(
                    mount.surface + mount.outward * 0.003 - Vec3::Y * 0.012,
                    Vec3::X,
                    0.012,
                    0.003,
                    10,
                    3,
                    metal,
                );
            }
        }
    }
}

#[cfg(test)]
mod ear_attachment_tests {
    use super::*;

    fn vertices(parts: Parts) -> Vec<Vec3> {
        let mut materials = Assets::<StandardMaterial>::default();
        let mut meshes = Assets::<Mesh>::default();
        let model = parts.finish(
            &mut meshes,
            &crate::props::PropMaterials::new(&mut materials),
        );
        model
            .0
            .iter()
            .flat_map(|(handle, _)| {
                let mesh = meshes.get(handle).unwrap();
                match mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
                    bevy::render::mesh::VertexAttributeValues::Float32x3(points) => {
                        points.iter().copied().map(Vec3::from).collect::<Vec<_>>()
                    }
                    _ => panic!("positions"),
                }
            })
            .collect()
    }

    #[test]
    fn emitted_earring_top_contacts_lobe_instead_of_floating_below_it() {
        for model in [
            CreatureModel::Human,
            CreatureModel::Nymph,
            CreatureModel::Medusa,
        ] {
            for woman in [false, true] {
                let look = Look::with_choice(model, 17, Some(Variant { woman, ..default() }));
                let form = face_shape(&look, true).form;
                let mut parts = Parts::default();
                earrings(
                    &mut parts,
                    Earrings::GemDrops {
                        metal: Vec3::ONE,
                        gem: Vec3::ONE,
                    },
                    &form,
                    EarAnatomy::default(),
                );
                let points = vertices(parts);
                for side in [-1.0, 1.0] {
                    let top = points
                        .iter()
                        .filter(|p| p.x * side > 0.09)
                        .max_by(|a, b| a.y.total_cmp(&b.y))
                        .unwrap();
                    assert!(
                        form.distance(*top).abs() < 0.004,
                        "{model:?} woman={woman} side={side} top={top:?} gap={}",
                        form.distance(*top)
                    );
                    assert!(
                        top.y > 0.115 && top.y < 0.145,
                        "contact belongs at the lobe, not neck/cheek"
                    );
                }
            }
        }
    }

    #[test]
    fn all_earring_recipes_contact_the_shared_ear_without_skin_penetration() {
        for model in [
            CreatureModel::Human,
            CreatureModel::Centaur,
            CreatureModel::Troll,
            CreatureModel::Nymph,
            CreatureModel::Medusa,
        ] {
            for woman in [false, true] {
                for build in 0..3 {
                    let look = Look::with_choice(
                        model,
                        17,
                        Some(Variant {
                            woman,
                            build,
                            ..default()
                        }),
                    );
                    let form = face_shape(
                        &look,
                        !matches!(look.style, Style::Hobgoblin | Style::Troll),
                    )
                    .form;
                    for recipe in [
                        Earrings::GemDrops {
                            metal: Vec3::ONE,
                            gem: Vec3::ONE,
                        },
                        Earrings::Drops { metal: Vec3::ONE },
                        Earrings::Hoops { metal: Vec3::ONE },
                    ] {
                        let mut parts = Parts::default();
                        earrings(&mut parts, recipe, &form, EarAnatomy::default());
                        let mut materials = Assets::<StandardMaterial>::default();
                        let mut meshes = Assets::<Mesh>::default();
                        let emitted = parts.finish(
                            &mut meshes,
                            &crate::props::PropMaterials::new(&mut materials),
                        );
                        let mut all = Vec::new();
                        for (handle, _) in emitted.0 {
                            let mesh = meshes.get(&handle).unwrap();
                            let bevy::render::mesh::VertexAttributeValues::Float32x3(points) =
                                mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
                            else {
                                panic!("positions")
                            };
                            let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                            for tri in indices.as_chunks::<3>().0 {
                                let a = Vec3::from(points[tri[0]]);
                                let b = Vec3::from(points[tri[1]]);
                                let c = Vec3::from(points[tri[2]]);
                                for sample in [
                                    a,
                                    b,
                                    c,
                                    (a + b) * 0.5,
                                    (b + c) * 0.5,
                                    (c + a) * 0.5,
                                    (a + b + c) / 3.0,
                                ] {
                                    assert!(form.distance(sample) > -0.002, "{model:?} woman={woman} build={build} {recipe:?} penetration {} at {sample:?}", form.distance(sample));
                                }
                            }
                            all.extend(points.iter().copied().map(Vec3::from));
                        }
                        for side in [-1.0, 1.0] {
                            let top = all
                                .iter()
                                .filter(|p| p.x * side > 0.09)
                                .max_by(|a, b| a.y.total_cmp(&b.y))
                                .expect("one mount per ear");
                            assert!(
                                form.distance(*top).abs() < 0.004,
                                "{model:?} {recipe:?} gap {}",
                                form.distance(*top)
                            );
                            assert!(top.y > 0.115 && top.y < 0.145 && top.z.abs() < 0.03);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn jewelry_uses_moved_scaled_mounts_and_rejects_missing_ear_support() {
        for scale in [0.7, 1.0, 1.4] {
            let anatomy = EarAnatomy {
                center: Vec3::new(0.098 * scale, 0.15 * scale + 0.04, -0.005 * scale - 0.03),
                radii: EarAnatomy::default().radii * scale,
            };
            let mut form = crate::sdf::Sculpt::default();
            for side in [-1.0, 1.0] {
                form.add(
                    crate::sdf::Shape::Ellipsoid(anatomy.center(side), anatomy.radii),
                    0.0,
                );
            }
            for recipe in [
                Earrings::GemDrops {
                    metal: Vec3::ONE,
                    gem: Vec3::ONE,
                },
                Earrings::Drops { metal: Vec3::ONE },
                Earrings::Hoops { metal: Vec3::ONE },
            ] {
                let mut parts = Parts::default();
                earrings(&mut parts, recipe, &form, anatomy);
                let points = vertices(parts);
                for side in [-1.0, 1.0] {
                    let top = points
                        .iter()
                        .filter(|p| p.x * side > 0.0)
                        .max_by(|a, b| a.y.total_cmp(&b.y))
                        .unwrap();
                    assert!(form.distance(*top).abs() < 0.004);
                    assert!((top.y - (anatomy.center.y - 0.8 * anatomy.radii.y)).abs() < 0.006);
                    assert!((top.z - (anatomy.center.z + 0.35 * anatomy.radii.z)).abs() < 0.01);
                }
                let mut unsupported = Parts::default();
                earrings(
                    &mut unsupported,
                    recipe,
                    &crate::sdf::Sculpt::default(),
                    anatomy,
                );
                assert!(
                    vertices(unsupported).is_empty(),
                    "missing ears must not mount on unrelated fallback coordinates"
                );
            }
        }
    }
}

#[cfg(test)]
mod neck_attachment_tests {
    use super::*;
    fn legacy_neck_band(p: &mut Parts, neck: Neck, build: f32, slim: f32) {
        let r = |y: f32| {
            let profile = neck_profile(build, slim);
            let (a, b) = (profile[0], profile[1]);
            let t = ((y - a.0.y) / (b.0.y - a.0.y)).clamp(0.0, 1.0);
            a.1.lerp(b.1, t).max_element()
        };
        match neck {
            Neck::Choker(band) => {
                p.satin.torus(
                    Vec3::new(0.0, -0.03, -0.008),
                    Vec3::Y,
                    r(-0.03) + 0.004,
                    0.009,
                    16,
                    4,
                    band,
                );
            }
            Neck::Jewel { band, gem } => {
                p.satin.torus(
                    Vec3::new(0.0, -0.03, -0.008),
                    Vec3::Y,
                    r(-0.03) + 0.004,
                    0.01,
                    16,
                    4,
                    band,
                );
                p.metal.blob(
                    Vec3::new(0.0, -0.03, r(-0.03) + 0.006),
                    Vec3::new(0.013, 0.016, 0.008),
                    1,
                    0.0,
                    gem,
                );
            }
            Neck::Stand(color) => {
                // A stiff collar rising round the neck, open in a V in front.
                for side in [-1.0f32, 1.0] {
                    let path: Vec<Vec3> = (0..=6)
                        .map(|k| {
                            let a = side * (0.35 + k as f32 / 6.0 * (PI - 0.35));
                            Vec3::new(
                                a.sin() * (r(-0.05) + 0.012),
                                -0.04,
                                a.cos() * (r(-0.05) + 0.012) - 0.008,
                            )
                        })
                        .collect();
                    for (i, w) in path.windows(2).enumerate() {
                        let rise = 0.05 + 0.015 * (i as f32 / 5.0);
                        p.satin.extrude(
                            &[
                                Vec2::new(0.0, 0.0),
                                Vec2::new(w[0].distance(w[1]), 0.0),
                                Vec2::new(w[0].distance(w[1]), rise),
                                Vec2::new(0.0, rise),
                            ],
                            w[0] - Vec3::Y * 0.025,
                            (w[1] - w[0]).normalize(),
                            Vec3::Y,
                            0.008,
                            color,
                        );
                    }
                }
            }
            Neck::Collar { .. } | Neck::Teeth { .. } | Neck::Pendant { .. } => {}
        }
    }

    #[test]
    fn neck_band_attachment_clears_composed_torso_at_actual_head_frame() {
        let torso_scale = storybook_part(Plan::Biped, J::Torso);
        let head_scale = storybook_part(Plan::Biped, J::Head);
        for model in [
            CreatureModel::Human,
            CreatureModel::Nymph,
            CreatureModel::Medusa,
        ] {
            for woman in [false, true] {
                for build in 0..3 {
                    let look = Look::with_choice(
                        model,
                        17,
                        Some(Variant {
                            woman,
                            build,
                            ..default()
                        }),
                    );
                    let body = HumanoidBody::resolve(&look);
                    let torso = torso_shape(body, None, true);
                    let face = face_shape(&look, true).form;
                    for half_height in [0.009, 0.01] {
                        let mut band = Builder::default();
                        fitted_neck_ring_on_support(
                            &mut band,
                            body.build,
                            neck_slim(woman),
                            half_height,
                            Vec3::ONE,
                            Some(&face),
                        );
                        let mut min = f32::INFINITY;
                        for tri in band.surface_triangles() {
                            for at in [
                                tri[0],
                                tri[1],
                                tri[2],
                                (tri[0] + tri[1]) * 0.5,
                                (tri[1] + tri[2]) * 0.5,
                                (tri[2] + tri[0]) * 0.5,
                                (tri[0] + tri[1] + tri[2]) / 3.0,
                            ] {
                                min = min.min(
                                    torso.distance((at * head_scale + body.head) / torso_scale),
                                );
                                assert!(face.distance(at) >= 0.001,
                                    "neck band buried in actual face support: model={model:?} woman={woman} build={build} at={at:?} clearance={}", face.distance(at));
                            }
                        }
                        eprintln!("composed neck {model:?} woman={woman} build={build} halfheight={half_height} min={min}");
                        assert!(
                            min >= 0.001,
                            "neck band buried in actual torso/head composition: {min}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn neck_attachment_uses_anatomy_sections_and_closed_small_mesh() {
        for build in [0.83, 1.12] {
            for slim in [0.9, 1.0] {
                let attachment = super::super::torso::NeckAttachment::new(build, slim);
                for (at, radii) in neck_profile(build, slim) {
                    let (center, resolved) = attachment.section(at.y);
                    assert!((center - at).length() < 1e-6);
                    assert!((resolved - radii).length() < 1e-6);
                }
                let mut skin = Builder::default();
                biped_neck(&mut skin, build, slim, Vec3::ONE);
                for height in [0.009, 0.01] {
                    let mut band = Builder::default();
                    fitted_neck_ring(&mut band, build, slim, height, Vec3::ONE);
                    assert_eq!(band.surface_triangles().count(), 384);
                    let (low, high) = band.bounds();
                    let top = neck_profile(build, slim)[2].0.y;
                    assert!((low.y - (top - 2.0 * height * build)).abs() < 1e-6);
                    assert!((high.y - top).abs() < 1e-6);
                    let mut edges = std::collections::BTreeMap::new();
                    for tri in band.surface_triangles() {
                        for i in 0..3 {
                            let mut edge = [
                                tri[i].to_array().map(f32::to_bits),
                                tri[(i + 1) % 3].to_array().map(f32::to_bits),
                            ];
                            edge.sort();
                            *edges.entry(edge).or_insert(0usize) += 1;
                        }
                        for at in [
                            tri[0],
                            tri[1],
                            tri[2],
                            (tri[0] + tri[1]) * 0.5,
                            (tri[1] + tri[2]) * 0.5,
                            (tri[2] + tri[0]) * 0.5,
                            (tri[0] + tri[1] + tri[2]) / 3.0,
                        ] {
                            let origin = Vec3::new(0.0, at.y, -0.008);
                            let out = (at - origin).normalize();
                            let body = skin
                                .outer_surface(origin, out)
                                .expect("actual neck below band");
                            let gap = (at - body).dot(out);
                            assert!(
                                gap >= 0.001 * build && gap <= 0.013 * build,
                                "emitted band triangle gap={gap} at={at:?}"
                            );
                        }
                    }
                    assert!(
                        edges.values().all(|n| *n == 2),
                        "closed ornament has no seam or caps through neck"
                    );
                }
            }
        }
    }

    #[test]
    fn jewel_mount_ignores_unrelated_satin_geometry() {
        for build in [0.83, 1.12] {
            let mut plain = Parts::default();
            let mut composed = Parts::default();
            composed.satin.blob(
                Vec3::new(0.0, neck_profile(build, 0.9)[2].0.y - 0.01 * build, 0.2),
                Vec3::splat(0.025),
                1,
                0.0,
                Vec3::ONE,
            );
            let neck = Neck::Jewel {
                band: Vec3::ONE,
                gem: Vec3::ONE,
            };
            neck_band(&mut plain, neck, build, 0.9);
            neck_band(&mut composed, neck, build, 0.9);
            assert_eq!(
                plain.metal.surface_triangles().collect::<Vec<_>>(),
                composed.metal.surface_triangles().collect::<Vec<_>>(),
                "mount belongs to ring, not another satin part"
            );
        }
    }

    #[test]
    fn jewel_mount_contacts_actual_band_and_stand_is_unchanged() {
        for smooth in [false, true] {
            for build in [0.83, 1.12] {
                for slim in [0.9, 1.0] {
                    let mut p = Parts::default();
                    if smooth {
                        p.soften();
                    }
                    neck_band(
                        &mut p,
                        Neck::Jewel {
                            band: Vec3::ONE,
                            gem: Vec3::ONE,
                        },
                        build,
                        slim,
                    );
                    let origin =
                        Vec3::new(0.0, neck_profile(build, slim)[2].0.y - 0.01 * build, -0.008);
                    let wall = p.satin.outer_surface(origin, Vec3::Z).unwrap();
                    let gem = p.metal.inner_surface(origin, Vec3::Z).unwrap();
                    let gap = (gem - wall).z;
                    assert!(
                        gap > -0.003 * build && gap < -0.0002 * build,
                        "gem needs shallow real mesh overlap: {gap}"
                    );
                    let mut old = Parts::default();
                    let mut new = Parts::default();
                    if smooth {
                        old.soften();
                        new.soften();
                    }
                    legacy_neck_band(&mut old, Neck::Stand(Vec3::ONE), build, slim);
                    neck_band(&mut new, Neck::Stand(Vec3::ONE), build, slim);
                    assert_eq!(
                        old.satin.surface_triangles().collect::<Vec<_>>(),
                        new.satin.surface_triangles().collect::<Vec<_>>()
                    );
                }
            }
        }
    }

    #[test]
    fn neck_bands_clear_actual_neck_without_excessive_gap() {
        for smooth in [false, true] {
            for build in [0.83, 1.0, 1.12] {
                for slim in [0.9, 1.0] {
                    for neck in [
                        Neck::Choker(Vec3::ONE),
                        Neck::Jewel {
                            band: Vec3::ONE,
                            gem: Vec3::ONE,
                        },
                    ] {
                        let mut skin = Parts::default();
                        let mut worn = Parts::default();
                        if smooth {
                            skin.soften();
                            worn.soften();
                        }
                        biped_neck(&mut skin.matte, build, slim, Vec3::ONE);
                        let half_height = match neck {
                            Neck::Choker(_) => 0.009,
                            _ => 0.01,
                        };
                        let seat = neck_profile(build, slim)[2].0.y - half_height * build;
                        neck_band(&mut worn, neck, build, slim);
                        let mut min = f32::INFINITY;
                        let mut max = f32::NEG_INFINITY;
                        for offset in [-0.007, -0.004, 0.0, 0.004, 0.007] {
                            let y = seat + offset * build;
                            for i in 0..48 {
                                let a = (i as f32 + 0.37) * TAU / 48.0;
                                let out = Vec3::new(a.sin(), 0.0, a.cos());
                                let origin = Vec3::new(0.0, y, -0.008);
                                let body =
                                    skin.matte.outer_surface(origin, out).expect("neck surface");
                                let inner = worn
                                    .satin
                                    .inner_surface(origin, out)
                                    .expect("closed neck band");
                                let gap = (inner - body).dot(out);
                                min = min.min(gap);
                                max = max.max(gap);
                            }
                        }
                        eprintln!("neck band smooth={smooth} build={build} slim={slim} min={min} max={max}");
                        assert!(min >= 0.001 * build, "neck band penetrates: {min}");
                        assert!(max <= 0.01 * build, "neck band floats: {max}");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod armlet_attachment_tests {
    use super::*;
    fn prior_neck_ring(
        b: &mut Builder,
        build: f32,
        slim: f32,
        half_height: f32,
        color: Vec3,
        support: Option<&crate::sdf::Sculpt>,
    ) -> Vec3 {
        const MAJOR: usize = 24;
        const MINOR: usize = 8;
        let attachment = super::torso::NeckAttachment::new(build, slim);
        let seat = attachment.band_seat(half_height * build);
        let clearance = 0.003 * build;
        let radial_half = 0.0035 * build;
        let point = |i: usize, j: usize| {
            let angle = (i % MAJOR) as f32 * TAU / MAJOR as f32;
            let cross = (j % MINOR) as f32 * TAU / MINOR as f32;
            let height = seat + cross.sin() * half_height * build;
            attachment.supported_surface(
                height,
                angle,
                clearance + radial_half * (1.0 + cross.cos()),
                support,
            )
        };
        let mut ring = Builder::default();
        for i in 0..MAJOR {
            let angle = (i as f32 + 0.5) * TAU / MAJOR as f32;
            let inside =
                attachment.supported_surface(seat, angle, clearance + radial_half, support);
            for j in 0..MINOR {
                ring.quad_out(
                    [
                        point(i, j),
                        point(i + 1, j),
                        point(i + 1, j + 1),
                        point(i, j + 1),
                    ],
                    inside,
                    color,
                );
            }
        }
        let front = ring
            .outer_surface(Vec3::new(0.0, seat, -0.008), Vec3::Z)
            .expect("own ring front mount");
        for triangle in ring.surface_triangles() {
            b.tri(triangle, color);
        }
        front
    }

    #[test]
    fn shared_rounded_profile_preserves_qualified_neck_mesh_and_mount() {
        for build in [0.85, 1.0, 1.15] {
            for slim in [0.82, 0.94, 1.0] {
                for half_height in [0.009, 0.01] {
                    let mut face_support = crate::sdf::Sculpt::default();
                    face_support.add(
                        crate::sdf::Shape::Ellipsoid(
                            Vec3::new(0.0, 0.02, -0.008),
                            Vec3::new(0.055, 0.08, 0.065) * build,
                        ),
                        0.0,
                    );
                    for face in [None, Some(face_support)] {
                        let mut old = Builder::default();
                        let old_mount = prior_neck_ring(
                            &mut old,
                            build,
                            slim,
                            half_height,
                            GILT,
                            face.as_ref(),
                        );
                        let mut new = Builder::default();
                        let new_mount = fitted_neck_ring_on_support(
                            &mut new,
                            build,
                            slim,
                            half_height,
                            GILT,
                            face.as_ref(),
                        );
                        assert!(old_mount.distance(new_mount) < 1e-6);
                        let old: Vec<_> = old.surface_triangles().collect();
                        let new: Vec<_> = new.surface_triangles().collect();
                        assert_eq!(old.len(), new.len());
                        for (a, b) in old.into_iter().zip(new) {
                            for (a, b) in a.into_iter().zip(b) {
                                assert!(a.distance(b) < 1e-6, "unchanged neck vertex/winding");
                            }
                        }
                    }
                }
            }
        }
    }

    fn assert_closed_consistently_wound(triangles: &[[Vec3; 3]]) {
        use std::collections::BTreeMap;
        let key = |v: Vec3| {
            [
                (v.x * 1e7).round() as i64,
                (v.y * 1e7).round() as i64,
                (v.z * 1e7).round() as i64,
            ]
        };
        let mut edges = BTreeMap::new();
        for tri in triangles {
            assert!(
                (tri[1] - tri[0]).cross(tri[2] - tri[0]).length() > 1e-9,
                "nondegenerate ring facet"
            );
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                let (a, b) = (key(tri[a]), key(tri[b]));
                let (edge, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                let count = edges.entry(edge).or_insert((0, 0));
                count.0 += 1;
                count.1 += sign;
            }
        }
        assert!(
            edges
                .values()
                .all(|&(count, direction)| count == 2 && direction == 0),
            "closed consistently oriented ring"
        );
    }

    fn layer_fixture(body: HumanoidBody, side: f32, style: usize) -> Parts {
        let mut p = Parts::default();
        organic_upper_arm(
            &mut p.matte,
            side,
            body.build,
            body.arm_length,
            body.profile.muscle,
            true,
            BONE,
        );
        match style {
            0 => organic_upper_sleeve(
                &mut p.matte,
                side,
                body.build,
                body.arm_length,
                body.profile.muscle,
                LEATHER,
            ),
            1 | 2 => styled_sleeve_top(
                &mut p,
                super::super::super::outfit::Sleeve {
                    color: LEATHER,
                    cut: if style == 1 {
                        SleeveCut::Puff
                    } else {
                        SleeveCut::Short
                    },
                    puff: false,
                    puff_lining: None,
                    sheer: style == 1,
                    flare: None,
                },
                side,
                body.build,
                body.arm_length,
            ),
            3 => biped_pauldron(&mut p, side, body, PauldronFinish::STEEL),
            _ => unreachable!(),
        }
        p
    }

    #[test]
    fn armlet_clears_retained_sleeves_and_pauldron_hem_between_vertices() {
        for woman in [false, true] {
            for build in 0..3 {
                let body = HumanoidBody::resolve(&Look::with_choice(
                    CreatureModel::Human,
                    22,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                ));
                for side in [-1.0, 1.0] {
                    for style in 0..4 {
                        let mut p = layer_fixture(body, side, style);
                        let prior_metal = p.metal.surface_triangles().count();
                        let mut support = Builder::default();
                        for b in [&p.matte, &p.metal, &p.satin, &p.sheer, &p.ghost, &p.glow] {
                            for tri in b.surface_triangles() {
                                support.tri(tri, BONE);
                            }
                        }
                        armlet(&mut p, GILT, body.build, body.arm_length);
                        let ring: Vec<_> = p.metal.surface_triangles().skip(prior_metal).collect();
                        assert_eq!(ring.len(), 384, "complete supported ring");
                        assert_closed_consistently_wound(&ring);
                        for tri in ring {
                            for point in [
                                tri[0],
                                tri[1],
                                tri[2],
                                (tri[0] + tri[1]) * 0.5,
                                (tri[1] + tri[2]) * 0.5,
                                (tri[2] + tri[0]) * 0.5,
                                (tri[0] + tri[1] + tri[2]) / 3.0,
                            ] {
                                let origin = Vec3::Y * point.y;
                                let ray = (point - origin).normalize();
                                {
                                    let hit = support
                                        .outer_surface(origin, ray)
                                        .expect("complete retained arm oracle");
                                    let gap = (point - hit).dot(ray);
                                    assert!(gap>=0.0008,"woman={woman} build={build} side={side} style={style} gap={gap:.6} point={point:?}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn armlet_support_is_material_independent_and_preserves_prior_meshes() {
        let mut expected = None;
        for material in 0..6 {
            let mut p = Parts::default();
            let builder = match material {
                0 => &mut p.matte,
                1 => &mut p.metal,
                2 => &mut p.satin,
                3 => &mut p.sheer,
                4 => &mut p.ghost,
                _ => &mut p.glow,
            };
            builder.tube(
                &[Vec3::Y * -0.2, Vec3::ZERO],
                &[Vec2::splat(0.06); 2],
                48,
                &|_, _, _| BONE,
            );
            let before: Vec<Vec<_>> = [&p.matte, &p.metal, &p.satin, &p.sheer, &p.ghost, &p.glow]
                .into_iter()
                .map(|b| b.surface_triangles().collect())
                .collect();
            armlet(&mut p, GILT, 1.0, 1.0);
            let after: Vec<Vec<_>> = [&p.matte, &p.metal, &p.satin, &p.sheer, &p.ghost, &p.glow]
                .into_iter()
                .map(|b| b.surface_triangles().collect())
                .collect();
            for i in 0..6 {
                assert_eq!(&after[i][..before[i].len()], before[i]);
            }
            let ring = after[1][before[1].len()..].to_vec();
            assert_eq!(ring.len(), 384);
            if let Some(ref expected) = expected {
                assert_eq!(&ring, expected);
            } else {
                expected = Some(ring);
            }
        }
    }

    #[test]
    fn armlet_missing_support_rejects_whole_ring() {
        let mut p = Parts::default();
        armlet(&mut p, GILT, 1.0, 1.0);
        assert_eq!(p.metal.surface_triangles().count(), 0);
        p.matte.blob(
            Vec3::new(0.045, -0.13, 0.0),
            Vec3::splat(0.025),
            1,
            0.0,
            BONE,
        );
        armlet(&mut p, GILT, 1.0, 1.0);
        assert_eq!(p.metal.surface_triangles().count(), 0);
    }

    #[test]
    fn armlet_clears_actual_upper_arm_on_both_sides_and_body_variants() {
        let mut failures = Vec::new();
        let mut range = (f32::INFINITY, f32::NEG_INFINITY);
        for kind in [
            CreatureModel::Human,
            CreatureModel::Orc,
            CreatureModel::Nymph,
        ] {
            for woman in [false, true] {
                for build in 0..3 {
                    let look = Look::with_choice(
                        kind,
                        22,
                        Some(Variant {
                            woman,
                            build,
                            ..default()
                        }),
                    );
                    let body = HumanoidBody::resolve(&look);
                    for side in [-1.0, 1.0] {
                        let support = upper_arm_form(
                            side,
                            body.build,
                            body.arm_length,
                            body.profile.muscle,
                            0.94,
                        );
                        let mut p = Parts::default();
                        organic_upper_arm(
                            &mut p.matte,
                            side,
                            body.build,
                            body.arm_length,
                            body.profile.muscle,
                            true,
                            look.skin,
                        );
                        armlet(&mut p, GILT, body.build, body.arm_length);
                        let mut minimum = f32::INFINITY;
                        let mut maximum = f32::NEG_INFINITY;
                        let mut count = 0;
                        for triangle in p.metal.surface_triangles() {
                            for point in [
                                triangle[0],
                                triangle[1],
                                triangle[2],
                                (triangle[0] + triangle[1]) * 0.5,
                                (triangle[1] + triangle[2]) * 0.5,
                                (triangle[2] + triangle[0]) * 0.5,
                                (triangle[0] + triangle[1] + triangle[2]) / 3.0,
                            ] {
                                let distance = support.distance(point);
                                minimum = minimum.min(distance);
                                maximum = maximum.max(distance);
                                count += 1;
                            }
                        }
                        assert!(count > 0);
                        range.0 = range.0.min(minimum);
                        range.1 = range.1.max(maximum);
                        if minimum < 0.001 || maximum > 0.018 {
                            failures.push(format!("{kind:?} woman={woman} build={build} side={side}: surface clearance {minimum:.6}..{maximum:.6}"));
                        }
                    }
                }
            }
        }
        println!(
            "Actual armlet triangle sample clearance: {:.6}..{:.6}m",
            range.0, range.1
        );
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}

#[cfg(test)]
#[path = "necklace_tests.rs"]
mod necklace_tests;

#[cfg(test)]
#[path = "collar_tests.rs"]
mod collar_tests;

#[cfg(test)]
#[path = "garter_tests.rs"]
mod garter_tests;

#[cfg(test)]
#[path = "waist_bow_tests.rs"]
mod waist_bow_tests;
