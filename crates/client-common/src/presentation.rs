//! Small, built-in presentation vocabulary shared by clients.
//!
//! These are cosmetic defaults, not rules or scenario overrides. Assets resolve
//! only through the connection's current palette. Disclosed names remain the
//! authority for identity and commands; nouns are for otherwise unnamed things.
use std::sync::OnceLock;

use crate::{AssetTable, Palette};

/// A keyed English noun message with explicit grammatical forms. The stable
/// key identifies the whole group for a future locale; assets are never prose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NounForms {
    pub message_key: &'static str,
    pub singular: &'static str,
    pub plural: &'static str,
    pub indefinite: &'static str,
}

/// A client's cosmetic ASCII default. It never determines a gameplay fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AsciiAppearance {
    pub glyph: char,
    pub color: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Presentation {
    pub noun: NounForms,
    pub ascii: AsciiAppearance,
}

/// The vocabulary rows, one per asset: singular, plural and indefinite noun
/// forms, then the ASCII glyph and colour. Every message key is
/// `presentation.<asset>.noun`, so keys cannot drift from their assets.
macro_rules! vocabulary_rows {
    ($($asset:literal => $singular:literal, $plural:literal, $indefinite:literal, $glyph:literal, $color:literal;)*) => {
        [$((
            $asset,
            Presentation {
                noun: NounForms {
                    message_key: concat!("presentation.", $asset, ".noun"),
                    singular: $singular,
                    plural: $plural,
                    indefinite: $indefinite,
                },
                ascii: AsciiAppearance { glyph: $glyph, color: $color },
            },
        )),*]
    };
}

/// Version-one common entries. Dotted-prefix inheritance uses `AssetTable`;
/// scenario-specific client overrides and appearance traits are not implemented.
pub fn vocabulary() -> &'static AssetTable<Presentation> {
    static TABLE: OnceLock<AssetTable<Presentation>> = OnceLock::new();
    TABLE.get_or_init(|| {
        AssetTable::new(vocabulary_rows! {
            "terrain.floor" => "flagstone", "flagstones", "flagstone", '.', 0x7890a2;
            "terrain.floor.cave" => "packed earth", "packed earth", "packed earth", '.', 0xa58f6c;
            "terrain.floor.marble" => "polished marble", "polished marble", "polished marble", '.', 0xc9cbd4;
            "terrain.wall" => "dressed stone", "dressed stone", "dressed stone", '#', 0x7890a2;
            "terrain.wall.cave" => "rough cave rock", "rough cave rock", "rough cave rock", '#', 0xa58f6c;
            "terrain.wall.marble" => "polished marble", "polished marble", "polished marble", '#', 0xc9cbd4;
            "terrain.door" => "door", "doors", "a door", '+', 0xc9a46a;
            "creature" => "creature", "creatures", "a creature", '&', 0xef958c;
            "creature.bat" => "bat", "bats", "a bat", 'B', 0xa58f6c;
            "creature.bird.kestrel" => "kestrel", "kestrels", "a kestrel", 'K', 0xc9a46a;
            "creature.plant.venus.flytrap" => "venus flytrap", "venus flytraps", "a venus flytrap", 'F', 0x67d8bd;
            "creature.griffin" => "griffin", "griffins", "a griffin", 'G', 0xd9c77a;
            "creature.rat" => "rat", "rats", "a rat", 'r', 0xa58f6c;
            "creature.humanoid" => "humanoid", "humanoids", "a humanoid", '@', 0xef958c;
            "creature.humanoid.human" => "human", "humans", "a human", '@', 0x67d8bd;
            "creature.goblin" => "goblin", "goblins", "a goblin", 'o', 0xa6e07a;
            "creature.humanoid.orc" => "orc", "orcs", "an orc", 'O', 0xa6e07a;
            "creature.goblin.hobgoblin" => "hobgoblin", "hobgoblins", "a hobgoblin", 'H', 0xef958c;
            "creature.bird" => "bird", "birds", "a bird", 'B', 0xf2c14e;
            "creature.bird.emu" => "emu", "emus", "an emu", 'E', 0xa58f6c;
            "creature.serpent" => "serpent", "serpents", "a serpent", 'S', 0xa6e07a;
            "creature.serpent.snake" => "snake", "snakes", "a snake", 'S', 0xa6e07a;
            "creature.serpent.rattlesnake" => "rattlesnake", "rattlesnakes", "a rattlesnake", 'R', 0xf2c14e;
            "creature.equine" => "horse-like beast", "horse-like beasts", "a horse-like beast", 'u', 0xa58f6c;
            "creature.fey.leprechaun" => "leprechaun", "leprechauns", "a leprechaun", 'L', 0x67d8bd;
            "creature.fey.nymph" => "nymph", "nymphs", "a nymph", 'N', 0xf2a8c1;
            "creature.medusa" => "medusa", "medusas", "a medusa", 'M', 0x8fb84f;
            "creature.undead.phantom" => "phantom", "phantoms", "a phantom", 'P', 0xaab9ca;
            "creature.xeroc" => "xeroc", "xerocs", "a xeroc", 'X', 0xae8f6d;
            "creature.equine.quagga" => "quagga", "quaggas", "a quagga", 'Q', 0xa58f6c;
            "creature.equine.unicorn" => "unicorn", "unicorns", "a unicorn", 'u', 0xc9cbd4;
            "creature.equine.unicorn.black" => "black unicorn", "black unicorns", "a black unicorn", 'U', 0x869ba9;
            "creature.centaur" => "centaur", "centaurs", "a centaur", 'C', 0xa58f6c;
            "creature.ape" => "ape", "apes", "an ape", 'Y', 0xa58f6c;
            "creature.ape.yeti" => "yeti", "yetis", "a yeti", 'Y', 0xc9cbd4;
            "creature.troll" => "troll", "trolls", "a troll", 'T', 0xa6e07a;
            "creature.elemental" => "elemental", "elementals", "an elemental", 'E', 0x7fd4f0;
            "creature.elemental.ice" => "ice creature", "ice creatures", "an ice creature", 'I', 0x8cbafa;
            "creature.aquator" => "aquator", "aquators", "an aquator", 'A', 0x7fd4f0;
            "creature.dragon" => "dragon", "dragons", "a dragon", 'D', 0xf09b5b;
            "creature.jabberwock" => "jabberwock", "jabberwocks", "a jabberwock", 'J', 0xd98cf0;
            "creature.undead" => "undead creature", "undead creatures", "an undead creature", '&', 0x869ba9;
            "creature.undead.zombie" => "zombie", "zombies", "a zombie", 'Z', 0xa6e07a;
            "creature.undead.vampire" => "vampire", "vampires", "a vampire", 'V', 0xd98cf0;
            "creature.undead.wraith" => "wraith", "wraiths", "a wraith", 'W', 0x869ba9;
            "item" => "object", "objects", "an object", '*', 0xedc579;
            "item.weapon" => "weapon", "weapons", "a weapon", ')', 0xc9cbd4;
            "item.weapon.mace" => "mace", "maces", "a mace", ')', 0xc9cbd4;
            "item.armor" => "armor", "armor", "armor", '[', 0xc9cbd4;
            "item.armor.mail" => "mail", "mail", "mail", '[', 0xc9cbd4;
            "item.bandage" => "bandage", "bandages", "a bandage", '*', 0xc9cbd4;
            "item.food" => "food", "food", "food", '%', 0xedc579;
            "item.food.ration" => "food ration", "food rations", "a food ration", '%', 0xedc579;
            "item.amulet" => "amulet", "amulets", "an amulet", '"', 0xf2c14e;
            "item.potion" => "potion", "potions", "a potion", '!', 0xd98cf0;
            "item.coin" => "coin", "coins", "a coin", '$', 0xf2c14e;
        })
    })
}

/// Resolve only disclosed, currently forecast assets. Unknown assets may use a
/// known dotted parent; absent, unforecast or stale assets use caller fallbacks.
pub fn resolve(palette: &Palette, asset: Option<&str>) -> Option<&'static Presentation> {
    palette.resolve(vocabulary(), asset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tor_protocol::{PaletteBody, PaletteUpdate};

    #[test]
    fn aquator_and_dragon_use_shared_keyed_presentations_only_when_forecast() {
        let mut palette = Palette::default();
        for (asset, glyph, noun) in [
            ("creature.aquator", 'A', "aquator"),
            ("creature.dragon", 'D', "dragon"),
        ] {
            assert!(resolve(&palette, Some(asset)).is_none());
            palette.apply(&PaletteUpdate {
                revision: 1,
                body: PaletteBody::Full {
                    assets: [asset.to_owned()].into(),
                },
            });
            let entry = resolve(&palette, Some(asset)).unwrap();
            assert_eq!(entry.ascii.glyph, glyph);
            assert_eq!(entry.noun.singular, noun);
            assert!(entry.noun.message_key.ends_with(&format!("{noun}.noun")));
            palette = Palette::default();
        }
    }

    #[test]
    fn rogue_roster_assets_have_distinct_classic_letters() {
        let assets = [
            ("creature.bird.emu", 'E'),
            ("creature.serpent.snake", 'S'),
            ("creature.serpent.rattlesnake", 'R'),
            ("creature.goblin.hobgoblin", 'H'),
            ("creature.undead.zombie", 'Z'),
            ("creature.equine.quagga", 'Q'),
            ("creature.centaur", 'C'),
            ("creature.ape.yeti", 'Y'),
            ("creature.troll", 'T'),
            ("creature.equine.unicorn.black", 'U'),
            ("creature.elemental.ice", 'I'),
            ("creature.undead.vampire", 'V'),
            ("creature.undead.wraith", 'W'),
            ("creature.jabberwock", 'J'),
            ("creature.bat", 'B'),
            ("creature.bird.kestrel", 'K'),
            ("creature.plant.venus.flytrap", 'F'),
            ("creature.griffin", 'G'),
            ("creature.fey.leprechaun", 'L'),
            ("creature.fey.nymph", 'N'),
            ("creature.humanoid.orc", 'O'),
            ("creature.medusa", 'M'),
            ("creature.undead.phantom", 'P'),
            ("creature.xeroc", 'X'),
            ("creature.aquator", 'A'),
            ("creature.dragon", 'D'),
        ];
        let mut palette = Palette::default();
        palette.apply(&PaletteUpdate {
            revision: 1,
            body: PaletteBody::Full {
                assets: assets.iter().map(|(asset, _)| (*asset).into()).collect(),
            },
        });
        for (asset, glyph) in assets {
            assert_eq!(
                resolve(&palette, Some(asset)).unwrap().ascii.glyph,
                glyph,
                "{asset}"
            );
        }
    }

    #[test]
    fn vocabulary_is_shared_keyed_and_falls_back_only_inside_a_current_palette() {
        let asset = "creature.equine.quagga.unfamiliar";
        let mut palette = Palette::default();
        assert!(resolve(&palette, Some(asset)).is_none());
        assert!(resolve(&palette, Some("creature.jabberwock")).is_none());
        palette.apply(&PaletteUpdate {
            revision: 1,
            body: PaletteBody::Full {
                assets: [
                    asset.to_owned(),
                    "unknown.thing".into(),
                    "creature.jabberwock".into(),
                ]
                .into(),
            },
        });
        let entry = resolve(&palette, Some(asset)).unwrap();
        let jabberwock = resolve(&palette, Some("creature.jabberwock")).unwrap();
        assert_eq!(jabberwock.ascii.glyph, 'J');
        assert_eq!(jabberwock.noun.singular, "jabberwock");
        assert_eq!(jabberwock.noun.plural, "jabberwocks");
        assert_eq!(jabberwock.noun.indefinite, "a jabberwock");
        assert_eq!(
            jabberwock.noun.message_key,
            "presentation.creature.jabberwock.noun"
        );
        assert_eq!(entry.ascii.glyph, 'Q');
        assert_eq!(entry.noun.singular, "quagga");
        assert_eq!(entry.noun.plural, "quaggas");
        assert_eq!(entry.noun.indefinite, "a quagga");
        assert_eq!(
            entry.noun.message_key,
            "presentation.creature.equine.quagga.noun"
        );
        assert!(resolve(&palette, None).is_none());
        assert!(resolve(&palette, Some("creature.equine")).is_none());
        assert!(resolve(&palette, Some("unknown.thing")).is_none());
        assert!(palette.apply(&PaletteUpdate {
            revision: 3,
            body: PaletteBody::Delta {
                base: 2,
                added: Default::default(),
                removed: Default::default(),
            }
        }));
        assert!(resolve(&palette, Some(asset)).is_none());
        palette.apply(&PaletteUpdate {
            revision: 4,
            body: PaletteBody::Full {
                assets: [asset.to_owned()].into(),
            },
        });
        assert_eq!(resolve(&palette, Some(asset)).unwrap().ascii.glyph, 'Q');
        assert!(resolve(&palette, Some("creature.jabberwock")).is_none());
    }
}
