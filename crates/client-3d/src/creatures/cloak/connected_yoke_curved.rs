//! Test-only curved junction and complete-face certificate prototype.
//! No native garment routing, source exclusions, or native body-fit acceptance.
use super::*;

#[derive(Debug)]
struct ArcJunction {
    points: Vec<Vec3>,
    reference: Vec<Vec3>,
    shape: (f32, f32, usize, f32),
    accepted_epoch: Option<u64>,
    faces: Vec<[usize; 3]>,
    /// Each station's three branch ends, with paired physical thickness corners.
    ports: Vec<[[usize; 2]; 3]>,
}

impl ArcJunction {
    fn new(radius: f32, angle: f32, stations: usize, reach: f32) -> Result<Self, &'static str> {
        if !radius.is_finite()
            || !angle.is_finite()
            || !reach.is_finite()
            || stations < 2
            || reach <= THICKNESS
            || radius <= reach + THICKNESS
            || angle <= 0.0
            || angle >= TAU
            || angle / (stations - 1) as f32 > PI / 4.0
            || 2.0 * (radius - reach) * ((TAU - angle) * 0.5).sin() < LAYER_GAP + THICKNESS
        {
            return Err("unsupported arc dimensions");
        }
        let h = THICKNESS * 0.5;
        // Miter the radial branch thickness: chord midpoints otherwise shrink
        // thickness by cos(step/2), despite correct station measurements.
        let hx = h / (angle / (stations - 1) as f32 * 0.5).cos();
        let ring = [
            (-reach, -h),
            (reach, -h),
            (reach, h),
            (hx, h),
            (hx, reach),
            (-hx, reach),
            (-hx, h),
            (-reach, h),
        ];
        let mut points = Vec::new();
        let mut ports = Vec::new();
        for station in 0..stations {
            let a = angle * station as f32 / (stations - 1) as f32;
            let radial = Vec3::new(a.cos(), a.sin(), 0.0);
            points.extend(ring.map(|(x, z)| radial * (radius + x) + Vec3::Z * z));
            let b = station * 8;
            ports.push([[b, b + 7], [b + 1, b + 2], [b + 4, b + 5]]);
        }
        let mut faces = Vec::new();
        for station in 0..stations - 1 {
            for i in 0..8 {
                let j = (i + 1) % 8;
                let a = station * 8;
                faces.extend([[a + i, a + i + 8, a + j], [a + j, a + i + 8, a + j + 8]]);
            }
        }
        let start = points.len();
        points.push(Vec3::X * radius);
        points.push(Vec3::new(angle.cos(), angle.sin(), 0.0) * radius);
        for i in 0..8 {
            let j = (i + 1) % 8;
            let end = (stations - 1) * 8;
            faces.extend([[start, i, j], [start + 1, end + j, end + i]]);
        }
        Ok(Self {
            reference: points.clone(),
            shape: (radius, angle, stations, reach),
            accepted_epoch: None,
            points,
            faces,
            ports,
        })
    }
}

#[derive(Clone)]
struct FaceCertificate {
    face: usize,
    // Externally supplied STATIC plane-direction restrictions only. These are
    // not authenticated canonical motion axes and impose no displacement law.
    // Native motion-axis/domain ownership must be supplied before integration.
    axes: [Vec3; 3],
    normals: [Vec3; 3],
}

struct SourceEpoch<'a> {
    revision: u64,
    // Complete retained source triangles for this bounded prototype. Full
    // projection is deliberately conservative, never a proximity-only subset.
    triangles: [&'a [[Vec3; 3]]; 3], // inner, arms, head
    // Analytic body halfspaces n.p >= d certify complete triangles. Real native
    // SDF/armor support is NOT represented by these synthetic test fixtures.
    body: &'a [(Vec3, f32)],
}

#[derive(Clone, Copy)]
struct VertexBounds {
    minimum: Vec3,
    maximum: Vec3,
    radial_min: f32,
    radial_max: f32,
}

/// Static final-geometry source/shape transaction for an analytic test fixture only.
/// No native motion-axis or general internal self-clearance acceptance is provided.
/// Every face and source is required. No partial-success or native fit label.
fn certify_and_commit(
    mesh: &mut ArcJunction,
    proposal: &ArcJunction,
    bounds: &[VertexBounds],
    expected_epoch: u64,
    source: &SourceEpoch<'_>,
    certificates: &[FaceCertificate],
) -> Result<usize, &'static str> {
    let (radius, angle, stations, reach) = proposal.shape;
    let regenerated = ArcJunction::new(radius, angle, stations, reach)?;
    if proposal.points != regenerated.points
        || proposal.faces != mesh.faces
        || proposal.ports != mesh.ports
    {
        return Err("unqualified junction shape or port correspondence");
    }
    let candidate = &proposal.points;
    if expected_epoch != source.revision {
        return Err("stale source epoch");
    }
    if source.triangles.iter().any(|s| s.is_empty()) || source.body.is_empty() {
        return Err("unsupported missing physical source");
    }
    if candidate.len() != mesh.points.len()
        || bounds.len() != candidate.len()
        || certificates.len() != mesh.faces.len()
    {
        return Err("incomplete owned domain");
    }
    let mut seen = vec![false; mesh.faces.len()];
    for c in certificates {
        if c.face >= seen.len() || std::mem::replace(&mut seen[c.face], true) {
            return Err("duplicate or invalid face ownership");
        }
    }
    for (p, b) in candidate.iter().zip(bounds) {
        let radius = p.with_y(0.0).length();
        if !p.is_finite()
            || !b.minimum.is_finite()
            || !b.maximum.is_finite()
            || !b.radial_min.is_finite()
            || !b.radial_max.is_finite()
            || p.cmplt(b.minimum).any()
            || p.cmpgt(b.maximum).any()
            || radius < b.radial_min
            || radius > b.radial_max
        {
            return Err("attachment or station bounds");
        }
    }
    for c in certificates {
        let face = mesh.faces[c.face];
        let old = face.map(|i| mesh.reference[i]);
        let new = face.map(|i| candidate[i]);
        let before = (old[1] - old[0]).cross(old[2] - old[0]);
        let after = (new[1] - new[0]).cross(new[2] - new[0]);
        if !after.is_finite() || after.length() < before.length() * 0.5 || after.dot(before) <= 0.0
        {
            return Err("collapsed or reversed incident face");
        }
        for &(n, d) in source.body {
            if !n.is_finite()
                || !d.is_finite()
                || (n.length_squared() - 1.0).abs() > 1e-5
                || new.iter().any(|p| n.dot(*p) < d)
            {
                return Err("body physical halfspace");
            }
        }
        for (s, n) in source.triangles.iter().zip(c.normals) {
            if !n.is_finite()
                || (n.length_squared() - 1.0).abs() > 1e-5
                || c.axes.iter().any(|a| !a.is_finite() || n.dot(*a) <= 1e-6)
            {
                return Err("invalid normal or supplied static direction");
            }
            let mut support = f32::NEG_INFINITY;
            for p in s.iter().flatten() {
                if !p.is_finite() {
                    return Err("invalid retained source");
                }
                support = support.max(n.dot(*p));
            }
            // The existing middle-sheet reserve is unchanged. Applying it to
            // this entire junction solid is conservative, not a new native gate.
            if new
                .iter()
                .any(|p| n.dot(*p) < support + LAYER_GAP + THICKNESS)
            {
                return Err("incident face source clearance");
            }
        }
    }
    mesh.points.clone_from_slice(candidate);
    mesh.shape = proposal.shape;
    mesh.accepted_epoch = Some(source.revision);
    Ok(certificates.len())
}

#[test]
fn curved_yoke_ports_share_closed_thickness_geometry() {
    use std::collections::BTreeMap;
    let m = ArcJunction::new(0.3, PI * 1.5, 49, 0.04).unwrap();
    let mut edges = BTreeMap::<(usize, usize), (usize, i32)>::new();
    let mut volume = 0.0;
    for f in &m.faces {
        let p = f.map(|i| m.points[i]);
        assert!((p[1] - p[0]).cross(p[2] - p[0]).length() > 1e-8);
        volume += p[0].dot(p[1].cross(p[2])) / 6.0;
        for (a, b) in [(f[0], f[1]), (f[1], f[2]), (f[2], f[0])] {
            let e = edges.entry((a.min(b), a.max(b))).or_default();
            e.0 += 1;
            e.1 += if a < b { 1 } else { -1 };
        }
    }
    assert!(volume > 0.0);
    assert!(edges.values().all(|&(count, sign)| count == 2 && sign == 0));
    assert_eq!(
        m.points.len() as isize - edges.len() as isize + m.faces.len() as isize,
        2
    );
    assert_eq!(m.ports.len(), 49);
    for ports in &m.ports {
        for &[a, b] in ports {
            assert!(m.points[a].distance(m.points[b]) >= 0.012 - 1e-7);
        }
    }
    // Adjacent station branch ports share actual topology, not duplicate cuts.
    for pair in m.ports.windows(2) {
        for (&[a, b], &[c, d]) in pair[0].iter().zip(&pair[1]) {
            assert!(edges.contains_key(&(a.min(c), a.max(c))));
            assert!(edges.contains_key(&(b.min(d), b.max(d))));
        }
    }
    for pair in m.ports.windows(2) {
        let [a, b] = pair[0][2];
        let [c, d] = pair[1][2];
        assert!(
            ((m.points[a] + m.points[c]) * 0.5).distance((m.points[b] + m.points[d]) * 0.5)
                >= 0.012 - 1e-7
        );
    }
    assert!(ArcJunction::new(0.03, PI, 49, 0.04).is_err());
    assert!(ArcJunction::new(0.3, TAU, 49, 0.04).is_err());
    // Distinct exposed caps must not approach within the unchanged reserve.
    assert!(ArcJunction::new(0.3, TAU - 0.01, 65, 0.04).is_err());
}

#[test]
fn curved_yoke_transaction_requires_every_incident_face_source_and_epoch() {
    let mut m = ArcJunction::new(0.3, PI * 0.5, 9, 0.04).unwrap();
    let old = m.points.clone();
    let candidate = ArcJunction::new(0.301, PI * 0.5 + 0.001, 9, 0.04).unwrap();
    let bounds: Vec<_> = old
        .iter()
        .map(|p| VertexBounds {
            minimum: *p - Vec3::splat(0.01),
            maximum: *p + Vec3::splat(0.01),
            radial_min: 0.0,
            radial_max: 0.4,
        })
        .collect();
    let triangles = vec![[
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(1.0, -1.0, -1.0),
        Vec3::new(0.0, 1.0, -1.0),
    ]];
    let body = [(Vec3::Z, -0.1)];
    let source = SourceEpoch {
        revision: 7,
        triangles: [&triangles; 3],
        body: &body,
    };
    let certs: Vec<_> = (0..m.faces.len())
        .map(|face| FaceCertificate {
            face,
            axes: [Vec3::Z; 3],
            normals: [Vec3::Z; 3],
        })
        .collect();
    assert_eq!(
        certify_and_commit(&mut m, &candidate, &bounds, 6, &source, &certs),
        Err("stale source epoch")
    );
    assert_eq!(m.points, old);
    assert!(certify_and_commit(&mut m, &candidate, &bounds, 7, &source, &certs[1..]).is_err());
    assert_eq!(m.points, old);
    let missing = SourceEpoch {
        revision: 7,
        triangles: [&triangles, &triangles, &[]],
        body: &body,
    };
    assert!(certify_and_commit(&mut m, &candidate, &bounds, 7, &missing, &certs).is_err());
    assert_eq!(m.points, old);
    let mut duplicate = certs.clone();
    duplicate[1].face = duplicate[0].face;
    assert_eq!(
        certify_and_commit(&mut m, &candidate, &bounds, 7, &source, &duplicate),
        Err("duplicate or invalid face ownership")
    );
    let mut invalid_axis = certs.clone();
    invalid_axis[0].axes[0] = Vec3::NEG_Z;
    assert!(certify_and_commit(&mut m, &candidate, &bounds, 7, &source, &invalid_axis).is_err());
    let mut tight = bounds.clone();
    tight[0].maximum.y = -1.0;
    assert!(certify_and_commit(&mut m, &candidate, &tight, 7, &source, &certs).is_err());
    tight = bounds.clone();
    tight[0].radial_max = 0.01;
    assert!(certify_and_commit(&mut m, &candidate, &tight, 7, &source, &certs).is_err());
    let intruding_body = [(Vec3::Z, 0.05)];
    let bad_body = SourceEpoch {
        revision: 7,
        triangles: [&triangles; 3],
        body: &intruding_body,
    };
    assert_eq!(
        certify_and_commit(&mut m, &candidate, &bounds, 7, &bad_body, &certs),
        Err("body physical halfspace")
    );
    let mut false_port = ArcJunction::new(0.301, PI * 0.5 + 0.001, 9, 0.04).unwrap();
    false_port.ports[0][0] = false_port.ports[1][0];
    assert!(certify_and_commit(&mut m, &false_port, &bounds, 7, &source, &certs).is_err());
    assert_eq!(m.points, old);
    assert_eq!(m.accepted_epoch, None);
    assert_eq!(
        certify_and_commit(&mut m, &candidate, &bounds, 7, &source, &certs),
        Ok(m.faces.len())
    );
    assert_eq!(m.points, candidate.points);
    // A newly activated source blocks one shared corner; no partial commit.
    let mut blocker = triangles.clone();
    blocker.push([candidate.points[8]; 3]);
    let blocked = SourceEpoch {
        revision: 8,
        triangles: [&triangles, &triangles, &blocker],
        body: &body,
    };
    assert_eq!(
        certify_and_commit(&mut m, &candidate, &bounds, 8, &blocked, &certs),
        Err("incident face source clearance")
    );
    assert_eq!(m.points, candidate.points);
    let mut collapsed = ArcJunction::new(0.301, PI * 0.5 + 0.001, 9, 0.04).unwrap();
    collapsed.points[8] = collapsed.points[0];
    assert!(certify_and_commit(&mut m, &collapsed, &bounds, 7, &source, &certs).is_err());
    assert_eq!(m.points, candidate.points);
    assert_eq!(m.accepted_epoch, Some(7));
}

#[test]
fn curved_yoke_exports_one_captured_cope_turnover_candidate() {
    // Circle through reference row6 columns23,24,25 in capture SHA256
    // 077151873a2a5561759b7d6f285b0a61aceff13d784610feec92cc905f72177a.
    // Mapping and all-source audit are separate retained diagnostics. This
    // constructor test does NOT assert clearance against that real fixture.
    let m = ArcJunction::new(0.499_282_57, 0.250_966_6, 3, 0.04).unwrap();
    assert_eq!(m.ports.len(), 3);
    eprintln!(
        "captured curved junction: {}",
        serde_json::json!({
            "points":m.points.iter().map(|p|p.to_array()).collect::<Vec<_>>(),
            "faces":m.faces,"ports":m.ports,"scope":"native curved candidate only; not accepted garment"
        })
    );
}
