//! Authored feather components reused by trailing plumes, cream fans and coverts.
//! Helmet assembly owns placement/counts; this module owns vane construction.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FeatherStyle {
    TrailingRed,
    CurledCream,
}

// Shared sampling topology, not an independent artistic shape parameter.
const FEATHER_INTERVALS: usize = 16;
pub(super) const FEATHER_STATIONS: usize = FEATHER_INTERVALS + 1;
pub(super) type FeatherPath = [Vec3; FEATHER_STATIONS];
pub(super) fn feather_fraction(station: usize) -> f32 {
    station as f32 / FEATHER_INTERVALS as f32
}

pub(super) fn plume_path(lane: f32, layer: f32, style: FeatherStyle) -> FeatherPath {
    // Different lengths and asymmetric arches prevent adjacent feather tips
    // from joining into the straight edge of a rigid folding fan.
    let stagger = (lane * 5.7 + layer * 2.3).sin();
    let arch = 0.17 + layer * 0.035 - 0.05 * lane.abs() + stagger * 0.018;
    // Keep this iterator/Vec evaluation path: direct array generation produced
    // one-bit coordinate differences in optimized builds (guarded by oracle).
    (0..=16)
        .map(|i| {
            let t = i as f32 / 16.0;
            if style == FeatherStyle::CurledCream {
                // Short cream feathers curl outward over the mount, rather
                // than reproducing the red plume as a tall pale folding fan.
                // Leave headroom for outer-lane vane width, which tilts upward
                // by about 0.02 above its centerline near the shoulder.
                let angle = lane * 1.30;
                let radius = 0.015 + 0.155 * (t * PI * 0.5).sin();
                return Vec3::new(
                    angle.sin() * radius,
                    0.375 + (0.08 + stagger * 0.008) * (t * PI * 0.85).sin() - 0.075 * t,
                    angle.cos() * radius + 0.045 * (1.0 - t),
                );
            }
            // Outer/foreground feathers bend early; the central rear sweep
            // bends later. Height variation alone left all broad vanes folded
            // at the same station, creating the straight cut-paper fan edge.
            let bend = t.powf(0.75 + 0.90 * (1.0 - lane.abs()) + 0.20 * (layer - 1.0));
            Vec3::new(
                lane * (0.02 + 0.105 * (bend * PI * 0.65).sin() + 0.05 * bend.powi(3))
                    + stagger * 0.015 * t * t,
                0.365 + arch * (bend * PI * 0.95).sin() - t * t * (0.035 + lane.abs() * 0.025),
                0.025 - t * (0.235 + layer * 0.035 + stagger * 0.028),
            )
        })
        .collect::<Vec<_>>()
        .try_into()
        .expect("fixed feather station inventory")
}

// Explicit vanes keep a visible center ridge and pointed, asymmetric barbs.
// A flattened tube rotates its frame around the bend and reads as folded ribbon.
pub(super) fn feather_section(
    path: &[Vec3],
    i: usize,
    lane: f32,
    style: FeatherStyle,
) -> [Vec3; 3] {
    let t = i as f32 / (path.len() - 1) as f32;
    let tangent = (path[(i + 1).min(path.len() - 1)] - path[i.saturating_sub(1)]).normalize();
    let across = (Vec3::X - tangent * tangent.x).normalize();
    // Fan the outer red vanes in three dimensions instead of aligning every
    // width with X. The smooth roll vanishes at root and tip, retaining the
    // mount and pointed silhouette while exposing actual area from the side.
    let across = if style == FeatherStyle::CurledCream {
        across
    } else {
        Quat::from_axis_angle(tangent, 0.50 * lane * (PI * t).sin()) * across
    };
    // The cream fan curls forward, while the red plume trails backward.
    // Reverse the red frame so its ridge rises on the exposed upper side
    // instead of carving a trough below the vane's shoulder centerline.
    let normal = tangent.cross(across).normalize()
        * if style == FeatherStyle::CurledCream {
            1.0
        } else {
            -1.0
        };
    let width = (PI * t.powf(0.72)).sin().max(0.0)
        * if style == FeatherStyle::CurledCream {
            0.022
        } else {
            0.029
        };
    let left = width * if i % 4 == 1 { 0.72 } else { 1.0 };
    let right = width * if i % 4 == 3 { 0.74 } else { 0.94 };
    let lean = lane * 0.002 * (PI * t).sin();
    [
        path[i] - across * left.max(0.0002),
        path[i] + normal * (width * 0.13 + 0.0005) + across * lean,
        path[i] + across * right.max(0.0002),
    ]
}

pub(super) fn plume_feather(b: &mut Builder, lane: f32, layer: f32, style: FeatherStyle) {
    let mut path = plume_path(lane, layer, style);
    // Cluster roots around the mount rather than gathering every vane into
    // one fan apex. Cream feathers sit as shorter overlapping shoulder tufts.
    for (i, point) in path.iter_mut().enumerate() {
        let t = feather_fraction(i);
        point.x += (layer - 1.0) * 0.009 * (1.0 - t);
        point.z += (lane * 9.0 + layer).sin() * 0.012 * (1.0 - t);
    }
    emit_feather_vane(b, &path, lane, layer, style);
}

pub(super) fn emit_feather_vane(
    b: &mut Builder,
    path: &FeatherPath,
    lane: f32,
    layer: f32,
    style: FeatherStyle,
) {
    let pigment = (if style == FeatherStyle::CurledCream {
        CREAM
    } else {
        RED
    }) * (0.90 + 0.10 * (lane * 7.0 + layer).cos());
    // Adjacent panels and the ridge own the same section coordinates. Resolve
    // each station once, retaining only the current pair and the ridge points.
    let mut a = feather_section(path, 0, lane, style);
    let mut ridge = [Vec3::ZERO; FEATHER_STATIONS];
    ridge[0] = a[1];
    for i in 0..path.len() - 1 {
        let z = feather_section(path, i + 1, lane, style);
        let normal = (a[2] - a[0]).cross(z[1] - a[1]).normalize();
        for half in 0..2 {
            let q = [a[half], z[half], z[half + 1], a[half + 1]];
            let center = (q[0] + q[2]) * 0.5;
            b.quad_out(
                q,
                center - normal,
                pigment * if half == 0 { 0.92 } else { 1.06 },
            );
            // A separate back surface makes each vane visible from behind.
            b.quad_out(
                q.map(|v| v - normal * 0.0006),
                center + normal,
                pigment * 0.78,
            );
        }
        ridge[i + 1] = z[1];
        a = z;
    }
    let radii: [_; FEATHER_STATIONS] = std::array::from_fn(|i| {
        let r = 0.0012 * (1.0 - i as f32 / (path.len() - 1) as f32) + 0.0002;
        (r, r)
    });
    tube(b, &ridge, &radii, pigment * 0.72);
}

// Short overlapping coverts hide the exposed roots between the long cream
// feathers. Their staggered curved tips stop above the outer collar's points.
pub(super) fn cream_covert_path(lane: f32) -> FeatherPath {
    let root = Vec3::new(lane * 0.012, 0.368, 0.055);
    let shoulder = Vec3::new(lane * 0.045, 0.418, 0.090);
    let bend = Vec3::new(lane * 0.095, 0.411, 0.125 - lane.abs() * 0.035);
    let tip = Vec3::new(
        lane * 0.120,
        0.363 + 0.006 * (lane * 7.0).cos(),
        0.145 - lane.abs() * 0.055,
    );
    // Keep this iterator/Vec evaluation path: direct array generation produced
    // one-bit coordinate differences in optimized builds (guarded by oracle).
    (0..=16)
        .map(|i| {
            let t = i as f32 / 16.0;
            let u = 1.0 - t;
            root * u.powi(3)
                + shoulder * (3.0 * u * u * t)
                + bend * (3.0 * u * t * t)
                + tip * t.powi(3)
        })
        .collect::<Vec<_>>()
        .try_into()
        .expect("fixed feather station inventory")
}

pub(super) fn cream_coverts(b: &mut Builder) {
    for i in 0..9 {
        let lane = i as f32 / 4.0 - 1.0;
        emit_feather_vane(
            b,
            &cream_covert_path(lane),
            lane,
            0.25,
            FeatherStyle::CurledCream,
        );
    }
}
