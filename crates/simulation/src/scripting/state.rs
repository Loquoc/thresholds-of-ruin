//! Persisted script values and the derived binding index.

use super::{
    ActorClass, BindingId, FactKeys, FilterKey, HandlerId, HookFact, HookKind, Owner, ScopeKind,
    TimerId,
    Value, ValueType, VarScope, MAX_BINDINGS, MAX_NAME_BYTES, MAX_PENDING_FACTS, MAX_TEXT_BYTES,
    MAX_TIMERS, MAX_VARIABLES,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use crate::{ActorId, ItemId, ItemIdentity};
use tor_world::{Location, RegionId, Shared};

/// A subscription: plain data keyed by a stable identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub kind: HookKind,
    pub key: FilterKey,
    #[serde(default)]
    pub actors: ActorClass,
    pub handler: HandlerId,
    #[serde(default)]
    pub owner: Option<Owner>,
    /// Retire after the first root invocation that hears it succeeds.
    #[serde(default)]
    pub once: bool,
}

impl Binding {
    pub fn valid(&self, handlers: u32) -> bool {
        self.handler.0 < handlers && self.kind.accepts(&self.key)
    }
}

/// A scheduled handler call. Running timers hold an absolute due tick; timers
/// whose owner is in a frozen region hold the remaining time instead and
/// resume without catch-up when it thaws.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timer {
    pub handler: HandlerId,
    pub owner: Option<Owner>,
    pub arg: i64,
    pub clock: TimerClock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TimerClock {
    Due(u64),
    Frozen { remaining: u64 },
}

/// The package's declared variables. Undeclared names and wrong types are
/// invariant violations.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariableSchema {
    pub variables: BTreeMap<String, (ScopeKind, ValueType)>,
}

impl VariableSchema {
    pub fn check(&self, scope: VarScope, name: &str, value: Option<&Value>) -> bool {
        let Some(&(kind, ty)) = self.variables.get(name) else {
            return false;
        };
        kind == scope.kind()
            && value.is_none_or(|value| {
                value.value_type() == ty
                    && match value {
                        Value::Text(text) => text.len() <= MAX_TEXT_BYTES,
                        _ => true,
                    }
            })
    }
}

/// Host-validated package data that starts a scripted game.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptSetup {
    /// Number of bindable handlers. Every [`HandlerId`] must be below it.
    pub handlers: u32,
    pub schema: VariableSchema,
    /// Initial bindings from the package manifest, in manifest order.
    pub bindings: Vec<Binding>,
    /// Initial scenario-scoped variable values.
    pub defaults: Vec<(String, Value)>,
}

/// Live bindings of one kind, split by key class. Each class is its own map,
/// so a fact looks up only the classes this kind actually uses and borrows
/// its keys instead of building them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct KindIndex {
    len: usize,
    any: BTreeSet<BindingId>,
    actor: BTreeMap<ActorId, BTreeSet<BindingId>>,
    item: BTreeMap<ItemId, BTreeSet<BindingId>>,
    identity: BTreeMap<ItemIdentity, BTreeSet<BindingId>>,
    cell: BTreeMap<Location, BTreeSet<BindingId>>,
    region: BTreeMap<RegionId, BTreeSet<BindingId>>,
}

fn insert_keyed<K: Ord>(map: &mut BTreeMap<K, BTreeSet<BindingId>>, key: K, id: BindingId) {
    map.entry(key).or_default().insert(id);
}

fn remove_keyed<K: Ord>(map: &mut BTreeMap<K, BTreeSet<BindingId>>, key: &K, id: BindingId) {
    if let Some(set) = map.get_mut(key) {
        set.remove(&id);
        if set.is_empty() {
            map.remove(key);
        }
    }
}

fn visit<K: Ord>(
    map: &BTreeMap<K, BTreeSet<BindingId>>,
    key: Option<&K>,
    found: &mut impl FnMut(BindingId),
) {
    if map.is_empty() {
        return;
    }
    if let Some(set) = key.and_then(|key| map.get(key)) {
        set.iter().copied().for_each(&mut *found);
    }
}

impl KindIndex {
    fn insert(&mut self, id: BindingId, key: &FilterKey) {
        match key {
            FilterKey::Any => {
                self.any.insert(id);
            }
            FilterKey::Actor(actor) => insert_keyed(&mut self.actor, *actor, id),
            FilterKey::Item(item) => insert_keyed(&mut self.item, *item, id),
            FilterKey::ItemIdentity(identity) => {
                insert_keyed(&mut self.identity, identity.clone(), id)
            }
            FilterKey::Cell(cell) => insert_keyed(&mut self.cell, *cell, id),
            FilterKey::Region(region) => insert_keyed(&mut self.region, *region, id),
        }
        self.len += 1;
    }
    fn remove(&mut self, id: BindingId, key: &FilterKey) {
        match key {
            FilterKey::Any => {
                self.any.remove(&id);
            }
            FilterKey::Actor(actor) => remove_keyed(&mut self.actor, actor, id),
            FilterKey::Item(item) => remove_keyed(&mut self.item, item, id),
            FilterKey::ItemIdentity(identity) => remove_keyed(&mut self.identity, identity, id),
            FilterKey::Cell(cell) => remove_keyed(&mut self.cell, cell, id),
            FilterKey::Region(region) => remove_keyed(&mut self.region, region, id),
        }
        self.len -= 1;
    }
    fn matching(&self, keys: &FactKeys<'_>, found: &mut impl FnMut(BindingId)) {
        self.any.iter().copied().for_each(&mut *found);
        visit(&self.actor, keys.actor.as_ref(), found);
        visit(&self.item, keys.item.as_ref(), found);
        visit(&self.identity, keys.identity, found);
        visit(&self.cell, keys.cell.as_ref(), found);
        visit(&self.region, keys.region.as_ref(), found);
    }
}

/// Derived per-kind lookup over live bindings. Rebuilt from the bindings on
/// restore; never serialized.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BindingIndex {
    kinds: [KindIndex; HookKind::ALL.len()],
}

impl BindingIndex {
    fn insert(&mut self, id: BindingId, binding: &Binding) {
        self.kinds[binding.kind as usize].insert(id, &binding.key);
    }
    fn remove(&mut self, id: BindingId, binding: &Binding) {
        self.kinds[binding.kind as usize].remove(id, &binding.key);
    }
    pub(crate) fn mask(&self) -> u64 {
        HookKind::ALL
            .iter()
            .filter(|kind| self.kinds[**kind as usize].len > 0)
            .fold(0, |mask, kind| mask | kind.bit())
    }
    /// Every binding of `kind` whose key `keys` offers, class by class.
    pub(crate) fn matching(
        &self,
        kind: HookKind,
        keys: &FactKeys<'_>,
        mut found: impl FnMut(BindingId),
    ) {
        self.kinds[kind as usize].matching(keys, &mut found);
    }
}

/// One matched fact waiting for its boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PendingFact {
    pub handler: HandlerId,
    /// The binding that matched, or `None` for a timer.
    pub binding: Option<BindingId>,
    pub fact: HookFact,
}

/// All persisted script state. The runtime holds none of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptState {
    pub(crate) handlers: u32,
    pub(crate) schema: Shared<VariableSchema>,
    pub(crate) bindings: Shared<BTreeMap<BindingId, Binding>>,
    pub(crate) timers: Shared<BTreeMap<TimerId, Timer>>,
    pub(crate) vars: Shared<BTreeMap<(VarScope, String), Value>>,
    pub(crate) rng: u64,
    pub(crate) next_binding: u64,
    pub(crate) next_timer: u64,
    /// Facts matched since the last boundary. Persisted so a region transition
    /// that defers settlement cannot drop them.
    pub(crate) pending: Vec<PendingFact>,
    /// Derived; rebuilt on restore.
    pub(crate) index: Shared<BindingIndex>,
    /// Derived; rebuilt on restore.
    pub(crate) due: Shared<BTreeSet<(u64, TimerId)>>,
}

/// Maps serialize as ordered pairs so non-string keys stay portable.
impl Serialize for ScriptState {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("ScriptState", 9)?;
        s.serialize_field("handlers", &self.handlers)?;
        s.serialize_field("schema", &*self.schema)?;
        s.serialize_field("bindings", &self.bindings.iter().collect::<Vec<_>>())?;
        s.serialize_field("timers", &self.timers.iter().collect::<Vec<_>>())?;
        s.serialize_field("vars", &self.vars.iter().collect::<Vec<_>>())?;
        s.serialize_field("rng", &self.rng)?;
        s.serialize_field("next_binding", &self.next_binding)?;
        s.serialize_field("next_timer", &self.next_timer)?;
        s.serialize_field("pending", &self.pending)?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for ScriptState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            handlers: u32,
            schema: VariableSchema,
            bindings: Vec<(BindingId, Binding)>,
            timers: Vec<(TimerId, Timer)>,
            vars: Vec<((VarScope, String), Value)>,
            rng: u64,
            next_binding: u64,
            next_timer: u64,
            pending: Vec<PendingFact>,
        }
        let f = Fields::deserialize(deserializer)?;
        let invalid = |what: &str| serde::de::Error::custom(format!("invalid script {what}"));
        if f.bindings.len() > MAX_BINDINGS
            || f.timers.len() > MAX_TIMERS
            || f.vars.len() > MAX_VARIABLES
            || f.pending.len() > MAX_PENDING_FACTS
        {
            return Err(invalid("capacity"));
        }
        if !strictly_ordered(f.bindings.iter().map(|(id, _)| id.0))
            || f.bindings.iter().any(|(id, b)| id.0 >= f.next_binding || !b.valid(f.handlers))
        {
            return Err(invalid("binding"));
        }
        if !strictly_ordered(f.timers.iter().map(|(id, _)| id.0))
            || f.timers.iter().any(|(id, t)| id.0 >= f.next_timer || t.handler.0 >= f.handlers)
        {
            return Err(invalid("timer"));
        }
        let bindings: BTreeMap<_, _> = f.bindings.into_iter().collect();
        let timers: BTreeMap<_, _> = f.timers.into_iter().collect();
        if f
            .schema
            .variables
            .keys()
            .any(|name| !super::bounded_name(name) || name.len() > MAX_NAME_BYTES)
        {
            return Err(invalid("schema"));
        }
        let mut vars = BTreeMap::new();
        for ((scope, name), value) in f.vars {
            if !f.schema.check(scope, &name, Some(&value))
                || vars.insert((scope, name), value).is_some()
            {
                return Err(invalid("variable"));
            }
        }
        if f
            .pending
            .iter()
            .any(|pending| {
                pending.handler.0 >= f.handlers
                    || pending.binding.is_some_and(|id| {
                        bindings.get(&id).is_none_or(|b| b.handler != pending.handler)
                    })
            })
        {
            return Err(invalid("pending fact"));
        }
        let mut state = ScriptState {
            handlers: f.handlers,
            schema: Shared::new(f.schema),
            bindings: Shared::new(bindings),
            timers: Shared::new(timers),
            vars: Shared::new(vars),
            rng: f.rng,
            next_binding: f.next_binding,
            next_timer: f.next_timer,
            pending: f.pending,
            index: Shared::default(),
            due: Shared::default(),
        };
        state.rebuild_derived();
        Ok(state)
    }
}

fn strictly_ordered(ids: impl Iterator<Item = u64>) -> bool {
    let mut last = 0;
    for id in ids {
        if id <= last {
            return false;
        }
        last = id;
    }
    true
}

impl ScriptState {
    pub(crate) fn new(setup: ScriptSetup, seed: u64) -> Option<Self> {
        let mut state = ScriptState {
            handlers: setup.handlers,
            schema: Shared::new(setup.schema),
            bindings: Shared::default(),
            timers: Shared::default(),
            vars: Shared::default(),
            // A dedicated stream: script draws never touch rules randomness.
            rng: crate::dice::random_word_at(seed ^ 0x746f_722d_7363_7269, 1),
            next_binding: 1,
            next_timer: 1,
            pending: Vec::new(),
            index: Shared::default(),
            due: Shared::default(),
        };
        for binding in setup.bindings {
            let id = BindingId(state.next_binding);
            state.bind(id, binding)?;
        }
        for (name, value) in setup.defaults {
            state.set_var(VarScope::Scenario, name, value)?;
        }
        Some(state)
    }

    fn rebuild_derived(&mut self) {
        let mut index = BindingIndex::default();
        for (&id, binding) in self.bindings.iter() {
            index.insert(id, binding);
        }
        self.index = Shared::new(index);
        self.due = Shared::new(
            self.timers
                .iter()
                .filter_map(|(&id, timer)| match timer.clock {
                    TimerClock::Due(due) => Some((due, id)),
                    TimerClock::Frozen { .. } => None,
                })
                .collect(),
        );
    }

    pub(crate) fn mask(&self) -> u64 {
        self.index.mask()
    }

    pub fn handlers(&self) -> u32 {
        self.handlers
    }
    pub fn binding(&self, id: BindingId) -> Option<&Binding> {
        self.bindings.get(&id)
    }
    pub fn bindings(&self) -> impl Iterator<Item = (BindingId, &Binding)> {
        self.bindings.iter().map(|(&id, b)| (id, b))
    }
    pub fn timer(&self, id: TimerId) -> Option<&Timer> {
        self.timers.get(&id)
    }
    pub fn var(&self, scope: VarScope, name: &str) -> Option<&Value> {
        self.vars.get(&(scope, name.to_owned()))
    }
    pub fn rng(&self) -> u64 {
        self.rng
    }
    pub fn next_binding(&self) -> BindingId {
        BindingId(self.next_binding)
    }
    pub fn next_timer(&self) -> TimerId {
        TimerId(self.next_timer)
    }
    pub fn schema(&self) -> &VariableSchema {
        &self.schema
    }

    /// Bind under exactly the predicted identity.
    pub(crate) fn bind(&mut self, id: BindingId, binding: Binding) -> Option<()> {
        if id.0 != self.next_binding
            || !binding.valid(self.handlers)
            || self.bindings.len() >= MAX_BINDINGS
        {
            return None;
        }
        self.next_binding = self.next_binding.checked_add(1)?;
        self.index.insert(id, &binding);
        self.bindings.insert(id, binding);
        Some(())
    }

    pub(crate) fn unbind(&mut self, id: BindingId) -> Option<Binding> {
        let binding = self.bindings.remove(&id)?;
        self.index.remove(id, &binding);
        self.pending.retain(|pending| pending.binding != Some(id));
        Some(binding)
    }

    pub(crate) fn set_var(&mut self, scope: VarScope, name: String, value: Value) -> Option<()> {
        if !self.schema.check(scope, &name, Some(&value)) {
            return None;
        }
        let key = (scope, name);
        if !self.vars.contains_key(&key) && self.vars.len() >= MAX_VARIABLES {
            return None;
        }
        self.vars.insert(key, value);
        Some(())
    }

    pub(crate) fn clear_var(&mut self, scope: VarScope, name: String) -> Option<()> {
        if !self.schema.check(scope, &name, None) {
            return None;
        }
        self.vars.remove(&(scope, name));
        Some(())
    }

    pub(crate) fn start_timer(&mut self, id: TimerId, timer: Timer) -> Option<()> {
        if id.0 != self.next_timer
            || timer.handler.0 >= self.handlers
            || self.timers.len() >= MAX_TIMERS
        {
            return None;
        }
        self.next_timer = self.next_timer.checked_add(1)?;
        if let TimerClock::Due(due) = timer.clock {
            self.due.insert((due, id));
        }
        self.timers.insert(id, timer);
        Some(())
    }

    pub(crate) fn cancel_timer(&mut self, id: TimerId) -> Option<Timer> {
        let timer = self.timers.remove(&id)?;
        if let TimerClock::Due(due) = timer.clock {
            self.due.remove(&(due, id));
        }
        Some(timer)
    }

    pub(crate) fn next_due(&self) -> Option<u64> {
        self.due.first().map(|&(due, _)| due)
    }

    /// Remove and return every running timer due at or before `tick`, in due
    /// then creation order.
    pub(crate) fn take_due(&mut self, tick: u64) -> Vec<(TimerId, Timer)> {
        let mut fired = Vec::new();
        while let Some(&(due, id)) = self.due.first() {
            if due > tick {
                break;
            }
            self.due.pop_first();
            if let Some(timer) = self.timers.remove(&id) {
                fired.push((id, timer));
            }
        }
        fired
    }

    /// Freeze or thaw timers selected by `owned`. Frozen timers keep their
    /// remaining time; thawed ones resume from `tick` without catch-up.
    pub(crate) fn shift_timers(&mut self, tick: u64, freeze: bool, owned: impl Fn(&Owner) -> bool) {
        let selected: Vec<TimerId> = self
            .timers
            .iter()
            .filter(|(_, timer)| {
                timer.owner.as_ref().is_some_and(&owned)
                    && matches!(timer.clock, TimerClock::Due(_)) == freeze
            })
            .map(|(&id, _)| id)
            .collect();
        for id in selected {
            let Some(timer) = self.timers.get_mut(&id) else {
                continue;
            };
            timer.clock = match timer.clock {
                TimerClock::Due(due) => {
                    self.due.remove(&(due, id));
                    TimerClock::Frozen {
                        remaining: due.saturating_sub(tick),
                    }
                }
                TimerClock::Frozen { remaining } => {
                    let due = tick.saturating_add(remaining);
                    self.due.insert((due, id));
                    TimerClock::Due(due)
                }
            };
        }
    }
}
