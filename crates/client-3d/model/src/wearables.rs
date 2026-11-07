//! Public worn appearance recipes, independent of magical identity or anatomy.
use std::sync::OnceLock;
use tor_client_common::{AssetTable, Palette};

/// A visible shape. Eligibility remains server-owned; this is not an item type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WearableLook {
    TShirt,
    HawaiianShirt,
    OldGloves,
    PaddedGloves,
    RidingGloves,
    FencingGloves,
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

    LeatherHat,
    IronSkullCap,
    HardHat,
    Fedora,
    ConicalHat,
    DentedPot,
    CrystalHelmet,
    PlumedHelmet,
    EtchedHelmet,
    CrestedHelmet,
    VisoredHelmet,
    WalkingShoes,
    HardShoes,
    Jackboots,
    CombatBoots,
    JungleBoots,
    HikingBoots,
    MudBoots,
    BuckledBoots,
    RidingBoots,
    SnowBoots,
}
impl WearableLook {
    pub const ALL: [Self; 39] = [
        Self::TShirt,
        Self::HawaiianShirt,
        Self::OldGloves,
        Self::PaddedGloves,
        Self::RidingGloves,
        Self::FencingGloves,
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
        Self::LeatherHat,
        Self::IronSkullCap,
        Self::HardHat,
        Self::Fedora,
        Self::ConicalHat,
        Self::DentedPot,
        Self::CrystalHelmet,
        Self::PlumedHelmet,
        Self::EtchedHelmet,
        Self::CrestedHelmet,
        Self::VisoredHelmet,
        Self::WalkingShoes,
        Self::HardShoes,
        Self::Jackboots,
        Self::CombatBoots,
        Self::JungleBoots,
        Self::HikingBoots,
        Self::MudBoots,
        Self::BuckledBoots,
        Self::RidingBoots,
        Self::SnowBoots,
    ];
    pub const fn name(self) -> &'static str {
        match self {
            Self::TShirt => "t-shirt",
            Self::HawaiianShirt => "hawaiian-shirt",
            Self::OldGloves => "old-gloves",
            Self::PaddedGloves => "padded-gloves",
            Self::RidingGloves => "riding-gloves",
            Self::FencingGloves => "fencing-gloves",
            Self::MummyWrapping => "mummy-wrapping",
            Self::FadedPall => "faded-pall",
            Self::CoarseMantelet => "coarse-mantelet",
            Self::HoodedCloak => "hooded-cloak",
            Self::SlipperyCloak => "slippery-cloak",
            Self::LeatherCloak => "leather-cloak",
            Self::Robe => "robe",
            Self::Apron => "apron",
            Self::TatteredCape => "tattered-cape",
            Self::OperaCloak => "opera-cloak",
            Self::OrnamentalCope => "ornamental-cope",
            Self::PieceOfCloth => "piece-of-cloth",

            Self::LeatherHat => "leather-hat",
            Self::IronSkullCap => "iron-skull-cap",
            Self::HardHat => "hard-hat",
            Self::Fedora => "fedora",
            Self::ConicalHat => "conical-hat",
            Self::DentedPot => "dented-pot",
            Self::CrystalHelmet => "crystal-helmet",
            Self::PlumedHelmet => "plumed-helmet",
            Self::EtchedHelmet => "etched-helmet",
            Self::CrestedHelmet => "crested-helmet",
            Self::VisoredHelmet => "visored-helmet",
            Self::WalkingShoes => "walking-shoes",
            Self::HardShoes => "hard-shoes",
            Self::Jackboots => "jackboots",
            Self::CombatBoots => "combat-boots",
            Self::JungleBoots => "jungle-boots",
            Self::HikingBoots => "hiking-boots",
            Self::MudBoots => "mud-boots",
            Self::BuckledBoots => "buckled-boots",
            Self::RidingBoots => "riding-boots",
            Self::SnowBoots => "snow-boots",
        }
    }
    pub const fn slot(self) -> &'static str {
        match self {
            Self::TShirt => "shirt",
            Self::HawaiianShirt => "shirt",
            Self::OldGloves => "hands_armor",
            Self::PaddedGloves => "hands_armor",
            Self::RidingGloves => "hands_armor",
            Self::FencingGloves => "hands_armor",
            Self::MummyWrapping => "cloak",
            Self::FadedPall => "cloak",
            Self::CoarseMantelet => "cloak",
            Self::HoodedCloak => "cloak",
            Self::SlipperyCloak => "cloak",
            Self::LeatherCloak => "cloak",
            Self::Robe => "cloak",
            Self::Apron => "cloak",
            Self::TatteredCape => "cloak",
            Self::OperaCloak => "cloak",
            Self::OrnamentalCope => "cloak",
            Self::PieceOfCloth => "cloak",

            Self::LeatherHat
            | Self::IronSkullCap
            | Self::HardHat
            | Self::Fedora
            | Self::ConicalHat
            | Self::DentedPot
            | Self::CrystalHelmet
            | Self::PlumedHelmet
            | Self::EtchedHelmet
            | Self::CrestedHelmet
            | Self::VisoredHelmet => "head_armor",
            Self::WalkingShoes
            | Self::HardShoes
            | Self::Jackboots
            | Self::CombatBoots
            | Self::JungleBoots
            | Self::HikingBoots
            | Self::MudBoots
            | Self::BuckledBoots
            | Self::RidingBoots
            | Self::SnowBoots => "feet_armor",
        }
    }
}

/// Palette validation happens before recipe selection. Unknown appearance
/// prefixes retain the ordinary slot fallback, without guessing an effect.
pub(crate) fn resolve(palette: &Palette, asset: Option<&str>, slot: &str) -> Option<WearableLook> {
    static TABLE: OnceLock<AssetTable<WearableLook>> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        AssetTable::new([
            ("item.armor.shirt.t-shirt", WearableLook::TShirt),
            (
                "item.armor.shirt.hawaiian-shirt",
                WearableLook::HawaiianShirt,
            ),
            ("item.armor.hands.old-gloves", WearableLook::OldGloves),
            ("item.armor.hands.padded-gloves", WearableLook::PaddedGloves),
            ("item.armor.hands.riding-gloves", WearableLook::RidingGloves),
            (
                "item.armor.hands.fencing-gloves",
                WearableLook::FencingGloves,
            ),
            (
                "item.armor.cloak.mummy-wrapping",
                WearableLook::MummyWrapping,
            ),
            ("item.armor.cloak.faded-pall", WearableLook::FadedPall),
            (
                "item.armor.cloak.coarse-mantelet",
                WearableLook::CoarseMantelet,
            ),
            ("item.armor.cloak.hooded-cloak", WearableLook::HoodedCloak),
            (
                "item.armor.cloak.slippery-cloak",
                WearableLook::SlipperyCloak,
            ),
            ("item.armor.cloak.leather-cloak", WearableLook::LeatherCloak),
            ("item.armor.cloak.robe", WearableLook::Robe),
            ("item.armor.cloak.apron", WearableLook::Apron),
            ("item.armor.cloak.tattered-cape", WearableLook::TatteredCape),
            ("item.armor.cloak.opera-cloak", WearableLook::OperaCloak),
            (
                "item.armor.cloak.ornamental-cope",
                WearableLook::OrnamentalCope,
            ),
            (
                "item.armor.cloak.piece-of-cloth",
                WearableLook::PieceOfCloth,
            ),
            ("item.armor.head.leather-hat", WearableLook::LeatherHat),
            ("item.armor.head.iron-skull-cap", WearableLook::IronSkullCap),
            ("item.armor.head.hard-hat", WearableLook::HardHat),
            ("item.armor.head.fedora", WearableLook::Fedora),
            ("item.armor.head.conical-hat", WearableLook::ConicalHat),
            ("item.armor.head.dented-pot", WearableLook::DentedPot),
            (
                "item.armor.head.crystal-helmet",
                WearableLook::CrystalHelmet,
            ),
            ("item.armor.head.plumed-helmet", WearableLook::PlumedHelmet),
            ("item.armor.head.etched-helmet", WearableLook::EtchedHelmet),
            (
                "item.armor.head.crested-helmet",
                WearableLook::CrestedHelmet,
            ),
            (
                "item.armor.head.visored-helmet",
                WearableLook::VisoredHelmet,
            ),
            ("item.armor.feet.walking-shoes", WearableLook::WalkingShoes),
            ("item.armor.feet.hard-shoes", WearableLook::HardShoes),
            ("item.armor.feet.jackboots", WearableLook::Jackboots),
            ("item.armor.feet.combat-boots", WearableLook::CombatBoots),
            ("item.armor.feet.jungle-boots", WearableLook::JungleBoots),
            ("item.armor.feet.hiking-boots", WearableLook::HikingBoots),
            ("item.armor.feet.mud-boots", WearableLook::MudBoots),
            ("item.armor.feet.buckled-boots", WearableLook::BuckledBoots),
            ("item.armor.feet.riding-boots", WearableLook::RidingBoots),
            ("item.armor.feet.snow-boots", WearableLook::SnowBoots),
        ])
    });
    palette
        .resolve(table, asset)
        .copied()
        .filter(|look| look.slot() == slot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tor_protocol::{PaletteBody, PaletteUpdate};

    #[test]
    fn every_physical_look_requires_palette_approval_and_its_own_slot() {
        for look in WearableLook::ALL {
            let family = match look.slot() {
                "head_armor" => "head",
                "feet_armor" => "feet",
                "hands_armor" => "hands",
                slot => slot,
            };
            let asset = format!("item.armor.{family}.{}", look.name());
            let mut palette = Palette::default();
            assert_eq!(resolve(&palette, Some(&asset), look.slot()), None);
            palette.apply(&PaletteUpdate {
                revision: 1,
                body: PaletteBody::Full {
                    assets: [asset.clone()].into(),
                },
            });
            assert_eq!(resolve(&palette, Some(&asset), look.slot()), Some(look));
            assert_eq!(resolve(&palette, Some(&asset), "body_armor"), None);
            assert_eq!(
                resolve(
                    &palette,
                    Some(&asset),
                    if family == "head" {
                        "feet_armor"
                    } else {
                        "head_armor"
                    }
                ),
                None
            );
        }
    }

    #[test]
    fn worn_mapping_preserves_appearance_without_relocating_an_unknown_socket() {
        use crate::{wearing, Wearing};
        use tor_protocol::{ItemClass, WornView};
        for look in WearableLook::ALL {
            let family = match look.slot() {
                "head_armor" => "head",
                "feet_armor" => "feet",
                "hands_armor" => "hands",
                slot => slot,
            };
            let asset = format!("item.armor.{family}.{}", look.name());
            let mut palette = Palette::default();
            palette.apply(&PaletteUpdate {
                revision: 1,
                body: PaletteBody::Full {
                    assets: [asset.clone()].into(),
                },
            });
            let mut piece = WornView {
                slot: look.slot().into(),
                occurrence: 0,
                class: ItemClass::Armor,
                appearance: "unidentified wearable".into(),
                asset: Some(asset),
            };
            let visible = wearing(&palette, &[piece.clone()]);
            let actual = match look.slot() {
                "head_armor" => visible.head_look,
                "feet_armor" => visible.feet_look,
                "shirt" => visible.shirt_look,
                "hands_armor" => visible.hands_look,
                "cloak" => visible.back_look,
                _ => unreachable!(),
            };
            assert_eq!(actual, Some(look));
            piece.appearance = "a completely different label".into();
            assert_eq!(wearing(&palette, &[piece.clone()]), visible);
            piece.slot = "tail_armor".into();
            assert_eq!(wearing(&palette, &[piece.clone()]), Wearing::default());
            piece.slot = look.slot().into();
            piece.occurrence = 1;
            assert_eq!(wearing(&palette, &[piece]), Wearing::default());
        }
    }
}
