//! Detached arm cloth fitted to one upper-arm seam, with free leaf-shaped tails.
//! The profile owns style dimensions; the arm surface owns the attachment seat.
use super::*;

#[derive(Clone, Copy)]
struct DrapeProfile {
    seat: f32,
    length: f32,
    spread: f32,
    half_angle: f32,
}

impl DrapeProfile {
    fn grid(
        self,
        support: &crate::sdf::Sculpt,
        side: f32,
        build: f32,
        length: f32,
    ) -> Vec<Vec<Vec3>> {
        (0..=16)
            .map(|row| {
                let v = row as f32 / 16.0;
                (0..=16)
                    .map(|col| {
                        let u = col as f32 / 16.0;
                        // Cloth hangs on the outer/back side, clear of the elbow and torso.
                        let angle = side * (PI * 0.58 + (u - 0.5) * 2.0 * self.half_angle);
                        let direction = Vec3::new(angle.sin(), 0.0, angle.cos());
                        let origin = Vec3::Y * (self.seat * length);
                        let seat =
                            surface_ray_hit(support, origin, direction, 0.006 * build, 0.3 * build)
                                .expect("upper-arm drape seam must have support");
                        let leaf = (PI * u).sin().max(0.0).powf(0.55);
                        let drop = self.length * (0.62 + 0.38 * leaf) * v * length;
                        let spread = self.spread * (PI * v * 0.8).sin() * build;
                        let folds = (u * TAU * 3.0).sin() * 0.007 * v * build;
                        seat + Vec3::new(
                            side * spread * (0.35 + 0.9 * u),
                            -drop,
                            (-0.07 * v + 0.12 * (u - 0.5) * (PI * v).sin()) * build + folds,
                        )
                    })
                    .collect()
            })
            .collect()
    }
}

pub(super) fn emit(
    p: &mut Parts,
    sleeve: super::super::outfit::Sleeve,
    side: f32,
    build: f32,
    length: f32,
    muscle: f32,
    trim: Vec3,
) {
    let support = upper_arm_form(side, build, length, muscle, 0.94);
    let grid = DrapeProfile {
        seat: -0.13,
        length: 0.73,
        spread: 0.10,
        half_angle: 0.85,
    }
    .grid(&support, side, build, length);
    let color = sleeve.flare.unwrap_or(sleeve.color);
    let b = if sleeve.sheer {
        &mut p.sheer
    } else {
        &mut p.matte
    };
    for rows in grid.windows(2) {
        for k in 0..16 {
            let q = [rows[0][k], rows[0][k + 1], rows[1][k + 1], rows[1][k]];
            // A free sheet has no common "inside" point. Preserve its grid
            // winding, and reverse each exact triangle for the opaque back.
            // Re-triangulating the reversed quad crosses non-planar diagonals.
            for triangle in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
                b.tri(triangle, color);
                if !sleeve.sheer {
                    b.tri([triangle[0], triangle[2], triangle[1]], color * 0.84);
                }
            }
        }
    }
    // Boundaries use exactly the same grid, never separately estimated ribbons.
    for path in [
        grid[0].clone(),
        grid[16].clone(),
        grid.iter().map(|r| r[0]).collect(),
        grid.iter().map(|r| r[16]).collect(),
    ] {
        tube(&mut p.metal, &path, &vec![(0.002, 0.002); path.len()], trim);
    }
    // Resolve the ring from the anatomical support, never from the new cloth.
    let ring = radial_parts::RoundedBandProfile {
        height: -0.13 * length,
        half_height: 0.011 * build,
        clearance: 0.004 * build,
        radial_half: 0.0025 * build,
        sides: 32,
        sections: 6,
    };
    if let Some(band) = ring.resolve(
        |y, a, gap| {
            surface_ray_hit(
                &support,
                Vec3::Y * y,
                Vec3::new(a.sin(), 0.0, a.cos()),
                gap,
                0.3 * build,
            )
        },
        trim,
    ) {
        p.metal.absorb(band);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opaque_drape_has_exact_opposite_winding_for_every_triangle() {
        let mut p = Parts::default();
        emit(
            &mut p,
            super::super::super::outfit::Sleeve {
                color: Vec3::Y,
                cut: SleeveCut::Draped,
                puff: false,
                puff_lining: None,
                sheer: false,
                flare: None,
            },
            1.0,
            1.0,
            1.0,
            1.0,
            Vec3::X,
        );
        let mut faces = std::collections::BTreeMap::new();
        for triangle in p.matte.surface_triangles() {
            let key = triangle.map(|p| p.to_array().map(f32::to_bits));
            let mut sorted = key;
            sorted.sort();
            let n = (triangle[1] - triangle[0])
                .cross(triangle[2] - triangle[0])
                .normalize();
            faces.entry(sorted).or_insert_with(Vec::new).push(n);
        }
        assert_eq!(
            faces.len(),
            16 * 16 * 2,
            "one shared triangulation, no crossing reverse diagonal"
        );
        for normals in faces.values() {
            assert_eq!(normals.len(), 2, "one front and one back");
            assert!(
                normals[0].dot(normals[1]) < -0.999,
                "opposed normals, not coincident duplicates"
            );
        }
    }
}
