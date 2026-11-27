//! Bounded appearance storage and a chart aligned only by disclosed identities.
use crate::RememberedCell;
use std::collections::{BTreeMap, BTreeSet};
use tor_protocol::*;

/// A chart occurrence borrows its appearance; several offsets may name one cell.
#[derive(Clone, Copy, Debug)]
pub struct MapCell<'a> {
    pub position: Position,
    pub cell: &'a RememberedCell,
}
impl std::ops::Deref for MapCell<'_> {
    type Target = RememberedCell;
    fn deref(&self) -> &Self::Target {
        self.cell
    }
}
impl<'a> MapCell<'a> {
    pub fn appearance(&self) -> RememberedMapCell {
        RememberedMapCell {
            position: self.position,
            ..self.cell.appearance.clone()
        }
    }
    pub fn items(&self) -> &'a [MapItemAppearance] {
        &self.cell.ground_items
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MapMemory {
    pub records: BTreeMap<String, RememberedCell>,
    /// Placed occurrences. Every value names a key in `records`.
    pub cells: BTreeMap<Position, String>,
    previous: BTreeMap<String, Vec<Position>>,
    previous_viewport: Option<MapWindow>,
    record_sizes: BTreeMap<String, usize>,
    bytes: usize,
    pub limited: bool,
    pub coverage: Option<MapWindow>,
}

pub(crate) fn appearance(cell: &CellView, items: &[&GroundItemView]) -> RememberedMapCell {
    RememberedMapCell {
        object_presentation: cell.object_presentation.clone(),
        key: cell.key.clone(),
        position: cell.position,
        wall: cell.wall,
        material: cell.material.clone(),
        asset: cell.asset.clone(),
        place_hint: cell.place_hint,
        stairs_up: cell.stairs_up,
        stairs_down: cell.stairs_down,
        trap: cell.trap.as_ref().map(|trap| MapTrapAppearance {
            family: trap.family,
        }),
        door: cell.door.as_ref().map(|d| MapDoorAppearance {
            name: d.name.clone(),
            description: d.description.clone(),
            open: d.open,
            asset: d.asset.clone(),
        }),
        ground_items: items
            .iter()
            .map(|g| MapItemAppearance {
                presentation: g.presentation.clone(),
                class: g.item.class,
                description: g.item.description.clone(),
                name: g.item.name.clone(),
                appearance: g.item.appearance.clone(),
                identified: g.item.identified,
                quantity: g.item.quantity,
                asset: g.item.asset.clone(),
            })
            .collect(),
    }
}

/// Distance to a viewport, used only when displaced occurrences exceed the
/// same placement budget that bounded the old viewport-only cache.
fn distance_to(position: Position, viewport: MapWindow) -> i64 {
    let axis = |at: i32, min: i32, max: i32| {
        let at = i64::from(at);
        (i64::from(min) - at).max(at - i64::from(max)).max(0)
    };
    axis(position.x, viewport.min.x, viewport.max.x)
        .max(axis(position.y, viewport.min.y, viewport.max.y))
        .max(axis(position.z, viewport.min.z, viewport.max.z))
}

impl MapMemory {
    pub fn positioned_in(&self, window: Option<MapWindow>) -> impl Iterator<Item = MapCell<'_>> {
        window.into_iter().flat_map(move |window| {
            self.cells
                .range(window.min..=window.max)
                .filter_map(move |(position, key)| {
                    if !window.contains(*position) {
                        return None;
                    }
                    self.records.get(key).map(|cell| MapCell {
                        position: *position,
                        cell,
                    })
                })
        })
    }
    pub fn at(&self, position: Position) -> Option<RememberedMapCell> {
        self.records
            .get(self.cells.get(&position)?)
            .map(|c| RememberedMapCell {
                position,
                ..c.appearance.clone()
            })
    }
    /// Translation uses every unique common anchor. No anchor or disagreement
    /// discards only placement, never creates a guessed embedding.
    pub fn observe(&mut self, view: &Observation, revision: u64) -> bool {
        let mut current: BTreeMap<String, Vec<Position>> = BTreeMap::new();
        for cell in &view.visible_cells {
            current
                .entry(cell.key.clone())
                .or_default()
                .push(cell.position);
        }
        let mut anchors = Vec::new();
        let mut shift = None;
        let mut conflict = false;
        for (key, positions) in &current {
            if let Some(old) = self.previous.get(key) {
                if let ([before], [after]) = (old.as_slice(), positions.as_slice()) {
                    anchors.push(*before);
                    let delta = (
                        i64::from(after.x) - i64::from(before.x),
                        i64::from(after.y) - i64::from(before.y),
                        i64::from(after.z) - i64::from(before.z),
                    );
                    conflict |= shift.is_some_and(|s| s != delta);
                    shift = Some(delta);
                }
            }
        }
        let translation = shift.filter(|_| !conflict);
        // A translation of one point or a line cannot exclude a rotation.
        // Only two independent anchor vectors certify retained coverage.
        let oriented = anchors.first().is_some_and(|origin| {
            let vector = |a: &Position| {
                [
                    i128::from(a.x) - i128::from(origin.x),
                    i128::from(a.y) - i128::from(origin.y),
                    i128::from(a.z) - i128::from(origin.z),
                ]
            };
            anchors
                .iter()
                .skip(1)
                .map(vector)
                .find(|v| *v != [0; 3])
                .is_some_and(|v| {
                    anchors.iter().skip(1).map(vector).any(|w| {
                        v[0] * w[1] != v[1] * w[0]
                            || v[0] * w[2] != v[2] * w[0]
                            || v[1] * w[2] != v[2] * w[1]
                    })
                })
        });
        let certified = oriented && translation.is_some();
        let lost_orientation = self.coverage.is_some() && !certified;
        if !certified {
            self.coverage = None;
            // Weak alignment retains the old viewport-only behavior. Discard
            // previously offscreen positions before any tentative shift, or
            // that shift could bring unprovably aligned terrain back into view.
            self.cells.retain(|position, _| {
                self.previous_viewport
                    .is_some_and(|old| old.contains(*position))
            });
        }
        if translation != Some((0, 0, 0)) {
            let old = std::mem::take(&mut self.cells);
            self.coverage = self.coverage.and_then(|w| {
                let (dx, dy, dz) = translation?;
                Some(MapWindow {
                    min: translate(w.min, dx, dy, dz)?,
                    max: translate(w.max, dx, dy, dz)?,
                })
            });
            if let Some((dx, dy, dz)) = translation {
                for (p, key) in old {
                    if let Some(p) = translate(p, dx, dy, dz) {
                        self.cells.insert(p, key);
                    }
                }
            }
        }
        let window = MapWindow::normal().checked_translate(view.position);
        // A verified translation keeps offscreen positions for the first
        // reverse frame. Without orientation evidence, keep the original
        // viewport-only behavior rather than extending a potentially rotated chart.
        if !certified {
            self.cells
                .retain(|p, _| window.is_some_and(|w| w.contains(*p)));
        }
        self.coverage = self.coverage.and_then(|old| {
            let window = window?;
            let clipped = MapWindow {
                min: Position {
                    x: old.min.x.max(window.min.x),
                    y: old.min.y.max(window.min.y),
                    z: old.min.z.max(window.min.z),
                },
                max: Position {
                    x: old.max.x.min(window.max.x),
                    y: old.max.y.min(window.max.y),
                    z: old.max.z.min(window.max.z),
                },
            };
            clipped.validate().ok().map(|_| clipped)
        });
        self.overlay(view, revision);
        self.previous = current;
        self.previous_viewport = window;
        translation != Some((0, 0, 0)) || lost_orientation
    }
    pub fn overlay(&mut self, view: &Observation, revision: u64) {
        let window = MapWindow::normal().checked_translate(view.position);
        let mut items: BTreeMap<Position, Vec<&GroundItemView>> = BTreeMap::new();
        for item in &view.ground_items {
            items.entry(item.position).or_default().push(item);
        }
        for cell in &view.visible_cells {
            let known = RememberedCell {
                appearance: appearance(cell, items.get(&cell.position).map_or(&[], Vec::as_slice)),
                last_seen_tick: Some(view.tick),
                last_seen_revision: Some(revision),
            };
            self.put(cell.key.clone(), known);
            if self.records.contains_key(&cell.key)
                && window.as_ref().is_some_and(|w| w.contains(cell.position))
            {
                self.cells.insert(cell.position, cell.key.clone());
            }
        }
        self.trim(window);
    }
    fn remove(&mut self, key: &str) {
        self.records.remove(key);
        self.bytes -= self.record_sizes.remove(key).unwrap_or(0);
    }
    fn put(&mut self, key: String, cell: RememberedCell) {
        if let Some(old) = self.records.get_mut(&key) {
            if old.appearance == cell.appearance {
                old.last_seen_tick = cell.last_seen_tick;
                old.last_seen_revision = cell.last_seen_revision;
                return;
            }
        }
        let size = encoded_length(&cell.appearance, MAX_MAP_TRANSFER_BYTES)
            .ok()
            .flatten();
        self.remove(&key);
        if let Some(size) = size {
            self.bytes += size;
            self.record_sizes.insert(key.clone(), size);
            self.records.insert(key, cell);
        } else {
            self.cells.retain(|_, k| k != &key);
            self.limited = true;
            self.coverage = None;
        }
    }
    fn trim(&mut self, viewport: Option<MapWindow>) {
        if let Some(window) = viewport {
            let excess = self.cells.len().saturating_sub(MAX_MAP_TRANSFER_CELLS);
            if excess > 0 {
                let mut offscreen: Vec<_> = self
                    .cells
                    .keys()
                    .copied()
                    .filter(|position| !window.contains(*position))
                    .collect();
                offscreen.sort_unstable_by_key(|position| {
                    std::cmp::Reverse((distance_to(*position, window), *position))
                });
                for position in offscreen.into_iter().take(excess) {
                    self.cells.remove(&position);
                }
            }
        } else {
            self.cells.clear();
        }
        let over_budget = |this: &Self| {
            this.records.len() > MAX_MAP_TRANSFER_CELLS || this.bytes > MAX_MAP_TRANSFER_BYTES
        };
        let mut evicted = false;
        if over_budget(self) {
            let placed: BTreeSet<_> = self.cells.values().cloned().collect();
            let keys: Vec<_> = self
                .records
                .keys()
                .filter(|key| !placed.contains(*key))
                .cloned()
                .collect();
            for key in keys {
                if self.records.len() <= MAX_MAP_TRANSFER_CELLS
                    && self.bytes <= MAX_MAP_TRANSFER_BYTES
                {
                    break;
                }
                self.remove(&key);
                evicted = true;
            }
        }
        if over_budget(self) {
            // Offscreen memory never squeezes out a known position in view.
            // Evict those extra appearances first, without invalidating coverage.
            let on_screen: BTreeSet<_> = self
                .cells
                .iter()
                .filter(|(position, _)| viewport.is_some_and(|w| w.contains(**position)))
                .map(|(_, key)| key.as_str())
                .collect();
            let offscreen: BTreeSet<_> = self
                .cells
                .values()
                .filter(|key| !on_screen.contains(key.as_str()))
                .cloned()
                .collect();
            for key in offscreen {
                if !over_budget(self) {
                    break;
                }
                self.remove(&key);
                evicted = true;
            }
        }
        // Pathological valid live metadata may exceed the independent cache
        // budget. Its current Observation still renders; no silent claim of
        // complete stale coverage survives an eviction.
        while self.records.len() > MAX_MAP_TRANSFER_CELLS || self.bytes > MAX_MAP_TRANSFER_BYTES {
            let Some(key) = self.records.last_key_value().map(|(k, _)| k.clone()) else {
                break;
            };
            self.remove(&key);
            evicted = true;
            self.limited = true;
            self.coverage = None;
        }
        // Without an eviction every placement still names a kept record, so
        // the ordinary per-update path skips this scan of every placement.
        if evicted {
            self.forget_unrecorded_placements();
        }
    }
    /// Restore the placement invariant after evictions or a bulk install.
    fn forget_unrecorded_placements(&mut self) {
        self.cells.retain(|_, key| self.records.contains_key(key));
    }
    pub fn needs_window(&self, requested: MapWindow) -> bool {
        self.coverage
            .as_ref()
            .is_none_or(|w| !w.contains(requested.min) || !w.contains(requested.max))
    }
    /// Grow the certified rectangle by an entire adjacent face, never a
    /// bounding box over holes. Later axes include already committed corners.
    pub fn next_uncovered(&self, requested: MapWindow) -> Option<MapWindow> {
        let Some(covered) = self.coverage else {
            return Some(requested);
        };
        let mut min = [covered.min.x, covered.min.y, covered.min.z];
        let mut max = [covered.max.x, covered.max.y, covered.max.z];
        let wanted_min = [requested.min.x, requested.min.y, requested.min.z];
        let wanted_max = [requested.max.x, requested.max.y, requested.max.z];
        for axis in 0..3 {
            if min[axis] > wanted_min[axis] {
                max[axis] = min[axis].checked_sub(1)?;
                min[axis] = wanted_min[axis];
                return Some(bounds(min, max));
            }
            if max[axis] < wanted_max[axis] {
                min[axis] = max[axis].checked_add(1)?;
                max[axis] = wanted_max[axis];
                return Some(bounds(min, max));
            }
        }
        None
    }
    pub fn install_region(
        &mut self,
        viewport: MapWindow,
        window: MapWindow,
        cells: BTreeMap<Position, RememberedMapCell>,
    ) -> Result<(), &'static str> {
        if self.next_uncovered(viewport) != Some(window) {
            return Err("Map region is not the next exact uncovered face");
        }
        let coverage = self.coverage.map_or(window, |old| MapWindow {
            min: Position {
                x: old.min.x.min(window.min.x),
                y: old.min.y.min(window.min.y),
                z: old.min.z.min(window.min.z),
            },
            max: Position {
                x: old.max.x.max(window.max.x),
                y: old.max.y.max(window.max.y),
                z: old.max.z.max(window.max.z),
            },
        });
        self.cells.retain(|position, _| !window.contains(*position));
        self.coverage = Some(coverage);
        self.install_cells(cells);
        self.trim(Some(viewport));
        self.forget_unrecorded_placements();
        Ok(())
    }
    /// Copy each appearance once for the native thread, even when several
    /// projected offsets refer to the same opaque cell key.
    pub fn snapshot_records(&self) -> BTreeMap<String, RememberedCell> {
        self.cells
            .values()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter_map(|key| {
                self.records
                    .get(key)
                    .map(|record| (key.clone(), record.clone()))
            })
            .collect()
    }

    /// Native presentation installs all verified local positions, including
    /// offscreen ones. This never includes unfinished transfer staging.
    pub fn install_snapshot(
        &mut self,
        viewport: Option<MapWindow>,
        coverage: Option<MapWindow>,
        placements: BTreeMap<Position, String>,
        records: BTreeMap<String, RememberedCell>,
    ) {
        self.cells = placements;
        self.coverage = coverage;
        self.limited = false;
        for (key, record) in records {
            self.put(key, record);
        }
        self.trim(viewport);
        self.forget_unrecorded_placements();
    }
    fn install_cells(&mut self, cells: BTreeMap<Position, RememberedMapCell>) {
        self.limited = false;
        for (position, appearance) in cells {
            let key = appearance.key.clone();
            // Existing exact last sightings retain their honest diagnostic metadata.
            let keep = self.records.get(&key).is_some_and(|old| {
                let mut same = old.appearance.clone();
                same.position = position;
                same == appearance
            });
            if !keep {
                self.put(
                    key.clone(),
                    RememberedCell {
                        appearance,
                        last_seen_tick: None,
                        last_seen_revision: None,
                    },
                );
            }
            self.cells.insert(position, key);
        }
    }
}
fn translate(p: Position, x: i64, y: i64, z: i64) -> Option<Position> {
    Some(Position {
        x: i32::try_from(i64::from(p.x).checked_add(x)?).ok()?,
        y: i32::try_from(i64::from(p.y).checked_add(y)?).ok()?,
        z: i32::try_from(i64::from(p.z).checked_add(z)?).ok()?,
    })
}

fn bounds(min: [i32; 3], max: [i32; 3]) -> MapWindow {
    MapWindow {
        min: Position {
            x: min[0],
            y: min[1],
            z: min[2],
        },
        max: Position {
            x: max[0],
            y: max[1],
            z: max[2],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remembered(key: &str, position: Position) -> RememberedCell {
        RememberedCell {
            appearance: RememberedMapCell {
                object_presentation: None,
                key: key.into(),
                position,
                wall: false,
                material: "stone".into(),
                asset: None,
                place_hint: false,
                stairs_up: false,
                stairs_down: false,
                trap: None,
                door: None,
                ground_items: vec![],
            },
            last_seen_tick: None,
            last_seen_revision: None,
        }
    }

    #[test]
    fn a_snapshot_placement_without_its_record_is_not_installed() {
        let viewport = MapWindow::normal();
        let kept = viewport.min;
        let orphan = viewport.max;
        let mut memory = MapMemory::default();
        memory.install_snapshot(
            Some(viewport),
            Some(viewport),
            BTreeMap::from([(kept, "kept".into()), (orphan, "missing".into())]),
            BTreeMap::from([("kept".into(), remembered("kept", kept))]),
        );
        assert!(memory.at(kept).is_some());
        assert!(memory.at(orphan).is_none());
        assert_eq!(memory.cells.len(), 1);
    }

    #[test]
    fn evicted_records_take_their_placements_with_them() {
        let viewport = MapWindow::normal();
        let mut memory = MapMemory {
            coverage: Some(viewport),
            ..Default::default()
        };
        // Few placed offscreen records whose bytes exceed the budget, so
        // eviction must remove records that still have placements.
        let large = "x".repeat(1 << 20);
        for index in 0..10 {
            let key = format!("cell-{index}");
            let position = Position {
                x: viewport.max.x + 1 + index,
                y: 0,
                z: 0,
            };
            let mut record = remembered(&key, position);
            record.appearance.material = large.clone();
            memory.put(key.clone(), record);
            memory.cells.insert(position, key);
        }
        assert!(memory.bytes > MAX_MAP_TRANSFER_BYTES);
        memory.trim(Some(viewport));
        assert!(memory.bytes <= MAX_MAP_TRANSFER_BYTES);
        assert!(memory.records.len() < 10);
        assert!(!memory.cells.is_empty());
        assert!(memory
            .cells
            .values()
            .all(|key| memory.records.contains_key(key)));
    }

    #[test]
    fn offscreen_placement_budget_drops_farthest_aliased_occurrences_not_visible_ones() {
        let viewport = MapWindow::normal();
        let mut memory = MapMemory {
            coverage: Some(viewport),
            ..Default::default()
        };
        let appearance = RememberedMapCell {
            object_presentation: None,
            key: "same".into(),
            position: viewport.min,
            wall: true,
            material: "stone".into(),
            asset: None,
            place_hint: false,
            stairs_up: false,
            stairs_down: false,
            trap: None,
            door: None,
            ground_items: vec![],
        };
        memory.put(
            appearance.key.clone(),
            RememberedCell {
                appearance,
                last_seen_tick: None,
                last_seen_revision: None,
            },
        );
        for x in viewport.min.x..=viewport.max.x {
            for y in viewport.min.y..=viewport.max.y {
                for z in viewport.min.z..=viewport.max.z {
                    if (x, y, z) != (0, 0, 0) {
                        memory.cells.insert(Position { x, y, z }, "same".into());
                    }
                }
            }
        }
        let near = Position {
            x: viewport.max.x + 1,
            y: 0,
            z: 0,
        };
        let far = Position {
            x: viewport.max.x + 80,
            y: 0,
            z: 0,
        };
        for position in [near, far] {
            memory.cells.insert(position, "same".into());
        }
        memory.trim(Some(viewport));
        assert_eq!(memory.cells.len(), MAX_MAP_TRANSFER_CELLS);
        assert_eq!(
            memory.positioned_in(Some(viewport)).count(),
            MAX_MAP_TRANSFER_CELLS - 1
        );
        assert!(memory.at(near).is_some());
        assert!(memory.at(far).is_none());
        assert!(memory.at(Position { x: 0, y: 0, z: 0 }).is_none());
        assert_eq!(memory.coverage, Some(viewport));
        assert_eq!(
            memory.snapshot_records().len(),
            1,
            "aliases share one handoff record"
        );
        assert!(!memory.limited);
    }
}
