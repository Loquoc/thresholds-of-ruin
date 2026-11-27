//! Bounded authoring adapters for once-only entry population and initial kits.
//! These declarations describe immutable content. Only simulation evaluates entry.
use super::*;
use tor_simulation::floor_activation as runtime;

const MAX_PROFILES: usize = runtime::MAX_PROFILES;
const MAX_STAGES: usize = runtime::MAX_STAGES;
const MAX_DEPTH_BANDS: usize = runtime::MAX_DEPTH_BANDS;
const MAX_CHOICES: usize = runtime::MAX_CHOICES;
const MAX_CARRIED_ITEM_CHOICES: usize = runtime::MAX_CARRIED_ITEM_CHOICES;
const MAX_ATTEMPTS: u16 = runtime::MAX_ATTEMPTS;
const MAX_LOADOUT: usize = 64;
const MAX_CARRIED_ITEM_POOLS: usize = runtime::MAX_CARRIED_ITEM_POOLS;

/// Fixed prepared definitions and content for one population-plan compilation.
/// A label locates at most one candidate; every compiled recipe field must match.
struct PopulationBirthValidation<'a> {
    index: &'a PackageIndex,
    successful: BTreeMap<String, runtime::PreparedActorChoice>,
    #[cfg(test)]
    checks: usize,
}
impl<'a> PopulationBirthValidation<'a> {
    fn new(index: &'a PackageIndex) -> Self {
        Self {
            index,
            successful: BTreeMap::new(),
            #[cfg(test)]
            checks: 0,
        }
    }
    fn validate(&mut self, choice: &runtime::PreparedActorChoice) -> Result<(), Failure> {
        if self
            .successful
            .get(&choice.content)
            .is_some_and(|previous| previous.same_spawn_recipe(choice))
        {
            return Ok(());
        }
        #[cfg(test)]
        {
            self.checks += 1;
        }
        choice
            .actor
            .validate_birth_advancement(
                self.index.creature_catalog().content().clone(),
                choice.depth_growth,
                &choice.advancement,
            )
            .map_err(|error| {
                fail(format!(
                    "Invalid population depth-growth prototype: {error:?}"
                ))
            })?;
        // Never retain more than one band's capacity. Failures are not retained.
        if self.successful.len() < MAX_CHOICES || self.successful.contains_key(&choice.content) {
            self.successful
                .insert(choice.content.clone(), choice.clone());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioCapabilities {
    pub npc_stair_traversal: bool,
    /// Omitted switches keep the engine default: enabled.
    #[serde(default = "enabled")]
    pub owner_advancement: bool,
    #[serde(default = "enabled")]
    pub corpses: bool,
    #[serde(default = "enabled")]
    pub low_health_flight: bool,
}

fn enabled() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Protagonist {
    Actor(u64),
    Selected(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FloorActivation {
    #[serde(default)]
    pub wandering: Option<runtime::WanderingPolicy>,
    pub version: u32,
    pub protagonist: Protagonist,
    #[serde(default)]
    pub families: BTreeMap<String, String>,
    pub profiles: BTreeMap<String, PopulationProfile>,
    #[serde(default, deserialize_with = "read_carried_item_pools")]
    pub carried_item_pools: BTreeMap<String, CarriedItemPool>,
}

fn read_carried_item_pools<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, CarriedItemPool>, D::Error> {
    struct Pools;
    impl<'de> serde::de::Visitor<'de> for Pools {
        type Value = BTreeMap<String, CarriedItemPool>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("at most 32 uniquely named carried item pools")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut mapping: A,
        ) -> Result<Self::Value, A::Error> {
            use serde::de::Error;
            if mapping
                .size_hint()
                .is_some_and(|size| size > MAX_CARRIED_ITEM_POOLS)
            {
                return Err(A::Error::custom("too many carried item pools"));
            }
            let mut pools = BTreeMap::new();
            while let Some(key) = mapping.next_key::<String>()? {
                if pools.len() >= MAX_CARRIED_ITEM_POOLS {
                    return Err(A::Error::custom("too many carried item pools"));
                }
                if pools.contains_key(&key) {
                    return Err(A::Error::custom("duplicate carried item pool"));
                }
                pools.insert(key, mapping.next_value()?);
            }
            Ok(pools)
        }
    }
    deserializer.deserialize_map(Pools)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationProfile {
    pub version: u32,
    pub stages: Vec<PopulationStage>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PopulationDepth {
    Finite { minimum: u32, maximum: u32 },
    Open { minimum: u32 },
}

impl PopulationDepth {
    fn minimum(self) -> u32 {
        match self {
            Self::Finite { minimum, .. } | Self::Open { minimum } => minimum,
        }
    }
    fn maximum(self) -> Option<u32> {
        match self {
            Self::Finite { maximum, .. } => Some(maximum),
            Self::Open { .. } => None,
        }
    }
    fn compiled(self) -> runtime::DepthInterval {
        match self {
            Self::Finite { minimum, maximum } => {
                runtime::DepthInterval::Finite { minimum, maximum }
            }
            Self::Open { minimum } => runtime::DepthInterval::Open { minimum },
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PopulationDepthGrowth {
    Fixed {},
    RacialAfter { depth: u32 },
}
impl Default for PopulationDepthGrowth {
    fn default() -> Self {
        Self::Fixed {}
    }
}
impl PopulationDepthGrowth {
    fn compiled(self) -> tor_simulation::prepared_spawn::DepthGrowth {
        match self {
            Self::Fixed {} => tor_simulation::prepared_spawn::DepthGrowth::Fixed {},
            Self::RacialAfter { depth } => {
                tor_simulation::prepared_spawn::DepthGrowth::RacialAfter { depth }
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationBand<C = PopulationChoice> {
    pub depth: PopulationDepth,
    pub choices: Vec<C>,
}

pub type ActorPopulationBand = PopulationBand<ActorPopulationChoice>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationChoice {
    pub archetype: String,
    pub weight: u32,
    /// Overrides the stage quantity for this choice, such as grouped missiles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<QuantityExpression>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorPopulationChoice {
    pub archetype: String,
    pub weight: u32,
    pub carried_loot: Option<CarriedLoot>,
    #[serde(
        default,
        deserialize_with = "crate::creature_authoring::bounded_strike_vec::<_, PopulationAdvancement, 256>"
    )]
    pub advancement: Vec<PopulationAdvancement>,
}

/// Birth-only choices for exact future racial ordinals, not a live HD formula.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationAdvancement {
    #[serde(deserialize_with = "read_birth_ordinal")]
    pub ordinal: u64,
    #[serde(
        default,
        deserialize_with = "crate::creature_authoring::bounded_strike_vec::<_, crate::creature_authoring::Purchase, 256>"
    )]
    pub purchases: Vec<crate::creature_authoring::Purchase>,
    pub attribute: Option<crate::creature_authoring::Attribute>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarriedItemPool {
    pub version: u32,
    pub choices: Vec<CarriedItemChoice>,
}

fn read_birth_ordinal<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let ordinal = u64::deserialize(d)?;
    if !(1..=tor_simulation::limits::HIT_DICE as u64).contains(&ordinal) {
        return Err(serde::de::Error::custom(
            "invalid birth advancement ordinal",
        ));
    }
    Ok(ordinal)
}

/// Source convenience is separate from the canonical tagged compiled/save form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QuantityExpression {
    Fixed(u64),
    Expression(PopulationQuantityExpr),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PopulationQuantityExpr {
    Fixed {
        quantity: u64,
    },
    Uniform {
        minimum: u64,
        maximum: u64,
    },
    DepthScaledUniform {
        minimum: u64,
        maximum_base: u64,
        per_depth: u64,
    },
}
impl PopulationQuantityExpr {
    pub fn compiled(self) -> tor_simulation::quantity::QuantityExpr {
        use tor_simulation::quantity::QuantityExpr;
        match self {
            Self::Fixed { quantity } => QuantityExpr::Fixed { quantity },
            Self::Uniform { minimum, maximum } => QuantityExpr::Uniform { minimum, maximum },
            Self::DepthScaledUniform {
                minimum,
                maximum_base,
                per_depth,
            } => QuantityExpr::DepthScaledUniform {
                minimum,
                maximum_base,
                per_depth,
            },
        }
    }
}
impl QuantityExpression {
    pub fn compiled(&self) -> tor_simulation::quantity::QuantityExpr {
        match *self {
            Self::Fixed(quantity) => tor_simulation::quantity::QuantityExpr::Fixed { quantity },
            Self::Expression(expression) => expression.compiled(),
        }
    }
    fn maximum(&self) -> Result<u64, Failure> {
        self.compiled()
            .resolve(Some(1))
            .map_err(|_| fail("Invalid quantity expression"))?;
        self.compiled()
            .resolve(Some(u32::MAX))
            .map(|range| range.maximum)
            .map_err(|_| fail("Quantity expression overflows its depth domain"))
    }
}
fn unit_population_quantity() -> QuantityExpression {
    QuantityExpression::Fixed(1)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarriedItemChoice {
    pub archetype: String,
    pub weight: u32,
    pub quantity: QuantityExpression,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarriedLoot {
    pub version: u32,
    pub pool: String,
    pub chance: CarryChance,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarryChance {
    pub numerator: u32,
    pub denominator: u32,
}

impl CarriedLoot {
    fn compiled(&self) -> runtime::CarriedLoot {
        runtime::CarriedLoot {
            version: self.version,
            pool: self.pool.clone(),
            chance: runtime::CarryChance {
                numerator: self.chance.numerator,
                denominator: self.chance.denominator,
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PopulationStage {
    RoomCluster {
        id: String,
        version: u32,
        chance: CarryChance,
        item_count: [u16; 2],
        actor_count: [u16; 2],
        actor_depth_offset: u32,
        item_bands: Vec<PopulationBand>,
        actor_bands: Vec<ActorPopulationBand>,
        quantity: QuantityExpression,
        ai: Option<String>,
        #[serde(default)]
        depth_growth: PopulationDepthGrowth,
        #[serde(default)]
        avoid_protagonist_sight: bool,
    },
    RoomItems {
        id: String,
        version: u32,
        bands: Vec<PopulationBand>,
        quantity: QuantityExpression,
        chance: CarryChance,
    },
    Items {
        id: String,
        version: u32,
        count: [u16; 2],
        bands: Vec<PopulationBand>,
        #[serde(default = "unit_population_quantity")]
        quantity: QuantityExpression,
    },
    /// `attempts` independent rolls, each placing one item with `chance`.
    ChanceItems {
        id: String,
        version: u32,
        attempts: u16,
        chance: CarryChance,
        bands: Vec<PopulationBand>,
        #[serde(default = "unit_population_quantity")]
        quantity: QuantityExpression,
    },
    /// One guaranteed item after more than `threshold` foodless first entries.
    FoodDrought {
        id: String,
        version: u32,
        threshold: u16,
        food: Vec<String>,
        bands: Vec<PopulationBand>,
        #[serde(default = "unit_population_quantity")]
        quantity: QuantityExpression,
    },
    Actors {
        id: String,
        version: u32,
        count: [u16; 2],
        bands: Vec<ActorPopulationBand>,
        ai: Option<String>,
        #[serde(default)]
        depth_growth: PopulationDepthGrowth,
        #[serde(default)]
        avoid_protagonist_sight: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterLoadout {
    pub archetype: String,
    #[serde(default = "unit_quantity")]
    pub quantity: u64,
    pub equipped_slot: Option<u16>,
}

impl PopulationStage {
    fn compiled(
        &self,
        births: &mut PopulationBirthValidation<'_>,
    ) -> Result<runtime::PopulationContent, Failure> {
        let index = births.index;
        Ok(match self {
            Self::RoomCluster {
                chance,
                item_count,
                actor_count,
                actor_depth_offset,
                avoid_protagonist_sight,
                ..
            } => {
                let parts = self.parts();
                let runtime::PopulationContent::Items { bands: item_bands } =
                    parts[0].compiled(births)?
                else {
                    unreachable!()
                };
                let runtime::PopulationContent::Actors {
                    bands: actor_bands, ..
                } = parts[1].compiled(births)?
                else {
                    unreachable!()
                };
                runtime::PopulationContent::RoomCluster {
                    chance: runtime::CarryChance {
                        numerator: chance.numerator,
                        denominator: chance.denominator,
                    },
                    item_count: *item_count,
                    actor_count: *actor_count,
                    actor_depth_offset: *actor_depth_offset,
                    item_bands,
                    actor_bands,
                    avoid_protagonist_sight: *avoid_protagonist_sight,
                }
            }
            PopulationStage::Items {
                quantity, bands, ..
            }
            | PopulationStage::RoomItems {
                quantity, bands, ..
            }
            | PopulationStage::ChanceItems {
                quantity, bands, ..
            }
            | PopulationStage::FoodDrought {
                quantity, bands, ..
            } => {
                let bands = bands
                    .iter()
                    .map(|band| {
                        Ok(runtime::DepthBand {
                            depth: band.depth.compiled(),
                            choices: band
                                .choices
                                .iter()
                                .map(|choice| {
                                    let quantity = choice.quantity.as_ref().unwrap_or(quantity);
                                    Ok(runtime::PreparedItemChoice {
                                        content: choice.archetype.clone(),
                                        weight: choice.weight,
                                        quantity: quantity.compiled(),
                                        spec: index.definitions.population_item(
                                            &choice.archetype,
                                            quantity.maximum()?,
                                            index.seed,
                                            &index.appearances,
                                        )?,
                                    })
                                })
                                .collect::<Result<_, Failure>>()?,
                        })
                    })
                    .collect::<Result<_, Failure>>()?;
                match self {
                    PopulationStage::RoomItems { chance, .. } => {
                        runtime::PopulationContent::RoomItems {
                            bands,
                            chance: runtime::CarryChance {
                                numerator: chance.numerator,
                                denominator: chance.denominator,
                            },
                        }
                    }
                    PopulationStage::ChanceItems { chance, .. } => {
                        runtime::PopulationContent::ChanceItems {
                            bands,
                            chance: runtime::CarryChance {
                                numerator: chance.numerator,
                                denominator: chance.denominator,
                            },
                        }
                    }
                    PopulationStage::FoodDrought {
                        threshold, food, ..
                    } => {
                        let mut food = food.clone();
                        food.sort();
                        runtime::PopulationContent::FoodDrought {
                            bands,
                            threshold: *threshold,
                            food,
                        }
                    }
                    _ => runtime::PopulationContent::Items { bands },
                }
            }
            PopulationStage::Actors {
                ai,
                bands,
                depth_growth,
                avoid_protagonist_sight,
                ..
            } => runtime::PopulationContent::Actors {
                bands: bands
                    .iter()
                    .map(|band| {
                        Ok(runtime::DepthBand {
                            depth: band.depth.compiled(),
                            choices: band
                                .choices
                                .iter()
                                .map(|choice| {
                                    let mut prepared = index.definitions.population_actor(
                                        &choice.archetype,
                                        choice.weight,
                                        ai.as_deref(),
                                    )?;
                                    prepared.carried_loot =
                                        choice.carried_loot.as_ref().map(CarriedLoot::compiled);
                                    prepared.depth_growth = depth_growth.compiled();
                                    prepared.advancement = choice
                                        .advancement
                                        .iter()
                                        .map(|entry| {
                                            Ok(tor_simulation::prepared_spawn::OrdinalAdvancement {
                                                ordinal: tor_simulation::HdCount(entry.ordinal),
                                                choices:
                                                    crate::creature_authoring::compile_choices(
                                                        &entry.purchases,
                                                        entry.attribute.as_ref(),
                                                        index.creature_catalog().content().rules(),
                                                    )?,
                                            })
                                        })
                                        .collect::<Result<_, Failure>>()?;
                                    births.validate(&prepared)?;
                                    Ok(prepared)
                                })
                                .collect::<Result<_, Failure>>()?,
                        })
                    })
                    .collect::<Result<_, Failure>>()?,
                avoid_protagonist_sight: *avoid_protagonist_sight,
            },
        })
    }
    fn maximum_attempts(&self) -> u32 {
        match self {
            Self::RoomCluster {
                item_count,
                actor_count,
                ..
            } => u32::from(item_count[1]) + u32::from(actor_count[1]),
            _ => u32::from(self.header().2[1]),
        }
    }
    pub(super) fn item_bands(&self) -> Option<&[PopulationBand]> {
        match self {
            Self::Items { bands, .. }
            | Self::RoomItems { bands, .. }
            | Self::RoomCluster {
                item_bands: bands, ..
            } => Some(bands),
            _ => None,
        }
    }
    pub(super) fn actor_bands(&self) -> Option<&[ActorPopulationBand]> {
        match self {
            Self::Actors { bands, .. }
            | Self::RoomCluster {
                actor_bands: bands, ..
            } => Some(bands),
            _ => None,
        }
    }
    fn parts(&self) -> Vec<std::borrow::Cow<'_, Self>> {
        use std::borrow::Cow;
        match self {
            Self::RoomCluster {
                id,
                version,
                item_count,
                actor_count,
                item_bands,
                actor_bands,
                quantity,
                ai,
                depth_growth,
                avoid_protagonist_sight,
                ..
            } => vec![
                Cow::Owned(Self::Items {
                    id: id.clone(),
                    version: *version,
                    count: *item_count,
                    bands: item_bands.clone(),
                    quantity: *quantity,
                }),
                Cow::Owned(Self::Actors {
                    id: id.clone(),
                    version: *version,
                    count: *actor_count,
                    bands: actor_bands.clone(),
                    ai: ai.clone(),
                    depth_growth: *depth_growth,
                    avoid_protagonist_sight: *avoid_protagonist_sight,
                }),
            ],
            _ => vec![Cow::Borrowed(self)],
        }
    }
    fn header(&self) -> (&str, u32, [u16; 2]) {
        match self {
            Self::RoomItems { id, version, .. } | Self::RoomCluster { id, version, .. } => {
                (id, *version, [1, 1])
            }
            Self::Items {
                id, version, count, ..
            }
            | Self::Actors {
                id, version, count, ..
            } => (id, *version, *count),
            Self::ChanceItems {
                id,
                version,
                attempts,
                ..
            } => (id, *version, [*attempts, *attempts]),
            Self::FoodDrought { id, version, .. } => (id, *version, [1, 1]),
        }
    }
}

impl Package {
    pub(super) fn check_population(&self) -> Result<(), Failure> {
        for character in &self.manifest.characters {
            require(
                character.loadout.len() <= MAX_LOADOUT,
                "Character loadout exceeds limit",
            )?;
            let mut slots = BTreeSet::new();
            for row in &character.loadout {
                require(
                    label(&row.archetype) && row.quantity > 0,
                    "Invalid character loadout",
                )?;
                self.author_archetype(&row.archetype)?;
                if let Some(slot) = row.equipped_slot {
                    require(
                        row.quantity == 1 && slots.insert(slot),
                        "Duplicate or stacked starting equipment",
                    )?;
                }
            }
        }
        let Some(plan) = &self.manifest.floor_activation else {
            return Ok(());
        };
        require(plan.version == 1, "Unsupported floor activation version")?;
        if let Some(policy) = &plan.wandering {
            require(policy.valid(), "Invalid wandering population policy")?;
            let mut content = BTreeSet::new();
            for profile in plan.profiles.values() {
                let stage = profile
                    .stages
                    .iter()
                    .find(|stage| stage.header().0 == policy.stage_id)
                    .ok_or_else(|| fail("Unknown wandering actor stage"))?;
                let PopulationStage::Actors { bands, .. } = stage else {
                    return Err(fail("Wandering population requires an actor stage"));
                };
                content.extend(
                    bands.iter().flat_map(|band| {
                        band.choices.iter().map(|choice| choice.archetype.as_str())
                    }),
                );
            }
            require(
                policy
                    .excluded_wandering_content
                    .iter()
                    .all(|id| content.contains(id.as_str())),
                "Unknown wandering exclusion",
            )?;
        }
        let protagonist = match &plan.protagonist {
            Protagonist::Actor(actor) => *actor,
            Protagonist::Selected(selector) => {
                require(
                    selector == "selected_character",
                    "Unknown protagonist selector",
                )?;
                self.selected
            }
        };
        require(
            protagonist == self.selected
                && self.manifest.characters.iter().any(|c| c.id == protagonist),
            "Activation protagonist must be the selected declared character",
        )?;
        require(
            !plan.profiles.is_empty()
                && plan.profiles.len() <= MAX_PROFILES
                && plan.families.len() <= MAX_REGIONS,
            "Floor activation definitions exceed limit",
        )?;
        for (family, profile) in &plan.families {
            require(
                self.manifest.procedural_worlds.contains_key(family)
                    && plan.profiles.contains_key(profile),
                "Unknown activation family or profile",
            )?;
        }
        require(
            plan.carried_item_pools.len() <= MAX_CARRIED_ITEM_POOLS,
            "Too many carried item pools",
        )?;
        for (name, pool) in &plan.carried_item_pools {
            require(
                label(name) && pool.version == 1,
                "Invalid carried item pool or unsupported version",
            )?;
            self.check_population_choices(
                pool.choices
                    .iter()
                    .map(|choice| (choice.archetype.as_str(), choice.weight)),
                pool.choices.len(),
                MAX_CARRIED_ITEM_CHOICES,
            )?;
            for choice in &pool.choices {
                choice.quantity.maximum()?;
            }
        }
        for (name, profile) in &plan.profiles {
            require(
                label(name)
                    && profile.version == 1
                    && profile.stages.len() <= MAX_STAGES
                    && profile
                        .stages
                        .iter()
                        .map(PopulationStage::maximum_attempts)
                        .sum::<u32>()
                        <= u32::from(MAX_ATTEMPTS),
                "Invalid population profile or unsupported version",
            )?;
            let mut ids = BTreeSet::new();
            for stage in &profile.stages {
                let (id, version, count) = stage.header();
                require(
                    label(id)
                        && ids.insert(id)
                        && version == 1
                        && count[0] <= count[1]
                        && count[1] <= MAX_ATTEMPTS,
                    "Invalid population stage",
                )?;
                if let PopulationStage::RoomCluster {
                    chance,
                    item_count,
                    actor_count,
                    actor_depth_offset,
                    ..
                } = stage
                {
                    require(
                        chance.denominator > 0
                            && chance.numerator <= chance.denominator
                            && *actor_depth_offset <= 1
                            && [item_count, actor_count].iter().all(|count| {
                                count[0] > 0 && count[0] <= count[1] && count[1] <= MAX_ATTEMPTS
                            })
                            && stage.maximum_attempts() <= u32::from(MAX_ATTEMPTS),
                        "Invalid room cluster policy",
                    )?;
                }
                for part in stage.parts() {
                    let stage = part.as_ref();
                    if let PopulationStage::Actors { depth_growth, .. } = stage {
                        require(
                            depth_growth.compiled().valid(),
                            "Invalid population depth growth",
                        )?;
                    }
                    match stage {
                        PopulationStage::RoomCluster { .. } => {
                            unreachable!("ordinary validation parts")
                        }
                        PopulationStage::Items {
                            bands, quantity, ..
                        }
                        | PopulationStage::RoomItems {
                            bands, quantity, ..
                        }
                        | PopulationStage::ChanceItems {
                            bands, quantity, ..
                        }
                        | PopulationStage::FoodDrought {
                            bands, quantity, ..
                        } => {
                            quantity.maximum()?;
                            if let PopulationStage::RoomItems { chance, .. }
                            | PopulationStage::ChanceItems { chance, .. } = stage
                            {
                                require(
                                    chance.denominator > 0
                                        && chance.numerator <= chance.denominator,
                                    "Invalid population chance",
                                )?;
                            }
                            if let PopulationStage::ChanceItems { attempts, .. } = stage {
                                require(*attempts > 0, "Chance item stages need attempts")?;
                            }
                            if let PopulationStage::FoodDrought { food, .. } = stage {
                                let unique: BTreeSet<_> = food.iter().collect();
                                require(
                                    !food.is_empty()
                                        && food.len() <= 64
                                        && unique.len() == food.len(),
                                    "Food drought needs distinct food archetypes",
                                )?;
                                for archetype in food {
                                    self.author_archetype(archetype)?;
                                }
                            }
                            self.check_population_bands(
                                bands,
                                runtime::MAX_ITEM_CHOICES,
                                |choice| (&choice.archetype, choice.weight),
                            )?;
                        }
                        PopulationStage::Actors { bands, ai, .. } => {
                            self.check_population_bands(bands, MAX_CHOICES, |choice| {
                                (&choice.archetype, choice.weight)
                            })?;
                            if let Some(ai) = ai {
                                require(
                                    self.manifest.ai_profiles.contains_key(ai),
                                    "Unknown population AI profile",
                                )?;
                            }
                            for choice in bands.iter().flat_map(|band| &band.choices) {
                                if let Some(policy) = &choice.carried_loot {
                                    require(
                                        policy.version == 1
                                            && label(&policy.pool)
                                            && plan.carried_item_pools.contains_key(&policy.pool)
                                            && policy.chance.denominator > 0
                                            && policy.chance.numerator <= policy.chance.denominator,
                                        "Invalid carried loot policy or unknown item pool",
                                    )?;
                                }
                            }
                        }
                    }
                }
            }
        }
        // Validate every possible choice, including future depth bands and
        // zero-chance carried pools. Effective family themes are structural;
        // actual activation outcomes must never determine a palette.
        for (family, profile) in &plan.families {
            let definition = &self.manifest.procedural_worlds[family];
            let themes = definition
                .zone
                .as_ref()
                .and_then(|zone| self.manifest.zones.get(zone))
                .and_then(|zone| zone.themes.as_deref())
                .unwrap_or(&self.manifest.themes);
            let forecast = self.theme_assets(themes);
            let mut assets = BTreeSet::new();
            let item_assets = |key: &str| -> Result<Vec<String>, Failure> {
                Ok(
                    compiler::item_assets(self.author_archetype(key)?, &self.manifest)
                        .map(str::to_owned)
                        .collect(),
                )
            };
            for stage in &plan.profiles[profile].stages {
                if let Some(bands) = stage.item_bands() {
                    for choice in bands.iter().flat_map(|band| &band.choices) {
                        assets.extend(item_assets(&choice.archetype)?);
                    }
                }
                if let Some(bands) = stage.actor_bands() {
                    for choice in bands.iter().flat_map(|band| &band.choices) {
                        assets.extend(self.author_archetype(&choice.archetype)?.asset.clone());
                        if let Some(policy) = &choice.carried_loot {
                            for item in &plan.carried_item_pools[&policy.pool].choices {
                                assets.extend(item_assets(&item.archetype)?);
                            }
                        }
                    }
                }
            }
            for asset in assets {
                require(
                    forecast.contains(&asset),
                    format!(
                        "Activation family {family}: asset {asset} isn't among its themes' assets"
                    ),
                )?;
            }
        }
        Ok(())
    }

    fn check_population_bands<C>(
        &self,
        bands: &[PopulationBand<C>],
        max_choices: usize,
        choice_fields: impl Fn(&C) -> (&str, u32),
    ) -> Result<(), Failure> {
        require(
            !bands.is_empty() && bands.len() <= MAX_DEPTH_BANDS,
            "Invalid population stage depth bands",
        )?;
        require(
            bands.windows(2).all(|pair| {
                pair[0]
                    .depth
                    .maximum()
                    .is_some_and(|end| end < pair[1].depth.minimum())
            }),
            "Population depth bands must be ordered and non-overlapping",
        )?;
        for band in bands {
            require(
                band.depth.minimum() > 0
                    && band
                        .depth
                        .maximum()
                        .is_none_or(|end| end >= band.depth.minimum()),
                "Invalid population depth band",
            )?;
            self.check_population_choices(
                band.choices.iter().map(&choice_fields),
                band.choices.len(),
                max_choices,
            )?;
        }
        Ok(())
    }

    fn check_population_choices<'a>(
        &self,
        choices: impl Iterator<Item = (&'a str, u32)>,
        count: usize,
        maximum: usize,
    ) -> Result<(), Failure> {
        require(
            count > 0 && count <= maximum,
            "Invalid population choice count",
        )?;
        let mut unique = BTreeSet::new();
        let mut total = 0u32;
        for (archetype, weight) in choices {
            require(
                label(archetype) && weight > 0 && unique.insert(archetype),
                "Population choices require distinct content and positive weights",
            )?;
            total = total
                .checked_add(weight)
                .ok_or_else(|| fail("Population choice weights overflow"))?;
            self.author_archetype(archetype)?;
        }
        Ok(())
    }

    fn capabilities(&self) -> runtime::ScenarioCapabilities {
        self.manifest.capabilities.as_ref().map_or_else(
            runtime::ScenarioCapabilities::default,
            |capabilities| runtime::ScenarioCapabilities {
                npc_stair_traversal: capabilities.npc_stair_traversal,
                owner_advancement: capabilities.owner_advancement,
                corpses: capabilities.corpses,
                low_health_flight: capabilities.low_health_flight,
            },
        )
    }

    fn population_plan(
        &self,
        index: &PackageIndex,
    ) -> Result<Option<runtime::PopulationPlan>, Failure> {
        self.population_plan_with_births(&mut PopulationBirthValidation::new(index))
    }

    fn population_plan_with_births(
        &self,
        births: &mut PopulationBirthValidation<'_>,
    ) -> Result<Option<runtime::PopulationPlan>, Failure> {
        let index = births.index;
        let Some(plan) = &self.manifest.floor_activation else {
            return Ok(None);
        };
        let profiles = plan
            .profiles
            .iter()
            .map(|(name, profile)| {
                let stages = profile
                    .stages
                    .iter()
                    .map(|stage| {
                        let (id, version, count) = stage.header();
                        let population = stage.compiled(births)?;
                        Ok(runtime::PopulationStage {
                            id: id.into(),
                            version,
                            count,
                            population,
                        })
                    })
                    .collect::<Result<_, Failure>>()?;
                Ok((
                    name.clone(),
                    runtime::PopulationProfile {
                        version: profile.version,
                        stages,
                    },
                ))
            })
            .collect::<Result<_, Failure>>()?;
        // Resolve all entries, including unused pools and policies with zero chance.
        // Preparation pins ordinary item specs without allocating any identity.
        let carried_item_pools = plan
            .carried_item_pools
            .iter()
            .map(|(name, pool)| {
                let choices = pool
                    .choices
                    .iter()
                    .map(|choice| {
                        Ok(runtime::PreparedItemChoice {
                            content: choice.archetype.clone(),
                            weight: choice.weight,
                            quantity: choice.quantity.compiled(),
                            spec: index.definitions.population_item(
                                &choice.archetype,
                                choice.quantity.maximum()?,
                                index.seed,
                                &index.appearances,
                            )?,
                        })
                    })
                    .collect::<Result<_, Failure>>()?;
                Ok((
                    name.clone(),
                    runtime::PreparedCarriedItemPool {
                        version: pool.version,
                        choices,
                    },
                ))
            })
            .collect::<Result<_, Failure>>()?;
        Ok(Some(runtime::PopulationPlan {
            protagonist: tor_simulation::ActorId(self.selected),
            profiles,
            carried_item_pools,
        }))
    }

    pub(super) fn configure_population(
        &self,
        game: &mut Game,
        index: &PackageIndex,
    ) -> Result<(), Failure> {
        game.configure_scenario_policy(
            self.capabilities(),
            Some(tor_simulation::ActorId(self.selected)),
        )
        .map_err(|error| fail(format!("Scenario capabilities: {error:?}")))?;
        game.bind_authored_teleport_levels(
            self.authored_teleport_levels()
                .map_err(|error| fail(format!("Authored teleport levels: {error:?}")))?,
        );
        if let Some(plan) = self.population_plan(index)? {
            game.configure_population(plan)
                .map_err(|error| fail(format!("Floor activation: {error:?}")))?;
            if let Some(policy) = self
                .manifest
                .floor_activation
                .as_ref()
                .and_then(|plan| plan.wandering.as_ref())
            {
                game.configure_wandering_population(policy.clone())
                    .map_err(|error| fail(format!("Wandering population: {error:?}")))?;
            }
        }
        if let Some(policy) = self.retrieval_policy(index)? {
            game.configure_unique_retrieval(policy)
                .map_err(|error| fail(format!("Unique retrieval: {error:?}")))?;
        }
        Ok(())
    }

    /// Compile once for a checkpoint recovery, then reuse across retained games.
    pub(crate) fn population_configuration(
        &self,
        seed: u64,
    ) -> Result<PopulationConfiguration<'_>, Failure> {
        let (plan, actor_floor, item_floor) = if self.manifest.floor_activation.is_some() {
            let index = self.index(seed)?;
            let character = self
                .manifest
                .characters
                .iter()
                .find(|character| character.id == self.selected)
                .ok_or_else(|| fail("Missing selected character"))?;
            // Base construction allocates one ordinary item identity per
            // selected loadout row before any initial population can run.
            let item_floor = index
                .ceilings
                .1
                .checked_add(character.loadout.len() as u64)
                .ok_or_else(|| fail("Population item identity floor overflow"))?;
            (self.population_plan(&index)?, index.ceilings.0, item_floor)
        } else {
            (None, 1, 1)
        };
        Ok(PopulationConfiguration {
            package: self,
            seed,
            plan,
            actor_floor,
            item_floor,
            capabilities: self.capabilities(),
            expected_scopes: Default::default(),
            expected_room_space: Default::default(),
        })
    }

    /// Register only immutable geometry. This is equally safe in a preload worker.
    pub(super) fn activation_scope(
        &self,
        identity: &str,
        depth: u32,
        regions: &[&RegionDef],
        rooms: &[runtime::PreparedRoomGroup],
    ) -> Result<Option<runtime::PreparedScope>, Failure> {
        let Some(plan) = &self.manifest.floor_activation else {
            return Ok(None);
        };
        let profile = identity
            .rsplit_once('/')
            .and_then(|(family, _)| plan.families.get(family));
        let Some(profile) = profile else {
            return Ok(None);
        };
        Ok(Some(
            self.profile_scope(identity, profile, depth, regions, rooms),
        ))
    }

    /// One floor scope of a known activation profile over immutable regions.
    pub(super) fn profile_scope(
        &self,
        identity: &str,
        profile: &str,
        depth: u32,
        regions: &[&RegionDef],
        rooms: &[runtime::PreparedRoomGroup],
    ) -> runtime::PreparedScope {
        let mut cells = BTreeSet::new();
        let mut protected = BTreeSet::new();
        for region in regions {
            // Generated RegionDef is the immutable terrain input: openings are
            // applied after walls by add_geometry. No mutable entity is read.
            let mut walls: BTreeSet<_> = region.walls.iter().copied().collect();
            for at in &region.openings {
                walls.remove(at);
            }
            for x in 0..region.size[0] {
                for y in 0..region.size[1] {
                    let at = loc(region.id, [x, y, 0]);
                    if !walls.contains(&[x, y, 0]) {
                        cells.insert(at);
                    }
                }
            }
            protected.extend(region.anchors.values().map(|at| loc(region.id, *at)));
        }
        runtime::PreparedScope {
            scope: identity.into(),
            depth,
            profile: profile.into(),
            members: regions.iter().map(|region| RegionId(region.id)).collect(),
            cells: cells.into_iter().collect(),
            protected,
            rooms: rooms.to_vec(),
        }
    }

    /// Called once by original/base reconstruction after the starting scope is
    /// loaded, and before scheduling or the first player observation.
    pub(crate) fn initialize_selected_character(&self, game: &mut Game) -> Result<(), Failure> {
        let has_loadout = self
            .manifest
            .characters
            .iter()
            .find(|character| character.id == self.selected)
            .is_some_and(|character| !character.loadout.is_empty());
        if !has_loadout {
            return game.activate_initial_floor().map_err(|error| {
                crate::Failure::simulation_failure(error)
                    .unwrap_or_else(|| fail(format!("Initial floor activation: {error:?}")))
            });
        }
        let index = self.index(game.seed())?;
        self.initialize_selected_character_with_index(game, &index)
    }

    pub(super) fn initialize_selected_character_with_index(
        &self,
        game: &mut Game,
        index: &PackageIndex,
    ) -> Result<(), Failure> {
        let character = self
            .manifest
            .characters
            .iter()
            .find(|character| character.id == self.selected)
            .ok_or_else(|| fail("Missing selected character"))?;
        let actor = tor_simulation::ActorId(self.selected);
        // Empty ordinary packages keep their existing observation work unchanged.
        // A carried item uses its owner's exact admitted initial location.
        let at = if character.loadout.is_empty() {
            None
        } else {
            Some(
                game.observe(actor)
                    .map_err(|_| fail("Selected character is not loaded"))?
                    .location,
            )
        };
        for row in &character.loadout {
            let spec = index.definitions.population_item(
                &row.archetype,
                row.quantity,
                game.seed(),
                &index.appearances,
            )?;
            let item = game
                .spawn_item_stack(
                    at.expect("loadout location"),
                    Some(actor),
                    row.quantity,
                    spec,
                )
                .map_err(|error| fail(format!("Character loadout: {error:?}")))?;
            if let Some(slot) = row.equipped_slot {
                game.equip_authored(actor, item, tor_simulation::EquipmentSlotId(slot))
                    .map_err(|_| {
                        fail("Starting equipment does not fit anatomy or occupied slot")
                    })?;
            }
        }
        game.activate_initial_floor().map_err(|error| {
            crate::Failure::simulation_failure(error)
                .unwrap_or_else(|| fail(format!("Initial floor activation: {error:?}")))
        })
    }
}

/// Recovery-only source binding. Scope reconstructions are cached across all
/// retained boundaries; current mutable terrain is deliberately never compared.
/// None of this work runs on the forward action path.
pub(crate) struct PopulationConfiguration<'a> {
    package: &'a Package,
    seed: u64,
    plan: Option<runtime::PopulationPlan>,
    actor_floor: u64,
    item_floor: u64,
    capabilities: runtime::ScenarioCapabilities,
    expected_scopes: std::cell::RefCell<BTreeMap<String, runtime::PreparedScope>>,
    // Only profiles containing clusters need a retained immutable room volume.
    // This is recovery-only derived data, not new saved/generated metadata.
    expected_room_space: std::cell::RefCell<BTreeMap<String, BTreeSet<Location>>>,
}

impl PopulationConfiguration<'_> {
    pub(crate) fn valid_game(&self, game: &Game) -> bool {
        if game.seed() != self.seed
            || game.wandering_policy()
                != self
                    .package
                    .manifest
                    .floor_activation
                    .as_ref()
                    .and_then(|plan| plan.wandering.as_ref())
            || !game.population_configuration_matches(
                self.plan.as_ref(),
                &self.capabilities,
                Some(tor_simulation::ActorId(self.package.selected)),
            )
        {
            return false;
        }
        let mut scopes = BTreeSet::new();
        for scope in game.activation_scopes() {
            if !self.valid_scope(scope) {
                return false;
            }
            scopes.insert(scope.scope.as_str());
        }
        let Some(plan) = &self.package.manifest.floor_activation else {
            return scopes.is_empty();
        };
        // Missing metadata for a published-but-unentered group must not silently
        // disable activation. Unbuilt declarations are not materialized evidence.
        for region in game
            .loaded_regions()
            .chain(game.detached_records().map(|(region, _)| region))
        {
            for (family_id, family) in &self.package.manifest.procedural_worlds {
                if plan.families.contains_key(family_id) {
                    if let Some((depth, _)) = family.address(region.0) {
                        if !scopes.contains(format!("{family_id}/{depth}").as_str()) {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }

    /// Bind historical receipts too: an abandoned branch may no longer retain
    /// the generated scope in the current Game, but its origin still belongs to
    /// the pinned family/profile, immutable placement geometry and population
    /// identity range. Authored reservations and the selected kit are excluded
    /// independently of whether their entities still exist.
    pub(crate) fn valid_record(&self, record: &runtime::FloorActivationRecord) -> bool {
        if !self.ensure_scope(&record.scope) {
            return false;
        }
        let scopes = self.expected_scopes.borrow();
        let scope = &scopes[&record.scope];
        let valid_attempt = |attempt: &runtime::PlacementDecision| match attempt {
            runtime::PlacementDecision::Skipped { .. } => true,
            runtime::PlacementDecision::Placed {
                location, entity, ..
            } => {
                entity
                    .actor_birth()
                    .is_none_or(|id| id.0 >= self.actor_floor)
                    && entity.item_birth().is_none_or(|id| id.0 >= self.item_floor)
                    && scope.cells.binary_search(location).is_ok()
                    && !scope.protected.contains(location)
            }
        };
        record.profile == scope.profile
            && record.decisions.iter().all(|decision| {
                if let runtime::StageResult::RoomCluster { selection } = &decision.result {
                    let Some(profile) = self
                        .plan
                        .as_ref()
                        .and_then(|plan| plan.profiles.get(&record.profile))
                    else {
                        return false;
                    };
                    let Some(stage) = profile.stages.iter().find(|stage| stage.id == decision.id)
                    else {
                        return false;
                    };
                    let room_space = self.expected_room_space.borrow();
                    let Some(room_space) = room_space.get(&record.scope) else {
                        return false;
                    };
                    if !stage.cluster_scope_valid(self.seed, profile, scope, selection)
                        || !stage
                            .cluster_body_geometry_valid(selection, |at| room_space.contains(&at))
                    {
                        return false;
                    }
                }
                if let runtime::StageResult::RoomsEvaluated { rooms } = &decision.result {
                    if rooms
                        .iter()
                        .map(|room| room.room)
                        .ne(scope.rooms.iter().map(|room| room.id))
                    {
                        return false;
                    }
                    for (decision, group) in rooms.iter().zip(&scope.rooms) {
                        if let runtime::RoomResult::Attempt {
                            decision: runtime::PlacementDecision::Placed { location, .. },
                        } = &decision.result
                        {
                            if group.cells.binary_search(location).is_err() {
                                return false;
                            }
                        }
                    }
                }
                decision.result.attempts().all(valid_attempt)
            })
    }

    fn valid_scope(&self, scope: &runtime::PreparedScope) -> bool {
        self.ensure_scope(&scope.scope)
            && self.expected_scopes.borrow().get(&scope.scope) == Some(scope)
    }

    fn ensure_scope(&self, identity: &str) -> bool {
        if self.expected_scopes.borrow().contains_key(identity) {
            return true;
        }
        let expected = (|| {
            let (family_id, depth) = identity.rsplit_once('/')?;
            let depth: u32 = depth.parse().ok()?;
            if depth == 0 || identity != format!("{family_id}/{depth}") {
                return None;
            }
            let family = self.package.manifest.procedural_worlds.get(family_id)?;
            // Validate this address against the family's finite ID domain before
            // reconstructing its pure geometry.
            family.group(family_id, self.seed, depth).ok()?;
            let definitions = self.package.generated_group(identity, self.seed).ok()?;
            let members: Vec<_> = definitions.regions.values().collect();
            let scope = self
                .package
                .activation_scope(identity, depth, &members, &definitions.rooms)
                .ok()??;
            let has_cluster = self
                .plan
                .as_ref()?
                .profiles
                .get(&scope.profile)?
                .stages
                .iter()
                .any(|stage| {
                    matches!(
                        stage.population,
                        runtime::PopulationContent::RoomCluster { .. }
                    )
                });
            let mut room_space = BTreeSet::new();
            if has_cluster {
                // Same immutable walls/openings order as activation_scope and
                // add_geometry. Room columns cannot touch generated portals;
                // mutable doors/occupancy are intentionally not consulted.
                for region in &members {
                    let mut walls: BTreeSet<_> = region.walls.iter().copied().collect();
                    for at in &region.openings {
                        walls.remove(at);
                    }
                    for floor in scope
                        .rooms
                        .iter()
                        .flat_map(|room| &room.cells)
                        .filter(|at| at.region.0 == region.id)
                    {
                        for z in 0..region.size[2] {
                            let position = [floor.position.x, floor.position.y, z];
                            if !walls.contains(&position) {
                                room_space.insert(loc(region.id, position));
                            }
                        }
                    }
                }
            }
            Some((scope, has_cluster.then_some(room_space)))
        })();
        let Some((expected, room_space)) = expected else {
            return false;
        };
        if let Some(room_space) = room_space {
            self.expected_room_space
                .borrow_mut()
                .insert(identity.into(), room_space);
        }
        self.expected_scopes
            .borrow_mut()
            .insert(identity.into(), expected);
        true
    }
}

#[cfg(test)]
#[path = "scenario_population_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "population_depth_authoring_tests.rs"]
mod population_depth_authoring_tests;
