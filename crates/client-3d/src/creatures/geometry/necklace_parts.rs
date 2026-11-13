//! Closed necklace paths anchored to the torso's upper-chest landmark.
//! Support and all mounts are resolved before emitting any ornament.
use super::*;

/// The necklace rides on the torso but crosses the actual head-joint neck.
/// Neither a torso-only proxy nor accessories/hair own this support.
pub(super) struct NecklaceSupport<'a> {
    pub torso: &'a crate::sdf::Sculpt,
    face: &'a crate::sdf::Sculpt,
    body: HumanoidBody,
    neck: NeckAttachment,
}

impl<'a> NecklaceSupport<'a> {
    pub fn new(
        body: HumanoidBody,
        torso: &'a crate::sdf::Sculpt,
        face: &'a crate::sdf::Sculpt,
    ) -> Self {
        Self {
            torso,
            face,
            body,
            neck: NeckAttachment::new(body.build, neck_slim(body.woman)),
        }
    }

    pub fn anchor(&self) -> Vec3 {
        let center = self.neck.section(0.0).0;
        (self.body.head + center * storybook_part(Plan::Biped, J::Head))
            / storybook_part(Plan::Biped, J::Torso)
    }

    pub fn distance(&self, at: Vec3) -> f32 {
        let torso_scale = storybook_part(Plan::Biped, J::Torso);
        let head_scale = storybook_part(Plan::Biped, J::Head);
        let head = (at * torso_scale - self.body.head) / head_scale;
        let (center, radii) = self.neck.section(head.y);
        let radial = (Vec2::new(head.x - center.x, head.z - center.z) / radii).length();
        let stations = neck_profile(self.body.build, neck_slim(self.body.woman));
        let neck = ((radial - 1.0) * radii.min_element())
            .max(stations[0].0.y - head.y)
            .max(head.y - stations[3].0.y);
        let scale = (head_scale / torso_scale).min_element();
        self.torso
            .distance(at)
            .min(self.face.distance(head).min(neck) * scale)
    }
}

#[derive(Clone, Copy)]
pub(super) struct NecklaceProfile {
    pub anchor: Vec3,
    pub front_drop: f32,
    pub clearance: f32,
    pub radius: f32,
    pub stations: usize,
    pub sides: usize,
}

pub(super) struct FittedNecklace {
    pub path: Vec<Vec3>,
    pub radius: f32,
    pub sides: usize,
}

impl NecklaceProfile {
    #[cfg(test)]
    pub fn resolve(self, support: &crate::sdf::Sculpt) -> Option<FittedNecklace> {
        self.resolve_field(&|at| support.distance(at))
    }

    pub fn resolve_field(self, support: &dyn Fn(Vec3) -> f32) -> Option<FittedNecklace> {
        if !self.anchor.is_finite()
            || !self.front_drop.is_finite()
            || !self.clearance.is_finite()
            || !self.radius.is_finite()
            || self.front_drop < 0.0
            || self.clearance < 0.0
            || self.radius <= 0.0
            || !(8..=192).contains(&self.stations)
            || !(3..=16).contains(&self.sides)
        {
            return None;
        }
        // Refinement is bounded and stays on the same support rays. Broad
        // collarbones may need shorter connecting chords than a narrow neck.
        // Never drop an otherwise supported necklace just because its first
        // discretization cuts a curved shoulder transition.
        let mut stations = self.stations;
        loop {
            if let Some(fitted) = self.resolve_at(support, stations) {
                return Some(fitted);
            }
            if stations >= 256 {
                return None;
            }
            stations = (stations * 2).min(256);
        }
    }

    fn resolve_at(self, support: &dyn Fn(Vec3) -> f32, stations: usize) -> Option<FittedNecklace> {
        let mut path = Vec::with_capacity(stations);
        for k in 0..stations {
            let angle = k as f32 * TAU / stations as f32;
            let origin = self.anchor - Vec3::Y * (self.front_drop * angle.cos().max(0.0));
            let outward = Vec3::new(angle.sin(), 0.0, angle.cos());
            let at = surface_field_ray_hit(
                support,
                origin,
                outward,
                self.clearance + self.radius,
                0.45,
            )?;
            // A hit on the opposite side is not support for this side of the loop.
            if !at.is_finite() || (at - origin).dot(outward) <= 0.0 {
                return None;
            }
            path.push(at);
        }
        // Bounded span witnesses prevent joining supported endpoints through
        // an unsupported sector or a nearby concave obstruction. This is not
        // a continuous collision certificate for arbitrary disconnected trim.
        for i in 0..path.len() {
            for t in [0.25, 0.5, 0.75] {
                let point = path[i].lerp(path[(i + 1) % path.len()], t);
                let distance = support(point);
                if !distance.is_finite()
                    || distance < self.radius + 0.001
                    || distance > self.radius + self.clearance + 0.003
                {
                    return None;
                }
            }
        }
        Some(FittedNecklace {
            path,
            radius: self.radius,
            sides: self.sides,
        })
    }
}

impl FittedNecklace {
    /// Mount on the actual polygonal centerline, not a second nominal ellipse.
    pub fn mount(&self, angle: f32) -> Vec3 {
        let along = angle.rem_euclid(TAU) / TAU * self.path.len() as f32;
        let i = along.floor() as usize % self.path.len();
        self.path[i].lerp(self.path[(i + 1) % self.path.len()], along.fract())
    }

    pub fn emit(&self, out: &mut Builder, color: Vec3) {
        let n = self.path.len();
        let mut rings = Vec::with_capacity(n);
        for i in 0..n {
            let tangent = (self.path[(i + 1) % n] - self.path[(i + n - 1) % n]).normalize();
            let across = tangent.cross(Vec3::Y).normalize();
            let up = tangent.cross(across);
            rings.push(
                (0..self.sides)
                    .map(|k| {
                        let a = k as f32 * TAU / self.sides as f32;
                        self.path[i] + self.radius * (across * a.cos() + up * a.sin())
                    })
                    .collect::<Vec<_>>(),
            );
        }
        for i in 0..n {
            let next = (i + 1) % n;
            for k in 0..self.sides {
                let j = (k + 1) % self.sides;
                let center = (self.path[i] + self.path[next]) * 0.5;
                for mut tri in [
                    [rings[i][k], rings[next][k], rings[next][j]],
                    [rings[i][k], rings[next][j], rings[i][j]],
                ] {
                    if (tri[1] - tri[0])
                        .cross(tri[2] - tri[0])
                        .dot((tri[0] + tri[1] + tri[2]) / 3.0 - center)
                        < 0.0
                    {
                        tri.swap(1, 2);
                    }
                    out.tri(tri, color);
                }
            }
        }
    }
}

pub(super) fn emit(
    p: &mut Parts,
    neck: super::super::outfit::Neck,
    core: super::super::anatomy::TorsoCoreProfile,
    composed: &NecklaceSupport<'_>,
) {
    let support = composed.torso;
    use super::super::outfit::Neck;
    let (front_drop, radius) = match neck {
        Neck::Pendant { .. } => (0.03, 0.0025),
        Neck::Teeth { .. } => (0.0, 0.005),
        _ => return,
    };
    let anchor = composed.anchor();
    let front_drop = anchor.y - (core.upper_chest_anchor().y - front_drop);
    let Some(fitted) = (NecklaceProfile {
        anchor,
        front_drop,
        radius,
        clearance: 0.004,
        stations: 64,
        sides: 6,
    })
    .resolve_field(&|at| composed.distance(at)) else {
        return;
    };
    match neck {
        Neck::Pendant { metal, gem } => {
            // Keep the gem's authored proportions while seating its flat back
            // in the actual sternum tangent plane. A world-upright setting can
            // clear the chest yet float above the sloping upper chest.
            let root = fitted.mount(0.0);
            let Some(surface) = torso_surface_hit(support, root.x, root.y - 0.015, true) else {
                return;
            };
            let normal = sculpt_surface_normal(support, surface, Vec3::Z);
            let across = Vec3::Y.cross(normal).normalize();
            let up = normal.cross(across);
            let radii = Vec3::new(0.015, 0.023, 0.006);
            let mut lift = 0.0f32;
            for row in -4i32..=4 {
                for col in -4i32..=4 {
                    let x = col as f32 / 4.0;
                    let y = row as f32 / 4.0;
                    let q = x * x + y * y;
                    if q > 1.0 {
                        continue;
                    }
                    let origin = surface + across * (x * radii.x) + up * (y * radii.y);
                    let Some(hit) = surface_ray_hit(support, origin, normal, 0.003, 0.08) else {
                        return;
                    };
                    if !hit.is_finite() {
                        return;
                    }
                    lift = lift.max((hit - origin).dot(normal) + radii.z * (1.0 - q).sqrt());
                }
            }
            let at = surface + normal * lift;
            let transform = |point: Vec3| at + across * point.x + up * point.y + normal * point.z;
            let mut setting = Builder::default();
            setting.res = p.metal.res;
            setting.blob(Vec3::ZERO, radii, 1, 0.0, metal);
            let mut stone = Builder::default();
            stone.res = p.satin.res;
            stone.blob(Vec3::Z * 0.005, Vec3::new(0.010, 0.017, 0.006), 1, 0.0, gem);
            fitted.emit(&mut p.metal, metal);
            p.metal
                .prism(root, at + up * 0.018, 0.0025, 0.0025, 6, metal, 0.0);
            for triangle in setting.surface_triangles() {
                p.metal.tri(triangle.map(transform), metal);
            }
            for triangle in stone.surface_triangles() {
                p.satin.tri(triangle.map(transform), gem);
            }
        }
        Neck::Teeth { cord } => {
            // Teeth stay individually tapered, with the central tooth longer.
            // Resolve their curved back along the chest instead of extending
            // a rigid cone through a rising collarbone or shoulder.
            let mut teeth = Vec::new();
            for k in -3i32..=3 {
                let angle = k as f32 * 0.32;
                let root = fitted.mount(angle);
                let length = if k == 0 { 0.05 } else { 0.036 };
                let mut path = vec![root];
                for (t, radius) in [(0.5, 0.005), (1.0, 0.0)] {
                    // A hanging tooth keeps its authored lateral splay. A
                    // radial projection would turn extra chest/armor depth
                    // into sideways drift, making outer teeth into hooks.
                    let origin = Vec3::new(
                        root.x + angle.sin() * 0.008 * t,
                        root.y - length * t,
                        anchor.z,
                    );
                    let Some(hit) = surface_field_ray_hit(
                        &|at| composed.distance(at),
                        origin,
                        Vec3::Z,
                        0.004 + radius,
                        0.45,
                    ) else {
                        return;
                    };
                    if !hit.is_finite() || hit.z <= origin.z {
                        return;
                    }
                    path.push(hit);
                }
                teeth.push((root, path));
            }
            fitted.emit(&mut p.satin, cord);
            for (root, path) in teeth {
                p.matte.tube(
                    &path,
                    &[Vec2::splat(0.007), Vec2::splat(0.004), Vec2::ZERO],
                    5,
                    &|_, _, _| BONE,
                );
                p.satin.blob(root, Vec3::splat(0.006), 1, 0.0, cord * 0.7);
            }
        }
        _ => {}
    }
}

// Opt-in test diagnostics retain the exact rejecting boundary without
// affecting production output or turning unsupported profiles into fallbacks.
fn note_collar_rejection(message: std::fmt::Arguments<'_>) {
    #[cfg(test)]
    if std::env::var_os("TOR_COLLAR_DIAGNOSTICS").is_some() {
        eprintln!("{message}");
    }
    let _ = message;
}

macro_rules! collar_rejected {
    ($($arg:tt)*) => { note_collar_rejection(format_args!($($arg)*)); };
}

/// A broad three-tier collar is one surface-following assembly, not three
/// independently seated necklaces. Each tier advances the same authored
/// distance along its radial support section; larger shoulders cannot pull
/// neighboring tiers apart.
#[derive(Clone, Copy)]
pub(super) struct CollarProfile {
    pub inner: NecklaceProfile,
    pub tier_spacing: f32,
}

pub(super) struct FittedCollar {
    pub tiers: Vec<FittedNecklace>,
    pub fringe: Vec<(Vec3, Vec3, Vec3)>,
}

impl CollarProfile {
    pub fn resolve(self, support: &dyn Fn(Vec3) -> f32) -> Option<FittedCollar> {
        if !self.tier_spacing.is_finite()
            || self.tier_spacing < 2.0 * self.inner.radius
            || self.tier_spacing > 0.05
        {
            return None;
        }
        // Validate the shared profile before attempting the coupled resolve.
        if self.inner.resolve_field(support).is_none() {
            collar_rejected!("collar rejected: initial inner profile");
            return None;
        }
        let mut stations = self.inner.stations;
        loop {
            if let Some(fitted) = self.resolve_at(support, stations) {
                return Some(fitted);
            }
            if stations >= 256 {
                return None;
            }
            stations = (stations * 2).min(256);
        }
    }

    /// Select the highest tested seat in the recipe's 12 mm lower window.
    /// Move the complete profile, including its front station, rather than
    /// shrinking individual tiers into a local inward neck taper.
    pub fn resolve_seated(self, support: &dyn Fn(Vec3) -> f32) -> Option<(FittedCollar, f32)> {
        for step in 0..=12 {
            let drop = step as f32 * 0.001;
            let profile = Self {
                inner: NecklaceProfile {
                    anchor: self.inner.anchor - Vec3::Y * drop,
                    ..self.inner
                },
                ..self
            };
            if let Some(collar) = profile.resolve(support) {
                return Some((collar, drop));
            }
        }
        None
    }

    fn resolve_at(self, support: &dyn Fn(Vec3) -> f32, stations: usize) -> Option<FittedCollar> {
        let Some(inner) = self.inner.resolve_at(support, stations) else {
            collar_rejected!("collar rejected: inner at {stations}");
            return None;
        };
        let mut tiers = vec![inner];
        for _ in 1..3 {
            let path: Option<Vec<_>> = tiers
                .last()?
                .path
                .iter()
                .enumerate()
                .map(|(i, &at)| {
                    let next = self.advance(
                        support,
                        at,
                        i as f32 * TAU / stations as f32,
                        self.tier_spacing,
                    );
                    if next.is_none() {
                        collar_rejected!(
                            "collar rejected: tier {} at {stations}, station {i}, from {at:?}",
                            tiers.len()
                        );
                    }
                    next
                })
                .collect();
            let path = path?;
            // Same bounded connecting-span contract as the shared necklace.
            // Keep rejection atomic across all three tiers and their fringe.
            for i in 0..path.len() {
                for t in [0.0, 0.25, 0.5, 0.75] {
                    let distance = support(path[i].lerp(path[(i + 1) % path.len()], t));
                    if !distance.is_finite()
                        || distance < self.inner.radius + 0.001
                        || distance > self.inner.radius + self.inner.clearance + 0.003
                    {
                        collar_rejected!("collar rejected: tier span {} at {stations}, station {i}, t={t}, gap={distance}", tiers.len());
                        return None;
                    }
                }
            }
            tiers.push(FittedNecklace {
                path,
                radius: self.inner.radius,
                sides: self.inner.sides,
            });
        }
        let mut fringe = Vec::new();
        for k in -6i32..=6 {
            let angle = k as f32 * 0.2;
            let root = tiers.last()?.mount(angle);
            let Some(center) = self.advance(support, root, angle, self.inner.radius + 0.010) else {
                collar_rejected!("collar rejected: fringe {k} at {stations}, root {root:?}");
                return None;
            };
            let tangent = (tiers.last()?.mount(angle + 0.002) - tiers.last()?.mount(angle - 0.002))
                .normalize();
            fringe.push((root, center, tangent));
        }
        Some(FittedCollar { tiers, fringe })
    }

    /// Keep the authored chord length while fitting the endpoint in the same
    /// radial/Y section. From the downward recipe direction, select the first
    /// inside-to-outside crossing over the shoulder rather than jumping to its
    /// outermost horizontal-ray silhouette. This permits upward tier travel.
    fn advance(
        self,
        support: &dyn Fn(Vec3) -> f32,
        from: Vec3,
        angle: f32,
        distance: f32,
    ) -> Option<Vec3> {
        let outward = Vec3::new(angle.sin(), 0.0, angle.cos());
        let target = self.inner.clearance + self.inner.radius;
        let point = |turn: f32| from + (outward * turn.sin() - Vec3::Y * turn.cos()) * distance;
        let value = |turn: f32| {
            let p = point(turn);
            let d = support(p) - target;
            (p.is_finite() && d.is_finite() && (p - self.inner.anchor).dot(outward) > 0.0)
                .then_some(d)
        };
        // Follow a bounded route through this support envelope before selecting
        // its exit. Do not skip an unsupported gap to land on a detached lobe.
        let steps = (distance / 0.001).ceil() as usize;
        for step in 0..=steps {
            let p = from - Vec3::Y * (distance * step as f32 / steps as f32);
            let d = support(p) - target;
            if !d.is_finite() || d > 0.0003 {
                collar_rejected!("collar rejected: outward section route gap {d}, from {from:?}");
                return None;
            }
        }
        let start = value(0.0)?;
        // Match the shared surface-ray convergence contract (0.2 mm).
        if start.abs() <= 0.0002 {
            return Some(point(0.0));
        }
        if start > 0.0 {
            return None;
        }
        let mut low = 0.0;
        // A broad collar fans outward. Bound the upward branch so a solver
        // cannot select the backward, straight-up intersection of the neck.
        for step in 1..=96 {
            let mut high = PI * 0.75 * step as f32 / 96.0;
            let d = value(high)?;
            if d >= 0.0 {
                for _ in 0..18 {
                    let middle = (low + high) * 0.5;
                    if value(middle)? < 0.0 {
                        low = middle;
                    } else {
                        high = middle;
                    }
                }
                let result = point((low + high) * 0.5);
                return ((support(result) - target).abs() <= 0.0003
                    && (result.distance(from) - distance).abs() <= 0.0003)
                    .then_some(result);
            }
            low = high;
        }
        None
    }
}

impl FittedCollar {
    pub fn emit(&self, p: &mut Parts, metal: Vec3, beads: Vec3) {
        for (i, tier) in self.tiers.iter().enumerate() {
            tier.emit(&mut p.metal, if i == 1 { beads } else { metal });
        }
        for &(root, center, tangent) in &self.fringe {
            let down = (center - root).normalize();
            let normal = tangent.cross(down).normalize();
            let across = down.cross(normal);
            let mut bead = Builder::default();
            bead.res = p.satin.res;
            bead.blob(Vec3::ZERO, Vec3::new(0.008, 0.012, 0.008), 1, 0.0, beads);
            for triangle in bead.surface_triangles() {
                p.satin.tri(
                    triangle.map(|v| center + across * v.x + down * v.y + normal * v.z),
                    beads,
                );
            }
        }
    }
}

pub(super) fn emit_collar(
    p: &mut Parts,
    core: super::super::anatomy::TorsoCoreProfile,
    support: &NecklaceSupport<'_>,
    metal: Vec3,
    beads: Vec3,
) {
    let anchor = support.anchor() + Vec3::Y * 0.010;
    let profile = CollarProfile {
        inner: NecklaceProfile {
            anchor,
            front_drop: (anchor.y - (core.upper_chest_anchor().y + 0.030)).max(0.0),
            clearance: 0.004,
            radius: 0.011,
            stations: 64,
            sides: 6,
        },
        tier_spacing: 0.024,
    };
    if let Some((collar, _)) = profile.resolve_seated(&|at| support.distance(at)) {
        collar.emit(p, metal, beads);
    }
}
