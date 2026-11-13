use super::*;

#[test]
fn chest_wrap_trim_reaches_the_actual_boundary_on_wider_support() {
    use crate::sdf::{Sculpt, Shape};
    let wrap = CentaurChestWrap {
        low: 0.2,
        high: 0.4,
        dip: 0.35,
        apex_x: 0.1,
    };
    let mut extents = Vec::new();
    for width in [0.18, 0.28] {
        let mut support = Sculpt::default();
        support.add(
            Shape::Ellipsoid(Vec3::new(0.0, 0.3, 0.0), Vec3::new(width, 0.3, 0.15)),
            0.0,
        );
        let mut b = Builder::default();
        wrap.emit_trim(&mut b, &support);
        let points: Vec<_> = b
            .surface_triangles()
            .flatten()
            .filter(|p| p.y < 0.23)
            .collect();
        assert!(!points.is_empty(), "real lower trim witness");
        let farthest = points.iter().map(|p| p.x.abs()).fold(0.0, f32::max);
        // Independent analytic silhouette of this ellipsoid at y=.2.
        let edge = width * (1.0_f32 - (0.1_f32 / 0.3).powi(2)).sqrt();
        assert!(
            farthest > edge * 0.88 && farthest < edge * 1.02,
            "trim must track actual support width {width}, emitted {farthest}, silhouette {edge}"
        );
        extents.push(farthest);
    }
    assert!(
        extents[1] > extents[0] * 1.4,
        "fixed endpoint cannot follow width changes"
    );
}

fn trim_profile() -> BoundaryTrimProfile {
    BoundaryTrimProfile {
        half_width: 0.012,
        support_fraction: 0.94,
        terminal_width_fraction: 0.2,
        taper_start: 0.75,
    }
}

#[test]
fn boundary_trim_resolves_each_side_and_preserves_actual_finite_taper() {
    use crate::sdf::{Sculpt, Shape};
    for front in [true, false] {
        let mut support = Sculpt::default();
        support.add(
            Shape::Ellipsoid(Vec3::new(0.035, 0.3, 0.0), Vec3::new(0.24, 0.3, 0.15)),
            0.0,
        );
        let profile = trim_profile();
        let path = profile.path(&support, &|_| 0.3, front);
        assert_eq!(path.len(), 49);
        assert!((path.first().unwrap().0.x + 0.205 * 0.94).abs() < 0.001);
        assert!((path.last().unwrap().0.x - 0.275 * 0.94).abs() < 0.001);
        let fit = profile.fit(&support, &|_| 0.3, front, 0.011);
        assert_eq!(fit.strips.len(), 1);
        let rows = &fit.strips[0];
        for row in [rows.first().unwrap(), rows.last().unwrap()] {
            let width = row
                .first()
                .unwrap()
                .0
                .truncate()
                .distance(row.last().unwrap().0.truncate());
            assert!(
                (width - 0.0048).abs() < 1e-5,
                "actual finite tapered terminal {width}"
            );
        }
        let middle = &rows[rows.len() / 2];
        assert!(
            (middle
                .first()
                .unwrap()
                .0
                .truncate()
                .distance(middle.last().unwrap().0.truncate())
                - 0.024)
                .abs()
                < 1e-5
        );
        let mut b = Builder::default();
        fit.emit(&mut b, Vec3::ONE, true);
        assert!(b.surface_triangles().count() > 100);
        for t in b.surface_triangles() {
            assert!(
                (t[1] - t[0]).cross(t[2] - t[0]).length() > 1e-10,
                "no collapsed tips"
            );
            for p in [
                t[0],
                t[1],
                t[2],
                (t[0] + t[1]) * 0.5,
                (t[1] + t[2]) * 0.5,
                (t[2] + t[0]) * 0.5,
                (t[0] + t[1] + t[2]) / 3.0,
            ] {
                assert!(p.is_finite());
                let d = support.distance(p);
                assert!(
                    (0.002..=0.013).contains(&d),
                    "complete sampled strip clearance {d}"
                );
            }
        }
    }
}

#[test]
fn boundary_trim_rejects_missing_center_and_stops_at_first_support_gap() {
    use crate::sdf::{Sculpt, Shape};
    let profile = trim_profile();
    let mut support = Sculpt::default();
    assert!(profile.path(&support, &|_| 0.3, true).is_empty());
    support.add(
        Shape::Ellipsoid(Vec3::new(0.3, 0.3, 0.0), Vec3::splat(0.06)),
        0.0,
    );
    assert!(
        profile.path(&support, &|_| 0.3, true).is_empty(),
        "detached island must not become a center attachment"
    );
    support.add(
        Shape::Ellipsoid(Vec3::new(0.0, 0.3, 0.0), Vec3::splat(0.12)),
        0.0,
    );
    let path = profile.path(&support, &|_| 0.3, true);
    assert_eq!(path.len(), 49);
    assert!(
        path.iter().all(|(p, _)| p.x.abs() < 0.115),
        "do not bridge to detached island"
    );
    let fit = profile.fit(&support, &|_| 0.3, true, 0.011);
    assert_eq!(fit.strips.len(), 1);
    assert!(fit.strips[0]
        .iter()
        .flatten()
        .all(|(p, _)| p.x.abs() < 0.116));
}

#[test]
fn chest_wrap_paint_and_trim_preserve_the_authored_sweetheart_boundary() {
    for volume in [0, 2, 4] {
        for lean in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Centaur,
                17,
                Some(Variant {
                    woman: true,
                    bust: volume,
                    bust_lean: lean,
                    ..default()
                }),
            );
            let bust = super::super::super::anatomy::centaur_bust(&look).unwrap();
            let wrap = CentaurChestWrap::new(bust);
            let apex = bust.apex(1.0);
            assert_eq!(wrap.edges(0.0).0, bust.fold - 0.05);
            assert!((wrap.edges(0.0).1 - (apex.y - 0.005)).abs() < 1e-6);
            assert!((wrap.edges(apex.x).1 - (apex.y + bust.radii.y * 0.62)).abs() < 1e-6);
            for x in [-0.18, -0.12, -0.06, 0.0, 0.06, 0.12, 0.18] {
                let (low, high) = wrap.edges(x);
                assert_eq!(
                    wrap.edges(-x),
                    (low, high),
                    "retain authored symmetric trim, not mirrored anatomy"
                );
                assert!(!wrap.covers(Vec3::new(x, low - 0.001, 0.1)));
                assert!(wrap.covers(Vec3::new(x, low + 0.001, 0.1)));
                assert!(wrap.covers(Vec3::new(x, high - 0.001, 0.1)));
                assert!(!wrap.covers(Vec3::new(x, high + 0.001, 0.1)));
                assert!(wrap.covers(Vec3::new(x, 0.409, -0.1)));
                assert!(!wrap.covers(Vec3::new(x, 0.411, -0.1)));
            }
        }
    }
    let look = Look::with_choice(
        CreatureModel::Centaur,
        17,
        Some(Variant {
            woman: false,
            ..default()
        }),
    );
    assert!(
        super::super::super::anatomy::centaur_bust(&look).is_none(),
        "male native style remains bare under the baldric"
    );
}

#[test]
fn chest_wrap_actual_trim_keeps_support_contact_across_native_chest_variants() {
    for volume in [0, 2, 4] {
        for form in [0, 3, 6, 8] {
            for lean in 0..3 {
                let look = Look::with_choice(
                    CreatureModel::Centaur,
                    17,
                    Some(Variant {
                        woman: true,
                        bust: volume,
                        bust_form: form,
                        bust_lean: lean,
                        ..default()
                    }),
                );
                let support = centaur_torso_support(&look);
                let wrap = CentaurChestWrap::new(
                    super::super::super::anatomy::centaur_bust(&look).unwrap(),
                );
                for top in [true, false] {
                    let fit = wrap.trim(&support, top);
                    assert_eq!(
                        fit.strips.len(),
                        1,
                        "continuous native trim {volume} {form} {lean} {top}"
                    );
                    let rows = &fit.strips[0];
                    for row in [rows.first().unwrap(), rows.last().unwrap()] {
                        assert!(
                            (row.first()
                                .unwrap()
                                .0
                                .truncate()
                                .distance(row.last().unwrap().0.truncate())
                                - 0.0048)
                                .abs()
                                < 1e-5,
                            "native terminal taper must survive row support"
                        );
                    }
                    if top {
                        for row in rows {
                            let p = row[row.len() / 2].0;
                            let skin = torso_surface_hit(&support, p.x, p.y, true).unwrap();
                            assert!(
                                skin.z > 0.0,
                                "front trim must remain on sweetheart paint branch: {skin}"
                            );
                        }
                    }
                }
                let mut b = Builder::default();
                wrap.emit_trim(&mut b, &support);
                assert!(b.surface_triangles().count() > 100);
                for t in b.surface_triangles() {
                    assert!(
                        (t[1] - t[0]).cross(t[2] - t[0]).length() > 1e-10,
                        "nondegenerate native trim"
                    );
                    for p in [
                        t[0],
                        t[1],
                        t[2],
                        (t[0] + t[1]) * 0.5,
                        (t[1] + t[2]) * 0.5,
                        (t[2] + t[0]) * 0.5,
                        (t[0] + t[1] + t[2]) / 3.0,
                    ] {
                        let d = support.distance(p);
                        assert!(
                            (0.001..=0.014).contains(&d),
                            "volume {volume} form {form} lean {lean}, sampled clearance {d} at {p}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn boundary_trim_rejects_support_without_a_silhouette_in_the_torso_search() {
    use crate::sdf::{Sculpt, Shape};
    let mut support = Sculpt::default();
    support.add(
        Shape::Ellipsoid(Vec3::new(0.0, 0.3, 0.0), Vec3::new(0.8, 0.3, 0.15)),
        0.0,
    );
    assert!(trim_profile().path(&support, &|_| 0.3, true).is_empty());
}
