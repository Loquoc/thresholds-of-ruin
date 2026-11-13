//! Shared, smooth compression of covered hair in the anatomical head frame.
//! Exposed locks and their authored lower endpoints are retained.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct HairEnvelope {
    crown: crown_parts::CrownProfile,
    lower_rim: f32,
    identity: &'static str,
}

impl HairEnvelope {
    pub fn for_look(look: &Look) -> Option<Self> {
        let (crown, lower_rim, identity) = match Outfit::worn(look).head {
            Headwear::Veiled { .. } => (
                crown_parts::CrownProfile::veiled(),
                crown_parts::VEILED_RIM_RADIUS,
                "veiled",
            ),
            Headwear::Tricorn { .. } => (crown_parts::CrownProfile::tricorn(), 0.006, "tricorn"),
            Headwear::WitchHat { .. } => {
                (crown_parts::CrownProfile::witch_hat(), 0.016, "witch-hat")
            }
            _ => return None,
        };
        Some(Self {
            crown,
            lower_rim,
            identity,
        })
    }

    pub fn transition(self) -> f32 {
        self.crown.opening - 0.04
    }

    pub fn fit(self, point: Vec3) -> Vec3 {
        let transition = self.transition();
        if point.y <= transition {
            return point;
        }
        // Reserve clearance to the actual faceted lining, not just its ideal
        // ellipsoid. The trim extends inside and below that lining: reserve its
        // inward reach and finish the blend at its lower edge. Compression
        // remains invertible, unlike surface projection.
        let center = self.crown.cranium.center;
        let rim = self.lower_rim;
        let clearance = (rim - self.crown.wall * 0.5).max(0.003) + 0.001;
        let radii = self.crown.cranium.radii + self.crown.ease - Vec3::splat(clearance);
        let mut fitted = point;
        let start = self.crown.opening + 0.025;
        let room = center.y + radii.y - start;
        if point.y > start {
            let over = point.y - start;
            fitted.y = start + room * over / (room + over);
        }
        let section = (1.0 - ((fitted.y - center.y) / radii.y).powi(2)).sqrt();
        let horizontal = Vec2::new(point.x - center.x, point.z - center.z);
        let radius = (horizontal / (Vec2::new(radii.x, radii.z) * section)).length();
        if radius > 0.85 {
            let over = radius - 0.85;
            let limited = 0.85 + 0.15 * over / (0.15 + over);
            let t =
                ((point.y - transition) / (self.crown.opening - rim - transition)).clamp(0.0, 1.0);
            let blend = t * t * (3.0 - 2.0 * t);
            let factor = 1.0 + blend * (limited / radius - 1.0);
            fitted.x = center.x + horizontal.x * factor;
            fitted.z = center.z + horizontal.y * factor;
        }
        fitted
    }
}

/// The same attachment map is used for mesh vertices and pinned hair controls.
pub(super) fn fit_point(look: &Look, point: Vec3) -> Vec3 {
    HairEnvelope::for_look(look).map_or(point, |profile| profile.fit(point))
}

pub(super) fn cache_key(look: &Look) -> String {
    let body = look.body_key.as_deref().unwrap_or(&look.key);
    match HairEnvelope::for_look(look) {
        Some(profile) => format!("{body}:hair-envelope:{}", profile.identity),
        None => body.to_owned(),
    }
}

/// Temporary source is always recipe-local. The destination owns the outer
/// head/drape transform, and no face, beard, accessory or earlier hair is moved.
pub(super) fn emit(p: &mut Parts, look: &Look, emit: impl FnOnce(&mut Parts)) {
    emit_with_refinement(p, look, false, emit);
}

/// Sparse species tails need vertices through the narrow crown transition;
/// fitting only their endpoints leaves long chords crossing the hat wall.
pub(super) fn emit_refined(p: &mut Parts, look: &Look, emit: impl FnOnce(&mut Parts)) {
    emit_with_refinement(p, look, true, emit);
}

fn emit_with_refinement(p: &mut Parts, look: &Look, refine: bool, emit: impl FnOnce(&mut Parts)) {
    if let Some(profile) = HairEnvelope::for_look(look) {
        let mut source = Parts::default();
        source.matte.res = p.matte.res;
        source.matte.sides_mul = p.matte.sides_mul;
        source.matte.smooth = p.matte.smooth;
        emit(&mut source);
        if refine {
            source.matte = source.matte.refine_for_fit(0.02, |tri| {
                // No upper cutoff: both sides of a shared covered edge
                // must use the same length target. Unselected edges lie
                // wholly below the transition, where fitting is identity.
                tri.iter().any(|p| p.y > profile.transition())
            });
        }
        p.matte
            .absorb_fitted(source.matte, |point| profile.fit(point));
    } else {
        emit(p);
    }
}
