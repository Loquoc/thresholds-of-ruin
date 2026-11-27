//! Immutable manifest definitions prepared for deterministic region construction.
//! Region instances remain lazy; this context contains no mutable game state and
//! is reconstructed rather than serialized into a save.
use super::{fail, label, require, Actor, AppearanceEntry, Archetype, Failure, Item, Manifest};
use crate::creature_authoring::{BuildSpec, CompiledCatalog, Recipe};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Debug)]
pub(super) enum RecipeError {
    Invalid(Failure),
    UnknownFaction(String),
}
impl From<Failure> for RecipeError {
    fn from(failure: Failure) -> Self {
        Self::Invalid(failure)
    }
}
impl RecipeError {
    pub fn into_failure(
        self,
        origin: super::diagnostics::Origin<'_>,
        source: impl FnOnce() -> Option<Arc<str>>,
    ) -> Failure {
        match self {
            Self::Invalid(failure) => origin.context(failure),
            Self::UnknownFaction(faction) => {
                let source = source();
                origin.reference_path(
                    fail("Unknown creature faction"),
                    source.as_deref(),
                    &[
                        super::diagnostics::PathSegment::Field("creature"),
                        super::diagnostics::PathSegment::Field("faction"),
                    ],
                    super::diagnostics::ReferenceValue::Text(&faction),
                )
            }
        }
    }
}

fn prepare_recipe(
    author: Option<&BuildSpec>,
    anatomy: bool,
    catalog: &CompiledCatalog,
    manifest: &Manifest,
) -> Result<Option<Recipe>, RecipeError> {
    let Some(author) = author else {
        return Ok(None);
    };
    require(!anatomy, "Creature builds cannot also specify anatomy")?;
    let recipe = catalog.prepare(author)?;
    if !manifest.factions.is_empty() && !manifest.factions.contains_key(&author.faction) {
        return Err(RecipeError::UnknownFaction(author.faction.clone()));
    }
    Ok(Some(recipe))
}

#[derive(Clone, Debug)]
pub(super) enum AiReference {
    Resolved(Arc<super::AiProfile>),
    // Defer reporting until the original AI configuration boundary, so earlier
    // combat/geometry failures retain their precedence. Names are diagnostic only.
    Missing(String),
}

#[derive(Clone, Debug)]
pub(super) enum PreparedControl {
    External,
    Omitted,
    Ai(AiReference),
}

impl PreparedControl {
    fn ai(name: Option<&String>, profiles: &BTreeMap<String, Arc<super::AiProfile>>) -> Self {
        Self::Ai(match name.and_then(|name| profiles.get(name)) {
            Some(profile) => AiReference::Resolved(profile.clone()),
            None if name.is_none() => AiReference::Resolved(Arc::new(super::AiProfile::default())),
            None => AiReference::Missing(name.cloned().expect("named profile")),
        })
    }

    pub fn spawned(&self) -> bool {
        match self {
            Self::External | Self::Ai(_) => true,
            Self::Omitted => false,
        }
    }

    pub fn profile(&self) -> Result<Option<&super::AiProfile>, MissingAiReference<'_>> {
        match self {
            Self::Ai(AiReference::Resolved(profile)) => Ok(Some(profile)),
            Self::Ai(AiReference::Missing(name)) => Err(MissingAiReference { name }),
            Self::External | Self::Omitted => Ok(None),
        }
    }
}

#[derive(Debug)]
pub(super) struct MissingAiReference<'a> {
    pub name: &'a str,
}

#[derive(Debug)]
pub(super) struct MissingArchetype<'a> {
    pub name: &'a str,
}

impl From<MissingArchetype<'_>> for Failure {
    fn from(reference: MissingArchetype<'_>) -> Self {
        fail(format!("Unknown archetype {}", reference.name))
    }
}

/// Runtime definitions only. Inherited definitions are borrowed until installed;
/// inline authoring values are converted once and then moved into the game.
#[derive(Debug)]
pub(super) struct PreparedCreature<'a> {
    pub recipe: Option<Cow<'a, Recipe>>,
    pub anatomy: Option<Cow<'a, tor_simulation::AnatomySpec>>,
    pub known_identities: Vec<tor_simulation::ItemIdentity>,
    pub body: Option<Cow<'a, tor_simulation::BodySpec>>,
    pub control: PreparedControl,
    pub velocity: Option<[i64; 3]>,
    pub asset: Option<Cow<'a, str>>,
}

impl PreparedCreature<'_> {
    pub fn borrowed(&self) -> PreparedCreature<'_> {
        PreparedCreature {
            recipe: self.recipe.as_deref().map(Cow::Borrowed),
            anatomy: self.anatomy.as_deref().map(Cow::Borrowed),
            known_identities: self.known_identities.clone(),

            body: self.body.as_deref().map(Cow::Borrowed),
            control: self.control.clone(),
            velocity: self.velocity,
            asset: self.asset.as_deref().map(Cow::Borrowed),
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct PreparedArchetype {
    pub recipe: Option<Recipe>,
    pub anatomy: Option<tor_simulation::AnatomySpec>,
    pub equipment: Option<tor_simulation::EquipmentSpec>,
    pub projectile: Option<tor_simulation::projectile_profiles::ProjectileSpec>,
    pub equipment_generation: Option<tor_simulation::EquipmentGeneration>,
    pub consumable: Option<tor_simulation::ConsumableSpec>,
    pub readable: Option<tor_simulation::ReadableSpec>,
    pub ground_scare: Option<tor_simulation::control_scrolls::ScareSpec>,
    pub device: Option<tor_simulation::DeviceSpec>,
    pub held_light: Option<tor_simulation::HeldLightSpec>,
    pub edible: Option<tor_simulation::EdibleSpec>,
    pub supply: Option<tor_simulation::content_ids::SupplyId>,
    pub magical: bool,
    pub currency: Option<tor_simulation::CurrencyKind>,
    pub class: tor_simulation::ItemClass,
    pub body: Option<tor_simulation::BodySpec>,
    pub identity: Option<tor_simulation::ItemIdentity>,
    pub concealed: bool,
    pub stackable: bool,
    pub properties: BTreeMap<String, String>,
    pub name: Option<String>,
    pub turn_ticks: Option<u64>,
    pub actor_asset: Option<String>,
    pub item_asset: Option<String>,
}

impl PreparedArchetype {
    fn wizard_item_identity(&self) -> Option<&tor_simulation::ItemIdentity> {
        self.name.as_ref()?;
        self.identity.as_ref()
    }

    fn wizard_item_appearance<'a>(
        &'a self,
        identity: &tor_simulation::ItemIdentity,
        appearances: &'a BTreeMap<tor_simulation::ItemIdentity, AppearanceEntry>,
    ) -> Option<&'a str> {
        // An assigned entry without an asset intentionally suppresses the
        // archetype's identity asset, just as the ordinary item adapter does.
        appearances
            .get(identity)
            .map_or(self.item_asset.as_deref(), |entry| entry.asset.as_deref())
    }

    fn new(
        author: &Archetype,
        manifest: &Manifest,
        catalog: &CompiledCatalog,
    ) -> Result<Self, RecipeError> {
        // Item payloads are not part of the Ruleset ability graph. Resolve their
        // roster references here, after catalog imports and eligibility sealing,
        // rather than deferring an invalid authored form until item use.
        for effect in author
            .consumable
            .iter()
            .flat_map(|spec| &spec.effects)
            .chain(author.readable.iter().flat_map(|spec| &spec.effects))
        {
            if let super::Effect::BalefulPolymorph { forms } = effect {
                for form in forms {
                    let id = tor_simulation::content_ids::BodyFormId::new(form)
                        .map_err(|_| fail("Invalid polymorph body package ID"))?;
                    require(
                        catalog.content().polymorph_body_package(&id).is_some(),
                        "Unknown or ineligible polymorph body package",
                    )?;
                }
            }
        }
        Ok(Self {
            recipe: prepare_recipe(
                author.creature.as_ref(),
                author.anatomy.is_some(),
                catalog,
                manifest,
            )?,
            anatomy: author
                .anatomy
                .as_ref()
                .map(|anatomy| anatomy.compile(catalog.content().rules()))
                .transpose()?,
            equipment: author
                .equipment
                .as_ref()
                .map(|equipment| equipment.compile(catalog.content().rules()))
                .transpose()?,
            projectile: author
                .projectile
                .as_ref()
                .map(|value| value.compile(catalog.content().rules()))
                .transpose()?,
            equipment_generation: author
                .equipment_generation
                .map(super::EquipmentGeneration::compile)
                .transpose()?,
            consumable: author
                .consumable
                .as_ref()
                .map(|consumable| consumable.compile(catalog.content().rules()))
                .transpose()?,
            ground_scare: author.ground_scare.clone(),
            readable: author
                .readable
                .as_ref()
                .map(|readable| readable.compile(catalog.content().rules()))
                .transpose()?,
            device: author
                .device
                .as_ref()
                .map(|device| device.compile(catalog.content().rules()))
                .transpose()?,
            held_light: author
                .held_light
                .map(super::HeldLightSpec::compile)
                .transpose()?,
            edible: author.edible.clone(),
            supply: author
                .supply
                .as_ref()
                .map(|value| {
                    let id = tor_simulation::content_ids::SupplyId::new(value)
                        .map_err(|_| fail(format!("Invalid supply ID {value:?}")))?;
                    require(
                        catalog.content().rules().supply(&id).is_some(),
                        "Unknown supply",
                    )?;
                    require(
                        author.equipment.is_none(),
                        "Equipment cannot also be supply content",
                    )?;
                    Ok::<_, Failure>(id)
                })
                .transpose()?,
            magical: author.magical,
            currency: author.currency.map(Into::into),
            class: author.class.into(),

            body: author.body.clone().map(Into::into),
            identity: author.identity.clone(),
            concealed: author.appearance_pool.is_some(),
            stackable: author.stackable,
            properties: author.properties.clone(),
            name: author.name.clone(),
            turn_ticks: author.turn_ticks,
            actor_asset: author.asset.clone(),
            item_asset: item_asset(author, manifest).map(str::to_owned),
        })
    }
}

/// A concealed item's key is assigned with its seeded public appearance at
/// birth, never inherited from its true identity's archetype.
pub(super) fn item_asset<'a>(author: &'a Archetype, _manifest: &'a Manifest) -> Option<&'a str> {
    author
        .appearance_pool
        .is_none()
        .then_some(author.asset.as_deref())
        .flatten()
}

/// Forecast every appearance of every possible form, independent of which
/// secret identity receives which appearance in a particular world.
pub(super) fn item_assets<'a>(
    author: &'a Archetype,
    manifest: &'a Manifest,
) -> impl Iterator<Item = &'a str> {
    let pools = author
        .forms
        .iter()
        .map(|form| form.appearance_pool.as_str())
        .chain(
            author
                .forms
                .is_empty()
                .then_some(author.appearance_pool.as_deref())
                .flatten(),
        );
    pools
        .filter_map(|key| manifest.appearance_pools.get(key))
        .flat_map(|pool| {
            pool.appearances
                .iter()
                .filter_map(|entry| entry.asset.as_deref())
        })
        .chain(
            (author.forms.is_empty() && author.appearance_pool.is_none())
                .then_some(author.asset.as_deref())
                .flatten(),
        )
}

#[derive(Debug)]
pub(super) struct PreparedCharacter {
    pub id: u64,
    pub anchor: String,
    pub turn_ticks: u64,
    pub creature: PreparedCreature<'static>,
    pub known_identities: Vec<tor_simulation::ItemIdentity>,
}

#[derive(Debug)]
pub(super) struct PreparedDefinitions {
    catalog: CompiledCatalog,
    factions: BTreeSet<String>,
    archetypes: BTreeMap<String, PreparedArchetype>,
    ordinary: PreparedArchetype,
    pub characters: Vec<PreparedCharacter>,
    ai_profiles: BTreeMap<String, Arc<super::AiProfile>>,
    omitted: BTreeSet<u64>,
}

/// Prepare ordinary recoverable ammunition before any creature build captures
/// Content. Reuse the exact normal item adapter, without allocating an entity.
fn compile_trap_ammunition(
    manifest: &Manifest,
    catalog: CompiledCatalog,
) -> Result<CompiledCatalog, Failure> {
    let archetypes = manifest
        .archetypes
        .iter()
        .filter(|(_, author)| {
            author.projectile.is_some()
                && author.stackable
                && !author.magical
                && author.appearance_pool.is_none()
                && author.equipment_generation.is_none()
        })
        .map(|(name, author)| {
            PreparedArchetype::new(author, manifest, &catalog)
                .map(|prepared| (name.clone(), prepared))
                .map_err(|error| {
                    error.into_failure(
                        super::diagnostics::Origin::Archetype {
                            file: "scenario.toml",
                            name,
                            catalog: "archetypes",
                        },
                        || None,
                    )
                })
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let definitions = PreparedDefinitions {
        catalog,
        factions: BTreeSet::new(),
        archetypes,
        ordinary: PreparedArchetype::default(),
        characters: vec![],
        ai_profiles: BTreeMap::new(),
        omitted: BTreeSet::new(),
    };
    let ammunition = definitions
        .archetypes
        .keys()
        .map(|key| {
            definitions
                .population_item(key, 1, 0, &BTreeMap::new())
                .map(|spec| (key.clone(), spec))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    definitions.catalog.with_trap_ammunition(ammunition)
}

/// Shared normal/source-free immutable decorations, before any actor captures Content.
pub(super) fn compile_content_decorations(
    manifest: &Manifest,
    catalog: CompiledCatalog,
) -> Result<CompiledCatalog, Failure> {
    let catalog = catalog.with_presentation_catalog(super::presentation::compile(manifest)?);
    compile_trap_ammunition(manifest, catalog)
}

impl PreparedDefinitions {
    pub fn creature_catalog(&self) -> &CompiledCatalog {
        &self.catalog
    }

    #[cfg(test)]
    pub fn new(manifest: &Manifest, selected: u64) -> Result<Self, Failure> {
        Self::new_with_source(
            manifest,
            selected,
            crate::scenario_package::parse_rules(
                include_str!("../../simulation/data/default-ruleset.toml"),
                "test rules",
            )
            .unwrap(),
            || None,
        )
    }
    #[cfg(test)]
    pub fn new_with_source(
        manifest: &Manifest,
        selected: u64,
        rules: Arc<tor_simulation::rules::Ruleset>,
        source: impl Fn() -> Option<Arc<str>>,
    ) -> Result<Self, Failure> {
        Self::new_with_sources(manifest, selected, rules, &BTreeMap::new(), |_| source())
    }

    pub fn new_with_sources(
        manifest: &Manifest,
        selected: u64,
        rules: Arc<tor_simulation::rules::Ruleset>,
        provenance: &BTreeMap<String, String>,
        source: impl Fn(&str) -> Option<Arc<str>>,
    ) -> Result<Self, Failure> {
        let catalog = manifest
            .creatures
            .compile_with_context(rules, |kind, id, failure| {
                super::source::catalog_context(provenance, kind, id, failure)
            })?;
        let catalog = compile_content_decorations(manifest, catalog)?;
        let ai_profiles: BTreeMap<_, _> = manifest
            .ai_profiles
            .iter()
            .map(|(name, author)| (name.clone(), Arc::new(author.clone())))
            .collect();
        let characters: Vec<_> =
            manifest
                .characters
                .iter()
                .map(|author| {
                    Ok(PreparedCharacter {
                        id: author.id,
                        anchor: author.anchor.clone(),
                        turn_ticks: author.turn_ticks,
                        creature: PreparedCreature {
                            recipe: prepare_recipe(
                                author.creature.as_ref(),
                                author.anatomy.is_some(),
                                &catalog,
                                manifest,
                            )
                            .map_err(|failure| {
                                failure.into_failure(
                                    super::diagnostics::Origin::Character(author.id),
                                    || source("scenario.toml"),
                                )
                            })?
                            .map(Cow::Owned),
                            anatomy: author
                                .anatomy
                                .as_ref()
                                .map(|anatomy| {
                                    anatomy.compile(catalog.content().rules()).map(Cow::Owned)
                                })
                                .transpose()?,
                            known_identities: author.known_identities.clone(),

                            body: author.body.clone().map(|body| Cow::Owned(body.into())),
                            control: if author.id == selected
                                && !manifest.arena.as_ref().is_some_and(|arena| {
                                    arena.control == super::ArenaControl::AllAi
                                }) {
                                PreparedControl::External
                            } else if author.unselected == "ai" {
                                PreparedControl::ai(author.ai.as_ref(), &ai_profiles)
                            } else {
                                PreparedControl::Omitted
                            },
                            velocity: author.velocity,
                            asset: author.asset.clone().map(Cow::Owned),
                        },
                        known_identities: author.known_identities.clone(),
                    })
                })
                .collect::<Result<_, Failure>>()?;
        let omitted = characters
            .iter()
            .filter(|c| !c.creature.control.spawned())
            .map(|c| c.id)
            .collect();
        Ok(Self {
            archetypes: manifest
                .archetypes
                .iter()
                .map(|(name, author)| {
                    PreparedArchetype::new(author, manifest, &catalog)
                        .map(|prepared| (name.clone(), prepared))
                        .map_err(|failure| {
                            let file =
                                super::source::declaration_file(provenance, "archetypes", name);
                            failure.into_failure(
                                super::source::archetype_origin(provenance, name),
                                || source(file),
                            )
                        })
                })
                .collect::<Result<_, Failure>>()?,
            catalog,
            factions: manifest.factions.keys().cloned().collect(),
            ordinary: PreparedArchetype::default(),
            characters,
            ai_profiles,
            omitted,
        })
    }

    /// Resolve only physical form, once for each identity and run. The retained
    /// source and full cosmetic roster stay unchanged; later births reuse this
    /// prepared context and a reopened context repeats the same seeded choice.
    pub(super) fn select_item_forms(
        &mut self,
        manifest: &Manifest,
        seed: u64,
        provenance: &BTreeMap<String, String>,
    ) -> Result<(), Failure> {
        for (key, author) in &manifest.archetypes {
            let Some(form) = super::item_forms::selected(author, key, seed) else {
                continue;
            };
            let resolved = super::item_forms::apply(author, form);
            let prepared =
                PreparedArchetype::new(&resolved, manifest, &self.catalog).map_err(|failure| {
                    failure.into_failure(super::source::archetype_origin(provenance, key), || None)
                })?;
            self.archetypes.insert(key.clone(), prepared);
        }
        Ok(())
    }

    pub(super) fn concealed_item_names(
        &self,
    ) -> impl Iterator<Item = (&tor_simulation::ItemIdentity, &str)> {
        self.archetypes.values().filter_map(|definition| {
            if !definition.concealed {
                return None;
            }
            Some((definition.identity.as_ref()?, definition.name.as_deref()?))
        })
    }

    /// Borrow only catalog labels; listing never copies or instantiates HD rows.
    pub fn creature_archetypes(&self) -> impl Iterator<Item = (&str, &str)> {
        self.archetypes.iter().filter_map(|(id, definition)| {
            definition
                .recipe
                .as_ref()
                .map(|recipe| (id.as_str(), recipe.identity().name.as_str()))
        })
    }

    pub fn archetype<'a>(
        &self,
        key: &'a Option<String>,
    ) -> Result<&PreparedArchetype, MissingArchetype<'a>> {
        match key {
            Some(key) => self
                .archetypes
                .get(key)
                .ok_or(MissingArchetype { name: key }),
            None => Ok(&self.ordinary),
        }
    }

    pub fn omitted_carrier(&self, carrier: Option<u64>) -> bool {
        carrier.is_some_and(|id| self.omitted.contains(&id))
    }

    pub fn actor(&self, author: &Actor) -> Result<PreparedCreature<'_>, RecipeError> {
        let inherited = self.archetype(&author.archetype).map_err(Failure::from)?;
        let recipe = author
            .creature
            .as_ref()
            .map(|build| {
                let recipe = self.catalog.prepare(build)?;
                if !self.factions.is_empty() && !self.factions.contains(&build.faction) {
                    return Err(RecipeError::UnknownFaction(build.faction.clone()));
                }
                Ok(recipe)
            })
            .transpose()?
            .map(Cow::Owned)
            .or_else(|| inherited.recipe.as_ref().map(Cow::Borrowed));
        require(
            recipe.is_none() || (author.anatomy.is_none() && inherited.anatomy.is_none()),
            "Creature builds cannot also specify anatomy",
        )?;
        Ok(PreparedCreature {
            recipe,
            anatomy: author
                .anatomy
                .as_ref()
                .map(|anatomy| {
                    anatomy
                        .compile(self.catalog.content().rules())
                        .map(Cow::Owned)
                })
                .transpose()?
                .or_else(|| inherited.anatomy.as_ref().map(Cow::Borrowed)),
            known_identities: author.known_identities.clone(),

            body: author
                .body
                .clone()
                .map(|body| Cow::Owned(body.into()))
                .or_else(|| inherited.body.as_ref().map(Cow::Borrowed)),
            control: if author.controller == "ai" {
                PreparedControl::ai(author.ai.as_ref(), &self.ai_profiles)
            } else {
                PreparedControl::External
            },
            velocity: author.velocity,
            asset: inherited.actor_asset.as_deref().map(Cow::Borrowed),
        })
    }

    pub(super) fn wizard_item_catalog(
        &self,
        after: Option<&str>,
        appearances: &BTreeMap<tor_simulation::ItemIdentity, AppearanceEntry>,
    ) -> Result<super::WizardItemCatalogPage, Failure> {
        use std::ops::Bound::{Excluded, Unbounded};
        const LIMIT: usize = 16;
        if let Some(after) = after {
            require(
                self.archetypes
                    .get(after)
                    .and_then(PreparedArchetype::wizard_item_identity)
                    .is_some(),
                "Unknown wizard item catalog cursor; use an exact recipe from item list",
            )?;
        }
        let mut rows: Vec<_> = self
            .archetypes
            .range::<str, _>((after.map_or(Unbounded, Excluded), Unbounded))
            .filter_map(|(key, definition)| {
                let identity = definition.wizard_item_identity()?;
                Some(super::WizardItemCatalogRow {
                    recipe: key.clone(),
                    identity: identity.clone(),
                    appearance: definition
                        .wizard_item_appearance(identity, appearances)
                        .map(str::to_owned),
                })
            })
            .take(LIMIT + 1)
            .collect();
        let next = if rows.len() > LIMIT {
            rows.truncate(LIMIT);
            rows.last().map(|row| row.recipe.clone())
        } else {
            None
        };
        Ok(super::WizardItemCatalogPage { rows, next })
    }

    /// Resolve only prepared authored items, never sampled instances or raw
    /// appearance pools. BTreeMap order chooses the canonical equivalent alias.
    pub(super) fn wizard_item(
        &self,
        identity: Option<&tor_simulation::ItemIdentity>,
        appearance: Option<&str>,
        recipe: Option<&str>,
        seed: u64,
        appearances: &BTreeMap<tor_simulation::ItemIdentity, AppearanceEntry>,
    ) -> Result<tor_simulation::ItemSpec, Failure> {
        let mut selected: Option<tor_simulation::ItemSpec> = None;
        for (key, definition) in &self.archetypes {
            if recipe.is_some_and(|recipe| recipe != key.as_str()) {
                continue;
            }
            let Some(candidate) = definition.wizard_item_identity() else {
                continue;
            };
            if identity.is_some_and(|id| id != candidate) {
                continue;
            }
            let asset = definition.wizard_item_appearance(candidate, appearances);
            if appearance.is_some_and(|selector| Some(selector) != asset) {
                continue;
            }
            let mut spec = self.population_item(key, 1, seed, appearances)?;
            if let Some(prior) = &selected {
                spec.archetype.clone_from(&prior.archetype);
                require(
                    &spec == prior,
                    "Ambiguous wizard item; specify an exact recipe",
                )?;
            } else {
                selected = Some(spec);
            }
        }
        selected.ok_or_else(|| fail("No authored item matches the wizard item selectors"))
    }

    /// Resolve a catalog item through the ordinary item-definition adapter.
    /// The zero authoring ID is only a resolver sentinel, never an entity allocation.
    pub fn population_item(
        &self,
        archetype: &str,
        quantity: u64,
        seed: u64,
        appearances: &BTreeMap<tor_simulation::ItemIdentity, AppearanceEntry>,
    ) -> Result<tor_simulation::ItemSpec, Failure> {
        self.item(
            &Item {
                id: 0,
                at: [0; 3],
                archetype: Some(archetype.into()),
                quantity,
                equipped_slot: None,
                class: None,
                stackable: None,
                properties: BTreeMap::new(),
                name: None,
                identity: None,
                carried_by: None,
                seed_names: vec![],
            },
            seed,
            appearances,
        )
    }

    pub fn population_actor(
        &self,
        archetype: &str,
        weight: u32,
        ai: Option<&str>,
    ) -> Result<tor_simulation::floor_activation::PreparedActorChoice, Failure> {
        let key = Some(archetype.to_owned());
        let definition = self.archetype(&key).map_err(Failure::from)?;
        let turn_ticks = definition.turn_ticks.unwrap_or(100);
        require(turn_ticks > 0, "Zero population actor duration")?;
        let ai = ai
            .map(|name| {
                self.ai_profiles
                    .get(name)
                    .map(|profile| {
                        profile.resolve(
                            definition
                                .recipe
                                .as_ref()
                                .map_or_else(Default::default, Recipe::behavior),
                        )
                    })
                    .ok_or_else(|| fail(format!("Unknown population AI profile {name:?}")))
            })
            .transpose()?;
        require(
            ai.is_none() || definition.recipe.is_some(),
            "Population AI requires a creature build",
        )?;
        Ok(tor_simulation::floor_activation::PreparedActorChoice {
            advancement: vec![],
            depth_growth: tor_simulation::prepared_spawn::DepthGrowth::Fixed {},
            content: archetype.into(),
            weight,
            turn_ticks,
            carried_loot: None,
            actor: tor_simulation::prepared_spawn::PreparedActor {
                build: definition.recipe.as_ref().map(Recipe::recorded),
                nutrition: definition.recipe.as_ref().map_or(
                    tor_simulation::nutrition::NutritionRequirement::Exempt,
                    Recipe::nutrition,
                ),
                identity: definition
                    .recipe
                    .as_ref()
                    .map(|recipe| recipe.identity().clone()),
                anatomy: definition.anatomy.clone(),
                body: definition.body.clone(),
                ai,
                known_identities: vec![],
                velocity: None,
                asset: definition.actor_asset.clone(),
            },
        })
    }

    pub fn item(
        &self,
        author: &Item,
        seed: u64,
        appearances: &BTreeMap<tor_simulation::ItemIdentity, AppearanceEntry>,
    ) -> Result<tor_simulation::ItemSpec, Failure> {
        let inherited = self.archetype(&author.archetype).map_err(Failure::from)?;
        require(
            !inherited.concealed
                || author
                    .class
                    .is_none_or(|class| tor_simulation::ItemClass::from(class) == inherited.class),
            "Concealed items cannot override their physical class",
        )?;
        require(
            author.seed_names.iter().all(|entry| {
                !entry.identity.is_unknown() && entry.identity.as_str().split('.').count() >= 3
            }),
            "Every seeded item identity must be a known leaf",
        )?;
        require(
            author.seed_names.iter().all(|entry| {
                inherited
                    .identity
                    .as_ref()
                    .is_none_or(|known| known == &entry.identity)
            }),
            "Seeded item cannot override its inherited identity",
        )?;
        let seeded = (!author.seed_names.is_empty())
            .then(|| &author.seed_names[(seed % author.seed_names.len() as u64) as usize]);
        require(
            seeded.is_none() || (author.name.is_none() && author.identity.is_none()),
            "Item cannot combine seed_names with a direct name or identity",
        )?;
        let name = seeded
            .map(|entry| entry.name.clone())
            .or_else(|| author.name.clone())
            .or_else(|| inherited.name.clone())
            .ok_or_else(|| fail("Item needs a name or archetype"))?;
        require(
            label(&name) && author.seed_names.iter().all(|entry| label(&entry.name)),
            "Invalid item name",
        )?;
        let identity = seeded
            .map(|entry| entry.identity.clone())
            .or_else(|| author.identity.clone())
            .or_else(|| inherited.identity.clone())
            .ok_or_else(|| fail("Concrete item requires an explicit canonical identity"))?;
        require(
            !identity.is_unknown() && identity.as_str().split('.').count() >= 3,
            "Concrete item identity must be a known leaf",
        )?;
        require(
            inherited
                .identity
                .as_ref()
                .is_none_or(|known| known == &identity),
            "Concrete item cannot override its inherited identity",
        )?;
        let key = author
            .archetype
            .clone()
            .unwrap_or_else(|| identity.to_string());
        require(
            !inherited.concealed || (author.name.is_none() && author.seed_names.is_empty()),
            "Concealed items cannot override their identity name",
        )?;
        let assigned = appearances.get(&identity);
        let appearance = assigned.map_or_else(|| name.clone(), |entry| entry.name.clone());
        let appearance_asset =
            assigned.map_or_else(|| inherited.item_asset.clone(), |entry| entry.asset.clone());
        let stackable = author.stackable.unwrap_or(inherited.stackable);
        let mut properties = inherited.properties.clone();
        properties.extend(author.properties.clone());
        let spec = tor_simulation::ItemSpec {
            equipment: inherited.equipment.clone(),
            projectile: inherited.projectile.clone(),
            equipment_generation: inherited.equipment_generation,
            consumable: inherited.consumable.clone(),
            readable: inherited.readable.clone(),
            ground_scare: inherited.ground_scare.clone(),
            device: inherited.device.clone(),
            held_light: inherited.held_light,
            edible: inherited.edible.clone(),
            supply: inherited.supply.clone(),
            magical: inherited.magical,
            currency: inherited.currency,
            class: author.class.map(Into::into).unwrap_or(inherited.class),
            archetype: key,
            identity,
            name,
            appearance,
            concealed: inherited.concealed,
            stackable,
            properties,
            asset: appearance_asset,
        };
        require(
            spec.device.is_none() || (!stackable && author.quantity == 1),
            "Device instances must be non-stackable single items",
        )?;
        require(
            author.quantity > 0 && (stackable || author.quantity == 1),
            "Invalid item quantity or non-stackable count",
        )?;
        require(
            spec.properties.len() <= 32
                && spec
                    .properties
                    .iter()
                    .all(|(k, v)| label(k) && v.len() <= 80 && !v.chars().any(char::is_control)),
            "Invalid item properties",
        )?;
        Ok(spec)
    }
}

#[cfg(test)]
#[path = "scenario_compiler_tests.rs"]
mod tests;
