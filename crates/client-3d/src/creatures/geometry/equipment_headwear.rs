//! Interchangeable head-local equipment. The brow and hair allowance match
//! `headwear_parts`; callers apply the resolved head frame, never world offsets.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum HeadwearKind {
    LeatherHat,
    IronSkullCap,
    HardHat,
    Fedora,
    ConicalHat,
    DentedPot,
    CrystalHelmet,
    Plumed,
    Etched,
    Crested,
    Visored,
}

impl HeadwearKind {
    pub(crate) const ALL: [Self; 11] = [
        Self::LeatherHat,
        Self::IronSkullCap,
        Self::HardHat,
        Self::Fedora,
        Self::ConicalHat,
        Self::DentedPot,
        Self::CrystalHelmet,
        Self::Plumed,
        Self::Etched,
        Self::Crested,
        Self::Visored,
    ];
    pub(crate) const NAMES: [&'static str; 11] = [
        "leather-hat",
        "iron-skull-cap",
        "hard-hat",
        "fedora",
        "conical-hat",
        "dented-pot",
        "crystal-helmet",
        "plumed-helmet",
        "etched-helmet",
        "crested-helmet",
        "visored-helmet",
    ];

    pub(crate) fn parse(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .position(|candidate| *candidate == name)
            .map(|index| Self::ALL[index])
    }

    pub(crate) const fn covers_scalp(self) -> bool {
        true
    }

    pub(crate) const fn covers_ears(self) -> bool {
        matches!(
            self,
            Self::CrystalHelmet | Self::Plumed | Self::Etched | Self::Crested | Self::Visored
        )
    }
}

const BROW: f32 = 0.245;
const IRON: Vec3 = Vec3::new(0.30, 0.34, 0.39);
const BRASS: Vec3 = Vec3::new(0.76, 0.56, 0.25);
const LEATHER: Vec3 = Vec3::new(0.39, 0.22, 0.10);
const RED: Vec3 = Vec3::new(0.66, 0.09, 0.06);

fn ring_point(angle: f32, y: f32, radius: f32) -> Vec3 {
    Vec3::new(angle.sin() * radius, y, angle.cos() * radius * 1.04 - 0.012)
}

fn rim(b: &mut Builder, y: f32, radius: f32, width: f32, color: Vec3) {
    let path: Vec<_> = (0..=32)
        .map(|i| ring_point(i as f32 * TAU / 32.0, y, radius))
        .collect();
    tube(b, &path, &vec![(width, width); path.len()], color);
}

/// Open-bottom, lined crown: no solid sphere occupies the wearer's forehead.
/// Alternating triangular planes deliberately expose crystal's faceting.
fn crown(
    b: &mut Builder,
    profile: &[(f32, f32)],
    facets: usize,
    color: Vec3,
    crystal: bool,
    dent: bool,
) {
    let point = |row: usize, column: usize, inner: bool| {
        let (y, radius) = profile[row];
        let a = column as f32 * TAU / facets as f32;
        let damage = if dent && row > 0 {
            0.018 * ((a - 0.8).cos().max(0.0)).powi(12)
        } else {
            0.0
        };
        ring_point(
            a,
            y - damage * 0.5,
            radius - damage - if inner { 0.006 } else { 0.0 },
        )
    };
    for row in 0..profile.len() - 1 {
        for col in 0..facets {
            for inner in [false, true] {
                if crystal && inner {
                    continue;
                }
                let q = [
                    point(row, col, inner),
                    point(row, col + 1, inner),
                    point(row + 1, col + 1, inner),
                    point(row + 1, col, inner),
                ];
                let middle = (q[0] + q[2]) * 0.5;
                let reference = if inner {
                    middle + Vec3::new(middle.x, 0.0, middle.z + 0.012)
                } else {
                    Vec3::new(0.0, middle.y, -0.012)
                };
                let pigment = color
                    * if inner {
                        0.68
                    } else if crystal {
                        0.75 + ((col * 3 + row) % 5) as f32 * 0.09
                    } else {
                        1.0
                    };
                b.quad_out(q, reference, pigment);
            }
        }
    }
    for col in 0..facets {
        b.quad_out(
            [
                point(0, col, false),
                point(0, col, true),
                point(0, col + 1, true),
                point(0, col + 1, false),
            ],
            Vec3::Y * (BROW + 0.02),
            color,
        );
        let row = profile.len() - 1;
        let center = Vec3::new(0.0, profile[row].0, -0.012);
        b.tri_out(
            [point(row, col, false), point(row, col + 1, false), center],
            center - Vec3::Y,
            color,
        );
        if !crystal {
            b.tri_out(
                [point(row, col, true), center, point(row, col + 1, true)],
                center + Vec3::Y,
                color * 0.68,
            );
        }
    }
}

fn dome(p: &mut Parts, color: Vec3, pointed: bool) {
    crown(
        &mut p.metal,
        &[
            (BROW, 0.145),
            (0.29, 0.14),
            (0.335, 0.108),
            (if pointed { 0.405 } else { 0.365 }, 0.012),
        ],
        24,
        color,
        false,
        false,
    );
    rim(&mut p.metal, BROW, 0.146, 0.008, color * 1.25);
}

fn rivets(p: &mut Parts, color: Vec3) {
    for i in 0..12 {
        p.metal.blob(
            ring_point(i as f32 * TAU / 12.0, BROW + 0.013, 0.149),
            Vec3::splat(0.005),
            1,
            0.0,
            color,
        );
    }
}

/// Side guards leave the mouth and both eyes open, with an actual rear skirt.
fn guards(p: &mut Parts, color: Vec3, trim: Vec3) {
    for side in [-1.0, 1.0] {
        p.metal.extrude(
            &[
                Vec2::new(-0.075, 0.0),
                Vec2::new(0.06, 0.0),
                Vec2::new(0.065, -0.11),
                Vec2::new(0.025, -0.13),
                Vec2::new(-0.02, -0.055),
                Vec2::new(-0.075, -0.055),
            ],
            Vec3::new(side * 0.135, BROW, -0.02),
            Vec3::Z,
            Vec3::Y,
            0.012,
            color,
        );
        tube(
            &mut p.metal,
            &[
                Vec3::new(side * 0.143, BROW, 0.04),
                Vec3::new(side * 0.143, 0.135, 0.045),
                Vec3::new(side * 0.143, 0.115, 0.005),
            ],
            &[(0.004, 0.004); 3],
            trim,
        );
        p.metal.blob(
            Vec3::new(side * 0.148, 0.235, 0.005),
            Vec3::new(0.009, 0.014, 0.014),
            2,
            0.0,
            trim,
        );
    }
    p.metal.extrude(
        &[
            Vec2::new(-0.12, 0.0),
            Vec2::new(0.12, 0.0),
            Vec2::new(0.135, -0.07),
            Vec2::new(-0.135, -0.07),
        ],
        Vec3::new(0.0, BROW, -0.135),
        Vec3::X,
        Vec3::Y,
        0.013,
        color,
    );
}

fn leather_hat(p: &mut Parts) {
    tube(
        &mut p.matte,
        &[
            Vec3::new(0.0, BROW, -0.012),
            Vec3::new(0.0, 0.32, -0.025),
            Vec3::new(0.03, 0.395, -0.055),
            Vec3::new(0.12, 0.395, -0.075),
            Vec3::new(0.18, 0.305, -0.085),
        ],
        &[
            (0.15, 0.153),
            (0.13, 0.13),
            (0.075, 0.075),
            (0.032, 0.032),
            (0.004, 0.004),
        ],
        LEATHER,
    );
    rim(&mut p.matte, BROW, 0.152, 0.012, LEATHER * 1.25);
    for side in [-1.0, 1.0] {
        p.matte.extrude(
            &[
                Vec2::new(-0.045, 0.0),
                Vec2::new(0.055, 0.0),
                Vec2::new(0.025, -0.07),
            ],
            Vec3::new(side * 0.14, BROW, -0.04),
            Vec3::Z,
            Vec3::Y,
            0.009,
            LEATHER,
        );
        for i in 0..4 {
            let root = Vec3::new(
                side * (0.095 + i as f32 * 0.01),
                0.27 + i as f32 * 0.012,
                0.112 - i as f32 * 0.008,
            );
            ornament_parts::leaf(
                &mut p.metal,
                root,
                Vec3::new(side * 0.3, 1.0, 0.0),
                0.026,
                0.007,
                BRASS,
            );
        }
    }
}

fn fedora_top(a: f32) -> Vec3 {
    // Raised shoulders and a strong front pinch keep the crease visible from
    // gameplay elevation without lowering any surface into the scalp.
    Vec3::new(
        a.sin() * (0.125 - 0.045 * a.cos().max(0.0).powi(2)),
        0.405 + 0.034 * a.sin().abs(),
        a.cos() * 0.13 - 0.012,
    )
}

fn fedora(p: &mut Parts) {
    let brown = Vec3::new(0.36, 0.24, 0.15);
    p.matte.blob(
        Vec3::new(0.0, BROW, -0.012),
        Vec3::new(0.235, 0.012, 0.215),
        4,
        0.01,
        brown,
    );
    for i in 0..32 {
        let a = i as f32 * TAU / 32.0;
        let b = (i + 1) as f32 * TAU / 32.0;
        let upper = [fedora_top(a), fedora_top(b)];
        p.matte.quad_out(
            [
                ring_point(a, BROW, 0.144),
                ring_point(b, BROW, 0.144),
                upper[1],
                upper[0],
            ],
            Vec3::new(0.0, 0.30, -0.012),
            brown,
        );
        // A long center crease, rather than a single conical indentation.
        let crease = |v: Vec3| Vec3::new(0.0, 0.375 + 0.016 * ((v.z + 0.012) / 0.13).abs(), v.z);
        p.matte.quad_out(
            [upper[0], upper[1], crease(upper[1]), crease(upper[0])],
            Vec3::Y * 0.30,
            brown * 0.78,
        );
    }
    rim(&mut p.matte, BROW + 0.022, 0.143, 0.014, DARK * 2.0);
}

/// Sample the emitted cone rather than guessing its forward plane. Its bent
/// axis makes a constant-z ornament disappear into the cloth.
fn cone_inlay(p: &Parts, at: Vec2) -> Vec3 {
    p.matte
        .outer_surface(Vec3::new(at.x, at.y, -0.4), Vec3::Z)
        .expect("conical ornament lies on the crown")
        + Vec3::Z * 0.005
}

fn conical_hat(p: &mut Parts) {
    let blue = Vec3::new(0.09, 0.20, 0.43);
    headwear_parts::witch_hat(p, blue, BRASS * 0.7);
    let gold = Vec3::new(0.98, 0.78, 0.26);
    for (x, y, size) in [
        (-0.045, 0.34, 0.03),
        (0.0, 0.495, 0.021),
        (0.05, 0.30, 0.021),
    ] {
        let center = cone_inlay(p, Vec2::new(x, y));
        let points: Vec<_> = (0..10)
            .map(|i| {
                let a = i as f32 * TAU / 10.0;
                let r = if i % 2 == 0 { size } else { size * 0.43 };
                cone_inlay(p, Vec2::new(x + a.sin() * r, y + a.cos() * r))
            })
            .collect();
        for i in 0..10 {
            p.metal.tri_out(
                [center, points[i], points[(i + 1) % 10]],
                center - Vec3::Z,
                gold,
            );
        }
    }
    // A wide crescent has actual negative space between its horns.
    let crescent = |t: f32, inner: bool| {
        let a = -PI * 0.5 + t * PI;
        Vec2::new(
            0.047 - a.cos() * if inner { 0.008 } else { 0.043 },
            0.405 + a.sin() * 0.038,
        )
    };
    for i in 0..16 {
        let a = i as f32 / 16.0;
        let b = (i + 1) as f32 / 16.0;
        let q = [
            cone_inlay(p, crescent(a, false)),
            cone_inlay(p, crescent(b, false)),
            cone_inlay(p, crescent(b, true)),
            cone_inlay(p, crescent(a, true)),
        ];
        p.metal.quad_out(q, q[0] - Vec3::Z, gold);
    }
}

fn pot(p: &mut Parts) {
    crown(
        &mut p.metal,
        &[(BROW, 0.157), (0.32, 0.148), (0.378, 0.142), (0.388, 0.012)],
        16,
        IRON * 0.8,
        false,
        true,
    );
    rim(&mut p.metal, BROW, 0.16, 0.009, IRON * 1.35);
    for side in [-1.0, 1.0] {
        tube(
            &mut p.metal,
            &[
                Vec3::new(side * 0.151, 0.31, -0.055),
                Vec3::new(side * 0.21, 0.30, -0.055),
                Vec3::new(side * 0.21, 0.26, 0.035),
                Vec3::new(side * 0.151, 0.27, 0.035),
            ],
            &[(0.009, 0.009); 4],
            IRON,
        );
    }
    for i in 0..7 {
        let a = 0.35 + i as f32 * 0.85;
        p.metal.blob(
            ring_point(a, 0.28 + (i % 3) as f32 * 0.035, 0.148),
            Vec3::new(0.014, 0.02, 0.008),
            1,
            0.0,
            Vec3::new(0.43, 0.22, 0.10),
        );
    }
}

fn crystal(p: &mut Parts) {
    let ice = Vec3::new(0.77, 0.88, 0.98);
    p.ghost.with_opacity(0.25, |glass| {
        crown(
            glass,
            &[(BROW, 0.15), (0.315, 0.145), (0.37, 0.09), (0.405, 0.012)],
            10,
            ice,
            true,
            false,
        )
    });
    for i in 0..10 {
        let a = i as f32 * TAU / 10.0;
        tube(
            &mut p.satin,
            &[
                ring_point(a, BROW, 0.15),
                ring_point(a, 0.315, 0.145),
                ring_point(a, 0.37, 0.09),
                Vec3::new(0.0, 0.405, -0.012),
            ],
            &[(0.002, 0.002); 4],
            ice * 1.15,
        );
    }
    for side in [-1.0, 1.0] {
        p.ghost.with_opacity(0.25, |glass| {
            glass.extrude(
                &[
                    Vec2::new(-0.07, 0.0),
                    Vec2::new(0.05, 0.0),
                    Vec2::new(0.06, -0.09),
                    Vec2::new(0.0, -0.065),
                    Vec2::new(-0.08, -0.10),
                ],
                Vec3::new(side * 0.142, BROW, -0.015),
                Vec3::Z,
                Vec3::Y,
                0.012,
                ice,
            )
        });
    }
    rim(&mut p.satin, BROW, 0.15, 0.004, ice);
}

fn plume_feather(b: &mut Builder, lane: f32, layer: f32, white: bool) {
    let path: Vec<_> = (0..=10)
        .map(|i| {
            let t = i as f32 / 10.0;
            Vec3::new(
                lane * (0.02 + 0.095 * (t * PI * 0.72).sin()),
                0.365 + (0.16 + layer * 0.035) * (t * PI * 0.95).sin() - t * t * 0.035,
                0.025 - t * (0.22 + layer * 0.035) + if white { 0.035 } else { 0.0 },
            )
        })
        .collect();
    let radii: Vec<_> = (0..=10)
        .map(|i| {
            let t = i as f32 / 10.0;
            let width = (t * PI).sin().max(0.0) * if white { 0.030 } else { 0.040 };
            Vec2::new(width.max(0.001), (width * 0.12).max(0.001))
        })
        .collect();
    b.tube(&path, &radii, 6, &|_, _, t| {
        (if white { CREAM } else { RED }) * (0.86 + 0.14 * (1.0 - t))
    });
}

fn plume(p: &mut Parts) {
    dome(p, IRON, false);
    guards(p, IRON, BRASS);
    // Three overlapping broad layers make a supported red-and-white plume,
    // arching over the crown and sweeping down behind it, not upright quills.
    for layer in 0..3 {
        for feather in 0..9 {
            plume_feather(
                &mut p.matte,
                feather as f32 / 4.0 - 1.0,
                layer as f32,
                false,
            );
        }
    }
    for feather in 0..7 {
        plume_feather(&mut p.matte, feather as f32 / 3.0 - 1.0, 0.7, true);
    }
    p.metal.blob(
        Vec3::new(0.0, 0.36, 0.015),
        Vec3::new(0.026, 0.025, 0.026),
        2,
        0.0,
        BRASS,
    );
}

fn etched(p: &mut Parts) {
    let green = Vec3::new(0.12, 0.35, 0.28);
    dome(p, green, true);
    guards(p, green, BRASS);
    // Raised inlay follows the crown's curvature, with large readable curls.
    for side in [-1.0, 1.0] {
        let path: Vec<_> = (0..=24)
            .map(|i| {
                let t = i as f32 / 24.0;
                let a = side * (0.35 + t * 1.8);
                let y = 0.275 + 0.025 * (t * TAU * 1.5).sin();
                ring_point(a, y, 0.146)
            })
            .collect();
        tube(
            &mut p.metal,
            &path,
            &vec![(0.003, 0.003); path.len()],
            BRASS,
        );
    }
    tube(
        &mut p.metal,
        &[
            Vec3::new(0.0, 0.18, 0.145),
            Vec3::new(0.0, BROW, 0.145),
            Vec3::new(0.0, 0.335, 0.10),
            Vec3::new(0.0, 0.405, -0.012),
        ],
        &[(0.006, 0.006); 4],
        BRASS,
    );
}

fn crested(p: &mut Parts) {
    dome(p, IRON, false);
    guards(p, IRON, BRASS);
    // A rigid sagittal fan differs from the hanging, individual plumes.
    for i in 0..19 {
        let a = -0.85 + i as f32 * 1.75 / 18.0;
        let root = Vec3::new(0.0, 0.285 + a.cos() * 0.085, -0.02 + a.sin() * 0.115);
        let tip = Vec3::new(0.0, 0.285 + a.cos() * 0.245, -0.02 + a.sin() * 0.24);
        tube(
            &mut p.matte,
            &[root, tip],
            &[(0.018, 0.010), (0.024, 0.012)],
            RED * (0.85 + (i % 3) as f32 * 0.10),
        );
    }
    let path: Vec<_> = (0..=20)
        .map(|i| {
            let a = -0.9 + i as f32 * 1.85 / 20.0;
            Vec3::new(0.0, 0.285 + a.cos() * 0.087, -0.02 + a.sin() * 0.118)
        })
        .collect();
    tube(
        &mut p.metal,
        &path,
        &vec![(0.025, 0.012); path.len()],
        BRASS,
    );
}

fn visor(p: &mut Parts) {
    dome(p, IRON, true);
    guards(p, IRON, BRASS);
    // Two curved plates separated by a real horizontal eye opening.
    for (bottom, top) in [(0.085, 0.18), (0.213, 0.255)] {
        for i in 0..10 {
            let a = -1.0 + i as f32 * 0.2;
            let b = a + 0.2;
            let point =
                |angle: f32, y| Vec3::new(angle.sin() * 0.151, y, angle.cos() * 0.173 + 0.005);
            p.metal.quad_out(
                [
                    point(a, bottom),
                    point(b, bottom),
                    point(b, top),
                    point(a, top),
                ],
                Vec3::new(0.0, 0.17, 0.0),
                IRON * 1.15,
            );
        }
    }
    for y in [0.18, 0.213] {
        let path: Vec<_> = (0..=16)
            .map(|i| {
                let a = -1.0 + i as f32 / 8.0;
                Vec3::new(a.sin() * 0.151, y, a.cos() * 0.173 + 0.005)
            })
            .collect();
        tube(
            &mut p.metal,
            &path,
            &vec![(0.004, 0.004); path.len()],
            BRASS,
        );
    }
    for x in [-0.05, -0.025, 0.0, 0.025, 0.05] {
        for y in [0.11, 0.14, 0.165] {
            p.matte.blob(
                Vec3::new(x, y, 0.179 - x * x * 3.9),
                Vec3::new(0.004, 0.006, 0.002),
                1,
                0.0,
                DARK,
            );
        }
    }
}

pub(super) fn emit(p: &mut Parts, kind: HeadwearKind) {
    match kind {
        HeadwearKind::LeatherHat => leather_hat(p),
        HeadwearKind::IronSkullCap => {
            dome(p, IRON, false);
            rivets(p, IRON * 1.5);
        }
        HeadwearKind::HardHat => {
            dome(p, IRON * 0.85, false);
            p.metal.blob(
                Vec3::new(0.0, BROW, 0.04),
                Vec3::new(0.18, 0.012, 0.19),
                3,
                0.0,
                IRON,
            );
            tube(
                &mut p.metal,
                &[
                    Vec3::new(0.0, BROW, -0.163),
                    Vec3::new(0.0, 0.36, -0.075),
                    Vec3::new(0.0, 0.385, -0.012),
                    Vec3::new(0.0, 0.34, 0.115),
                ],
                &[(0.018, 0.012); 4],
                IRON * 1.4,
            );
            p.metal.torus(
                Vec3::new(0.0, 0.30, 0.149),
                Vec3::Z,
                0.043,
                0.009,
                20,
                6,
                BRASS,
            );
            p.satin.blob(
                Vec3::new(0.0, 0.30, 0.15),
                Vec3::new(0.037, 0.037, 0.015),
                2,
                0.0,
                Vec3::new(1.0, 0.83, 0.40),
            );
            rivets(p, BRASS);
        }
        HeadwearKind::Fedora => fedora(p),
        HeadwearKind::ConicalHat => conical_hat(p),
        HeadwearKind::DentedPot => pot(p),
        HeadwearKind::CrystalHelmet => crystal(p),
        HeadwearKind::Plumed => plume(p),
        HeadwearKind::Etched => etched(p),
        HeadwearKind::Crested => crested(p),
        HeadwearKind::Visored => visor(p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(p: &Parts) -> (Vec3, Vec3) {
        [&p.matte, &p.metal, &p.satin, &p.ghost, &p.sheer]
            .into_iter()
            .filter(|b| !b.is_empty())
            .map(|b| b.bounds())
            .fold(
                (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                |(lo, hi), (a, b)| (lo.min(a), hi.max(b)),
            )
    }

    #[test]
    fn disclosed_names_resolve_exactly_and_reject_unknown_equipment() {
        for (name, kind) in HeadwearKind::NAMES.into_iter().zip(HeadwearKind::ALL) {
            assert_eq!(HeadwearKind::parse(name), Some(kind));
            assert!(kind.covers_scalp());
        }
        assert_eq!(HeadwearKind::parse("helmet"), None);
        assert_eq!(HeadwearKind::parse("visored helmet"), None);
        assert!(!HeadwearKind::IronSkullCap.covers_ears());
        assert!(HeadwearKind::Visored.covers_ears());
    }

    #[test]
    fn every_study_has_finite_head_local_geometry() {
        assert_eq!(HeadwearKind::ALL.len(), HeadwearKind::NAMES.len());
        for kind in HeadwearKind::ALL {
            let mut p = Parts::default();
            emit(&mut p, kind);
            let (lo, hi) = bounds(&p);
            assert!(lo.is_finite() && hi.is_finite(), "{kind:?}");
            assert!(
                lo.x < -0.10 && hi.x > 0.10 && hi.y > 0.32,
                "{kind:?}: {lo:?}..{hi:?}"
            );
            assert!(
                lo.y > -0.2 && hi.y < 0.75 && hi.x < 0.4 && lo.x > -0.4,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn study_silhouettes_retain_their_defining_features() {
        let get = |kind| {
            let mut p = Parts::default();
            emit(&mut p, kind);
            bounds(&p)
        };
        let cap = get(HeadwearKind::IronSkullCap);
        let fedora = get(HeadwearKind::Fedora);
        let pot = get(HeadwearKind::DentedPot);
        assert!(fedora.1.x > cap.1.x + 0.06, "fedora has a broad brim");
        assert!(pot.1.x > cap.1.x + 0.04, "pot has side handles");
        assert!(get(HeadwearKind::ConicalHat).1.y > cap.1.y + 0.20);
        assert!(get(HeadwearKind::Crested).1.y > cap.1.y + 0.12);
        assert!(
            get(HeadwearKind::LeatherHat).1.x > -get(HeadwearKind::LeatherHat).0.x + 0.015,
            "leather tip bends sideways"
        );
    }

    #[test]
    fn visor_leaves_eye_slit_open_and_covers_lower_face() {
        let mut p = Parts::default();
        emit(&mut p, HeadwearKind::Visored);
        for x in [-0.045, 0.045] {
            assert!(p
                .metal
                .outer_surface(Vec3::new(x, 0.196, 0.0), Vec3::Z)
                .is_none());
            assert!(p
                .metal
                .outer_surface(Vec3::new(x, 0.14, 0.0), Vec3::Z)
                .is_some());
        }
    }

    #[test]
    fn fedora_has_raised_shoulders_and_a_long_center_crease() {
        let mut p = Parts::default();
        fedora(&mut p);
        for z in [-0.06, 0.04] {
            let center = p
                .matte
                .outer_surface(Vec3::new(0.0, 0.30, z), Vec3::Y)
                .unwrap();
            let shoulder = p
                .matte
                .outer_surface(Vec3::new(0.075, 0.30, z), Vec3::Y)
                .unwrap();
            assert!(center.y >= 0.375, "crease cannot reduce scalp clearance");
            assert!(shoulder.y > center.y + 0.018, "crease must read from above");
        }
    }

    #[test]
    fn wizard_stars_and_crescent_are_above_the_actual_cone() {
        let mut p = Parts::default();
        conical_hat(&mut p);
        for (x, y) in [(-0.045, 0.34), (0.0, 0.495), (0.05, 0.30), (0.022, 0.405)] {
            let origin = Vec3::new(x, y, -0.4);
            let cloth = p.matte.outer_surface(origin, Vec3::Z).unwrap();
            let inlay = p.metal.outer_surface(origin, Vec3::Z).unwrap();
            assert!(inlay.z > cloth.z + 0.001, "ornament buried at {x}, {y}");
            assert!(inlay.z < cloth.z + 0.015, "ornament floats off the cone");
        }
        assert!(
            p.metal
                .outer_surface(Vec3::new(0.045, 0.405, -0.4), Vec3::Z)
                .is_none(),
            "crescent's opening stays unfilled"
        );
    }

    #[test]
    fn plume_is_a_broad_connected_sweep_rather_than_sparse_shafts() {
        let mut p = Parts::default();
        plume(&mut p);
        let (lo, hi) = p.matte.bounds();
        assert!(hi.x - lo.x > 0.22);
        assert!(
            lo.z < -0.25 && hi.y > 0.56,
            "plume arches then sweeps behind the helmet"
        );
        for i in -4..=4 {
            assert!(
                p.matte
                    .outer_surface(Vec3::new(i as f32 * 0.02, 0.46, -0.4), Vec3::Z)
                    .is_some(),
                "plume has a gap in its broad mass at {i}"
            );
        }
    }

    #[test]
    fn crystal_uses_translucent_facets_and_solid_facet_edges() {
        let mut p = Parts::default();
        emit(&mut p, HeadwearKind::CrystalHelmet);
        assert!(!p.ghost.is_empty());
        assert!(!p.satin.is_empty());
        assert!(p.metal.is_empty());
        let mut iron = Parts::default();
        emit(&mut iron, HeadwearKind::IronSkullCap);
        assert!(iron.ghost.is_empty());
        assert!(!iron.metal.is_empty());
    }
}
