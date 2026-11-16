//! Inner shirt recipes retain the wearer's anatomy and unrelated worn pieces.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ShirtAppearance {
    TShirt,
    HawaiianShirt,
}

/// Shared authored openings for outfit coverage and visible shirt detailing.
/// Keep both sex-specific defaults even when composing only one fitted outfit.
#[derive(Clone, Copy)]
struct ShirtNecklines {
    feminine: Neckline,
    masculine: Neckline,
}
impl ShirtNecklines {
    fn select(self, woman: bool) -> Neckline {
        if woman {
            self.feminine
        } else {
            self.masculine
        }
    }
}

impl ShirtAppearance {
    pub const ALL: [Self; 2] = [Self::TShirt, Self::HawaiianShirt];
    pub const NAMES: [&'static str; 2] = ["t-shirt", "hawaiian-shirt"];
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }
    pub fn color(self, woman: bool) -> Vec3 {
        match self {
            Self::TShirt => Vec3::new(0.87, 0.85, 0.76),
            Self::HawaiianShirt if woman => Vec3::new(0.12, 0.29, 0.43),
            Self::HawaiianShirt => Vec3::new(0.62, 0.10, 0.055),
        }
    }
    fn necklines(self) -> ShirtNecklines {
        match self {
            Self::TShirt => ShirtNecklines {
                feminine: Neckline::Scoop {
                    depth: 0.28,
                    width: 0.18,
                },
                masculine: Neckline::Scoop {
                    depth: 0.0,
                    width: 0.11,
                },
            },
            Self::HawaiianShirt => ShirtNecklines {
                feminine: Neckline::Plunge {
                    depth: 0.65,
                    width: 0.16,
                    wrap: false,
                },
                masculine: Neckline::Plunge {
                    depth: 0.15,
                    width: 0.12,
                    wrap: false,
                },
            },
        }
    }
    pub(crate) fn neckline(self, woman: bool) -> Neckline {
        self.necklines().select(woman)
    }
    /// This replaces the costume's inner torso/sleeves, not its legs, hair,
    /// cloak or independent armor. Armor composition happens afterwards.
    pub fn compose(self, mut base: Outfit, woman: bool) -> Outfit {
        let color = self.color(woman);
        base.tunic = color;
        base.outer = None;
        base.torso_ornament = None;
        base.armhole_edge = None;
        base.collar_edge = color * 0.85;
        base.bodice = Bodice {
            cut: self.necklines().feminine,
            male_cut: Some(self.necklines().masculine),
            cover: if woman && self == Self::HawaiianShirt {
                Cover::Crop(Hem::Even)
            } else {
                Cover::Full
            },
            edge: color * 0.82,
            support: Support::Loose,
            pattern: match self {
                Self::TShirt => Pattern::Plain,
                Self::HawaiianShirt => Pattern::Floral,
            },
            ..Bodice::PLAIN
        };
        base.bodice = base.bodice.fitted(woman);
        base.arms = Arms::Styled(Sleeve {
            color,
            cut: SleeveCut::Short,
            puff: false,
            puff_lining: None,
            sheer: false,
            flare: None,
        });
        base.cuffs = None;
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn original_compose(kind: ShirtAppearance, mut base: Outfit, woman: bool) -> Outfit {
        let color = kind.color(woman);
        base.tunic = color;
        base.outer = None;
        base.torso_ornament = None;
        base.armhole_edge = None;
        base.collar_edge = color * 0.85;
        base.bodice = Bodice {
            cut: match kind {
                ShirtAppearance::TShirt => Neckline::Scoop {
                    depth: 0.28,
                    width: 0.18,
                },
                ShirtAppearance::HawaiianShirt => Neckline::Plunge {
                    depth: 0.65,
                    width: 0.16,
                    wrap: false,
                },
            },
            male_cut: Some(match kind {
                ShirtAppearance::TShirt => Neckline::Scoop {
                    depth: 0.0,
                    width: 0.11,
                },
                ShirtAppearance::HawaiianShirt => Neckline::Plunge {
                    depth: 0.15,
                    width: 0.12,
                    wrap: false,
                },
            }),
            cover: if woman && kind == ShirtAppearance::HawaiianShirt {
                Cover::Crop(Hem::Even)
            } else {
                Cover::Full
            },
            edge: color * 0.82,
            support: Support::Loose,
            pattern: match kind {
                ShirtAppearance::TShirt => Pattern::Plain,
                ShirtAppearance::HawaiianShirt => Pattern::Floral,
            },
            ..Bodice::PLAIN
        };
        base.bodice = base.bodice.fitted(woman);
        base.arms = Arms::Styled(Sleeve {
            color,
            cut: SleeveCut::Short,
            puff: false,
            puff_lining: None,
            sheer: false,
            flare: None,
        });
        base.cuffs = None;
        base
    }
    #[test]
    fn shared_openings_preserve_complete_composed_outfits() {
        for seed in 0..8 {
            let look = Look::of(CreatureModel::Human, seed);
            let base = Outfit::native(&look);
            for kind in ShirtAppearance::ALL {
                for woman in [false, true] {
                    let expected = original_compose(kind, base, woman);
                    assert_eq!(kind.compose(base, woman), expected);
                    let expected_cut = match (kind, woman) {
                        (ShirtAppearance::TShirt, true) => Neckline::Scoop {
                            depth: 0.28,
                            width: 0.18,
                        },
                        (ShirtAppearance::TShirt, false) => Neckline::Scoop {
                            depth: 0.0,
                            width: 0.11,
                        },
                        (ShirtAppearance::HawaiianShirt, true) => Neckline::Plunge {
                            depth: 0.65,
                            width: 0.16,
                            wrap: false,
                        },
                        (ShirtAppearance::HawaiianShirt, false) => Neckline::Plunge {
                            depth: 0.15,
                            width: 0.12,
                            wrap: false,
                        },
                    };
                    assert_eq!(kind.neckline(woman), expected_cut);
                }
            }
        }
    }

    #[test]
    fn tropical_study_retains_full_male_and_knotted_feminine_cuts() {
        let look = Look::of(CreatureModel::Human, 7);
        let base = Outfit::native(&look);
        let male = ShirtAppearance::HawaiianShirt.compose(base, false);
        let female = ShirtAppearance::HawaiianShirt.compose(base, true);
        assert_eq!(male.bodice.cover, Cover::Full);
        assert_eq!(female.bodice.cover, Cover::Crop(Hem::Even));
        assert!(male.tunic.x > male.tunic.z);
        assert!(female.tunic.z > female.tunic.x);
        for woman in [false, true] {
            assert_eq!(
                ShirtAppearance::TShirt.compose(base, woman).bodice.cover,
                Cover::Full
            );
        }
    }
}
