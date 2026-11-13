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
pub(super) fn fit_shape(
    appearance: FootwearAppearance,
    mut shape: BootShape,
    build: f32,
) -> BootShape {
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
    if appearance == SnowBoots {
        let contact_length = shape.top - shape.bottom;
        fit_snow_cuff_contact(&mut shape, build, contact_length);
    }
    shape
}

// Fleece dimensions also drive the contact envelope. Blob jitter multiplies
// non-pole radii by at most 1 + jitter/2; poles stay within the same bound.
const SNOW_PILE_RADIAL_OFFSET: f32 = 0.008;
const SNOW_PILE_RADIUS_MAX: f32 = 0.015;
const SNOW_PILE_HEIGHT_MAX: f32 = 0.0195;
const SNOW_PILE_JITTER: f32 = 0.10;

fn fit_snow_cuff_contact(shape: &mut BootShape, build: f32, contact_length: f32) {
    let wobble = 1.0 + SNOW_PILE_JITTER * 0.5;
    let radial = (SNOW_PILE_RADIAL_OFFSET + SNOW_PILE_RADIUS_MAX * wobble) * build;
    // Centers are at -.009 +/- .001. This also bounds the two underfur
    // tori, whose lowest possible point is -.012 - .016.
    let vertical = (0.010 + SNOW_PILE_HEIGHT_MAX * wobble) * build;
    shape.cuff_radius = shape.radius_top;
    // Robe triangle contact uses 12 straight capsule segments, not a circle.
    // Include their inward chord sagitta and the full radial/Y corner distance.
    let chord_inset = shape.cuff_radius * (1.0 - (PI / 12.0).cos());
    shape.cuff_thickness = (radial + chord_inset).hypot(vertical) + 0.001 * build;
    let outer = shape.radius_top + radial;
    let t = ((contact_length - vertical) / contact_length).clamp(0.01, 1.0);
    // Sample contact tapers below top and rounds above it. Reserve enough
    // top radius at the lowest pile height, not just at the cuff center.
    let tapered = (outer - shape.sample_radius_bottom * (1.0 - t)) / t;
    shape.sample_radius_top = tapered
        .max(outer.hypot(vertical))
        .max(shape.radius_top + shape.cuff_thickness)
        + 0.004 * build;
}

#[cfg(test)]
thread_local! {
    static SHIN_SAMPLES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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

/// One wearer/outfit fit supplies both contact and the two rendered shafts.
/// Only riding leather retains directional radii; other families need just
/// the conservative envelope, without a per-Figure grid allocation.
pub(super) struct FittedShin {
    shape: BootShape,
    appearance: FootwearAppearance,
    build: f32,
    riding: Option<Box<[[f32; 48]; 49]>>,
}

impl FittedShin {
    pub fn envelope(&self) -> BootShape {
        self.shape
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
) -> FittedShin {
    let mut shape = fit_shape(appearance, shape, build);
    let radii = calf_support_grid(shape, shin, trousered, build);
    let required: [f32; 49] = std::array::from_fn(|row| {
        radii[row]
            .iter()
            .map(|radius| (radius + 0.006 * build) / (PI / 12.0).cos())
            .fold(0.0f32, f32::max)
    });
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
    if appearance == FootwearAppearance::SnowBoots {
        let contact_length = shin.length + shape.top;
        fit_snow_cuff_contact(&mut shape, build, contact_length);
    }
    let riding = if appearance == FootwearAppearance::RidingBoots {
        Some(Box::new(std::array::from_fn(|row| {
            std::array::from_fn(|column| {
                ((radii[row][column] + 0.006 * build) / (PI / 48.0).cos()).max(shape.radius_bottom)
            })
        })))
    } else {
        None
    };
    FittedShin {
        shape,
        appearance,
        build,
        riding,
    }
}

/// Raw directional support, before each item's polygon compensation or
/// ankle floor. Coverage is already resolved by fit_shape. Keep rear calf
/// fullness rather than reducing it to a circular proxy for rendered leather.
fn calf_support_grid(
    shape: BootShape,
    shin: LimbShape,
    trousered: bool,
    build: f32,
) -> [[f32; 48]; 49] {
    #[cfg(test)]
    SHIN_SAMPLES.with(|count| count.set(count.get() + 1));
    let support = shin_support(shin, trousered, build);
    let (low, high) = shin.mesh_bounds(0.05 * build + 0.02);
    let outside = low.abs().max(high.abs()).xz().length() + 0.05;
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
            high
        })
    })
}

/// Riding leather follows the calf then narrows again below the knee. Other
/// study boots retain their authored construction and the same public catalog.
/// `shape` remains the conservative envelope returned by `fit_shin`, so cloth
/// contact continues to enclose every ring without changing its API.
pub(super) fn emit_fitted_shaft(
    p: &mut Parts,
    fit: &FittedShin,
    side: f32,
    color: Vec3,
    trim: Vec3,
) {
    let appearance = fit.appearance;
    let shape = fit.shape;
    let build = fit.build;
    if appearance != FootwearAppearance::RidingBoots {
        emit_shaft(p, appearance, shape, build, side, color, trim);
        return;
    }
    let radii = fit
        .riding
        .as_deref()
        .expect("riding fit retains its directional support");
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
    riding_ring(&mut p.satin, &lip, 0.004, trim);
    // End inside the dipped rear lip, rather than extending the seam toward
    // the higher, undipped shaft edge and leaving a visible protruding stem.
    let seam_end = shape.top - 0.025;
    let mut seam: Vec<_> = (2..47)
        .map(|row| row as f32 / 48.0)
        .filter(|&t| y(t) < seam_end)
        .map(|t| Vec3::new(0.0, y(t), -radius(t, PI)))
        .collect();
    let end_t = (seam_end - shape.bottom) / (shape.top - shape.bottom);
    seam.push(Vec3::new(0.0, seam_end, -radius(end_t, PI)));
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
    riding_ring(&mut p.satin, &ankle, 0.004, color * 0.6);
}

fn riding_ring(builder: &mut Builder, path: &[Vec3], width: f32, color: Vec3) {
    closed_ring(builder, path, width, color);
}

fn cord(builder: &mut Builder, a: Vec3, b: Vec3, radius: f32, color: Vec3) {
    builder.prism(a, b, radius, radius, 5, color, 0.0);
}

fn band(p: &mut Parts, shape: BootShape, t: f32, width: f32, color: Vec3) {
    emit_band(&mut p.satin, shape, t, width, color);
}

fn emit_band(b: &mut Builder, shape: BootShape, t: f32, width: f32, color: Vec3) {
    let y = shape.bottom + (shape.top - shape.bottom) * t;
    let radius = shape.radius_bottom + (shape.radius_top - shape.radius_bottom) * t;
    b.prism(
        Vec3::Y * (y + width),
        Vec3::Y * (y - width),
        radius + 0.004,
        radius + 0.004,
        12,
        color,
        0.0,
    );
}

/// Hardware belongs to this emitted leather band, not to a second nominal
/// shaft radius. Snap its rigid frame to an actual outward side facet; detail
/// settings and the current recipe frame stay owned by the band builder.
fn band_attachment(band: &Builder, height: f32, side: f32) -> Option<fitting::StrapAttachment> {
    let desired = Vec3::new(side * 0.58, 0.0, 0.85).normalize();
    let normal = band
        .surface_triangles()
        .filter_map(|tri| {
            let normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]).try_normalize()?;
            (normal.y.abs() < 0.001 && normal.dot(desired) > 0.5).then_some(normal)
        })
        .max_by(|a, b| a.dot(desired).total_cmp(&b.dot(desired)))?;
    let at = band.outer_surface(Vec3::Y * height, normal)?;
    Some(fitting::StrapAttachment {
        position: at,
        normal,
        rotation: Quat::from_rotation_y(normal.x.atan2(normal.z)),
    })
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

/// Deep fleece facing with a continuous backing and overlapping long tufts.
/// Keep the compact cuff envelope, but fill its lower half instead of drawing
/// two regular bead rows around a thin rope. All pile remains seated in underfur.
fn snow_cuff(p: &mut Parts, shape: BootShape, build: f32, color: Vec3) {
    for depth in [0.006, 0.012] {
        p.matte.torus(
            Vec3::Y * (shape.top - depth * build),
            Vec3::Y,
            shape.radius_top - 0.001 * build,
            0.016 * build,
            48,
            10,
            color * 0.92,
        );
    }
    for i in 0..32 {
        let variation = (i as f32 * 2.4).sin() * 0.5 + 0.5;
        let stagger = (i as f32 * 1.7).sin();
        let angle = (i as f32 + stagger * 0.18) * TAU / 32.0;
        let center = Vec3::new(
            angle.sin() * (shape.radius_top + SNOW_PILE_RADIAL_OFFSET * build),
            shape.top + (-0.009 + stagger * 0.001) * build,
            angle.cos() * (shape.radius_top + SNOW_PILE_RADIAL_OFFSET * build),
        );
        p.matte.blob(
            center,
            Vec3::new(
                SNOW_PILE_RADIUS_MAX - 0.002 + variation * 0.002,
                SNOW_PILE_HEIGHT_MAX - 0.0015 + variation * 0.0015,
                SNOW_PILE_RADIUS_MAX - 0.002 + variation * 0.002,
            ) * build,
            i as u64,
            SNOW_PILE_JITTER,
            color * (0.90 + variation * 0.10),
        );
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
        hiking_bumper(p, last, color);
    }
    if appearance == SnowBoots {
        snow_vamp(p, last, trim);
    }
}

fn hiking_bumper(p: &mut Parts, last: BootLast, color: Vec3) {
    let mut upper = Parts::default();
    boot_parts::emit(&mut upper, last, color);
    let sections = last.sections();
    // Begin behind the toe-box station so the tapered end caps overlap the
    // side edge instead of cutting diagonally across its first section.
    let center = Vec3::new(
        0.0,
        sections[4].top + 0.010,
        (sections[3].z + sections[4].z) * 0.5,
    );
    // Seat the protective rand in the actual lofted upper. A nominal ellipse
    // hovered over the narrow toe sides and looked like a detached horseshoe.
    let path: Vec<_> = (0..=24)
        .map(|i| {
            let angle = i as f32 * PI / 24.0;
            upper
                .satin
                .outer_surface(center, Vec3::new(angle.cos(), 0.0, angle.sin()))
                .expect("hiking rand rays start inside the fitted toe")
        })
        .collect();
    let radii: Vec<_> = (0..path.len())
        .map(|i| {
            let end = i.min(path.len() - 1 - i) as f32;
            let taper = 0.5 + 0.5 * (end / 2.0).min(1.0);
            // Taper height at the ends without withdrawing the lateral
            // overlap that seats the rand on the actual upper side edge.
            (0.006 * last.build, 0.006 * taper)
        })
        .collect();
    tube(&mut p.matte, &path, &radii, color * 0.36);
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
            // Each crossing joins two real, consecutive eyelet rows. The
            // former short X marks ended in the facing between eyelets.
            for k in 0..count {
                let t = 0.10 + k as f32 / (count - 1) as f32 * 0.82;
                for sign in [-1.0, 1.0] {
                    let a = Vec3::new(sign * radius(t) * 0.40, y(t), radius(t) + 0.009);
                    p.metal
                        .torus(a, Vec3::Z, 0.004 * build, 0.0015 * build, 8, 3, trim * 0.7);
                    if k + 1 < count {
                        let next = 0.10 + (k + 1) as f32 / (count - 1) as f32 * 0.82;
                        let b =
                            Vec3::new(-sign * radius(next) * 0.40, y(next), radius(next) + 0.009);
                        cord(&mut p.satin, a, b, 0.0025 * build, trim);
                    }
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
                let mut leather = p.satin.empty_like();
                emit_band(&mut leather, shape, t, 0.014, color * 0.68);
                let mount = band_attachment(&leather, y(t), side);
                p.satin.absorb(leather);
                let Some(mount) = mount else {
                    continue;
                };
                let point = |x, y| mount.position + mount.rotation * Vec3::new(x, y, 0.0);
                let corners = [
                    point(-0.012, -0.015),
                    point(0.012, -0.015),
                    point(0.012, 0.015),
                    point(-0.012, 0.015),
                ];
                for k in 0..4 {
                    cord(&mut p.metal, corners[k], corners[(k + 1) % 4], 0.003, trim);
                }
                cord(
                    &mut p.metal,
                    point(0.0, -0.012),
                    point(0.0, 0.012),
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
            riding_ring(&mut p.satin, &path, 0.004, trim);
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

    // Frozen pre-consolidation recipes independently witness coverage and
    // polygon compensation; do not rewrite these to call the new sampler.
    fn legacy_fit_shin(
        appearance: FootwearAppearance,
        shape: BootShape,
        shin: LimbShape,
        trousered: bool,
        build: f32,
    ) -> BootShape {
        let mut shape = fit_shape(appearance, shape, build);
        let required = legacy_shin_profile(shape, shin, trousered, build);
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
        if appearance == FootwearAppearance::SnowBoots {
            let contact_length = shin.length + shape.top;
            fit_snow_cuff_contact(&mut shape, build, contact_length);
        }
        shape
    }
    fn legacy_shin_profile(
        shape: BootShape,
        shin: LimbShape,
        trousered: bool,
        build: f32,
    ) -> [f32; 49] {
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
    fn legacy_riding_profile(
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
    fn envelope_values(shape: BootShape) -> [f32; 8] {
        [
            shape.top,
            shape.bottom,
            shape.radius_top,
            shape.radius_bottom,
            shape.cuff_radius,
            shape.cuff_thickness,
            shape.sample_radius_top,
            shape.sample_radius_bottom,
        ]
    }

    /// Manual construction-only benchmark. Includes fitting moved into Figure,
    /// unlike per-part mesh logs; intentionally makes no timing assertion.
    #[test]
    #[ignore = "manual isolated old/new calf-fit timing"]
    fn profile_complete_calf_fit_work() {
        let wearers: Vec<_> = [false, true]
            .into_iter()
            .flat_map(|woman| {
                (0..3).map(move |build| {
                    HumanoidBody::resolve(&Look::with_choice(
                        CreatureModel::Human,
                        7,
                        Some(Variant {
                            woman,
                            build,
                            ..default()
                        }),
                    ))
                })
            })
            .collect();
        for appearance in [
            FootwearAppearance::RidingBoots,
            FootwearAppearance::Jackboots,
            FootwearAppearance::SnowBoots,
        ] {
            for pair in 0..6 {
                for candidate in if pair % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    let start = std::time::Instant::now();
                    for body in &wearers {
                        for trousered in [false, true] {
                            let native = BootShape::fitted(body.build, -0.24);
                            if candidate {
                                let fit =
                                    fit_shin(appearance, native, body.shin, trousered, body.build);
                                std::hint::black_box(fit.envelope());
                                for _side in [-1.0, 1.0] {
                                    std::hint::black_box(&fit.riding);
                                }
                            } else {
                                let shape = legacy_fit_shin(
                                    appearance, native, body.shin, trousered, body.build,
                                );
                                std::hint::black_box(shape);
                                if appearance == FootwearAppearance::RidingBoots {
                                    for _side in [-1.0, 1.0] {
                                        std::hint::black_box(legacy_riding_profile(
                                            shape, body.shin, trousered, body.build,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    eprintln!("calf_fit_profile appearance={appearance:?} pair={pair} candidate={candidate} ms={:.6}",start.elapsed().as_secs_f64()*1000.0);
                }
            }
        }
    }

    #[test]
    fn shared_calf_grid_preserves_all_family_fit_policies() {
        for model in [CreatureModel::Human, CreatureModel::Orc] {
            for woman in [false, true] {
                for build in 0..3 {
                    let body = HumanoidBody::resolve(&Look::with_choice(
                        model,
                        7,
                        Some(Variant {
                            woman,
                            build,
                            ..default()
                        }),
                    ));
                    for trousered in [false, true] {
                        for appearance in FootwearAppearance::ALL {
                            let native = BootShape::fitted(body.build, -0.24);
                            let expected = legacy_fit_shin(
                                appearance, native, body.shin, trousered, body.build,
                            );
                            let fit =
                                fit_shin(appearance, native, body.shin, trousered, body.build);
                            assert_eq!(envelope_values(fit.envelope()), envelope_values(expected));
                            assert_eq!(
                                fit.riding.is_some(),
                                appearance == FootwearAppearance::RidingBoots
                            );
                            if let Some(grid) = fit.riding.as_deref() {
                                assert_eq!(
                                    *grid,
                                    legacy_riding_profile(
                                        expected, body.shin, trousered, body.build
                                    )
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn resolved_calf_support_is_sampled_once_for_both_legs() {
        let body = HumanoidBody::resolve(&Look::with_choice(CreatureModel::Human, 7, None));
        for appearance in FootwearAppearance::ALL {
            SHIN_SAMPLES.with(|count| count.set(0));
            let fit = fit_shin(
                appearance,
                BootShape::fitted(body.build, -0.24),
                body.shin,
                true,
                body.build,
            );
            let before = fit.riding.clone();
            let envelope = envelope_values(fit.envelope());
            for side in [-1.0, 1.0] {
                let mut first = Parts::default();
                let mut second = Parts::default();
                emit_fitted_shaft(&mut first, &fit, side, Vec3::ONE, Vec3::ONE);
                emit_fitted_shaft(&mut second, &fit, side, Vec3::ONE, Vec3::ONE);
                assert!(!first.satin.is_empty());
                for (a, b) in [
                    (&first.matte, &second.matte),
                    (&first.satin, &second.satin),
                    (&first.metal, &second.metal),
                ] {
                    assert_eq!(
                        a.surface_triangles().collect::<Vec<_>>(),
                        b.surface_triangles().collect::<Vec<_>>()
                    );
                }
            }
            assert_eq!(
                SHIN_SAMPLES.with(|count| count.get()),
                1,
                "one wearer fit serves contact and repeated meshes on both legs"
            );
            assert_eq!(fit.riding, before);
            assert_eq!(envelope_values(fit.envelope()), envelope);
        }
    }

    #[test]
    fn riding_rear_seam_is_continuous_above_the_emitted_shaft() {
        for woman in [false, true] {
            for build in [0, 2] {
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
                    let fit = fit_shin(
                        FootwearAppearance::RidingBoots,
                        BootShape::fitted(body.build, -0.24),
                        body.shin,
                        trousered,
                        body.build,
                    );
                    let shape = fit.envelope();
                    let mut parts = Parts::default();
                    emit_fitted_shaft(&mut parts, &fit, 1.0, Vec3::splat(0.2), Vec3::ZERO);
                    let mut meshes = Assets::<Mesh>::default();
                    let mut materials = Assets::<StandardMaterial>::default();
                    let model = parts.finish(
                        &mut meshes,
                        &crate::props::PropMaterials::new(&mut materials),
                    );
                    let mut wall = Builder::default();
                    let mut thread = Builder::default();
                    let wall_color = Color::srgb(0.2, 0.2, 0.2).to_linear().red;
                    let seam_color = Color::srgb(0.24, 0.24, 0.24).to_linear().red;
                    for (handle, _) in &model.0 {
                        let mesh = meshes.get(handle).unwrap();
                        let bevy::render::mesh::VertexAttributeValues::Float32x3(vertices) =
                            mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
                        else {
                            panic!("shaft positions")
                        };
                        let bevy::render::mesh::VertexAttributeValues::Float32x4(colors) =
                            mesh.attribute(Mesh::ATTRIBUTE_COLOR).unwrap()
                        else {
                            panic!("shaft colors")
                        };
                        let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                        for face in indices.as_chunks::<3>().0 {
                            let tri = face.map(|i| Vec3::from_array(vertices[i]));
                            if face
                                .iter()
                                .all(|&i| (colors[i][0] - wall_color).abs() < 1e-6)
                            {
                                wall.tri(tri, Vec3::ONE);
                            }
                            if face
                                .iter()
                                .all(|&i| (colors[i][0] - seam_color).abs() < 1e-6)
                            {
                                thread.tri(tri, Vec3::ONE);
                            }
                        }
                    }
                    assert!(
                        !wall.is_empty() && !thread.is_empty(),
                        "independent material-isolated meshes"
                    );
                    let lip_y = shape.top - 0.025;
                    assert!(thread.bounds().1.y <= lip_y + 0.004,
                        "rear seam must end beneath the dipped lip trim, not protrude above it: woman={woman} build={build} trousers={trousered} seam_top={} lip={lip_y}",thread.bounds().1.y);
                    for row in 2..46 {
                        let t = (row as f32 + 0.5) / 48.0;
                        let y = shape.bottom + (shape.top - shape.bottom) * t;
                        if y >= lip_y - 0.006 {
                            continue;
                        }
                        let origin = Vec3::Y * y;
                        let shaft = wall.outer_surface(origin, Vec3::NEG_Z).expect("shaft wall");
                        let seam = thread
                            .outer_surface(origin, Vec3::NEG_Z)
                            .expect("continuous rear seam between stations");
                        let relief = (seam - shaft).dot(Vec3::NEG_Z);
                        assert!(relief>0.0004 && relief<0.005,"rear seam must sit proud of actual shaft: woman={woman} build={build} trousers={trousered} row={row} relief={relief}");
                    }
                }
            }
        }
    }

    #[test]
    fn riding_trim_has_no_coincident_end_caps() {
        for dip in [0.0, 0.025] {
            let path: Vec<_> = (0..=24)
                .map(|i| {
                    let a = i as f32 * TAU / 24.0;
                    Vec3::new(0.065 * a.sin(), 0.28 - dip * a.cos().abs(), 0.065 * a.cos())
                })
                .collect();
            let mut parts = Parts::default();
            riding_ring(&mut parts.satin, &path, 0.004, Vec3::ONE);
            let mut meshes = Assets::<Mesh>::default();
            let mut materials = Assets::<StandardMaterial>::default();
            let model = parts.finish(
                &mut meshes,
                &crate::props::PropMaterials::new(&mut materials),
            );
            let mut count = 0;
            for (handle, _) in &model.0 {
                let mesh = meshes.get(handle).unwrap();
                let bevy::render::mesh::VertexAttributeValues::Float32x3(vertices) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
                else {
                    panic!("riding trim positions")
                };
                for vertex in vertices {
                    let v = Vec3::from_array(*vertex);
                    let distance = path
                        .iter()
                        .map(|p| v.distance(*p))
                        .fold(f32::INFINITY, f32::min);
                    assert!((distance-0.004).abs()<0.00001,"riding ring must only contain wall, not cap centers: {v:?}, distance={distance}");
                    count += 1;
                }
            }
            assert!(count >= 240);
        }
    }

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
    fn snow_cuff_vertices_fit_both_robe_contact_envelopes() {
        let body = HumanoidBody::resolve(&Look::with_choice(CreatureModel::Human, 7, None));
        for build in [0.75, 1.0, 1.4] {
            for (fitted_index, shape) in [
                fit_shape(
                    FootwearAppearance::SnowBoots,
                    BootShape::fitted(build, -0.24),
                    build,
                ),
                fit_shin(
                    FootwearAppearance::SnowBoots,
                    BootShape::fitted(build, -0.24),
                    body.shin,
                    false,
                    build,
                )
                .envelope(),
                fit_shin(
                    FootwearAppearance::SnowBoots,
                    BootShape::fitted(build, -0.24),
                    body.shin,
                    true,
                    build,
                )
                .envelope(),
            ]
            .into_iter()
            .enumerate()
            {
                let mut cuff = Parts::default();
                snow_cuff(&mut cuff, shape, build, Vec3::ONE);
                let mut meshes = Assets::<Mesh>::default();
                let mut materials = Assets::<StandardMaterial>::default();
                let model = cuff.finish(
                    &mut meshes,
                    &crate::props::PropMaterials::new(&mut materials),
                );
                let mut count = 0;
                for (handle, _) in &model.0 {
                    let mesh = meshes.get(handle).unwrap();
                    let bevy::render::mesh::VertexAttributeValues::Float32x3(vertices) =
                        mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
                    else {
                        panic!("cuff positions");
                    };
                    for vertex in vertices {
                        count += 1;
                        let point = Vec3::from_array(*vertex);
                        let radial = point.xz().length();
                        let height = point.y - shape.top;
                        // Independent cylinder used by robe cuff_far, without
                        // its additional cloth-row sweep (a stricter bound).
                        assert!(radial <= shape.cuff_radius + shape.cuff_thickness);
                        assert!(height.abs() <= shape.cuff_thickness);
                        // Actual 12-segment capsule ring used for triangle
                        // contact: radial maxima alone do not prove this.
                        let mut nearest = f32::INFINITY;
                        for k in 0..12 {
                            let center = |j: usize| {
                                let angle = j as f32 * TAU / 12.0;
                                Vec3::new(
                                    -angle.sin() * shape.cuff_radius,
                                    shape.top,
                                    -angle.cos() * shape.cuff_radius,
                                )
                            };
                            let a = center(k);
                            let edge = center(k + 1) - a;
                            let t = ((point - a).dot(edge) / edge.length_squared()).clamp(0.0, 1.0);
                            nearest = nearest.min(point.distance(a + edge * t));
                        }
                        assert!(
                            nearest <= shape.cuff_thickness,
                            "pile escapes robe capsule ring: build={build} point={point:?}"
                        );
                        // Linear sample-radius interpolation below the top;
                        // spherical cap above it. Do not clamp Y to the rim.
                        let sample = if height > 0.0 {
                            (shape.sample_radius_top.powi(2) - height.powi(2)).sqrt()
                        } else {
                            let length = if fitted_index == 0 {
                                shape.top - shape.bottom
                            } else {
                                body.shin.length + shape.top
                            };
                            let t = 1.0 + height / length;
                            shape.sample_radius_bottom
                                + (shape.sample_radius_top - shape.sample_radius_bottom) * t
                        };
                        assert!(radial <= sample, "pile escapes tapered sample contact");
                    }
                }
                assert!(count > 100, "real emitted cuff vertices required");
            }
        }
    }

    #[test]
    fn snow_cuff_has_deep_continuous_pile_not_a_thin_rope() {
        for build in [0.75, 1.0, 1.4] {
            let shape = fit_shape(
                FootwearAppearance::SnowBoots,
                BootShape::fitted(build, -0.24),
                build,
            );
            let mut cuff = Parts::default();
            snow_cuff(&mut cuff, shape, build, Vec3::ONE);
            // These lower-facing witnesses are below the old bead rows and
            // rounded rope. Probe emitted triangles, not recipe parameters.
            for depth in [0.021, 0.024] {
                let origin = Vec3::Y * (shape.top - depth * build);
                for k in 0..73 {
                    let angle = (k as f32 + 0.37) * TAU / 73.0;
                    let ray = Vec3::new(angle.sin(), 0.0, angle.cos());
                    let hit = cuff
                        .matte
                        .outer_surface(origin, ray)
                        .expect("deep fleece facing has no bare-leather gaps");
                    assert!(
                        hit.distance(origin) > shape.radius_top + 0.006 * build,
                        "lower pile must be substantial, not a dangling tip"
                    );
                }
            }
            // Recolouring cannot satisfy this silhouette contract. Keep the
            // existing compact bounds and forward-vamp support test below.
            let (low, high) = cuff.matte.bounds();
            assert!(high.y - low.y > 0.037 * build, "deep fleece band");
        }
    }

    #[test]
    fn snow_fleece_is_a_continuous_compact_roll_and_covers_the_forward_vamp() {
        for build in [0.75, 1.0, 1.4] {
            let shape = fit_shape(
                FootwearAppearance::SnowBoots,
                BootShape::fitted(build, -0.24),
                build,
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
            let fit = |appearance| fit_shape(appearance, BootShape::fitted(build, -0.24), build);
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
            let shape = fit_shape(appearance, BootShape::fitted(1.0, -0.24), 1.0);
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
    fn hiking_bumper_is_seated_on_the_actual_upper_instead_of_a_floating_hoop() {
        for build in [0.75, 1.0, 1.4] {
            for soft in [false, true] {
                let last = BootLast {
                    build,
                    heeled: false,
                };
                let mut base = Parts::default();
                boot_parts::emit(&mut base, last, Vec3::ONE);
                let mut bumper = Parts::default();
                if soft {
                    bumper.soften();
                }
                hiking_bumper(&mut bumper, last, Vec3::ONE);
                // Probe real outer-upper side edges, including the intermediate
                // loft. A nominal ellipse can be well outside this tapered last.
                let sections = last.sections();
                for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
                    let width = sections[4].width * (1.0 - t) + sections[5].width * t;
                    let z = sections[4].z * (1.0 - t) + sections[5].z * t;
                    for side in [-1.0, 1.0] {
                        let origin = Vec3::new(side * width * 0.94, -0.09, z);
                        let upper = base.satin.outer_surface(origin, Vec3::Y).unwrap();
                        let rubber = bumper
                        .matte
                        .outer_surface(origin, Vec3::Y)
                        .unwrap_or_else(|| panic!("continuous rubber covers actual toe-side edge: build={build}, t={t}, side={side}"));
                        assert!(
                            rubber.y > upper.y + 0.001,
                            "rubber remains visibly substantial"
                        );
                        assert!(rubber.y - upper.y <= 0.012,
                        "toe bumper must sit on the upper, not hover above it: build={build}, t={t}, gap={}", rubber.y - upper.y);
                    }
                }
                let origin = Vec3::new(0.0, -0.055, 0.12 * build);
                let upper = base.satin.outer_surface(origin, Vec3::Z).unwrap();
                let rubber = bumper.matte.outer_surface(origin, Vec3::Z).unwrap();
                assert!(rubber.z > upper.z + 0.001 * build);
                assert!(rubber.z < upper.z + 0.010 * build, "compact fitted nose");
                assert!(bumper.matte.bounds().0.y > BootLast::FLOOR);
                let mut meshes = Assets::<Mesh>::default();
                let mut materials = Assets::<StandardMaterial>::default();
                let model = bumper.finish(
                    &mut meshes,
                    &crate::props::PropMaterials::new(&mut materials),
                );
                let contacts = last.contacts();
                let mut checked = 0;
                for (handle, _) in &model.0 {
                    let mesh = meshes.get(handle).unwrap();
                    let bevy::render::mesh::VertexAttributeValues::Float32x3(vertices) =
                        mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
                    else {
                        panic!("bumper positions")
                    };
                    for vertex in vertices {
                        let p = Vec3::from_array(*vertex);
                        assert!(
                            contacts
                                .iter()
                                .any(
                                    |(center, radius)| ((p - *center) / *radius).length_squared()
                                        <= 1.0
                                ),
                            "bumper stays inside the existing cloth contact envelope"
                        );
                        checked += 1;
                    }
                }
                assert!(checked > 100, "check actual uploaded bumper geometry");
            }
        }
    }

    #[test]
    fn boot_cross_lacing_connects_consecutive_eyelet_rows() {
        for (appearance, rows) in [
            (FootwearAppearance::HikingBoots, 5),
            (FootwearAppearance::SnowBoots, 6),
            (FootwearAppearance::CombatBoots, 8),
            (FootwearAppearance::JungleBoots, 8),
        ] {
            for build in [0.75, 1.0, 1.4] {
                let shape = fit_shape(appearance, BootShape::fitted(build, -0.24), build);
                let mut parts = Parts::default();
                emit_shaft(
                    &mut parts,
                    appearance,
                    shape,
                    build,
                    1.0,
                    Vec3::ONE,
                    Vec3::ONE,
                );
                let y = |t: f32| shape.bottom + t * (shape.top - shape.bottom);
                let radius =
                    |t: f32| shape.radius_bottom + t * (shape.radius_top - shape.radius_bottom);
                for row in 0..rows - 1 {
                    let t = 0.10 + (row as f32 + 0.5) * 0.82 / (rows - 1) as f32;
                    let crossing = parts
                        .satin
                        .outer_surface(Vec3::new(0.0, y(t), 0.0), Vec3::Z)
                        .unwrap();
                    assert!(crossing.z > radius(t) + 0.006,
                        "{appearance:?}, build {build}, row {row}: lacing must cross between adjacent eyelet rows, not form detached short X marks");
                }
                for row in 0..rows {
                    let t = 0.10 + row as f32 * 0.82 / (rows - 1) as f32;
                    for side in [-1.0, 1.0] {
                        let rim_x = side * (radius(t) * 0.40 + 0.004 * build);
                        let rim = parts
                            .metal
                            .outer_surface(Vec3::new(rim_x, y(t), 0.0), Vec3::Z)
                            .expect("every lacing row has two actual metal eyelet rims");
                        assert!(
                            rim.z > radius(t) + 0.009,
                            "eyelet rim remains above the lacing plane"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn jungle_canvas_gusset_is_wider_and_thinner_than_combat_leather() {
        let shape = fit_shape(
            FootwearAppearance::CombatBoots,
            BootShape::fitted(1.0, -0.24),
            1.0,
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
                    let fit = fit_shin(
                        FootwearAppearance::RidingBoots,
                        BootShape::fitted(body.build, -0.24),
                        body.shin,
                        trousered,
                        body.build,
                    );
                    let mut parts = Parts::default();
                    emit_fitted_shaft(&mut parts, &fit, 1.0, Vec3::ONE, Vec3::ONE);
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
                let fit = fit_shin(
                    FootwearAppearance::RidingBoots,
                    BootShape::fitted(body.build, -0.24),
                    body.shin,
                    true,
                    body.build,
                );
                let shape = fit.envelope();
                let mut parts = Parts::default();
                emit_fitted_shaft(&mut parts, &fit, 1.0, Vec3::ONE, Vec3::ONE);
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
                        let fit = fit_shin(
                            appearance,
                            BootShape::fitted(body.build, -0.24),
                            body.shin,
                            trousered,
                            body.build,
                        );
                        let shape = fit.envelope();
                        let mut shaft = Parts::default();
                        emit_fitted_shaft(&mut shaft, &fit, 1.0, Vec3::ONE, Vec3::ONE);
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
                let shape = fit_shape(appearance, BootShape::fitted(build, -0.24), build);
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

#[cfg(test)]
mod buckle_seating_tests {
    use super::*;
    fn check(shape: BootShape, build: f32, side: f32, detail: usize) {
        let mut shaft = Parts::default();
        shaft.satin.sides_mul = detail;
        shaft.metal.sides_mul = detail;
        emit_shaft(
            &mut shaft,
            FootwearAppearance::BuckledBoots,
            shape,
            build,
            side,
            Vec3::ONE,
            Vec3::ONE,
        );
        let bands = [0.18, 0.52, 0.86].map(|t| {
            let mut band = Builder::default();
            band.sides_mul = detail;
            emit_band(&mut band, shape, t, 0.014, Vec3::ONE);
            (shape.bottom + (shape.top - shape.bottom) * t, band)
        });
        assert!(shaft.metal.surface_triangles().count() > 0);
        for at in shaft.metal.surface_triangles().flatten() {
            let (y, support) = bands
                .iter()
                .min_by(|a, b| (at.y - a.0).abs().total_cmp(&(at.y - b.0).abs()))
                .unwrap();
            // End rails intentionally overhang the 14mm half-height leather by
            // 1mm plus wire radius. Project to its side to test radial seating.
            let hit = support
                .outer_surface(
                    Vec3::new(at.x, at.y.clamp(y - 0.013, y + 0.013), -0.2),
                    Vec3::Z,
                )
                .expect("hardware footprint stays over band");
            let gap = at.z - hit.z;
            assert!(
                (-0.005..=0.008).contains(&gap),
                "band rail gap={gap} build={build} side={side} detail={detail}"
            );
        }
    }
    #[test]
    fn buckles_follow_their_band_surface_across_builds_and_sides() {
        for build in [0.85, 1.0, 1.15] {
            let shape = fit_shape(
                FootwearAppearance::BuckledBoots,
                BootShape::fitted(build, -0.24),
                build,
            );
            for side in [-1.0, 1.0] {
                for detail in [1, 2, 3] {
                    check(shape, build, side, detail);
                }
            }
        }
    }
    #[test]
    fn buckle_mounts_follow_actual_anatomy_and_trouser_fits() {
        for model in [CreatureModel::Human, CreatureModel::Orc] {
            for woman in [false, true] {
                for build in 0..3 {
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
                    for trousers in [false, true] {
                        let fitted = fit_shin(
                            FootwearAppearance::BuckledBoots,
                            BootShape::fitted(body.build, -0.24),
                            body.shin,
                            trousers,
                            body.build,
                        );
                        for side in [-1.0, 1.0] {
                            check(fitted.envelope(), body.build, side, 1);
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn band_mount_preserves_leather_recipe_and_recipe_frame() {
        assert!(band_attachment(&Builder::default(), 0.0, 1.0).is_none());
        let shape = BootShape::fitted(1.0, -0.24);
        for detail in [1, 2, 3] {
            for smooth in [false, true] {
                for transformed in [false, true] {
                    let mut parts = Parts::default();
                    if transformed {
                        parts.set_transform(Affine3A::from_scale_rotation_translation(
                            Vec3::new(1.1, 0.9, 1.2),
                            Quat::from_rotation_y(0.4),
                            Vec3::new(0.3, 0.2, -0.1),
                        ));
                    }
                    let mut target = parts.satin;
                    target.sides_mul = detail;
                    target.smooth = smooth;
                    let mut old = target.empty_like();
                    let mut band = target.empty_like();
                    let t = 0.52;
                    let y = shape.bottom + (shape.top - shape.bottom) * t;
                    let radius = shape.radius_bottom + (shape.radius_top - shape.radius_bottom) * t;
                    old.prism(
                        Vec3::Y * (y + 0.014),
                        Vec3::Y * (y - 0.014),
                        radius + 0.004,
                        radius + 0.004,
                        12,
                        Vec3::ONE,
                        0.0,
                    );
                    emit_band(&mut band, shape, t, 0.014, Vec3::ONE);
                    old.assert_same_recipe(&band);
                    for side in [-1.0, 1.0] {
                        let mount = band_attachment(&band, y, side).unwrap();
                        assert!(mount.normal.x * side > 0.0 && mount.normal.z > 0.0);
                        assert!((mount.rotation * Vec3::Y - Vec3::Y).length() < 1e-5);
                        assert!((mount.rotation * Vec3::Z - mount.normal).length() < 1e-4);
                        assert!((mount.position.y - y).abs() < 1e-5);
                    }
                    target.absorb(band);
                    old.assert_same_recipe(&target);
                }
            }
        }
    }
}
