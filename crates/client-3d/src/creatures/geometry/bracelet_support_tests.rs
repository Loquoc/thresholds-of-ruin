use super::*;

fn royal_robe(model: CreatureModel, build: u8) -> Look {
    let mut look = Look::with_choice(
        model,
        22,
        Some(Variant {
            woman: true,
            build,
            ..default()
        }),
    )
    .dressed(Wardrobe::RoyalPrincess);
    look.equipment.select("robe").unwrap();
    look
}

fn parts(plan: Plan, body: HumanoidBody, side: f32, smooth: bool) -> Parts {
    let mut parts = Parts::default();
    if smooth {
        parts.soften();
    }
    plan.emit(&mut parts, body, side);
    parts
}

#[test]
fn actual_royal_robe_retains_the_complete_underlying_bracelet() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Orc,
        CreatureModel::Nymph,
    ] {
        for build in 0..3 {
            let look = royal_robe(model, build);
            let body = HumanoidBody::resolve(&look);
            let plan = Plan::resolve(&look, &Composition::resolve(&look).outfit);
            assert_eq!(plan.bracelet.unwrap().support, BraceletSupport::Forearm);
            for smooth in [false, true] {
                for side in [-1., 1.] {
                    let actual = parts(plan, body, side, smooth);
                    let ring: Vec<_> = actual.metal.surface_triangles().collect();
                    assert_eq!(
                        ring.len(),
                        384,
                        "full production lower arm retains underlying bracelet"
                    );
                    super::super::bracelet_tests::closed_ring(&ring);
                    let base = parts(
                        Plan {
                            cuff: None,
                            bracelet: None,
                            outer_sleeve: None,
                            glove: None,
                            ..plan
                        },
                        body,
                        side,
                        smooth,
                    );
                    for tri in &ring {
                        for at in [
                            tri[0],
                            tri[1],
                            tri[2],
                            (tri[0] + tri[1]) * 0.5,
                            (tri[1] + tri[2]) * 0.5,
                            (tri[2] + tri[0]) * 0.5,
                            (tri[0] + tri[1] + tri[2]) / 3.,
                        ] {
                            let center = Vec3::Y * at.y;
                            let out = (at - center).normalize();
                            let hit = base
                                .matte
                                .outer_surface(center, out)
                                .expect("actual bare support");
                            let gap = (at - hit).dot(out);
                            assert!(
                                (0.001..=0.018).contains(&gap),
                                "{model:?} build={build} gap={gap}"
                            );
                        }
                    }
                    // All cuff and equipment robe triangles follow the base.
                    let base_count = base.matte.surface_triangles().count();
                    let native = parts(
                        Plan {
                            bracelet: None,
                            outer_sleeve: None,
                            glove: None,
                            ..plan
                        },
                        body,
                        side,
                        smooth,
                    );
                    let native_count = native.matte.surface_triangles().count();
                    assert!(
                        assert_outside_band(
                            native.matte.surface_triangles().skip(base_count),
                            &ring
                        ) > 0,
                        "native cuff overlap witness"
                    );
                    assert!(
                        assert_outside_band(
                            actual.matte.surface_triangles().skip(native_count),
                            &ring
                        ) > 0,
                        "equipment sleeve overlap witness"
                    );
                    assert_unchanged_except_bracelet(
                        &actual,
                        &parts(
                            Plan {
                                bracelet: None,
                                ..plan
                            },
                            body,
                            side,
                            smooth,
                        ),
                    );
                }
            }
        }
    }
}

fn assert_unchanged_except_bracelet(actual: &Parts, without: &Parts) {
    for (a, b) in [
        &actual.matte,
        &actual.satin,
        &actual.sheer,
        &actual.ghost,
        &actual.glow,
    ]
    .into_iter()
    .zip([
        &without.matte,
        &without.satin,
        &without.sheer,
        &without.ghost,
        &without.glow,
    ]) {
        a.assert_same_recipe(b);
    }
    // This also checks later metal glove buckles, not just cloth buckets.
    actual.metal.assert_same_recipe_after(384, &without.metal);
}

#[test]
fn later_gloves_and_outer_robe_keep_geometry_and_clear_underlying_band() {
    for build in [0, 2] {
        for glove in [
            "old-gloves",
            "padded-gloves",
            "riding-gloves",
            "fencing-gloves",
        ] {
            let mut look = royal_robe(CreatureModel::Human, build);
            look.equipment.select(glove).unwrap();
            let body = HumanoidBody::resolve(&look);
            let plan = Plan::resolve(&look, &Composition::resolve(&look).outfit);
            for side in [-1., 1.] {
                let actual = parts(plan, body, side, true);
                let ring: Vec<_> = actual.metal.surface_triangles().take(384).collect();
                let without = parts(
                    Plan {
                        bracelet: None,
                        ..plan
                    },
                    body,
                    side,
                    true,
                );
                assert_unchanged_except_bracelet(&actual, &without);
                let base = parts(
                    Plan {
                        cuff: None,
                        bracelet: None,
                        outer_sleeve: None,
                        glove: None,
                        ..plan
                    },
                    body,
                    side,
                    true,
                );
                assert_outside_band(
                    actual
                        .matte
                        .surface_triangles()
                        .skip(base.matte.surface_triangles().count()),
                    &ring,
                );
                assert_outside_band(actual.satin.surface_triangles(), &ring);
                assert_outside_band(actual.metal.surface_triangles().skip(384), &ring);
            }
        }
    }
}

// A conservative whole-triangle separation oracle, independent of the fitter.
// Test every garment triangle intersecting the band's axial slab against the
// band's enclosing cylinder, including open/scalloped edges and triangle interiors.
fn assert_outside_band(garment: impl Iterator<Item = [Vec3; 3]>, ring: &[[Vec3; 3]]) -> usize {
    assert!(!ring.is_empty(), "clearance requires a band");
    let mut overlaps = 0;
    let low = ring
        .iter()
        .flatten()
        .map(|p| p.y)
        .fold(f32::INFINITY, f32::min);
    let high = ring
        .iter()
        .flatten()
        .map(|p| p.y)
        .fold(f32::NEG_INFINITY, f32::max);
    let radius = ring
        .iter()
        .flatten()
        .map(|p| Vec2::new(p.x, p.z).length())
        .fold(0., f32::max);
    for triangle in garment {
        if triangle.iter().all(|p| p.y < low) || triangle.iter().all(|p| p.y > high) {
            continue;
        }
        overlaps += 1;
        let distance = projected_distance(triangle.map(|p| Vec2::new(p.x, p.z)));
        assert!(
            distance > radius + 0.0001,
            "garment separation {distance}, bracelet cylinder {radius}"
        );
    }
    overlaps
}

fn projected_distance(p: [Vec2; 3]) -> f32 {
    let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
    let signs = [
        cross(p[1] - p[0], -p[0]),
        cross(p[2] - p[1], -p[1]),
        cross(p[0] - p[2], -p[2]),
    ];
    if cross(p[1] - p[0], p[2] - p[0]).abs() > 1e-12
        && (signs.iter().all(|v| *v >= 0.) || signs.iter().all(|v| *v <= 0.))
    {
        return 0.;
    }
    [(p[0], p[1]), (p[1], p[2]), (p[2], p[0])]
        .into_iter()
        .map(|(a, b)| {
            let edge = b - a;
            let t = if edge.length_squared() > 1e-20 {
                (-a.dot(edge) / edge.length_squared()).clamp(0., 1.)
            } else {
                0.
            };
            (a + t * edge).length()
        })
        .fold(f32::INFINITY, f32::min)
}

#[test]
fn separation_oracle_checks_interiors_edges_and_degenerate_projections() {
    assert_eq!(
        projected_distance([Vec2::new(-1., -1.), Vec2::new(1., -1.), Vec2::new(0., 1.)]),
        0.
    );
    assert_eq!(
        projected_distance([Vec2::new(-1., 1.), Vec2::new(1., 1.), Vec2::new(0., 2.)]),
        1.
    );
    assert_eq!(
        projected_distance([Vec2::new(-1., 1.), Vec2::new(1., 1.), Vec2::new(1., 1.)]),
        1.
    );
}

#[test]
fn support_choice_is_bounded_and_missing_selected_support_does_not_fallback() {
    let look = royal_robe(CreatureModel::Human, 1);
    let outfit = Composition::resolve(&look).outfit;
    let plan = Plan::resolve(&look, &outfit);
    let body = HumanoidBody::resolve(&look);
    let missing = parts(
        Plan {
            surface: Surface::NoOrganicBase,
            ..plan
        },
        body,
        1.,
        true,
    );
    assert_eq!(
        missing.metal.surface_triangles().count(),
        0,
        "later robe must not substitute for missing selected base"
    );
    let native_sleeve = match outfit.arms {
        Arms::Styled(s) => s,
        _ => panic!("royal styled sleeve"),
    };
    for arms in [Arms::Plate, Arms::Sleeves(Vec3::X)].into_iter().chain(
        [SleeveCut::Bell, SleeveCut::Gauntlet, SleeveCut::Draped].map(|cut| {
            Arms::Styled(crate::creatures::outfit::Sleeve {
                cut,
                ..native_sleeve
            })
        }),
    ) {
        let mut changed = outfit;
        changed.arms = arms;
        assert_eq!(
            Plan::resolve(&look, &changed).bracelet.unwrap().support,
            BraceletSupport::ForearmAndCuff
        );
    }
    let mut fur = look.clone();
    fur.style = Style::Yeti;
    assert_eq!(
        Plan::resolve(&fur, &outfit).bracelet.unwrap().support,
        BraceletSupport::ForearmAndCuff
    );
    let mut bare = look.clone();
    bare.equipment = Equipment::default();
    assert_eq!(
        Plan::resolve(&bare, &Composition::resolve(&bare).outfit)
            .bracelet
            .unwrap()
            .support,
        BraceletSupport::ForearmAndCuff
    );
}
