//! Study-authored footwear, fitted to the same last and shin as native outfits.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum FootwearAppearance {
    WalkingShoes,
    HardShoes,
    Jackboots,
    CombatBoots,
    JungleBoots,
    HikingBoots,
    MudBoots,
    BuckledBoots,
    RidingBoots,
    SnowBoots,
}

impl FootwearAppearance {
    pub const ALL: [Self; 10] = [
        Self::WalkingShoes,
        Self::HardShoes,
        Self::Jackboots,
        Self::CombatBoots,
        Self::JungleBoots,
        Self::HikingBoots,
        Self::MudBoots,
        Self::BuckledBoots,
        Self::RidingBoots,
        Self::SnowBoots,
    ];
    pub const NAMES: [&'static str; 10] = [
        "walking-shoes",
        "hard-shoes",
        "jackboots",
        "combat-boots",
        "jungle-boots",
        "hiking-boots",
        "mud-boots",
        "buckled-boots",
        "riding-boots",
        "snow-boots",
    ];

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    pub fn name(self) -> &'static str {
        Self::NAMES[Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap()]
    }

    pub fn heeled(self) -> bool {
        matches!(self, Self::RidingBoots | Self::Jackboots)
    }

    pub fn color(self) -> Vec3 {
        use FootwearAppearance::*;
        match self {
            WalkingShoes => Vec3::new(0.39, 0.23, 0.12),
            HardShoes => Vec3::new(0.25, 0.20, 0.15),
            Jackboots => Vec3::new(0.16, 0.13, 0.11),
            CombatBoots => Vec3::new(0.23, 0.17, 0.12),
            JungleBoots => Vec3::new(0.33, 0.32, 0.17),
            HikingBoots => Vec3::new(0.42, 0.29, 0.16),
            MudBoots => Vec3::new(0.20, 0.22, 0.19),
            BuckledBoots => Vec3::new(0.40, 0.22, 0.10),
            RidingBoots => Vec3::new(0.12, 0.13, 0.14),
            SnowBoots => Vec3::new(0.37, 0.27, 0.18),
        }
    }

    pub fn trim(self) -> Vec3 {
        match self {
            Self::HardShoes => Vec3::new(0.64, 0.68, 0.72),
            Self::HikingBoots => Vec3::new(0.62, 0.09, 0.06),
            Self::SnowBoots => Vec3::new(0.85, 0.79, 0.63),
            Self::BuckledBoots => Vec3::new(0.70, 0.49, 0.20),
            _ => Vec3::new(0.52, 0.39, 0.22),
        }
    }
}

/// Keep the fitted ankle and radii; coverage is a fraction of available shin.
/// Both the cloth contact envelope and emitted shaft should use this result.
pub(super) fn fit_shape(appearance: FootwearAppearance, mut shape: BootShape) -> BootShape {
    use FootwearAppearance::*;
    let fraction = match appearance {
        WalkingShoes => 0.07,
        HardShoes => 0.28,
        Jackboots | RidingBoots => 0.86,
        CombatBoots | JungleBoots => 0.55,
        HikingBoots => 0.39,
        MudBoots => 0.78,
        BuckledBoots => 0.61,
        SnowBoots => 0.58,
    };
    // The knee is the origin, even when the incoming native boot was ankle high.
    shape.top = shape.bottom * (1.0 - fraction);
    let original_top = shape.radius_top;
    shape.radius_top = shape.radius_bottom + (original_top - shape.radius_bottom) * fraction;
    if appearance == SnowBoots {
        shape.radius_top *= 1.12;
    }
    shape.cuff_radius = shape.radius_top + 0.002;
    shape.cuff_thickness = if appearance == SnowBoots {
        0.018
    } else {
        0.008
    };
    shape.sample_radius_top = shape.radius_top + shape.cuff_thickness + 0.004;
    shape
}

fn shin_support(shin: LimbShape, trousered: bool, build: f32) -> crate::sdf::Sculpt {
    let form = limb_form(shin);
    if trousered {
        let ease = cloth::TROUSER_EASE * build;
        cloth_over(form, ease, &[(-0.035, shin.radius_top + ease + 0.004)])
    } else {
        form
    }
}

/// Fit the complete calf or trouser exterior, not just the ankle's girth.
/// Call once on the native shape; the returned shape owns both the emitted
/// shaft and its garment contact envelope. The polygon's *inradius* clears
/// the support, including the rear calf bulge and the knee's cloth gather.
pub(super) fn fit_shin(
    appearance: FootwearAppearance,
    shape: BootShape,
    shin: LimbShape,
    trousered: bool,
    build: f32,
) -> BootShape {
    let mut shape = fit_shape(appearance, shape);
    let required = shin_profile(shape, shin, trousered, build);
    shape.radius_bottom = shape.radius_bottom.max(required[0]);
    // Hold the fitted ankle, then open the calf end enough to clear every row.
    // This preserves an ankle taper instead of inflating the entire boot to
    // the largest calf cross-section.
    for (row, radius) in required.iter().enumerate().skip(1) {
        let t = row as f32 / 48.0;
        shape.radius_top = shape
            .radius_top
            .max((radius - shape.radius_bottom * (1.0 - t)) / t);
    }
    shape.cuff_radius = shape.radius_top + 0.002;
    shape.sample_radius_top = shape.radius_top + shape.cuff_thickness + 0.004;
    shape.sample_radius_bottom = shape.radius_bottom + 0.012;
    shape
}

/// Per-height circular envelope of the actual clothed calf. The 12-sided
/// shaft's flat faces retain the same clearance as the conservative cone.
fn shin_profile(shape: BootShape, shin: LimbShape, trousered: bool, build: f32) -> [f32; 49] {
    let support = shin_support(shin, trousered, build);
    let (low, high) = shin.mesh_bounds(0.05 * build + 0.02);
    let outside = low.abs().max(high.abs()).xz().length() + 0.05;
    let clearance = 0.006 * build;
    let polygon_inradius = (PI / 12.0).cos();
    let mut required = [0.0f32; 49];
    for (row, radius) in required.iter_mut().enumerate() {
        let t = row as f32 / 48.0;
        let center = Vec3::Y * (shape.bottom + (shape.top - shape.bottom) * t);
        for k in 0..48 {
            let angle = k as f32 * TAU / 48.0;
            let ray = Vec3::new(angle.sin(), 0.0, angle.cos());
            let mut lo = 0.0;
            let mut hi = outside;
            for _ in 0..18 {
                let middle = (lo + hi) * 0.5;
                if support.distance(center + ray * middle) <= 0.0 {
                    lo = middle;
                } else {
                    hi = middle;
                }
            }
            *radius = radius.max((hi + clearance) / polygon_inradius);
        }
    }
    required
}

/// Riding leather is fitted by direction: a rear calf muscle must not
/// inflate the front and both sides into the same circular goblet shape.
/// The coarser circular `fit_shin` envelope remains the conservative bound
/// used by surrounding garments; no anatomy or contact contract is changed.
fn riding_profile(
    shape: BootShape,
    shin: LimbShape,
    trousered: bool,
    build: f32,
) -> [[f32; 48]; 49] {
    let support = shin_support(shin, trousered, build);
    let (low, high) = shin.mesh_bounds(0.05 * build + 0.02);
    let outside = low.abs().max(high.abs()).xz().length() + 0.05;
    let facet = (PI / 48.0).cos();
    std::array::from_fn(|row| {
        let t = row as f32 / 48.0;
        let origin = Vec3::Y * (shape.bottom + (shape.top - shape.bottom) * t);
        std::array::from_fn(|column| {
            let angle = column as f32 * TAU / 48.0;
            let direction = Vec3::new(angle.sin(), 0.0, angle.cos());
            let mut low = 0.0;
            let mut high = outside;
            for _ in 0..18 {
                let middle = (low + high) * 0.5;
                if support.distance(origin + direction * middle) <= 0.0 {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            ((high + 0.006 * build) / facet).max(shape.radius_bottom)
        })
    })
}

/// Riding leather follows the calf then narrows again below the knee. Other
/// study boots retain their authored construction and the same public catalog.
/// `shape` remains the conservative envelope returned by `fit_shin`, so cloth
/// contact continues to enclose every ring without changing its API.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_fitted_shaft(
    p: &mut Parts,
    appearance: FootwearAppearance,
    shape: BootShape,
    build: f32,
    side: f32,
    color: Vec3,
    trim: Vec3,
    shin: LimbShape,
    trousered: bool,
) {
    if appearance != FootwearAppearance::RidingBoots {
        emit_shaft(p, appearance, shape, build, side, color, trim);
        return;
    }
    let radii = riding_profile(shape, shin, trousered, build);
    let y = |t: f32| shape.bottom + (shape.top - shape.bottom) * t;
    let radius = |t: f32, angle: f32| {
        let row = t.clamp(0.0, 1.0) * 48.0;
        let i = (row.floor() as usize).min(47);
        let u = row - i as f32;
        let column = angle.rem_euclid(TAU) / TAU * 48.0;
        let j = (column.floor() as usize).min(47);
        let v = column - j as f32;
        let next = (j + 1) % 48;
        let a = radii[i][j] + (radii[i][next] - radii[i][j]) * v;
        let b = radii[i + 1][j] + (radii[i + 1][next] - radii[i + 1][j]) * v;
        a + (b - a) * u
    };
    let point = |row: usize, column: usize| {
        let angle = column as f32 * TAU / 48.0;
        Vec3::new(
            angle.sin() * radii[row][column],
            y(row as f32 / 48.0),
            angle.cos() * radii[row][column],
        )
    };
    // Closed shared rings give a continuous silhouette; smoothing affects
    // shading only, never the fitted supporting surface.
    p.satin.smooth = true;
    for row in 0..48 {
        let inside = Vec3::Y * y((row as f32 + 0.5) / 48.0);
        for column in 0..48 {
            let next = (column + 1) % 48;
            p.satin.quad_out(
                [
                    point(row, column),
                    point(row, next),
                    point(row + 1, next),
                    point(row + 1, column),
                ],
                inside,
                color,
            );
        }
    }
    for row in [0, 48] {
        let center = Vec3::Y * y(row as f32 / 48.0);
        let inside = center + Vec3::Y * if row == 0 { 0.01 } else { -0.01 };
        for column in 0..48 {
            p.satin.tri_out(
                [center, point(row, column), point(row, (column + 1) % 48)],
                inside,
                color,
            );
        }
    }
    // The piping stays on the curved shaft rather than floating on the old
    // extrapolated cone. Its dipped front/back is the riding study's topline.
    let lip: Vec<_> = (0..=24)
        .map(|i| {
            let a = i as f32 * TAU / 24.0;
            let top = shape.top - 0.025 * a.cos().abs();
            let t = (top - shape.bottom) / (shape.top - shape.bottom);
            Vec3::new(a.sin() * radius(t, a), top, a.cos() * radius(t, a))
        })
        .collect();
    tube(&mut p.satin, &lip, &vec![(0.004, 0.004); lip.len()], trim);
    let seam: Vec<_> = (2..47)
        .map(|row| {
            let t = row as f32 / 48.0;
            Vec3::new(0.0, y(t), -radius(t, PI))
        })
        .collect();
    tube(
        &mut p.satin,
        &seam,
        &vec![(0.003, 0.003); seam.len()],
        color * 1.2,
    );
    let ankle: Vec<_> = (0..=48)
        .map(|column| {
            let angle = column as f32 * TAU / 48.0;
            let r = radius(0.13, angle);
            Vec3::new(angle.sin() * r, y(0.13), angle.cos() * r)
        })
        .collect();
    tube(
        &mut p.satin,
        &ankle,
        &vec![(0.004, 0.004); ankle.len()],
        color * 0.6,
    );
}

fn cord(builder: &mut Builder, a: Vec3, b: Vec3, radius: f32, color: Vec3) {
    builder.prism(a, b, radius, radius, 5, color, 0.0);
}

fn band(p: &mut Parts, shape: BootShape, t: f32, width: f32, color: Vec3) {
    let y = shape.bottom + (shape.top - shape.bottom) * t;
    let radius = shape.radius_bottom + (shape.radius_top - shape.radius_bottom) * t;
    p.satin.prism(
        Vec3::Y * (y + width),
        Vec3::Y * (y - width),
        radius + 0.004,
        radius + 0.004,
        12,
        color,
        0.0,
    );
}

/// A thin closed leather panel, with a buried underside rather than a
/// coplanar decal. Corners follow the shoe last or its raised instep tongue.
fn leather_panel(builder: &mut Builder, top: [Vec3; 4], color: Vec3) {
    let bottom = top.map(|point| point - Vec3::Y * 0.003);
    let inside = (top.iter().copied().sum::<Vec3>() + bottom.iter().copied().sum::<Vec3>()) / 8.0;
    builder.quad_out(top, inside, color);
    builder.quad_out(bottom, inside, color * 0.75);
    for i in 0..4 {
        let next = (i + 1) % 4;
        builder.quad_out(
            [top[i], bottom[i], bottom[next], top[next]],
            inside,
            color * 0.85,
        );
    }
}

/// Visible low-shoe construction is on the exposed instep, ahead of the
/// trouser cuff. The supporting foot and sole remain the original fitted last.
fn walking_upper(p: &mut Parts, last: BootLast, color: Vec3) {
    let b = last.build;
    let mut base = Parts::default();
    boot_parts::emit(&mut base, last, color);
    let surface = |x: f32, z: f32| {
        base.satin
            .outer_surface(Vec3::new(x * b, -0.08, z * b), Vec3::Y)
            .expect("walking panel stays on the fitted last")
    };
    let tongue_height = |z: f32| surface(0.0, z).y.max(0.035 - (z - 0.045) * 0.65) + 0.004;
    // Long separate tongue slopes from the ankle into the vamp. Its dark
    // leather remains visible between the crossings instead of a flat clog.
    let z_rows = [0.035, 0.055, 0.075, 0.095, 0.118];
    for pair in z_rows.windows(2) {
        leather_panel(
            &mut p.matte,
            [
                Vec3::new(-0.019 * b, tongue_height(pair[0]), pair[0] * b),
                Vec3::new(0.019 * b, tongue_height(pair[0]), pair[0] * b),
                Vec3::new(0.019 * b, tongue_height(pair[1]), pair[1] * b),
                Vec3::new(-0.019 * b, tongue_height(pair[1]), pair[1] * b),
            ],
            color * 0.57,
        );
    }
    // Raised eyelet facings slope down into the quarters on each side, so
    // tongue, laces, and heel leather form one continuous constructed upper.
    for sign in [-1.0, 1.0] {
        let inner = |z: f32| Vec3::new(sign * 0.025 * b, tongue_height(z) + 0.002, z * b);
        let outer = |z: f32| surface(sign * 0.046, z) + Vec3::Y * 0.004;
        for pair in z_rows.windows(2) {
            leather_panel(
                &mut p.satin,
                [
                    inner(pair[0]),
                    outer(pair[0]),
                    outer(pair[1]),
                    inner(pair[1]),
                ],
                color * 0.85,
            );
        }
        let seam: Vec<_> = [-0.025, 0.0, 0.035, 0.07, 0.105]
            .into_iter()
            .map(|z| surface(sign * 0.046, z) + Vec3::Y * 0.006)
            .collect();
        tube(
            &mut p.satin,
            &seam,
            &vec![(0.0018 * b, 0.0018 * b); seam.len()],
            color * 0.42,
        );
        // The rear quarter overlaps the vamp instead of ending at an ankle ring.
        for pair in [-0.025, 0.0, 0.035].windows(2) {
            leather_panel(
                &mut p.satin,
                [
                    surface(sign * 0.025, pair[0]) + Vec3::Y * 0.004,
                    surface(sign * 0.046, pair[0]) + Vec3::Y * 0.004,
                    surface(sign * 0.046, pair[1]) + Vec3::Y * 0.004,
                    surface(sign * 0.025, pair[1]) + Vec3::Y * 0.004,
                ],
                color * 0.88,
            );
        }
    }
    let lace_color = color * 0.20;
    for z in [0.058, 0.074, 0.090, 0.106] {
        for sign in [-1.0, 1.0] {
            let a = Vec3::new(sign * 0.025 * b, tongue_height(z) + 0.007, z * b);
            let next = z + 0.010;
            let end = Vec3::new(-sign * 0.025 * b, tongue_height(next) + 0.007, next * b);
            cord(&mut p.satin, a, end, 0.0032 * b, lace_color);
            p.metal.torus(
                a - Vec3::Y * 0.002,
                Vec3::Y,
                0.004 * b,
                0.0013 * b,
                8,
                3,
                color * 0.48,
            );
        }
    }
    // A curved cap seam and contrasting welt give the rounded toe a separate
    // leather cap and a substantial supporting sole, as in the study.
    let cap: Vec<_> = (0..=12)
        .map(|i| {
            let x = -0.048 + i as f32 * 0.008;
            surface(x, 0.135 - 0.012 * (x / 0.048).powi(2)) + Vec3::Y * 0.0025
        })
        .collect();
    tube(
        &mut p.satin,
        &cap,
        &vec![(0.0017 * b, 0.0017 * b); cap.len()],
        color * 0.35,
    );
    let sections = last.sections();
    for sign in [-1.0, 1.0] {
        let welt: Vec<_> = sections
            .iter()
            .map(|section| Vec3::new(sign * section.width, section.top - 0.002, section.z))
            .collect();
        tube(
            &mut p.matte,
            &welt,
            &vec![(0.002 * b, 0.002); welt.len()],
            color * 0.25,
        );
    }
}

/// A rolled fleece cuff: continuous underfur carries short overlapping
/// rounded curls. Unlike a hanging fringe, no row is suspended below its coat.
fn snow_cuff(p: &mut Parts, shape: BootShape, build: f32, color: Vec3) {
    p.matte.torus(
        Vec3::Y * (shape.top - 0.006 * build),
        Vec3::Y,
        shape.radius_top - 0.001 * build,
        0.016 * build,
        48,
        10,
        color * 0.92,
    );
    for row in 0..2 {
        for i in 0..40 {
            let angle = (i as f32 + row as f32 * 0.5) * TAU / 40.0;
            let variation = (i as f32 * 2.4 + row as f32).sin() * 0.5 + 0.5;
            let center = Vec3::new(
                angle.sin() * (shape.radius_top + 0.004 * build),
                shape.top + (0.001 - row as f32 * 0.014) * build,
                angle.cos() * (shape.radius_top + 0.004 * build),
            );
            p.matte.blob(
                center,
                Vec3::new(0.010, 0.008 + variation * 0.002, 0.010) * build,
                (row * 40 + i) as u64,
                0.0,
                color * (0.90 + variation * 0.10),
            );
        }
    }
}

/// Fleece curves across the forward vamp, seated on the actual upper rather
/// than looping around the ankle in a small horizontal bracelet.
fn snow_vamp(p: &mut Parts, last: BootLast, color: Vec3) {
    let mut base = Parts::default();
    boot_parts::emit(&mut base, last, Vec3::ONE);
    let b = last.build;
    let path: Vec<_> = (0..=24)
        .map(|i| {
            let angle = -PI * 0.5 + i as f32 * PI / 24.0;
            let x = angle.sin() * 0.050 * b;
            let z = (0.120 + angle.cos() * 0.045) * b;
            base.satin
                .outer_surface(Vec3::new(x, -0.08, z), Vec3::Y)
                .expect("snow trim follows the forward vamp")
                + Vec3::Y * 0.007
        })
        .collect();
    tube(
        &mut p.matte,
        &path,
        &vec![(0.007 * b, 0.007 * b); path.len()],
        color * 0.92,
    );
    for (i, point) in path.into_iter().enumerate() {
        let variation = (i as f32 * 2.4).sin() * 0.5 + 0.5;
        p.matte.blob(
            point + Vec3::Y * 0.002,
            Vec3::new(0.009, 0.008, 0.009) * b,
            i as u64,
            0.0,
            color * (0.91 + variation * 0.09),
        );
    }
}

/// Complete ankle-local shoe geometry. The caller supplies the recipe's heel
/// choice in `last`; no supporting part extends below the shared floor plane.
pub(super) fn emit_foot(
    p: &mut Parts,
    appearance: FootwearAppearance,
    last: BootLast,
    color: Vec3,
    trim: Vec3,
) {
    use FootwearAppearance::*;
    boot_parts::emit(p, last, color);
    let b = last.build;
    let sections = last.sections();
    if appearance == WalkingShoes {
        walking_upper(p, last, color);
    }
    let laced = matches!(
        appearance,
        CombatBoots | JungleBoots | HikingBoots | SnowBoots
    );
    if laced {
        // Sample the authored vamp, rather than suspending flat laces above it.
        for k in 0..4 {
            let t = k as f32 / 4.0;
            let s = sections[2];
            let n = sections[3];
            let y = (s.top + s.height) * (1.0 - t) + (n.top + n.height) * t + 0.001;
            let z = s.z * (1.0 - t) + n.z * t;
            for sign in [-1.0, 1.0] {
                cord(
                    &mut p.satin,
                    Vec3::new(-0.024 * b * sign, y - 0.003, z),
                    Vec3::new(0.024 * b * sign, y - 0.005, z + 0.012 * b),
                    0.0025 * b,
                    trim,
                );
            }
        }
    }
    if appearance == HardShoes {
        // Four overlapping curved sabaton plates, each following the last.
        for index in 2..6 {
            let s = sections[index];
            let n = sections[index + 1];
            for k in 0..8 {
                let point = |section: super::super::anatomy::BootSection, j: usize| {
                    let a = j as f32 * PI / 8.0;
                    Vec3::new(
                        a.cos() * (section.width * 0.96 + 0.003 * b),
                        section.top + a.sin() * (section.height + 0.007),
                        section.z,
                    )
                };
                let corners = [point(s, k), point(s, k + 1), point(n, k + 1), point(n, k)];
                p.metal.tri([corners[0], corners[1], corners[2]], trim);
                p.metal.tri([corners[0], corners[2], corners[3]], trim);
                cord(&mut p.metal, corners[0], corners[1], 0.0025 * b, trim * 0.7);
            }
        }
    }
    if matches!(
        appearance,
        CombatBoots | JungleBoots | HikingBoots | MudBoots | SnowBoots
    ) {
        // Raised edge lugs produce a serrated outsole without lowering the foot.
        for k in 0..7 {
            let t = k as f32 / 6.0;
            let z = (-0.045 + t * 0.20) * b;
            let w = (0.049 + 0.011 * (t * PI).sin()) * b;
            for side in [-1.0, 1.0] {
                p.matte.cuboid(
                    Vec3::new(side * w, BootLast::FLOOR + 0.009, z),
                    Vec3::new(0.009 * b, 0.009, 0.009 * b),
                    Quat::IDENTITY,
                    color * 0.38,
                );
            }
        }
    }
    if appearance == HikingBoots {
        // Protective rubber toe bumper follows the rounded toe.
        let s = sections[4];
        let path: Vec<_> = (0..=12)
            .map(|i| {
                let a = i as f32 * PI / 12.0;
                Vec3::new(a.cos() * s.width, -0.045, (0.12 + a.sin() * 0.073) * b)
            })
            .collect();
        tube(
            &mut p.matte,
            &path,
            &vec![(0.008, 0.012); path.len()],
            color * 0.36,
        );
    }
    if appearance == SnowBoots {
        snow_vamp(p, last, trim);
    }
}

/// Complete shaft in the established lower-leg frame, with outward-facing
/// hardware mirrored to the wearer's left and right boot.
pub(super) fn emit_shaft(
    p: &mut Parts,
    appearance: FootwearAppearance,
    shape: BootShape,
    build: f32,
    side: f32,
    color: Vec3,
    trim: Vec3,
) {
    use FootwearAppearance::*;
    let radius = |t: f32| shape.radius_bottom + (shape.radius_top - shape.radius_bottom) * t;
    let y = |t: f32| shape.bottom + (shape.top - shape.bottom) * t;
    p.satin.prism(
        Vec3::Y * shape.top,
        Vec3::Y * shape.bottom,
        shape.radius_top,
        shape.radius_bottom,
        12,
        color,
        0.0,
    );
    match appearance {
        WalkingShoes => {
            // Low dark leather topline, not a contrasting gold ankle bracelet.
            band(p, shape, 0.88, 0.002, color * 0.48);
        }
        HardShoes => {
            // Front greave and two separate ankle straps.
            p.metal.cuboid(
                Vec3::new(0.0, y(0.52), radius(0.52)),
                Vec3::new(radius(0.5) * 0.52, (shape.top - shape.bottom) * 0.42, 0.008),
                Quat::from_rotation_x(-0.06),
                trim,
            );
            for t in [0.2, 0.8] {
                band(p, shape, t, 0.006, color * 0.55);
            }
        }
        Jackboots => {
            // Wide turned-down leather cuff and soft stacked ankle folds.
            band(
                p,
                shape,
                0.88,
                (shape.top - shape.bottom) * 0.11,
                color * 1.15,
            );
            for t in [0.14, 0.25, 0.36] {
                band(p, shape, t, 0.005, color * 0.85);
            }
        }
        CombatBoots | JungleBoots | HikingBoots | SnowBoots => {
            let count = match appearance {
                HikingBoots => 5,
                SnowBoots => 6,
                _ => 8,
            };
            // Jungle boots have a broad, thin canvas gusset rather than the
            // combat boot's thick leather tongue. Keep that construction
            // visible in geometry even when both materials share a pigment.
            let (tongue_width, tongue_depth) = if appearance == JungleBoots {
                (0.66, 0.003)
            } else {
                (0.50, 0.006)
            };
            p.matte.cuboid(
                Vec3::new(0.0, y(0.5), radius(0.5)),
                Vec3::new(
                    radius(0.5) * tongue_width,
                    (shape.top - shape.bottom) * 0.47,
                    tongue_depth,
                ),
                Quat::IDENTITY,
                if appearance == JungleBoots {
                    trim * 0.62
                } else {
                    color * 0.75
                },
            );
            for k in 0..count {
                let t = 0.10 + k as f32 / (count as f32) * 0.82;
                let next = t + 0.065;
                for sign in [-1.0, 1.0] {
                    let a = Vec3::new(sign * radius(t) * 0.40, y(t), radius(t) + 0.009);
                    let b = Vec3::new(-sign * radius(next) * 0.40, y(next), radius(next) + 0.009);
                    cord(&mut p.satin, a, b, 0.0025 * build, trim);
                    p.metal
                        .torus(a, Vec3::Z, 0.004 * build, 0.0015 * build, 8, 3, trim * 0.7);
                }
            }
            band(
                p,
                shape,
                0.98,
                if appearance == HikingBoots {
                    0.012
                } else {
                    0.005
                },
                color * 0.6,
            );
            if appearance == JungleBoots {
                for t in [0.13, 0.24] {
                    p.metal.torus(
                        Vec3::new(side * (radius(t) + 0.002), y(t), 0.0),
                        Vec3::X,
                        0.009 * build,
                        0.002 * build,
                        10,
                        4,
                        trim,
                    );
                }
            }
            if appearance == SnowBoots {
                snow_cuff(p, shape, build, trim);
            }
        }
        MudBoots => {
            // Molded, unbroken waterproof shaft with a rolled rim and ankle welt.
            band(p, shape, 0.99, 0.006, color * 1.12);
            band(p, shape, 0.12, 0.008, color * 0.65);
        }
        BuckledBoots => {
            for t in [0.18, 0.52, 0.86] {
                band(p, shape, t, 0.014, color * 0.68);
                let x = side * radius(t) * 0.58;
                let z = radius(t) * 0.85 + 0.008;
                let corners = [
                    Vec3::new(x - 0.012, y(t) - 0.015, z),
                    Vec3::new(x + 0.012, y(t) - 0.015, z),
                    Vec3::new(x + 0.012, y(t) + 0.015, z),
                    Vec3::new(x - 0.012, y(t) + 0.015, z),
                ];
                for k in 0..4 {
                    cord(&mut p.metal, corners[k], corners[(k + 1) % 4], 0.003, trim);
                }
                cord(
                    &mut p.metal,
                    Vec3::new(x, y(t) - 0.012, z),
                    Vec3::new(x, y(t) + 0.012, z),
                    0.002,
                    trim,
                );
            }
        }
        RidingBoots => {
            // Scalloped topline and a vertical rear seam distinguish the sleek
            // shaft from the folded jackboot, even with identical pigment.
            let path: Vec<_> = (0..=24)
                .map(|i| {
                    let a = i as f32 * TAU / 24.0;
                    Vec3::new(
                        a.sin() * shape.radius_top,
                        shape.top - 0.025 * a.cos().abs(),
                        a.cos() * shape.radius_top,
                    )
                })
                .collect();
            tube(&mut p.satin, &path, &vec![(0.004, 0.004); path.len()], trim);
            cord(
                &mut p.satin,
                Vec3::new(0.0, y(0.05), -radius(0.05)),
                Vec3::new(0.0, y(0.94), -radius(0.94)),
                0.003,
                color * 1.2,
            );
            band(p, shape, 0.13, 0.004, color * 0.6);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walking_shoe_has_exposed_lacing_tongue_and_quarter_panels_over_its_last() {
        for build in [0.75, 1.0, 1.4] {
            let last = BootLast {
                build,
                heeled: false,
            };
            let mut original = Parts::default();
            boot_parts::emit(&mut original, last, Vec3::ONE);
            let mut shoe = Parts::default();
            emit_foot(
                &mut shoe,
                FootwearAppearance::WalkingShoes,
                last,
                Vec3::ONE,
                Vec3::ONE,
            );
            for z in [0.079, 0.095, 0.111] {
                let origin = Vec3::new(0.0, -0.08, z * build);
                let vamp = original.satin.outer_surface(origin, Vec3::Y).unwrap();
                let tongue = shoe.matte.outer_surface(origin, Vec3::Y).unwrap();
                let lace = shoe.satin.outer_surface(origin, Vec3::Y).unwrap();
                assert!(
                    tongue.y > vamp.y + 0.002,
                    "separate tongue stands above vamp"
                );
                assert!(
                    lace.y > tongue.y + 0.002,
                    "lace crossings stay visible above tongue"
                );
            }
            let origin = Vec3::new(0.035 * build, -0.08, 0.08 * build);
            assert!(
                shoe.satin.outer_surface(origin, Vec3::Y).unwrap().y
                    > original.satin.outer_surface(origin, Vec3::Y).unwrap().y + 0.003,
                "raised quarter slopes into lacing facing"
            );
            assert!(
                !shoe.metal.is_empty(),
                "visible eyelets accompany the laces"
            );
            // No anatomical support is removed: all original foot/sole rays
            // remain covered and the supporting plane remains unchanged.
            for z in [-0.03, 0.02, 0.08, 0.14] {
                for x in [-0.025, 0.0, 0.025] {
                    let origin = Vec3::new(x * build, -0.08, z * build);
                    let inner = original.satin.outer_surface(origin, Vec3::Y).unwrap();
                    let outer = shoe.satin.outer_surface(origin, Vec3::Y).unwrap();
                    assert!(outer.y >= inner.y - 1e-6);
                }
            }
            assert!(shoe.matte.bounds().0.y >= BootLast::FLOOR - 1e-6);
            let contacts = last.contacts();
            for mesh in [&shoe.satin, &shoe.matte, &shoe.metal] {
                for z in [0.058, 0.074, 0.09, 0.106] {
                    for x in [-0.025, 0.0, 0.025] {
                        if let Some(hit) =
                            mesh.outer_surface(Vec3::new(x * build, -0.08, z * build), Vec3::Y)
                        {
                            assert!(
                                contacts
                                    .iter()
                                    .any(|(center, radii)| ((hit - *center) / *radii)
                                        .length_squared()
                                        <= 1.0),
                                "construction stays within the existing cloth contact envelope"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn snow_fleece_is_a_continuous_compact_roll_and_covers_the_forward_vamp() {
        for build in [0.75, 1.0, 1.4] {
            let shape = fit_shape(
                FootwearAppearance::SnowBoots,
                BootShape::fitted(build, -0.24),
            );
            let mut cuff = Parts::default();
            snow_cuff(&mut cuff, shape, build, Vec3::ONE);
            let (low, high) = cuff.matte.bounds();
            assert!(
                low.y > shape.top - 0.032 * build,
                "fleece must not dangle down the shaft"
            );
            assert!(high.y < shape.top + 0.016 * build, "compact rounded cuff");
            let mut shaft = Parts::default();
            emit_shaft(
                &mut shaft,
                FootwearAppearance::SnowBoots,
                shape,
                build,
                1.0,
                Vec3::ONE,
                Vec3::ONE,
            );
            // Off-grid rays witness a solid rolled lip above the leather and
            // continuous underfur between curls around the entire boot.
            for height in [0.004, -0.004, -0.015] {
                let origin = Vec3::Y * (shape.top + height * build);
                for k in 0..73 {
                    let angle = (k as f32 + 0.37) * TAU / 73.0;
                    let ray = Vec3::new(angle.sin(), 0.0, angle.cos());
                    let hit = shaft
                        .matte
                        .outer_surface(origin, ray)
                        .expect("opaque fleece around entire rim");
                    assert!(
                        hit.distance(origin) > shape.radius_top + 0.003 * build,
                        "curl gaps must expose underfur, not bare leather"
                    );
                }
            }
            let last = BootLast {
                build,
                heeled: false,
            };
            let mut base = Parts::default();
            boot_parts::emit(&mut base, last, Vec3::ONE);
            let mut boot = Parts::default();
            emit_foot(
                &mut boot,
                FootwearAppearance::SnowBoots,
                last,
                Vec3::ONE,
                Vec3::ONE,
            );
            for x in [-0.025f32, 0.0, 0.025] {
                let z = 0.120 + 0.045 * (1.0 - (x / 0.05).powi(2)).sqrt();
                let origin = Vec3::new(x * build, -0.08, z * build);
                let upper = base.satin.outer_surface(origin, Vec3::Y).unwrap();
                let fleece = boot
                    .matte
                    .outer_surface(origin, Vec3::Y)
                    .expect("fur spans forward vamp");
                assert!(
                    fleece.y > upper.y + 0.005,
                    "fur lies outside supporting upper"
                );
                assert!(
                    last.contacts()
                        .iter()
                        .any(
                            |(center, radii)| ((fleece - *center) / *radii).length_squared() <= 1.0
                        ),
                    "fur stays inside cloth contact envelope"
                );
                assert_eq!(
                    boot.satin.outer_surface(origin, Vec3::Y).unwrap(),
                    upper,
                    "snow refinement must retain underlying foot support"
                );
            }
            assert!(boot.matte.bounds().0.y >= BootLast::FLOOR - 1e-6);
        }
    }

    #[test]
    fn study_catalog_has_ten_stable_distinct_names() {
        let mut names = std::collections::HashSet::new();
        for item in FootwearAppearance::ALL {
            assert!(names.insert(item.name()));
            assert_eq!(FootwearAppearance::parse(item.name()), Some(item));
        }
        assert_eq!(names.len(), 10);
        assert_eq!(FootwearAppearance::parse("speed-boots"), None);
    }

    #[test]
    fn coverage_follows_the_study_without_changing_the_fitted_ankle() {
        use FootwearAppearance::*;
        for build in [0.75, 1.0, 1.4] {
            let fit = |appearance| fit_shape(appearance, BootShape::fitted(build, -0.24));
            assert!(fit(WalkingShoes).top < fit(HikingBoots).top);
            assert!(fit(HikingBoots).top < fit(CombatBoots).top);
            assert!(fit(CombatBoots).top < fit(MudBoots).top);
            assert!(fit(MudBoots).top < fit(RidingBoots).top);
            for appearance in FootwearAppearance::ALL {
                let shape = fit(appearance);
                assert_eq!(shape.bottom, -0.42);
                assert_eq!(shape.radius_bottom, 0.064 * build);
                assert!(shape.top < 0.0 && shape.top > shape.bottom);
                assert!(shape.sample_radius_top >= shape.radius_top + shape.cuff_thickness);
            }
        }
    }

    #[test]
    fn every_study_has_distinct_geometry_even_with_identical_colors() {
        let mut signatures = std::collections::HashSet::new();
        for appearance in FootwearAppearance::ALL {
            let mut parts = Parts::default();
            let shape = fit_shape(appearance, BootShape::fitted(1.0, -0.24));
            emit_shaft(
                &mut parts,
                appearance,
                shape,
                1.0,
                1.0,
                Vec3::ONE,
                Vec3::ONE,
            );
            let mut signature = Vec::new();
            for y in 0..40 {
                for x in [-0.04, 0.0, 0.04] {
                    let origin = Vec3::new(x, -0.42 + y as f32 * 0.01, 0.0);
                    for mesh in [&parts.matte, &parts.satin, &parts.metal] {
                        let value = mesh
                            .outer_surface(origin, Vec3::Z)
                            .map_or(-1, |point| (point.z * 100_000.0).round() as i32);
                        signature.push(value);
                    }
                }
            }
            assert!(
                signatures.insert(signature),
                "duplicate silhouette: {appearance:?}"
            );
        }
    }

    #[test]
    fn jungle_canvas_gusset_is_wider_and_thinner_than_combat_leather() {
        let shape = fit_shape(
            FootwearAppearance::CombatBoots,
            BootShape::fitted(1.0, -0.24),
        );
        let emit = |appearance| {
            let mut parts = Parts::default();
            emit_shaft(
                &mut parts,
                appearance,
                shape,
                1.0,
                1.0,
                Vec3::ONE,
                Vec3::ONE,
            );
            parts
        };
        let combat = emit(FootwearAppearance::CombatBoots);
        let jungle = emit(FootwearAppearance::JungleBoots);
        // These probes sit inside the pre-existing catalog silhouette grid.
        // Same fit and colors isolate the canvas construction difference.
        let origin = Vec3::new(0.0, -0.30, 0.0);
        let leather = combat.matte.outer_surface(origin, Vec3::Z).unwrap();
        let canvas = jungle.matte.outer_surface(origin, Vec3::Z).unwrap();
        assert!((leather.z - canvas.z - 0.003).abs() < 1e-6);
        let outside_leather = Vec3::new(0.04, -0.30, 0.0);
        assert!(combat
            .matte
            .outer_surface(outside_leather, Vec3::Z)
            .is_none());
        assert!(jungle
            .matte
            .outer_surface(outside_leather, Vec3::Z)
            .is_some());
    }

    #[test]
    fn riding_calf_volume_stays_at_rear_instead_of_inflating_front_and_sides() {
        for woman in [false, true] {
            for build in 0..3 {
                let body = HumanoidBody::resolve(&Look::with_choice(
                    CreatureModel::Human,
                    7,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                ));
                for trousered in [false, true] {
                    let shape = fit_shin(
                        FootwearAppearance::RidingBoots,
                        BootShape::fitted(body.build, -0.24),
                        body.shin,
                        trousered,
                        body.build,
                    );
                    let mut parts = Parts::default();
                    emit_fitted_shaft(
                        &mut parts,
                        FootwearAppearance::RidingBoots,
                        shape,
                        body.build,
                        1.0,
                        Vec3::ONE,
                        Vec3::ONE,
                        body.shin,
                        trousered,
                    );
                    let origin = Vec3::Y * -0.14;
                    let radius = |direction| {
                        parts
                            .satin
                            .outer_surface(origin, direction)
                            .unwrap()
                            .distance(origin)
                    };
                    let rear = radius(Vec3::NEG_Z);
                    let front = radius(Vec3::Z);
                    let left = radius(Vec3::NEG_X);
                    let right = radius(Vec3::X);
                    assert!(rear > front+0.012*body.build,
                        "rear calf must not balloon the shin front: woman={woman} build={build} trousers={trousered}: {rear}/{front}");
                    assert!(
                        rear > right + 0.006 * body.build,
                        "retain narrower lateral calf silhouette"
                    );
                    assert!(
                        (left - right).abs() < 0.001 * body.build,
                        "fit remains laterally balanced"
                    );
                    // Independently emitted support must still fit at the
                    // rear, front, and diagonals after removing empty volume.
                    let mut support = Builder::default();
                    if trousered {
                        trouser_leg(&mut support, body.shin, body.build, true, Vec3::ONE);
                    } else {
                        sculpted_limb(&mut support, body.shin, Vec3::ONE);
                    }
                    for k in 0..61 {
                        let angle = (k as f32 + 0.39) * TAU / 61.0;
                        let ray = Vec3::new(angle.sin(), 0.0, angle.cos());
                        let inner = support.outer_surface(origin, ray).unwrap().distance(origin);
                        assert!(
                            radius(ray) > inner + 0.002 * body.build,
                            "directional shaft must enclose real calf/trouser mesh"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn riding_shaft_narrows_above_calf_without_exceeding_contact_envelope() {
        for woman in [false, true] {
            for build in 0..3 {
                let body = HumanoidBody::resolve(&Look::with_choice(
                    CreatureModel::Human,
                    7,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                ));
                let shape = fit_shin(
                    FootwearAppearance::RidingBoots,
                    BootShape::fitted(body.build, -0.24),
                    body.shin,
                    true,
                    body.build,
                );
                let mut parts = Parts::default();
                emit_fitted_shaft(
                    &mut parts,
                    FootwearAppearance::RidingBoots,
                    shape,
                    body.build,
                    1.0,
                    Vec3::ONE,
                    Vec3::ONE,
                    body.shin,
                    true,
                );
                let rear_radius = |y| {
                    parts
                        .satin
                        .outer_surface(Vec3::Y * y, Vec3::NEG_Z)
                        .expect("riding shaft surface")
                        .z
                        .abs()
                };
                let calf = rear_radius(-0.14);
                let rim = rear_radius(shape.top - 0.004);
                assert!(
                    rim < calf - 0.003,
                    "calf should narrow toward knee: {woman}/{build}: {rim}/{calf}"
                );
                assert!(
                    rim < shape.radius_top - 0.008,
                    "must remove unsupported cone flare: {woman}/{build}: {rim}/{}",
                    shape.radius_top
                );
                for row in 1..48 {
                    let t = row as f32 / 48.0;
                    let y = shape.bottom + (shape.top - shape.bottom) * t;
                    let bound = shape.sample_radius_bottom
                        + (shape.sample_radius_top - shape.sample_radius_bottom) * t;
                    for k in 0..24 {
                        let angle = k as f32 * TAU / 24.0;
                        let ray = Vec3::new(angle.sin(), 0.0, angle.cos());
                        let origin = Vec3::Y * y;
                        let at = parts.satin.outer_surface(origin, ray).unwrap();
                        assert!(
                            at.distance(origin) < bound,
                            "fitted riding shaft escaped contact envelope"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn fitted_shaft_covers_actual_calf_and_trousers_across_wearers() {
        for woman in [false, true] {
            for build in 0..3 {
                let look = Look::with_choice(
                    CreatureModel::Human,
                    7,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                );
                let body = HumanoidBody::resolve(&look);
                for trousered in [false, true] {
                    // Build the support independently through the production
                    // emitter, so this checks actual mesh coverage as well as SDF.
                    let mut support = Builder::default();
                    if trousered {
                        trouser_leg(&mut support, body.shin, body.build, true, Vec3::ONE);
                    } else {
                        sculpted_limb(&mut support, body.shin, Vec3::ONE);
                    }
                    for appearance in FootwearAppearance::ALL {
                        let shape = fit_shin(
                            appearance,
                            BootShape::fitted(body.build, -0.24),
                            body.shin,
                            trousered,
                            body.build,
                        );
                        let mut shaft = Parts::default();
                        emit_fitted_shaft(
                            &mut shaft,
                            appearance,
                            shape,
                            body.build,
                            1.0,
                            Vec3::ONE,
                            Vec3::ONE,
                            body.shin,
                            trousered,
                        );
                        assert!(shape.sample_radius_top >= shape.radius_top);
                        assert!(shape.sample_radius_bottom >= shape.radius_bottom);
                        // Offset samples from the fitter's rows/angles; testing
                        // only fit samples could miss the prism's flat panels.
                        for row in 0..31 {
                            let t = (row as f32 + 0.37) / 31.0;
                            let origin = Vec3::Y * (shape.bottom + (shape.top - shape.bottom) * t);
                            for k in 0..37 {
                                let angle = (k as f32 + 0.31) * TAU / 37.0;
                                let ray = Vec3::new(angle.sin(), 0.0, angle.cos());
                                let Some(inner) = support.outer_surface(origin, ray) else {
                                    continue;
                                };
                                let outer = shaft
                                    .satin
                                    .outer_surface(origin, ray)
                                    .expect("closed shaft");
                                assert!(outer.distance(origin) > inner.distance(origin) + 0.002*body.build,
                                    "{appearance:?} woman={woman} build={build} trousers={trousered} y={} inner={inner:?} outer={outer:?}",origin.y);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn emitted_footwear_stays_planted_and_has_required_material_geometry() {
        for appearance in FootwearAppearance::ALL {
            for build in [0.75, 1.0, 1.4] {
                let mut foot = Parts::default();
                emit_foot(
                    &mut foot,
                    appearance,
                    BootLast {
                        build,
                        heeled: appearance.heeled(),
                    },
                    Vec3::ONE,
                    Vec3::ONE,
                );
                for mesh in [&foot.matte, &foot.satin, &foot.metal] {
                    if mesh.is_empty() {
                        continue;
                    }
                    let (low, high) = mesh.bounds();
                    assert!(low.is_finite() && high.is_finite());
                    assert!(low.y >= BootLast::FLOOR - 1e-5, "{appearance:?}: {low:?}");
                }
                assert_eq!(
                    !foot.metal.is_empty(),
                    matches!(
                        appearance,
                        FootwearAppearance::HardShoes | FootwearAppearance::WalkingShoes
                    )
                );
                let mut shaft = Parts::default();
                let shape = fit_shape(appearance, BootShape::fitted(build, -0.24));
                emit_shaft(
                    &mut shaft,
                    appearance,
                    shape,
                    build,
                    -1.0,
                    Vec3::ONE,
                    Vec3::ONE,
                );
                assert!(!shaft.satin.is_empty());
                if matches!(
                    appearance,
                    FootwearAppearance::BuckledBoots
                        | FootwearAppearance::HardShoes
                        | FootwearAppearance::JungleBoots
                ) {
                    assert!(!shaft.metal.is_empty());
                }
            }
        }
    }
}
