//! Selection of cosmetic recipes, separate from mesh construction.
use std::sync::OnceLock;
use tor_client_common::{AssetTable, Palette};
use tor_protocol::{
    ActorView, ItemClass, ItemView, ObjectPresentation, ObjectPresentationKind, WornView,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CreatureModel {
    Placeholder,
    Humanoid,
    Human,
    Sentinel,
    RedSentinel,
    BlueSentinel,
    Mage,
    RedMage,
    Scout,
    Guardian,
    Hobgoblin,
    Zombie,
    Vampire,
    Yeti,
    Troll,
    Bird,
    Emu,
    Equine,
    Quagga,
    Unicorn,
    BlackUnicorn,
    Centaur,
    Serpent,
    Snake,
    Wisp,
    Ice,
    Orc,
    Leprechaun,
    Nymph,
    Medusa,
    Phantom,
    Wraith,
    Bat,
    Kestrel,
    Griffin,
    Dragon,
    Jabberwock,
    Aquator,
    Xeroc,
    Flytrap,
    Rattlesnake,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemModel {
    Stairs,
    Placeholder,
    Mace,
    Greatsword,
    ShortSword,
    LongSword,
    Dagger,
    Shortbow,
    Arrow,
    Spear,
    Dart,
    Shuriken,
    Fruit,
    Torch,
    Staff,
    LeatherArmor,
    StuddedLeather,
    RingMail,
    ScaleMail,
    ChainMail,
    SplintMail,
    BandedMail,
    PlateMail,
    Bandage,
    Ration,
    Corpse,
    Seal,
    Coins,
    Tablet,
    Potion,
    Ring,
    Amulet,
    Scroll,
    Spellbook,
    Wand,
    Gem,
    Pick,
    Pouch,
}

/// Public, persistent cosmetic appearance; never an item's true identity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ItemSurface {
    #[default]
    Neutral,
    Potion(u8),
    Scroll(u8),
    Wand(u8),
    Staff(u8),
    Ring(u8),
}

/// Resolve a hierarchical public appearance asset within its model family.
pub fn item_surface(kind: ItemModel, asset: &str) -> ItemSurface {
    let mut key = asset;
    loop {
        let surface = match (kind, key) {
            (ItemModel::Potion, "item.potion.amber") => Some(ItemSurface::Potion(0)),
            (ItemModel::Potion, "item.potion.smoky") => Some(ItemSurface::Potion(1)),
            (ItemModel::Potion, "item.potion.violet") => Some(ItemSurface::Potion(2)),
            (ItemModel::Potion, "item.potion.azure") => Some(ItemSurface::Potion(3)),
            (ItemModel::Potion, "item.potion.crimson") => Some(ItemSurface::Potion(4)),
            (ItemModel::Potion, "item.potion.silver") => Some(ItemSurface::Potion(5)),
            (ItemModel::Potion, "item.potion.pearl") => Some(ItemSurface::Potion(6)),
            (ItemModel::Potion, "item.potion.teal") => Some(ItemSurface::Potion(7)),
            (ItemModel::Potion, "item.potion.ochre") => Some(ItemSurface::Potion(8)),
            (ItemModel::Potion, "item.potion.indigo") => Some(ItemSurface::Potion(9)),
            (ItemModel::Potion, "item.potion.copper") => Some(ItemSurface::Potion(10)),
            (ItemModel::Potion, "item.potion.chartreuse") => Some(ItemSurface::Potion(11)),
            (ItemModel::Potion, "item.potion.saffron") => Some(ItemSurface::Potion(12)),
            (ItemModel::Potion, "item.potion.cerulean") => Some(ItemSurface::Potion(13)),
            (ItemModel::Scroll, "item.scroll.vel-mor") => Some(ItemSurface::Scroll(0)),
            (ItemModel::Scroll, "item.scroll.elam-tor") => Some(ItemSurface::Scroll(1)),
            (ItemModel::Scroll, "item.scroll.nar-shen") => Some(ItemSurface::Scroll(2)),
            (ItemModel::Scroll, "item.scroll.heth-ara") => Some(ItemSurface::Scroll(3)),
            (ItemModel::Scroll, "item.scroll.tal-ion") => Some(ItemSurface::Scroll(4)),
            (ItemModel::Scroll, "item.scroll.ur-sek") => Some(ItemSurface::Scroll(5)),
            (ItemModel::Scroll, "item.scroll.kel-ran") => Some(ItemSurface::Scroll(6)),
            (ItemModel::Scroll, "item.scroll.an-dor") => Some(ItemSurface::Scroll(7)),
            (ItemModel::Scroll, "item.scroll.iv-mem") => Some(ItemSurface::Scroll(8)),
            (ItemModel::Scroll, "item.scroll.os-thal") => Some(ItemSurface::Scroll(9)),
            (ItemModel::Scroll, "item.scroll.vel-tor") => Some(ItemSurface::Scroll(10)),
            (ItemModel::Scroll, "item.scroll.sah-ren") => Some(ItemSurface::Scroll(11)),
            (ItemModel::Scroll, "item.scroll.or-veth") => Some(ItemSurface::Scroll(12)),
            (ItemModel::Scroll, "item.scroll.nem-al") => Some(ItemSurface::Scroll(13)),
            (ItemModel::Scroll, "item.scroll.tir-os") => Some(ItemSurface::Scroll(14)),
            (ItemModel::Scroll, "item.scroll.kha-mer") => Some(ItemSurface::Scroll(15)),
            (ItemModel::Scroll, "item.scroll.lor-nev") => Some(ItemSurface::Scroll(16)),
            (ItemModel::Scroll, "item.scroll.mir-vel") => Some(ItemSurface::Scroll(17)),
            (ItemModel::Wand, "item.wand.aluminum") => Some(ItemSurface::Wand(0)),
            (ItemModel::Wand, "item.wand.bronze") => Some(ItemSurface::Wand(1)),
            (ItemModel::Wand, "item.wand.copper") => Some(ItemSurface::Wand(2)),
            (ItemModel::Wand, "item.wand.electrum") => Some(ItemSurface::Wand(3)),
            (ItemModel::Wand, "item.wand.gold") => Some(ItemSurface::Wand(4)),
            (ItemModel::Wand, "item.wand.iron") => Some(ItemSurface::Wand(5)),
            (ItemModel::Wand, "item.wand.lead") => Some(ItemSurface::Wand(6)),
            (ItemModel::Wand, "item.wand.magnesium") => Some(ItemSurface::Wand(7)),
            (ItemModel::Wand, "item.wand.nickel") => Some(ItemSurface::Wand(8)),
            (ItemModel::Wand, "item.wand.platinum") => Some(ItemSurface::Wand(9)),
            (ItemModel::Wand, "item.wand.silver") => Some(ItemSurface::Wand(10)),
            (ItemModel::Wand, "item.wand.steel") => Some(ItemSurface::Wand(11)),
            (ItemModel::Wand, "item.wand.tin") => Some(ItemSurface::Wand(12)),
            (ItemModel::Wand, "item.wand.zinc") => Some(ItemSurface::Wand(13)),
            (ItemModel::Staff, "item.wand.staff.ash") => Some(ItemSurface::Staff(0)),
            (ItemModel::Staff, "item.wand.staff.birch") => Some(ItemSurface::Staff(1)),
            (ItemModel::Staff, "item.wand.staff.cedar") => Some(ItemSurface::Staff(2)),
            (ItemModel::Staff, "item.wand.staff.cherry") => Some(ItemSurface::Staff(3)),
            (ItemModel::Staff, "item.wand.staff.elm") => Some(ItemSurface::Staff(4)),
            (ItemModel::Staff, "item.wand.staff.hazel") => Some(ItemSurface::Staff(5)),
            (ItemModel::Staff, "item.wand.staff.mahogany") => Some(ItemSurface::Staff(6)),
            (ItemModel::Staff, "item.wand.staff.maple") => Some(ItemSurface::Staff(7)),
            (ItemModel::Staff, "item.wand.staff.oak") => Some(ItemSurface::Staff(8)),
            (ItemModel::Staff, "item.wand.staff.pine") => Some(ItemSurface::Staff(9)),
            (ItemModel::Staff, "item.wand.staff.teak") => Some(ItemSurface::Staff(10)),
            (ItemModel::Staff, "item.wand.staff.walnut") => Some(ItemSurface::Staff(11)),
            (ItemModel::Staff, "item.wand.staff.willow") => Some(ItemSurface::Staff(12)),
            (ItemModel::Staff, "item.wand.staff.yew") => Some(ItemSurface::Staff(13)),
            (ItemModel::Ring, "item.ring.agate") => Some(ItemSurface::Ring(0)),
            (ItemModel::Ring, "item.ring.bronze") => Some(ItemSurface::Ring(1)),
            (ItemModel::Ring, "item.ring.coral") => Some(ItemSurface::Ring(2)),
            (ItemModel::Ring, "item.ring.diamond") => Some(ItemSurface::Ring(3)),
            (ItemModel::Ring, "item.ring.emerald") => Some(ItemSurface::Ring(4)),
            (ItemModel::Ring, "item.ring.ivory") => Some(ItemSurface::Ring(5)),
            (ItemModel::Ring, "item.ring.jade") => Some(ItemSurface::Ring(6)),
            (ItemModel::Ring, "item.ring.opal") => Some(ItemSurface::Ring(7)),
            (ItemModel::Ring, "item.ring.ruby") => Some(ItemSurface::Ring(8)),
            (ItemModel::Ring, "item.ring.sapphire") => Some(ItemSurface::Ring(9)),
            (ItemModel::Ring, "item.ring.topaz") => Some(ItemSurface::Ring(10)),
            (ItemModel::Ring, "item.ring.amethyst") => Some(ItemSurface::Ring(11)),
            (ItemModel::Ring, "item.ring.garnet") => Some(ItemSurface::Ring(12)),
            (ItemModel::Ring, "item.ring.quartz") => Some(ItemSurface::Ring(13)),
            _ => None,
        };
        if let Some(surface) = surface {
            return surface;
        }
        let Some((parent, _)) = key.rsplit_once('.') else {
            return ItemSurface::Neutral;
        };
        key = parent;
    }
}

pub(crate) fn surface_for_asset(
    palette: &Palette,
    kind: ItemModel,
    asset: Option<&str>,
) -> ItemSurface {
    if palette.resolve(items(), asset).is_some() {
        asset.map_or(ItemSurface::Neutral, |key| item_surface(kind, key))
    } else {
        ItemSurface::Neutral
    }
}

fn creatures() -> &'static AssetTable<CreatureModel> {
    static TABLE: OnceLock<AssetTable<CreatureModel>> = OnceLock::new();
    TABLE.get_or_init(|| {
        AssetTable::new([
            ("creature", CreatureModel::Placeholder),
            ("creature.humanoid", CreatureModel::Humanoid),
            ("creature.humanoid.human", CreatureModel::Human),
            ("creature.humanoid.human.warrior", CreatureModel::Human),
            ("creature.humanoid.human.mage", CreatureModel::Mage),
            ("creature.humanoid.sentinel", CreatureModel::Sentinel),
            ("creature.humanoid.sentinel.red", CreatureModel::RedSentinel),
            (
                "creature.humanoid.sentinel.blue",
                CreatureModel::BlueSentinel,
            ),
            ("creature.humanoid.mage", CreatureModel::Mage),
            ("creature.humanoid.mage.red", CreatureModel::RedMage),
            ("creature.humanoid.scout", CreatureModel::Scout),
            ("creature.construct.stone", CreatureModel::Guardian),
            ("creature.goblin", CreatureModel::Hobgoblin),
            ("creature.goblin.hobgoblin", CreatureModel::Hobgoblin),
            ("creature.undead.zombie", CreatureModel::Zombie),
            ("creature.undead.vampire", CreatureModel::Vampire),
            ("creature.ape.yeti", CreatureModel::Yeti),
            ("creature.troll", CreatureModel::Troll),
            ("creature.bird", CreatureModel::Bird),
            ("creature.bird.emu", CreatureModel::Emu),
            ("creature.equine", CreatureModel::Equine),
            ("creature.equine.quagga", CreatureModel::Quagga),
            ("creature.equine.unicorn", CreatureModel::Unicorn),
            ("creature.equine.unicorn.black", CreatureModel::BlackUnicorn),
            ("creature.centaur", CreatureModel::Centaur),
            ("creature.serpent", CreatureModel::Serpent),
            ("creature.serpent.snake", CreatureModel::Snake),
            ("creature.elemental.ember", CreatureModel::Wisp),
            ("creature.elemental.ice", CreatureModel::Ice),
            ("creature.humanoid.orc", CreatureModel::Orc),
            ("creature.fey.leprechaun", CreatureModel::Leprechaun),
            ("creature.fey.nymph", CreatureModel::Nymph),
            ("creature.medusa", CreatureModel::Medusa),
            ("creature.undead.phantom", CreatureModel::Phantom),
            ("creature.undead.wraith", CreatureModel::Wraith),
            ("creature.bat", CreatureModel::Bat),
            ("creature.bird.kestrel", CreatureModel::Kestrel),
            ("creature.griffin", CreatureModel::Griffin),
            ("creature.dragon", CreatureModel::Dragon),
            ("creature.jabberwock", CreatureModel::Jabberwock),
            ("creature.aquator", CreatureModel::Aquator),
            ("creature.xeroc", CreatureModel::Xeroc),
            ("creature.plant.venus.flytrap", CreatureModel::Flytrap),
            ("creature.serpent.rattlesnake", CreatureModel::Rattlesnake),
        ])
    })
}

fn items() -> &'static AssetTable<ItemModel> {
    static TABLE: OnceLock<AssetTable<ItemModel>> = OnceLock::new();
    TABLE.get_or_init(|| {
        AssetTable::new([
            ("item.weapon", ItemModel::ShortSword),
            ("item.weapon.mace", ItemModel::Mace),
            ("item.weapon.sword.long", ItemModel::LongSword),
            ("item.weapon.bow.short", ItemModel::Shortbow),
            ("item.weapon.arrow", ItemModel::Arrow),
            ("item.weapon.spear", ItemModel::Spear),
            ("item.weapon.dart", ItemModel::Dart),
            ("item.weapon.shuriken", ItemModel::Shuriken),
            ("item.food.fruit", ItemModel::Fruit),
            ("item.tool.torch", ItemModel::Torch),
            ("item.wand.staff", ItemModel::Staff),
            ("item.weapon.greatsword", ItemModel::Greatsword),
            ("item.weapon.dagger", ItemModel::Dagger),
            ("item.armor", ItemModel::RingMail),
            ("item.armor.leather", ItemModel::LeatherArmor),
            ("item.armor.leather.studded", ItemModel::StuddedLeather),
            ("item.armor.mail", ItemModel::RingMail),
            ("item.armor.mail.ring", ItemModel::RingMail),
            ("item.armor.mail.scale", ItemModel::ScaleMail),
            ("item.armor.mail.chain", ItemModel::ChainMail),
            ("item.armor.plate", ItemModel::PlateMail),
            ("item.armor.plate.splint", ItemModel::SplintMail),
            ("item.armor.plate.banded", ItemModel::BandedMail),
            ("item.bandage", ItemModel::Bandage),
            ("item.food", ItemModel::Ration),
            ("item.corpse", ItemModel::Corpse),
            ("item.seal", ItemModel::Seal),
            ("item.coin", ItemModel::Coins),
            ("item.tablet", ItemModel::Tablet),
            ("item.potion", ItemModel::Potion),
            ("item.ring", ItemModel::Ring),
            ("item.amulet", ItemModel::Amulet),
            ("item.scroll", ItemModel::Scroll),
            ("item.spellbook", ItemModel::Spellbook),
            ("item.wand", ItemModel::Wand),
            ("item.gem", ItemModel::Gem),
            ("item.tool", ItemModel::Pick),
            ("item.pouch", ItemModel::Pouch),
        ])
    })
}

/// Resolve only palette-approved appearance, with dotted-prefix inheritance.
pub fn actor_model(palette: &Palette, actor: &ActorView) -> CreatureModel {
    creature_model(palette, actor.asset.as_deref())
}

pub fn creature_model(palette: &Palette, asset: Option<&str>) -> CreatureModel {
    palette
        .resolve(creatures(), asset)
        .copied()
        .unwrap_or(CreatureModel::Placeholder)
}

/// Class is a disclosed shape fallback; display text never identifies a model.
pub fn item_model(palette: &Palette, item: &ItemView) -> ItemModel {
    item_shape(palette, item.class, item.asset.as_deref())
}

/// Use only the sampled visual recipe, never the real item's class or corpse.
pub fn object_model(palette: &Palette, presentation: &ObjectPresentation) -> ItemModel {
    match presentation.kind {
        ObjectPresentationKind::Item { class } => {
            item_shape(palette, class, presentation.asset.as_deref())
        }
        ObjectPresentationKind::Stairs => ItemModel::Stairs,
    }
}

fn item_shape(palette: &Palette, class: ItemClass, asset: Option<&str>) -> ItemModel {
    if let Some(model) = palette.resolve(items(), asset) {
        return *model;
    }
    match class {
        ItemClass::Weapon => ItemModel::ShortSword,
        ItemClass::Armor => ItemModel::RingMail,
        ItemClass::Potion => ItemModel::Potion,
        ItemClass::Food => ItemModel::Ration,
        ItemClass::Corpse => ItemModel::Corpse,
        ItemClass::Tool => ItemModel::Pick,
        ItemClass::Amulet => ItemModel::Amulet,
        ItemClass::Ring => ItemModel::Ring,
        ItemClass::Scroll => ItemModel::Scroll,
        ItemClass::Spellbook => ItemModel::Spellbook,
        ItemClass::Wand => ItemModel::Wand,
        ItemClass::Coin => ItemModel::Coins,
        ItemClass::Gem => ItemModel::Gem,
        ItemClass::Misc => ItemModel::Placeholder,
    }
}

/// Visible equipment as the renderer places it on a body, from the disclosed
/// worn list. Slots the renderer cannot place are ignored, never moved to
/// another part of the body; an empty list leaves the body's own costume.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Wearing {
    pub head: Option<ItemModel>,
    pub body: Option<ItemModel>,
    pub hands: Option<ItemModel>,
    pub feet: Option<ItemModel>,
    pub back: Option<ItemModel>,
    pub shield: Option<ItemModel>,
    /// Main and off hand, by weapon socket occurrence.
    pub weapons: [Option<ItemModel>; 2],
    pub weapon_surfaces: [ItemSurface; 2],
}

impl Wearing {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Resolve each disclosed piece through the palette, as ground items are.
pub fn wearing(palette: &Palette, worn: &[WornView]) -> Wearing {
    let mut result = Wearing::default();
    for piece in worn {
        let model = palette
            .resolve(items(), piece.asset.as_deref())
            .copied()
            .unwrap_or(match piece.class {
                ItemClass::Weapon => ItemModel::ShortSword,
                _ => ItemModel::RingMail,
            });
        let slot = match (piece.slot.as_str(), piece.occurrence) {
            ("head_armor", 0) => &mut result.head,
            ("body_armor", 0) => &mut result.body,
            ("hands_armor", 0) => &mut result.hands,
            ("feet_armor", 0) => &mut result.feet,
            ("cloak", 0) => &mut result.back,
            ("shield", 0) => &mut result.shield,
            ("weapon", n @ (0 | 1)) => &mut result.weapons[usize::from(n)],
            _ => continue,
        };
        *slot = Some(model);
        if piece.slot == "weapon" && piece.occurrence < 2 {
            result.weapon_surfaces[usize::from(piece.occurrence)] =
                surface_for_asset(palette, model, piece.asset.as_deref());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use tor_protocol::{PaletteBody, PaletteUpdate};

    #[test]
    fn rogue_distinct_physical_shapes_resolve_without_names_or_identity() {
        let cases = [
            (
                "item.weapon.sword.long",
                ItemClass::Weapon,
                ItemModel::LongSword,
            ),
            ("item.weapon.dagger", ItemClass::Weapon, ItemModel::Dagger),
            (
                "item.weapon.bow.short",
                ItemClass::Weapon,
                ItemModel::Shortbow,
            ),
            ("item.weapon.arrow", ItemClass::Weapon, ItemModel::Arrow),
            (
                "item.weapon.greatsword",
                ItemClass::Weapon,
                ItemModel::Greatsword,
            ),
            ("item.weapon.spear", ItemClass::Weapon, ItemModel::Spear),
            ("item.weapon.dart", ItemClass::Weapon, ItemModel::Dart),
            (
                "item.weapon.shuriken",
                ItemClass::Weapon,
                ItemModel::Shuriken,
            ),
            ("item.food.fruit", ItemClass::Food, ItemModel::Fruit),
            ("item.tool.torch", ItemClass::Tool, ItemModel::Torch),
            ("item.wand.staff", ItemClass::Wand, ItemModel::Staff),
            ("item.wand", ItemClass::Wand, ItemModel::Wand),
        ];
        let mut palette = Palette::default();
        palette.apply(&PaletteUpdate {
            revision: 1,
            body: PaletteBody::Full {
                assets: cases.iter().map(|(asset, _, _)| (*asset).into()).collect(),
            },
        });
        for (asset, class, expected) in cases {
            assert_eq!(
                item_shape(&palette, class, Some(asset)),
                expected,
                "{asset}"
            );
            // The worn path must use the same public cosmetic asset, including
            // devices whose disclosed class is Wand rather than Weapon.
            if matches!(class, ItemClass::Weapon | ItemClass::Wand) {
                assert_eq!(
                    wearing(&palette, &[piece("weapon", 0, class, asset)]).weapons[0],
                    Some(expected),
                    "{asset}"
                );
            }
        }
        // Absence from a current palette still obeys the disclosed class
        // fallback; new precise recipes cannot bypass palette authority.
        assert_eq!(
            item_shape(
                &Palette::default(),
                ItemClass::Weapon,
                Some("item.weapon.bow.short")
            ),
            ItemModel::ShortSword
        );
    }

    #[test]
    fn hierarchical_surfaces_are_family_bounded_and_worn_devices_use_public_assets() {
        assert_eq!(
            item_surface(ItemModel::Potion, "item.potion.amber"),
            ItemSurface::Potion(0)
        );
        assert_eq!(
            item_surface(ItemModel::Potion, "item.potion.amber.cloudy"),
            ItemSurface::Potion(0)
        );
        assert_eq!(
            item_surface(ItemModel::Ring, "item.ring.ruby"),
            ItemSurface::Ring(8)
        );
        assert_eq!(
            item_surface(ItemModel::Potion, "item.ring.ruby"),
            ItemSurface::Neutral
        );
        assert_eq!(
            item_surface(ItemModel::Potion, "item.potion.unknown"),
            ItemSurface::Neutral
        );
        assert_eq!(
            item_surface(ItemModel::Potion, "potion of healing"),
            ItemSurface::Neutral
        );
        let mut palette = Palette::default();
        palette.apply(&PaletteUpdate {
            revision: 1,
            body: PaletteBody::Full {
                assets: ["item.wand.silver".into(), "item.wand.staff.oak".into()]
                    .into_iter()
                    .collect(),
            },
        });
        let mut wand = piece("weapon", 0, ItemClass::Wand, "item.wand.silver");
        wand.appearance = "arbitrary translated public label".into();
        let mut staff = piece("weapon", 1, ItemClass::Wand, "item.wand.staff.oak");
        staff.appearance = "unrelated display label".into();
        let result = wearing(&palette, &[wand, staff]);
        assert_eq!(
            result.weapon_surfaces,
            [ItemSurface::Wand(10), ItemSurface::Staff(8)]
        );
        assert_eq!(
            result.weapons,
            [Some(ItemModel::Wand), Some(ItemModel::Staff)]
        );
        assert_eq!(
            surface_for_asset(
                &Palette::default(),
                ItemModel::Potion,
                Some("item.potion.amber")
            ),
            ItemSurface::Neutral
        );
    }

    /// The monsters the Rogue scenario names in its dungeon asset list; the
    /// player's own body is named elsewhere in the manifest.
    fn rogue_creature_assets() -> BTreeSet<String> {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../scenarios/rogue-exploration/scenario.toml");
        let text = std::fs::read_to_string(&manifest).expect("rogue scenario manifest");
        let dungeon = text
            .split("dungeon = [")
            .nth(1)
            .and_then(|rest| rest.split(']').next())
            .expect("dungeon asset list");
        dungeon
            .split('"')
            .filter(|token| token.starts_with("creature."))
            .map(str::to_owned)
            .collect()
    }

    fn piece(slot: &str, occurrence: u8, class: ItemClass, asset: &str) -> WornView {
        WornView {
            slot: slot.into(),
            occurrence,
            class,
            appearance: "seen".into(),
            asset: Some(asset.into()),
        }
    }

    #[test]
    fn worn_pieces_resolve_by_slot_and_unplaceable_ones_are_ignored() {
        let mut palette = Palette::default();
        palette.apply(&PaletteUpdate {
            revision: 1,
            body: PaletteBody::Full {
                assets: ["item.armor", "item.weapon"].map(String::from).into(),
            },
        });
        let worn = [
            piece("body_armor", 0, ItemClass::Armor, "item.armor.mail"),
            piece("weapon", 0, ItemClass::Weapon, "item.weapon.mace"),
            piece("weapon", 1, ItemClass::Weapon, "item.weapon"),
            piece("weapon", 2, ItemClass::Weapon, "item.weapon"),
            piece("tail_armor", 0, ItemClass::Armor, "item.armor"),
        ];
        let resolved = wearing(&palette, &worn);
        assert_eq!(
            resolved,
            Wearing {
                body: Some(ItemModel::RingMail),
                // The palette approves only "item.weapon": the mace asset falls
                // back by prefix, never past what the palette allows.
                weapons: [Some(ItemModel::ShortSword), Some(ItemModel::ShortSword)],
                ..Wearing::default()
            },
            "a third weapon socket and an unknown slot are not moved elsewhere"
        );
        assert!(
            wearing(&palette, &[]).is_empty(),
            "nothing disclosed, nothing worn"
        );
    }

    #[test]
    fn every_rogue_monster_resolves_to_its_own_model() {
        let assets = rogue_creature_assets();
        assert_eq!(assets.len(), 26, "one asset per Rogue monster: {assets:?}");
        let mut palette = Palette::default();
        palette.apply(&PaletteUpdate {
            revision: 1,
            body: PaletteBody::Full {
                assets: assets.clone(),
            },
        });
        let mut owners: BTreeMap<String, &str> = BTreeMap::new();
        for asset in &assets {
            let model = creature_model(&palette, Some(asset));
            assert!(
                !matches!(model, CreatureModel::Placeholder | CreatureModel::Humanoid),
                "{asset} has no model of its own: {model:?}"
            );
            if let Some(other) = owners.insert(format!("{model:?}"), asset) {
                panic!("{asset} and {other} share the {model:?} model");
            }
        }
    }
}
