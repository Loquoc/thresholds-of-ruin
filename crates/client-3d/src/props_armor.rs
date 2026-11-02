//! Sculpted tabletop armor: known construction, not enchantment or worn anatomy.
use super::*;

pub(crate) fn append(p: &mut Parts, kind: Kind) {
    const OUTLINE: [Vec2; 11] = [
        Vec2::new(-0.17, -0.25),
        Vec2::new(0.17, -0.25),
        Vec2::new(0.18, 0.06),
        Vec2::new(0.3, 0.1),
        Vec2::new(0.28, 0.2),
        Vec2::new(0.1, 0.25),
        Vec2::new(0.0, 0.2),
        Vec2::new(-0.1, 0.25),
        Vec2::new(-0.28, 0.2),
        Vec2::new(-0.3, 0.1),
        Vec2::new(-0.18, 0.06),
    ];
    let leathern = matches!(
        kind,
        Kind::LeatherArmor | Kind::StuddedLeather | Kind::RingMail | Kind::SplintMail
    );
    let base = match kind {
        Kind::LeatherArmor | Kind::StuddedLeather => LEATHER * 1.15,
        Kind::RingMail | Kind::SplintMail => LEATHER * 0.85,
        Kind::ChainMail => IRON * 0.95,
        Kind::PlateMail => STEEL,
        _ => IRON * 0.8,
    };
    let body = if leathern { &mut p.matte } else { &mut p.metal };
    body.extrude(
        &OUTLINE,
        Vec3::new(0.0, 0.025, 0.0),
        Vec3::X,
        Vec3::NEG_Z,
        0.05,
        base,
    );
    let top = 0.052;
    let grid = |rows: usize, cols: usize, dx: f32, dz: f32| {
        (0..rows).flat_map(move |row| {
            (0..cols).filter_map(move |col| {
                let x = -0.13 + col as f32 * dx + if row % 2 == 1 { dx * 0.5 } else { 0.0 };
                (x <= 0.15).then(|| Vec2::new(x, 0.18 - row as f32 * dz))
            })
        })
    };
    match kind {
        Kind::RingMail => {
            for at in grid(6, 5, 0.065, 0.062) {
                p.metal.torus(
                    Vec3::new(at.x, top, at.y),
                    Vec3::Y,
                    0.024,
                    0.0045,
                    10,
                    4,
                    STEEL,
                );
            }
        }
        Kind::StuddedLeather => {
            for at in grid(8, 6, 0.054, 0.048) {
                let at = Vec3::new(at.x, top - 0.002, at.y);
                p.metal
                    .prism(at, at + Vec3::Y * 0.008, 0.008, 0.006, 8, BRONZE, 0.0);
            }
        }
        Kind::ScaleMail => {
            for at in grid(9, 6, 0.054, 0.045) {
                p.metal.extrude(
                    &[
                        Vec2::new(-0.024, 0.02),
                        Vec2::new(0.024, 0.02),
                        Vec2::new(0.02, -0.012),
                        Vec2::new(0.0, -0.03),
                        Vec2::new(-0.02, -0.012),
                    ],
                    Vec3::new(at.x, top + 0.002, at.y),
                    Vec3::X,
                    Vec3::NEG_Z,
                    0.004,
                    STEEL * (0.9 + 0.1 * (at.y * 40.0).sin().abs()),
                );
            }
        }
        Kind::SplintMail => {
            for k in 0..6 {
                let x = -0.125 + k as f32 * 0.05;
                p.metal.cuboid(
                    Vec3::new(x, top + 0.003, -0.03),
                    Vec3::new(0.018, 0.004, 0.17),
                    Quat::IDENTITY,
                    STEEL,
                );
            }
        }
        Kind::BandedMail => {
            for k in 0..7 {
                p.metal.cuboid(
                    Vec3::new(0.0, top + 0.003 + 0.002 * k as f32, 0.15 - k as f32 * 0.058),
                    Vec3::new(0.17, 0.004, 0.032),
                    Quat::IDENTITY,
                    STEEL * (1.0 - 0.03 * k as f32),
                );
            }
        }
        Kind::ChainMail => {
            for at in grid(9, 10, 0.032, 0.039) {
                p.metal.torus(
                    Vec3::new(at.x, top + 0.002, at.y),
                    Vec3::Y,
                    0.0115,
                    0.0027,
                    8,
                    3,
                    STEEL * 0.94,
                );
            }
        }
        Kind::PlateMail => {
            p.metal.cuboid(
                Vec3::new(0.0, top + 0.003, -0.24),
                Vec3::new(0.17, 0.005, 0.012),
                Quat::IDENTITY,
                GOLD,
            );
        }
        _ => {}
    }
    p.matte.cuboid(
        Vec3::new(0.0, 0.054, 0.17),
        Vec3::new(0.172, 0.01, 0.022),
        Quat::IDENTITY,
        LEATHER,
    );
    p.metal.cuboid(
        Vec3::new(0.0, 0.066, 0.17),
        Vec3::new(0.03, 0.006, 0.026),
        Quat::IDENTITY,
        BRONZE,
    );
    p.matte.torus(
        Vec3::new(0.0, 0.05, -0.2),
        Vec3::Y,
        0.075,
        0.012,
        9,
        4,
        LEATHER,
    );
    // A rolled sleeve opening, not a flat shirt tab. This is loose ground armor,
    // never the character's equipped costume or an implied extra item.
    for sign in [-1.0, 1.0] {
        let cuff = Vec3::new(sign * 0.292, 0.07, -0.145);
        let material = if leathern { &mut p.matte } else { &mut p.metal };
        material.torus(cuff, Vec3::X, 0.041, 0.008, 10, 4, base * 1.1);
        p.matte.prism(
            cuff - Vec3::X * 0.002,
            cuff + Vec3::X * 0.002,
            0.033,
            0.033,
            10,
            LEATHER * 0.33,
            0.0,
        );
    }
    if kind == Kind::LeatherArmor {
        for x in [-0.12, 0.12] {
            p.matte.prism(
                Vec3::new(x, 0.055, -0.12),
                Vec3::new(x, 0.055, 0.12),
                0.004,
                0.004,
                5,
                LEATHER * 0.55,
                0.0,
            );
            for z in [-0.12, -0.075, -0.03, 0.015, 0.06, 0.105] {
                p.matte.prism(
                    Vec3::new(x - 0.008, 0.058, z),
                    Vec3::new(x + 0.008, 0.058, z),
                    0.002,
                    0.002,
                    4,
                    Vec3::new(0.63, 0.48, 0.29),
                    0.0,
                );
            }
        }
    }
    if kind == Kind::PlateMail {
        p.metal.prism(
            Vec3::new(0.0, 0.058, -0.15),
            Vec3::new(0.0, 0.058, 0.12),
            0.005,
            0.005,
            5,
            STEEL * 0.78,
            0.0,
        );
        p.metal.prism(
            Vec3::new(-0.16, 0.056, 0.23),
            Vec3::new(0.16, 0.056, 0.23),
            0.008,
            0.008,
            7,
            STEEL,
            0.0,
        );
    }
    for builder in [&mut p.matte, &mut p.metal] {
        fold(builder);
    }
}

#[derive(Clone, Copy)]
struct Vertex {
    position: Vec3,
    normal: Vec3,
    color: [f32; 4],
    shade: f32,
}
impl Vertex {
    fn midpoint(self, other: Self) -> Self {
        Self {
            position: self.position.lerp(other.position, 0.5),
            normal: self.normal.lerp(other.normal, 0.5).normalize_or_zero(),
            color: std::array::from_fn(|i| (self.color[i] + other.color[i]) * 0.5),
            shade: (self.shade + other.shade) * 0.5,
        }
    }
}

/// Arch the chest, subdividing only long edges so details follow the surface
/// instead of disappearing through a raised center. Transform normals with the
/// inverse-transpose Jacobian; lighting must agree with the actual sculpt.
fn fold(builder: &mut Builder) {
    let points = std::mem::take(&mut builder.positions);
    let normals = std::mem::take(&mut builder.normals);
    let colors = std::mem::take(&mut builder.colors);
    let shades = std::mem::take(&mut builder.shades);
    for i in (0..points.len()).step_by(3) {
        let tri = std::array::from_fn(|j| Vertex {
            position: Vec3::from_array(points[i + j]),
            normal: Vec3::from_array(normals[i + j]),
            color: colors[i + j],
            shade: shades[i + j],
        });
        fold_triangle(builder, tri, 0);
    }
}
fn fold_triangle(builder: &mut Builder, tri: [Vertex; 3], depth: u8) {
    let edge = (0..3)
        .max_by(|a, b| {
            tri[*a]
                .position
                .distance_squared(tri[(*a + 1) % 3].position)
                .total_cmp(
                    &tri[*b]
                        .position
                        .distance_squared(tri[(*b + 1) % 3].position),
                )
        })
        .unwrap();
    let a = tri[edge];
    let b = tri[(edge + 1) % 3];
    let c = tri[(edge + 2) % 3];
    if depth < 6 && a.position.distance_squared(b.position) > 0.09_f32.powi(2) {
        let m = a.midpoint(b);
        fold_triangle(builder, [a, m, c], depth + 1);
        fold_triangle(builder, [m, b, c], depth + 1);
        return;
    }
    for mut v in tri {
        let x = v.position.x / 0.24;
        let z = (v.position.z + 0.015) / 0.30;
        let hx = (1.0 - x * x).max(0.0);
        let hz = (1.0 - z.powi(4)).max(0.0);
        let dx = if x.abs() < 1.0 { -2.0 * x / 0.24 } else { 0.0 };
        let dz = if z.abs() < 1.0 {
            -4.0 * z.powi(3) / 0.30
        } else {
            0.0
        };
        v.position.y += 0.09 * hx * hz;
        v.normal = Vec3::new(
            v.normal.x - 0.09 * dx * hz * v.normal.y,
            v.normal.y,
            v.normal.z - 0.09 * hx * dz * v.normal.y,
        )
        .normalize_or_zero();
        builder.positions.push(v.position.to_array());
        builder.normals.push(v.normal.to_array());
        builder.colors.push(v.color);
        builder.shades.push(v.shade);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn armor_is_dimensional_bounded_nonmagical_and_keeps_distinct_construction() {
        let mut signatures = Vec::new();
        for kind in [
            Kind::LeatherArmor,
            Kind::StuddedLeather,
            Kind::RingMail,
            Kind::ScaleMail,
            Kind::ChainMail,
            Kind::SplintMail,
            Kind::BandedMail,
            Kind::PlateMail,
        ] {
            let mut p = Parts::default();
            append(&mut p, kind);
            let points: Vec<_> = p
                .matte
                .positions
                .iter()
                .chain(&p.metal.positions)
                .copied()
                .collect();
            assert!(points.iter().any(|v| v[1] > 0.12), "{kind:?}: flat armor");
            assert!(
                points.iter().all(|v| v.iter().all(|f| f.is_finite())
                    && v[0].abs() <= 0.305
                    && v[2].abs() <= 0.30
                    && v[1] >= -0.001
                    && v[1] <= 0.19),
                "{kind:?}: bounds"
            );
            assert!(p.glow.is_empty() && p.accent.is_none());
            assert!(
                !signatures.contains(&points),
                "{kind:?}: lost material construction"
            );
            signatures.push(points);
            for builder in [&p.matte, &p.metal] {
                assert_eq!(builder.positions.len(), builder.normals.len());
                assert!(builder
                    .normals
                    .iter()
                    .all(|n| (Vec3::from_array(*n).length() - 1.0).abs() < 0.001));
                assert!(
                    builder.positions.len() < 40_000,
                    "bounded cached prop geometry"
                );
            }
        }
    }
}
