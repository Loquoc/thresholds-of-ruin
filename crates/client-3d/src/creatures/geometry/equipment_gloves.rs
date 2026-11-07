//! Hand-local glove recipes and separate hollow forearm cuffs. Appearance is
//! cosmetic: these shapes do not encode protection or magical identification.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum GloveAppearance {
    Old,
    Padded,
    Riding,
    Fencing,
}

impl GloveAppearance {
    pub(crate) const ALL: [Self; 4] = [Self::Old, Self::Padded, Self::Riding, Self::Fencing];
    pub(crate) const NAMES: [&'static str; 4] = [
        "old-gloves",
        "padded-gloves",
        "riding-gloves",
        "fencing-gloves",
    ];

    pub(crate) fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    pub(crate) const fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }

    pub(crate) const fn color(self) -> Vec3 {
        match self {
            Self::Old => Vec3::new(0.37, 0.23, 0.12),
            Self::Padded => Vec3::new(0.23, 0.29, 0.39),
            Self::Riding => Vec3::new(0.30, 0.16, 0.075),
            Self::Fencing => Vec3::new(0.075, 0.075, 0.095),
        }
    }
}

const THREAD: Vec3 = Vec3::new(0.67, 0.46, 0.23);
const GOLD: Vec3 = Vec3::new(0.76, 0.57, 0.25);

fn seam(b: &mut Builder, points: &[Vec3], width: f32, color: Vec3) {
    tube(b, points, &vec![(width, width); points.len()], color);
}

/// Replace the bare hand, rather than overlaying a second set of fingers.
/// Uses the existing palm, thumb and finger paths, preserving the grip socket
/// and free-hand articulation on bipeds and compatible non-humanoid hands.
/// `build` is the resolved anatomical hand scale, not a sex-specific recipe.
pub(in crate::creatures) fn emit_hand(
    p: &mut Parts,
    appearance: GloveAppearance,
    at: Vec3,
    side: f32,
    build: f32,
    open: bool,
    clawed: bool,
) {
    let color = appearance.color();
    if open {
        open_hand(p, at, side, build, color, clawed);
    } else {
        fist(p, at, side, build, color);
    }
    let point = |x: f32, y: f32, z: f32| at + Vec3::new(side * x, y, z) * build;
    match appearance {
        GloveAppearance::Old => {
            // Irregular repair seams cross the back, with individual stitches.
            seam(
                &mut p.matte,
                &[
                    point(0.029, 0.018, -0.025),
                    point(0.032, -0.003, 0.0),
                    point(0.029, -0.019, 0.026),
                ],
                0.0015 * build,
                color * 0.63,
            );
            for i in 0..6 {
                let t = i as f32 / 5.0;
                let y = 0.018 - t * 0.037;
                let z = -0.025 + t * 0.051;
                seam(
                    &mut p.matte,
                    &[
                        point(0.033, y - 0.004, z - 0.002),
                        point(0.033, y + 0.004, z + 0.002),
                    ],
                    0.0012 * build,
                    THREAD,
                );
            }
        }
        GloveAppearance::Padded => {
            // Raised quilt cells and separate knuckle pads, not a mitten shell.
            for row in 0..3 {
                for col in 0..3 {
                    p.matte.blob(
                        point(
                            0.029,
                            0.022 - row as f32 * 0.022,
                            (col as f32 - 1.0) * 0.023,
                        ),
                        Vec3::new(0.009, 0.012, 0.012) * build,
                        1,
                        0.0,
                        color * (0.94 + row as f32 * 0.04),
                    );
                }
            }
            for finger in 0..4 {
                p.matte.blob(
                    point(0.013, -0.038, (1.5 - finger as f32) * 0.019),
                    Vec3::new(0.013, 0.014, 0.010) * build,
                    1,
                    0.0,
                    color * 0.84,
                );
            }
        }
        GloveAppearance::Riding => {
            // A separate palm reinforcement remains inside the grip, and the
            // back has two long inset seams like the riding study.
            p.matte.blob(
                point(-0.026, 0.0, 0.0),
                Vec3::new(0.006, 0.033, 0.029) * build,
                2,
                0.0,
                color * 0.70,
            );
            for z in [-0.019, 0.019] {
                seam(
                    &mut p.matte,
                    &[
                        point(0.024, 0.029, z * 0.7),
                        point(0.032, 0.0, z),
                        point(0.025, -0.029, z),
                    ],
                    0.0014 * build,
                    THREAD * 0.8,
                );
            }
        }
        GloveAppearance::Fencing => {
            // Fine dorsal piping and a small gold stitch ornament. The hand
            // stays slim; the dramatic silhouette belongs to the flared cuff.
            for z in [-0.021, 0.021] {
                seam(
                    &mut p.satin,
                    &[
                        point(0.023, 0.028, z * 0.7),
                        point(0.029, 0.0, z),
                        point(0.023, -0.027, z),
                    ],
                    0.001 * build,
                    GOLD * 0.65,
                );
            }
        }
    }
}

fn cuff_profile(appearance: GloveAppearance) -> [(f32, f32); 3] {
    match appearance {
        GloveAppearance::Old => [(0.0, 0.045), (0.035, 0.044), (0.105, 0.064)],
        GloveAppearance::Padded => [(0.0, 0.047), (0.035, 0.048), (0.110, 0.066)],
        GloveAppearance::Riding => [(0.0, 0.043), (0.035, 0.042), (0.120, 0.063)],
        GloveAppearance::Fencing => [(0.0, 0.042), (0.030, 0.043), (0.140, 0.082)],
    }
}

/// Emit in the lower-arm frame, with `wrist` at the hand socket. The open cuff
/// extends toward +Y (elbow). Separate attachment avoids the hand's 1.22 grip
/// scale enlarging a cuff or moving its forearm fit. Radial build and axial arm
/// length are independent; both adult body sizes share the same recipe. Call
/// last in the lower-arm attachment so fitting sees all underlying clothing.
pub(in crate::creatures) fn emit_cuff(
    p: &mut Parts,
    appearance: GloveAppearance,
    wrist: Vec3,
    build: f32,
    length: f32,
) {
    let authored = cuff_profile(appearance);
    // Fit against the actual emitted forearm and every clothing material before
    // adding ourselves. Intermediate stations follow sleeve folds and flares.
    let profile: Vec<_> = (0..=8)
        .map(|i| {
            let segment = (i / 4).min(1);
            let t = (i - segment * 4) as f32 / 4.0;
            let a = authored[segment];
            let b = authored[segment + 1];
            (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
        })
        .collect();
    let fitted: Vec<Vec<f32>> = profile
        .iter()
        .map(|&(y, radius)| {
            (0..=96)
                .map(|col| {
                    let a = col as f32 * TAU / 96.0;
                    let direction = Vec3::new(a.cos(), 0.0, a.sin());
                    let origin = wrist + Vec3::Y * (y * length);
                    [&p.matte, &p.metal, &p.satin, &p.sheer, &p.ghost]
                        .into_iter()
                        .filter_map(|builder| builder.outer_surface(origin, direction))
                        .map(|hit| (hit - origin).dot(direction) + 0.007 * build)
                        .fold(radius * build, f32::max)
                })
                .collect()
        })
        .collect();
    let point = |row: usize, angle: f32, inner: bool| {
        let (y, _) = profile[row];
        let wear = if appearance == GloveAppearance::Old && row == 8 {
            (angle * 5.0).sin() * 0.003
        } else {
            0.0
        };
        let sample = angle.rem_euclid(TAU) / TAU * 96.0;
        let index = (sample.floor() as usize).min(95);
        // The larger neighboring radius is conservative between support rays.
        let r = fitted[row][index].max(fitted[row][index + 1])
            - if inner { 0.003 * build } else { 0.0 };
        wrist + Vec3::new(angle.cos() * r, (y + wear) * length, angle.sin() * r)
    };
    let color = appearance.color();
    for row in 0..8 {
        for col in 0..24 {
            let a = col as f32 * TAU / 24.0;
            let b = (col + 1) as f32 * TAU / 24.0;
            for inner in [false, true] {
                let q = [
                    point(row, a, inner),
                    point(row, b, inner),
                    point(row + 1, b, inner),
                    point(row + 1, a, inner),
                ];
                let mid = (q[0] + q[2]) * 0.5;
                let axis = Vec3::new(wrist.x, mid.y, wrist.z);
                p.matte.quad_out(
                    q,
                    if inner { mid * 2.0 - axis } else { axis },
                    color * if inner { 0.65 } else { 1.0 },
                );
            }
        }
    }
    for row in [0, 8] {
        let path: Vec<_> = (0..=24)
            .map(|i| point(row, i as f32 * TAU / 24.0, false))
            .collect();
        seam(
            &mut p.satin,
            &path,
            0.002 * build,
            if appearance == GloveAppearance::Fencing {
                GOLD
            } else {
                THREAD * 0.65
            },
        );
    }
    if appearance == GloveAppearance::Fencing {
        for col in 0..8 {
            let angle = col as f32 * TAU / 8.0;
            // Repeated scalloped embroidery fans around the broad cuff lip.
            seam(
                &mut p.satin,
                &[
                    point(8, angle - 0.14, false),
                    point(4, angle, false),
                    point(8, angle + 0.14, false),
                ],
                0.0013 * build,
                GOLD,
            );
        }
    } else {
        // Narrow circumferential strap and a framed brass buckle.
        let path: Vec<_> = (0..=32)
            .map(|i| point(4, i as f32 * TAU / 32.0, false))
            .collect();
        seam(
            &mut p.matte,
            &path,
            0.006 * build,
            Vec3::new(0.21, 0.12, 0.055),
        );
        let center = point(4, 0.0, false) + Vec3::X * (0.006 * build);
        let corners = [
            Vec3::new(0.0, -0.008, -0.010),
            Vec3::new(0.0, 0.008, -0.010),
            Vec3::new(0.0, 0.008, 0.010),
            Vec3::new(0.0, -0.008, 0.010),
            Vec3::new(0.0, -0.008, -0.010),
        ]
        .map(|v| center + v * build);
        seam(&mut p.metal, &corners, 0.0018 * build, GOLD);
        if appearance == GloveAppearance::Padded {
            for col in 0..12 {
                let a = col as f32 * TAU / 12.0;
                seam(
                    &mut p.matte,
                    &[point(4, a, false), point(8, a + 0.26, false)],
                    0.002 * build,
                    color * 0.65,
                );
                seam(
                    &mut p.matte,
                    &[point(4, a + 0.26, false), point(8, a, false)],
                    0.002 * build,
                    color * 0.65,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_names_round_trip_without_power_aliases() {
        for (appearance, name) in GloveAppearance::ALL.into_iter().zip(GloveAppearance::NAMES) {
            assert_eq!(appearance.name(), name);
            assert_eq!(GloveAppearance::parse(name), Some(appearance));
        }
        for name in ["gloves", "gauntlets-of-power", "Old Gloves", ""] {
            assert_eq!(GloveAppearance::parse(name), None);
        }
    }

    #[test]
    fn every_glove_preserves_open_fingers_and_closed_grip_at_both_sizes() {
        for appearance in GloveAppearance::ALL {
            for build in [0.83, 1.08] {
                for side in [-1.0, 1.0] {
                    let mut open = Parts::default();
                    let mut closed = Parts::default();
                    emit_hand(&mut open, appearance, Vec3::ZERO, side, build, true, false);
                    emit_hand(
                        &mut closed,
                        appearance,
                        Vec3::ZERO,
                        side,
                        build,
                        false,
                        false,
                    );
                    let (lo, hi) = open.matte.bounds();
                    assert!(lo.is_finite() && hi.is_finite());
                    assert!(
                        lo.y < closed.matte.bounds().0.y - 0.025 * build,
                        "{appearance:?}: fingers must open"
                    );
                    // Finger tips and palm remain on the authored hand frame.
                    assert!(hi.y < 0.06 * build && lo.y > -0.14 * build);
                    assert!(hi.z > 0.065 * build, "separate thumb retained");
                }
            }
        }
    }

    #[test]
    fn cuff_is_open_and_fits_independent_radial_and_axial_scales() {
        for appearance in GloveAppearance::ALL {
            for (build, length) in [(0.83, 0.94), (1.08, 1.03)] {
                let wrist = Vec3::new(0.2, -0.3 * length, 0.1);
                let mut p = Parts::default();
                emit_cuff(&mut p, appearance, wrist, build, length);
                assert!(
                    p.matte
                        .outer_surface(wrist - Vec3::Y * 0.02, Vec3::Y)
                        .is_none(),
                    "cuff must have an open axial bore"
                );
                let wall = p
                    .matte
                    .outer_surface(wrist + Vec3::Y * (0.02 * length), Vec3::X)
                    .expect("wrist wall");
                assert!(wall.x - wrist.x > 0.037 * build, "clearance around forearm");
                let (lo, hi) = p.matte.bounds();
                assert!(lo.is_finite() && hi.is_finite());
                assert!(hi.y > wrist.y + 0.1 * length);
            }
        }
    }

    #[test]
    fn cuff_clears_existing_wide_sleeve_instead_of_nominal_forearm() {
        let mut cuff = Parts::default();
        cuff.matte.blob(
            Vec3::Y * 0.065,
            Vec3::new(0.095, 0.1, 0.088),
            2,
            0.0,
            Vec3::ONE,
        );
        emit_cuff(&mut cuff, GloveAppearance::Riding, Vec3::ZERO, 1.0, 1.0);
        let hit = cuff.matte.outer_surface(Vec3::Y * 0.065, Vec3::X).unwrap();
        assert!(hit.x > 0.098, "cuff clears actual broad sleeve: {hit:?}");
    }

    #[test]
    fn styles_change_geometry_not_only_pigment() {
        let mut padded = Parts::default();
        let mut riding = Parts::default();
        emit_hand(
            &mut padded,
            GloveAppearance::Padded,
            Vec3::ZERO,
            1.0,
            1.0,
            false,
            false,
        );
        emit_hand(
            &mut riding,
            GloveAppearance::Riding,
            Vec3::ZERO,
            1.0,
            1.0,
            false,
            false,
        );
        assert!(padded.matte.bounds().1.x > riding.matte.bounds().1.x + 0.003);
        let mut fencing = Parts::default();
        let mut old = Parts::default();
        emit_cuff(&mut fencing, GloveAppearance::Fencing, Vec3::ZERO, 1.0, 1.0);
        emit_cuff(&mut old, GloveAppearance::Old, Vec3::ZERO, 1.0, 1.0);
        assert!(fencing.matte.bounds().1.x > old.matte.bounds().1.x + 0.015);
        assert!(fencing.matte.bounds().1.y > old.matte.bounds().1.y + 0.025);
        assert!(fencing.metal.is_empty());
        assert!(!old.metal.is_empty());
    }
}
