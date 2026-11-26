//! Client-side visual construction sites and ownership, not permission to equip.
use super::equipment::{BackItem, BodyItem, Equipment, FootItem, HeadItem};
use super::outerwear::{OuterwearCut, OuterwearHood};
use super::*;
use crate::creatures::geometry::{
    equipment_footwear::FootwearAppearance, equipment_gloves::GloveAppearance,
    equipment_headwear::HeadwearKind,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum Slot {
    InnerTop,
    OuterTop,
    TorsoPlate,
    Sleeves,
    ArmPlate,
    ShoulderArmor,
    Trousers,
    ThighPlate,
    KneePlate,
    WaistPanels,
    Skirt,
    Peplum,
    Cape,
    Robe,
    Apron,
    Wrap,
    Head,
    Veil,
    HoodCollar,
    Scarf,
    Earrings,
    Necklace,
    Hands,
    Feet,
    BootTrim,
    CalfBindings,
    ArmholeTrim,
    TorsoTrim,
    Regalia,
    Cuffs,
    ShoulderFringe,
    ShoulderDisc,
    ShoulderPeaks,
    Armlets,
    Bracelets,
    Heraldry,
    Waist,
    Belt,
    Baldric,
    Pouch,
    Garter,
    HipDetail,
    ForearmCloth,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Slots(u64);

impl Slots {
    fn of(slots: &[Slot]) -> Self {
        let mut result = Self::default();
        for &slot in slots {
            result.add(slot);
        }
        result
    }

    pub(super) fn has(self, slot: Slot) -> bool {
        self.0 & (1 << slot as u8) != 0
    }

    fn add(&mut self, slot: Slot) {
        self.0 |= 1 << slot as u8;
    }

    fn join(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    fn common(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

/// A recipe's identity is distinct from the sites it occupies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GearPiece {
    Shirt(shirts::ShirtAppearance),
    Body(BodyItem),
    Head(HeadItem),
    Hands(GloveAppearance),
    Feet(FootItem),
    Back(BackItem),
}

#[derive(Clone, Copy, Default)]
pub(super) struct Claims {
    pub(super) occupied: Slots,
    replaces: Slots,
    covers: Slots,
}

impl Claims {
    fn new(occupied: &[Slot], replaces: &[Slot], covers: &[Slot]) -> Self {
        Self {
            occupied: Slots::of(occupied),
            replaces: Slots::of(replaces),
            covers: Slots::of(covers),
        }
    }

    pub(super) fn merge(&mut self, other: Self) {
        self.occupied = self.occupied.join(other.occupied);
        self.replaces = self.replaces.join(other.replaces);
        self.covers = self.covers.join(other.covers);
    }
}

impl GearPiece {
    pub(super) fn parse(name: &str) -> Option<Self> {
        GloveAppearance::parse(name)
            .map(Self::Hands)
            .or_else(|| {
                outerwear::OuterwearAppearance::parse(name)
                    .map(|kind| Self::Back(BackItem::Appearance(kind)))
            })
            .or_else(|| shirts::ShirtAppearance::parse(name).map(Self::Shirt))
            .or_else(|| {
                HeadwearKind::parse(name).map(|kind| Self::Head(HeadItem::Appearance(kind)))
            })
            .or_else(|| {
                FootwearAppearance::parse(name).map(|kind| Self::Feet(FootItem::Appearance(kind)))
            })
            .or_else(|| match name {
                "helm" => Some(Self::Head(HeadItem::SteelHelm)),
                "boots" => Some(Self::Feet(FootItem::LeatherBoots)),
                "cloak" => Some(Self::Back(BackItem::TravelCloak)),
                _ => BodyItem::NAMES
                    .iter()
                    .find_map(|&(label, item)| (label == name).then_some(Self::Body(item))),
            })
    }

    pub(super) fn claims(self) -> Claims {
        use Slot::*;
        match self {
            Self::Shirt(_) => Claims::new(
                &[InnerTop, Sleeves],
                &[
                    InnerTop,
                    OuterTop,
                    TorsoPlate,
                    Sleeves,
                    ArmPlate,
                    ShoulderArmor,
                    ArmholeTrim,
                    TorsoTrim,
                    Cuffs,
                    ForearmCloth,
                ],
                &[],
            ),
            Self::Body(body) => {
                let mut result = Claims::new(
                    &[TorsoPlate],
                    &[TorsoPlate, ArmPlate, ShoulderArmor, ThighPlate, KneePlate],
                    &[TorsoTrim, Regalia],
                );
                if body.tassets().is_some() {
                    result.occupied.add(WaistPanels);
                }
                if body.articulated_limbs() {
                    result.merge(Claims::new(
                        &[ArmPlate, ThighPlate, KneePlate],
                        &[Cuffs],
                        &[ForearmCloth],
                    ));
                } else if body.bracer().is_some() {
                    result.merge(Claims::new(&[Cuffs], &[Cuffs], &[]));
                }
                if super::super::geometry::body_armor_has_shoulder_caps(body) {
                    result.merge(Claims::new(
                        &[ShoulderArmor],
                        &[],
                        &[ShoulderFringe, ShoulderDisc, ShoulderPeaks],
                    ));
                }
                result
            }
            Self::Head(item) => {
                let mut result = Claims::new(&[Head], &[Head], &[Veil, HoodCollar]);
                if match item {
                    HeadItem::SteelHelm => true,
                    HeadItem::Appearance(kind) => kind.covers_ears(),
                } {
                    result.covers.add(Earrings);
                }
                result
            }
            Self::Hands(_) => Claims::new(&[Hands], &[Hands], &[]),
            Self::Feet(_) => Claims::new(&[Feet], &[Feet], &[BootTrim]),
            Self::Back(item) => match item {
                BackItem::TravelCloak => Claims::new(&[Cape], &[Cape], &[]),
                BackItem::Appearance(kind) => match kind.recipe().cut {
                    OuterwearCut::Drape(_) => Claims::new(&[Cape], &[Cape], &[]),
                    OuterwearCut::Robe => Claims::new(&[Robe], &[Cape, Robe], &[]),
                    OuterwearCut::Apron { .. } => Claims::new(&[Apron], &[Cape], &[]),
                    OuterwearCut::Wraps { .. } => Claims::new(&[Wrap], &[Cape], &[]),
                },
            },
        }
    }
}

struct SecondaryClaim {
    requires_free: Slots,
    claims: Claims,
}

impl SecondaryClaim {
    fn admit(self, primary: Claims) -> Claims {
        if primary.occupied.common(self.requires_free) == Slots::default() {
            self.claims
        } else {
            Claims::default()
        }
    }
}

pub(super) fn admit_hood(primary: Claims, hood: OuterwearHood) -> Claims {
    use Slot::*;
    SecondaryClaim {
        requires_free: Slots::of(&[Head]),
        claims: match hood {
            OuterwearHood::None => Claims::default(),
            OuterwearHood::Lowered => Claims::new(&[HoodCollar], &[HoodCollar], &[]),
            OuterwearHood::Raised => Claims::new(&[Head, HoodCollar], &[Head, HoodCollar], &[Veil]),
        },
    }
    .admit(primary)
}

/// Explicitly authored, so an unrelated cape cannot disown the robe's hood.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HoodMount {
    Cape,
    Robe,
}

impl HoodMount {
    fn slot(self) -> Slot {
        match self {
            Self::Cape => Slot::Cape,
            Self::Robe => Slot::Robe,
        }
    }
}

fn inventory(o: &Outfit) -> Slots {
    use Slot::*;
    let mut present = Slots::of(&[InnerTop]);
    match o.outer {
        Some(Outer::Cuirass) => present.add(TorsoPlate),
        Some(Outer::Shell { peplum, .. }) => {
            present.add(OuterTop);
            if peplum.is_some() {
                present.add(Peplum);
            }
        }
        None => {}
    }
    match o.arms {
        Arms::Plate => present = present.join(Slots::of(&[ArmPlate, ShoulderArmor])),
        Arms::Bare => {}
        _ => present.add(Sleeves),
    }
    match o.legs {
        Legs::Plate => present = present.join(Slots::of(&[ThighPlate, KneePlate])),
        Legs::Bare(_) => {}
        _ => present.add(Trousers),
    }
    for (slot, has_piece) in [
        (Skirt, o.skirt.is_some()),
        (Cape, o.cloak.is_some()),
        (Robe, o.robe.is_some()),
        (Head, o.head != Headwear::Bare),
        (
            Veil,
            o.veil.is_some() || matches!(o.head, Headwear::Veiled { .. }),
        ),
        (
            HoodCollar,
            o.neck_drape.is_some_and(|d| d.hood_mount.is_some()),
        ),
        (Scarf, o.neck_drape.is_some_and(|d| d.hood_mount.is_none())),
        (Earrings, o.trimmings.earrings.is_some()),
        (Necklace, o.trimmings.neck.is_some()),
        (Hands, matches!(o.hands, Pigment::Fixed(_))),
        (Feet, !matches!(o.feet, super::Feet::Bare(_))),
        (BootTrim, o.trimmings.boot_tops.is_some()),
        (CalfBindings, o.trimmings.calf_bindings.is_some()),
        (ArmholeTrim, o.armhole_edge.is_some()),
        (TorsoTrim, o.torso_ornament.is_some()),
        (Regalia, o.regalia.is_some()),
        (Cuffs, o.cuffs.is_some()),
        (ShoulderFringe, o.trimmings.shoulders.is_some()),
        (ShoulderDisc, o.details.shoulder_discs.is_some()),
        (ShoulderPeaks, o.trimmings.peaks.is_some()),
        (Armlets, o.trimmings.armlets.is_some()),
        (Bracelets, o.trimmings.bracelets.is_some()),
        (Heraldry, o.trimmings.heraldry.is_some()),
        (Waist, o.trimmings.waist.is_some()),
        (Belt, o.belt),
        (Baldric, o.baldric.is_some()),
        (Pouch, o.pouch.is_some()),
        (Garter, o.trimmings.garter.is_some()),
        (HipDetail, o.details.hip.is_some()),
        (ForearmCloth, o.details.sleeve_tatters.is_some()),
    ] {
        if has_piece {
            present.add(slot);
        }
    }
    present
}

#[derive(Clone, Copy)]
struct Conflicts {
    replaced: Slots,
    covered: Slots,
    lost_mount: Slots,
}

impl Conflicts {
    fn removed(self) -> Slots {
        self.replaced.join(self.covered).join(self.lost_mount)
    }
}

fn conflicts(o: &Outfit, claims: Claims) -> Conflicts {
    use Slot::*;
    let present = inventory(o);
    let mut result = Conflicts {
        replaced: claims.replaces.common(present),
        covered: claims.covers.common(present),
        lost_mount: Slots::default(),
    };
    let shoulder_mount = o.trimmings.plate_shoulder_mount.then_some(ArmPlate);
    let edges = [
        (Head, o.head_mount.map(HoodMount::slot)),
        (
            HoodCollar,
            o.neck_drape.and_then(|d| d.hood_mount).map(HoodMount::slot),
        ),
        (Peplum, Some(OuterTop)),
        (ShoulderFringe, shoulder_mount),
        (ShoulderPeaks, shoulder_mount),
        (Cuffs, present.has(ArmPlate).then_some(ArmPlate)),
    ];
    loop {
        let removed = result.removed();
        for (piece, parent) in edges {
            if present.has(piece)
                && !removed.has(piece)
                && parent.is_some_and(|slot| removed.has(slot))
            {
                result.lost_mount.add(piece);
            }
        }
        if removed == result.removed() {
            return result;
        }
    }
}

pub(super) fn apply_preset_conflicts(o: &mut Outfit, claims: Claims) {
    use Slot::*;
    if claims.replaces.join(claims.covers) == Slots::default() {
        return;
    }
    let removed = conflicts(o, claims).removed();
    if removed.has(OuterTop) || removed.has(TorsoPlate) {
        o.outer = None;
    }
    if removed.has(ShoulderDisc) {
        o.details.shoulder_discs = None;
    }
    if removed.has(ShoulderFringe) {
        o.trimmings.shoulders = None;
    }
    if removed.has(ShoulderPeaks) {
        o.trimmings.peaks = None;
    }
    if removed.has(ArmPlate) {
        o.arms = Arms::Sleeves(o.tunic);
        o.trimmings.plate_shoulder_mount = false;
    }
    if removed.has(KneePlate) {
        o.legs = Legs::Fitted(Pigment::Fixed(IRON));
    }
    if removed.has(Cuffs) {
        o.cuffs = None;
    }
    if removed.has(ArmholeTrim) {
        o.armhole_edge = None;
    }
    if removed.has(ForearmCloth) {
        o.details.sleeve_tatters = None;
    }
    if removed.has(TorsoTrim) {
        o.torso_ornament = None;
    }
    if removed.has(Regalia) {
        o.regalia = None;
    }
    if removed.has(Veil) {
        o.veil = None;
    }
    if removed.has(Head) {
        o.head = Headwear::Bare;
        o.head_mount = None;
    }
    if removed.has(HoodCollar) {
        o.neck_drape = None;
    }
    if removed.has(Earrings) {
        o.trimmings.earrings = None;
    }
    if removed.has(BootTrim) {
        o.trimmings.boot_tops = None;
    }
    if removed.has(Cape) {
        o.cloak = None;
    }
    if removed.has(Robe) {
        o.robe = None;
    }
}

struct FitRule {
    selected: Slot,
    retained: Slot,
}

impl FitRule {
    fn applies(&self, selected: Claims, retained: Slots) -> bool {
        selected.occupied.has(self.selected) && retained.has(self.retained)
    }
}

const BOOT_BELOW_KNEE: FitRule = FitRule {
    selected: Slot::Feet,
    retained: Slot::KneePlate,
};

pub(super) fn footwear_fits_below_knee(gear: Equipment, outfit: &Outfit) -> bool {
    gear.feet
        .map(GearPiece::Feet)
        .is_some_and(|piece| BOOT_BELOW_KNEE.applies(piece.claims(), inventory(outfit)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use Slot::*;

    fn study(model: CreatureModel, wardrobe: Wardrobe, woman: bool) -> Look {
        Look::with_choice(model, 17, Some(Variant { woman, ..default() })).dressed(wardrobe)
    }

    fn gear(items: &[&str]) -> Equipment {
        let mut gear = Equipment::default();
        for item in items {
            gear.select(item).unwrap();
        }
        gear
    }

    #[test]
    fn typed_catalog_and_presets_cover_all_named_selections() {
        let names: Vec<_> = ["helm", "boots", "cloak"]
            .into_iter()
            .chain(shirts::ShirtAppearance::NAMES)
            .chain(GloveAppearance::NAMES)
            .chain(HeadwearKind::NAMES)
            .chain(FootwearAppearance::NAMES)
            .chain(BodyItem::NAMES.map(|(name, _)| name))
            .chain(outerwear::OuterwearAppearance::NAMES.map(|(name, _)| name))
            .collect();
        let mut ids = Vec::new();
        for name in &names {
            let id = GearPiece::parse(name).expect("catalog recipe");
            assert!(!ids.contains(&id), "{name} duplicates a typed identity");
            assert_ne!(id.claims().occupied, Slots::default(), "{name} has no slot");
            ids.push(id);
        }
        assert_eq!(ids.len(), 51);
        assert_eq!(Wardrobe::ALL.len(), 34);
        for woman in [false, true] {
            for wardrobe in Wardrobe::ALL {
                let look = study(CreatureModel::Human, wardrobe, woman);
                let preset = Outfit::worn(&look);
                for name in &names {
                    let selected = gear(&[name]);
                    let equipped = look.clone().equipped(selected);
                    assert_eq!(equipped.equipment, selected);
                    let resolved = Outfit::worn(&equipped);
                    assert_eq!(
                        resolved.feet == preset.feet,
                        selected.feet.is_none(),
                        "{wardrobe:?}: {name}"
                    );
                    assert_eq!(
                        Outfit::worn(&equipped.equipped(Equipment::default())),
                        preset
                    );
                }
            }
        }
    }

    #[test]
    fn authored_mounts_have_both_a_parent_and_an_accessory() {
        for model in [
            CreatureModel::Human,
            CreatureModel::Scout,
            CreatureModel::Mage,
            CreatureModel::Wraith,
            CreatureModel::Medusa,
            CreatureModel::RedSentinel,
        ] {
            for woman in [false, true] {
                for wardrobe in Wardrobe::ALL {
                    let o = Outfit::worn(&study(model, wardrobe, woman));
                    let present = inventory(&o);
                    if let Some(mount) = o.head_mount {
                        assert!(
                            present.has(Head) && present.has(mount.slot()),
                            "{model:?} {wardrobe:?}"
                        );
                    }
                    if let Some(mount) = o.neck_drape.and_then(|d| d.hood_mount) {
                        assert!(
                            present.has(HoodCollar) && present.has(mount.slot()),
                            "{model:?} {wardrobe:?}"
                        );
                    }
                    if o.trimmings.plate_shoulder_mount {
                        assert!(present.has(ArmPlate) && present.has(ShoulderFringe));
                    }
                }
            }
        }
    }

    #[test]
    fn replacement_uses_mount_identity_not_co_occurrence() {
        let mage = study(CreatureModel::Mage, Wardrobe::Native, true);
        let mut outfit = Outfit::worn(&mage);
        outfit.cloak = Some(Cloak::new(Vec3::ONE, Vec3::X));
        let apron = GearPiece::parse("apron").unwrap().claims();
        let changes = conflicts(&outfit, apron);
        assert!(changes.replaced.has(Cape));
        assert!(!changes.lost_mount.has(Head));
        let dressed = gear(&["apron"]).compose(outfit);
        assert!(dressed.cloak.is_none());
        assert_eq!(dressed.head, outfit.head);
        assert_eq!(dressed.neck_drape, outfit.neck_drape);
        assert_eq!(dressed.robe, outfit.robe);
        let changes = conflicts(&outfit, GearPiece::parse("robe").unwrap().claims());
        assert!(changes.replaced.has(Robe));
        assert!(changes.lost_mount.has(Head) && changes.lost_mount.has(HoodCollar));
        assert_eq!(gear(&["robe"]).compose(outfit).head, Headwear::Bare);

        let ranger = Outfit::worn(&study(CreatureModel::Human, Wardrobe::Ranger, false));
        let changes = conflicts(&ranger, apron);
        assert!(changes.lost_mount.has(Head));
        assert!(!changes.covered.has(Head));
    }

    #[test]
    fn same_shoulder_trim_follows_its_declared_mount() {
        let valkyrie = study(CreatureModel::Human, Wardrobe::Valkyrie, true);
        let mut original = Outfit::worn(&valkyrie);
        let shirt = gear(&["t-shirt"]);
        let mounted = Outfit::worn(&valkyrie.clone().equipped(shirt));
        assert!(original.trimmings.shoulders.is_some() && original.trimmings.peaks.is_some());
        assert!(mounted.trimmings.shoulders.is_none() && mounted.trimmings.peaks.is_none());
        original.trimmings.plate_shoulder_mount = false;
        let mut independent = valkyrie;
        independent.outfit = Some(original);
        let retained = Outfit::worn(&independent.equipped(shirt));
        assert_eq!(retained.trimmings.shoulders, original.trimmings.shoulders);
        assert_eq!(retained.trimmings.peaks, original.trimmings.peaks);
    }

    #[test]
    fn only_covered_regions_suppress_unattached_accessories() {
        let medusa = Outfit::worn(&study(CreatureModel::Medusa, Wardrobe::Native, true));
        assert!(
            conflicts(&medusa, GearPiece::parse("plate-mail").unwrap().claims())
                .covered
                .has(ShoulderDisc)
        );
        assert!(
            !conflicts(&medusa, GearPiece::parse("breastplate").unwrap().claims())
                .removed()
                .has(ShoulderDisc)
        );
        assert!(
            conflicts(&medusa, GearPiece::parse("helm").unwrap().claims())
                .covered
                .has(Earrings)
        );
        assert!(
            !conflicts(&medusa, GearPiece::parse("fedora").unwrap().claims())
                .removed()
                .has(Earrings)
        );
    }

    #[test]
    fn forearm_garment_detail_yields_to_replacement_or_coverage() {
        for model in [CreatureModel::Phantom, CreatureModel::Wraith] {
            let look = study(model, Wardrobe::Native, true);
            let original = Outfit::worn(&look);
            assert!(original.details.sleeve_tatters.is_some(), "{model:?}");
            assert!(original.details.hip.is_some(), "{model:?}");
            for (item, retains_tatters) in [
                ("breastplate", true),
                ("leather", true),
                ("t-shirt", false),
                ("hawaiian-shirt", false),
                ("splint-mail", false),
                ("banded-mail", false),
                ("plate-mail", false),
            ] {
                let outfit = Outfit::worn(&look.clone().equipped(gear(&[item])));
                assert_eq!(
                    outfit.details.sleeve_tatters,
                    original.details.sleeve_tatters.filter(|_| retains_tatters),
                    "{model:?}/{item}"
                );
                assert_eq!(outfit.details.hip, original.details.hip, "{model:?}/{item}");
            }
        }
    }

    #[test]
    fn the_new_body_armor_replaces_preset_plate_but_explicit_plate_survives_a_shirt() {
        for wardrobe in [
            Wardrobe::Paladin,
            Wardrobe::Valkyrie,
            Wardrobe::Knight,
            Wardrobe::RoyalGroom,
        ] {
            let look = study(CreatureModel::Human, wardrobe, true);
            let preset = Outfit::worn(&look);
            assert_eq!(preset.outer, Some(Outer::Cuirass));
            for (pieces, expected) in [
                (&["t-shirt"][..], None),
                (&["breastplate"][..], Some(BodyItem::SteelBreastplate)),
                (
                    &["hawaiian-shirt", "plate-mail"][..],
                    Some(BodyItem::PlateMail),
                ),
            ] {
                let result = Composition::resolve(&look.clone().equipped(gear(pieces)));
                assert_eq!(result.body, expected);
                assert_ne!(result.outfit.outer, Some(Outer::Cuirass));
            }
        }
    }
}
