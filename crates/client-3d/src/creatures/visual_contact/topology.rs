//! Immutable local-mesh topology. Pose transforms cannot change edge incidence.
use super::Vec3;
use std::collections::HashMap;

pub(super) struct Topology {
    pub(super) faces: Vec<Option<usize>>,
    pub(super) count: usize,
}

impl Topology {
    pub(super) fn new(triangles: &[[Vec3; 3]]) -> Self {
        type Point = [i64; 3];
        let key = |p: Vec3| p.to_array().map(|v| (v as f64 * 100_000.0).round() as i64);
        let mut edges: HashMap<(Point, Point), Vec<(usize, bool)>> = HashMap::new();
        let mut parent: Vec<_> = (0..triangles.len()).collect();
        let mut eligible = vec![false; triangles.len()];
        fn root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        for (i, tri) in triangles.iter().enumerate() {
            // Never let nonfinite coordinates manufacture a closed edge.
            if tri.iter().any(|p| !p.is_finite()) {
                continue;
            }
            let keys = tri.map(key);
            if keys[0] == keys[1]
                || keys[1] == keys[2]
                || keys[2] == keys[0]
                || (tri[1] - tri[0]).cross(tri[2] - tri[0]).length_squared() <= 1e-16
            {
                continue;
            }
            eligible[i] = true;
            for [a, b] in [[keys[0], keys[1]], [keys[1], keys[2]], [keys[2], keys[0]]] {
                let forward = a < b;
                edges
                    .entry(if forward { (a, b) } else { (b, a) })
                    .or_default()
                    .push((i, forward));
            }
        }
        for faces in edges.values() {
            for &(i, _) in &faces[1..] {
                let first = root(&mut parent, faces[0].0);
                let other = root(&mut parent, i);
                // Canonical roots make labels independent of HashMap order.
                parent[first.max(other)] = first.min(other);
            }
        }
        let mut closed = vec![true; triangles.len()];
        for faces in edges.values() {
            if faces.len() != 2 || faces[0].1 == faces[1].1 {
                let group = root(&mut parent, faces[0].0);
                closed[group] = false;
            }
        }
        let mut labels = vec![None; triangles.len()];
        let mut count = 0;
        let faces = (0..triangles.len())
            .map(|i| {
                let group = root(&mut parent, i);
                if !eligible[i] || !closed[group] {
                    return None;
                }
                Some(*labels[group].get_or_insert_with(|| {
                    let label = count;
                    count += 1;
                    label
                }))
            })
            .collect();
        Self { faces, count }
    }
}
