//! Fitted pieces which are not shoulder capes: strips, apron and closures.
use super::*;
use crate::creatures::outfit::outerwear::{
    OuterwearAppearance, OuterwearCut, OuterwearFastening, OuterwearNeck,
};

// Torso-local apron attachment. The lower panel is emitted in the hip frame,
// so its root must include both the torso joint offset and rendered scale.
const APRON_WAIST_LOW: f32 = 0.045;
const APRON_WAIST_HIGH: f32 = 0.069;
const APRON_ROOT_OVERLAP: f32 = 0.040;

fn apron_panel_top(body: HumanoidBody) -> f32 {
    body.torso.y + APRON_WAIST_HIGH * storybook_part(Plan::Biped, J::Torso).y + APRON_ROOT_OVERLAP
}

/// Torso-local decoration. `support` must include every retained shirt and
/// armour layer; fitting against naked anatomy would bury these outer pieces.
pub(super) fn emit_torso(
    p: &mut Parts,
    appearance: OuterwearAppearance,
    body: HumanoidBody,
    support: &crate::sdf::Sculpt,
) {
    let r = appearance.recipe();
    match r.cut {
        OuterwearCut::Robe => emit_robe_torso(p, body, support, r.cloth, r.trim),
        OuterwearCut::Wraps { bands, width } => {
            for i in 0..bands {
                let y = 0.045 + i as f32 * 0.041;
                let slope = if i % 2 == 0 { 0.075 } else { -0.075 };
                for front in [true, false] {
                    let path: Vec<_> = (0..17)
                        .map(|j| {
                            let x = (j as f32 / 16.0 * 2.0 - 1.0) * 0.23 * body.build;
                            (
                                Vec2::new(x, y + slope * x / (0.23 * body.build)),
                                width * 0.60,
                            )
                        })
                        .collect();
                    fitting::fitted_ribbon(
                        &mut p.matte,
                        support,
                        &path,
                        front,
                        0.014 + i as f32 * 0.0005,
                        r.cloth * (0.76 + 0.08 * (i % 4) as f32),
                    );
                    let edge: Vec<_> = path
                        .iter()
                        .map(|(at, _)| (*at + Vec2::Y * (width * 0.53), 0.0015))
                        .collect();
                    fitting::fitted_ribbon(
                        &mut p.matte,
                        support,
                        &edge,
                        front,
                        0.018 + i as f32 * 0.0005,
                        r.cloth * 0.52,
                    );
                }
            }
            for sign in [-1.0, 1.0] {
                fitting::fitted_ribbon(
                    &mut p.matte,
                    support,
                    &[
                        (Vec2::new(-sign * 0.13 * body.build, 0.12), 0.023),
                        (Vec2::new(sign * 0.16 * body.build, 0.48), 0.023),
                    ],
                    true,
                    0.024,
                    r.cloth * 1.05,
                );
            }
        }
        OuterwearCut::Apron { bib_width, .. } => {
            let width = bib_width * body.build;
            fitting::fitted_ribbon(
                &mut p.matte,
                support,
                &[
                    (Vec2::new(0.0, 0.035), width * 0.55),
                    (Vec2::new(0.0, 0.22), width * 0.475),
                    (Vec2::new(0.0, 0.40), width * 0.5),
                ],
                true,
                0.018,
                r.cloth,
            );
            for sign in [-1.0, 1.0] {
                let x = sign * width * 0.43;
                for front in [true, false] {
                    fitting::fitted_ribbon(
                        &mut p.matte,
                        support,
                        &[
                            (Vec2::new(x, 0.37), 0.024),
                            (Vec2::new(sign * width * 0.65, 0.49), 0.024),
                        ],
                        front,
                        0.022,
                        r.cloth * 0.88,
                    );
                }
            }
            fitting::fitted_band(
                &mut p.matte,
                support,
                APRON_WAIST_LOW,
                APRON_WAIST_HIGH,
                0.024,
                r.cloth * 0.9,
            );
        }
        _ => {}
    }
    match r.neck {
        OuterwearNeck::Cowl => {
            for layer in 0..3 {
                let path: Vec<_> = (0..17)
                    .map(|i| {
                        let u = i as f32 / 16.0 * 2.0 - 1.0;
                        (
                            Vec2::new(
                                u * 0.18 * body.build,
                                0.405 + 0.070 * u.abs() + 0.018 * layer as f32,
                            ),
                            0.025,
                        )
                    })
                    .collect();
                fitting::fitted_ribbon(
                    &mut p.matte,
                    support,
                    &path,
                    true,
                    0.024 + layer as f32 * 0.004,
                    r.cloth * (0.88 + 0.06 * layer as f32),
                );
            }
        }
        OuterwearNeck::StandingCollar => {
            // Two stiff flared collar leaves, each anchored to the actual
            // shoulder surface, with the throat deliberately left open.
            for sign in [-1.0, 1.0] {
                let Some(root) = torso_surface_hit(support, sign * 0.09 * body.build, 0.47, true)
                else {
                    continue;
                };
                let q = [
                    root + Vec3::Z * 0.025,
                    root + Vec3::new(sign * 0.09 * body.build, -0.015, 0.025),
                    root + Vec3::new(sign * 0.07 * body.build, 0.14, -0.015),
                    root + Vec3::new(-sign * 0.025, 0.10, 0.01),
                ];
                p.matte.quad_out(q, root - Vec3::Z, r.cloth);
                p.matte
                    .quad_out(q.map(|v| v - Vec3::Z * 0.004), root + Vec3::Z, r.lining);
            }
        }
        OuterwearNeck::None => {}
    }
    if matches!(
        r.fastening,
        OuterwearFastening::None | OuterwearFastening::Cord
    ) {
        return;
    }
    let Some(at) = torso_surface_hit(support, 0.0, 0.425, true) else {
        return;
    };
    let at = at + Vec3::Z * 0.035;
    match r.fastening {
        OuterwearFastening::Brooch => {
            p.metal.torus(at, Vec3::Z, 0.023, 0.004, 16, 5, r.trim);
            p.metal
                .torus(at + Vec3::Z * 0.002, Vec3::Z, 0.012, 0.003, 12, 4, r.trim);
        }
        OuterwearFastening::Buckle | OuterwearFastening::DoubleBuckle => {
            let count = if r.fastening == OuterwearFastening::DoubleBuckle {
                2
            } else {
                1
            };
            for index in 0..count {
                let y = 0.425 - index as f32 * 0.047;
                fitting::fitted_ribbon(
                    &mut p.satin,
                    support,
                    &[
                        (Vec2::new(-0.10 * body.build, y), 0.022),
                        (Vec2::new(0.10 * body.build, y), 0.022),
                    ],
                    true,
                    0.027,
                    r.cloth * 0.65,
                );
                let Some(center) = torso_surface_hit(support, 0.0, y, true) else {
                    continue;
                };
                p.metal.torus(
                    center + Vec3::Z * 0.038,
                    Vec3::Z,
                    0.018,
                    0.003,
                    4,
                    4,
                    r.trim,
                );
            }
        }
        _ => {}
    }
}

/// The support remains untouched: this is an additional cloth shell, including
/// shoulders and back, rather than a replacement colour on the existing shell.
fn emit_robe_torso(
    p: &mut Parts,
    body: HumanoidBody,
    support: &crate::sdf::Sculpt,
    cloth: Vec3,
    trim: Vec3,
) {
    use crate::sdf::Shape::Beyond;
    let mut shell = torso_garment(body, TorsoMaterial::Cloth, false);
    shell.union(support.clone(), 0.0);
    shell.inflate(0.018);
    shell.carve(Beyond(Vec3::NEG_Y, 0.005), 0.0);
    shell.carve(Beyond(Vec3::Y, 0.535), 0.005);
    let cut = NecklineCut::resolve(
        if body.woman {
            Neckline::WRAP
        } else {
            Neckline::SPLIT
        },
        body,
    )
    .expect("robe has an authored open neckline");
    shell.carve(cut.opening(body, 0.535, 0.012), 0.004);
    let span = 0.48 * body.build.max(1.0);
    shell.mesh(
        &mut p.matte,
        Vec3::new(-span, -0.02, -span),
        Vec3::new(span, 0.56, span),
        0.006,
        &|at, _| cloth * crate::sdf::Sculpt::weave(at),
    );
    // The binding follows the authored V and continues diagonally across the
    // waist for the wrap cut, using the same retained outer-layer support.
    for side in [-1.0, 1.0] {
        let mut path = vec![
            (Vec2::new(side * cut.wide, cut.collar - 0.015), 0.009),
            (Vec2::new(side * cut.point, cut.bottom), 0.009),
        ];
        if cut.wrap && side < 0.0 {
            path.push((Vec2::new(0.12 * body.build, 0.055), 0.009));
        }
        fitting::fitted_ribbon(&mut p.matte, support, &path, true, 0.027, trim * 0.65);
    }
}

/// Attachment seam shared with the dynamic robe waist bridge. Unscaled torso
/// coordinates, before the caller applies HIP_SCALE and the torso joint pose.
pub(super) fn robe_waist_seam(body: HumanoidBody, support: &crate::sdf::Sculpt) -> Vec<Vec3> {
    let mut shell = torso_garment(body, TorsoMaterial::Cloth, false);
    shell.union(support.clone(), 0.0);
    shell.inflate(0.018);
    (0..48)
        .map(|column| {
            let angle = column as f32 / 48.0 * TAU;
            let out = Vec3::new(angle.sin(), 0.0, angle.cos());
            fitting::surface_ray_hit(&shell, Vec3::Y * 0.16, out, -0.002, 0.65)
                .expect("robe torso has a continuous waist seam")
        })
        .collect()
}

/// Overlay after all arm/armour geometry, before gloves. `length` is the actual
/// segment length in local meters, NOT the anatomy arm-length multiplier.
/// The joint is the origin and the segment runs down -Y. All Parts builders
/// must retain the same transforms used for the underlying segment. Existing
/// surfaces are sampled completely before emission to avoid self-inflation.
pub(super) fn emit_robe_sleeve(
    p: &mut Parts,
    appearance: OuterwearAppearance,
    length: f32,
    build: f32,
    upper: bool,
) {
    if appearance == OuterwearAppearance::MummyWrapping && length > 0.0 {
        emit_arm_wraps(p, length, build, upper);
        return;
    }
    if appearance != OuterwearAppearance::Robe || length <= 0.0 {
        return;
    }
    const ROWS: usize = 32;
    const SIDES: usize = 64;
    let r = appearance.recipe();
    // Cover the positive-y shoulder dome, not just the arm below its joint.
    // Forearm cloth overlaps above the elbow so bending cannot open a seam.
    let top = if upper { 0.10 * build } else { 0.035 * build };
    let height = |t: f32| top - (length + top) * t;
    let mut support = [0.0f32; ROWS + 1];
    for (row, radius) in support.iter_mut().enumerate() {
        // Inset only the ray sampling at the open joint boundaries, where a
        // ray exactly along an end cap can be parallel to every triangle.
        let t = (row as f32 / ROWS as f32).clamp(0.002, 0.998);
        let origin = Vec3::Y * height(t);
        for col in 0..SIDES {
            let a = col as f32 / SIDES as f32 * TAU;
            let direction = Vec3::new(a.sin(), 0.0, a.cos());
            for builder in [&p.matte, &p.satin, &p.metal, &p.glow, &p.ghost, &p.sheer] {
                if let Some(hit) = builder.outer_surface(origin, direction) {
                    *radius = radius.max((hit - origin).length());
                }
            }
        }
    }
    let rings: Vec<_> = (0..=ROWS)
        .map(|row| {
            let t = row as f32 / ROWS as f32;
            let neighbor = support[row.saturating_sub(1)..=(row + 1).min(ROWS)]
                .iter()
                .copied()
                .fold(0.0f32, f32::max);
            let y = height(t);
            let loose = if upper && y > 0.0 {
                0.085 * build * (1.0 - (y / top).powi(2)).max(0.0).sqrt()
            } else if upper {
                0.076 * build
            } else {
                (0.072 + 0.058 * t.powi(2)) * build
            };
            let radius = (neighbor + 0.016).max(loose);
            std::array::from_fn::<_, SIDES, _>(|col| {
                let a = col as f32 / SIDES as f32 * TAU;
                // Folds only add ease. The trough never invades retained armour.
                let fold = (1.0 + (a * 8.0).cos()) * 0.003 * t;
                Vec3::new(a.sin() * (radius + fold), y, a.cos() * (radius + fold))
            })
        })
        .collect();
    for row in 0..ROWS {
        for col in 0..SIDES {
            let next = (col + 1) % SIDES;
            let q = [
                rings[row][col],
                rings[row][next],
                rings[row + 1][next],
                rings[row + 1][col],
            ];
            let inside = Vec3::Y * height((row as f32 + 0.5) / ROWS as f32);
            let color = if !upper && row >= ROWS - 2 {
                r.trim * 0.78
            } else {
                r.cloth
            };
            p.matte.quad_out(q, inside, color);
            // The entire sleeve is lined and open-ended, not a solid cone
            // occluding the wrist or a few coloured rods suggesting a cuff.
            let inner = q.map(|v| v - Vec3::new(v.x, 0.0, v.z).normalize_or(Vec3::Z) * 0.005);
            p.matte.quad_out(
                inner,
                inside + Vec3::new(q[0].x, 0.0, q[0].z) * 3.0,
                r.cloth * 0.70,
            );
        }
    }
    if upper {
        let center = Vec3::Y * top;
        for col in 0..SIDES {
            p.matte.tri_out(
                [center, rings[0][col], rings[0][(col + 1) % SIDES]],
                center - Vec3::Y,
                r.cloth,
            );
        }
    }
    for row in [0, ROWS] {
        for col in 0..SIDES {
            let next = (col + 1) % SIDES;
            let a = rings[row][col];
            let b = rings[row][next];
            let inward = |v: Vec3| v - Vec3::new(v.x, 0.0, v.z).normalize_or(Vec3::Z) * 0.005;
            p.matte.quad_out(
                [a, b, inward(b), inward(a)],
                Vec3::Y * if row == 0 { -length } else { 0.0 },
                if upper || row == 0 {
                    r.cloth
                } else {
                    r.trim * 0.78
                },
            );
        }
    }
}

/// Separate overlapping windings over the retained arm mesh. Compute every
/// contact before emitting anything so the wraps never fit to themselves.
fn emit_arm_wraps(p: &mut Parts, length: f32, build: f32, upper: bool) {
    let recipe = OuterwearAppearance::MummyWrapping.recipe();
    let bands = if upper { 5 } else { 7 };
    const SIDES: usize = 48;
    let first = if upper { 0.07 } else { 0.01 };
    let pitch = (length - first) / bands as f32;
    let layers: Vec<[[Vec3; SIDES + 1]; 2]> = (0..bands)
        .map(|band| {
            std::array::from_fn(|edge| {
                std::array::from_fn(|col| {
                    let a = col as f32 / SIDES as f32 * TAU;
                    let y =
                        -first - pitch * band as f32 - edge as f32 * pitch * 0.92 + 0.009 * a.sin();
                    let origin = Vec3::Y * y;
                    let out = Vec3::new(a.sin(), 0.0, a.cos());
                    let mut radius = 0.04 * build;
                    for builder in [&p.matte, &p.satin, &p.metal, &p.glow, &p.ghost, &p.sheer] {
                        for dy in [-0.008, 0.0, 0.008] {
                            let at = origin + Vec3::Y * dy;
                            if let Some(hit) = builder.outer_surface(at, out) {
                                radius = radius.max((hit - at).dot(out));
                            }
                        }
                    }
                    origin + out * (radius + 0.009)
                })
            })
        })
        .collect();
    for (band, rings) in layers.iter().enumerate() {
        for col in 0..SIDES {
            let q = [
                rings[0][col],
                rings[0][col + 1],
                rings[1][col + 1],
                rings[1][col],
            ];
            p.matte.quad_out(
                q,
                Vec3::Y * q[0].y,
                recipe.cloth * (0.78 + 0.07 * (band % 4) as f32),
            );
        }
        let path = &rings[1];
        p.matte.tube(
            path,
            &vec![Vec2::splat(0.0015); path.len()],
            4,
            &|_, _, _| recipe.cloth * 0.54,
        );
    }
}

/// Head-local folded binding for the explicit hooded-cloak study. The native
/// hood and its face opening remain unchanged for every other wardrobe.
pub(super) fn emit_hood_rim(p: &mut Parts, cloth: Vec3, trim: Vec3) {
    let path: Vec<_> = (0..=40)
        .map(|i| {
            let angle = i as f32 / 40.0 * TAU;
            Vec3::new(
                0.096 * angle.sin(),
                0.15 + 0.133 * angle.cos(),
                0.117 - 0.018 * angle.cos(),
            )
        })
        .collect();
    p.matte.tube(
        &path,
        &vec![Vec2::splat(0.009); path.len()],
        7,
        &|_, _, _| cloth * 0.68,
    );
    let edge: Vec<_> = path.iter().map(|p| *p + Vec3::Z * 0.006).collect();
    p.matte.tube(
        &edge,
        &vec![Vec2::splat(0.0025); edge.len()],
        5,
        &|_, _, _| trim * 0.85,
    );
}

/// Only these cuts have an independently animated piece below the waist.
pub(super) fn lower_length(appearance: OuterwearAppearance) -> Option<f32> {
    match appearance.recipe().cut {
        OuterwearCut::Apron { length, .. } => Some(length),
        OuterwearCut::Wraps { .. } => Some(0.39),
        _ => None,
    }
}

// Least slope-limited majorant: preserve every support depth while easing the
// abrupt end of a retained tunic into a hanging cloth panel.
fn apron_depth_envelope(depths: &[f32], step: f32) -> Vec<f32> {
    (0..depths.len())
        .map(|i| {
            depths
                .iter()
                .enumerate()
                .map(|(j, z)| z - 0.35 * step * i.abs_diff(j) as f32)
                .fold(f32::NEG_INFINITY, f32::max)
        })
        .collect()
}

/// Hips-local hanging pieces. `clearance` contains the resolved outermost
/// hip half-width and front depth. `support` contains the actual retained skirt
/// and neutral robe outer surfaces in the same unscaled hip-joint frame. Fit
/// each height to the local envelope rather than extending belt depth downward.
/// Attach using the normal skirt drape binding, synchronized with skirt layers.
pub(super) fn emit_lower(
    p: &mut Parts,
    appearance: OuterwearAppearance,
    body: HumanoidBody,
    clearance: Vec2,
    support: &[Vec3],
    torso_support: &crate::sdf::Sculpt,
) {
    let r = appearance.recipe();
    let (width, length, pocket) = match r.cut {
        OuterwearCut::Apron { length, pocket, .. } => (clearance.x * 1.40, length, pocket),
        OuterwearCut::Wraps { .. } => (0.055 * body.build, 0.39, false),
        _ => return,
    };
    let wraps = matches!(r.cut, OuterwearCut::Wraps { .. });
    let torso_scale = storybook_part(Plan::Biped, J::Torso);
    // Match the second emitted winding, not the pelvis clearance ellipse.
    let mut root_support = torso_support.clone();
    root_support.inflate(0.0145);
    // Preserve the authored hem and pocket proportions. Only the upper edge
    // extends to overlap the actual torso-bound waistband/bib; translating the
    // entire panel would shorten it and move its established floor clearance.
    let top = apron_panel_top(body);
    let height = top - (0.045 - length);
    let centers: &[f32] = if wraps { &[-0.10, 0.075] } else { &[0.0] };
    // A conservative height neighborhood covers the retained mesh's row
    // spacing and the apron panel between its own row vertices. Keep the
    // envelope across the whole panel width so a turn does not expose a fold.
    let depths: Vec<f32> = (0..=20)
        .map(|row| {
            let y = top - height * row as f32 / 20.0;
            support
                .iter()
                .filter(|p| (p.y - y).abs() <= 0.065)
                .map(|p| p.z)
                .fold(clearance.y, f32::max)
        })
        .collect();
    let depths = apron_depth_envelope(&depths, height / 20.0);
    for &center in centers {
        let point = |u: f32, t: f32| {
            let row = (t * 20.0).floor() as usize;
            let front = depths[row.min(20)].max(depths[(row + 1).min(20)]);
            let mut at = Vec3::new(
                center * body.build + u * width * 0.5 * (1.0 + 0.20 * t),
                top - height * t
                    - if wraps {
                        0.025 * u.abs() * t.powi(5)
                    } else {
                        0.0
                    },
                front + 0.035 + 0.025 * t + (u * PI * 4.0).cos() * 0.009 * t,
            );
            if wraps {
                let root_x = (center * body.build + u * width * 0.5) / torso_scale.x;
                let root_y = 0.086 - 0.075 * root_x / (0.23 * body.build);
                let root_height = body.torso.y + root_y * torso_scale.y;
                at.y += (root_height - top) * (1.0 - t);
                let blend = ((root_height - at.y - 0.012) / 0.070).clamp(0.0, 1.0);
                if blend < 1.0 {
                    if let Some(hit) = torso_surface_hit(
                        &root_support,
                        at.x / torso_scale.x,
                        (at.y - body.torso.y) / torso_scale.y,
                        true,
                    ) {
                        // First follow the real winding through its cloth
                        // thickness, then ease out over the retained skirt.
                        let sewn_z = hit.z * torso_scale.z + 0.001;
                        let ease = blend * blend * (3.0 - 2.0 * blend);
                        at.z = sewn_z + (at.z - sewn_z) * ease;
                    }
                }
            }
            at
        };
        for row in 0..20 {
            let t0 = row as f32 / 20.0;
            let t1 = (row + 1) as f32 / 20.0;
            for column in 0..20 {
                let u0 = column as f32 / 10.0 - 1.0;
                let u1 = (column + 1) as f32 / 10.0 - 1.0;
                let q = [point(u0, t0), point(u1, t0), point(u1, t1), point(u0, t1)];
                let color = if pocket && (0.44..0.72).contains(&t0) && u0.abs() < 0.5 {
                    r.cloth * 0.86
                } else {
                    r.cloth
                };
                // A hanging panel is not star-shaped about the hip origin.
                // Explicit front/back references cannot invert at a support step.
                let middle = q.iter().copied().sum::<Vec3>() * 0.25;
                p.matte.quad_out(q, middle - Vec3::Z, color);
                p.matte.quad_out(
                    q.map(|v| v - Vec3::Z * 0.004),
                    middle + Vec3::Z,
                    color * 0.8,
                );
            }
        }
        if pocket {
            // Raised open pocket lip, rather than just a painted rectangle.
            let path: Vec<_> = (0..11)
                .map(|i| point(i as f32 / 10.0 - 0.5, 0.44) + Vec3::Z * 0.007)
                .collect();
            p.matte.tube(
                &path,
                &vec![Vec2::splat(0.003); path.len()],
                5,
                &|_, _, _| r.cloth * 0.72,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mummy_tail_roots_contact_emitted_torso_wrappings() {
        for (woman, layer_ease) in [(false, 0.0), (true, 0.0), (false, 0.04), (true, 0.04)] {
            for build in 0..3 {
                let look = Look::with_choice(
                    CreatureModel::Human,
                    22,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                );
                let body = HumanoidBody::resolve(&look);
                let mut support = torso_garment(body, TorsoMaterial::Cloth, false);
                support.inflate(layer_ease);
                let scale = storybook_part(Plan::Biped, J::Torso);
                let belt = crate::creatures::robe::belt_on(body, BELT_CENTER_Y, &support);
                let mut upper = Parts::default();
                upper.set_transform(Affine3A::from_scale_rotation_translation(
                    scale,
                    Quat::IDENTITY,
                    body.torso,
                ));
                emit_torso(
                    &mut upper,
                    OuterwearAppearance::MummyWrapping,
                    body,
                    &support,
                );
                // Compare both actual emitted surfaces in one hip frame.
                upper.set_transform(Affine3A::IDENTITY);
                let mut lower = Parts::default();
                emit_lower(
                    &mut lower,
                    OuterwearAppearance::MummyWrapping,
                    body,
                    Vec2::new(belt.x * scale.x, belt.y * scale.z),
                    &[],
                    &support,
                );
                for center in [-0.10, 0.075] {
                    let x = center * body.build;
                    let mut upper_hits = 0;
                    let mut contacts = 0;
                    for row in 0..=100 {
                        let y = body.torso.y + 0.025 + row as f32 * 0.001;
                        let ray = Vec3::new(x, y, 0.0);
                        if let Some(a) = upper.matte.outer_surface(ray, Vec3::Z) {
                            upper_hits += 1;
                            if let Some(b) = lower.matte.outer_surface(ray, Vec3::Z) {
                                if (a.z - b.z).abs() <= 0.004 {
                                    contacts += 1;
                                }
                            }
                        }
                    }
                    assert!(upper_hits >= 5, "fixture must intersect actual torso bands");
                    assert!(contacts >= 5,
                        "tail root must overlap emitted torso bands in height AND depth: woman {woman}, build {build}, center {center}, layer ease {layer_ease}, contacts {contacts}");
                }
                let low = lower.matte.bounds().0;
                assert!(
                    (low.y - (0.045 - 0.39 - 0.025)).abs() < 0.0001,
                    "attachment change preserves authored torn hem clearance"
                );
            }
        }
    }

    #[test]
    fn apron_panel_overlaps_actual_torso_waistband_on_body_and_robe_variants() {
        for woman in [false, true] {
            for build in 0..3 {
                for model in [CreatureModel::Human, CreatureModel::Mage] {
                    let look = Look::with_choice(
                        model,
                        7,
                        Some(Variant {
                            woman,
                            build,
                            ..default()
                        }),
                    );
                    let body = HumanoidBody::resolve(&look);
                    let support = torso_garment(body, TorsoMaterial::Cloth, false);
                    let retained = if model == CreatureModel::Mage {
                        crate::creatures::robe::neutral_outer_envelope(look.v.shade, body)
                    } else {
                        Vec::new()
                    };
                    let clearance = Vec2::new(0.25, 0.30);
                    let mut upper = Parts::default();
                    upper.set_transform(Affine3A::from_scale_rotation_translation(
                        storybook_part(Plan::Biped, J::Torso),
                        Quat::IDENTITY,
                        body.torso,
                    ));
                    emit_torso(&mut upper, OuterwearAppearance::Apron, body, &support);
                    let mut lower = Parts::default();
                    emit_lower(
                        &mut lower,
                        OuterwearAppearance::Apron,
                        body,
                        clearance,
                        &retained,
                        &support,
                    );
                    let (low, high) = lower.matte.bounds();
                    // Independent world-space witness of the actual band. The
                    // old hip-local .045 root missed this entire overlap.
                    let band_y = body.torso.y + 0.057;
                    for x in [-0.04, 0.0, 0.04] {
                        let ray = Vec3::new(x, band_y, 0.0);
                        assert!(
                            upper.matte.outer_surface(ray, Vec3::Z).is_some(),
                            "fixture must hit the emitted waistband"
                        );
                        assert!(lower.matte.outer_surface(ray, Vec3::Z).is_some(),
                            "hanging panel overlaps waistband: {model:?}, woman {woman}, build {build}");
                        for y in [0.055, 0.075, 0.095] {
                            assert!(
                                lower
                                    .matte
                                    .outer_surface(Vec3::new(x, y, 0.0), Vec3::Z)
                                    .is_some(),
                                "no exposed underdress between bib and panel"
                            );
                        }
                    }
                    assert!(
                        high.y >= body.torso.y + 0.10,
                        "reserve overlap for independently moving torso and skirt drape"
                    );
                    let length = lower_length(OuterwearAppearance::Apron).unwrap();
                    assert!(
                        (low.y - (0.045 - length)).abs() < 0.0001,
                        "raising root does not move established hem/floor clearance"
                    );
                    assert!(low.z > clearance.y, "retained-layer clearance is preserved");
                }
            }
        }
    }

    #[test]
    fn apron_tunic_end_depths_keep_clearance_without_a_horizontal_shelf() {
        let support = [0.34, 0.34, 0.34, 0.20, 0.20, 0.20];
        let depth = apron_depth_envelope(&support, 0.03);
        for (actual, required) in depth.iter().zip(support) {
            assert!(*actual >= required);
        }
        for pair in depth.windows(2) {
            assert!((pair[1] - pair[0]).abs() <= 0.010501);
        }
        assert_eq!(apron_depth_envelope(&[0.2; 6], 0.03), vec![0.2; 6]);
    }

    fn wearer(woman: bool) -> HumanoidBody {
        HumanoidBody::resolve(&Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant { woman, ..default() }),
        ))
    }
    #[test]
    fn robe_upper_sleeve_covers_positive_shoulder_dome_and_overlaps_elbow() {
        let mut p = Parts::default();
        p.metal.blob(
            Vec3::new(0.0, 0.015, 0.0),
            Vec3::new(0.07, 0.065, 0.07),
            1,
            0.0,
            STEEL,
        );
        let before = p.metal.bounds();
        emit_robe_sleeve(&mut p, OuterwearAppearance::Robe, 0.31, 1.0, true);
        assert_eq!(p.metal.bounds(), before);
        let top = p
            .matte
            .outer_surface(Vec3::new(0.0, 0.015, 0.0), Vec3::Y)
            .unwrap();
        assert!(top.y >= 0.099, "closed cloth cap above existing deltoid");
        assert!(
            p.matte.bounds().0.y <= -0.309,
            "upper sleeve crosses the .30 elbow joint"
        );
    }

    #[test]
    fn mummy_forearm_windings_cover_retained_surface_without_replacing_it() {
        let mut p = Parts::default();
        p.metal
            .prism(Vec3::ZERO, Vec3::Y * -0.26, 0.05, 0.04, 12, STEEL, 0.0);
        let before = p.metal.bounds();
        emit_robe_sleeve(&mut p, OuterwearAppearance::MummyWrapping, 0.26, 1.0, false);
        assert_eq!(p.metal.bounds(), before);
        assert!(!p.matte.is_empty());
        let (low, high) = p.matte.bounds();
        assert!(low.y < -0.23 && high.y > -0.03);
        assert!(high.x > 0.05, "windings remain above retained arm armor");
    }

    #[test]
    fn apron_is_front_only_and_reserves_the_supplied_outer_layer() {
        for woman in [false, true] {
            let body = wearer(woman);
            let clearance = Vec2::new(0.25, 0.30);
            let mut p = Parts::default();
            emit_lower(
                &mut p,
                OuterwearAppearance::Apron,
                body,
                clearance,
                &[],
                &torso_garment(body, TorsoMaterial::Cloth, false),
            );
            assert!(!p.matte.is_empty());
            let (lo, hi) = p.matte.bounds();
            assert!(lo.is_finite() && hi.is_finite());
            assert!(lo.z > clearance.y, "apron clears retained hip layers");
            assert!(lo.y < -0.49 && hi.y > 0.04, "full knee-length panel");
            let mut cape = Parts::default();
            emit_lower(
                &mut cape,
                OuterwearAppearance::OperaCloak,
                body,
                clearance,
                &[],
                &torso_garment(body, TorsoMaterial::Cloth, false),
            );
            assert!(
                cape.matte.is_empty(),
                "capes use the shoulder drape, not apron geometry"
            );
        }
    }
    #[test]
    fn hanging_equipment_is_a_dedicated_synchronized_skirt_drape() {
        use crate::creatures::outfit::Equipment;
        use crate::creatures::rogue_tests::spawn_one;
        for name in ["apron", "mummy-wrapping"] {
            let base = Look::with_choice(
                CreatureModel::Human,
                17,
                Some(Variant {
                    woman: true,
                    ..default()
                }),
            )
            .dressed(Wardrobe::RoyalPrincess);
            let (_, native, _, _) = spawn_one(&base);
            let mut equipment = Equipment::default();
            equipment.select(name).unwrap();
            let (_, worn, _, _) = spawn_one(&base.equipped(equipment));
            assert_eq!(
                worn.skirt_layers.len(),
                native.skirt_layers.len() + 1,
                "{name}: retained layers plus independent equipment drape"
            );
            assert_eq!(
                worn.drapes
                    .iter()
                    .filter(|(entity, kind)| worn.skirt_layers.contains(entity)
                        && matches!(kind, Drape::Skirt(_)))
                    .count(),
                worn.skirt_layers.len()
            );
        }
    }

    #[test]
    fn apron_and_wrap_tails_clear_retained_flared_skirt_at_each_height() {
        for appearance in [
            OuterwearAppearance::Apron,
            OuterwearAppearance::MummyWrapping,
        ] {
            let body = wearer(true);
            let support: Vec<_> = (0..=64)
                .map(|row| {
                    let t = row as f32 / 64.0;
                    Vec3::new(0.0, 0.045 - 0.6 * t, 0.20 + 0.32 * t)
                })
                .collect();
            let mut p = Parts::default();
            emit_lower(
                &mut p,
                appearance,
                body,
                Vec2::new(0.25, 0.20),
                &support,
                &torso_garment(body, TorsoMaterial::Cloth, false),
            );
            let length = lower_length(appearance).unwrap();
            let x = if appearance == OuterwearAppearance::MummyWrapping {
                -0.10 * body.build
            } else {
                0.0
            };
            for t in [0.2, 0.5, 0.9] {
                let y = 0.045 - length * t;
                let hit = p
                    .matte
                    .outer_surface(Vec3::new(x, y, 0.0), Vec3::Z)
                    .expect("hanging front panel");
                let expected_front = 0.20 + 0.32 * (0.045 - y) / 0.6;
                assert!(
                    hit.z > expected_front + 0.025,
                    "{appearance:?}: {hit:?} must clear retained flare {expected_front}"
                );
            }
        }
    }

    #[test]
    fn wrappers_and_aprons_fit_both_adult_torso_shapes() {
        for woman in [false, true] {
            let body = wearer(woman);
            let mut support = torso_shape(body, Some(TorsoMaterial::Leather), false);
            support.inflate(0.025);
            for kind in [
                OuterwearAppearance::MummyWrapping,
                OuterwearAppearance::Apron,
                OuterwearAppearance::Robe,
            ] {
                let mut p = Parts::default();
                emit_torso(&mut p, kind, body, &support);
                assert!(!p.matte.is_empty());
                let (lo, hi) = p.matte.bounds();
                assert!(lo.is_finite() && hi.is_finite());
                assert!(hi.y > 0.36 && lo.y < 0.10);
            }
        }
    }
    #[test]
    fn robe_sleeve_clears_existing_armour_and_forms_a_real_bell() {
        let mut p = Parts::default();
        p.metal
            .prism(Vec3::ZERO, Vec3::Y * -0.26, 0.085, 0.065, 12, STEEL, 0.0);
        let before = p.metal.bounds();
        emit_robe_sleeve(&mut p, OuterwearAppearance::Robe, 0.26, 1.0, false);
        assert_eq!(p.metal.bounds(), before, "underlying armour remains intact");
        for y in [-0.025, -0.13, -0.245] {
            let origin = Vec3::Y * y;
            for direction in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
                let armour = p.metal.outer_surface(origin, direction).unwrap();
                let cloth = p.matte.outer_surface(origin, direction).unwrap();
                assert!((cloth - origin).length() > (armour - origin).length() + 0.009);
            }
        }
        let elbow = p.matte.outer_surface(Vec3::Y * -0.01, Vec3::X).unwrap().x;
        let wrist = p.matte.outer_surface(Vec3::Y * -0.25, Vec3::X).unwrap().x;
        assert!(
            wrist > elbow + 0.015,
            "bell cuff widens rather than repainting plate"
        );
        let mut other = Parts::default();
        emit_robe_sleeve(&mut other, OuterwearAppearance::Apron, 0.26, 1.0, false);
        assert!(other.matte.is_empty());
    }
}
