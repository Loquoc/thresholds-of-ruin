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
    let laced = matches!(
        appearance,
        WalkingShoes | CombatBoots | JungleBoots | HikingBoots | SnowBoots
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
        fur_parts::rim(
            p,
            super::super::outfit::Fringe {
                kind: super::super::outfit::FringeKind::Fur,
                color: trim,
                accent: trim * 0.7,
                length: 0.022,
            },
            Vec3::new(0.0, 0.015, 0.065 * b),
            Vec2::new(0.056 * b, 0.045 * b),
            0.55,
        );
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
            band(p, shape, 0.9, 0.003, trim);
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
            // Broad tongue under cross-laces; the lighter canvas upper is an
            // independently modeled panel on jungle boots.
            p.matte.cuboid(
                Vec3::new(0.0, y(0.5), radius(0.5)),
                Vec3::new(radius(0.5) * 0.50, (shape.top - shape.bottom) * 0.47, 0.006),
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
                fur_parts::rim(
                    p,
                    super::super::outfit::Fringe {
                        kind: super::super::outfit::FringeKind::Fur,
                        color: trim,
                        accent: trim * 0.7,
                        length: 0.05,
                    },
                    Vec3::Y * shape.top,
                    Vec2::splat(shape.radius_top + 0.002),
                    build,
                );
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
                        emit_shaft(
                            &mut shaft,
                            appearance,
                            shape,
                            body.build,
                            1.0,
                            Vec3::ONE,
                            Vec3::ONE,
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
                    appearance == FootwearAppearance::HardShoes
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
