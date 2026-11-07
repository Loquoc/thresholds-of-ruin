//! Portable outerwear patterns. These describe garments, never body identity.
//! An open drape does not suppress the shirt or armour beneath it. Wraps and
//! aprons have their own surface construction instead of masquerading as capes.
use super::{Cloak, FittedRobe, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum OuterwearAppearance {
    MummyWrapping,
    FadedPall,
    CoarseMantelet,
    HoodedCloak,
    SlipperyCloak,
    LeatherCloak,
    Robe,
    Apron,
    TatteredCape,
    OperaCloak,
    OrnamentalCope,
    PieceOfCloth,
}

/// Pattern family is independent of pigment and anatomy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum OuterwearCut {
    Wraps {
        bands: u8,
        width: f32,
    },
    Apron {
        bib_width: f32,
        length: f32,
        pocket: bool,
    },
    Robe,
    Drape(DrapeCut),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DrapeHem {
    Rounded,
    Frayed,
    Torn,
    Rectangular,
}

/// A shoulder pattern, fitted over the resolved torso surface at construction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DrapeCut {
    pub length: f32,
    /// Multiplier of the wearer's shoulder span at the free hem.
    pub fullness: f32,
    pub folds: u8,
    /// Front angular gap in radians. Never a sealed torso shell.
    pub opening: f32,
    pub hem: DrapeHem,
    /// Separate short shoulder layer over the main drape, when present.
    pub shoulder_layer: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OuterwearHood {
    None,
    Raised,
    Lowered,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OuterwearNeck {
    None,
    Cowl,
    StandingCollar,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OuterwearFastening {
    None,
    Brooch,
    Buckle,
    DoubleBuckle,
    Cord,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OuterwearRecipe {
    pub cloth: Vec3,
    pub lining: Vec3,
    pub trim: Vec3,
    pub cut: OuterwearCut,
    pub hood: OuterwearHood,
    pub neck: OuterwearNeck,
    pub fastening: OuterwearFastening,
    /// Waxed oilcloth and leather use the satin surface rather than matte.
    pub satin: bool,
    pub embroidered: bool,
}

impl OuterwearAppearance {
    pub const ALL: [Self; 12] = [
        Self::MummyWrapping,
        Self::FadedPall,
        Self::CoarseMantelet,
        Self::HoodedCloak,
        Self::SlipperyCloak,
        Self::LeatherCloak,
        Self::Robe,
        Self::Apron,
        Self::TatteredCape,
        Self::OperaCloak,
        Self::OrnamentalCope,
        Self::PieceOfCloth,
    ];
    pub const NAMES: [(&'static str, Self); 12] = [
        ("mummy-wrapping", Self::MummyWrapping),
        ("faded-pall", Self::FadedPall),
        ("coarse-mantelet", Self::CoarseMantelet),
        ("hooded-cloak", Self::HoodedCloak),
        ("slippery-cloak", Self::SlipperyCloak),
        ("leather-cloak", Self::LeatherCloak),
        ("robe", Self::Robe),
        ("apron", Self::Apron),
        ("tattered-cape", Self::TatteredCape),
        ("opera-cloak", Self::OperaCloak),
        ("ornamental-cope", Self::OrnamentalCope),
        ("piece-of-cloth", Self::PieceOfCloth),
    ];
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
    pub fn name(self) -> &'static str {
        Self::NAMES
            .iter()
            .find(|(_, kind)| *kind == self)
            .expect("complete outerwear catalog")
            .0
    }
    pub fn recipe(self) -> OuterwearRecipe {
        use DrapeHem::*;
        use OuterwearFastening::*;
        let linen = Vec3::new(0.66, 0.57, 0.39);
        let gold = Vec3::new(0.72, 0.49, 0.18);
        let mut r = OuterwearRecipe {
            cloth: linen,
            lining: linen * 0.72,
            trim: linen * 0.63,
            cut: OuterwearCut::Drape(DrapeCut {
                length: 0.95,
                fullness: 1.8,
                folds: 10,
                opening: 1.3,
                hem: Rounded,
                shoulder_layer: false,
            }),
            hood: OuterwearHood::None,
            neck: OuterwearNeck::None,
            fastening: None,
            satin: false,
            embroidered: false,
        };
        match self {
            Self::MummyWrapping => {
                r.cut = OuterwearCut::Wraps {
                    bands: 9,
                    width: 0.036,
                }
            }
            Self::Apron => {
                r.cloth = Vec3::new(0.84, 0.80, 0.65);
                r.cut = OuterwearCut::Apron {
                    bib_width: 0.20,
                    length: 0.55,
                    pocket: true,
                };
                r.fastening = Cord;
            }
            Self::Robe => {
                r.cloth = Vec3::new(0.37, 0.12, 0.055);
                r.trim = linen;
                r.cut = OuterwearCut::Robe;
                r.hood = OuterwearHood::Lowered;
                r.neck = OuterwearNeck::Cowl;
                r.fastening = Cord;
            }
            _ => {
                let (length, fullness, folds, opening, hem) = match self {
                    Self::FadedPall => (1.13, 1.65, 12, 1.45, Torn),
                    Self::CoarseMantelet => (0.38, 1.95, 8, 0.85, Frayed),
                    Self::HoodedCloak => (1.00, 1.95, 12, 1.05, Rounded),
                    Self::SlipperyCloak => (1.03, 2.05, 10, 1.20, Rounded),
                    Self::LeatherCloak => (1.04, 1.72, 6, 1.45, Rounded),
                    Self::TatteredCape => (1.10, 1.50, 7, 2.20, Torn),
                    Self::OperaCloak => (1.27, 2.65, 14, 1.55, Rounded),
                    Self::OrnamentalCope => (1.06, 2.55, 16, 1.15, Rounded),
                    Self::PieceOfCloth => (0.65, 1.65, 5, 1.80, Rectangular),
                    _ => unreachable!(),
                };
                r.cut = OuterwearCut::Drape(DrapeCut {
                    length,
                    fullness,
                    folds,
                    opening,
                    hem,
                    shoulder_layer: matches!(self, Self::LeatherCloak | Self::OrnamentalCope),
                });
                match self {
                    Self::FadedPall => {
                        r.cloth = Vec3::new(0.17, 0.18, 0.16);
                        r.neck = OuterwearNeck::Cowl;
                        r.fastening = Brooch;
                    }
                    Self::CoarseMantelet => {
                        r.cloth = Vec3::new(0.32, 0.26, 0.15);
                        r.fastening = Buckle;
                    }
                    Self::HoodedCloak => {
                        r.cloth = Vec3::new(0.38, 0.23, 0.10);
                        r.hood = OuterwearHood::Raised;
                        r.fastening = Brooch;
                        r.trim = gold;
                    }
                    Self::SlipperyCloak => {
                        r.cloth = Vec3::new(0.55, 0.34, 0.07);
                        r.hood = OuterwearHood::Lowered;
                        r.fastening = DoubleBuckle;
                        r.satin = true;
                    }
                    Self::LeatherCloak => {
                        r.cloth = Vec3::new(0.26, 0.12, 0.055);
                        r.neck = OuterwearNeck::StandingCollar;
                        r.fastening = DoubleBuckle;
                        r.satin = true;
                    }
                    Self::TatteredCape | Self::PieceOfCloth => {
                        r.neck = OuterwearNeck::Cowl;
                    }
                    Self::OperaCloak => {
                        r.cloth = Vec3::new(0.035, 0.03, 0.045);
                        r.lining = Vec3::new(0.48, 0.025, 0.045);
                        r.neck = OuterwearNeck::StandingCollar;
                        r.fastening = Brooch;
                        r.trim = gold;
                        r.satin = true;
                    }
                    Self::OrnamentalCope => {
                        r.cloth = Vec3::new(0.86, 0.79, 0.60);
                        r.lining = Vec3::new(0.055, 0.19, 0.30);
                        r.trim = gold;
                        r.fastening = Brooch;
                        r.embroidered = true;
                    }
                    _ => unreachable!(),
                }
                if !matches!(self, Self::OperaCloak | Self::OrnamentalCope) {
                    r.lining = r.cloth * 0.72;
                }
            }
        }
        r
    }
}
impl OuterwearRecipe {
    /// Base animated cloth properties. The renderer must also apply DrapeCut;
    /// this method deliberately returns no cape for non-cape pattern families.
    pub fn cloak(self) -> Option<Cloak> {
        let OuterwearCut::Drape(cut) = self.cut else {
            return None;
        };
        Some(Cloak {
            exterior: self.cloth,
            lining: self.lining,
            length: cut.length,
            ragged: matches!(cut.hem, DrapeHem::Frayed | DrapeHem::Torn),
        })
    }
    pub fn robe(self) -> Option<FittedRobe> {
        matches!(self.cut, OuterwearCut::Robe).then_some(FittedRobe {
            color: self.cloth,
            trim: self.trim,
            fold_seed: 3,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_round_trips_without_aliasing_unknown_names() {
        assert_eq!(OuterwearAppearance::ALL.len(), 12);
        for (index, kind) in OuterwearAppearance::ALL.into_iter().enumerate() {
            assert_eq!(OuterwearAppearance::parse(kind.name()), Some(kind));
            assert!(!OuterwearAppearance::ALL[..index].contains(&kind));
        }
        for name in ["cloak", "robe-extra", "", "MummyWrapping"] {
            assert_eq!(OuterwearAppearance::parse(name), None);
        }
    }
    #[test]
    fn non_capes_keep_their_own_construction_and_open_drapes_remain_open() {
        for kind in [
            OuterwearAppearance::MummyWrapping,
            OuterwearAppearance::Robe,
            OuterwearAppearance::Apron,
        ] {
            assert!(kind.recipe().cloak().is_none());
        }
        for kind in OuterwearAppearance::ALL {
            let r = kind.recipe();
            assert_eq!(r.robe().is_some(), kind == OuterwearAppearance::Robe);
            if let OuterwearCut::Drape(cut) = r.cut {
                assert!(cut.opening >= 0.8);
                assert!(cut.length > 0.0);
                assert!(cut.fullness >= 1.0);
            }
        }
        let opera = OuterwearAppearance::OperaCloak.recipe();
        assert_ne!(opera.cloth, opera.lining);
        assert_eq!(opera.neck, OuterwearNeck::StandingCollar);
        assert_eq!(
            OuterwearAppearance::HoodedCloak.recipe().hood,
            OuterwearHood::Raised
        );
        assert!(OuterwearAppearance::OrnamentalCope.recipe().embroidered);
    }
}
