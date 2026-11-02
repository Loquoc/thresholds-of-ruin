//! Persistent public appearance finishes. Indices follow the authored appearance
//! pools, never effect/identity IDs, item names, curses or identification state.
use super::*;
use tor_client_3d_model::ItemSurface;

// amber, smoky, violet, azure, crimson, silver, pearl, teal, ochre,
// indigo, copper, chartreuse, saffron, cerulean.
const POTIONS: [[f32; 3]; 14] = [
    [0.69, 0.34, 0.07],
    [0.28, 0.30, 0.28],
    [0.50, 0.22, 0.64],
    [0.11, 0.43, 0.79],
    [0.69, 0.09, 0.14],
    [0.62, 0.67, 0.71],
    [0.82, 0.78, 0.67],
    [0.08, 0.52, 0.46],
    [0.62, 0.43, 0.10],
    [0.23, 0.20, 0.51],
    [0.74, 0.36, 0.16],
    [0.57, 0.72, 0.10],
    [0.88, 0.61, 0.08],
    [0.08, 0.56, 0.72],
];
// aluminum, bronze, copper, electrum, gold, iron, lead, magnesium,
// nickel, platinum, silver, steel, tin, zinc.
const METALS: [[f32; 3]; 14] = [
    [0.79, 0.82, 0.84],
    [0.60, 0.36, 0.15],
    [0.76, 0.35, 0.16],
    [0.78, 0.68, 0.40],
    [0.88, 0.64, 0.19],
    [0.30, 0.32, 0.35],
    [0.36, 0.38, 0.44],
    [0.73, 0.76, 0.73],
    [0.60, 0.64, 0.61],
    [0.87, 0.85, 0.77],
    [0.90, 0.93, 0.96],
    [0.48, 0.58, 0.66],
    [0.68, 0.71, 0.78],
    [0.48, 0.52, 0.58],
];
// ash, birch, cedar, cherry, elm, hazel, mahogany, maple, oak, pine,
// teak, walnut, willow, yew.
const WOODS: [[f32; 3]; 14] = [
    [0.60, 0.47, 0.29],
    [0.82, 0.78, 0.63],
    [0.53, 0.25, 0.13],
    [0.57, 0.24, 0.19],
    [0.46, 0.34, 0.19],
    [0.57, 0.48, 0.25],
    [0.38, 0.14, 0.10],
    [0.76, 0.57, 0.32],
    [0.54, 0.37, 0.16],
    [0.76, 0.65, 0.40],
    [0.43, 0.30, 0.14],
    [0.30, 0.18, 0.10],
    [0.51, 0.55, 0.35],
    [0.65, 0.35, 0.14],
];
// agate, bronze, coral, diamond, emerald, ivory, jade, opal, ruby,
// sapphire, topaz, amethyst, garnet, quartz.
const RINGS: [[f32; 3]; 14] = [
    [0.55, 0.37, 0.22],
    [0.70, 0.45, 0.22],
    [0.85, 0.34, 0.25],
    [0.80, 0.88, 0.91],
    [0.10, 0.54, 0.29],
    [0.87, 0.81, 0.62],
    [0.35, 0.62, 0.40],
    [0.72, 0.81, 0.76],
    [0.72, 0.09, 0.18],
    [0.12, 0.29, 0.71],
    [0.88, 0.59, 0.16],
    [0.58, 0.29, 0.72],
    [0.39, 0.09, 0.14],
    [0.76, 0.66, 0.72],
];

fn tint(builder: &mut Builder, colour: Vec3, reference: f32) {
    for c in &mut builder.colors {
        let shade = c[0].max(c[1]).max(c[2]) / reference;
        let rgb = (colour * shade).clamp(Vec3::ZERO, Vec3::ONE);
        *c = [rgb.x, rgb.y, rgb.z, c[3]];
    }
}

fn replace_pigment(builder: &mut Builder, old: Vec3, colour: Vec3) {
    for c in &mut builder.colors {
        if Vec3::new(c[0], c[1], c[2]).distance(old) < 0.001 {
            *c = [colour.x, colour.y, colour.z, c[3]];
        }
    }
}

/// Invalid/mismatched descriptors deliberately leave the neutral recipe alone.
/// Neither caller nor cache needs an unbounded string or an identity-based seed.
pub(super) fn apply(parts: &mut Parts, kind: Kind, surface: ItemSurface) {
    match (kind, surface) {
        (Kind::Potion, ItemSurface::Potion(i)) => {
            let Some(rgb) = POTIONS.get(usize::from(i)) else {
                return;
            };
            tint(&mut parts.satin, Vec3::from_array(*rgb), 0.49);
        }
        (Kind::Wand, ItemSurface::Wand(i)) => {
            let Some(rgb) = METALS.get(usize::from(i)) else {
                return;
            };
            let metal = Vec3::from_array(*rgb);
            // Thin reflective skin over the exposed shaft; wood/leather grip
            // and ivory end remain physical construction, not magical effects.
            parts.metal.prism(
                Vec3::new(-0.035, 0.028, 0.0),
                Vec3::new(0.18, 0.028, 0.0),
                0.025,
                0.021,
                10,
                metal,
                0.0,
            );
            for k in 0..=i % 3 {
                parts.metal.torus(
                    Vec3::new(0.025 + f32::from(k) * 0.045, 0.028, 0.0),
                    Vec3::X,
                    0.025,
                    0.0035,
                    10,
                    4,
                    metal * 0.62,
                );
            }
        }
        (Kind::Staff, ItemSurface::Staff(i)) => {
            let Some(rgb) = WOODS.get(usize::from(i)) else {
                return;
            };
            let wood = Vec3::from_array(*rgb);
            replace_pigment(&mut parts.matte, WOOD * 1.12, wood);
            // Fine grain only on exposed wood, avoiding the leather grip.
            for z in [-0.012, 0.012] {
                parts.matte.prism(
                    Vec3::new(0.08, 0.061, z),
                    Vec3::new(0.36, 0.063, z),
                    0.0018,
                    0.0012,
                    4,
                    wood * 0.58,
                    0.0,
                );
            }
            if i == 1 {
                // birch's naturally pale bark and dark horizontal scars
                for x in [-0.52, -0.38, 0.30] {
                    parts.matte.cuboid(
                        Vec3::new(x, 0.060, 0.0),
                        Vec3::new(0.012, 0.0015, 0.016),
                        Quat::from_rotation_y(0.15),
                        Vec3::new(0.20, 0.19, 0.16),
                    );
                }
            }
        }
        (Kind::Ring, ItemSurface::Ring(i)) => {
            let Some(rgb) = RINGS.get(usize::from(i)) else {
                return;
            };
            let colour = Vec3::from_array(*rgb);
            tint(&mut parts.metal, colour, 0.95);
            if i != 1 {
                // Stone/ivory bands use the existing nonmetal satin material.
                // The neutral ring has no other satin part to discard.
                debug_assert!(parts.satin.positions.is_empty());
                parts.satin = std::mem::take(&mut parts.metal);
                let at = Vec3::new(0.086, 0.093, 0.0);
                parts
                    .satin
                    .prism(at, at + Vec3::Y * 0.027, 0.031, 0.020, 6, colour * 1.1, 0.0);
                if i == 0 || i == 7 {
                    // agate bands / opal's pale inset facets
                    parts.satin.torus(
                        at + Vec3::Y * 0.010,
                        Vec3::Y,
                        0.028,
                        0.003,
                        10,
                        4,
                        colour.lerp(Vec3::ONE, 0.36),
                    );
                }
            }
        }
        (Kind::Scroll, ItemSurface::Scroll(i)) if i < 18 => {
            // Stable visible ink and binding distinguish labels, not effects.
            // These are decorative letter-like strokes, never spell symbols.
            const BANDS: [[f32; 3]; 6] = [
                [0.40, 0.13, 0.10],
                [0.20, 0.27, 0.37],
                [0.47, 0.31, 0.12],
                [0.27, 0.33, 0.14],
                [0.34, 0.20, 0.32],
                [0.32, 0.30, 0.26],
            ];
            replace_pigment(
                &mut parts.matte,
                Vec3::new(0.40, 0.13, 0.10),
                Vec3::from_array(BANDS[usize::from(i) % 6]),
            );
            let mut code = u32::from(i) + 1;
            for z in [-0.115, -0.07, 0.07, 0.115] {
                let variant = code % 3;
                code /= 3;
                let angle = match variant {
                    0 => -0.65,
                    1 => 0.0,
                    _ => 0.65,
                };
                parts.matte.cuboid(
                    Vec3::new(0.0, 0.136, z),
                    Vec3::new(0.020, 0.0015, 0.003),
                    Quat::from_rotation_y(angle),
                    Vec3::new(0.20, 0.16, 0.12),
                );
                if variant == 2 {
                    parts.matte.cuboid(
                        Vec3::new(0.010, 0.136, z + 0.008),
                        Vec3::new(0.003, 0.0015, 0.009),
                        Quat::IDENTITY,
                        Vec3::new(0.20, 0.16, 0.12),
                    );
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn base(kind: Kind) -> Parts {
        super::super::rogue_supplies::ground(kind)
            .unwrap_or_else(|| super::super::ground_item_parts(kind))
    }
    fn signature(p: &Parts) -> Vec<u32> {
        let mut bits = Vec::new();
        for b in [&p.matte, &p.satin, &p.metal, &p.glow] {
            bits.push(b.positions.len() as u32);
            bits.extend(b.positions.iter().flatten().map(|x| x.to_bits()));
            bits.extend(b.colors.iter().flatten().map(|x| x.to_bits()));
        }
        bits
    }
    #[test]
    fn all_74_public_finishes_are_repeatable_distinct_and_nonemissive() {
        for (kind, count, make) in [
            (
                Kind::Potion,
                14,
                ItemSurface::Potion as fn(u8) -> ItemSurface,
            ),
            (Kind::Scroll, 18, ItemSurface::Scroll),
            (Kind::Wand, 14, ItemSurface::Wand),
            (Kind::Staff, 14, ItemSurface::Staff),
            (Kind::Ring, 14, ItemSurface::Ring),
        ] {
            let mut seen = Vec::new();
            for i in 0..count {
                let mut p = base(kind);
                apply(&mut p, kind, make(i));
                assert!(
                    p.glow.positions.is_empty() && p.accent.is_none(),
                    "{kind:?}/{i}"
                );
                let sig = signature(&p);
                assert!(
                    !seen.contains(&sig),
                    "{kind:?}/{i} aliases another appearance"
                );
                let mut repeated = base(kind);
                apply(&mut repeated, kind, make(i));
                assert_eq!(sig, signature(&repeated));
                seen.push(sig);
                for b in [&p.matte, &p.satin, &p.metal] {
                    assert!(b.positions.iter().flatten().all(|v| v.is_finite()));
                    assert!(b.colors.iter().flatten().all(|v| v.is_finite()));
                }
            }
        }
    }
    #[test]
    fn missing_invalid_or_wrong_family_surface_preserves_neutral_geometry_and_pigment() {
        for (kind, surface) in [
            (Kind::Potion, ItemSurface::Neutral),
            (Kind::Potion, ItemSurface::Potion(14)),
            (Kind::Scroll, ItemSurface::Scroll(18)),
            (Kind::Wand, ItemSurface::Wand(255)),
            (Kind::Staff, ItemSurface::Staff(14)),
            (Kind::Ring, ItemSurface::Ring(14)),
            (Kind::Wand, ItemSurface::Potion(0)),
        ] {
            let mut p = base(kind);
            let before = signature(&p);
            apply(&mut p, kind, surface);
            assert_eq!(before, signature(&p), "{kind:?}/{surface:?}");
        }
    }
}
