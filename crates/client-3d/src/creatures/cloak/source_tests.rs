//! Full-grid witnesses for independently certified inner and head planes.
use super::*;

const OFFSET: Vec3 = Vec3::new(0.0, 0.53, -0.15);

fn grid() -> Vec<Vec3> {
    (0..GRID)
        .map(|i| {
            let row = i / (COLUMNS + 1);
            let angle = -0.01 + 0.02 * (i % (COLUMNS + 1)) as f32 / COLUMNS as f32;
            let radius = if row == 0 {
                0.20
            } else {
                0.22 + 0.002 * (row - 1) as f32
            };
            Vec3::new(angle.sin() * radius, 0.625, angle.cos() * radius) - OFFSET
        })
        .collect()
}

fn patch(z: f32, y: f32) -> [Vec3; 3] {
    [
        Vec3::new(-0.02, y, z),
        Vec3::new(0.02, y, z),
        Vec3::new(0.0, y, z + 0.0005),
    ]
}

fn limits(reference: &[Vec3]) -> Vec<f32> {
    reference
        .iter()
        .map(|p| (*p + OFFSET).with_y(0.0).length() + 0.001)
        .collect()
}

fn support<'a>(
    inner: Option<&'a LayerSurface>,
    head: Option<&'a LayerSurface>,
    profile: Option<&'a [Vec2]>,
    limits: &'a [f32],
) -> FaceSupport<'a> {
    FaceSupport {
        inner,
        arms: None,
        head,
        profile,
        radial_limits: Some(limits),
    }
}

fn solve(reference: &[Vec3], support: FaceSupport<'_>, cached: bool) -> (Vec<Vec3>, FaceFitWork) {
    let mut points = reference.to_vec();
    let mut work = FaceFitWork::default();
    fit_radial_faces_impl(
        &mut points,
        Quat::IDENTITY,
        support,
        &mut None,
        cached,
        cached,
        &mut work,
    )
    .unwrap();
    (points, work)
}

// Independent plane/edge projection against the emitted open sheets. This
// oracle neither queries the solver's footprints nor selects support planes.
fn distance(point: Vec3, [a, b, c]: [Vec3; 3]) -> f32 {
    let normal = (b - a).cross(c - a).normalize();
    let signed = normal.dot(point - a);
    let projected = point - normal * signed;
    let mut nearest = f32::INFINITY;
    if [(a, b), (b, c), (c, a)]
        .iter()
        .all(|&(a, b)| (b - a).cross(projected - a).dot(normal) >= 0.0)
    {
        nearest = signed.abs();
    }
    for (a, b) in [(a, b), (b, c), (c, a)] {
        let edge = b - a;
        let t = ((point - a).dot(edge) / edge.length_squared()).clamp(0.0, 1.0);
        nearest = nearest.min(point.distance(a + edge * t));
    }
    nearest
}

fn assert_clear_faces(points: &[Vec3], sources: &[[Vec3; 3]]) {
    let mut samples = 0;
    for row in 0..ROWS {
        for column in 0..COLUMNS {
            let [a, b, c, d] = [
                index(row, column),
                index(row, column + 1),
                index(row + 1, column),
                index(row + 1, column + 1),
            ];
            for face in [[a, b, c], [b, d, c]] {
                for weights in [
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [0.5, 0.5, 0.0],
                    [0.5, 0.0, 0.5],
                    [0.0, 0.5, 0.5],
                    [1.0 / 3.0; 3],
                    [0.2, 0.3, 0.5],
                ] {
                    let point = face
                        .into_iter()
                        .zip(weights)
                        .map(|(i, w)| (points[i] + OFFSET) * w)
                        .sum::<Vec3>();
                    for &source in sources {
                        let clearance = distance(point, source);
                        assert!(
                            clearance >= LAYER_GAP + THICKNESS - 2e-5,
                            "row {row} column {column} face {face:?} has {clearance} clearance at {point:?}"
                        );
                    }
                    samples += 1;
                }
            }
        }
    }
    assert_eq!(samples, ROWS * COLUMNS * 2 * 8);
}

#[test]
fn independent_inner_and_head_planes_clear_shared_vertices_in_the_full_solver() {
    let reference = grid();
    let radial_limits = limits(&reference);
    // Fixed +Y motion admits opposite radial/upward separating planes. The
    // source union crosses the collar's allowed position, so one plane cannot
    // reserve 32 mm from both patches below the unchanged attachment ceiling.
    let profile = vec![Vec2::Y; GRID];
    let inner_patch = patch(0.169, 0.635);
    let head_patch = patch(0.251, 0.639);
    let inner = LayerSurface::from_triangles(vec![inner_patch]);
    let head = LayerSurface::from_triangles(vec![head_patch]);
    let (inner_only, _) = solve(
        &reference,
        support(Some(&inner), None, Some(&profile), &radial_limits),
        true,
    );
    let (head_only, _) = solve(
        &reference,
        support(None, Some(&head), Some(&profile), &radial_limits),
        true,
    );
    assert_clear_faces(&inner_only, &[inner_patch]);
    assert_clear_faces(&head_only, &[head_patch]);
    let collar = index(0, COLUMNS / 2);
    let shared = index(1, COLUMNS / 2);
    assert!((inner_only[collar] + OFFSET).y > 0.63);
    assert!((head_only[shared] + OFFSET).y > 0.63);
    assert!((head_only[collar] + OFFSET).y < 0.626);
    assert!((inner_only[shared] + OFFSET).y < 0.626);

    let combined = support(Some(&inner), Some(&head), Some(&profile), &radial_limits);
    let (fitted, work) = solve(&reference, combined, true);
    let (uncached, _) = solve(&reference, combined, false);
    assert_eq!(
        fitted, uncached,
        "source-specific constraints survive cache/requery decisions"
    );
    assert!(
        work.rounds >= 2,
        "shared travel must trigger a complete grown-domain pass"
    );
    assert_clear_faces(&fitted, &[inner_patch, head_patch]);
    assert!((fitted[collar] + OFFSET).y > 0.63 && (fitted[shared] + OFFSET).y > 0.63);
    let ceiling = 0.565 + 2.0 * (LAYER_GAP + THICKNESS + ROOF_SLOPE_SLACK);
    for i in 0..GRID {
        let before = reference[i] + OFFSET;
        let after = fitted[i] + OFFSET;
        assert_eq!(after.x, before.x);
        assert_eq!(after.z, before.z);
        assert!(after.y >= before.y);
        assert!(after.with_y(0.0).length() <= radial_limits[i]);
        if i <= COLUMNS {
            assert!(after.y <= ceiling);
        }
    }

    // Both patches are within the first-band finite obstacle footprint. Treat
    // them as one source to exercise the previous common-plane requirement.
    let union = LayerSurface::from_triangles(vec![inner_patch, head_patch]);
    let mut rejected = reference.clone();
    let error = fit_radial_supports_traced(
        &mut rejected,
        Quat::IDENTITY,
        support(None, Some(&union), Some(&profile), &radial_limits),
        &mut None,
    )
    .unwrap_err();
    assert_eq!(
        error.reason,
        "no separating plane preserves bounded collar attachment"
    );
    assert_eq!(
        rejected, reference,
        "the common-plane negative control is transactional"
    );
}

#[test]
fn empty_head_preserves_default_and_explicit_profile_full_solver_geometry() {
    let reference = grid();
    let radial_limits = limits(&reference);
    let inner_patch = patch(0.169, 0.635);
    let inner = LayerSurface::from_triangles(vec![inner_patch]);
    let empty = LayerSurface::from_triangles(Vec::new());
    let profile = vec![Vec2::Y; GRID];
    for profile in [None, Some(profile.as_slice())] {
        let (ordinary, _) = solve(
            &reference,
            support(Some(&inner), None, profile, &radial_limits),
            true,
        );
        let (with_empty, _) = solve(
            &reference,
            support(Some(&inner), Some(&empty), profile, &radial_limits),
            true,
        );
        assert_eq!(
            ordinary, with_empty,
            "an empty head must not change default or explicit movement geometry"
        );
        assert_ne!(
            ordinary, reference,
            "equivalence must include an actual fitted contact"
        );
        assert_clear_faces(&ordinary, &[inner_patch]);
    }
}

#[test]
fn an_impossible_individual_source_rolls_back_the_combined_full_solve() {
    let reference = grid();
    let radial_limits = limits(&reference);
    let profile = vec![Vec2::Y; GRID];
    let inner = LayerSurface::from_triangles(vec![patch(0.169, 0.635)]);
    let head = LayerSurface::from_triangles(vec![patch(0.251, 0.639)]);
    let impossible = LayerSurface::from_triangles(vec![patch(0.20, 0.635)]);
    // Establish that both unchanged sources have valid full-grid fixtures.
    solve(
        &reference,
        support(Some(&inner), None, Some(&profile), &radial_limits),
        true,
    );
    solve(
        &reference,
        support(None, Some(&head), Some(&profile), &radial_limits),
        true,
    );
    for (inner, head) in [(&impossible, &head), (&inner, &impossible)] {
        let mut candidate = reference.clone();
        let error = fit_radial_supports_traced(
            &mut candidate,
            Quat::IDENTITY,
            support(Some(inner), Some(head), Some(&profile), &radial_limits),
            &mut None,
        )
        .unwrap_err();
        assert_eq!(
            error.reason,
            "no separating plane preserves bounded collar attachment"
        );
        assert_eq!(
            candidate, reference,
            "one impossible source must not publish any other source's partial fit"
        );
    }
}
