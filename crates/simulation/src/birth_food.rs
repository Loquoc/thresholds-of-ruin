//! Transient generation history reconstructed from immutable first-entry receipts.
//! This does not change the independent ground-food safety stage, or consult
//! current ownership: eating or dropping an item cannot rewrite a birth roll.
use super::*;

pub(super) struct BirthFoodClock {
    food: Vec<String>,
    limit: u32,
    count: u32,
    incremented: bool,
}
impl BirthFoodClock {
    pub(super) fn before_stage(&mut self, stage: &PopulationStage) {
        if !self.incremented && !matches!(stage.population, PopulationContent::Actors { .. }) {
            self.count = self.count.saturating_add(1).min(self.limit + 1);
            self.incremented = true;
        }
    }
    pub(super) fn forced_food(&self) -> Option<&[String]> {
        (self.count > self.limit).then_some(self.food.as_slice())
    }
    fn is_food(&self, attempt: &PlacementDecision) -> bool {
        let content = match attempt {
            PlacementDecision::Placed {
                content,
                entity: PopulationEntity::Item { .. },
                ..
            } => content,
            PlacementDecision::Placed {
                entity:
                    PopulationEntity::Actor {
                        carried_loot: Some(CarriedLootDecision::Carried { content, .. }),
                        ..
                    },
                ..
            } => content,
            _ => return false,
        };
        self.food.binary_search(content).is_ok()
    }
    pub(super) fn observe(&mut self, attempt: &PlacementDecision) {
        if self.is_food(attempt) {
            self.count = 0;
        }
    }
    pub(super) fn observe_result(&mut self, result: &StageResult) {
        for attempt in result.attempts() {
            self.observe(attempt);
        }
    }
}
impl Game {
    pub(super) fn birth_food_clock(
        &self,
        profile: &PopulationProfile,
        ordinal: u64,
    ) -> Result<Option<BirthFoodClock>, GameError> {
        let mut policies = profile
            .stages
            .iter()
            .filter_map(|stage| match &stage.population {
                PopulationContent::FoodDrought {
                    food, threshold, ..
                } => Some((food, threshold)),
                _ => None,
            });
        let Some((food, threshold)) = policies.next() else {
            return Ok(None);
        };
        // Multiple independent safety policies have no unambiguous common
        // carrier clock. They retain ordinary weighted stock selection.
        if policies.any(|policy| policy != (food, threshold)) {
            return Ok(None);
        }
        let mut clock = BirthFoodClock {
            food: food.clone(),
            limit: u32::from(*threshold) + 1,
            count: 0,
            incremented: false,
        };
        let state = self.floor_activation.as_ref().ok_or_else(invalid)?;
        let previous = state
            .order
            .get(..usize::try_from(ordinal).map_err(|_| invalid())?)
            .ok_or_else(invalid)?;
        // Traverse backwards only to the latest generation or saturated drought.
        // The floor increment follows initial actor stages, not current ownership.
        for scope in previous.iter().rev() {
            let record = state.floors.get(scope).ok_or_else(invalid)?;
            let prior = state
                .plan
                .profiles
                .get(&record.profile)
                .ok_or_else(invalid)?;
            let increment = prior
                .stages
                .iter()
                .position(|stage| !matches!(stage.population, PopulationContent::Actors { .. }))
                .unwrap_or(prior.stages.len());
            if increment == prior.stages.len() {
                clock.count += 1;
            }
            for (position, decision) in record.decisions.iter().enumerate().rev() {
                if decision
                    .result
                    .attempts()
                    .any(|attempt| clock.is_food(attempt))
                {
                    return Ok(Some(clock));
                }
                if position == increment {
                    clock.count += 1;
                }
                if clock.count > clock.limit {
                    return Ok(Some(clock));
                }
            }
            if clock.count > clock.limit {
                break;
            }
        }
        Ok(Some(clock))
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn clock(count: u32) -> BirthFoodClock {
        BirthFoodClock {
            food: vec!["ration".into()],
            limit: 3,
            count,
            incremented: false,
        }
    }
    fn stage(actors: bool) -> PopulationStage {
        PopulationStage {
            id: "test-stage".into(),
            version: 1,
            count: [1, 1],
            population: if actors {
                PopulationContent::Actors {
                    bands: vec![],
                    avoid_protagonist_sight: false,
                }
            } else {
                PopulationContent::Items { bands: vec![] }
            },
        }
    }
    fn food(carried: bool) -> PlacementDecision {
        PlacementDecision::Placed {
            content: if carried { "carrier" } else { "ration" }.into(),
            location: Location {
                region: RegionId(1),
                position: tor_world::Position { x: 1, y: 1, z: 0 },
            },
            entity: if carried {
                PopulationEntity::Actor {
                    id: ActorId(2),
                    carried_loot: Some(CarriedLootDecision::Carried {
                        content: "ration".into(),
                        quantity: 1,
                        item: ItemId(999),
                    }),
                }
            } else {
                PopulationEntity::Item {
                    id: ItemId(999),
                    quantity: 1,
                }
            },
        }
    }

    #[test]
    fn initial_actor_food_precedes_increment_but_later_food_follows_it() {
        let mut initial = clock(3);
        initial.before_stage(&stage(true));
        assert!(initial.forced_food().is_none());
        initial.observe(&food(true));
        initial.before_stage(&stage(false));
        assert_eq!(initial.count, 1);
        let mut later = clock(3);
        later.before_stage(&stage(false));
        assert!(later.forced_food().is_some());
        later.observe(&food(false));
        assert_eq!(later.count, 0);
        later.before_stage(&stage(true));
        later.before_stage(&stage(false));
        assert_eq!(
            later.count, 0,
            "one increment per first entry, never per stage"
        );
    }

    #[test]
    fn skipped_food_and_nonfood_do_not_reset_the_clock() {
        let mut clock = clock(4);
        clock.observe(&PlacementDecision::Skipped {
            content: "ration".into(),
            reason: SkipReason::NoEligibleCell,
        });
        assert!(clock.forced_food().is_some());
        let mut nonfood = food(false);
        if let PlacementDecision::Placed { content, .. } = &mut nonfood {
            *content = "marker".into();
        }
        clock.observe(&nonfood);
        assert!(clock.forced_food().is_some());
    }

    #[test]
    fn retained_ground_and_carried_receipts_reset_without_resolving_live_item_ids() {
        // No live item store or ownership is passed to this operation. ID999 may
        // have been consumed, merged, dropped or retired since the birth event.
        for carried in [false, true] {
            let mut clock = clock(4);
            clock.observe(&food(carried));
            assert_eq!(clock.count, 0);
            assert!(clock.forced_food().is_none());
        }
    }

    #[test]
    fn absent_or_conflicting_food_policies_do_not_invent_a_shared_clock() {
        let content =
            crate::creatures::Content::new(crate::test_support::test_ruleset(), Default::default())
                .unwrap();
        let game = Game::region_corridor(42, 2, content);
        let profile = PopulationProfile {
            version: 1,
            stages: vec![stage(true)],
        };
        assert!(game.birth_food_clock(&profile, 0).unwrap().is_none());
        for (other_food, other_threshold) in [("ration", 3), ("fruit", 2)] {
            let mut profile = profile.clone();
            for (id, food, threshold) in [
                ("first", "ration", 2),
                ("second", other_food, other_threshold),
            ] {
                profile.stages.push(PopulationStage {
                    id: id.into(),
                    version: 1,
                    count: [1, 1],
                    population: PopulationContent::FoodDrought {
                        bands: vec![],
                        food: vec![food.into()],
                        threshold,
                    },
                });
            }
            assert!(game.birth_food_clock(&profile, 0).unwrap().is_none());
        }
    }
    // Insert inside birth_food::tests. These are reconstruction-helper unit tests,
    // not claims of valid full activation records or end-to-end replay qualification.
    fn policy_stage(id: &str, content: &str, threshold: u16) -> PopulationStage {
        PopulationStage {
            id: id.into(),
            version: 1,
            count: [1, 1],
            population: PopulationContent::FoodDrought {
                bands: vec![],
                food: vec![content.into()],
                threshold,
            },
        }
    }

    fn history_game(prior: PopulationProfile, decisions: Vec<StageDecision>) -> Game {
        let content =
            crate::creatures::Content::new(crate::test_support::test_ruleset(), Default::default())
                .unwrap();
        let mut game = Game::region_corridor(42, 2, content);
        let record = FloorActivationRecord {
            scope: "earlier".into(),
            entrant: ActorId(1),
            tick: 0,
            ordinal: 0,
            cause: EntryCause::Initial,
            profile: "prior".into(),
            profile_version: 1,
            decisions,
        };
        game.floor_activation = Some(ActivationState {
            wandering: None,
            plan: Shared::new(PopulationPlan {
                protagonist: ActorId(1),
                carried_item_pools: BTreeMap::new(),
                profiles: BTreeMap::from([("prior".into(), prior)]),
            }),
            scopes: Shared::default(),
            members: Shared::default(),
            floors: Shared::new(BTreeMap::from([("earlier".into(), Shared::new(record))])),
            order: Shared::new(vec!["earlier".into()]),
            decision_count: 2,
        });
        game
    }

    #[test]
    fn reverse_history_uses_current_food_ids_and_prior_increment_boundary() {
        // The prior profile calls only fruit food and uses a different threshold.
        // Neither classification nor threshold is inherited by the current policy.
        let current = PopulationProfile {
            version: 1,
            stages: vec![policy_stage("current", "ration", 2)],
        };
        for before_increment in [false, true] {
            for receipt_content in ["ration", "fruit"] {
                let prior = PopulationProfile {
                    version: 1,
                    stages: vec![stage(true), policy_stage("prior-food", "fruit", 9)],
                };
                let mut receipt = food(before_increment);
                if let PlacementDecision::Placed {
                    content, entity, ..
                } = &mut receipt
                {
                    if let PopulationEntity::Actor {
                        carried_loot: Some(CarriedLootDecision::Carried { content, .. }),
                        ..
                    } = entity
                    {
                        *content = receipt_content.into();
                    } else {
                        *content = receipt_content.into();
                    }
                }
                let receipt_position = if before_increment { 0 } else { 1 };
                let decisions = prior
                    .stages
                    .iter()
                    .enumerate()
                    .map(|(position, stage)| StageDecision {
                        id: stage.id.clone(),
                        version: 1,
                        result: StageResult::Evaluated {
                            requested: 1,
                            attempts: if position == receipt_position {
                                vec![receipt.clone()]
                            } else {
                                vec![]
                            },
                        },
                    })
                    .collect();
                let game = history_game(prior, decisions);
                let clock = game.birth_food_clock(&current, 1).unwrap().unwrap();
                let expected = if receipt_content == "ration" && !before_increment {
                    0
                } else {
                    1
                };
                assert_eq!(
                    clock.count, expected,
                    "receipt={receipt_content}, initial={before_increment}"
                );
                assert_eq!(clock.limit, 3);
                assert_eq!(clock.food, vec!["ration"]);
                assert!(
                    game.items.get(&ItemId(999)).is_none(),
                    "receipt lookup must not require a live generated object"
                );
            }
        }
    }

    #[test]
    fn identical_food_policies_share_clock_but_conflicting_policies_do_not() {
        let game = history_game(
            PopulationProfile {
                version: 1,
                stages: vec![],
            },
            vec![],
        );
        let identical = PopulationProfile {
            version: 1,
            stages: vec![
                policy_stage("a", "ration", 2),
                policy_stage("b", "ration", 2),
            ],
        };
        assert_eq!(
            game.birth_food_clock(&identical, 0).unwrap().unwrap().count,
            0
        );
        for (content, threshold) in [("fruit", 2), ("ration", 3)] {
            let mut conflicting = identical.clone();
            conflicting.stages[1] = policy_stage("b", content, threshold);
            assert!(game.birth_food_clock(&conflicting, 0).unwrap().is_none());
        }
    }
}
