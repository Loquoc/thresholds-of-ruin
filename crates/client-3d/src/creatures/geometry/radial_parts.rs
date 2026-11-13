//! Shared support-fitted radial bands in translated local-Y frames.
use super::*;

/// Farthest selected mesh-layer hit in the caller's common recipe frame.
/// Selected builders must share that frame. `outward` is a unit direction;
/// origin, material selection and any allowance
/// belong to the item. A missing hit stays missing, without a nominal fallback.
/// Query before emission so an attachment cannot become its own support.
pub(super) fn outermost_layer_radius(
    layers: &[&Builder],
    origin: Vec3,
    outward: Vec3,
) -> Option<f32> {
    layers
        .iter()
        .filter_map(|builder| builder.outer_surface(origin, outward))
        .map(|hit| (hit - origin).dot(outward))
        .reduce(f32::max)
}

pub(super) fn support_point(
    support: &crate::sdf::Sculpt,
    support_to_output_y: f32,
    height: f32,
    angle: f32,
    gap: f32,
) -> Vec3 {
    let out = Vec3::new(angle.sin(), 0.0, angle.cos());
    let y = height - support_to_output_y;
    let surface = support.onto(Vec3::Y * y + out * 0.5, -out);
    let mut near = surface.dot(out);
    let mut far = near + 0.04;
    // Keep the authored height while offsetting by real surface clearance.
    // A radial offset alone gets thinner on a steeply flaring hip.
    for _ in 0..14 {
        let radius = (near + far) * 0.5;
        let sample = out * radius + Vec3::Y * y;
        if support.distance(sample) < gap {
            near = radius;
        } else {
            far = radius;
        }
    }
    out * ((near + far) * 0.5) + Vec3::Y * height
}

/// Authored band dimensions in the emission frame, with explicit support-frame
/// translation. Anatomy stays in the actual support rather than proxy radii.
pub(super) struct RadialBandProfile {
    pub heights: Vec2,
    pub support_to_output_y: f32,
    pub tilt: f32,
    pub clearance: f32,
    pub thickness: f32,
    pub rows: usize,
    pub sides: usize,
}

impl RadialBandProfile {
    pub fn point(
        &self,
        support: &crate::sdf::Sculpt,
        height: f32,
        angle: f32,
        outer: bool,
    ) -> Vec3 {
        support_point(
            support,
            self.support_to_output_y,
            height,
            angle,
            self.clearance + if outer { self.thickness } else { 0.0 },
        )
    }

    pub fn emit(&self, b: &mut Builder, support: &crate::sdf::Sculpt, color: Vec3) {
        let sides = self.sides.max(3);
        let rows = self.rows.max(1);
        let rings: Vec<_> = (0..=rows)
            .map(|row| {
                let height =
                    self.heights.x + (self.heights.y - self.heights.x) * row as f32 / rows as f32;
                (0..sides)
                    .map(|k| {
                        let angle = k as f32 * TAU / sides as f32;
                        let height = height + self.tilt * angle.sin();
                        [
                            self.point(support, height, angle, false),
                            self.point(support, height, angle, true),
                        ]
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let mut quad = |points: [Vec3; 4], facing: Vec3| {
            let inside = points.iter().copied().sum::<Vec3>() * 0.25 - facing * 0.01;
            b.tri_out([points[0], points[1], points[2]], inside, color);
            b.tri_out([points[0], points[2], points[3]], inside, color);
        };
        for k in 0..sides {
            let next = (k + 1) % sides;
            let angle = (k as f32 + 0.5) * TAU / sides as f32;
            let out = Vec3::new(angle.sin(), 0.0, angle.cos());
            for row in 0..rows {
                for (side, facing) in [(0, -1.0), (1, 1.0)] {
                    quad(
                        [
                            rings[row][k][side],
                            rings[row][next][side],
                            rings[row + 1][next][side],
                            rings[row + 1][k][side],
                        ],
                        out * facing,
                    );
                }
            }
            for (row, facing) in [(0, Vec3::NEG_Y), (rows, Vec3::Y)] {
                quad(
                    [
                        rings[row][k][0],
                        rings[row][next][0],
                        rings[row][next][1],
                        rings[row][k][1],
                    ],
                    facing,
                );
            }
        }
    }
}

/// Retain only triangles that can intersect a horizontal query in this window.
/// This is a conservative construction-time filter, not a fitted approximation.
/// Builders already expose triangles in their common recipe coordinates.
fn band_support_window(layers: &[&Builder], low: f32, high: f32) -> Builder {
    let mut support = Builder::default();
    for layer in layers {
        for triangle in layer.surface_triangles() {
            let min = triangle.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
            let max = triangle
                .iter()
                .map(|p| p.y)
                .fold(f32::NEG_INFINITY, f32::max);
            // Include the ray/triangle barycentric tolerance at the boundary.
            let tolerance = 0.0001 + (max - min) * 0.0001;
            if max + tolerance >= low && min - tolerance <= high {
                support.tri(triangle, Vec3::ONE);
            }
        }
    }
    support
}

/// A closed rounded band resolved before emission. The caller owns its support
/// and attachment frame; dimensions and tessellation belong to the ornament.
/// Missing support rejects the whole ring rather than leaving a floating arc.
#[derive(Clone, Copy)]
pub(super) struct RoundedBandProfile {
    pub height: f32,
    pub half_height: f32,
    pub clearance: f32,
    pub radial_half: f32,
    pub sides: usize,
    pub sections: usize,
}

impl RoundedBandProfile {
    /// Fit to the retained mesh layers in a local-Y limb frame. Resolve a
    /// bounded angular/axial footprint before emission, so the ornament never
    /// supports itself and facets bridge an abrupt sleeve/cuff edge.
    /// This is sampled construction-time support, not arbitrary disconnected
    /// geometry or later-layer collision clearance.
    pub fn resolve_layers(
        self,
        p: &Parts,
        color: Vec3,
        max_relief: Option<f32>,
    ) -> Option<Builder> {
        let layers = band_support_window(
            &[&p.matte, &p.metal, &p.satin, &p.sheer, &p.ghost, &p.glow],
            self.height - self.half_height,
            self.height + self.half_height,
        );
        const RAYS: usize = 96;
        let mut support = [f32::NEG_INFINITY; RAYS];
        for row in 0..=8 {
            let y = self.height + (row as f32 / 4.0 - 1.0) * self.half_height;
            // Each ray is horizontal: triangles elsewhere in the band cannot
            // support this row. Keep the full window for later relief checks.
            let axial = band_support_window(&[&layers], y, y);
            for (column, radius) in support.iter_mut().enumerate() {
                let angle = column as f32 * TAU / RAYS as f32;
                let out = Vec3::new(angle.sin(), 0.0, angle.cos());
                let hit = outermost_layer_radius(&[&axial], Vec3::Y * y, out)?;
                *radius = radius.max(hit);
            }
        }
        // Resolve before adding any ornament, so it cannot become its own support.
        let ring = self.resolve(
            |height, angle, gap| {
                let index = (angle.rem_euclid(TAU) / TAU * RAYS as f32).round() as usize % RAYS;
                // Both neighboring angular cells support each emitted vertex. The
                // polygon inradius correction protects the facets between vertices.
                let radius = (-4..=4)
                    .map(|offset| {
                        support[(index as isize + offset).rem_euclid(RAYS as isize) as usize]
                    })
                    .reduce(f32::max)?;
                let radius = radius / (PI / self.sides as f32).cos() + gap;
                Some(Vec3::new(
                    angle.sin() * radius,
                    height,
                    angle.cos() * radius,
                ))
            },
            color,
        )?;
        if let Some(limit) = max_relief {
            if !limit.is_finite() || limit < 0.0 {
                return None;
            }
            // A scalloped cuff can end inside the band's authored height.
            // Do not bridge that cliff into a floating ornament. Validate the
            // complete candidate, including connecting facet samples, against
            // the same retained layers before appending any new geometry.
            for triangle in ring.surface_triangles() {
                for at in [
                    triangle[0],
                    triangle[1],
                    triangle[2],
                    (triangle[0] + triangle[1]) * 0.5,
                    (triangle[1] + triangle[2]) * 0.5,
                    (triangle[2] + triangle[0]) * 0.5,
                    (triangle[0] + triangle[1] + triangle[2]) / 3.0,
                ] {
                    let origin = Vec3::Y * at.y;
                    let out = (at - origin).try_normalize()?;
                    let support = outermost_layer_radius(&[&layers], origin, out)?;
                    let relief = (at - origin).dot(out) - support;
                    if !relief.is_finite() || !(0.0..=limit).contains(&relief) {
                        return None;
                    }
                }
            }
        }
        Some(ring)
    }

    pub fn resolve(
        self,
        support: impl Fn(f32, f32, f32) -> Option<Vec3>,
        color: Vec3,
    ) -> Option<Builder> {
        let sides = self.sides.max(3);
        let sections = self.sections.max(4);
        let rings: Vec<Vec<Vec3>> = (0..sides)
            .map(|i| {
                let angle = i as f32 * TAU / sides as f32;
                (0..sections)
                    .map(|j| {
                        let cross = j as f32 * TAU / sections as f32;
                        support(
                            self.height + cross.sin() * self.half_height,
                            angle,
                            self.clearance + self.radial_half * (1.0 + cross.cos()),
                        )
                        .filter(|point| point.is_finite())
                    })
                    .collect::<Option<Vec<_>>>()
            })
            .collect::<Option<_>>()?;
        // Keep the mid-height support checks as well as the surface samples.
        // This rejects a ring that would bridge an unsupported center section.
        for i in 0..sides {
            support(
                self.height,
                i as f32 * TAU / sides as f32,
                self.clearance + self.radial_half,
            )
            .filter(|point| point.is_finite())?;
        }
        let mut ring = Builder::default();
        for i in 0..sides {
            let next = (i + 1) % sides;
            for j in 0..sections {
                let end = (j + 1) % sections;
                let q = [rings[i][j], rings[next][j], rings[next][end], rings[i][end]];
                // Increasing angle and cross-section phase wind outward.
                // A single mid-height "inside" point can lie outside a narrow
                // bent section and incorrectly reverse individual triangles.
                ring.tri([q[0], q[1], q[2]], color);
                ring.tri([q[0], q[2], q[3]], color);
            }
        }
        Some(ring)
    }
}

#[cfg(test)]
mod band_profile_tests {
    use super::*;
    fn legacy_waist_emit(
        profile: &RadialBandProfile,
        b: &mut Builder,
        support: &crate::sdf::Sculpt,
        color: Vec3,
    ) {
        const SIDES: usize = 48;
        let rows = profile.rows.max(1);
        let rings: Vec<_> = (0..=rows)
            .map(|row| {
                let height = profile.heights.x
                    + (profile.heights.y - profile.heights.x) * row as f32 / rows as f32;
                std::array::from_fn::<_, SIDES, _>(|k| {
                    let angle = k as f32 * TAU / SIDES as f32;
                    let height = height + profile.tilt * angle.sin();
                    [
                        profile.point(support, height, angle, false),
                        profile.point(support, height, angle, true),
                    ]
                })
            })
            .collect();
        let mut quad = |points: [Vec3; 4], facing: Vec3| {
            let inside = points.iter().copied().sum::<Vec3>() * 0.25 - facing * 0.01;
            b.tri_out([points[0], points[1], points[2]], inside, color);
            b.tri_out([points[0], points[2], points[3]], inside, color);
        };
        for k in 0..SIDES {
            let next = (k + 1) % SIDES;
            let angle = (k as f32 + 0.5) * TAU / SIDES as f32;
            let out = Vec3::new(angle.sin(), 0.0, angle.cos());
            for row in 0..rows {
                for (side, facing) in [(0, -1.0), (1, 1.0)] {
                    quad(
                        [
                            rings[row][k][side],
                            rings[row][next][side],
                            rings[row + 1][next][side],
                            rings[row + 1][k][side],
                        ],
                        out * facing,
                    );
                }
            }
            for (row, facing) in [(0, Vec3::NEG_Y), (rows, Vec3::Y)] {
                quad(
                    [
                        rings[row][k][0],
                        rings[row][next][0],
                        rings[row][next][1],
                        rings[row][k][1],
                    ],
                    facing,
                );
            }
        }
    }
    #[test]
    fn radial_band_budget_preserves_legacy_waist_geometry() {
        let mut support = crate::sdf::Sculpt::default();
        support.add(
            crate::sdf::Shape::Ellipsoid(Vec3::ZERO, Vec3::new(0.18, 0.3, 0.12)),
            0.0,
        );
        for tilt in [0.0, 0.03] {
            let profile = RadialBandProfile {
                heights: Vec2::new(-0.02, 0.02),
                support_to_output_y: 0.0,
                tilt,
                clearance: 0.004,
                thickness: 0.003,
                rows: 4,
                sides: 48,
            };
            let mut old = Builder::default();
            legacy_waist_emit(&profile, &mut old, &support, LEATHER);
            let mut new = Builder::default();
            profile.emit(&mut new, &support, LEATHER);
            assert_eq!(
                old.surface_triangles().collect::<Vec<_>>(),
                new.surface_triangles().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn radial_band_budget_clamps_degenerate_counts_to_topological_minimum() {
        let mut support = crate::sdf::Sculpt::default();
        support.add(
            crate::sdf::Shape::Ellipsoid(Vec3::ZERO, Vec3::splat(0.2)),
            0.0,
        );
        let profile = RadialBandProfile {
            heights: Vec2::new(-0.02, 0.02),
            support_to_output_y: 0.0,
            tilt: 0.0,
            clearance: 0.004,
            thickness: 0.003,
            rows: 0,
            sides: 0,
        };
        let mut mesh = Builder::default();
        profile.emit(&mut mesh, &support, LEATHER);
        assert_eq!(mesh.surface_triangles().count(), 24);
        assert!(mesh.surface_triangles().flatten().all(|p| p.is_finite()));
        let mut edges = std::collections::BTreeMap::new();
        for tri in mesh.surface_triangles() {
            for i in 0..3 {
                let mut edge = [
                    tri[i].to_array().map(f32::to_bits),
                    tri[(i + 1) % 3].to_array().map(f32::to_bits),
                ];
                edge.sort();
                *edges.entry(edge).or_insert(0usize) += 1;
            }
        }
        assert!(edges.values().all(|count| *count == 2));
        // This minimum is topologically closed, not a support-clearance promise.
    }

    #[test]
    fn support_band_frame_preserves_translation_thickness_and_closure() {
        let mut support = crate::sdf::Sculpt::default();
        support.add(
            crate::sdf::Shape::Ellipsoid(Vec3::ZERO, Vec3::new(0.18, 0.3, 0.12)),
            0.0,
        );
        let profile = RadialBandProfile {
            heights: Vec2::new(-0.02, 0.02),
            support_to_output_y: 0.0,
            tilt: 0.0,
            clearance: 0.004,
            thickness: 0.003,
            rows: 4,
            sides: 48,
        };
        let shifted = RadialBandProfile {
            heights: profile.heights + Vec2::splat(0.7),
            support_to_output_y: 0.7,
            ..profile
        };
        for i in 0..48 {
            let angle = i as f32 * TAU / 48.0;
            for outer in [false, true] {
                let a = profile.point(&support, 0.0, angle, outer);
                let b = shifted.point(&support, 0.7, angle, outer);
                assert!((a + Vec3::Y * 0.7 - b).length() < 1e-6);
                assert!((support.distance(a) - if outer { 0.007 } else { 0.004 }).abs() < 0.00001);
            }
        }
        let mut mesh = Builder::default();
        profile.emit(&mut mesh, &support, LEATHER);
        let mut edges = std::collections::BTreeMap::new();
        for tri in mesh.surface_triangles() {
            for i in 0..3 {
                let mut edge = [
                    tri[i].to_array().map(f32::to_bits),
                    tri[(i + 1) % 3].to_array().map(f32::to_bits),
                ];
                edge.sort();
                *edges.entry(edge).or_insert(0usize) += 1;
            }
            for height in [profile.heights.x, profile.heights.y] {
                if tri.iter().all(|p| p.y == height) {
                    let normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]);
                    assert!(
                        normal.y
                            * if height == profile.heights.x {
                                -1.0
                            } else {
                                1.0
                            }
                            > 0.0,
                        "caps face outward"
                    );
                }
            }
        }
        assert!(!edges.is_empty());
        assert!(
            edges.values().all(|n| *n == 2),
            "angular seam, caps and shells close exactly"
        );
    }
}

#[cfg(test)]
mod rounded_band_tests {
    use super::*;
    use std::collections::BTreeMap;

    fn profile() -> RoundedBandProfile {
        RoundedBandProfile {
            height: 0.17,
            half_height: 0.011,
            clearance: 0.003,
            radial_half: 0.004,
            sides: 24,
            sections: 8,
        }
    }

    #[test]
    fn band_window_preserves_ray_hits_and_discards_irrelevant_triangles() {
        for transformed in [false, true] {
            let mut p = Parts::default();
            if transformed {
                p.set_transform(Affine3A::from_rotation_translation(
                    Quat::from_rotation_z(0.37),
                    Vec3::new(0.3, -0.2, 0.4),
                ));
            }
            for row in 0..40 {
                let y = -0.4 + row as f32 * 0.02;
                p.matte.prism(
                    Vec3::Y * y,
                    Vec3::Y * (y + 0.02),
                    0.065,
                    0.066,
                    24,
                    Vec3::ONE,
                    0.0,
                );
            }
            p.metal.cuboid(
                Vec3::new(0.06, 0.17, 0.0),
                Vec3::new(0.012, 0.014, 0.025),
                Quat::IDENTITY,
                GILT,
            );
            let layers = [&p.matte, &p.metal];
            let selected = band_support_window(&layers, 0.159, 0.181);
            assert!(
                selected.surface_triangles().count() * 5
                    < layers
                        .iter()
                        .map(|l| l.surface_triangles().count())
                        .sum::<usize>()
            );
            for row in 0..=8 {
                let y = 0.159 + 0.022 * row as f32 / 8.0;
                let axial = band_support_window(&[&selected], y, y);
                assert!(axial.surface_triangles().count() < selected.surface_triangles().count());
                for k in 0..96 {
                    let a = k as f32 * TAU / 96.0;
                    let out = Vec3::new(a.sin(), 0.0, a.cos());
                    let full = outermost_layer_radius(&layers, Vec3::Y * y, out)
                        .expect("complete support");
                    let window = outermost_layer_radius(&[&selected], Vec3::Y * y, out)
                        .expect("window support");
                    let narrow =
                        outermost_layer_radius(&[&axial], Vec3::Y * y, out).expect("axial support");
                    assert!((narrow - window).abs() < 1e-5, "axial support agrees");
                    assert!(
                        (full - window).abs() < 1e-5,
                        "same support radius: {full}/{window}"
                    );
                }
            }
            let empty = band_support_window(&layers, 1.0, 1.1);
            assert_eq!(empty.surface_triangles().count(), 0);
        }
    }

    #[test]
    fn axial_window_keeps_crossing_triangles_and_missing_support() {
        let mut layer = Builder::default();
        // All vertices lie outside the queried slice, but its face crosses it.
        layer.tri(
            [
                Vec3::new(-0.1, -0.1, 0.06),
                Vec3::new(0.1, -0.1, 0.06),
                Vec3::new(0.0, 0.1, 0.06),
            ],
            Vec3::ONE,
        );
        for y in [-0.100001, -0.04, 0.0, 0.09, 0.100001, 0.2] {
            let axial = band_support_window(&[&layer], y, y);
            for angle in [0.0_f32, 0.4, PI] {
                let out = Vec3::new(angle.sin(), 0.0, angle.cos());
                let full = outermost_layer_radius(&[&layer], Vec3::Y * y, out);
                let narrow = outermost_layer_radius(&[&axial], Vec3::Y * y, out);
                assert_eq!(full, narrow, "presence and radius at {y}/{angle}");
            }
        }
        let parts = Parts {
            matte: layer,
            ..Parts::default()
        };
        assert!(profile().resolve_layers(&parts, GILT, None).is_none());
        // Angular support is complete at the center, but not at every axial row.
        let mut short = Parts::default();
        short.matte.prism(
            Vec3::Y * 0.165,
            Vec3::Y * 0.175,
            0.06,
            0.06,
            24,
            Vec3::ONE,
            0.0,
        );
        assert!(profile().resolve_layers(&short, GILT, None).is_none());
    }

    #[test]
    fn rounded_band_is_closed_finite_and_outward_with_explicit_dimensions() {
        let p = profile();
        let ring = p
            .resolve(
                |y, a, gap| Some(Vec3::new(a.sin() * (0.06 + gap), y, a.cos() * (0.06 + gap))),
                GILT,
            )
            .unwrap();
        let triangles: Vec<_> = ring.surface_triangles().collect();
        assert_eq!(triangles.len(), 384);
        let key = |v: Vec3| {
            [
                (v.x * 1e7).round() as i64,
                (v.y * 1e7).round() as i64,
                (v.z * 1e7).round() as i64,
            ]
        };
        let mut edges = BTreeMap::new();
        let mut low = f32::INFINITY;
        let mut high = f32::NEG_INFINITY;
        for tri in triangles {
            let center = (tri[0] + tri[1] + tri[2]) / 3.0;
            let radial = Vec3::new(center.x, 0.0, center.z).normalize();
            let inside = radial * (0.06 + p.clearance + p.radial_half) + Vec3::Y * p.height;
            let normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]);
            assert!(normal.length() > 1e-8);
            assert!(normal.dot(center - inside) > 0.0, "outward winding {tri:?}");
            for v in tri {
                assert!(v.is_finite());
                low = low.min(v.y);
                high = high.max(v.y);
            }
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                let (a, b) = (key(tri[a]), key(tri[b]));
                let (edge, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                let count = edges.entry(edge).or_insert((0, 0));
                count.0 += 1;
                count.1 += sign;
            }
        }
        assert!((low - (p.height - p.half_height)).abs() < 1e-6);
        assert!((high - (p.height + p.half_height)).abs() < 1e-6);
        assert!(edges
            .values()
            .all(|&(count, direction)| count == 2 && direction == 0));
    }

    #[test]
    fn rounded_band_rejects_late_missing_support_and_invalid_points() {
        let p = profile();
        let shape =
            |y: f32, a: f32, gap: f32| Vec3::new(a.sin() * (0.06 + gap), y, a.cos() * (0.06 + gap));
        assert!(p
            .resolve(|y, a, gap| (a < 5.9).then(|| shape(y, a, gap)), GILT)
            .is_none());
        assert!(p
            .resolve(
                |y, a, gap| Some(if a > 5.9 {
                    Vec3::splat(f32::NAN)
                } else {
                    shape(y, a, gap)
                }),
                GILT
            )
            .is_none());
    }

    #[test]
    fn rounded_band_parameters_follow_support_and_budget() {
        for (sides, sections) in [(12, 4), (24, 8), (32, 8)] {
            let p = RoundedBandProfile {
                sides,
                sections,
                ..profile()
            };
            let center = Vec3::new(0.017, 0.0, -0.012);
            let ring = p
                .resolve(
                    |y, a, gap| {
                        Some(center + Vec3::new(a.sin() * (0.08 + gap), y, a.cos() * (0.05 + gap)))
                    },
                    GILT,
                )
                .unwrap();
            assert_eq!(ring.surface_triangles().count(), sides * sections * 2);
            let front = ring
                .outer_surface(center + Vec3::Y * p.height, Vec3::Z)
                .unwrap();
            assert!((front.z - (center.z + 0.05 + p.clearance + 2.0 * p.radial_half)).abs() < 1e-6);
        }
    }
}

/// Construction-time shell over a complete local-Y limb support. The profile
/// owns style span, thickness and facet budget; the caller owns anatomy/layers.
/// Samples cover neighboring axial and angular half-cells, with polygon
/// inradius correction. This bounds the ordinary smooth limb footprint, not
/// arbitrary disconnected or sub-resolution obstacles.
pub(super) struct LimbShellProfile {
    pub heights: Vec2,
    pub clearance: f32,
    pub thickness: f32,
    pub rows: usize,
    pub sides: usize,
}

impl LimbShellProfile {
    pub fn resolve(
        &self,
        support: &dyn Fn(f32, f32) -> Option<f32>,
        color: Vec3,
        trim: Vec3,
        hollow: bool,
    ) -> Option<Builder> {
        if self.rows < 1
            || self.sides < 3
            || !self.heights.is_finite()
            || self.heights.x == self.heights.y
            || !self.clearance.is_finite()
            || self.clearance < 0.0
            || !self.thickness.is_finite()
            || self.thickness <= 0.0
        {
            return None;
        }
        let rows = self.rows;
        let sides = self.sides;
        let mut rings = Vec::with_capacity(rows + 1);
        for row in 0..=rows {
            let t = row as f32 / rows as f32;
            let height = self.heights.x + (self.heights.y - self.heights.x) * t;
            let mut ring = Vec::with_capacity(sides);
            for col in 0..sides {
                let angle = col as f32 * TAU / sides as f32;
                let mut radius = 0.0_f32;
                for axial in -2..=2 {
                    let sample_t =
                        ((row as f32 + axial as f32 * 0.25) / rows as f32).clamp(0.0, 1.0);
                    let y = self.heights.x + (self.heights.y - self.heights.x) * sample_t;
                    for angular in -1..=1 {
                        let sample_angle = angle + angular as f32 * PI / sides as f32;
                        let r = support(y, sample_angle)?;
                        if !r.is_finite() || r <= 0.0 || r > 0.3 {
                            return None;
                        }
                        radius = radius.max(r);
                    }
                }
                let out = Vec3::new(angle.sin(), 0.0, angle.cos());
                let inner = radius / (PI / sides as f32).cos() + self.clearance;
                ring.push([
                    Vec3::Y * height + out * inner,
                    Vec3::Y * height + out * (inner + self.thickness),
                ]);
            }
            rings.push(ring);
        }
        let mut shell = Builder::default();
        let mut face = |points: [Vec3; 4], facing: Vec3, tint| {
            let inside = points.iter().copied().sum::<Vec3>() * 0.25 - facing * 0.01;
            shell.tri_out([points[0], points[1], points[2]], inside, tint);
            shell.tri_out([points[0], points[2], points[3]], inside, tint);
        };
        for col in 0..sides {
            let next = (col + 1) % sides;
            let angle = (col as f32 + 0.5) * TAU / sides as f32;
            let out = Vec3::new(angle.sin(), 0.0, angle.cos());
            for row in 0..rows {
                let tint = if row + 1 == rows { trim } else { color };
                for (layer, facing) in [(0, -out), (1, out)] {
                    if !hollow && layer == 0 {
                        continue;
                    }
                    face(
                        [
                            rings[row][col][layer],
                            rings[row][next][layer],
                            rings[row + 1][next][layer],
                            rings[row + 1][col][layer],
                        ],
                        facing,
                        tint,
                    );
                }
            }
            if hollow {
                let along = Vec3::Y * (self.heights.y - self.heights.x).signum();
                for (row, facing, tint) in [(0, -along, color), (rows, along, trim)] {
                    face(
                        [
                            rings[row][col][0],
                            rings[row][next][0],
                            rings[row][next][1],
                            rings[row][col][1],
                        ],
                        facing,
                        tint,
                    );
                }
            }
        }
        if !hollow {
            let along = Vec3::Y * (self.heights.y - self.heights.x).signum();
            for (row, facing, tint) in [(0, -along, color), (rows, along, trim)] {
                let center = Vec3::Y * rings[row][0][1].y;
                for col in 0..sides {
                    shell.tri_out(
                        [center, rings[row][col][1], rings[row][(col + 1) % sides][1]],
                        center - facing * 0.01,
                        tint,
                    );
                }
            }
        }
        Some(shell)
    }
}

/// A finite leather attachment spanning a bounded angular patch of smooth
/// radial support. Both faces and every exposed edge belong to one closed mesh.
/// The recipe owns its station span, clearance, thickness and tessellation;
/// the caller resolves support in the attachment frame. Half-cell probes reject
/// sampled holes, not arbitrary disconnected or sub-resolution obstacles.
#[derive(Clone, Copy)]
pub(super) struct RadialAttachmentProfile {
    pub heights: Vec2,
    pub angles: Vec2,
    pub clearance: f32,
    pub thickness: f32,
    pub rows: usize,
    pub columns: usize,
}

impl RadialAttachmentProfile {
    pub fn resolve(
        &self,
        support: &dyn Fn(f32, f32, f32) -> Option<f32>,
        color: Vec3,
    ) -> Option<Builder> {
        if self.rows == 0
            || self.columns == 0
            || !self.heights.is_finite()
            || self.heights.x == self.heights.y
            || !self.angles.is_finite()
            || self.angles.x == self.angles.y
            || (self.angles.y - self.angles.x).abs() >= PI
            || !self.clearance.is_finite()
            || self.clearance < 0.0
            || !self.thickness.is_finite()
            || self.thickness <= 0.0
        {
            return None;
        }
        let rows = self.rows;
        let columns = self.columns;
        let height = |t: f32| self.heights.x + (self.heights.y - self.heights.x) * t;
        let angle = |t: f32| self.angles.x + (self.angles.y - self.angles.x) * t;
        let mut grid = Vec::with_capacity(rows + 1);
        // Resolve the entire patch before emitting, including half-cell support
        // witnesses. A missing late row or narrow sampled gap cannot be bridged.
        for row in 0..=rows * 2 {
            let y = height(row as f32 / (rows * 2) as f32);
            let mut points = Vec::with_capacity(columns + 1);
            for col in 0..=columns * 2 {
                let a = angle(col as f32 / (columns * 2) as f32);
                let radius = support(y, a, self.clearance)?;
                let outer = radius + self.thickness;
                if !radius.is_finite() || radius <= 0.0 || !outer.is_finite() || outer <= radius {
                    return None;
                }
                if row % 2 == 0 && col % 2 == 0 {
                    let out = Vec3::new(a.sin(), 0.0, a.cos());
                    points.push([Vec3::Y * y + out * radius, Vec3::Y * y + out * outer]);
                }
            }
            if row % 2 == 0 {
                grid.push(points);
            }
        }
        let mut patch = Builder::default();
        let mut face = |q: [Vec3; 4], facing: Vec3| {
            let inside = q.iter().copied().sum::<Vec3>() * 0.25 - facing * 0.01;
            patch.quad_out(q, inside, color);
        };
        for row in 0..rows {
            for col in 0..columns {
                let a = angle((col as f32 + 0.5) / columns as f32);
                let out = Vec3::new(a.sin(), 0.0, a.cos());
                for (layer, facing) in [(0, -out), (1, out)] {
                    face(
                        [
                            grid[row][col][layer],
                            grid[row][col + 1][layer],
                            grid[row + 1][col + 1][layer],
                            grid[row + 1][col][layer],
                        ],
                        facing,
                    );
                }
            }
            for (col, sign) in [(0, -1.0), (columns, 1.0)] {
                let a = angle(col as f32 / columns as f32);
                let tangent = Vec3::new(a.cos(), 0.0, -a.sin())
                    * (self.angles.y - self.angles.x).signum()
                    * sign;
                face(
                    [
                        grid[row][col][0],
                        grid[row + 1][col][0],
                        grid[row + 1][col][1],
                        grid[row][col][1],
                    ],
                    tangent,
                );
            }
        }
        for (row, sign) in [(0, -1.0), (rows, 1.0)] {
            let along = Vec3::Y * (self.heights.y - self.heights.x).signum() * sign;
            for col in 0..columns {
                face(
                    [
                        grid[row][col][0],
                        grid[row][col + 1][0],
                        grid[row][col + 1][1],
                        grid[row][col][1],
                    ],
                    along,
                );
            }
        }
        Some(patch)
    }
}

#[cfg(test)]
mod layer_query_tests {
    use super::*;

    #[test]
    fn selected_layer_query_preserves_frame_misses_and_outermost_hit() {
        for scale in [0.7, 1.0, 1.4] {
            for origin in [Vec3::ZERO, Vec3::new(0.2, -0.3, 0.1)] {
                let mut inner = Builder::default();
                let mut outer = Builder::default();
                let mut excluded = Builder::default();
                inner.blob(
                    origin,
                    Vec3::new(0.02, 0.06, 0.03) * scale,
                    2,
                    0.0,
                    Vec3::ONE,
                );
                outer.blob(
                    origin,
                    Vec3::new(0.035, 0.08, 0.045) * scale,
                    2,
                    0.0,
                    Vec3::ONE,
                );
                excluded.blob(origin, Vec3::splat(0.2) * scale, 2, 0.0, Vec3::ONE);
                let before: Vec<_> = outer.surface_triangles().collect();
                for out in [Vec3::X, Vec3::NEG_Z, Vec3::new(1.0, 0.0, 1.0).normalize()] {
                    let expected = outer
                        .outer_surface(origin, out)
                        .map(|hit| (hit - origin).dot(out));
                    assert!(expected.is_some());
                    assert_eq!(
                        outermost_layer_radius(&[&inner, &outer], origin, out),
                        expected
                    );
                    assert_eq!(
                        outermost_layer_radius(&[&outer, &inner], origin, out),
                        expected
                    );
                    assert!(
                        outermost_layer_radius(&[&excluded], origin, out).unwrap()
                            > expected.unwrap()
                    );
                    assert_eq!(
                        outermost_layer_radius(&[&inner, &outer], origin + Vec3::Y, out),
                        None
                    );
                    assert_eq!(outermost_layer_radius(&[], origin, out), None);
                }
                assert_eq!(before, outer.surface_triangles().collect::<Vec<_>>());
            }
        }
    }
}
