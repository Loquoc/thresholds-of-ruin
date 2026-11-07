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

// Crown-to-jaw envelope: y, lateral radius, forward radius, front edge angle.
// The eye-height opening stays wide; below it the plates turn toward the cheeks.
const CHEEK_ROWS: [(f32, f32, f32, f32); 5] = [
    (BROW, 0.145, 0.1508, 0.80),
    (0.195, 0.140, 0.154, 0.83),
    (0.155, 0.137, 0.150, 0.78),
    (0.115, 0.125, 0.140, 0.67),
    (0.085, 0.110, 0.125, 0.70),
];
const CHEEK_COLUMNS: usize = 8;

fn cheek_point(side: f32, row: usize, column: usize, inner: bool) -> Vec3 {
    let (y, rx, rz, front) = CHEEK_ROWS[row];
    let angle = front + (2.05 - front) * column as f32 / CHEEK_COLUMNS as f32;
    let inset = if inner { 0.006 } else { 0.0 };
    Vec3::new(
        side * angle.sin() * (rx - inset),
        y,
        angle.cos() * (rz - inset) - 0.012,
    )
}

/// Closed curved plates frame the cheeks and taper around the jaw, without
/// covering the eyes or mouth. Their top edge shares the crown's outer surface.
fn guards(p: &mut Parts, color: Vec3, trim: Vec3) {
    for side in [-1.0, 1.0] {
        for row in 0..CHEEK_ROWS.len() - 1 {
            for column in 0..CHEEK_COLUMNS {
                for inner in [false, true] {
                    let q = [
                        cheek_point(side, row, column, inner),
                        cheek_point(side, row, column + 1, inner),
                        cheek_point(side, row + 1, column + 1, inner),
                        cheek_point(side, row + 1, column, inner),
                    ];
                    let center = (q[0] + q[2]) * 0.5;
                    let axis = Vec3::new(0.0, center.y, -0.012);
                    p.metal.quad_out(
                        q,
                        if inner { center * 2.0 - axis } else { axis },
                        color * if inner { 0.68 } else { 1.0 },
                    );
                }
            }
            for column in [0, CHEEK_COLUMNS] {
                p.metal.quad_out(
                    [
                        cheek_point(side, row, column, false),
                        cheek_point(side, row + 1, column, false),
                        cheek_point(side, row + 1, column, true),
                        cheek_point(side, row, column, true),
                    ],
                    cheek_point(side, row, if column == 0 { 1 } else { column - 1 }, false),
                    color,
                );
            }
        }
        for row in [0, CHEEK_ROWS.len() - 1] {
            for column in 0..CHEEK_COLUMNS {
                let q = [
                    cheek_point(side, row, column, false),
                    cheek_point(side, row, column + 1, false),
                    cheek_point(side, row, column + 1, true),
                    cheek_point(side, row, column, true),
                ];
                p.metal.quad_out(
                    q,
                    q[0] + Vec3::Y * if row == 0 { -0.01 } else { 0.01 },
                    color,
                );
            }
        }
        let edge: Vec<_> = (0..CHEEK_ROWS.len())
            .map(|row| cheek_point(side, row, 0, false))
            .collect();
        tube(
            &mut p.metal,
            &edge,
            &vec![(0.0035, 0.0035); edge.len()],
            trim,
        );
        p.metal.blob(
            cheek_point(side, 0, 4, false) + Vec3::new(side * 0.003, -0.012, 0.0),
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
    // A low felt crown, tapered sides and paired front pinches distinguish a
    // fedora from a tall straight-sided top hat. The crease stays above the
    // actual head-local cranium, whose crown is at approximately y=0.284.
    Vec3::new(
        a.sin() * (0.115 - 0.055 * a.cos().max(0.0).powi(2)),
        0.335 + 0.025 * a.sin().abs(),
        a.cos() * 0.125 - 0.012,
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
        let crease = |v: Vec3| Vec3::new(0.0, 0.312 + 0.016 * ((v.z + 0.012) / 0.125).abs(), v.z);
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
        // Project a fine grid, not just the outline: a broad flat quad
        // between fitted edges cuts through the curved crown in its middle.
        // Keep the same 5 mm surface offset rather than floating the entire
        // crescent forward to conceal that chord-intersection defect.
        for column in 0..8 {
            let left = column as f32 / 8.0;
            let right = (column + 1) as f32 / 8.0;
            let at = |t, across| crescent(t, false).lerp(crescent(t, true), across);
            let q = [
                cone_inlay(p, at(a, left)),
                cone_inlay(p, at(b, left)),
                cone_inlay(p, at(b, right)),
                cone_inlay(p, at(a, right)),
            ];
            p.metal.quad_out(q, q[0] - Vec3::Z, gold);
        }
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
    let profile = [(BROW, 0.15), (0.315, 0.145), (0.37, 0.09), (0.405, 0.012)];
    // Broad differently angled glass facets carry the material. Continuous
    // opaque longitude ribs made the transparent shell read as a wire cage.
    for row in 0..profile.len() - 1 {
        for column in 0..10 {
            let a = column as f32 * TAU / 10.0;
            let b = (column + 1) as f32 * TAU / 10.0;
            let q = [
                ring_point(a, profile[row].0, profile[row].1),
                ring_point(b, profile[row].0, profile[row].1),
                ring_point(b, profile[row + 1].0, profile[row + 1].1),
                ring_point(a, profile[row + 1].0, profile[row + 1].1),
            ];
            let radial = Vec3::new(((a + b) * 0.5).sin(), 0.0, ((a + b) * 0.5).cos());
            let center = (q[0] + q[1] + q[2] + q[3]) * 0.25 + radial * 0.006;
            let inside = Vec3::new(0.0, center.y, -0.012);
            for face in 0..4 {
                let pigment = match (column + row + face) % 4 {
                    0 => Vec3::new(0.58, 0.75, 0.96),
                    1 => Vec3::new(0.84, 0.90, 1.0),
                    2 => Vec3::new(0.69, 0.74, 0.94),
                    _ => ice,
                };
                let opacity = if face == 1 { 0.62 } else { 0.40 };
                let triangle = [q[face], q[(face + 1) % 4], center];
                p.ghost
                    .with_opacity(opacity, |glass| glass.tri_out(triangle, inside, pigment));
                // Sparse small plane-shaped gleams, rather than a painted
                // outline around every pane, keep the glass visibly faceted.
                if (column + row * 3) % 4 == 0 && face == 2 {
                    let glint = [
                        center.lerp(q[face], 0.72),
                        center.lerp(q[(face + 1) % 4], 0.35),
                        center.lerp(q[face], 0.25),
                    ]
                    .map(|v| v + radial * 0.001);
                    p.satin.tri_out(glint, inside, ice * 1.12);
                }
            }
        }
    }
    // Close the tiny summit without introducing a second overlapping wall.
    for column in 0..10 {
        let a = column as f32 * TAU / 10.0;
        let b = (column + 1) as f32 * TAU / 10.0;
        p.ghost.with_opacity(0.4, |glass| {
            glass.tri_out(
                [
                    ring_point(a, 0.405, 0.012),
                    ring_point(b, 0.405, 0.012),
                    Vec3::new(0.0, 0.409, -0.012),
                ],
                Vec3::Y * 0.38,
                ice,
            )
        });
    }

    for side in [-1.0, 1.0] {
        p.ghost.with_opacity(0.40, |glass| {
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
    rim(&mut p.satin, BROW, 0.15, 0.0025, ice);
}

fn plume_path(lane: f32, layer: f32, white: bool) -> Vec<Vec3> {
    // Different lengths and asymmetric arches prevent adjacent feather tips
    // from joining into the straight edge of a rigid folding fan.
    let stagger = (lane * 5.7 + layer * 2.3).sin();
    let arch = 0.17 + layer * 0.035 - 0.05 * lane.abs() + stagger * 0.018;
    (0..=16)
        .map(|i| {
            let t = i as f32 / 16.0;
            Vec3::new(
                lane * (0.02 + 0.105 * (t * PI * 0.72).sin()) + stagger * 0.015 * t * t,
                0.365 + arch * (t * PI * 0.95).sin() - t * t * (0.035 + lane.abs() * 0.025),
                0.025 - t * (0.235 + layer * 0.035 + stagger * 0.028)
                    + if white { 0.045 } else { 0.0 },
            )
        })
        .collect()
}

fn plume_feather(b: &mut Builder, lane: f32, layer: f32, white: bool) {
    let path = plume_path(lane, layer, white);
    let radii: Vec<_> = (0..=16)
        .map(|i| {
            let t = i as f32 / 16.0;
            let notch = if i > 3 && i % 3 == 0 { 0.78 } else { 1.0 };
            let width = (t * PI).sin().max(0.0) * if white { 0.025 } else { 0.032 } * notch;
            Vec2::new(width.max(0.001), (width * 0.12).max(0.001))
        })
        .collect();
    let pigment = (if white { CREAM } else { RED }) * (0.90 + 0.10 * (lane * 7.0 + layer).cos());
    b.tube(&path, &radii, 6, &|_, _, t| {
        pigment * (0.86 + 0.14 * (1.0 - t))
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
    // The shared human/centaur face has eyes at y=0.16, not at the
    // eyebrow ridge (y~0.196). Keep the brass lips outside the eye opening.
    for (bottom, top) in [(0.085, 0.14), (0.18, 0.255)] {
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
    for y in [0.14, 0.18] {
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
        for y in [0.10, 0.115, 0.13] {
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
    fn cheek_plates_wrap_forward_taper_and_join_the_crown() {
        let mut plates = Parts::default();
        guards(&mut plates, IRON, BRASS);
        let mut crown = Parts::default();
        dome(&mut crown, IRON, false);
        for side in [-1.0, 1.0] {
            // A front-facing cheek plane, not an edge-on vertical side bar.
            for y in [0.10, 0.115, 0.13] {
                assert!(plates
                    .metal
                    .outer_surface(Vec3::new(side * 0.09, y, 0.0), Vec3::Z)
                    .is_some());
            }
            let temple = plates
                .metal
                .outer_surface(Vec3::new(0.0, 0.195, -0.012), Vec3::X * side)
                .unwrap();
            let jaw = plates
                .metal
                .outer_surface(Vec3::new(0.0, 0.09, -0.012), Vec3::X * side)
                .unwrap();
            assert!(
                temple.x.abs() - jaw.x.abs() > 0.02,
                "plate follows the narrowing jaw"
            );
            for angle in [1.0f32, 1.4, 1.8] {
                let ray = Vec3::new(side * angle.sin(), 0.0, angle.cos());
                let root = plates
                    .metal
                    .outer_surface(Vec3::new(0.0, BROW - 0.001, -0.012), ray)
                    .unwrap();
                let cap = crown
                    .metal
                    .outer_surface(Vec3::new(0.0, BROW + 0.001, -0.012), ray)
                    .unwrap();
                assert!(
                    (root - cap).length() < 0.012,
                    "plate must join the crown rim"
                );
            }
        }
    }

    #[test]
    fn cheek_plates_clear_actual_faces_without_detached_side_bar_spacing() {
        let mut plates = Parts::default();
        guards(&mut plates, IRON, BRASS);
        for kind in [CreatureModel::Human, CreatureModel::Centaur] {
            for woman in [false, true] {
                for build in 0..3 {
                    let look = Look::with_choice(
                        kind,
                        7,
                        Some(Variant {
                            woman,
                            build,
                            ..default()
                        }),
                    );
                    let mut face = Parts::default();
                    human_face(&mut face, &look, true);
                    for side in [-1.0, 1.0] {
                        for y in [0.10, 0.13, 0.17, 0.21] {
                            for angle in [0.95f32, 1.25, 1.6] {
                                let ray = Vec3::new(side * angle.sin(), 0.0, angle.cos());
                                let origin = Vec3::new(0.0, y, -0.012);
                                let skin = face
                                    .matte
                                    .outer_surface(origin, ray)
                                    .expect("actual cheek/temple support");
                                let outer = plates
                                    .metal
                                    .outer_surface(origin, ray)
                                    .expect("continuous cheek plate");
                                // Include the 6 mm lining thickness, plus clearance.
                                let gap = (outer - skin).dot(ray);
                                assert!(gap > 0.010 && gap < 0.065,
                                    "{kind:?}, woman={woman}, build={build}: cheek gap {gap} at y={y}, angle={angle}");
                            }
                        }
                        for y in [0.154, 0.16, 0.166] {
                            assert!(
                                plates
                                    .metal
                                    .outer_surface(Vec3::new(side * 0.044, y, 0.0), Vec3::Z)
                                    .is_none(),
                                "cheek plate must not cover eyes"
                            );
                        }
                    }
                    assert!(
                        plates
                            .metal
                            .outer_surface(Vec3::new(0.0, 0.08, 0.0), Vec3::Z)
                            .is_none(),
                        "mouth stays open"
                    );
                }
            }
        }
    }

    #[test]
    fn visor_opens_at_actual_human_and_centaur_eyes_and_clears_faces() {
        for kind in [CreatureModel::Human, CreatureModel::Centaur] {
            for woman in [false, true] {
                for build in 0..3 {
                    let look = Look::with_choice(
                        kind,
                        7,
                        Some(Variant {
                            woman,
                            build,
                            ..default()
                        }),
                    );
                    let mut face = Parts::default();
                    human_face(&mut face, &look, true);
                    let mut helmet = Parts::default();
                    emit(&mut helmet, HeadwearKind::Visored);
                    // Independent fixtures from human_face/soft_eyes, shared by
                    // both rigs. Face and equipment use the same head transform.
                    // Check a pupil-sized window, not merely one assumed slit ray.
                    for side in [-1.0, 1.0] {
                        for dx in [-0.006, 0.0, 0.006] {
                            for dy in [-0.006, 0.0, 0.006] {
                                let origin = Vec3::new(side * 0.038 + dx, 0.16 + dy, 0.0);
                                let eye = face
                                    .matte
                                    .outer_surface(origin, Vec3::Z)
                                    .expect("actual emitted eye-region fixture");
                                assert!(eye.z > 0.05, "fixture must reach the front of the face");
                                for material in [&helmet.metal, &helmet.matte] {
                                    assert!(material.outer_surface(eye, Vec3::Z).is_none(),
                                        "{kind:?}, woman={woman}, build={build}: blocked eye ray at {origin:?}");
                                }
                            }
                        }
                    }
                    // Preserve lower-face protection and an air gap in front of
                    // cheeks/nose/brows, including the former erroneous slit oracle.
                    for y in [0.10, 0.12, 0.196, 0.22] {
                        for x in [-0.038, 0.0, 0.038] {
                            let origin = Vec3::new(x, y, 0.0);
                            let skin = face
                                .matte
                                .outer_surface(origin, Vec3::Z)
                                .expect("actual face fixture under plate");
                            let plate = helmet
                                .metal
                                .outer_surface(origin, Vec3::Z)
                                .expect("visor retains upper and lower plates");
                            assert!(plate.z - skin.z > 0.015,
                                "{kind:?}, woman={woman}, build={build}: plate meets face at {origin:?}");
                        }
                    }
                }
            }
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
            assert!(center.y >= 0.312, "crease retains the fitted crown floor");
            assert!(shoulder.y > center.y + 0.018, "crease must read from above");
            assert!(
                shoulder.y < 0.362,
                "fedora shoulders must not become a tall top-hat crown"
            );
        }
    }

    #[test]
    fn low_fedora_clears_actual_human_and_centaur_scalps() {
        for kind in [CreatureModel::Human, CreatureModel::Centaur] {
            for woman in [false, true] {
                for build in 0..3 {
                    let look = Look::with_choice(
                        kind,
                        7,
                        Some(Variant {
                            woman,
                            build,
                            ..default()
                        }),
                    );
                    let mut face = Parts::default();
                    human_face(&mut face, &look, true);
                    let mut hat = Parts::default();
                    fedora(&mut hat);
                    // Face and hat share the same head-local transform in both
                    // rigs, so compare their emitted surfaces in that frame.
                    for x in [-0.06, 0.0, 0.06] {
                        for z in [-0.06, -0.012, 0.04] {
                            let origin = Vec3::new(x, 0.08, z);
                            let scalp = face.matte.outer_surface(origin, Vec3::Y).unwrap();
                            let crown = hat.matte.outer_surface(origin, Vec3::Y).unwrap();
                            assert!(crown.y - scalp.y > 0.015, "{kind:?}, woman={woman}, build={build}: crown meets scalp at {x}, {z}");
                        }
                    }
                    for y in [0.26, 0.275] {
                        for column in 0..12 {
                            let a = column as f32 * TAU / 12.0;
                            let out = Vec3::new(a.sin(), 0.0, a.cos());
                            let origin = Vec3::new(0.0, y, -0.012);
                            let scalp = face.matte.outer_surface(origin, out).unwrap();
                            let wall = hat.matte.outer_surface(origin, out).unwrap();
                            assert!(
                                (wall - scalp).dot(out) > 0.012,
                                "taper must clear the side of the scalp"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn fedora_has_a_low_tapered_crown_and_paired_front_pinches() {
        let mut p = Parts::default();
        fedora(&mut p);
        let (lo, hi) = p.matte.bounds();
        assert!(
            hi.y - BROW < 0.12,
            "crown is no longer a tall top-hat cylinder"
        );
        assert!(hi.x - lo.x > 0.44, "the broad felt brim is retained");
        for side in [-1.0, 1.0] {
            let front = fedora_top(side * PI * 0.25);
            let rear = fedora_top(side * PI * 0.75);
            assert!(
                front.x.abs() + 0.018 < rear.x.abs(),
                "paired front pinches narrow the felt crown"
            );
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
        // These points are inside the crescent's broad central strip,
        // away from both its outline and the triangulation's fitted corners.
        // In particular (0.022, 0.405) lies between x=0.004 and x=0.039:
        // surface-fitting only the outline must not bury its interior.
        for x in [0.014, 0.022, 0.030] {
            for y in [0.397, 0.405, 0.413] {
                let origin = Vec3::new(x, y, -0.4);
                let cloth = p.matte.outer_surface(origin, Vec3::Z).unwrap();
                let inlay = p.metal.outer_surface(origin, Vec3::Z).unwrap();
                assert!(
                    inlay.z > cloth.z + 0.001,
                    "crescent interior buried at {x}, {y}"
                );
                assert!(
                    inlay.z < cloth.z + 0.015,
                    "crescent interior floats at {x}, {y}"
                );
            }
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
    fn plume_feathers_have_staggered_arches_and_individual_swept_tips() {
        let paths = [-1.0, -0.5, 0.0, 0.5, 1.0].map(|lane| plume_path(lane, 2.0, false));
        let peaks = paths
            .each_ref()
            .map(|path| path.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max));
        let low = peaks.iter().copied().fold(f32::INFINITY, f32::min);
        let high = peaks.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            high - low > 0.035,
            "a straight shared top edge reads as a rigid fan"
        );
        for path in paths {
            assert!(path.last().unwrap().z < -0.24);
            assert!(path.last().unwrap().y < 0.38, "tips fall behind the helmet");
        }
    }

    #[test]
    fn glass_panels_are_not_hidden_by_continuous_opaque_cage_ribs() {
        let mut p = Parts::default();
        crystal(&mut p);
        for column in 0..10 {
            let a = column as f32 * TAU / 10.0;
            let direction = Vec3::new(a.sin(), 0.0, a.cos());
            let origin = Vec3::new(0.0, 0.29, -0.012);
            assert!(p.ghost.outer_surface(origin, direction).is_some());
            assert!(
                p.satin.outer_surface(origin, direction).is_none(),
                "continuous opaque rib at column {column}"
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
