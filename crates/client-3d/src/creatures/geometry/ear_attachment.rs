//! Ear anatomy and jewelry mounts share one pre-presentation coordinate frame.
use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct EarAnatomy {
    /// Center of the right ear; the left ear mirrors it across the head center.
    pub center: Vec3,
    pub radii: Vec3,
}

impl Default for EarAnatomy {
    fn default() -> Self {
        Self {
            center: Vec3::new(0.098, 0.15, -0.005),
            radii: Vec3::new(0.014, 0.03, 0.022),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct EarAttachment {
    pub surface: Vec3,
    pub outward: Vec3,
}

impl EarAnatomy {
    pub fn center(self, side: f32) -> Vec3 {
        Vec3::new(side * self.center.x, self.center.y, self.center.z)
    }

    pub fn pointed_root(self, side: f32) -> Vec3 {
        self.center(side) - Vec3::X * side * self.radii.x * (0.005 / 0.014)
    }

    pub fn attachment(self, form: &crate::sdf::Sculpt, side: f32) -> Option<EarAttachment> {
        if !matches!(side, -1.0 | 1.0)
            || !self.center.is_finite()
            || !self.radii.is_finite()
            || self.radii.min_element() <= 0.0
        {
            return None;
        }
        let center = self.center(side);
        let origin = center + Vec3::new(0.0, -0.8 * self.radii.y, 0.35 * self.radii.z);
        let outward = Vec3::X * side;
        let surface = surface_ray_hit(form, origin, outward, 0.0, self.radii.x * 2.0)?;
        // The generic ray may cross an unrelated cheek or neck after missing the
        // ear. A mount must belong to this ear's local outward lobe region.
        let local = (surface - center) / self.radii;
        if local.x * side < 0.0
            || local.x.abs() > 1.4
            || local.y < -1.0
            || local.y > -0.5
            || local.z.abs() > 0.7
            || form.distance(surface).abs() > 0.0005
        {
            return None;
        }
        let normal = sculpt_surface_normal(form, surface, outward);
        if normal.dot(outward) <= 0.0 {
            return None;
        }
        Some(EarAttachment {
            surface,
            outward: normal,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lobe_frame_tracks_anatomy_and_rejects_unrelated_surface() {
        for scale in [0.7, 1.0, 1.4] {
            let anatomy = EarAnatomy {
                center: Vec3::new(0.098 * scale, 0.15 * scale + 0.04, -0.005 * scale - 0.03),
                radii: EarAnatomy::default().radii * scale,
            };
            let mut form = crate::sdf::Sculpt::default();
            for side in [-1.0, 1.0] {
                form.add(
                    crate::sdf::Shape::Ellipsoid(anatomy.center(side), anatomy.radii),
                    0.0,
                );
            }
            let a = anatomy.attachment(&form, 1.0).unwrap();
            let b = anatomy.attachment(&form, -1.0).unwrap();
            assert!(a.surface.is_finite() && a.outward.is_finite());
            assert!(form.distance(a.surface).abs() < 0.0005);
            assert!((a.surface.x + b.surface.x).abs() < 0.0005);
            assert!((a.surface.y - b.surface.y).abs() < 0.0005);
            assert!(a.outward.x > 0.0 && b.outward.x < 0.0);
            assert!((a.surface.y - (anatomy.center.y - 0.8 * anatomy.radii.y)).abs() < 0.00001);
            assert!(anatomy.attachment(&form, 0.0).is_none());
            let mut cheek = crate::sdf::Sculpt::default();
            cheek.add(
                crate::sdf::Shape::Ellipsoid(
                    Vec3::new(0.0, anatomy.center.y, anatomy.center.z),
                    Vec3::splat(0.03),
                ),
                0.0,
            );
            assert!(anatomy.attachment(&cheek, 1.0).is_none());
        }
    }
}
