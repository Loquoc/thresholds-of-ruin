//! Open-front forest garments assembled from shared fitted parts.
use super::*;

pub(super) fn duelist(wearer: &Look) -> Outfit {
    debug_assert!(
        wearer.woman(),
        "catalog must resolve the body-specific recipe"
    );
    let green = Vec3::new(0.065, 0.22, 0.15);
    let dark = Vec3::new(0.10, 0.075, 0.065);
    let gold = GILT * 0.8;
    Outfit {
        tunic: green,
        bodice: Bodice {
            straps: Straps::Panels { width: 0.85 },
            edge: gold,
            pattern: Pattern::Brocade {
                thread: gold,
                motif: Motif::Leaf,
            },
            ..Bodice::PLAIN
        },
        arms: Arms::Styled(Sleeve {
            color: dark,
            cut: SleeveCut::Draped,
            puff: false,
            puff_lining: None,
            sheer: false,
            flare: Some(green),
        }),
        legs: Legs::Trousers(dark),
        feet: Feet::Boots(Boots {
            color: Pigment::Fixed(dark),
            tall: true,
            heel: true,
            buckles: true,
            cuffed: true,
        }),
        belt: false,
        cuffs: Some(dark),
        trimmings: Trimmings {
            earrings: Some(Earrings::GemDrops {
                metal: gold,
                gem: gold,
            }),
            neck: Some(Neck::Pendant {
                metal: gold,
                gem: gold,
            }),
            waist: Some(Waist::Pointed {
                color: green * 0.7,
                edge: gold,
            }),
            ..Trimmings::NONE
        },
        skirt: Some(Skirt {
            color: green,
            length: 0.86,
            fullness: 1.05,
            split: 0.18,
            cutaway: true,
            center_panel: Some(0.12),
            leg_relief: Some(skirt_motion::LegRelief::FITTED.with_hem_yield(0.60)),
            hem: Hem::Points {
                count: 4,
                depth: 0.28,
            },
            waistline: Some(0.04),
            trim: Some(gold),
            pattern: Pattern::Brocade {
                thread: gold,
                motif: Motif::Leaf,
            },
            ..Skirt::PLAIN
        }),
        cloak: None,
        ..Outfit::base(wearer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forest_recipe_composes_panels_detached_drapes_and_pointed_waist() {
        let wearer = Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        let outfit = duelist(&wearer);
        assert!(matches!(outfit.bodice.straps, Straps::Panels { width } if width >= 0.8));
        assert!(matches!(
            outfit.trimmings.waist,
            Some(Waist::Pointed { .. })
        ));
        assert!(
            outfit.cloak.is_none(),
            "arm drapes carry silhouette instead of a redundant cloak"
        );
        assert!(
            !outfit.belt,
            "no generic buckle over the pointed centerpiece"
        );
        assert!(outfit.cuffs.is_some());
        assert!(matches!(outfit.skirt.unwrap().hem, Hem::Points { .. }));
    }
}
