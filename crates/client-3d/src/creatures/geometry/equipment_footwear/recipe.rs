//! Authored footwear choices shared by foot, shaft and coverage resolution.
//! Contact fitting and mesh construction remain in their existing owners.
use super::{FootwearAppearance, Vec3};

#[derive(Clone, Copy)]
pub(super) enum Tongue {
    Leather,
    Canvas,
}
impl Tongue {
    pub(super) fn width(self) -> f32 {
        match self {
            Self::Leather => 0.50,
            Self::Canvas => 0.66,
        }
    }
    pub(super) fn depth(self) -> f32 {
        match self {
            Self::Leather => 0.006,
            Self::Canvas => 0.003,
        }
    }
    pub(super) fn color(self, leather: Vec3, trim: Vec3) -> Vec3 {
        match self {
            Self::Leather => leather * 0.75,
            Self::Canvas => trim * 0.62,
        }
    }
}
#[derive(Clone, Copy)]
pub(super) struct LacingRecipe {
    pub(super) rows: usize,
    pub(super) tongue: Tongue,
    pub(super) top_band: f32,
}
#[derive(Clone, Copy)]
pub(super) struct FootwearRecipe {
    pub(super) heeled: bool,
    pub(super) color: Vec3,
    pub(super) trim: Vec3,
    pub(super) coverage: f32,
    pub(super) snow_cuff: bool,
    pub(super) lacing: Option<LacingRecipe>,
}
pub(super) fn resolve(kind: FootwearAppearance) -> FootwearRecipe {
    use FootwearAppearance::*;
    let ordinary_trim = Vec3::new(0.52, 0.39, 0.22);
    let leather_lacing = |rows, top_band| {
        Some(LacingRecipe {
            rows,
            tongue: Tongue::Leather,
            top_band,
        })
    };
    let (heeled, color, trim, coverage, lacing) = match kind {
        WalkingShoes => (
            false,
            Vec3::new(0.39, 0.23, 0.12),
            ordinary_trim,
            0.07,
            None,
        ),
        HardShoes => (
            false,
            Vec3::new(0.25, 0.20, 0.15),
            Vec3::new(0.64, 0.68, 0.72),
            0.28,
            None,
        ),
        Jackboots => (true, Vec3::new(0.16, 0.13, 0.11), ordinary_trim, 0.86, None),
        CombatBoots => (
            false,
            Vec3::new(0.23, 0.17, 0.12),
            ordinary_trim,
            0.55,
            leather_lacing(8, 0.005),
        ),
        JungleBoots => (
            false,
            Vec3::new(0.33, 0.32, 0.17),
            ordinary_trim,
            0.55,
            Some(LacingRecipe {
                rows: 8,
                tongue: Tongue::Canvas,
                top_band: 0.005,
            }),
        ),
        HikingBoots => (
            false,
            Vec3::new(0.42, 0.29, 0.16),
            Vec3::new(0.62, 0.09, 0.06),
            0.39,
            leather_lacing(5, 0.012),
        ),
        MudBoots => (
            false,
            Vec3::new(0.20, 0.22, 0.19),
            ordinary_trim,
            0.78,
            None,
        ),
        BuckledBoots => (
            false,
            Vec3::new(0.40, 0.22, 0.10),
            Vec3::new(0.70, 0.49, 0.20),
            0.61,
            None,
        ),
        RidingBoots => (true, Vec3::new(0.12, 0.13, 0.14), ordinary_trim, 0.86, None),
        SnowBoots => (
            false,
            Vec3::new(0.37, 0.27, 0.18),
            Vec3::new(0.85, 0.79, 0.63),
            0.58,
            leather_lacing(6, 0.005),
        ),
    };
    FootwearRecipe {
        heeled,
        color,
        trim,
        coverage,
        snow_cuff: kind == SnowBoots,
        lacing,
    }
}
