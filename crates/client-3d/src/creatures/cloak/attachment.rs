//! Bounded radial seating of the emitted first band before its attachment is
//! frozen. This is a head-only prepass, not the subsequent all-support solve.
use super::*;

const ATTACHMENT_VERTICES: usize = 2 * (COLUMNS + 1);

#[derive(Default)]
struct Work {
    #[cfg(test)]
    rounds: usize,
    #[cfg(test)]
    queries: usize,
}

/// All positions are in torso space. Only the first two rings may move, on
/// their original horizontal radial axes. Limits are absolute radii for those
/// 98 vertices, or a full GRID inventory whose later entries are unused.
/// The input is never mutated, including when no bounded solution exists.
pub(super) fn fit_head_attachment(
    reference: &[Vec3],
    head: &LayerSurface,
    radial_limits: &[f32],
) -> Result<Vec<Vec3>, RadialFitFailure> {
    fit(reference, head, radial_limits, &mut Work::default())
}

fn failure(vertex: usize, reason: &'static str, detail: [f32; 3]) -> RadialFitFailure {
    RadialFitFailure {
        row: vertex / (COLUMNS + 1),
        column: vertex % (COLUMNS + 1),
        reason,
        detail,
    }
}

fn domain(
    vertices: [Vec3; 3],
    endpoints: [Vec3; 3],
    rays: [Vec3; 3],
    horizontal: Vec3,
) -> Vec<(Vec3, f32)> {
    let tangent = Vec3::new(horizontal.z, 0.0, -horizontal.x);
    let (lo, hi) = rays
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), ray| {
            let angle = ray.dot(tangent).atan2(ray.dot(horizontal));
            (lo.min(angle), hi.max(angle))
        });
    let left = horizontal * lo.cos() + tangent * lo.sin();
    let right = horizontal * hi.cos() + tangent * hi.sin();
    let (low, high, radius) = swept_face_bounds(vertices, endpoints, horizontal);
    let prism = [
        (Vec3::Y, low - THICKNESS),
        (Vec3::NEG_Y, -high - THICKNESS),
        (Vec3::new(left.z, 0.0, -left.x), -THICKNESS),
        (Vec3::new(-right.z, 0.0, right.x), -THICKNESS),
        (-horizontal, -radius - THICKNESS),
    ];
    // The shared helper intersects the exact six-endpoint hull and prism,
    // with every unit half-space expanded by LAYER_GAP + THICKNESS (32 mm).
    arm_contact_planes(vertices, endpoints, &prism)
}

fn fit(
    reference: &[Vec3],
    head: &LayerSurface,
    radial_limits: &[f32],
    work: &mut Work,
) -> Result<Vec<Vec3>, RadialFitFailure> {
    #[cfg(not(test))]
    let _ = work;
    if reference.len() != GRID
        || !matches!(radial_limits.len(), ATTACHMENT_VERTICES | GRID)
        || reference.iter().any(|point| !point.is_finite())
    {
        return Err(failure(
            0,
            "invalid head attachment grid or limit inventory",
            [reference.len() as f32, radial_limits.len() as f32, 0.0],
        ));
    }
    let mut axes = [Vec3::ZERO; ATTACHMENT_VERTICES];
    let mut minimum = [0.0f32; ATTACHMENT_VERTICES];
    for i in 0..ATTACHMENT_VERTICES {
        let radial = reference[i].with_y(0.0);
        let radius = radial.length();
        if radius <= 1e-6 || !radial_limits[i].is_finite() || radius > radial_limits[i] {
            return Err(failure(
                i,
                "head attachment reference exceeds a finite radial limit",
                [radius, radial_limits[i], 0.0],
            ));
        }
        axes[i] = radial / radius;
        // A clear vertex never follows an unconditional far ray. Only proven
        // embedded points need an exit before finite face domains are formed.
        if !head.containing_components(reference[i]).is_empty() {
            let outside = head.outside_point(reference[i], axes[i]).ok_or_else(|| {
                failure(
                    i,
                    "head attachment volume exit is ambiguous",
                    reference[i].to_array(),
                )
            })?;
            minimum[i] = (outside - reference[i]).dot(axes[i]).max(0.0);
            let radius = outside.with_y(0.0).length();
            if !outside.is_finite() || radius > radial_limits[i] {
                return Err(failure(
                    i,
                    "head attachment volume exit exceeds radial limit",
                    [radius, radial_limits[i], minimum[i]],
                ));
            }
        }
    }
    let mut budgets = minimum;
    for _round in 0..12 {
        #[cfg(test)]
        {
            work.rounds += 1;
        }
        let mut travels = minimum;
        // Every face is queried again after ANY budget growth. Shared maximum
        // shifts remain positive on each selected plane's three radial axes.
        for column in 0..COLUMNS {
            let [a, b, c, d] = [
                index(0, column),
                index(0, column + 1),
                index(1, column),
                index(1, column + 1),
            ];
            for face in [[a, b, c], [b, d, c]] {
                let vertices = face.map(|i| reference[i]);
                let rays = face.map(|i| axes[i]);
                let horizontal = rays.iter().copied().sum::<Vec3>().normalize_or_zero();
                if rays.iter().any(|ray| horizontal.dot(*ray) <= 0.05) {
                    return Err(failure(
                        column,
                        "head attachment has nonconvex angular footprint",
                        horizontal.to_array(),
                    ));
                }
                let endpoints = face.map(|i| reference[i] + axes[i] * budgets[i]);
                let planes = domain(vertices, endpoints, rays, horizontal);
                let cross = (vertices[1] - vertices[0]).cross(vertices[2] - vertices[0]);
                let authored = if cross.dot(horizontal).abs() > 1e-6 {
                    cross.y / cross.dot(horizontal)
                } else {
                    0.0
                };
                let normals = face_normals(horizontal, authored);
                let admissible = normals.map(|normal| {
                    normal.is_finite() && rays.iter().all(|ray| normal.dot(*ray) > 1e-6)
                });
                let mut supports = [f32::NEG_INFINITY; FACE_NORMALS];
                let mut found = false;
                #[cfg(test)]
                {
                    work.queries += 1;
                }
                for &triangle in head.clipped_candidates(&planes) {
                    if let Some((polygon, count)) = clipped_polygon(triangle, &planes) {
                        found = true;
                        for ((normal, support), allowed) in
                            normals.iter().zip(&mut supports).zip(admissible)
                        {
                            if allowed {
                                for point in &polygon[..count] {
                                    *support = support.max(normal.dot(*point));
                                }
                            }
                        }
                    }
                }
                if !found {
                    continue;
                }
                let mut best: Option<([f32; 3], f32, Vec3, f32)> = None;
                for ((normal, support), allowed) in
                    normals.into_iter().zip(supports).zip(admissible)
                {
                    if !allowed {
                        continue;
                    }
                    let plane = support + LAYER_GAP + THICKNESS;
                    let shifts = face.map(|i| {
                        ((plane - normal.dot(reference[i])).max(0.0) / normal.dot(axes[i]))
                            .max(minimum[i])
                    });
                    // Reject this candidate, never truncate a collision solve
                    // to the anatomical limit or allow aggregation to hide it.
                    if face.iter().zip(shifts).any(|(&i, shift)| {
                        !shift.is_finite()
                            || (reference[i] + axes[i] * shift).with_y(0.0).length()
                                > radial_limits[i]
                    }) {
                        continue;
                    }
                    let cost = shifts.iter().map(|shift| shift * shift).sum::<f32>();
                    if best.is_none_or(|(_, old, _, _)| cost < old) {
                        best = Some((shifts, cost, normal, plane));
                    }
                }
                let Some((shifts, _, normal, plane)) = best else {
                    return Err(failure(
                        column,
                        "no bounded radial head attachment plane",
                        face.map(|i| radial_limits[i]),
                    ));
                };
                #[cfg(not(test))]
                let _ = (normal, plane);
                #[cfg(test)]
                if column == 0 && std::env::var_os("TOR_CLOAK_SUPPORT_AUDIT").is_some() {
                    eprintln!(
                        "head attachment plane: {}",
                        serde_json::json!({
                            "context":audit_context(),"face":face,"iteration":_round,
                            "normal":normal.to_array(),"plane":plane,
                            "reference":vertices.map(|p|p.to_array()),
                            "budget_endpoints":endpoints.map(|p|p.to_array()),
                            "candidate":std::array::from_fn::<_,3,_>(|j| (reference[face[j]]+axes[face[j]]*shifts[j]).to_array()),
                            "face_shifts":shifts,
                        })
                    );
                }
                for (i, shift) in face.into_iter().zip(shifts) {
                    travels[i] = travels[i].max(shift);
                }
            }
        }
        for i in 0..ATTACHMENT_VERTICES {
            let proposed = reference[i] + axes[i] * travels[i];
            if !head.containing_components(proposed).is_empty() {
                let outside = head.outside_point(proposed, axes[i]).ok_or_else(|| {
                    failure(
                        i,
                        "proposed head attachment exit is ambiguous",
                        proposed.to_array(),
                    )
                })?;
                travels[i] = travels[i].max((outside - reference[i]).dot(axes[i]));
            }
            let proposed = reference[i] + axes[i] * travels[i];
            let radius = proposed.with_y(0.0).length();
            if !proposed.is_finite() || radius > radial_limits[i] {
                return Err(failure(
                    i,
                    "aggregated head attachment exceeds radial limit",
                    [radius, radial_limits[i], travels[i]],
                ));
            }
            if !head.containing_components(proposed).is_empty() {
                return Err(failure(
                    i,
                    "head attachment remains inside a closed component",
                    proposed.to_array(),
                ));
            }
        }
        if travels
            .iter()
            .zip(budgets)
            .all(|(&travel, budget)| travel <= budget)
        {
            let mut result = reference.to_vec();
            for i in 0..ATTACHMENT_VERTICES {
                result[i] = reference[i] + axes[i] * travels[i];
            }
            return Ok(result);
        }
        for (budget, travel) in budgets.iter_mut().zip(travels) {
            *budget = budget.max(travel);
        }
    }
    Err(failure(
        0,
        "head attachment swept domains did not converge",
        [budgets.into_iter().fold(0.0, f32::max), 12.0, 0.0],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(radius: f32) -> Vec<Vec3> {
        (0..GRID)
            .map(|i| {
                let row = i / (COLUMNS + 1);
                let angle = TAU * (i % (COLUMNS + 1) % COLUMNS) as f32 / COLUMNS as f32;
                let y = if row == 0 {
                    0.59
                } else {
                    0.61 - (row - 1) as f32 * 0.03
                };
                Vec3::new(angle.sin() * radius, y, angle.cos() * radius)
            })
            .collect()
    }

    fn sphere() -> LayerSurface {
        let mut triangles = Vec::new();
        for x in [-1.0, 1.0] {
            for y in [-1.0, 1.0] {
                for z in [-1.0, 1.0] {
                    let mut face = [Vec3::X * x, Vec3::Y * y, Vec3::Z * z];
                    if (face[1] - face[0]).cross(face[2] - face[0]).dot(face[0]) < 0.0 {
                        face.swap(1, 2);
                    }
                    triangles.push(face);
                }
            }
        }
        for _ in 0..3 {
            triangles = triangles
                .into_iter()
                .flat_map(|[a, b, c]| {
                    let [ab, bc, ca] = [
                        (a + b).normalize(),
                        (b + c).normalize(),
                        (c + a).normalize(),
                    ];
                    [[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]
                })
                .collect();
        }
        LayerSurface::from_triangles(
            triangles
                .into_iter()
                .map(|face| face.map(|p| p * 0.16 + Vec3::Y * 0.6))
                .collect(),
        )
    }

    fn sheet(z: f32) -> [Vec3; 3] {
        [
            Vec3::new(-0.12, 0.45, z),
            Vec3::new(0.12, 0.45, z),
            Vec3::new(0.0, 0.85, z),
        ]
    }

    // Independent plane/edge distance enumeration, without support domains,
    // solver candidates, production closest-point code, or BVH rejection.
    fn mesh_distance(point: Vec3, triangles: &[[Vec3; 3]]) -> f32 {
        let mut nearest = f32::INFINITY;
        for &[a, b, c] in triangles {
            let normal = (b - a).cross(c - a).normalize();
            let distance = normal.dot(point - a);
            let projected = point - normal * distance;
            if [(a, b), (b, c), (c, a)]
                .iter()
                .all(|&(a, b)| (b - a).cross(projected - a).dot(normal) >= 0.0)
            {
                nearest = nearest.min(distance.abs());
            }
            for (a, b) in [(a, b), (b, c), (c, a)] {
                let edge = b - a;
                let t = ((point - a).dot(edge) / edge.length_squared()).clamp(0.0, 1.0);
                nearest = nearest.min(point.distance(a + edge * t));
            }
        }
        nearest
    }

    #[test]
    fn closed_head_first_band_has_independent_sampled_clearance() {
        let reference = grid(0.12);
        let original = reference.clone();
        let head = sphere();
        assert!(head.obstacle_groups().iter().all(Option::is_some));
        assert!(!head.containing_components(reference[0]).is_empty());
        let fitted = fit_head_attachment(&reference, &head, &[0.24; ATTACHMENT_VERTICES]).unwrap();
        assert_eq!(reference, original);
        assert_eq!(
            &fitted[ATTACHMENT_VERTICES..],
            &reference[ATTACHMENT_VERTICES..]
        );
        for i in 0..ATTACHMENT_VERTICES {
            assert_eq!(fitted[i].y, reference[i].y);
            assert!(fitted[i].with_y(0.0).length() <= 0.24);
            assert!(
                reference[i]
                    .with_y(0.0)
                    .cross(fitted[i].with_y(0.0))
                    .length()
                    < 1e-7
            );
        }
        for column in 0..COLUMNS {
            let [a, b, c, d] = [
                index(0, column),
                index(0, column + 1),
                index(1, column),
                index(1, column + 1),
            ];
            for face in [[a, b, c], [b, d, c]] {
                for u in 0..=6 {
                    for v in 0..=6 - u {
                        let weights = [u as f32 / 6.0, v as f32 / 6.0, (6 - u - v) as f32 / 6.0];
                        let point = face
                            .into_iter()
                            .zip(weights)
                            .map(|(i, weight)| fitted[i] * weight)
                            .sum::<Vec3>();
                        assert!(
                            point.distance(Vec3::Y * 0.6) > 0.16,
                            "sample is inside analytic head sphere"
                        );
                        assert!(
                            mesh_distance(point, &head.triangles) >= LAYER_GAP + THICKNESS - 2e-5
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn bounded_attachment_failure_preserves_input() {
        let reference = grid(0.2);
        let original = reference.clone();
        let head = LayerSurface::from_triangles(vec![sheet(0.21)]);
        assert!(fit_head_attachment(&reference, &head, &[0.32; ATTACHMENT_VERTICES]).is_ok());
        let mut limits = [0.32; ATTACHMENT_VERTICES];
        limits[0] = 0.205;
        assert!(fit_head_attachment(&reference, &head, &limits).is_err());
        assert_eq!(reference, original);
        let embedded = grid(0.12);
        assert!(fit_head_attachment(&embedded, &sphere(), &[0.15; ATTACHMENT_VERTICES]).is_err());
        assert_eq!(embedded, grid(0.12));
        assert!(fit_head_attachment(&reference[..GRID - 1], &head, &limits).is_err());
        assert!(
            fit_head_attachment(&reference, &head, &limits[..ATTACHMENT_VERTICES - 1]).is_err()
        );
        assert!(fit_head_attachment(&reference, &head, &[0.32; GRID]).is_ok());
    }

    #[test]
    fn nearby_candidate_moves_attachment_but_distant_ray_obstacle_does_not() {
        let reference = grid(0.2);
        let near = LayerSurface::from_triangles(vec![sheet(0.21)]);
        let remote = LayerSurface::from_triangles(vec![sheet(2.0)]);
        let both = LayerSurface::from_triangles(vec![sheet(0.21), sheet(2.0)]);
        let limits = [0.32; ATTACHMENT_VERTICES];
        assert!(remote.ray(reference[0], Vec3::Z).is_some());
        assert_eq!(
            fit_head_attachment(&reference, &remote, &limits).unwrap(),
            reference
        );
        let fitted = fit_head_attachment(&reference, &near, &limits).unwrap();
        assert!(fitted[0].z >= 0.21 + LAYER_GAP + THICKNESS - 1e-6);
        assert_eq!(
            fit_head_attachment(&reference, &both, &limits).unwrap(),
            fitted
        );
    }

    #[test]
    fn budget_growth_requeries_every_face_and_discovers_new_head_surface() {
        let reference = grid(0.2);
        let first = LayerSurface::from_triangles(vec![sheet(0.21)]);
        let both = LayerSurface::from_triangles(vec![sheet(0.21), sheet(0.27)]);
        let limits = [0.34; ATTACHMENT_VERTICES];
        let first_only = fit_head_attachment(&reference, &first, &limits).unwrap();
        assert!(first_only[0].z < 0.27);
        assert!(0.27 - reference[0].z > LAYER_GAP + THICKNESS);
        let mut work = Work::default();
        let fitted = fit(&reference, &both, &limits, &mut work).unwrap();
        assert!(fitted[0].z >= 0.27 + LAYER_GAP + THICKNESS - 1e-6);
        assert!(
            work.rounds >= 3,
            "initial miss must be requeried after domain growth"
        );
        assert_eq!(work.queries, work.rounds * COLUMNS * 2);
        assert_eq!(
            &fitted[ATTACHMENT_VERTICES..],
            &reference[ATTACHMENT_VERTICES..]
        );
    }
}
