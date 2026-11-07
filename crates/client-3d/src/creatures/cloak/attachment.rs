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
    #[cfg(test)]
    fallbacks: usize,
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

type Candidate = ([f32; 3], f32, Vec3, f32);

fn candidate(
    normal: Vec3,
    support: f32,
    vertices: [Vec3; 3],
    axes: [Vec3; 3],
    minimum: [f32; 3],
    limits: [f32; 3],
) -> Option<Candidate> {
    if !normal.is_finite() || axes.iter().any(|axis| normal.dot(*axis) <= 1e-6) {
        return None;
    }
    let plane = support + LAYER_GAP + THICKNESS;
    let shifts = std::array::from_fn::<_, 3, _>(|i| {
        ((plane - normal.dot(vertices[i])).max(0.0) / normal.dot(axes[i])).max(minimum[i])
    });
    // Reject, never truncate, both the original family and geometry-derived
    // candidates using exactly the same absolute-radius contract.
    if shifts.iter().enumerate().any(|(i, &shift)| {
        !shift.is_finite() || (vertices[i] + axes[i] * shift).with_y(0.0).length() > limits[i]
    }) {
        return None;
    }
    let cost = shifts.iter().map(|shift| shift * shift).sum::<f32>();
    Some((shifts, cost, normal, plane))
}

fn closest_segment(point: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let edge = b - a;
    let length = edge.length_squared();
    if length <= 1e-20 {
        a
    } else {
        a + edge * ((point - a).dot(edge) / length).clamp(0.0, 1.0)
    }
}

// The same Voronoi-region construction as robe contact, with an edge fallback
// for collapsed clipped fans. No nearest point is inferred from a proxy head.
fn closest_triangle(point: Vec3, [a, b, c]: [Vec3; 3]) -> Vec3 {
    let ab = b - a;
    let ac = c - a;
    if ab.cross(ac).length_squared() <= 1e-18 {
        return [(a, b), (b, c), (c, a)]
            .map(|(a, b)| closest_segment(point, a, b))
            .into_iter()
            .min_by(|a, b| {
                a.distance_squared(point)
                    .total_cmp(&b.distance_squared(point))
            })
            .unwrap();
    }
    let ap = point - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = point - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = point - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 >= d3 && d5 >= d6 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denominator = 1.0 / (va + vb + vc);
    a + ab * (vb * denominator) + ac * (vc * denominator)
}

fn closest_edge_pair(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> (Vec3, Vec3) {
    // Double-precision parameters avoid subtracting nearly equal squared
    // lengths for shallow edge pairs. Returned witnesses remain render-space.
    let u = (b - a).as_dvec3();
    let v = (d - c).as_dvec3();
    let w = (a - c).as_dvec3();
    let aa = u.length_squared();
    let ee = v.length_squared();
    if aa <= 1e-20 {
        return (a, closest_segment(a, c, d));
    }
    if ee <= 1e-20 {
        return (closest_segment(c, a, b), c);
    }
    let cross = u.dot(v);
    let along_u = u.dot(w);
    let along_v = v.dot(w);
    let denominator = u.cross(v).length_squared();
    let mut s = if denominator > 0.0 {
        ((cross * along_v - along_u * ee) / denominator).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut t = (cross * s + along_v) / ee;
    if t < 0.0 {
        t = 0.0;
        s = (-along_u / aa).clamp(0.0, 1.0);
    } else if t > 1.0 {
        t = 1.0;
        s = ((cross - along_u) / aa).clamp(0.0, 1.0);
    }
    (
        (a.as_dvec3() + u * s).as_vec3(),
        (c.as_dvec3() + v * t).as_vec3(),
    )
}

fn edge_pierces_triangle(a: Vec3, b: Vec3, triangle: [Vec3; 3]) -> bool {
    let normal = (triangle[1] - triangle[0]).cross(triangle[2] - triangle[0]);
    let start = normal.dot(a - triangle[0]);
    let denominator = normal.dot(a - b);
    if denominator == 0.0 {
        return false;
    }
    let t = start / denominator;
    if !(0.0..=1.0).contains(&t) {
        return false;
    }
    let point = a.lerp(b, t);
    closest_triangle(point, triangle).distance_squared(point) <= 1e-14
}

fn closest_triangle_pair(caps: [Vec3; 3], support: [Vec3; 3]) -> Option<(Vec3, Vec3)> {
    let mut closest = (caps[0], support[0]);
    let mut distance = closest.0.distance_squared(closest.1);
    let mut consider = |pair: (Vec3, Vec3)| {
        let next = pair.0.distance_squared(pair.1);
        if next < distance {
            closest = pair;
            distance = next;
        }
    };
    for point in caps {
        consider((point, closest_triangle(point, support)));
    }
    for point in support {
        consider((closest_triangle(point, caps), point));
    }
    for i in 0..3 {
        let (a, b) = (caps[i], caps[(i + 1) % 3]);
        // Intersecting faces can have no coincident vertices or edge pairs:
        // an edge piercing the opposite interior is a separate zero case.
        if edge_pierces_triangle(a, b, support)
            || edge_pierces_triangle(support[i], support[(i + 1) % 3], caps)
        {
            return None;
        }
        for j in 0..3 {
            consider(closest_edge_pair(a, b, support[j], support[(j + 1) % 3]));
        }
    }
    (distance.is_finite() && distance > 1e-14).then_some(closest)
}

/// Propose one support-to-cap normal from the globally nearest triangle
/// features in this exact clipped domain. The caller must still project every
/// support source and enforce all movement-axis and travel-limit constraints.
pub(super) fn cap_surface_normal(
    surface: &LayerSurface,
    planes: &[(Vec3, f32)],
    caps: [Vec3; 3],
) -> Option<Vec3> {
    if caps.iter().any(|p| !p.is_finite())
        || (caps[1] - caps[0])
            .cross(caps[2] - caps[0])
            .length_squared()
            <= 1e-18
    {
        return None;
    }
    let mut closest = None;
    let mut distance = f32::INFINITY;
    for &triangle in surface.clipped_candidates(planes) {
        let Some((polygon, count)) = clipped_polygon(triangle, planes) else {
            continue;
        };
        if polygon[..count].iter().any(|p| !p.is_finite()) {
            return None;
        }
        for fan in 1..count.saturating_sub(1).max(2) {
            let support = [
                polygon[0],
                polygon[fan.min(count - 1)],
                polygon[(fan + 1).min(count - 1)],
            ];
            // Any touching/intersecting patch rules out a separating proposal,
            // including degenerate point/segment patches created by clipping.
            let pair = closest_triangle_pair(caps, support)?;
            let next = pair.0.distance_squared(pair.1);
            if next < distance {
                closest = Some(pair);
                distance = next;
            }
        }
    }
    closest.and_then(|(cap, support)| (cap - support).try_normalize())
}

/// Only a failed original family pays for three closest-point queries and one
/// complete support projection. Clip one triangle at a time; no mesh is copied.
fn cap_candidates(
    head: &LayerSurface,
    planes: &[(Vec3, f32)],
    caps: [Vec3; 3],
) -> [(Option<Vec3>, f32); 3] {
    let mut closest = [None; 3];
    let mut distances = [f32::INFINITY; 3];
    for &triangle in head.clipped_candidates(planes) {
        let Some((polygon, count)) = clipped_polygon(triangle, planes) else {
            continue;
        };
        // A clip may collapse to a point or segment. Retain those boundary
        // witnesses as degenerate triangles rather than dropping the support.
        for fan in 1..count.saturating_sub(1).max(2) {
            let triangle = [
                polygon[0],
                polygon[fan.min(count - 1)],
                polygon[(fan + 1).min(count - 1)],
            ];
            for i in 0..3 {
                let point = closest_triangle(caps[i], triangle);
                let distance = point.distance_squared(caps[i]);
                if distance < distances[i] {
                    distances[i] = distance;
                    closest[i] = Some(point);
                }
            }
        }
    }
    let normals = std::array::from_fn::<_, 3, _>(|i| {
        closest[i].and_then(|point| (caps[i] - point).try_normalize())
    });
    let mut supports = [f32::NEG_INFINITY; 3];
    for &triangle in head.clipped_candidates(planes) {
        if let Some((polygon, count)) = clipped_polygon(triangle, planes) {
            for (normal, support) in normals.iter().zip(&mut supports) {
                if let Some(normal) = normal {
                    for point in &polygon[..count] {
                        *support = support.max(normal.dot(*point));
                    }
                }
            }
        }
    }
    std::array::from_fn(|i| (normals[i], supports[i]))
}

fn fit(
    reference: &[Vec3],
    head: &LayerSurface,
    radial_limits: &[f32],
    work: &mut Work,
) -> Result<Vec<Vec3>, RadialFitFailure> {
    #[cfg(not(test))]
    let _ = work;
    #[cfg(test)]
    let audit_enabled = std::env::var_os("TOR_CLOAK_SUPPORT_AUDIT").is_some();
    #[cfg(test)]
    let audit_all = audit_enabled && std::env::var_os("TOR_CLOAK_ATTACHMENT_ALL").is_some();
    #[cfg(test)]
    if audit_all {
        eprintln!(
            "head attachment begin: {}",
            serde_json::json!({"context":audit_context()})
        );
    }
    #[cfg(test)]
    let audit_column = std::env::var("TOR_CLOAK_ATTACHMENT_COLUMN")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|&column| column < COLUMNS)
        .unwrap_or(0);
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
                let minima = face.map(|i| minimum[i]);
                let limits = face.map(|i| radial_limits[i]);
                let mut best: Option<Candidate> = None;
                for ((normal, support), allowed) in
                    normals.into_iter().zip(supports).zip(admissible)
                {
                    if !allowed {
                        continue;
                    }
                    if let Some(proposed) =
                        candidate(normal, support, vertices, rays, minima, limits)
                    {
                        if best.is_none_or(|(_, old, _, _)| proposed.1 < old) {
                            best = Some(proposed);
                        }
                    }
                }
                #[cfg(test)]
                let needed_geometry = best.is_none();
                let geometry_candidates = if best.is_none() {
                    #[cfg(test)]
                    {
                        work.fallbacks += 1;
                    }
                    let caps =
                        std::array::from_fn(|i| rays[i] * limits[i] + Vec3::Y * vertices[i].y);
                    let geometry_candidates = cap_candidates(head, &planes, caps);
                    for (normal, support) in geometry_candidates {
                        if let Some(proposed) = normal.and_then(|normal| {
                            candidate(normal, support, vertices, rays, minima, limits)
                        }) {
                            if best.is_none_or(|(_, old, _, _)| proposed.1 < old) {
                                best = Some(proposed);
                            }
                        }
                    }
                    geometry_candidates
                } else {
                    [(None, f32::NEG_INFINITY); 3]
                };
                #[cfg(not(test))]
                let _ = geometry_candidates;
                let Some((shifts, _, normal, plane)) = best else {
                    #[cfg(test)]
                    if audit_enabled {
                        let context = audit_context();
                        let limits = face.map(|i| radial_limits[i]);
                        let mut polygons = 0usize;
                        let mut triangles = 0usize;
                        for &triangle in head.clipped_candidates(&planes) {
                            triangles += 1;
                            if let Some((polygon, count)) = clipped_polygon(triangle, &planes) {
                                eprintln!(
                                    "head attachment audit polygon: {}",
                                    serde_json::json!({
                                        "context":context,"source":"head","face":face,
                                        "column":column,"iteration":_round,"polygon":polygons,
                                        "reference":vertices.map(|p|p.to_array()),
                                        "budget_endpoints":endpoints.map(|p|p.to_array()),
                                        "axes":rays.map(|p|p.to_array()),"radial_limits":limits,
                                        "points":polygon[..count].iter().map(|p|p.to_array()).collect::<Vec<_>>(),
                                    })
                                );
                                polygons += 1;
                            }
                        }
                        let mut candidates: Vec<_> = normals
                            .into_iter()
                            .zip(supports)
                            .zip(admissible)
                            .enumerate()
                            .map(|(candidate, ((normal, support), allowed))| {
                                let plane = support + LAYER_GAP + THICKNESS;
                                let shifts = allowed.then(|| {
                                    face.map(|i| {
                                        ((plane - normal.dot(reference[i])).max(0.0)
                                            / normal.dot(axes[i]))
                                        .max(minimum[i])
                                    })
                                });
                                let radii = shifts.map(|shifts| {
                                    std::array::from_fn::<_, 3, _>(|j| {
                                        (vertices[j] + rays[j] * shifts[j]).with_y(0.0).length()
                                    })
                                });
                                serde_json::json!({
                                    "candidate":candidate,"label":format!("normal {candidate}"),
                                    "normal":normal.to_array(),"admissible":allowed,
                                    "axis_projections":rays.map(|axis|normal.dot(axis)),
                                    "support":allowed.then_some(support),"plane":allowed.then_some(plane),
                                    "face_shifts":shifts,"candidate_radii":radii,
                                    "within_limits":shifts.zip(radii).is_some_and(|(shifts,radii)| {
                                        shifts.into_iter().zip(radii).zip(limits).all(|((shift,radius),limit)| {
                                            shift.is_finite() && radius <= limit
                                        })
                                    }),
                                })
                            })
                            .collect();
                        candidates.extend(geometry_candidates.into_iter().enumerate().map(|(cap, (normal, support))| {
                            let evaluated = normal.and_then(|normal| candidate(normal, support, vertices, rays, minima, limits));
                            serde_json::json!({
                                "candidate":FACE_NORMALS+cap,"label":format!("closest clipped surface to cap {cap}"),
                                "normal":normal.map(|n|n.to_array()),"support":normal.map(|_|support),
                                "axis_projections":normal.map(|n|rays.map(|axis|n.dot(axis))),
                                "within_limits":evaluated.is_some(),
                                "face_shifts":evaluated.map(|value|value.0),
                            })
                        }));
                        eprintln!(
                            "head attachment audit complete: {}",
                            serde_json::json!({
                                "context":context,"source":"head","status":"no bounded plane",
                                "face":face,"column":column,"iteration":_round,
                                "reference":vertices.map(|p|p.to_array()),
                                "budget_endpoints":endpoints.map(|p|p.to_array()),
                                "axes":rays.map(|p|p.to_array()),"radial_limits":limits,
                                "reference_radii":vertices.map(|p|p.with_y(0.0).length()),
                                "face_budgets":face.map(|i|budgets[i]),
                                "minimum_exits":face.map(|i|minimum[i]),
                                "clearance":LAYER_GAP+THICKNESS,
                                "domain_planes":planes.iter().map(|(normal,offset)| {
                                    serde_json::json!({"normal":normal.to_array(),"offset":offset})
                                }).collect::<Vec<_>>(),
                                "triangles":triangles,"polygons":polygons,"candidates":candidates,
                            })
                        );
                    }
                    return Err(failure(
                        column,
                        "no bounded radial head attachment plane",
                        face.map(|i| radial_limits[i]),
                    ));
                };
                #[cfg(not(test))]
                let _ = (normal, plane);
                #[cfg(test)]
                if audit_enabled && (audit_all || column == audit_column) {
                    eprintln!(
                        "head attachment plane: {}",
                        serde_json::json!({
                            "context":audit_context(),"face":face,"column":column,"iteration":_round,
                            "normal":normal.to_array(),"plane":plane,
                            "candidate_family":if needed_geometry {"head cap closest point"} else {"original"},
                            "reference":vertices.map(|p|p.to_array()),
                            "budget_endpoints":endpoints.map(|p|p.to_array()),
                            "axes":rays.map(|p|p.to_array()),
                            "radial_limits":face.map(|i|radial_limits[i]),
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
            #[cfg(test)]
            if audit_all {
                eprintln!(
                    "head attachment complete: {}",
                    serde_json::json!({"context":audit_context(),"iteration":_round,"points":result[..ATTACHMENT_VERTICES].iter().map(|p|p.to_array()).collect::<Vec<_>>(),"radial_limits":&radial_limits[..ATTACHMENT_VERTICES]})
                );
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

    fn tangential_head_fixture() -> (Vec<Vec3>, LayerSurface, Vec3, f32) {
        let mut reference = grid(0.2);
        for (i, point) in reference.iter_mut().enumerate().take(ATTACHMENT_VERTICES) {
            let angle = 0.08 * (i % (COLUMNS + 1)) as f32 / COLUMNS as f32;
            *point = Vec3::new(
                angle.sin() * 0.2,
                0.6 + 0.02 * (i / (COLUMNS + 1)) as f32,
                angle.cos() * 0.2,
            );
        }
        // The independent sheet rises both along the radial direction and
        // around the collar. It is not aligned with any captured head sample.
        let normal = Vec3::new(0.75, -0.55, 0.36).normalize();
        let center = Vec3::new(0.0, 0.62, 0.21) - normal * 0.034;
        let across = normal.any_orthonormal_vector() * 0.25;
        let along = normal.cross(across);
        let [a, b, c, d] = [
            center - across - along,
            center + across - along,
            center + across + along,
            center - across + along,
        ];
        (
            reference,
            LayerSurface::from_triangles(vec![[a, b, c], [a, c, d]]),
            normal,
            normal.dot(center),
        )
    }

    #[test]
    fn rising_tangential_head_sheet_uses_bounded_geometry_normal_after_old_family_fails() {
        let (reference, head, normal, plane) = tangential_head_fixture();
        let limits = [0.21; ATTACHMENT_VERTICES];
        let mut old_family_failed = false;
        for face in [
            [index(0, 0), index(0, 1), index(1, 0)],
            [index(0, 1), index(1, 1), index(1, 0)],
        ] {
            let vertices = face.map(|i| reference[i]);
            let axes = vertices.map(|p| p.with_y(0.0).normalize());
            let horizontal = axes.into_iter().sum::<Vec3>().normalize();
            let planes = domain(vertices, vertices, axes, horizontal);
            let cross = (vertices[1] - vertices[0]).cross(vertices[2] - vertices[0]);
            let authored = if cross.dot(horizontal).abs() > 1e-6 {
                cross.y / cross.dot(horizontal)
            } else {
                0.0
            };
            let feasible = face_normals(horizontal, authored).into_iter().any(|n| {
                let support = head
                    .triangles
                    .iter()
                    .filter_map(|&triangle| clipped_support(triangle, n, &planes))
                    .reduce(f32::max);
                support.is_some_and(|support| {
                    candidate(n, support, vertices, axes, [0.0; 3], [0.21; 3]).is_some()
                })
            });
            old_family_failed |= !feasible;
        }
        assert!(
            old_family_failed,
            "fixture must reject the original 45-normal family"
        );
        let mut work = Work::default();
        let fitted = fit(&reference, &head, &limits, &mut work).unwrap();
        assert!(work.fallbacks > 0);
        assert_eq!(
            &fitted[ATTACHMENT_VERTICES..],
            &reference[ATTACHMENT_VERTICES..]
        );
        for i in 0..ATTACHMENT_VERTICES {
            assert_eq!(fitted[i].y, reference[i].y);
            assert!(fitted[i].with_y(0.0).length() <= limits[i]);
        }
        for column in 0..COLUMNS {
            let [a, b, c, d] = [
                index(0, column),
                index(0, column + 1),
                index(1, column),
                index(1, column + 1),
            ];
            for face in [[a, b, c], [b, d, c]] {
                for u in 0..=4 {
                    for v in 0..=4 - u {
                        let weights = [u as f32 / 4.0, v as f32 / 4.0, (4 - u - v) as f32 / 4.0];
                        let point = face
                            .into_iter()
                            .zip(weights)
                            .map(|(i, w)| fitted[i] * w)
                            .sum::<Vec3>();
                        assert!(normal.dot(point) - plane >= LAYER_GAP + THICKNESS - 2e-5);
                        assert!(
                            mesh_distance(point, &head.triangles) >= LAYER_GAP + THICKNESS - 2e-5
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn geometry_candidates_reject_nonpositive_axes_and_insufficient_caps() {
        let (reference, head, _, _) = tangential_head_fixture();
        let face = [index(0, 0), index(0, 1), index(1, 0)];
        let vertices = face.map(|i| reference[i]);
        let axes = vertices.map(|p| p.with_y(0.0).normalize());
        for normal in [Vec3::Y, Vec3::NEG_Z] {
            assert!(candidate(normal, 0.0, vertices, axes, [0.0; 3], [0.21; 3]).is_none());
        }
        assert!(candidate(Vec3::Z, 0.3, vertices, axes, [0.0; 3], [0.21; 3]).is_none());
        assert!(fit_head_attachment(&reference, &head, &[0.21; ATTACHMENT_VERTICES]).is_ok());
        let limits: Vec<_> = reference[..ATTACHMENT_VERTICES]
            .iter()
            .map(|p| p.with_y(0.0).length() + 1e-5)
            .collect();
        let error = fit_head_attachment(&reference, &head, &limits).unwrap_err();
        assert_eq!(error.reason, "no bounded radial head attachment plane");
        assert_eq!(reference, tangential_head_fixture().0);
    }

    #[test]
    fn nearest_polygon_normal_is_rejected_when_a_decoy_controls_full_support() {
        let vertices = [
            Vec3::new(0.0, 0.59, 0.2),
            Vec3::new(0.08f32.sin() * 0.2, 0.59, 0.08f32.cos() * 0.2),
            Vec3::new(0.0, 0.61, 0.2),
        ];
        let axes = vertices.map(|p| p.with_y(0.0).normalize());
        let caps = std::array::from_fn(|i| axes[i] * 0.25 + Vec3::Y * vertices[i].y);
        let horizontal = axes.into_iter().sum::<Vec3>().normalize();
        let planes = domain(vertices, caps, axes, horizontal);
        let near = sheet(0.213);
        let decoy = [
            Vec3::new(-0.028, 0.58, 0.245),
            Vec3::new(-0.026, 0.62, 0.245),
            Vec3::new(-0.024, 0.60, 0.245),
        ];
        assert!(clipped_polygon(decoy, &planes).is_some());
        let near_only = LayerSurface::from_triangles(vec![near]);
        let all = LayerSurface::from_triangles(vec![near, decoy]);
        let (near_normal, near_support) = cap_candidates(&near_only, &planes, caps)[1];
        let (all_normal, all_support) = cap_candidates(&all, &planes, caps)[1];
        let near_normal = near_normal.unwrap();
        let all_normal = all_normal.unwrap();
        assert!(
            near_normal.dot(all_normal) > 0.99999,
            "decoy must not become this cap's nearest polygon"
        );
        assert!(candidate(
            near_normal,
            near_support,
            vertices,
            axes,
            [0.0; 3],
            [0.25; 3]
        )
        .is_some());
        assert!(
            all_support > near_support + 0.025,
            "full projection must include the nonnearest decoy"
        );
        assert!(candidate(all_normal, all_support, vertices, axes, [0.0; 3], [0.25; 3]).is_none());
    }

    #[test]
    fn closest_clipped_triangle_keeps_interior_edge_and_collapsed_witnesses() {
        let triangle = [Vec3::ZERO, Vec3::X, Vec3::Y];
        for (point, expected) in [
            (Vec3::new(0.25, 0.25, 1.0), Vec3::new(0.25, 0.25, 0.0)),
            (Vec3::new(0.5, -1.0, 0.0), Vec3::X * 0.5),
            (Vec3::new(-1.0, -1.0, 0.0), Vec3::ZERO),
        ] {
            assert!(closest_triangle(point, triangle).distance(expected) < 1e-6);
        }
        assert_eq!(closest_triangle(Vec3::Y, [Vec3::ZERO; 3]), Vec3::ZERO);
        assert!(
            closest_triangle(Vec3::new(0.4, 1.0, 0.0), [Vec3::ZERO, Vec3::X, Vec3::X])
                .distance(Vec3::X * 0.4)
                < 1e-6
        );
    }

    #[test]
    fn captured_tick_93_support_vertex_projects_to_cap_edge_interior() {
        // Actual float32 cap endpoints and the single clipped inner polygon
        // for leather-cloak tick 93, authoritative face [2, 51, 50].
        let caps = [
            Vec3::new(-0.12459273, 0.597, 0.08987747),
            Vec3::new(-0.24976021, 0.6106703, 0.06458989),
            Vec3::new(-0.24286704, 0.6029612, 0.086992055),
        ];
        let triangle = [
            Vec3::new(-0.1662941, 0.585, 0.036956266),
            Vec3::new(-0.1678184, 0.585, 0.031004336),
            Vec3::new(-0.16875507, 0.5858029, 0.031246567),
        ];
        let axes = [
            Vec3::new(-0.8110071, 0.0, 0.5850363),
            Vec3::new(-0.96814996, 0.0, 0.25037095),
            Vec3::new(-0.9414299, 0.0, 0.3372089),
        ];
        let surface = LayerSurface::from_triangles(vec![triangle]);
        for (normal, support) in cap_candidates(&surface, &[], caps) {
            let normal = normal.unwrap();
            let margin = caps
                .iter()
                .map(|p| normal.dot(*p))
                .fold(f32::INFINITY, f32::min)
                - support;
            assert!(
                margin < LAYER_GAP + THICKNESS || axes.iter().any(|axis| normal.dot(*axis) <= 1e-6)
            );
        }
        let (cap, support) = closest_triangle_pair(caps, triangle).unwrap();
        let edge = caps[1] - caps[0];
        let t = (cap - caps[0]).dot(edge) / edge.length_squared();
        assert!((t - 0.3876656).abs() < 2e-6);
        assert!(cap.distance(caps[0] + edge * t) < 1e-6);
        assert!(support.distance(triangle[0]) < 1e-6);
        let normal = cap_surface_normal(&surface, &[], caps).unwrap();
        assert!(normal.dot(Vec3::new(-0.14527598, 0.36841074, 0.91824204)) > 0.99999);
        let margin = caps
            .iter()
            .map(|p| normal.dot(*p))
            .fold(f32::INFINITY, f32::min)
            - triangle
                .iter()
                .map(|p| normal.dot(*p))
                .fold(f32::NEG_INFINITY, f32::max);
        assert!((margin - 0.0469572).abs() < 2e-6);
        assert!(axes.iter().all(|axis| normal.dot(*axis) > 0.3));
    }

    #[test]
    fn cap_surface_pair_finds_both_edge_interiors_and_global_nearest_patch() {
        let caps = [
            Vec3::new(-1.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(0.0, -1.0, 2.0),
        ];
        let triangle = [
            Vec3::new(0.0, -1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(1.0, 0.0, -1.0),
        ];
        let pair = closest_triangle_pair(caps, triangle).unwrap();
        assert!(pair.0.distance(Vec3::Z) < 1e-6);
        assert!(pair.1.length() < 1e-6);
        let remote = triangle.map(|point| point - Vec3::Z * 4.0);
        for triangles in [vec![remote, triangle], vec![triangle, remote]] {
            let surface = LayerSurface::from_triangles(triangles);
            assert!(
                cap_surface_normal(&surface, &[], caps)
                    .unwrap()
                    .distance(Vec3::Z)
                    < 1e-6
            );
            assert!(cap_surface_normal(&surface, &[(Vec3::X, 5.0)], caps).is_none());
        }
        let reversed = LayerSurface::from_triangles(vec![caps]);
        assert!(
            cap_surface_normal(&reversed, &[], triangle)
                .unwrap()
                .distance(Vec3::NEG_Z)
                < 1e-6
        );
    }

    #[test]
    fn cap_surface_proposal_rejects_collapsed_caps_and_true_intersections() {
        let caps = [
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::Y,
        ];
        let below = LayerSurface::from_triangles(vec![caps.map(|p| p - Vec3::Z)]);
        for collapsed in [[Vec3::ZERO; 3], [Vec3::ZERO, Vec3::X, Vec3::X * 2.0]] {
            assert!(cap_surface_normal(&below, &[], collapsed).is_none());
        }
        let mut invalid = caps;
        invalid[0].x = f32::NAN;
        assert!(cap_surface_normal(&below, &[], invalid).is_none());
        assert!(cap_surface_normal(&LayerSurface::from_triangles(Vec::new()), &[], caps).is_none());
        assert!(cap_surface_normal(&LayerSurface::from_triangles(vec![caps]), &[], caps).is_none());
        let piercing = [
            Vec3::new(0.0, -0.5, -1.0),
            Vec3::new(0.0, -0.5, 1.0),
            Vec3::new(0.0, 0.5, 1.0),
        ];
        assert!(
            closest_triangle_pair(caps, piercing).is_none(),
            "edge/face piercing is zero distance even without vertex or edge-pair contact"
        );
        assert!(
            cap_surface_normal(&LayerSurface::from_triangles(vec![piercing]), &[], caps).is_none()
        );
        assert!(cap_surface_normal(
            &LayerSurface::from_triangles(vec![[Vec3::ZERO; 3]]),
            &[],
            caps
        )
        .is_none());
        let point = LayerSurface::from_triangles(vec![[Vec3::NEG_Z; 3]]);
        assert!(
            cap_surface_normal(&point, &[], caps)
                .unwrap()
                .distance(Vec3::Z)
                < 1e-6
        );
        let nearly_parallel =
            LayerSurface::from_triangles(
                vec![caps.map(|p| Vec3::new(p.x, p.y, 1e-7 * p.x - 1e-4))],
            );
        assert!(
            cap_surface_normal(&nearly_parallel, &[], caps)
                .unwrap()
                .dot(Vec3::Z)
                > 0.99999,
            "a disjoint near-parallel pair must not be treated as intersecting"
        );
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
        let mut work = Work::default();
        let fitted = fit(&reference, &near, &limits, &mut work).unwrap();
        assert_eq!(
            work.fallbacks, 0,
            "successful original families must not invoke geometry candidates"
        );
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
