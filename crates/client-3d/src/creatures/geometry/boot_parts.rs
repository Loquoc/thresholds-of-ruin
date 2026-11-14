//! Constructed leather uppers, layered welts and separate tapered heels.
//! The last and the garment contact envelope resolve from the same footwear choice.
use super::*;

fn loft<const N: usize>(rings: &[[Vec3; N]]) -> Vec<[Vec3; 3]> {
    let mut faces = Vec::new();
    for pair in rings.windows(2) {
        for j in 0..N {
            let k = (j + 1) % N;
            faces.push([pair[0][j], pair[1][k], pair[1][j]]);
            faces.push([pair[0][j], pair[0][k], pair[1][k]]);
        }
    }
    for (ring, forward) in [(&rings[0], false), (&rings[rings.len() - 1], true)] {
        let center = ring.iter().copied().sum::<Vec3>() / N as f32;
        for j in 0..N {
            let k = (j + 1) % N;
            faces.push(if forward {
                [center, ring[j], ring[k]]
            } else {
                [center, ring[k], ring[j]]
            });
        }
    }
    faces
}

fn components(last: BootLast) -> [Vec<[Vec3; 3]>; 3] {
    let sections = last.sections();
    // Counterclockwise D-shaped sections, with a flat buried base and rounded vamp.
    const UPPER: [(f32, f32); 9] = [
        (-1.0, 0.0),
        (1.0, 0.0),
        (1.0, 0.18),
        (0.78, 0.70),
        (0.35, 0.96),
        (0.0, 1.0),
        (-0.35, 0.96),
        (-0.78, 0.70),
        (-1.0, 0.18),
    ];
    let upper = sections.map(|s| {
        UPPER.map(|(x, y)| Vec3::new(x * s.width * 0.94, s.top - 0.003 + y * s.height, s.z))
    });
    // A bevel distinguishes the welt edge from the flat supporting outsole.
    let sole = sections.map(|s| {
        [
            Vec3::new(-s.width, s.top, s.z),
            Vec3::new(-s.width, s.bottom + 0.004, s.z),
            Vec3::new(-s.width * 0.82, s.bottom, s.z),
            Vec3::new(s.width * 0.82, s.bottom, s.z),
            Vec3::new(s.width, s.bottom + 0.004, s.z),
            Vec3::new(s.width, s.top, s.z),
        ]
    });
    let heel = if last.heeled {
        let b = last.build;
        const CORNERS: [(f32, f32); 8] = [
            (-0.7, -1.0),
            (0.7, -1.0),
            (1.0, -0.7),
            (1.0, 0.7),
            (0.7, 1.0),
            (-0.7, 1.0),
            (-1.0, 0.7),
            (-1.0, -0.7),
        ];
        // Loft toward +Y: use XZ rings with reversed winding for an upward cap.
        let rings = [
            (BootLast::FLOOR, 0.022 * b, 0.020 * b),
            (-0.036, 0.034 * b, 0.026 * b),
        ]
        .map(|(y, w, d)| CORNERS.map(|(x, z)| Vec3::new(x * w, y, -0.035 * b - z * d)));
        loft(&rings)
    } else {
        Vec::new()
    };
    [loft(&upper), loft(&sole), heel]
}

/// Highest actual shoe-upper point in the ankle frame. Adjacent calf bands
/// use this attachment limit rather than guessing clearance from ankle alone.
pub(super) fn shoe_upper_top(last: BootLast) -> f32 {
    components(last)[0]
        .iter()
        .flatten()
        .map(|p| p.y)
        .fold(f32::NEG_INFINITY, f32::max)
}

/// A shoe on the same last: the upper closed in its colour, or for a sandal
/// the bare foot, on a sole and heel of the shoe's.
pub(super) fn emit_shoe(p: &mut Parts, last: BootLast, upper: Vec3, sole: Vec3) {
    let [foot, base, heel] = components(last);
    for face in foot {
        p.satin.tri(face, upper);
    }
    for face in base {
        p.matte.tri(face, sole * 0.6);
    }
    for face in heel {
        p.matte.tri(face, sole * 0.8);
    }
}

pub(super) fn emit(p: &mut Parts, last: BootLast, color: Vec3) {
    let [upper, sole, heel] = components(last);
    for face in upper {
        p.satin.tri(face, color);
    }
    for face in sole {
        let n = (face[1] - face[0])
            .cross(face[2] - face[0])
            .normalize_or_zero();
        p.matte
            .tri(face, color * if n.y < -0.5 { 0.32 } else { 0.56 });
    }
    for face in heel {
        p.matte.tri(face, color * 0.40);
    }
}

fn clip(polygon: Vec<Vec3>, normal: Vec3, offset: f32) -> Vec<Vec3> {
    let mut out = Vec::new();
    if polygon.is_empty() {
        return out;
    }
    let mut a = *polygon.last().unwrap();
    let mut da = a.dot(normal) - offset;
    for b in polygon {
        let db = b.dot(normal) - offset;
        if (da >= 0.0) != (db >= 0.0) {
            out.push(a.lerp(b, da / (da - db)));
        }
        if db >= 0.0 {
            out.push(b);
        }
        a = b;
        da = db;
    }
    out
}

/// Native leather follows retained calf/cloth triangles by direction. The
/// authored ankle, height and front silhouette remain lower bounds; only
/// support-facing portions expand. This does not alter the foot last or sole.
pub(super) struct NativeShaft {
    rings: [[Vec3; 32]; 25],
    authored_radius_top: f32,
}
impl NativeShaft {
    pub(super) fn fitted(shape: BootShape, support: &Builder, build: f32) -> Self {
        let mut radii: [[f32; 32]; 25] = std::array::from_fn(|row| {
            let t = row as f32 / 24.0;
            [shape.radius_bottom + (shape.radius_top - shape.radius_bottom) * t; 32]
        });
        let height = |row: usize| shape.bottom + (shape.top - shape.bottom) * row as f32 / 24.0;
        for tri in support.surface_triangles() {
            let low = tri.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
            let high = tri.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
            for row in 0..24 {
                if high < height(row) || low > height(row + 1) {
                    continue;
                }
                let slab = clip(
                    clip(tri.to_vec(), Vec3::Y, height(row)),
                    Vec3::NEG_Y,
                    -height(row + 1),
                );
                if slab.is_empty() {
                    continue;
                }
                for col in 0..32 {
                    let angle = col as f32 * TAU / 32.0;
                    let next = (col + 1) as f32 * TAU / 32.0;
                    let cell = clip(
                        clip(slab.clone(), Vec3::new(-angle.sin(), 0.0, angle.cos()), 0.0),
                        Vec3::new(next.sin(), 0.0, -next.cos()),
                        0.0,
                    );
                    if cell.is_empty() {
                        continue;
                    }
                    // Norm is convex, so its maximum on a clipped support
                    // polygon occurs at a vertex. Requiring every cell corner
                    // to clear this radius bounds both shaft triangle interiors,
                    // including varying radii and support between sampled rays.
                    let radius = (cell.iter().map(|p| p.xz().length()).fold(0.0, f32::max)
                        + 0.006 * build)
                        / (PI / 32.0).cos();
                    for r in [row, row + 1] {
                        for c in [col, (col + 1) % 32] {
                            radii[r][c] = radii[r][c].max(radius);
                        }
                    }
                }
            }
        }
        let rings = std::array::from_fn(|row| {
            std::array::from_fn(|col| {
                let angle = col as f32 * TAU / 32.0;
                Vec3::new(
                    angle.cos() * radii[row][col],
                    height(row),
                    angle.sin() * radii[row][col],
                )
            })
        });
        Self {
            rings,
            authored_radius_top: shape.radius_top,
        }
    }

    fn radius(&self, y: f32, theta: f32) -> f32 {
        let low = self.rings[0][0].y;
        let high = self.rings[24][0].y;
        let row = ((y - low) / (high - low)).clamp(0.0, 1.0) * 24.0;
        let i = (row.floor() as usize).min(23);
        let a = row - i as f32;
        let col = theta.rem_euclid(TAU) * 32.0 / TAU;
        let j = (col.floor() as usize).min(31);
        let b = col - j as f32;
        let at = |r: usize| {
            self.rings[r][j]
                .xz()
                .length()
                .lerp(self.rings[r][(j + 1) % 32].xz().length(), b)
        };
        at(i).lerp(at(i + 1), a)
    }

    /// Conservative circular envelope for existing surrounding-cloth queries;
    /// the rendered leather remains directional rather than using this cone.
    pub(super) fn envelope(&self, mut shape: BootShape) -> BootShape {
        shape.radius_bottom = self.rings[0]
            .iter()
            .map(|p| p.xz().length())
            .fold(shape.radius_bottom, f32::max)
            + 0.03;
        for (row, ring) in self.rings.iter().enumerate().skip(1) {
            let t = row as f32 / 24.0;
            let required = ring.iter().map(|p| p.xz().length()).fold(0.0, f32::max) + 0.03;
            shape.radius_top = shape
                .radius_top
                .max((required - shape.radius_bottom * (1.0 - t)) / t);
        }
        shape.cuff_radius = self.rings[24]
            .iter()
            .map(|p| p.xz().length())
            .fold(0.0, f32::max)
            + 0.002;
        shape.sample_radius_top = shape.radius_top + shape.cuff_thickness + 0.006;
        shape.sample_radius_bottom = shape.radius_bottom + 0.012;
        shape
    }

    pub(super) fn emit(&self, builder: &mut Builder, color: Vec3) {
        for rows in self.rings.windows(2) {
            let inside = Vec3::Y * (rows[0][0].y + rows[1][0].y) * 0.5;
            for column in 0..32 {
                let next = (column + 1) % 32;
                builder.quad_out(
                    [
                        rows[0][column],
                        rows[0][next],
                        rows[1][next],
                        rows[1][column],
                    ],
                    inside,
                    color,
                );
            }
        }
        for row in [0, 24] {
            let center = Vec3::Y * self.rings[row][0].y;
            let inside = Vec3::Y * (self.rings[0][0].y + self.rings[24][0].y) * 0.5;
            for column in 0..32 {
                builder.tri_out(
                    [
                        center,
                        self.rings[row][column],
                        self.rings[row][(column + 1) % 32],
                    ],
                    inside,
                    color,
                );
            }
        }
    }

    pub(super) fn band(&self, builder: &mut Builder, y: f32, thickness: f32, color: Vec3) {
        let point = |i: usize, j: usize| {
            let u = i as f32 * TAU / 32.0;
            let v = j as f32 * TAU / 6.0;
            let h = y + thickness * v.sin();
            let radial = Vec3::new(u.cos(), 0.0, u.sin());
            Vec3::Y * h + radial * (self.radius(h, u) + 0.002 + thickness * v.cos())
        };
        for i in 0..32 {
            let u = (i as f32 + 0.5) * TAU / 32.0;
            let inside =
                Vec3::Y * y + Vec3::new(u.cos(), 0.0, u.sin()) * (self.radius(y, u) + 0.002);
            for j in 0..6 {
                builder.quad_out(
                    [
                        point(i, j),
                        point(i + 1, j),
                        point(i + 1, j + 1),
                        point(i, j + 1),
                    ],
                    inside,
                    color,
                );
            }
        }
    }

    /// Keep the original fur density, length and stagger, changing only the
    /// attachment path to follow the directional leather at each root height.
    pub(super) fn fringe(
        &self,
        p: &mut Parts,
        fringe: super::super::outfit::Fringe,
        build: f32,
    ) -> usize {
        let top = self.rings[24][0].y;
        let padding = 0.008 * build;
        let mut wall = Builder::default();
        self.emit(&mut wall, Vec3::ONE);
        fur_parts::rim_profile(
            p,
            fringe,
            Vec3::Y * top,
            Vec2::splat(self.authored_radius_top + padding),
            build,
            &|y, angle, extra| {
                let ray = Vec3::new(angle.sin(), 0.0, angle.cos());
                let surface = wall
                    .outer_surface(Vec3::Y * (top + y), ray)
                    .expect("native fur roots lie within shaft height");
                surface - Vec3::Y * top + ray * (padding + extra)
            },
        )
    }

    /// Expand only surrounding-cloth bounds for the actual emitted fur.
    /// The visible shaft and locks are never inflated to this circular cone.
    pub(super) fn fringe_envelope(
        &self,
        mut shape: BootShape,
        fringe: super::super::outfit::Fringe,
        build: f32,
    ) -> BootShape {
        let mut trim = Parts::default();
        self.fringe(&mut trim, fringe, build);
        for triangle in trim.matte.surface_triangles() {
            for p in triangle {
                if p.y >= shape.top {
                    shape.radius_top = shape
                        .radius_top
                        .max(p.distance(Vec3::Y * shape.top) + 0.002);
                } else if p.y <= shape.bottom {
                    shape.radius_bottom = shape
                        .radius_bottom
                        .max(p.distance(Vec3::Y * shape.bottom) + 0.002);
                } else {
                    let t = (p.y - shape.bottom) / (shape.top - shape.bottom);
                    shape.radius_top = shape
                        .radius_top
                        .max((p.xz().length() + 0.002 - shape.radius_bottom * (1.0 - t)) / t);
                }
            }
        }
        shape.sample_radius_top = shape
            .sample_radius_top
            .max(shape.radius_top + shape.cuff_thickness + 0.006);
        shape.sample_radius_bottom = shape.sample_radius_bottom.max(shape.radius_bottom + 0.012);
        shape
    }

    /// The band's emitted surface supplies the hardware seat. Reuse the
    /// same open frame as torso straps; native boots keep their brass color,
    /// two stations and side-facing rectangular silhouette.
    pub(super) fn buckled_band(&self, p: &mut Parts, y: f32, side: f32, color: Vec3) {
        let mut leather = p.satin.empty_like();
        self.band(&mut leather, y, 0.009, color);
        let normal = Vec3::X * side;
        let mount = leather.outer_surface(Vec3::new(0.0, y, 0.02), normal);
        p.satin.absorb(leather);
        if let Some(at) = mount {
            fitting::buckle_frame(
                &mut p.metal,
                at,
                Quat::from_rotation_y(side * PI * 0.5),
                Vec2::splat(0.014),
                Vec3::new(0.74, 0.5, 0.26),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn native_buckle_has_open_windows_on_its_own_band() {
        for model in [CreatureModel::Human, CreatureModel::Orc] {
            for woman in [false, true] {
                for build in 0..3 {
                    let look = Look::preview_adjusted(model, 22, |v| {
                        *v = Variant {
                            woman,
                            build,
                            bust: 2,
                            figure: 2,
                            ..default()
                        };
                    });
                    let body = HumanoidBody::resolve(&look);
                    let mut support = Builder::default();
                    trouser_leg(&mut support, body.shin, body.build, true, Vec3::ONE);
                    let shape = BootShape::fitted(body.build, if woman { -0.075 } else { -0.24 });
                    let shaft = NativeShaft::fitted(shape, &support, body.build);
                    for y in [-0.3, -0.37] {
                        for side in [-1.0, 1.0] {
                            let mut p = Parts::default();
                            shaft.buckled_band(&mut p, y, side, Vec3::ONE);
                            let mut band = Builder::default();
                            shaft.band(&mut band, y, 0.009, Vec3::ONE);
                            assert_eq!(
                                p.satin.surface_triangles().collect::<Vec<_>>(),
                                band.surface_triangles().collect::<Vec<_>>(),
                                "leather recipe retained"
                            );
                            let normal = Vec3::X * side;
                            let at = band.outer_surface(Vec3::new(0.0, y, 0.02), normal).unwrap();
                            for z in [-0.007, 0.007] {
                                let origin = at + normal * 0.1 + Vec3::new(0.0, -0.004, z);
                                assert!(
                                p.metal.outer_surface(origin, -normal).is_none(),
                                "buckle window must reveal leather: woman={woman} build={build}"
                            );
                                assert!(
                                    p.satin.outer_surface(origin, -normal).is_some(),
                                    "window shows band"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn constructed_boot_components_are_closed_outward_volumes() {
        for heeled in [false, true] {
            for build in [0.8, 1.0, 1.4] {
                for faces in components(BootLast { build, heeled }) {
                    if faces.is_empty() {
                        continue;
                    }
                    let mut edges = BTreeMap::new();
                    let mut volume = 0.0;
                    for p in faces {
                        assert!(p.iter().all(|v| v.is_finite()));
                        assert!((p[1] - p[0]).cross(p[2] - p[0]).length_squared() > 1e-18);
                        volume += p[0].dot(p[1].cross(p[2])) / 6.0;
                        let keys =
                            p.map(|v| v.to_array().map(|x| (x * 1_000_000.0).round() as i32));
                        for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                            let (key, sign) = if keys[a] < keys[b] {
                                ((keys[a], keys[b]), 1)
                            } else {
                                ((keys[b], keys[a]), -1)
                            };
                            let entry = edges.entry(key).or_insert((0, 0));
                            entry.0 += 1;
                            entry.1 += sign;
                        }
                    }
                    assert!(
                        edges.values().all(|v| *v == (2, 0)),
                        "closed material layer"
                    );
                    assert!(volume > 0.00001, "positive material volume {volume}");
                }
            }
        }
    }

    #[test]
    fn constructed_boot_surfaces_fit_the_same_contact_envelope_used_by_cloth() {
        for heeled in [false, true] {
            for build in [0.8, 1.0, 1.4] {
                let last = BootLast { build, heeled };
                let shapes = last.contacts();
                for faces in components(last) {
                    for p in faces {
                        assert!(shapes.iter().any(|(center,radii)|p.iter().all(|v|((*v-*center)/ *radii).length_squared()<=1.00001)),"one convex contact ellipsoid must contain each complete triangle: heeled {heeled} build {build} triangle {p:?}");
                        assert!(
                            p.iter().all(|v| v.y >= BootLast::FLOOR - 1e-6),
                            "nothing extends below the supporting sole"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn constructed_boot_heel_and_forefoot_share_the_support_plane() {
        for build in [0.8, 1.0, 1.4] {
            let [_, sole, heel] = components(BootLast {
                build,
                heeled: true,
            });
            let area = |faces: Vec<[Vec3; 3]>| {
                faces
                    .into_iter()
                    .filter(|p| p.iter().all(|v| (v.y - BootLast::FLOOR).abs() < 1e-6))
                    .map(|p| (p[1] - p[0]).cross(p[2] - p[0]).length() * 0.5)
                    .sum::<f32>()
            };
            assert!(
                area(heel) > 0.001 * build * build,
                "heel tip supports weight"
            );
            assert!(
                area(sole) > 0.003 * build * build,
                "forefoot has a substantial contact patch"
            );
        }
    }
    #[test]
    fn native_shaft_bounds_support_between_fit_rows_and_rays() {
        let shape = BootShape::fitted(1.0, -0.075);
        let angle = TAU / 64.0;
        let y = shape.bottom + (shape.top - shape.bottom) * 10.5 / 24.0;
        let radial = Vec3::new(angle.cos(), 0.0, angle.sin());
        let tangent = Vec3::new(-angle.sin(), 0.0, angle.cos());
        // Entire narrow triangle lies strictly between the former fitter's
        // ray columns and height rows, and protrudes 50 mm beyond the boot.
        let tri = [
            radial * 0.125 + Vec3::Y * y - tangent * 0.001,
            radial * 0.125 + Vec3::Y * (y + 0.001) + tangent * 0.001,
            radial * 0.127 + Vec3::Y * (y - 0.001) + tangent * 0.001,
        ];
        let mut support = Builder::default();
        support.tri_out(tri, Vec3::ZERO, Vec3::ONE);
        let shaft = NativeShaft::fitted(shape, &support, 1.0);
        let mut leather = Builder::default();
        shaft.emit(&mut leather, Vec3::ONE);
        for i in 0..=20 {
            for j in 0..=20 - i {
                let p = tri[0]
                    + (tri[1] - tri[0]) * i as f32 / 20.0
                    + (tri[2] - tri[0]) * j as f32 / 20.0;
                let origin = Vec3::Y * p.y;
                let ray = (p - origin).normalize();
                let wall = leather.outer_surface(origin, ray).unwrap();
                assert!(
                    (wall - p).dot(ray) >= 0.00599,
                    "between-cell support must retain radial clearance"
                );
            }
        }
        // Independent triangle/triangle crossing check: every edge of either
        // face is checked against the other's finite interior, not fitter cells.
        fn edge_hits(a: Vec3, b: Vec3, tri: [Vec3; 3]) -> bool {
            let ray = b - a;
            let e1 = tri[1] - tri[0];
            let e2 = tri[2] - tri[0];
            let h = ray.cross(e2);
            let det = e1.dot(h);
            if det.abs() < 1e-10 {
                return false;
            }
            let s = a - tri[0];
            let u = s.dot(h) / det;
            let q = s.cross(e1);
            let v = ray.dot(q) / det;
            let t = e2.dot(q) / det;
            (0.0..=1.0).contains(&u) && v >= 0.0 && u + v <= 1.0 && (0.0..=1.0).contains(&t)
        }
        for wall in leather.surface_triangles() {
            for i in 0..3 {
                assert!(!edge_hits(tri[i], tri[(i + 1) % 3], wall));
                assert!(!edge_hits(wall[i], wall[(i + 1) % 3], tri));
            }
        }
        // An opposite-side triangle-free sector retains authored radius.
        let opposite = shaft.radius(y, PI + angle);
        let t = (y - shape.bottom) / (shape.top - shape.bottom);
        let authored = shape.radius_bottom + (shape.radius_top - shape.radius_bottom) * t;
        assert!(
            (opposite - authored).abs() < 1e-6,
            "no global circular inflation"
        );
    }
    #[test]
    fn native_shaft_and_trim_fit_rendered_contact_capsules() {
        for woman in [false, true] {
            for build in 0..3 {
                let look = Look::preview_adjusted(CreatureModel::Human, 22, |v| {
                    *v = Variant {
                        woman,
                        build,
                        bust: 2,
                        figure: 2,
                        ..default()
                    };
                });
                let body = HumanoidBody::resolve(&look);
                let b = body.build;
                let mut support = Builder::default();
                trouser_leg(&mut support, body.shin, b, true, Vec3::ONE);
                let shape = BootShape::fitted(b, if woman { -0.075 } else { -0.24 });
                let shaft = NativeShaft::fitted(shape, &support, b);
                let envelope = shaft.envelope(shape);
                let mut render = Builder::default();
                shaft.emit(&mut render, Vec3::ONE);
                shaft.band(&mut render, shape.top, shape.cuff_thickness, Vec3::ONE);
                for y in [-0.3, -0.37] {
                    for side in [-1.0, 1.0] {
                        let mut parts = Parts::default();
                        shaft.buckled_band(&mut parts, y, side, Vec3::ONE);
                        render.absorb(parts.satin);
                        render.absorb(parts.metal);
                    }
                }
                // Match robe's three convex shaft capsules. Requiring all three
                // corners in one capsule proves the complete trim triangle is
                // contained, rather than merely checking isolated vertices.
                for tri in render.surface_triangles() {
                    assert!((0..3).any(|piece|{
                    let t=piece as f32/3.0;
                    let length=envelope.top-envelope.bottom;
                    let a=Vec3::Y*(envelope.top-length*t);
                    let z=Vec3::Y*(envelope.top-length*(piece+1) as f32/3.0);
                    let radius=envelope.radius_top+(envelope.radius_bottom-envelope.radius_top)*t;
                    tri.iter().all(|p|{
                        let u=((*p-a).dot(z-a)/(z-a).length_squared()).clamp(0.0,1.0);
                        p.distance(a.lerp(z,u))<=radius+1e-6
                    })
                }),"native trim triangle outside robe core proxies: woman={woman} build={build} triangle={tri:?}");
                }
            }
        }
    }
    #[test]
    fn barbarian_fur_undercoat_and_each_lock_root_follow_actual_shaft() {
        for woman in [false, true] {
            for build in 0..3 {
                let look = Look::preview_adjusted(CreatureModel::Human, 22, |v| {
                    *v = Variant {
                        woman,
                        build,
                        bust: 2,
                        figure: 2,
                        ..default()
                    };
                })
                .dressed(Wardrobe::Barbarian);
                let outfit = Outfit::worn(&look);
                let fringe = outfit.trimmings.boot_tops.expect("Barbarian boot fur");
                let body = HumanoidBody::resolve(&look);
                let b = body.build;
                let mut support = Builder::default();
                if matches!(outfit.legs, Legs::Trousers(_)) {
                    trouser_leg(&mut support, body.shin, b, true, Vec3::ONE);
                } else {
                    sculpted_limb(&mut support, body.shin, Vec3::ONE);
                }
                let shape = BootShape::fitted(b, -0.075);
                let shaft = NativeShaft::fitted(shape, &support, b);
                let mut leather = Builder::default();
                shaft.emit(&mut leather, Vec3::ONE);
                let mut fur = Parts::default();
                let undercoat_end = shaft.fringe(&mut fur, fringe, b);
                let triangles: Vec<_> = fur.matte.surface_triangles().collect();
                assert!(undercoat_end > 0 && undercoat_end < triangles.len());
                let mut undercoat = Builder::default();
                for &tri in &triangles[..undercoat_end] {
                    undercoat.tri_out(tri, Vec3::ZERO, Vec3::ONE);
                }
                for col in 0..192 {
                    let angle = (col as f32 + 0.31) * TAU / 192.0;
                    let ray = Vec3::new(angle.sin(), 0.0, angle.cos());
                    let origin = Vec3::Y * (shape.top - 0.012 * b);
                    let wall = leather.outer_surface(origin, ray).unwrap();
                    let inner = undercoat
                        .inner_surface(origin, ray)
                        .expect("continuous underfur, not sparse fur tips");
                    let gap = (inner - wall).dot(ray);
                    assert!(
                        (-0.004 * b..=0.003 * b).contains(&gap),
                        "underfur attachment woman={woman} build={build} gap={gap}"
                    );
                }
                // Each authored lock has five sides and four faces per side; its
                // first cap's first corner is the actual emitted root center.
                assert_eq!(triangles.len() - undercoat_end, 4 * 64 * 20);
                let mut own_height_variation = 0.0f32;
                for faces in triangles[undercoat_end..].as_chunks::<20>().0 {
                    let root = faces[0][0];
                    let ray = root.with_y(0.0).normalize();
                    let wall = leather.outer_surface(Vec3::Y * root.y, ray).unwrap();
                    let top = leather.outer_surface(Vec3::Y * shape.top, ray).unwrap();
                    own_height_variation = own_height_variation.max((wall - top).dot(ray).abs());
                    assert!(
                        ((root - wall).dot(ray) - 0.014 * b).abs() < 0.0001,
                        "lock root must follow its own height"
                    );
                }
                assert!(
                    own_height_variation > 0.001,
                    "fixture exercises a changing calf profile"
                );
                let envelope = shaft.fringe_envelope(shaft.envelope(shape), fringe, b);
                for tri in triangles {
                    assert!(
                        (0..3).any(|piece| {
                            let t = piece as f32 / 3.0;
                            let length = envelope.top - envelope.bottom;
                            let a = Vec3::Y * (envelope.top - length * t);
                            let z = Vec3::Y * (envelope.top - length * (piece + 1) as f32 / 3.0);
                            let radius = envelope.radius_top
                                + (envelope.radius_bottom - envelope.radius_top) * t;
                            tri.iter().all(|p| {
                                let u = ((*p - a).dot(z - a) / (z - a).length_squared())
                                    .clamp(0.0, 1.0);
                                p.distance(a.lerp(z, u)) <= radius + 1e-6
                            })
                        }),
                        "whole emitted fur triangle must fit one convex cloth-contact capsule"
                    );
                }
            }
        }
    }
}
