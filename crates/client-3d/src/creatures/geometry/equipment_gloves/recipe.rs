//! Four authored glove defaults shared by hand appearance and cuff fitting.
//! Values are unscaled; radial build and axial arm length stay independent.
use super::{GloveAppearance, Vec3};

#[derive(Clone, Copy)]
pub(super) struct CuffRecipe {
    pub(super) profile: [(f32, f32); 3],
    pub(super) opening_ease: f32,
}
#[derive(Clone, Copy)]
pub(super) struct GloveRecipe {
    pub(super) color: Vec3,
    pub(super) cuff: CuffRecipe,
}
pub(super) const fn resolve(appearance: GloveAppearance) -> GloveRecipe {
    let (color, profile, opening_ease) = match appearance {
        GloveAppearance::Old => (
            Vec3::new(0.37, 0.23, 0.12),
            [(0.0, 0.045), (0.035, 0.044), (0.105, 0.064)],
            0.001,
        ),
        // Leave the assembled middle palm-quilting row exposed. The lip and
        // radial fit stay fixed; only the lower cuff moves toward the forearm.
        GloveAppearance::Padded => (
            Vec3::new(0.23, 0.29, 0.39),
            [(0.025, 0.047), (0.045, 0.048), (0.075, 0.058)],
            0.003,
        ),
        GloveAppearance::Riding => (
            Vec3::new(0.30, 0.16, 0.075),
            [(0.0, 0.043), (0.035, 0.042), (0.120, 0.063)],
            0.0,
        ),
        GloveAppearance::Fencing => (
            Vec3::new(0.075, 0.075, 0.095),
            [(0.0, 0.042), (0.030, 0.043), (0.140, 0.072)],
            0.0035,
        ),
    };
    GloveRecipe {
        color,
        cuff: CuffRecipe {
            profile,
            opening_ease,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Independent authored defaults; other styles retain their original bits.
    #[test]
    fn authored_defaults_and_scaled_openings_match_independent_values() {
        let original = [
            (
                GloveAppearance::Old,
                [0.37_f32, 0.23, 0.12],
                [(0.0_f32, 0.045_f32), (0.035, 0.044), (0.105, 0.064)],
                0.001_f32,
            ),
            (
                GloveAppearance::Padded,
                [0.23, 0.29, 0.39],
                [(0.025, 0.047), (0.045, 0.048), (0.075, 0.058)],
                0.003,
            ),
            (
                GloveAppearance::Riding,
                [0.30, 0.16, 0.075],
                [(0.0, 0.043), (0.035, 0.042), (0.120, 0.063)],
                0.0,
            ),
            (
                GloveAppearance::Fencing,
                [0.075, 0.075, 0.095],
                [(0.0, 0.042), (0.030, 0.043), (0.140, 0.072)],
                0.0035,
            ),
        ];
        for (kind, color, profile, ease) in original {
            let actual = resolve(kind);
            assert_eq!(
                actual.color.to_array().map(f32::to_bits),
                color.map(f32::to_bits)
            );
            assert_eq!(
                kind.color().to_array().map(f32::to_bits),
                color.map(f32::to_bits)
            );
            for (a, b) in actual.cuff.profile.into_iter().zip(profile) {
                assert_eq!(
                    [a.0.to_bits(), a.1.to_bits()],
                    [b.0.to_bits(), b.1.to_bits()]
                );
            }
            assert_eq!(actual.cuff.opening_ease.to_bits(), ease.to_bits());
            for build in [0.75, 1.0, 1.22, 1.4] {
                let opening = super::super::opening_policy(kind, build);
                assert_eq!(opening.anchor.to_bits(), profile[1].0.to_bits());
                assert_eq!(opening.lip.to_bits(), profile[2].0.to_bits());
                assert_eq!(opening.ease.to_bits(), (ease * build).to_bits());
            }
        }
    }
}
