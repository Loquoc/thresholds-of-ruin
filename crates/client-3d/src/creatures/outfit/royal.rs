//! Royal garments composed from wearer-fitted shared parts.
use super::*;

pub(super) fn princess(wearer: &Look) -> Outfit {
    debug_assert!(
        wearer.woman(),
        "catalog must resolve the body-specific recipe"
    );
    let purple = Vec3::new(0.30, 0.095, 0.52);
    let lavender = Vec3::new(0.62, 0.43, 0.76);
    let gem = Vec3::new(0.59, 0.16, 0.85);
    Outfit {
        head: Headwear::Crown { metal: GILT, gem },
        tunic: purple,
        bodice: Bodice {
            straps: Straps::Sweetheart,
            edge: GILT,
            support: Support::Structured,
            ..Bodice::PLAIN
        },
        arms: Arms::Styled(Sleeve {
            color: purple,
            cut: SleeveCut::OffShoulder,
            puff: false,
            puff_lining: None,
            sheer: false,
            flare: Some(GILT),
        }),
        legs: Legs::Bare(Pigment::Skin(1.0)),
        belt: false,
        cuffs: None,
        torso_ornament: Some(TorsoOrnament::Corset { metal: GILT, gem }),
        trimmings: Trimmings {
            armlets: Some(GILT),
            bracelets: Some(GILT),
            earrings: Some(Earrings::GemDrops { metal: GILT, gem }),
            neck: Some(Neck::Pendant { metal: GILT, gem }),
            waist: Some(Waist::Jeweled { metal: GILT, gem }),
            ..Trimmings::NONE
        },
        skirt: Some(Skirt {
            color: purple,
            length: 0.86,
            fullness: 1.65,
            split: 0.33,
            cutaway: true,
            center_panel: Some(0.23),
            trim: Some(GILT),
            pattern: Pattern::Brocade {
                thread: GILT,
                motif: Motif::Scroll,
            },
            layers: [
                Some(Layer {
                    longer: -0.015,
                    fullness: Some(1.55),
                    trim: Some(GILT * 0.8),
                    ..Layer::petticoat(lavender, false)
                }),
                None,
            ],
            ..Skirt::PLAIN
        }),
        feet: Feet::Shoes(Shoes {
            color: GILT,
            heel: true,
            sandal: true,
            straps: 0.23,
        }),
        ..Outfit::base(wearer)
    }
}

const IVORY: Vec3 = Vec3::new(0.94, 0.84, 0.68);
const SAPPHIRE: Vec3 = Vec3::new(0.035, 0.12, 0.67);
const NAVY: Vec3 = Vec3::new(0.065, 0.065, 0.16);

pub(super) fn bride(wearer: &Look) -> Outfit {
    let mut dress = princess(wearer);
    dress.tunic = IVORY;
    dress.head = Headwear::Crown {
        metal: GILT,
        gem: SAPPHIRE,
    };
    dress.veil = Some(Veil {
        color: IVORY,
        trim: GILT,
        gem: SAPPHIRE,
        length: 1.80,
        spread: 0.65,
    });
    dress.bodice.pattern = Pattern::Brocade {
        thread: GILT,
        motif: Motif::Scroll,
    };
    dress.torso_ornament = Some(TorsoOrnament::Corset {
        metal: GILT,
        gem: SAPPHIRE,
    });
    dress.arms = Arms::Styled(Sleeve {
        color: IVORY,
        cut: SleeveCut::Bishop,
        puff: false,
        puff_lining: None,
        sheer: true,
        flare: Some(GILT),
    });
    dress.trimmings = Trimmings {
        earrings: Some(Earrings::GemDrops {
            metal: GILT,
            gem: SAPPHIRE,
        }),
        neck: Some(Neck::Pendant {
            metal: GILT,
            gem: SAPPHIRE,
        }),
        waist: Some(Waist::Jeweled {
            metal: GILT,
            gem: SAPPHIRE,
        }),
        bracelets: Some(GILT),
        ..Trimmings::NONE
    };
    dress.skirt = Some(Skirt {
        hem: Hem::Train {
            extension: 0.42,
            front_rise: 0.16,
        },
        color: IVORY,
        length: 0.90,
        fullness: 1.85,
        split: 0.17,
        cutaway: true,
        trim: Some(GILT),
        pattern: Pattern::Brocade {
            thread: GILT,
            motif: Motif::Scroll,
        },
        layers: [
            Some(Layer {
                longer: 0.015,
                fullness: Some(1.72),
                trim: Some(IVORY),
                ..Layer::petticoat(Vec3::new(0.70, 0.47, 0.65), false)
            }),
            Some(Layer {
                longer: 0.03,
                fullness: Some(1.60),
                trim: Some(GILT),
                ..Layer::petticoat(IVORY * 0.87, false)
            }),
        ],
        ..Skirt::PLAIN
    });
    dress
}

pub(super) fn groom(wearer: &Look) -> Outfit {
    Outfit {
        tunic: NAVY,
        collar_edge: GILT,
        bodice: Bodice {
            support: Support::Structured,
            pattern: Pattern::Brocade {
                thread: GILT,
                motif: Motif::Scroll,
            },
            ..Bodice::PLAIN
        },
        outer: Some(Outer::Cuirass),
        regalia: Some(Regalia {
            metal: GILT,
            gem: SAPPHIRE,
        }),
        arms: Arms::Sleeves(NAVY),
        hands: Pigment::Fixed(NAVY * 0.55),
        cuffs: None,
        legs: Legs::Trousers(NAVY),
        feet: Feet::Boots(Boots {
            color: Pigment::Fixed(NAVY * 0.50),
            tall: true,
            heel: false,
            buckles: false,
            cuffed: false,
        }),
        belt: true,
        baldric: None,
        cloak: Some(Cloak {
            embroidery: Some(GILT),
            exterior: NAVY,
            lining: Vec3::new(0.23, 0.055, 0.19),
            length: 1.40,
            ragged: false,
        }),
        skirt: Some(Skirt {
            color: NAVY,
            length: 0.38,
            split: 0.16,
            cutaway: true,
            trim: Some(GILT),
            pattern: Pattern::Brocade {
                thread: GILT,
                motif: Motif::Scroll,
            },
            ..Skirt::PLAIN
        }),
        ..Outfit::base(wearer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wedding_catalog_keeps_authored_pair_and_independent_accessories() {
        assert_eq!(Wardrobe::parse("royal-bride"), Some(Wardrobe::RoyalBride));
        assert_eq!(Wardrobe::parse("royal_groom"), Some(Wardrobe::RoyalGroom));
        assert_eq!(Wardrobe::RoyalBride.recipe_for(false), Wardrobe::RoyalGroom);
        for build in 0..3 {
            let woman = Look::with_choice(
                CreatureModel::Human,
                42,
                Some(Variant {
                    woman: true,
                    build,
                    ..default()
                }),
            );
            let bride = bride(&woman);
            assert_eq!(bride.bodice.straps, Straps::Sweetheart);
            assert_eq!(bride.bodice.support, Support::Structured);
            assert!(matches!(
                bride.arms,
                Arms::Styled(Sleeve {
                    cut: SleeveCut::Bishop,
                    sheer: true,
                    ..
                })
            ));
            assert!(bride.veil.is_some());
            assert!(bride.skirt.unwrap().split > 0.0);
            assert!(matches!(bride.skirt.unwrap().hem, Hem::Train { .. }));
            assert_eq!(
                bride.head,
                Headwear::Crown {
                    metal: GILT,
                    gem: SAPPHIRE
                }
            );
            let groom = groom(&woman);
            assert!(groom.regalia.is_some());
            assert!(groom.veil.is_none());
            assert_eq!(groom.outer, Some(Outer::Cuirass));
            assert_eq!(groom.arms, Arms::Sleeves(NAVY));
            assert!(groom.cloak.is_some());
        }
    }

    #[test]
    fn wedding_equipment_replaces_covered_ornament_without_mutating_recipe() {
        let look = Look::with_choice(
            CreatureModel::Human,
            42,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        let original = bride(&look);
        let mut equipment = Equipment::default();
        equipment.select("helm").unwrap();
        let dressed = equipment.compose(original);
        assert!(dressed.veil.is_none());
        assert!(original.veil.is_some());
        let mut equipment = Equipment::default();
        equipment.select("breastplate").unwrap();
        assert!(equipment.compose(groom(&look)).regalia.is_none());
    }
}
