//! Actor-owned earned XP and explicit future HD source policy.
//!
//! XP applies HD immediately when its declared source is available. A missing
//! source retains the XP balance; it never silently spends or discards it.
use crate::creatures::{BuildError, CreatureBuild, DerivedCreature, PurchaseSelection};
use crate::rules::{GrowthPolicy, ThreatSource};
use crate::HdCount;
use serde::{Deserialize, Serialize};

/// One bounded causal origin. Later condition/motion adapters must carry their
/// original owner and episode through the applied effect, not guess a last hit.
/// This transient type is not a wire capability or a saved death-ID ledger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FatalOrigin {
    Direct {
        ability: Option<crate::content_ids::AbilityId>,
        intention: Option<crate::IntentionId>,
        strike: Option<u16>,
        effect: u16,
    },
    Condition {
        condition: crate::content_ids::ConditionId,
        application: u64,
    },
    Displacement {
        episode: std::num::NonZeroU64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FatalCause {
    pub owner: crate::ActorId,
    pub origin: FatalOrigin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrowthError {
    InvalidRecord,
    Arithmetic,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GrowthSource {
    Racial {},
    Class { class: crate::content_ids::ClassId },
}
impl<'de> Deserialize<'de> for GrowthSource {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Racial {},
            Class { class: crate::content_ids::ClassId },
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Racial {} => Self::Racial {},
            Fields::Class { class } => Self::Class { class },
        })
    }
}
impl GrowthSource {
    pub fn hd_source(&self) -> crate::progression::HdSource {
        match self {
            Self::Racial {} => crate::progression::HdSource::Racial,
            Self::Class { class } => crate::progression::HdSource::Class(class.clone()),
        }
    }
    pub fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        match self {
            Self::Racial {} => true,
            Self::Class { class } => rules.class(class).is_some(),
        }
    }
}

/// Ordinary growth is explicit data. Control ownership never enables NPC growth.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FutureGrowth {
    Disabled {},
    Selected { source: GrowthSource },
    Authored {},
}
impl<'de> Deserialize<'de> for FutureGrowth {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Disabled {},
            Selected { source: GrowthSource },
            Authored {},
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Disabled {} => Self::Disabled {},
            Fields::Selected { source } => Self::Selected { source },
            Fields::Authored {} => Self::Authored {},
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EarnedProgress {
    authored_baseline: HdCount,
    highest_applied_tier: HdCount,
    highest_earned_tier: HdCount,
    peak_threat: u64,
    earned_xp: u64,
    spent_xp: u64,
    held_xp: u64,
}
impl<'de> Deserialize<'de> for EarnedProgress {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            authored_baseline: HdCount,
            highest_applied_tier: HdCount,
            highest_earned_tier: HdCount,
            peak_threat: u64,
            earned_xp: u64,
            spent_xp: u64,
            held_xp: u64,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            authored_baseline: f.authored_baseline,
            highest_applied_tier: f.highest_applied_tier,
            highest_earned_tier: f.highest_earned_tier,
            peak_threat: f.peak_threat,
            earned_xp: f.earned_xp,
            spent_xp: f.spent_xp,
            held_xp: f.held_xp,
        };
        if !value.shape_valid() {
            return Err(serde::de::Error::custom("invalid earned progression"));
        }
        Ok(value)
    }
}
impl EarnedProgress {
    pub(crate) fn new(hit_dice: HdCount, threat: u64) -> Self {
        Self {
            authored_baseline: hit_dice,
            highest_applied_tier: hit_dice,
            highest_earned_tier: hit_dice,
            peak_threat: threat,
            earned_xp: 0,
            spent_xp: 0,
            held_xp: 0,
        }
    }
    pub fn highest_earned_tier(&self) -> HdCount {
        self.highest_earned_tier
    }
    pub fn highest_applied_tier(&self) -> HdCount {
        self.highest_applied_tier
    }
    pub fn peak_threat(&self) -> u64 {
        self.peak_threat
    }
    pub fn held_xp(&self) -> u64 {
        self.held_xp
    }
    pub fn earned_xp(&self) -> u64 {
        self.earned_xp
    }
    pub fn spent_xp(&self) -> u64 {
        self.spent_xp
    }
    pub fn next_threshold(&self, policy: &GrowthPolicy) -> Result<u64, GrowthError> {
        policy
            .threshold(self.highest_applied_tier)
            .ok_or(GrowthError::Arithmetic)
    }
    fn shape_valid(&self) -> bool {
        self.authored_baseline <= self.highest_applied_tier
            && self.highest_applied_tier <= self.highest_earned_tier
            && self.peak_threat > 0
            && self.spent_xp.checked_add(self.held_xp) == Some(self.earned_xp)
    }
    pub(crate) fn validate(
        &self,
        current: HdCount,
        policy: &GrowthPolicy,
    ) -> Result<(), GrowthError> {
        if self.shape_valid()
            && current <= self.highest_applied_tier
            && self.next_threshold(policy).is_ok()
            && policy
                .threshold_sum(self.authored_baseline, self.highest_applied_tier)
                .is_some_and(|minimum| self.spent_xp >= minimum)
            && policy
                .threshold_sum(self.authored_baseline, self.highest_earned_tier)
                .is_some_and(|minimum| self.earned_xp >= minimum)
            && self
                .projected_tier(current, policy)
                .is_ok_and(|tier| tier <= self.highest_earned_tier)
        {
            Ok(())
        } else {
            Err(GrowthError::InvalidRecord)
        }
    }
    fn projected_tier(
        &self,
        current: HdCount,
        policy: &GrowthPolicy,
    ) -> Result<HdCount, GrowthError> {
        let tier = self.highest_applied_tier;
        let lost = tier
            .checked_sub(current)
            .ok_or(GrowthError::InvalidRecord)?
            .get();
        let price = self.next_threshold(policy)?;
        if price == 0 || policy.threshold_base == 0 {
            return Err(GrowthError::InvalidRecord);
        }
        let mut xp = self.held_xp;
        // Regrowth pays today's applied-tier price, not the historic price of
        // each lost ordinal, and cannot increase the applied-tier watermark.
        let restored = lost.min(xp / price);
        xp -= restored.checked_mul(price).ok_or(GrowthError::Arithmetic)?;
        if restored < lost {
            return Ok(tier);
        }

        let room = u64::MAX - tier.get();
        let mut low = 0;
        let mut high = room.min(xp / u64::from(policy.threshold_base));
        while low < high {
            let distance = high - low;
            let middle = low + distance / 2 + distance % 2;
            let end = tier.checked_add(middle).ok_or(GrowthError::Arithmetic)?;
            if policy
                .threshold_sum(tier, end)
                .is_some_and(|cost| cost <= xp)
            {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        let projected = tier.checked_add(low).ok_or(GrowthError::Arithmetic)?;
        if low == room {
            let spent = policy
                .threshold_sum(tier, projected)
                .ok_or(GrowthError::Arithmetic)?;
            // Exhausted representation is a hard error if another ordinal is
            // affordable. A binary-search endpoint must never become a plateau.
            if policy
                .threshold(projected)
                .is_some_and(|next| xp - spent >= next)
            {
                return Err(GrowthError::Arithmetic);
            }
        }
        Ok(projected)
    }
    pub(crate) fn next_unfunded_credit(
        &self,
        current: HdCount,
        policy: &GrowthPolicy,
    ) -> Result<u64, GrowthError> {
        let lost = self
            .highest_applied_tier
            .checked_sub(current)
            .ok_or(GrowthError::InvalidRecord)?
            .get();
        let price = self.next_threshold(policy)?;
        if price == 0 {
            return Err(GrowthError::InvalidRecord);
        }
        if self.held_xp / price < lost {
            // Earlier funded regrowth ordinals all have this same current price.
            return Ok(price - self.held_xp % price);
        }
        let regrowth = lost.checked_mul(price).ok_or(GrowthError::Arithmetic)?;
        let forward_xp = self
            .held_xp
            .checked_sub(regrowth)
            .ok_or(GrowthError::InvalidRecord)?;
        let funded = self.projected_tier(current, policy)?;
        funded.checked_add(1).ok_or(GrowthError::Arithmetic)?;
        let forward_cost = policy
            .threshold_sum(self.highest_applied_tier, funded)
            .ok_or(GrowthError::Arithmetic)?;
        let residual = forward_xp
            .checked_sub(forward_cost)
            .ok_or(GrowthError::InvalidRecord)?;
        let next = policy.threshold(funded).ok_or(GrowthError::Arithmetic)?;
        next.checked_sub(residual)
            .filter(|credit| *credit > 0)
            .ok_or(GrowthError::InvalidRecord)
    }

    pub(crate) fn award(
        &mut self,
        amount: u32,
        current: HdCount,
        policy: &GrowthPolicy,
    ) -> Result<(), GrowthError> {
        self.award_wide(u64::from(amount), current, policy)
    }

    pub(crate) fn award_wide(
        &mut self,
        amount: u64,
        current: HdCount,
        policy: &GrowthPolicy,
    ) -> Result<(), GrowthError> {
        let earned = self
            .earned_xp
            .checked_add(amount)
            .ok_or(GrowthError::Arithmetic)?;
        let held = self
            .held_xp
            .checked_add(amount)
            .ok_or(GrowthError::Arithmetic)?;
        let mut candidate = self.clone();
        candidate.earned_xp = earned;
        candidate.held_xp = held;
        candidate.highest_earned_tier = candidate
            .highest_earned_tier
            .max(candidate.projected_tier(current, policy)?);
        *self = candidate;
        Ok(())
    }
    pub(crate) fn reconcile_personal_build(
        &mut self,
        current: HdCount,
        threat: u64,
        policy: &GrowthPolicy,
    ) -> Result<(), GrowthError> {
        // Both inputs belong to personal advancement, not active combat composition.
        // The caller supplies the canonical original-body reward assessment.
        // General-purpose personal additions establish an authored baseline;
        // ordinary growth records its paid tier before invoking this boundary.
        let mut candidate = self.clone();
        if current > candidate.highest_applied_tier {
            candidate.authored_baseline = candidate.authored_baseline.max(current);
            candidate.highest_applied_tier = current;
        }
        candidate.peak_threat = candidate.peak_threat.max(threat);
        candidate.highest_earned_tier = candidate
            .highest_earned_tier
            .max(candidate.projected_tier(current, policy)?);
        candidate.validate(current, policy)?;
        *self = candidate;
        Ok(())
    }
    pub(crate) fn spend(
        &mut self,
        next_ordinal: HdCount,
        policy: &GrowthPolicy,
    ) -> Result<(), GrowthError> {
        let price = self.next_threshold(policy)?;
        let held = self
            .held_xp
            .checked_sub(price)
            .ok_or(GrowthError::InvalidRecord)?;
        let spent = self
            .spent_xp
            .checked_add(price)
            .ok_or(GrowthError::Arithmetic)?;
        self.held_xp = held;
        self.spent_xp = spent;
        self.highest_applied_tier = self.highest_applied_tier.max(next_ordinal);
        self.highest_earned_tier = self.highest_earned_tier.max(next_ordinal);
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreatContribution {
    pub source: ThreatSource,
    pub points: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreatAssessment {
    pub sources: Vec<ThreatContribution>,
    pub hit_dice_points: u64,
    pub training_points: u64,
    pub total: u64,
    pub tier: u64,
}

/// Source-only assessment from the permanent fold. The caller must supply that
/// build's permanent derivation, never an equipment/condition-adjusted creature.
pub(crate) fn assess(
    build: &CreatureBuild,
    permanent: &DerivedCreature,
) -> Result<ThreatAssessment, BuildError> {
    let policy = &build.ruleset().document().growth.threat;
    let mut identities = std::collections::BTreeSet::new();
    identities.insert(ThreatSource::Species {
        id: build.species_id().clone(),
    });
    identities.insert(ThreatSource::Type {
        id: permanent.kind.clone(),
    });
    identities.extend(
        permanent
            .subtypes
            .iter()
            .cloned()
            .map(|id| ThreatSource::Subtype { id }),
    );
    identities.extend(build.templates().map(|template| ThreatSource::Template {
        id: template.id.clone(),
    }));
    for die in build.ledger().entries() {
        crate::diagnostics::raw_hit_die_visit();
        if let crate::progression::HdSource::Class(id) = die.source() {
            identities.insert(ThreatSource::Class { id });
        }
    }
    let mut trained = 2_u64;
    for choice in build.choices() {
        crate::diagnostics::advancement_choice_visit();
        for purchase in &choice.purchases {
            match &purchase.selection {
                PurchaseSelection::SkillRank(_) => {
                    trained = trained.checked_add(1).ok_or(BuildError::Arithmetic)?
                }
                // Effective expertise benefits are counted once below, even
                // when the same ID has several independent owned grant sources.
                PurchaseSelection::Expertise(_) => {}
                PurchaseSelection::Talent(id)
                | PurchaseSelection::TalentForm { talent: id, .. } => {
                    identities.insert(ThreatSource::Talent { id: id.clone() });
                }
            }
        }
    }
    identities.extend(
        permanent
            .expertises
            .active_ids()
            .cloned()
            .map(|id| ThreatSource::Expertise { id }),
    );
    let sources: Vec<_> = identities
        .into_iter()
        .map(|source| ThreatContribution {
            points: u64::from(policy.contribution(&source)),
            source,
        })
        .collect();
    let hit_dice_points = u64::from(policy.hit_die)
        .checked_mul(build.ledger().total_hd().get())
        .ok_or(BuildError::Arithmetic)?;
    let training_points = u64::from(policy.trained_rank)
        .checked_mul(trained)
        .ok_or(BuildError::Arithmetic)?;
    let total = sources.iter().try_fold(
        hit_dice_points
            .checked_add(training_points)
            .ok_or(BuildError::Arithmetic)?,
        |sum, source| sum.checked_add(source.points).ok_or(BuildError::Arithmetic),
    )?;
    let tier = total.div_ceil(u64::from(policy.points_per_tier)).max(1);
    Ok(ThreatAssessment {
        sources,
        hit_dice_points,
        training_points,
        total,
        tier,
    })
}

#[cfg(test)]
#[path = "earned_growth_tests.rs"]
mod tests;

#[cfg(test)]
mod source_tests {
    use super::*;
    use crate::creatures::{
        BuildAdvancement, CreatureState, HdChoices, PlannedAdvancement, RestorationPolicy,
    };
    use crate::progression::{HdLedger, HdSource};

    #[test]
    fn authored_regrowth_restores_choices_while_choose_again_appends_an_empty_slot() {
        let original = crate::test_creatures::build(crate::test_creatures::species());
        for authored in [false, true] {
            let mut choice = HdChoices::default();
            choice
                .purchases
                .push(crate::creatures::AdvancementPurchase {
                    id: None,
                    pool: crate::content_ids::PointPoolId::new("advancement").unwrap(),
                    selection: PurchaseSelection::SkillRank(crate::attributes::Skill::Athletics),
                });
            let intended = vec![
                PlannedAdvancement {
                    source: HdSource::Racial,
                    choices: HdChoices::default(),
                },
                PlannedAdvancement {
                    source: HdSource::Racial,
                    choices: choice.clone(),
                },
            ];
            let mut build = CreatureBuild::from_recorded(
                original.content().clone(),
                original.species_id().clone(),
                original.initial_attributes(),
                HdLedger::seeded(vec![HdSource::Racial; 2], 42).unwrap(),
                original.binding(),
                BuildAdvancement {
                    starting_skills: *original.starting_skills(),
                    choices: vec![HdChoices::default(), choice.clone()],
                    restoration: if authored {
                        RestorationPolicy::Authored(intended)
                    } else {
                        RestorationPolicy::ChooseAgain
                    },
                },
                vec![],
            )
            .unwrap();
            build
                .set_future_growth(FutureGrowth::Selected {
                    source: GrowthSource::Racial {},
                })
                .unwrap();
            let mut state = CreatureState::new(build).unwrap();
            let root = state.build().ledger().entries()[1].health_seed();
            let mut drained = state.build().clone();
            drained.remove_latest().unwrap();
            state.rebuild(drained).unwrap();
            state.award_xp(200).unwrap();
            assert_eq!(state.build().ledger().total_hd().get(), 2);
            assert_eq!(state.build().ledger().entries()[1].health_seed(), root);
            assert_eq!(
                state.build().choices()[1],
                if authored {
                    choice
                } else {
                    HdChoices::default()
                }
            );
        }
    }
    #[test]
    fn exhausted_authored_source_keeps_earned_value_until_an_explicit_future_source_is_selected() {
        let original = crate::test_creatures::build(crate::test_creatures::species());
        let mut build = CreatureBuild::from_recorded(
            original.content().clone(),
            original.species_id().clone(),
            original.initial_attributes(),
            HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
            original.binding(),
            BuildAdvancement {
                starting_skills: *original.starting_skills(),
                choices: vec![HdChoices::default()],
                restoration: RestorationPolicy::Authored(vec![PlannedAdvancement {
                    source: HdSource::Racial,
                    choices: HdChoices::default(),
                }]),
            },
            vec![],
        )
        .unwrap();
        build.set_future_growth(FutureGrowth::Authored {}).unwrap();
        let mut state = CreatureState::new(build).unwrap();
        state.award_xp(100).unwrap();
        assert_eq!(state.build().ledger().total_hd().get(), 1);
        assert_eq!(state.earned_progress().held_xp(), 100);
        let mut next = state.build().clone();
        next.set_future_growth(FutureGrowth::Selected {
            source: GrowthSource::Class {
                class: crate::content_ids::ClassId::new("mage").unwrap(),
            },
        })
        .unwrap();
        state.rebuild(next).unwrap();
        assert_eq!(state.build().ledger().total_hd().get(), 2);
        assert_eq!(
            state.build().ledger().entries()[1].source(),
            HdSource::Class(crate::content_ids::ClassId::new("mage").unwrap())
        );
        assert_eq!(state.earned_progress().held_xp(), 0);
    }
    #[test]
    fn composed_threat_accounts_for_innate_layers_class_template_and_owned_training() {
        use crate::content_ids::*;
        let mut rules = crate::test_creatures::ruleset().document().clone();
        let policy = &mut rules.growth.threat;
        policy.points_per_tier = 10;
        policy.hit_die = 23;
        policy.trained_rank = 29;
        policy.species = 5;
        policy.creature_type = 7;
        policy.subtype = 11;
        policy.class = 17;
        policy.template = 13;
        policy.talent = 19;
        rules.talents.push(crate::test_creatures::talent(
            "fixture_threat_source",
            vec![],
        ));
        let mut species = crate::test_creatures::species();
        species.subtypes.insert(SubtypeId::new("fire").unwrap());
        let id = species.id.clone();
        let template =
            crate::creatures::Template::new(TemplateId::new("threat_template").unwrap(), 0);
        let content = crate::creatures::Content::new(
            crate::rules::Ruleset::validate(rules).unwrap(),
            crate::creatures::ScenarioCatalog {
                species: [(id.clone(), species)].into(),
                templates: [(template.id.clone(), template.clone())].into(),
            },
        )
        .unwrap();
        let mut build = CreatureBuild::new(
            content,
            id,
            HdLedger::seeded(
                vec![HdSource::Class(ClassId::new("warrior").unwrap()); 2],
                42,
            )
            .unwrap(),
            crate::attributes::ManaBinding::Intellect,
            [
                crate::attributes::Skill::Deception,
                crate::attributes::Skill::Persuasion,
            ],
        )
        .unwrap();
        build.set_templates(vec![template.id]).unwrap();
        build
            .train(crate::HdIndex(0), crate::attributes::Skill::Athletics)
            .unwrap();
        build
            .select_talent(
                crate::HdIndex(1),
                TalentId::new("fixture_threat_source").unwrap(),
            )
            .unwrap();
        let assessment = assess(&build, &build.derive().unwrap()).unwrap();
        assert_eq!(assessment.hit_dice_points, 46);
        assert_eq!(assessment.training_points, 87);
        assert_eq!(assessment.sources.len(), 6);
        assert_eq!(assessment.total, 205);
        assert_eq!(assessment.tier, 21);
        let mut state = CreatureState::new(build).unwrap();
        let peak = state.earned_progress().peak_threat();
        state
            .set_equipment(vec![crate::creatures::EquipmentSource {
                weapon_profile: None,
                item: crate::ItemId(1),
                socket: crate::EquipmentSlotId(0),
                attack: None,
                modifiers: vec![crate::rules::Modifier {
                    cancellable: false,
                    scope: crate::rules::Scope::Always,
                    op: crate::rules::ModifierOp::Add {
                        stat: crate::rules::Stat::Health,
                        value: 100,
                    },
                }],
            }])
            .unwrap();
        state
            .apply_condition(
                &ConditionId::new("readied").unwrap(),
                crate::ActorId(2),
                100,
            )
            .unwrap();
        assert_eq!(
            state.earned_progress().peak_threat(),
            peak,
            "temporary sources cannot inflate earned threat"
        );
    }
}
