//! Local inventory browsing, independent of actions and server admission.
use crate::{Carried, View};
use tor_protocol::{ItemClass, ItemTarget};

/// Maximum Unicode scalar values retained in the search query, not a byte limit.
pub const MAX_INVENTORY_SEARCH: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
/// Local carried-item categories; none changes disclosed item affordances.
pub enum InventoryFilter {
    #[default]
    All,
    Gear,
    Usable,
}

impl InventoryFilter {
    fn accepts(self, item: &Carried) -> bool {
        match self {
            Self::All => true,
            Self::Gear => {
                item.equipped.is_some()
                    || item.can_equip
                    || matches!(
                        item.class,
                        ItemClass::Weapon | ItemClass::Armor | ItemClass::Ring | ItemClass::Amulet
                    )
            }
            Self::Usable => {
                item.drinkable
                    || item.edible
                    || item.readable
                    || item.device.is_some()
                    || item.portable_light.is_some()
            }
        }
    }
}

/// Search and selection by exact item target, without submitting game actions.
///
/// Carried items follow the filter and query. Reachable ground items are also
/// selectable and follow carried items during keyboard navigation. Call
/// [`Self::reconcile`] after changing a filter/query or receiving a new view;
/// setters alone do not invalidate the retained selection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InventoryBrowser {
    filter: InventoryFilter,
    query: String,
    selected: Option<ItemTarget>,
}

impl InventoryBrowser {
    pub fn filter(&self) -> InventoryFilter {
        self.filter
    }
    pub fn set_filter(&mut self, filter: InventoryFilter) {
        self.filter = filter;
    }
    pub fn query(&self) -> &str {
        &self.query
    }
    pub fn selected(&self) -> Option<ItemTarget> {
        self.selected
    }
    pub fn clear_selection(&mut self) {
        self.selected = None;
    }
    /// Remove control characters and retain at most [`MAX_INVENTORY_SEARCH`]
    /// Unicode scalar values. Matching trims whitespace and ignores case.
    ///
    /// ```
    /// use tor_client_3d_model::{InventoryBrowser, MAX_INVENTORY_SEARCH};
    /// let mut browser = InventoryBrowser::default();
    /// browser.set_query("  Potion\n");
    /// assert_eq!(browser.query(), "  Potion");
    /// browser.set_query(&"é".repeat(MAX_INVENTORY_SEARCH + 1));
    /// assert_eq!(browser.query().chars().count(), MAX_INVENTORY_SEARCH);
    /// ```
    pub fn set_query(&mut self, query: &str) {
        self.query = query
            .chars()
            .filter(|c| !c.is_control())
            .take(MAX_INVENTORY_SEARCH)
            .collect();
    }
    /// Matching carried items in their disclosed order; ground items are excluded.
    pub fn carried<'a>(&'a self, view: &'a View) -> impl Iterator<Item = &'a Carried> {
        let query = self.query.trim().to_lowercase();
        view.inventory.iter().filter(move |item| {
            self.filter.accepts(item) && item.name.to_lowercase().contains(&query)
        })
    }
    /// Select a matching carried item or reachable ground item.
    /// Returns false without changing the old selection if the target is absent.
    pub fn select(&mut self, view: &View, target: ItemTarget) -> bool {
        if self.contains(view, target) {
            self.selected = Some(target);
            true
        } else {
            false
        }
    }
    /// Clear a target that no longer passes the carried/ground selection rules.
    pub fn reconcile(&mut self, view: &View) {
        if self
            .selected
            .is_some_and(|target| !self.contains(view, target))
        {
            self.selected = None;
        }
    }
    /// Move through matching carried items, then reachable ground items.
    /// Selection clamps at the ends; an absent selection starts at the end only
    /// for a negative delta, otherwise at the beginning. An empty list clears it.
    pub fn shift(&mut self, view: &View, delta: i32) {
        let targets: Vec<_> = self
            .carried(view)
            .map(|item| item.id)
            .chain(
                view.items
                    .iter()
                    .filter(|item| item.reachable)
                    .map(|item| item.id),
            )
            .collect();
        if targets.is_empty() {
            self.selected = None;
            return;
        }
        let next = self
            .selected
            .and_then(|target| targets.iter().position(|id| *id == target))
            .map_or_else(
                || if delta < 0 { targets.len() - 1 } else { 0 },
                |index| {
                    (index as i64 + i64::from(delta)).clamp(0, targets.len() as i64 - 1) as usize
                },
            );
        self.selected = Some(targets[next]);
    }
    fn contains(&self, view: &View, target: ItemTarget) -> bool {
        self.carried(view).any(|item| item.id == target)
            || view
                .items
                .iter()
                .any(|item| item.id == target && item.reachable)
    }
}
