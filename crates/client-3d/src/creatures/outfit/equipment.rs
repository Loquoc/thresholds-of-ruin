//! Independently selected visual pieces composed over a wardrobe preset.
//! Offline previews select them by name; live play selects them from the
//! server's disclosed worn equipment, which stays server-owned.
use super::super::geometry::equipment_footwear::FootwearAppearance;
use super::super::geometry::equipment_gloves::GloveAppearance;
use super::super::geometry::equipment_headwear::HeadwearKind;
use super::outerwear::{OuterwearAppearance, OuterwearCut, OuterwearHood};
use super::shirts::ShirtAppearance;
use super::slots::{self, Claims, GearPiece, HoodClearance, Slot};
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum HeadItem {
    SteelHelm,
    Appearance(HeadwearKind),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum FootItem {
    LeatherBoots,
    Appearance(FootwearAppearance),
}
/// Torso armour, one recipe per Rogue body armour in the style guide's order.
/// The steel breastplate is plate mail's torso.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BodyItem {
    Leather,
    StuddedLeather,
    RingMail,
    ScaleMail,
    ChainMail,
    SplintMail,
    BandedMail,
    /// The plate torso alone, for fitting studies.
    SteelBreastplate,
    /// Plate mail as Rogue wears it: the plate torso with articulated arms
    /// and plated legs.
    PlateMail,
}

impl BodyItem {
    /// Preview names, as `--wear` and review scripts spell them.
    pub const NAMES: [(&'static str, Self); 9] = [
        ("leather", Self::Leather),
        ("studded-leather", Self::StuddedLeather),
        ("ring-mail", Self::RingMail),
        ("scale-mail", Self::ScaleMail),
        ("chain-mail", Self::ChainMail),
        ("splint-mail", Self::SplintMail),
        ("banded-mail", Self::BandedMail),
        ("breastplate", Self::SteelBreastplate),
        ("plate-mail", Self::PlateMail),
    ];

    pub(in crate::creatures) fn articulated_limbs(self) -> bool {
        matches!(self, Self::SplintMail | Self::BandedMail | Self::PlateMail)
    }

    pub(super) fn bracer(self) -> Option<Vec3> {
        match self {
            Self::Leather | Self::StuddedLeather | Self::RingMail => {
                Some(Vec3::new(0.43, 0.24, 0.12))
            }
            Self::ScaleMail | Self::ChainMail => Some(Vec3::new(0.55, 0.56, 0.6)),
            _ => None,
        }
    }

    /// Leather and mail give a little with the body; scales and plates do not.
    pub fn flexible(self) -> bool {
        matches!(
            self,
            Self::Leather | Self::StuddedLeather | Self::RingMail | Self::ChainMail
        )
    }

    /// The colour of the split panels it hangs below the belt, if any. The
    /// bare breastplate, a torso study, has none.
    pub fn tassets(self) -> Option<Vec3> {
        match self {
            Self::Leather | Self::StuddedLeather => Some(Vec3::new(0.43, 0.24, 0.12)),
            Self::RingMail => Some(Vec3::new(0.29, 0.16, 0.08)),
            Self::ChainMail => Some(Vec3::new(0.5, 0.51, 0.55)),
            Self::ScaleMail | Self::SplintMail | Self::BandedMail => {
                Some(Vec3::new(0.54, 0.56, 0.6))
            }
            Self::PlateMail => Some(Vec3::new(0.6, 0.62, 0.66)),
            Self::SteelBreastplate => None,
        }
    }

    /// The recipe for a disclosed armour model; unrecognised armour keeps the
    /// generic plate rather than guessing a lighter style.
    pub fn of(model: tor_client_3d_model::ItemModel) -> Self {
        use tor_client_3d_model::ItemModel as M;
        match model {
            M::LeatherArmor => Self::Leather,
            M::StuddedLeather => Self::StuddedLeather,
            M::RingMail => Self::RingMail,
            M::ScaleMail => Self::ScaleMail,
            M::ChainMail => Self::ChainMail,
            M::SplintMail => Self::SplintMail,
            M::BandedMail => Self::BandedMail,
            M::PlateMail => Self::PlateMail,
            _ => Self::SteelBreastplate,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BackItem {
    TravelCloak,
    Appearance(OuterwearAppearance),
}

/// Selection is independent of the underlying preset. None restores that
/// preset's piece; it does not erase it or mutate the wearer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Equipment {
    pub shirt: Option<ShirtAppearance>,
    pub hands: Option<GloveAppearance>,
    pub head: Option<HeadItem>,
    pub feet: Option<FootItem>,
    pub back: Option<BackItem>,
    pub body: Option<BodyItem>,
}

/// Renderer-local resolution: preset replacements and independent outer layers
/// travel together. Geometry must consume this result rather than interpreting
/// raw selection presence as a particular item recipe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Composition {
    pub outfit: Outfit,
    pub body: Option<BodyItem>,
    /// An independent armor panel over an authored skirt stack.
    pub tassets: Option<Skirt>,
    /// Retain both pieces while fitting the footwear below the knee.
    pub footwear_below_knee: bool,
}

impl Composition {
    pub fn resolve(look: &Look) -> Self {
        let preset = look.outfit.unwrap_or_else(|| Outfit::native(look));
        let mut outfit = look
            .equipment
            .shirt
            .map_or(preset, |shirt| shirt.compose(preset, look.v.woman));
        if let Some(HipDecoration::TunicTatters { shade }) = outfit.details.hip {
            // Cut the damage into the tunic's own drape, after resolving its
            // dye but before adding armour over it. With no cloth to cut, there
            // is no decoration to emit or carry into armour composition.
            outfit.details.hip = None;
            if let Some(skirt) = &mut outfit.skirt {
                skirt.length *= 1.4;
                skirt.hem = Hem::Points {
                    count: 5,
                    depth: 0.32,
                };
                skirt.color = outfit.tunic * shade;
            }
        }
        let mut outfit = look
            .equipment
            .compose_with_clearance(outfit, HoodClearance::for_style(look.style));
        let body = look.equipment.body;
        let mut armor_panel = None;
        if let Some(color) = body.and_then(BodyItem::tassets) {
            let plated_legs = outfit.legs == Legs::Plate;
            if outfit
                .skirt
                .is_some_and(|skirt| skirt.layers.iter().any(Option::is_some))
            {
                armor_panel = Some(tassets(color, None, plated_legs));
            } else {
                outfit.skirt = Some(tassets(color, outfit.skirt, plated_legs));
            }
        }
        Self {
            footwear_below_knee: slots::footwear_fits_below_knee(look.equipment, &outfit),
            outfit,
            body,
            tassets: armor_panel,
        }
    }
}

impl Equipment {
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }

    /// The pieces this renderer can fit for disclosed visible equipment. Each
    /// disclosed region gets its one current recipe; undisclosed regions keep
    /// the body's own costume rather than being stripped.
    pub fn disclosed(wearing: &tor_client_3d_model::Wearing) -> Self {
        Self {
            shirt: wearing
                .shirt_look
                .and_then(|look| ShirtAppearance::parse(look.name())),
            hands: wearing.hands.map(|_| {
                wearing
                    .hands_look
                    .and_then(|look| GloveAppearance::parse(look.name()))
                    .unwrap_or(GloveAppearance::Old)
            }),
            head: wearing.head.map(|_| {
                wearing
                    .head_look
                    .and_then(|look| HeadwearKind::parse(look.name()))
                    .map_or(HeadItem::SteelHelm, HeadItem::Appearance)
            }),
            feet: wearing.feet.map(|_| {
                wearing
                    .feet_look
                    .and_then(|look| FootwearAppearance::parse(look.name()))
                    .map_or(FootItem::LeatherBoots, FootItem::Appearance)
            }),
            back: wearing.back.map(|_| {
                wearing
                    .back_look
                    .and_then(|look| OuterwearAppearance::parse(look.name()))
                    .map_or(BackItem::TravelCloak, BackItem::Appearance)
            }),
            body: wearing.body.map(BodyItem::of),
        }
    }

    /// Renderer attachment support only, never permission to equip an item.
    /// The centaur has a humanoid head but no boot-shaped feet. Other adapters
    /// can add supported regions without granting simulation capabilities.
    pub fn for_body(self, look: &Look) -> Self {
        if super::dresses(look) {
            self
        } else if look.plan == Plan::Centaur {
            Self {
                head: self.head,
                hands: self.hands,
                ..Self::default()
            }
        } else {
            Self::default()
        }
    }

    pub fn outerwear(self) -> Option<OuterwearAppearance> {
        match self.back {
            Some(BackItem::Appearance(kind)) => Some(kind),
            _ => None,
        }
    }

    pub fn headwear(self) -> Option<HeadwearKind> {
        self.head.map(|item| match item {
            HeadItem::SteelHelm => HeadwearKind::Visored,
            HeadItem::Appearance(kind) => kind,
        })
    }

    pub fn footwear(self) -> Option<FootwearAppearance> {
        match self.feet {
            Some(FootItem::Appearance(kind)) => Some(kind),
            _ => None,
        }
    }

    pub fn select(&mut self, name: &str) -> Result<(), String> {
        match GearPiece::parse(name) {
            Some(GearPiece::Shirt(item)) => self.shirt = Some(item),
            Some(GearPiece::Body(item)) => self.body = Some(item),
            Some(GearPiece::Head(item)) => self.head = Some(item),
            Some(GearPiece::Hands(item)) => self.hands = Some(item),
            Some(GearPiece::Feet(item)) => self.feet = Some(item),
            Some(GearPiece::Back(item)) => self.back = Some(item),
            None => {
                return Err(format!(
                    "unknown equipment {name}; expected helm, boots, cloak, a study hat/footwear name, or body armour ({})",
                    BodyItem::NAMES.map(|(spelled, _)| spelled).join(", ")
                ));
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn compose(self, base: Outfit) -> Outfit {
        self.compose_with_clearance(base, HoodClearance::Standard)
    }

    fn compose_with_clearance(self, mut base: Outfit, clearance: HoodClearance) -> Outfit {
        let back_recipe = self.outerwear().map(OuterwearAppearance::recipe);
        let head_claim = self
            .head
            .map(GearPiece::Head)
            .map(GearPiece::claims)
            .unwrap_or_default();
        let requested_hood = back_recipe.map_or(OuterwearHood::None, |recipe| recipe.hood);
        let fitted_hood = clearance.fit(requested_hood);
        let hood_claim = slots::admit_hood(head_claim, fitted_hood);
        let mut claims = Claims::default();
        for gear in [
            self.body.map(GearPiece::Body),
            self.hands.map(GearPiece::Hands),
            self.feet.map(GearPiece::Feet),
            self.back.map(GearPiece::Back),
        ]
        .into_iter()
        .flatten()
        {
            claims.merge(gear.claims());
        }
        claims.merge(head_claim);
        claims.merge(hood_claim);
        slots::apply_preset_conflicts(&mut base, claims);
        if let Some(item) = self.body {
            if item.articulated_limbs() {
                base.arms = Arms::Plate;
                base.legs = Legs::Plate;
            } else if let Some(color) = item.bracer() {
                base.cuffs = Some(color);
            }
        }
        if let Some(item) = self.head {
            base.head = match item {
                HeadItem::SteelHelm => Headwear::Helm,
                HeadItem::Appearance(kind) => Headwear::Equipment(kind),
            };
        }
        if let Some(item) = self.feet {
            base.feet = Feet::Boots(Boots {
                color: Pigment::Fixed(LEATHER * 0.75),
                tall: false,
                heel: false,
                buckles: true,
                cuffed: false,
            });
            if let FootItem::Appearance(kind) = item {
                base.feet = Feet::Boots(Boots {
                    color: Pigment::Fixed(kind.color()),
                    tall: false,
                    heel: kind.heeled(),
                    buckles: false,
                    cuffed: false,
                });
            }
        }
        if let Some(kind) = self.hands {
            base.hands = Pigment::Fixed(kind.color());
        }
        match self.back {
            Some(BackItem::TravelCloak) => {
                base.cloak = Some(Cloak::new(
                    Vec3::new(0.18, 0.29, 0.23),
                    Vec3::new(0.48, 0.40, 0.25),
                ));
            }
            Some(BackItem::Appearance(_)) => {
                let recipe = back_recipe.expect("appearance has a recipe");
                base.cloak = recipe.cloak();
                if let Some(robe) = recipe.robe() {
                    base.robe = Some(robe);
                }
                if hood_claim.occupied.has(Slot::HoodCollar) {
                    let mount = if recipe.cut == OuterwearCut::Robe {
                        HoodMount::Robe
                    } else {
                        HoodMount::Cape
                    };
                    base.neck_drape = Some(NeckDrape {
                        color: recipe.cloth,
                        tail: false,
                        meet_neckline: false,
                        hood_mount: Some(mount),
                    });
                    if hood_claim.occupied.has(Slot::Head) {
                        base.head = Headwear::Hood {
                            color: recipe.cloth,
                            pointed: false,
                        };
                        base.head_mount = Some(mount);
                    } else if requested_hood != fitted_hood && base.head == Headwear::Bare {
                        base.head = Headwear::HoodDown(recipe.cloth);
                        base.head_mount = Some(mount);
                    }
                }
            }
            None => {}
        }
        base
    }
}

/// Split panels over the wearer's own lower garment, which shows beneath them
/// as a longer layer; without one, the panels hang over the trousers alone.
/// Over plated thighs both layers flare to clear the cuisses.
fn tassets(color: Vec3, under: Option<Skirt>, plated_legs: bool) -> Skirt {
    let fullness = if plated_legs { 1.42 } else { 1.0 };
    let panels = Skirt {
        color,
        length: 0.17,
        split: 0.09,
        trim: Some(color * 0.7),
        cutaway: false,
        fullness,
        ..Skirt::PLAIN
    };
    let Some(under) = under else {
        return panels;
    };
    Skirt {
        length: (under.length * 0.75).min(0.19),
        split: under.split.max(0.09),
        layers: [
            Some(Layer {
                longer: (under.length * 0.25).max(0.04),
                split: Some(under.split),
                fullness: plated_legs.then_some(fullness),
                hem: under.hem,
                ..Layer::petticoat(under.color, under.sheer)
            }),
            None,
        ],
        ..panels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wearer_pigments_preserve_shading_and_fixed_color_identity() {
        let donor = Look::of(CreatureModel::Troll, 42);
        let recipe = Outfit::native(&donor);
        let recipient_skin = Vec3::new(0.72, 0.35, 0.23);
        assert_eq!(recipe.hands.resolve(donor.skin), donor.skin);
        assert_eq!(recipe.pants(donor.skin), donor.skin);
        assert_eq!(recipe.boot(donor.skin), donor.skin * 0.88);
        assert_eq!(recipe.hands.resolve(recipient_skin), recipient_skin);
        assert_eq!(recipe.pants(recipient_skin), recipient_skin);
        assert_eq!(recipe.boot(recipient_skin), recipient_skin * 0.88);
        let mut fixed = recipe;
        fixed.hands = Pigment::Fixed(donor.skin);
        fixed.legs = Legs::Fitted(Pigment::Fixed(donor.skin));
        fixed.feet = Feet::Bare(Pigment::Fixed(donor.skin));
        assert_eq!(fixed.hands.resolve(recipient_skin), donor.skin);
        assert_eq!(fixed.pants(recipient_skin), donor.skin);
        assert_eq!(fixed.boot(recipient_skin), donor.skin);
        assert_ne!(fixed, recipe, "cache identity retains pigment meaning");
    }

    #[test]
    fn native_boot_recipes_resolve_skin_for_the_wearer() {
        let recipient = Vec3::new(0.72, 0.35, 0.23);
        for (model, shade) in [
            (CreatureModel::Zombie, 0.8),
            (CreatureModel::Guardian, 0.85),
            (CreatureModel::Yeti, 1.0),
        ] {
            let donor = Look::of(model, 42);
            let recipe = Outfit::native(&donor);
            assert_eq!(recipe.boot(donor.skin), donor.skin * shade);
            assert_eq!(recipe.boot(recipient), recipient * shade, "{model:?}");
            let mut fixed = recipe;
            let Feet::Boots(mut boots) = recipe.feet else {
                panic!("native boot recipe")
            };
            boots.color = Pigment::Fixed(donor.skin * shade);
            fixed.feet = Feet::Boots(boots);
            assert_eq!(fixed.boot(recipient), donor.skin * shade);
            assert_ne!(
                fixed, recipe,
                "equal donor RGB must not erase the skin binding"
            );
            let mut gear = Equipment::default();
            gear.select("boots").unwrap();
            let covered = gear.compose(recipe);
            assert_eq!(covered.boot(recipient), LEATHER * 0.75);
            assert_eq!(Equipment::default().compose(recipe), recipe);
        }
    }

    #[test]
    fn native_chest_details_are_recipe_owned_and_covered_without_mutation() {
        for model in [
            CreatureModel::Human,
            CreatureModel::Mage,
            CreatureModel::Hobgoblin,
            CreatureModel::Zombie,
            CreatureModel::Vampire,
        ] {
            let look = Look::with_choice(
                model,
                42,
                Some(Variant {
                    woman: true,
                    ..default()
                }),
            );
            let original = Outfit::native(&look);
            let detail = original.torso_ornament.expect("native sewn detail");
            assert!(matches!(
                (model, detail),
                (CreatureModel::Human, TorsoOrnament::LacedFront { .. })
                    | (CreatureModel::Mage, TorsoOrnament::Placket { .. })
                    | (CreatureModel::Hobgoblin, TorsoOrnament::RivetedVest { .. })
                    | (CreatureModel::Zombie, TorsoOrnament::StitchedFront { .. })
                    | (CreatureModel::Vampire, TorsoOrnament::HighCollar { .. })
            ));
            let mut explicit = look.clone();
            explicit.outfit = Some(original);
            let resolved = Outfit::worn(&look);
            assert_eq!(Outfit::worn(&explicit), resolved);
            let mut equipment = Equipment::default();
            equipment.select("breastplate").unwrap();
            let covered = equipment.compose(original);
            assert!(covered.torso_ornament.is_none());
            assert_eq!(
                Outfit {
                    torso_ornament: Some(detail),
                    ..covered
                },
                original
            );
            assert_eq!(Equipment::default().compose(original), original);
        }
    }

    #[test]
    fn tunic_damage_does_not_cut_the_armour_or_invent_cloth() {
        let look = Look::of(CreatureModel::Zombie, 42);
        let mut gear = Equipment::default();
        gear.select("ring-mail").unwrap();
        let equipped = Outfit::worn(&look.clone().equipped(gear));
        let panels = equipped.skirt.unwrap();
        assert_eq!(panels.hem, Hem::Even);
        assert!(matches!(
            panels.layers[0].unwrap().hem,
            Hem::Points { count: 5, .. }
        ));

        let mut no_skirt = look;
        let mut recipe = Outfit::native(&no_skirt);
        recipe.skirt = None;
        no_skirt.outfit = Some(recipe);
        assert!(Outfit::worn(&no_skirt).skirt.is_none());
        let panels = Outfit::worn(&no_skirt.equipped(gear)).skirt.unwrap();
        assert_eq!(panels.hem, Hem::Even);
        assert_eq!(panels.layers, [None, None]);
    }

    #[test]
    fn resolved_composition_keeps_outer_recipe_and_covered_details_together() {
        let base = Look::with_choice(
            CreatureModel::Human,
            17,
            Some(Variant {
                woman: true,
                ..default()
            }),
        )
        .dressed(Wardrobe::RoyalPrincess);
        let original = Composition::resolve(&base);
        assert!(original.body.is_none());
        assert!(original.outfit.torso_ornament.is_some());
        let mut equipment = Equipment::default();
        equipment.select("breastplate").unwrap();
        let equipped = base.clone().equipped(equipment);
        let resolved = Composition::resolve(&equipped);
        assert_eq!(resolved.body, Some(BodyItem::SteelBreastplate));
        assert!(resolved.outfit.torso_ornament.is_none());
        assert_eq!(resolved.outfit.skirt, original.outfit.skirt);
        assert_eq!(resolved.outfit.bodice, original.outfit.bodice);
        assert_eq!(
            Composition::resolve(&equipped.equipped(Equipment::default())),
            original
        );
    }

    #[test]
    fn native_earrings_follow_outfit_ownership_and_helm_removal() {
        for model in [CreatureModel::Nymph, CreatureModel::Medusa] {
            let native = Look::with_choice(
                model,
                17,
                Some(Variant {
                    woman: true,
                    ..default()
                }),
            );
            let earrings = Composition::resolve(&native).outfit.trimmings.earrings;
            assert!(
                earrings.is_some(),
                "{model:?}: native jewelry must belong to the resolved outfit"
            );
            let royal = native.clone().dressed(Wardrobe::RoyalPrincess);
            assert_eq!(
                Composition::resolve(&royal).outfit.trimmings.earrings,
                Outfit::from_wardrobe(Wardrobe::RoyalPrincess, &native)
                    .trimmings
                    .earrings
            );
            let mut equipment = Equipment::default();
            equipment.select("helm").unwrap();
            let helmed = native.clone().equipped(equipment);
            assert!(
                Composition::resolve(&helmed)
                    .outfit
                    .trimmings
                    .earrings
                    .is_none()
            );
            assert_eq!(
                Composition::resolve(&helmed.equipped(Equipment::default()))
                    .outfit
                    .trimmings
                    .earrings,
                earrings
            );
        }
    }

    #[test]
    fn equipment_overrides_only_its_regions_and_preserves_the_preset() {
        let look = Look::with_choice(
            CreatureModel::Human,
            17,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        let preset = Outfit::from_wardrobe(Wardrobe::RoyalPrincess, &look);
        let mut equipment = Equipment::default();
        equipment.select("boots").unwrap();
        let boots = equipment.compose(preset);
        assert!(
            matches!(boots.feet, Feet::Boots(_)),
            "equipped boots replace sandals"
        );
        let mut restored = boots;
        restored.feet = preset.feet;
        restored.trimmings.boot_tops = preset.trimmings.boot_tops;
        assert_eq!(
            restored, preset,
            "footwear must not change the gown or crown"
        );
        equipment.select("helm").unwrap();
        let helm = equipment.compose(preset);
        assert_eq!(helm.head, Headwear::Helm);
        assert!(
            helm.trimmings.earrings.is_none(),
            "closed helm covers earrings"
        );
        assert_eq!(helm.trimmings.neck, preset.trimmings.neck);
        equipment.select("cloak").unwrap();
        let all = equipment.compose(preset);
        assert!(all.cloak.is_some());
        assert_eq!(all.skirt, preset.skirt);
        assert_eq!(all.bodice, preset.bodice);
        equipment.head = None;
        let removed_helm = equipment.compose(preset);
        assert_eq!(removed_helm.head, preset.head);
        assert_eq!(removed_helm.trimmings.earrings, preset.trimmings.earrings);
        assert_eq!(removed_helm.feet, all.feet);
        assert_eq!(removed_helm.cloak, all.cloak);
        assert_eq!(Equipment::default().compose(preset), preset);
    }

    #[test]
    fn helm_suppresses_only_the_hood_base_and_removal_restores_it() {
        let mut equipment = Equipment::default();
        equipment.select("helm").unwrap();
        for model in [
            CreatureModel::Mage,
            CreatureModel::Human,
            CreatureModel::Orc,
        ] {
            let look = Look::of(model, 17);
            let original = Composition::resolve(&look);
            let drape = original.outfit.neck_drape.expect("native neck drape");
            let equipped = look.clone().equipped(equipment);
            let resolved = Composition::resolve(&equipped);
            assert_eq!(
                resolved.outfit.neck_drape,
                drape.hood_mount.is_none().then_some(drape)
            );
            assert_eq!(
                Composition::resolve(&equipped.equipped(Equipment::default())),
                original
            );
        }
    }

    #[test]
    fn equipment_selection_is_order_independent_and_rejects_unknown_items() {
        let mut first = Equipment::default();
        let mut second = Equipment::default();
        for name in ["helm", "boots", "cloak", "breastplate"] {
            first.select(name).unwrap();
        }
        for name in ["breastplate", "cloak", "boots", "helm", "boots"] {
            second.select(name).unwrap();
        }
        assert_eq!(first, second);
        assert!(first.select("unimplemented-armor").is_err());
        assert_eq!(
            first, second,
            "invalid selection leaves equipment unchanged"
        );
        assert!(!first.is_empty());
    }
}

#[cfg(test)]
mod look_tests {
    use super::*;

    #[test]
    fn equipment_survives_redressing_and_removal_restores_canonical_identity() {
        let mut equipment = Equipment::default();
        for name in ["helm", "boots", "cloak", "breastplate"] {
            equipment.select(name).unwrap();
        }
        for model in [
            CreatureModel::Human,
            CreatureModel::Mage,
            CreatureModel::Orc,
        ] {
            for woman in [false, true] {
                let native = Look::with_choice(model, 17, Some(Variant { woman, ..default() }));
                for wardrobe in Wardrobe::ALL {
                    let preset = native.clone().dressed(wardrobe);
                    let composed = preset.clone().equipped(equipment);
                    let reversed = native.clone().equipped(equipment).dressed(wardrobe);
                    assert_eq!(
                        composed.key, reversed.key,
                        "selection order must not change cache identity"
                    );
                    assert_eq!(Outfit::worn(&composed), Outfit::worn(&reversed));
                    assert_ne!(composed.key, preset.key);
                    assert_eq!(composed.body_key.as_deref(), Some(native.key.as_str()));
                    assert_eq!(composed.v, native.v);
                    assert_eq!(composed.style, native.style);
                    assert_eq!(composed.scale, native.scale);
                    assert_eq!(composed.clone().equipped(equipment).key, composed.key);
                    let restored = composed.clone().equipped(Equipment::default());
                    assert_eq!(restored.key, preset.key);
                    assert_eq!(Outfit::worn(&restored), Outfit::worn(&preset));
                    let native_equipped = composed.dressed(Wardrobe::Native);
                    assert_eq!(native_equipped.equipment, equipment);
                    assert!(native_equipped.outfit.is_none());
                    assert_eq!(
                        native_equipped.equipped(Equipment::default()).key,
                        native.key
                    );
                }
            }
        }
    }

    #[test]
    fn wardrobe_catalog_preserves_browsing_fallback_and_exact_selection() {
        let man = Look::with_choice(CreatureModel::Human, 17, Some(Variant::default()));
        for wardrobe in Wardrobe::ALL {
            let expected = match wardrobe {
                Wardrobe::RoyalBride => Wardrobe::RoyalGroom,
                Wardrobe::Princess | Wardrobe::RoyalPrincess => Wardrobe::Courtier,
                Wardrobe::SylvanDuelist => Wardrobe::Ranger,
                other => other,
            };
            assert_eq!(wardrobe.recipe_for(false), expected);
            assert_eq!(wardrobe.recipe_for(true), wardrobe);
            assert_eq!(
                Outfit::from_wardrobe(wardrobe, &man),
                Outfit::from_wardrobe(expected, &man)
            );
            assert_eq!(
                man.clone().composed(wardrobe, Equipment::default()).is_ok(),
                expected == wardrobe
            );
        }
    }

    #[test]
    fn checked_composition_is_exact_or_reports_unsupported_selection() {
        let empty = Equipment::default();
        let mut boots = empty;
        boots.select("boots").unwrap();
        let animal = Look::of(CreatureModel::Bat, 17);
        assert_eq!(
            animal
                .clone()
                .composed(Wardrobe::Native, empty)
                .unwrap()
                .key,
            animal.key
        );
        assert!(animal.clone().composed(Wardrobe::Native, boots).is_err());
        assert!(animal.composed(Wardrobe::Ranger, empty).is_err());
        let man = Look::with_choice(CreatureModel::Human, 17, Some(Variant::default()));
        for wardrobe in [
            Wardrobe::Princess,
            Wardrobe::RoyalPrincess,
            Wardrobe::SylvanDuelist,
        ] {
            assert!(
                man.clone()
                    .composed(wardrobe, empty)
                    .unwrap_err()
                    .contains("female body")
            );
        }
        let woman = Look::with_choice(
            CreatureModel::Human,
            17,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        let exact = woman
            .clone()
            .composed(Wardrobe::RoyalPrincess, boots)
            .unwrap();
        let expected = woman.dressed(Wardrobe::RoyalPrincess).equipped(boots);
        assert_eq!(exact.key, expected.key);
        assert_eq!(
            Composition::resolve(&exact),
            Composition::resolve(&expected)
        );
        assert_eq!(exact.equipment(), boots);
    }

    #[test]
    fn equipment_does_not_change_unsupported_body_recipes() {
        let mut equipment = Equipment::default();
        equipment.select("boots").unwrap();
        for model in [
            CreatureModel::Bat,
            CreatureModel::Guardian,
            CreatureModel::Yeti,
            CreatureModel::Centaur,
        ] {
            let look = Look::of(model, 17);
            let equipped = look.clone().equipped(equipment);
            assert_eq!(equipped.key, look.key);
            assert!(equipped.equipment.is_empty());
        }
    }
}

#[cfg(test)]
mod armor_tests {
    use super::*;
    #[test]
    fn breastplate_selection_preserves_compatible_garment_regions() {
        let mut equipment = Equipment::default();
        assert!(
            equipment.select("breastplate").is_ok(),
            "breastplate must be independently selectable"
        );
        let look = Look::with_choice(
            CreatureModel::Human,
            17,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        for wardrobe in [
            Wardrobe::RoyalPrincess,
            Wardrobe::Bard,
            Wardrobe::Ranger,
            Wardrobe::Native,
        ] {
            let preset = Outfit::from_wardrobe(wardrobe, &look);
            let resolved = equipment.compose(preset);
            assert_eq!(resolved.arms, preset.arms);
            assert_eq!(resolved.legs, preset.legs);
            assert_eq!(resolved.skirt, preset.skirt);
            assert_eq!(
                resolved.outer, preset.outer,
                "retain coat and peplum beneath armor"
            );
            assert_eq!(resolved.head, preset.head);
            assert_eq!(resolved.feet, preset.feet);
            assert_eq!(resolved.trimmings.neck, preset.trimmings.neck);
            assert!(
                resolved.torso_ornament.is_none(),
                "covered bodice ornaments must not pierce plate"
            );
            assert_eq!(Equipment::default().compose(preset), preset);
        }
    }
}

#[cfg(test)]
mod study_tests {
    use super::*;

    #[test]
    fn disclosed_studies_match_preview_selection_without_using_item_identity() {
        use tor_client_3d_model::{ItemModel, WearableLook, Wearing};
        for appearance in WearableLook::ALL {
            let mut worn = Wearing::default();
            match appearance.slot() {
                "head_armor" => {
                    worn.head = Some(ItemModel::RingMail);
                    worn.head_look = Some(appearance);
                }
                "feet_armor" => {
                    worn.feet = Some(ItemModel::RingMail);
                    worn.feet_look = Some(appearance);
                }
                "shirt" => worn.shirt_look = Some(appearance),
                "hands_armor" => {
                    worn.hands = Some(ItemModel::RingMail);
                    worn.hands_look = Some(appearance);
                }
                "cloak" => {
                    worn.back = Some(ItemModel::RingMail);
                    worn.back_look = Some(appearance);
                }
                _ => unreachable!(),
            }
            let mut preview = Equipment::default();
            preview.select(appearance.name()).unwrap();
            assert_eq!(Equipment::disclosed(&worn), preview);
            let base = Look::of(CreatureModel::Human, 17).dressed(Wardrobe::Bard);
            let original = Outfit::worn(&base);
            let equipped = base.clone().equipped(preview);
            let resolved = Outfit::worn(&equipped);
            if appearance.slot() != "shirt" {
                assert_eq!(resolved.bodice, original.bodice);
            }
            assert_eq!(resolved.skirt, original.skirt);
            if appearance.slot() != "cloak" {
                assert_eq!(resolved.cloak, original.cloak);
            }
            assert_eq!(
                Outfit::worn(&equipped.equipped(Equipment::default())),
                original
            );
        }
    }

    #[test]
    fn centaur_adapter_accepts_head_and_handwear_without_inventing_boot_attachments() {
        let body = Look::of(CreatureModel::Centaur, 17);
        let mut equipment = Equipment::default();
        equipment.select("fedora").unwrap();
        equipment.select("riding-gloves").unwrap();
        assert!(body.clone().composed(Wardrobe::Native, equipment).is_ok());
        equipment.select("riding-boots").unwrap();
        assert!(body.clone().composed(Wardrobe::Native, equipment).is_err());
        let supported = body.clone().equipped(equipment);
        assert!(supported.equipment.head.is_some());
        assert!(supported.equipment.hands.is_some());
        assert!(supported.equipment.feet.is_none());
        assert_eq!(supported.equipped(Equipment::default()).key, body.key);
    }
}
