//! Capacity-preserving resource balances and deterministic active-time recovery.
//!
//! [`Resources`] owns three independent integer balances and their fractional
//! recovery phases. Capacities are supplied by the caller; this module does not
//! derive actor builds, reserve action costs, enforce death/freeze policy or read
//! a clock. Recovery periods come from a validated [`Ruleset`]. Callers advance
//! only actor-active simulation ticks, not wall time or frozen-region time.
//!
//! Each pool maintains `balance <= maximum`. A full pool has zero accumulated
//! recovery time: unused recovery never becomes credit for a later spend.
//! Raising a maximum adds capacity, not balance; lowering it clamps the balance.
//!
//! # Spending, partial recovery and saturation
//!
//! ```
//! use tor_simulation::resources::{ByResource, Resource, Resources};
//! use tor_simulation::rules::{Ruleset, RulesetDocument, DEFAULT_RULESET_TOML};
//!
//! let mut document: RulesetDocument = toml::from_str(DEFAULT_RULESET_TOML).unwrap();
//! for definition in &mut document.resources {
//!     definition.recovery_period = 10;
//! }
//! let rules = Ruleset::validate(document).unwrap();
//! let mut pools = Resources::new(ByResource::from_fn(|_| 5), &rules);
//! assert_eq!(pools.next_recovery_in(), None);
//! assert!(pools.spend(Resource::Stamina, 3));
//! pools.advance_active(4);
//! assert_eq!(pools.balance(Resource::Stamina), 2);
//! assert_eq!(pools.recovery_elapsed(Resource::Stamina), 4);
//! assert_eq!(pools.next_recovery_in(), Some(6));
//!
//! let before = pools.clone();
//! assert!(!pools.spend(Resource::Stamina, 3));
//! assert_eq!(pools, before); // Insufficient funds change neither balance nor phase.
//! pools.advance_active(6);
//! assert_eq!(pools.balance(Resource::Stamina), 3);
//! assert_eq!(pools.restore(Resource::Stamina, u32::MAX), 2);
//! assert_eq!(pools.recovery_elapsed(Resource::Stamina), 0);
//! pools.advance_active(u64::MAX); // Full pools do not bank excess ticks.
//! assert!(pools.spend(Resource::Stamina, 1));
//! assert_eq!(pools.next_recovery_in(), Some(10));
//! ```

use crate::rules::Ruleset;

/// Rejection of a recorded balance or fractional recovery phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceError {
    /// A balance exceeds capacity, a phase reaches its period, or a full pool
    /// carries nonzero phase. See [`Resources::from_recorded`].
    InvalidState,
}

/// One of the three independently recovered actor resource pools.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    /// Physical exertion pool.
    Stamina,
    /// Concentration pool.
    Focus,
    /// Magical energy pool.
    Mana,
}

impl Resource {
    /// Every resource in deterministic stamina, focus, mana order.
    pub const ALL: [Self; 3] = [Self::Stamina, Self::Focus, Self::Mana];
}

/// Exhaustive named values for all three resources, without numeric validation.
///
/// Serialization uses named fields; deserialization requires the complete named
/// map and rejects unknown fields. Mapping operations use [`Resource::ALL`] order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ByResource<T> {
    /// Value for [`Resource::Stamina`].
    pub stamina: T,
    /// Value for [`Resource::Focus`].
    pub focus: T,
    /// Value for [`Resource::Mana`].
    pub mana: T,
}
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for ByResource<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields<T> {
            stamina: T,
            focus: T,
            mana: T,
        }
        let fields: Fields<T> = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            stamina: fields.stamina,
            focus: fields.focus,
            mana: fields.mana,
        })
    }
}
impl<T> ByResource<T> {
    /// Consume the map and transform each value once, in [`Resource::ALL`] order.
    ///
    /// ```
    /// use tor_simulation::resources::{ByResource, Resource};
    /// let mut visited = Vec::new();
    /// let values = ByResource::from_fn(|resource| {
    ///     visited.push(resource);
    ///     visited.len() as u32
    /// });
    /// assert_eq!(visited, Resource::ALL);
    /// let mut doubled = values.map(|value| value * 2);
    /// assert_eq!(doubled.stamina, 2);
    /// assert_eq!(doubled.focus, 4);
    /// *doubled.get_mut(Resource::Mana) += 1;
    /// assert_eq!(*doubled.get(Resource::Mana), 7);
    /// ```
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> ByResource<U> {
        ByResource {
            stamina: f(self.stamina),
            focus: f(self.focus),
            mana: f(self.mana),
        }
    }

    /// Construct each value once, calling `f` in [`Resource::ALL`] order.
    pub fn from_fn(mut f: impl FnMut(Resource) -> T) -> Self {
        Self {
            stamina: f(Resource::Stamina),
            focus: f(Resource::Focus),
            mana: f(Resource::Mana),
        }
    }
    /// Borrow the value for exactly the selected pool.
    pub fn get(&self, resource: Resource) -> &T {
        match resource {
            Resource::Stamina => &self.stamina,
            Resource::Focus => &self.focus,
            Resource::Mana => &self.mana,
        }
    }
    /// Mutably borrow the value for exactly the selected pool.
    pub fn get_mut(&mut self, resource: Resource) -> &mut T {
        match resource {
            Resource::Stamina => &mut self.stamina,
            Resource::Focus => &mut self.focus,
            Resource::Mana => &mut self.mana,
        }
    }
}
/// Caller-derived integer capacities; zero disables capacity for that pool.
pub type ResourceMaxima = ByResource<u32>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pool {
    maximum: u32,
    balance: u32,
    remainder: u64,
}

impl Pool {
    fn new(maximum: u32) -> Self {
        Self {
            maximum,
            balance: maximum,
            remainder: 0,
        }
    }

    fn set_maximum(&mut self, maximum: u32) {
        self.maximum = maximum;
        self.balance = self.balance.min(maximum);
        if self.balance == maximum {
            self.remainder = 0;
        }
    }

    fn recover(&mut self, ticks: u64, period: u64) {
        let capacity = self.maximum - self.balance;
        if capacity == 0 {
            self.remainder = 0;
            return;
        }
        let elapsed = u128::from(self.remainder) + u128::from(ticks);
        let gained = (elapsed / u128::from(period)).min(u128::from(capacity)) as u32;
        self.balance += gained;
        self.remainder = if self.balance == self.maximum {
            0
        } else {
            (elapsed % u128::from(period)) as u64
        };
    }
}

/// Mutable balances, capacities and sub-period active recovery for one actor.
///
/// Cloning produces independent value state, including recovery phases. This
/// primitive does not own action reservations or actor lifecycle eligibility.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resources {
    pools: ByResource<Pool>,
    periods: ByResource<u64>,
}

impl Resources {
    /// Reconstruct balances and fractional active recovery against fresh capacities.
    ///
    /// `elapsed` is the sub-period phase for each pool, not absolute simulation
    /// time. Recovery periods come from `rules`; no recovery is applied here.
    ///
    /// # Errors
    ///
    /// Returns [`ResourceError::InvalidState`] if any balance exceeds its maximum,
    /// any elapsed phase is at least its period, or a full pool has nonzero phase.
    /// Invalid state rejects as a whole rather than clamping or returning a
    /// partially restored value. A zero-capacity pool must have zero balance and
    /// zero phase.
    ///
    /// ```
    /// use tor_simulation::resources::{ByResource, Resource, ResourceError, Resources};
    /// use tor_simulation::rules::{Ruleset, RulesetDocument, DEFAULT_RULESET_TOML};
    /// let mut document: RulesetDocument = toml::from_str(DEFAULT_RULESET_TOML).unwrap();
    /// for definition in &mut document.resources { definition.recovery_period = 10; }
    /// let rules = Ruleset::validate(document).unwrap();
    /// let maxima = ByResource::from_fn(|_| 5);
    /// let balances = ByResource { stamina: 2, focus: 5, mana: 5 };
    /// let phases = ByResource { stamina: 9, focus: 0, mana: 0 };
    /// let mut restored = Resources::from_recorded(maxima, balances, phases, &rules).unwrap();
    /// assert_eq!(restored.next_recovery_in(), Some(1));
    /// restored.advance_active(1);
    /// assert_eq!(restored.balance(Resource::Stamina), 3);
    /// for invalid in [
    ///     ByResource { stamina: 10, ..phases }, // Phase must be below the period.
    ///     ByResource { focus: 1, ..phases }, // Full pools cannot retain phase.
    /// ] {
    ///     assert_eq!(Resources::from_recorded(maxima, balances, invalid, &rules),
    ///         Err(ResourceError::InvalidState));
    /// }
    /// assert_eq!(Resources::from_recorded(maxima,
    ///     ByResource { stamina: 6, ..balances }, phases, &rules),
    ///     Err(ResourceError::InvalidState));
    /// ```
    pub fn from_recorded(
        maxima: ResourceMaxima,
        balances: ByResource<u32>,
        elapsed: ByResource<u64>,
        rules: &Ruleset,
    ) -> Result<Self, ResourceError> {
        let mut resources = Self::new(maxima, rules);
        for resource in Resource::ALL {
            let pool = resources.pools.get_mut(resource);
            if *balances.get(resource) > pool.maximum
                || *elapsed.get(resource) >= *resources.periods.get(resource)
                || (*balances.get(resource) == pool.maximum && *elapsed.get(resource) != 0)
            {
                return Err(ResourceError::InvalidState);
            }
            pool.balance = *balances.get(resource);
            pool.remainder = *elapsed.get(resource);
        }
        Ok(resources)
    }

    /// Start every pool full, with zero phase and the ruleset's recovery periods.
    ///
    /// Capacities are accepted as supplied, including zero. The validated ruleset
    /// guarantees exactly one positive recovery period for each resource.
    pub fn new(maxima: ResourceMaxima, rules: &Ruleset) -> Self {
        let mut periods = ByResource {
            stamina: 1,
            focus: 1,
            mana: 1,
        };
        for definition in &rules.document().resources {
            *periods.get_mut(definition.resource) = definition.recovery_period;
        }
        Self {
            pools: ByResource::from_fn(|resource| Pool::new(*maxima.get(resource))),
            periods,
        }
    }
    /// Replace recovery periods without changing balances or capacities.
    ///
    /// Each existing phase is reduced modulo its new period. This does not grant
    /// retroactive recovery when a shorter period discards complete intervals.
    ///
    /// ```
    /// use tor_simulation::resources::{ByResource, Resource, Resources};
    /// use tor_simulation::rules::{Ruleset, RulesetDocument, DEFAULT_RULESET_TOML};
    /// let mut document: RulesetDocument = toml::from_str(DEFAULT_RULESET_TOML).unwrap();
    /// for definition in &mut document.resources { definition.recovery_period = 10; }
    /// let rules = Ruleset::validate(document.clone()).unwrap();
    /// let mut pools = Resources::new(ByResource::from_fn(|_| 5), &rules);
    /// assert!(pools.spend(Resource::Focus, 2));
    /// pools.advance_active(8);
    /// for definition in &mut document.resources { definition.recovery_period = 3; }
    /// let faster = Ruleset::validate(document).unwrap();
    /// pools.set_recovery_rules(&faster);
    /// assert_eq!(pools.balance(Resource::Focus), 3); // No retroactive grant.
    /// assert_eq!(pools.recovery_elapsed(Resource::Focus), 2); // 8 modulo 3.
    /// assert_eq!(pools.next_recovery_in(), Some(1));
    /// pools.advance_active(1);
    /// assert_eq!(pools.balance(Resource::Focus), 4);
    /// ```
    pub fn set_recovery_rules(&mut self, rules: &Ruleset) {
        for definition in &rules.document().resources {
            *self.periods.get_mut(definition.resource) = definition.recovery_period;
            self.pools.get_mut(definition.resource).remainder %= definition.recovery_period;
        }
    }
    /// Add at most the missing capacity and return the amount actually restored.
    ///
    /// Excess `amount` is discarded without overflow. Partial restoration keeps
    /// the existing phase; reaching capacity clears it. Restoring zero is valid.
    pub fn restore(&mut self, resource: Resource, amount: u32) -> u32 {
        let pool = self.pools.get_mut(resource);
        let restored = amount.min(pool.maximum - pool.balance);
        pool.balance += restored;
        if pool.balance == pool.maximum {
            pool.remainder = 0;
        }
        restored
    }

    /// Current spendable integer amount, never greater than [`Self::maximum`].
    pub fn balance(&self, resource: Resource) -> u32 {
        self.pools.get(resource).balance
    }

    /// Current caller-supplied capacity for this pool.
    pub fn maximum(&self, resource: Resource) -> u32 {
        self.pools.get(resource).maximum
    }

    /// Active ticks accumulated toward the next unit, strictly below its period.
    /// Full pools always return zero.
    pub fn recovery_elapsed(&self, resource: Resource) -> u64 {
        self.pools.get(resource).remainder
    }

    /// Active ticks until the next balance change. Full pools have no event;
    /// partially recovered pools retain their phase when scheduling resumes.
    /// Returns the minimum strictly positive delay across non-full pools, or
    /// `None` if every pool is full (including zero-capacity pools). This query
    /// does not advance time or decide whether the actor is eligible to recover.
    pub fn next_recovery_in(&self) -> Option<u64> {
        Resource::ALL
            .into_iter()
            .filter_map(|resource| {
                let pool = *self.pools.get(resource);
                (pool.balance < pool.maximum).then(|| *self.periods.get(resource) - pool.remainder)
            })
            .min()
    }

    /// Replace capacities, clamping balances downward but never filling new room.
    ///
    /// Phase survives if a pool remains below its new capacity; a full pool's
    /// phase clears. Applying the same maxima leaves valid state unchanged.
    ///
    /// ```
    /// use tor_simulation::resources::{ByResource, Resource, Resources};
    /// use tor_simulation::rules::{Ruleset, RulesetDocument, DEFAULT_RULESET_TOML};
    /// let mut document: RulesetDocument = toml::from_str(DEFAULT_RULESET_TOML).unwrap();
    /// for definition in &mut document.resources { definition.recovery_period = 10; }
    /// let rules = Ruleset::validate(document).unwrap();
    /// let mut pools = Resources::new(ByResource::from_fn(|_| 5), &rules);
    /// assert!(pools.spend(Resource::Mana, 3));
    /// pools.advance_active(4);
    /// pools.set_maxima(ByResource { stamina: 5, focus: 0, mana: 8 });
    /// assert_eq!(pools.balance(Resource::Mana), 2); // Increase is not healing.
    /// assert_eq!(pools.maximum(Resource::Mana), 8);
    /// assert_eq!(pools.recovery_elapsed(Resource::Mana), 4);
    /// assert_eq!(pools.balance(Resource::Focus), 0);
    /// pools.set_maxima(ByResource { stamina: 5, focus: 0, mana: 1 });
    /// assert_eq!(pools.balance(Resource::Mana), 1);
    /// assert_eq!(pools.recovery_elapsed(Resource::Mana), 0);
    /// assert_eq!(pools.next_recovery_in(), None);
    /// ```
    pub fn set_maxima(&mut self, maxima: ResourceMaxima) {
        for resource in Resource::ALL {
            self.pools
                .get_mut(resource)
                .set_maximum(*maxima.get(resource));
        }
    }

    /// Direct spending primitive. Action execution additionally owns reservations
    /// and paid-cost history; queue admission must not invoke this method.
    /// Returns `false` on insufficient balance without changing any pool or phase.
    /// Otherwise subtracts exactly `amount`, preserves phase and returns `true`.
    /// Spending zero succeeds, including on a zero-capacity pool.
    pub fn spend(&mut self, resource: Resource, amount: u32) -> bool {
        let pool = self.pools.get_mut(resource);
        if amount > pool.balance {
            return false;
        }
        pool.balance -= amount;
        true
    }

    /// Pass only elapsed active simulation ticks. Frozen regions do not call
    /// this method and receive no catch-up recovery on reactivation.
    /// Each complete recovery period adds one unit up to capacity; a non-full pool
    /// retains the remainder. Reaching capacity discards all excess time. Zero
    /// ticks leave valid state unchanged; even `u64::MAX` ticks are accumulated
    /// without overflow. No RNG or action history is consumed.
    pub fn advance_active(&mut self, ticks: u64) {
        for resource in Resource::ALL {
            self.pools
                .get_mut(resource)
                .recover(ticks, *self.periods.get(resource));
        }
    }

    /// Pure, bounded projection of actual periodic gains, excluding capped excess.
    pub(crate) fn regeneration_after(&self, ticks: u64) -> ByResource<u32> {
        ByResource::from_fn(|resource| {
            let pool = self.pools.get(resource);
            ((u128::from(pool.remainder) + u128::from(ticks))
                / u128::from(*self.periods.get(resource)))
            .min(u128::from(pool.maximum - pool.balance)) as u32
        })
    }
}
