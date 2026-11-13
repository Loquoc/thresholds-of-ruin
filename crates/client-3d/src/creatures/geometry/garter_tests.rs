use super::*;

fn wearer(model: CreatureModel, woman: bool, build: u8) -> (Look, HumanoidBody) {
    let look = Look::with_choice(
        model,
        17,
        Some(Variant {
            woman,
            build,
            hair: 0,
            ..default()
        }),
    );
    let body = HumanoidBody::resolve(&look);
    (look, body)
}

#[test]
fn garter_clears_the_actual_thigh_instead_of_a_nominal_radius() {
    let (look, body) = wearer(CreatureModel::Human, true, 2);
    let mut p = Parts::default();
    sculpted_limb(&mut p.matte, body.thigh, look.skin);
    let before = p.matte.surface_triangles().count();
    garter(&mut p, Vec3::ONE, body.build);
    let garment: Vec<_> = p.matte.surface_triangles().skip(before).collect();
    assert!(garment.len() >= 100, "fixture must emit the complete tie");
    let thigh = limb_form(body.thigh);
    let minimum = garment
        .iter()
        .flat_map(|t| {
            [
                t[0],
                t[1],
                t[2],
                (t[0] + t[1]) * 0.5,
                (t[1] + t[2]) * 0.5,
                (t[2] + t[0]) * 0.5,
                (t[0] + t[1] + t[2]) / 3.0,
            ]
        })
        .map(|p| thigh.distance(p))
        .fold(f32::INFINITY, f32::min);
    assert!(
        minimum >= 0.001,
        "actual thigh penetrates complete garter: minimum clearance {minimum}m"
    );
}

fn mesh(triangles: &[[Vec3; 3]]) -> Builder {
    let mut b = Builder::default();
    for &t in triangles {
        b.tri(t, Vec3::ONE);
    }
    b
}
fn parts_builders(p: &Parts) -> [&Builder; 6] {
    [&p.matte, &p.metal, &p.satin, &p.sheer, &p.ghost, &p.glow]
}
fn assert_prefix(p: &Parts, before: &Parts, rejected: bool) {
    for (index, (a, b)) in parts_builders(p)
        .into_iter()
        .zip(parts_builders(before))
        .enumerate()
    {
        if index == 0 && !rejected {
            a.assert_preserves_recipe_prefix(b);
        } else {
            a.assert_same_recipe(b);
        }
    }
}
fn contact_witness(triangles: &[[Vec3; 3]], build: f32) -> bool {
    if triangles.len() <= 400 {
        return false;
    }
    let ring = mesh(&triangles[..384]);
    let knot = mesh(&triangles[384..triangles.len() - 16]);
    let out = Vec3::new(0.07 * build, 0.0, 0.03).normalize();
    let origin = Vec3::Y * -0.2;
    let Some(band) = ring.outer_surface(origin, out) else {
        return false;
    };
    let Some(near) = knot.inner_surface(origin, out) else {
        return false;
    };
    let Some(far) = knot.outer_surface(origin, out) else {
        return false;
    };
    if near.dot(out) > band.dot(out) + 1e-5 || far.dot(out) < band.dot(out) - 1e-5 {
        return false;
    }
    let tail = &triangles[triangles.len() - 16..];
    let top = tail
        .iter()
        .flatten()
        .map(|p| p.y)
        .fold(f32::NEG_INFINITY, f32::max);
    let low = tail
        .iter()
        .flatten()
        .map(|p| p.y)
        .fold(f32::INFINITY, f32::min);
    if (top + 0.21).abs() > 1e-6 || (low + 0.35).abs() > 1e-6 {
        return false;
    }
    let mut roots = Vec::new();
    for p in tail.iter().flatten().filter(|p| (p.y - top).abs() < 1e-6) {
        if !roots.iter().any(|q: &Vec3| q.distance(*p) < 1e-6) {
            roots.push(*p);
        }
    }
    if roots.len() != 4 {
        return false;
    }
    roots.sort_by(|a, b| a.dot(out).total_cmp(&b.dot(out)));
    let inner = (roots[0] + roots[1]) * 0.5;
    let outer = (roots[2] + roots[3]) * 0.5;
    let width = roots[0] - roots[1];
    let tangent = out.cross(Vec3::Y);
    let Some(mount) = ring.outer_surface(Vec3::Y * top, out) else {
        return false;
    };
    let root_center = (inner + outer) * 0.5;
    let tips_preserved = [(0.020, -0.140), (-0.004, -0.130)]
        .into_iter()
        .all(|(x, y)| {
            [-0.002, 0.002].into_iter().all(|depth| {
                let expected = root_center + tangent * x + Vec3::Y * y + out * depth;
                tail.iter().flatten().any(|p| p.distance(expected) < 1e-5)
            })
        });
    tips_preserved
        && inner.distance(mount) < 1e-5
        && ((outer - inner) - out * 0.004).length() < 1e-5
        && (width.length() - 0.024).abs() < 1e-5
        && width.normalize().dot(tangent).abs() > 0.99999
}

#[test]
fn garter_fits_real_bare_and_clothed_thighs_and_keeps_tie_attached() {
    let mut cases = 0;
    for model in [
        CreatureModel::Human,
        CreatureModel::Orc,
        CreatureModel::Nymph,
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                for smooth in [false, true] {
                    for layer in 0..3 {
                        let (look, body) = wearer(model, woman, build);
                        let fixture = || {
                            let mut p = Parts::default();
                            if smooth {
                                p.soften();
                            }
                            if layer == 1 {
                                trouser_leg(&mut p.matte, body.thigh, body.build, false, LEATHER);
                            } else {
                                sculpted_limb(&mut p.matte, body.thigh, look.skin);
                                if layer == 2 {
                                    plate_lames(&mut p.metal, &cuisse_lames(body));
                                }
                            }
                            p
                        };
                        let before = fixture();
                        let mut p = fixture();
                        let start = p.matte.surface_triangles().count();
                        let support = mesh(
                            &parts_builders(&before)
                                .into_iter()
                                .flat_map(|b| b.surface_triangles())
                                .collect::<Vec<_>>(),
                        );
                        garter(&mut p, LEATHER, body.build);
                        assert_prefix(&p, &before, false);
                        let triangles: Vec<_> = p.matte.surface_triangles().skip(start).collect();
                        assert!(contact_witness(&triangles,body.build),"detached tie {model:?} woman={woman} build={build} smooth={smooth} layer={layer}");
                        for t in &triangles {
                            for at in [
                                t[0],
                                t[1],
                                t[2],
                                (t[0] + t[1]) * 0.5,
                                (t[1] + t[2]) * 0.5,
                                (t[2] + t[0]) * 0.5,
                                (t[0] + t[1] + t[2]) / 3.0,
                            ] {
                                let origin = Vec3::Y * at.y;
                                let out = (at - origin).normalize();
                                let hit = support
                                    .outer_surface(origin, out)
                                    .expect("actual thigh spans complete tie");
                                let clearance = (at - hit).dot(out);
                                assert!(clearance>=0.001,"tie penetrates {model:?} woman={woman} build={build} smooth={smooth} layer={layer} at={at:?} gap={clearance}");
                            }
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 108);
}

fn cylinder() -> Builder {
    let mut b = Builder::default();
    b.tube(
        &[Vec3::Y * -0.45, Vec3::Y * -0.20, Vec3::ZERO],
        &[Vec2::splat(0.11); 3],
        48,
        &|_, _, _| BONE,
    );
    b
}
#[test]
fn garter_preserves_band_dimensions_closure_and_rejects_detached_tie_oracles() {
    let mut p = Parts {
        metal: cylinder(),
        ..default()
    };
    garter(&mut p, LEATHER, 1.0);
    let tris: Vec<_> = p.matte.surface_triangles().collect();
    assert!(contact_witness(&tris, 1.0));
    let ring = mesh(&tris[..384]);
    let (low, high) = ring.bounds();
    assert!((low.y + 0.212).abs() < 1e-6 && (high.y + 0.188).abs() < 1e-6);
    let inner = ring.inner_surface(Vec3::Y * -0.2, Vec3::Z).unwrap();
    let outer = ring.outer_surface(Vec3::Y * -0.2, Vec3::Z).unwrap();
    assert!((outer.z - inner.z - 0.024).abs() < 1e-6);
    let mut edges = std::collections::BTreeMap::new();
    let key = |p: Vec3| p.to_array().map(|x| (x * 1e7).round() as i32);
    for t in &tris[..384] {
        assert!((t[1] - t[0]).cross(t[2] - t[0]).length() > 1e-10);
        for i in 0..3 {
            let (a, b) = (key(t[i]), key(t[(i + 1) % 3]));
            let (edge, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
            let e = edges.entry(edge).or_insert((0, 0));
            e.0 += 1;
            e.1 += sign;
        }
    }
    assert!(edges
        .values()
        .all(|&(count, orientation)| count == 2 && orientation == 0));
    for part in 0..3 {
        let mut broken = tris.clone();
        let tail = broken.len() - 16;
        let range = if part == 0 {
            384..tail
        } else {
            tail..broken.len()
        };
        let out = Vec3::new(0.07, 0.0, 0.03).normalize();
        let across = out.cross(Vec3::Y);
        let root = ring.outer_surface(Vec3::Y * -0.21, out).unwrap() + out * 0.002;
        for t in &mut broken[range] {
            for at in t {
                if part < 2 {
                    *at += out * 0.05;
                } else {
                    let local = *at - root;
                    *at = root
                        + Vec3::Z * local.dot(across)
                        + Vec3::Y * local.y
                        + Vec3::X * local.dot(out);
                }
            }
        }
        assert!(
            !contact_witness(&broken, 1.0),
            "oracle must reject mutation {part}"
        );
    }
}

#[test]
fn garter_is_material_independent_and_preserves_the_recipe_frame() {
    let xf = Affine3A::from_scale_rotation_translation(
        Vec3::new(1.2, 0.8, 0.9),
        Quat::from_rotation_z(0.3),
        Vec3::new(1.0, 2.0, 3.0),
    );
    let mut expected = None;
    for material in 0..6 {
        let fixture = || {
            let mut p = Parts::default();
            p.soften();
            p.set_transform(xf);
            let target = [
                &mut p.matte,
                &mut p.metal,
                &mut p.satin,
                &mut p.sheer,
                &mut p.ghost,
                &mut p.glow,
            ]
            .into_iter()
            .nth(material)
            .unwrap();
            for tri in cylinder().surface_triangles() {
                target.tri(tri, BONE);
            }
            p
        };
        let before = fixture();
        let mut p = fixture();
        let start = p.matte.surface_triangles().count();
        garter(&mut p, LEATHER, 1.0);
        assert_prefix(&p, &before, false);
        let tris: Vec<_> = p.matte.surface_triangles().skip(start).collect();
        assert!(contact_witness(&tris, 1.0));
        if let Some(ref e) = expected {
            assert_eq!(&tris, e);
        } else {
            expected = Some(tris);
        }
    }
}

#[test]
fn garter_missing_late_angular_or_axial_support_rejects_the_whole_tie() {
    let source: Vec<_> = cylinder().surface_triangles().collect();
    // The complete source is accepted above before these missing-support cases.
    for kind in 0..3 {
        let fixture = || {
            let mut p = Parts::default();
            p.soften();
            for &t in &source {
                let center = (t[0] + t[1] + t[2]) / 3.0;
                let keep = match kind {
                    0 => false,
                    1 => center.x.atan2(center.z).rem_euclid(TAU) < 5.7,
                    _ => center.y < -0.20,
                };
                if keep {
                    p.matte.tri(t, BONE);
                }
            }
            p.metal
                .blob(Vec3::Y * 0.1, Vec3::splat(0.01), 1, 0.0, STEEL);
            p
        };
        let before = fixture();
        let mut p = fixture();
        if kind > 0 {
            assert!(p.matte.outer_surface(Vec3::Y * -0.21, Vec3::Z).is_some());
        }
        garter(&mut p, LEATHER, 1.0);
        assert_prefix(&p, &before, true);
    }
}

#[test]
fn garter_production_order_keeps_left_thigh_and_guardian_exclusion() {
    let source = include_str!("biped.rs");
    let legs = source
        .split("fn legs(")
        .nth(1)
        .unwrap()
        .split("let lower_leg")
        .next()
        .unwrap();
    let garter = legs.find("accessory_parts::garter(").unwrap();
    assert!(legs.find("if blocky").unwrap() < garter);
    assert!(legs.rfind("sculpted_limb(").unwrap() < garter);
    assert!(legs.find("trouser_leg(").unwrap() < garter);
    assert!(legs.find("plate_lames(").unwrap() < garter);
    assert!(legs.contains("outfit.trimmings.garter.filter(|_| side > 0.0 && !blocky)"));
    for wardrobe in [Wardrobe::Jungle, Wardrobe::Rogue] {
        let (look, _) = wearer(CreatureModel::Human, true, 1);
        assert!(Outfit::worn(&look.dressed(wardrobe))
            .trimmings
            .garter
            .is_some());
    }
}
