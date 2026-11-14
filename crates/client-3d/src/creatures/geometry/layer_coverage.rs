//! Conservative whole-detail suppression inside actual containing-solid armor.
//! Not a thin-cloth or generic directional occlusion mask. Body/support fields
//! are not occluders. Evaluated only during mesh construction; no speedup claimed.
use super::*;

/// A caller-owned projection frame and explicit bounded search policy.
/// Failure to prove coverage (including exhausted budget) always keeps the detail.
pub(super) struct WholeDetailCoverage {
    pub origin: Vec3,
    pub across: Vec3,
    pub up: Vec3,
    pub outward: Vec3,
    pub margin: f32,
    pub reach: f32,
    pub budget: usize,
}

impl WholeDetailCoverage {
    pub fn front() -> Self {
        Self {
            origin: Vec3::ZERO,
            across: Vec3::X,
            up: Vec3::Y,
            outward: Vec3::Z,
            margin: 0.002,
            reach: 0.12,
            budget: 2048,
        }
    }

    /// The supplied points must bound every piece of the detail, including relief.
    /// Certifies a covering plane patch in front of each projected rectangle tile;
    /// interval upper bounds, not isolated field samples, protect small holes.
    pub fn hides(&self, cover: &crate::sdf::Sculpt, points: &[Vec3]) -> bool {
        self.hides_components(cover, [points])
    }

    /// All component bounds must certify within one shared budget; there is no
    /// selective component removal. Components bound actual emitted surfaces,
    /// avoiding empty corners of a curved detail's loose global box.
    pub fn hides_components<'a>(
        &self,
        cover: &crate::sdf::Sculpt,
        components: impl IntoIterator<Item = &'a [Vec3]>,
    ) -> bool {
        if !self.origin.is_finite()
            || !self.margin.is_finite()
            || self.margin <= 0.0
            || !self.reach.is_finite()
            || self.reach <= self.margin
            || self.budget == 0
        {
            return false;
        }
        let axes = [self.across, self.up, self.outward];
        if axes
            .iter()
            .any(|a| !a.is_finite() || a.length_squared() < 1e-12)
        {
            return false;
        }
        let [x, y, z] = axes.map(|a| a.normalize());
        if x.dot(y).abs() > 1e-5 || x.dot(z).abs() > 1e-5 || y.dot(z).abs() > 1e-5 {
            return false;
        }
        let mut remaining = self.budget;
        let mut any = false;
        for points in components {
            if points.is_empty() {
                return false;
            }
            any = true;
            let mut low = Vec3::splat(f32::INFINITY);
            let mut high = Vec3::splat(f32::NEG_INFINITY);
            for p in points {
                if !p.is_finite() {
                    return false;
                }
                let p = *p - self.origin;
                let q = Vec3::new(p.dot(x), p.dot(y), p.dot(z));
                low = low.min(q);
                high = high.max(q);
            }
            // A forward patch alone could hide a detail visible from the side.
            // Require the full detail volume to be enclosed by this actual armor.
            let mut volumes = vec![(low, high, 0u8)];
            while let Some((lo, hi, level)) = volumes.pop() {
                if remaining == 0 {
                    return false;
                }
                remaining -= 1;
                let mid = (lo + hi) * 0.5;
                let center = self.origin + x * mid.x + y * mid.y + z * mid.z;
                if cover.field_bounds(center, (hi - lo).length() * 0.5).1 < -1e-6 {
                    continue;
                }
                if level == 12 {
                    return false;
                }
                let size = hi - lo;
                let axis = if size.x >= size.y && size.x >= size.z {
                    0
                } else if size.y >= size.z {
                    1
                } else {
                    2
                };
                let mut a = hi;
                a[axis] = mid[axis];
                let mut b = lo;
                b[axis] = mid[axis];
                volumes.push((lo, a, level + 1));
                volumes.push((b, hi, level + 1));
            }
            let mut pending = vec![(
                low.truncate() - Vec2::splat(self.margin),
                high.truncate() + Vec2::splat(self.margin),
                0u8,
            )];
            while let Some((lo, hi, level)) = pending.pop() {
                if remaining == 0 {
                    return false;
                }
                remaining -= 1;
                let mid = (lo + hi) * 0.5;
                let radius = (hi - lo).length() * 0.5;
                let covered = (0..16).any(|i| {
                    let depth = high.z + self.margin + (self.reach - self.margin) * i as f32 / 15.0;
                    let center = self.origin + x * mid.x + y * mid.y + z * depth;
                    cover.field_bounds(center, radius).1 < -1e-6
                });
                if covered {
                    continue;
                }
                if level == 12 {
                    return false;
                }
                if hi.x - lo.x >= hi.y - lo.y {
                    pending.push((lo, Vec2::new(mid.x, hi.y), level + 1));
                    pending.push((Vec2::new(mid.x, lo.y), hi, level + 1));
                } else {
                    pending.push((lo, Vec2::new(hi.x, mid.y), level + 1));
                    pending.push((Vec2::new(lo.x, mid.y), hi, level + 1));
                }
            }
        }
        any
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdf::{Sculpt, Shape};
    fn slab() -> Sculpt {
        let mut s = Sculpt::default();
        s.add(
            Shape::Ellipsoid(Vec3::new(0.0, 0.0, -0.2), Vec3::splat(0.5)),
            0.0,
        );
        s
    }
    fn detail() -> [Vec3; 2] {
        [Vec3::new(-0.02, -0.02, 0.0), Vec3::new(0.02, 0.02, 0.01)]
    }
    #[test]
    fn certified_components_preserve_curves_and_share_budget() {
        let mask = WholeDetailCoverage::front();
        let solid = slab();
        let a = [Vec3::new(-0.4, 0.0, -0.2)];
        let b = [Vec3::new(0.0, 0.4, -0.2)];
        assert!(
            !mask.hides(&solid, &[a[0], b[0]]),
            "empty global box corner is outside sphere"
        );
        assert!(mask.hides_components(&solid, [a.as_slice(), b.as_slice()]));
        let mut budget = WholeDetailCoverage::front();
        budget.budget = 2;
        assert!(budget.hides(&solid, &a));
        assert!(budget.hides(&solid, &b));
        assert!(
            !budget.hides_components(&solid, [a.as_slice(), b.as_slice()]),
            "one shared budget, not per-component reset"
        );
        let mut hole = solid.clone();
        hole.carve(
            Shape::Capsule(a[0] - Vec3::Z, a[0] + Vec3::Z, 0.0001, 0.0001),
            0.0,
        );
        assert!(
            !mask.hides_components(&hole, [a.as_slice(), b.as_slice()]),
            "one affected component retains whole detail"
        );
    }

    #[test]
    fn certified_cover_handles_holes_partial_depth_and_budget() {
        let mask = WholeDetailCoverage::front();
        let solid = slab();
        assert!(mask.hides(&solid, &detail()));
        let mut hole = solid.clone();
        hole.carve(
            Shape::Capsule(
                Vec3::new(0.003, 0.007, -1.0),
                Vec3::new(0.003, 0.007, 1.0),
                0.0001,
                0.0001,
            ),
            0.0,
        );
        assert!(
            !mask.hides(&hole, &detail()),
            "off-grid tiny hole must retain whole detail"
        );
        let mut half = solid.clone();
        half.carve(Shape::Beyond(Vec3::X, 0.0), 0.0);
        assert!(!mask.hides(&half, &detail()));
        let mut other = solid.clone();
        other.carve(Shape::Beyond(Vec3::NEG_X, -0.001), 0.0);
        half.carve(Shape::Beyond(Vec3::X, -0.001), 0.0);
        half.union(other, 0.0);
        assert!(
            !mask.hides(&half, &detail()),
            "disjoint covers leave a visible gap"
        );
        let mut budget = WholeDetailCoverage::front();
        budget.budget = 1;
        assert!(
            !budget.hides(&hole, &detail()),
            "exhaustion during subdivision retains"
        );
        let mut detached = solid.clone();
        detached.carve(Shape::Beyond(Vec3::NEG_Z, -0.05), 0.0);
        assert!(
            !mask.hides(&detached, &detail()),
            "front slab cannot hide side-visible detail"
        );
        assert!(!mask.hides(&solid, &[Vec3::ZERO, Vec3::new(0.0, 0.0, 0.4)]));
        let mut limited = WholeDetailCoverage::front();
        limited.budget = 0;
        assert!(!limited.hides(&solid, &detail()));
        assert!(!mask.hides(&Sculpt::default(), &detail()));
        assert!(mask.hides(&solid, &[Vec3::ZERO]));
    }
    #[test]
    fn certified_cover_validates_frame_and_explicit_margin() {
        let mut mask = WholeDetailCoverage::front();
        let solid = slab();
        mask.across *= 3.0;
        assert!(mask.hides(&solid, &detail()));
        mask.up = mask.across;
        assert!(!mask.hides(&solid, &detail()));
        mask.up = Vec3::Y;
        mask.margin = f32::NAN;
        assert!(!mask.hides(&solid, &detail()));
        for invalid in [0.0, -0.001] {
            mask.margin = invalid;
            assert!(!mask.hides(&solid, &detail()));
        }
        mask.margin = 0.002;
        assert!(!mask.hides(&solid, &[Vec3::splat(f32::NAN)]));
        mask.margin = 1.0;
        assert!(!mask.hides(&solid, &detail()));
        let rotation = Quat::from_rotation_y(0.7);
        let offset = Vec3::new(1.0, 2.0, 3.0);
        let mut shifted = Sculpt::default();
        shifted.add(
            Shape::Ellipsoid(
                offset + rotation * Vec3::new(0.0, 0.0, -0.2),
                Vec3::splat(0.5),
            ),
            0.0,
        );
        let mask = WholeDetailCoverage {
            origin: offset,
            across: rotation * Vec3::X,
            up: rotation * Vec3::Y,
            outward: rotation * Vec3::Z,
            ..WholeDetailCoverage::front()
        };
        assert!(mask.hides(&shifted, &detail().map(|p| offset + rotation * p)));
    }
}
