//! Inner shirt recipes retain the wearer's anatomy and unrelated worn pieces.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ShirtAppearance {
    TShirt,
    HawaiianShirt,
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
            cut: match self {
                Self::TShirt => Neckline::Scoop {
                    depth: 0.28,
                    width: 0.18,
                },
                Self::HawaiianShirt => Neckline::Plunge {
                    depth: 0.65,
                    width: 0.16,
                    wrap: false,
                },
            },
            male_cut: Some(match self {
                Self::TShirt => Neckline::Scoop {
                    depth: 0.0,
                    width: 0.11,
                },
                Self::HawaiianShirt => Neckline::Plunge {
                    depth: 0.15,
                    width: 0.12,
                    wrap: false,
                },
            }),
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
