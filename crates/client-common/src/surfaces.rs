//! Floors, ceilings and walls are ordinary seen solid cells; the server sends no
//! separate surface facts. A solid cell's role follows from the open cells seen
//! next to it: an open cell above makes it a floor, one below a ceiling, and
//! one beside it a wall. A cell can have several roles.
//!
//! These helpers describe the supplied disclosed cells. They do not query the
//! server, fill unseen space, infer passability, or grant action authority.
//! Positions share the observer-relative frame of that cell collection. Do not
//! combine cells from different observation frames without the caller's normal
//! validated map transformation.
//!
//! [`roles`] and [`roles_by`] collect sorted, distinct labels, not one label per
//! surface. [`floor_below`] inspects the adjacent cell only; [`ceiling_above`]
//! requires an uninterrupted disclosed column and searches at most 16 cells.
use std::collections::{BTreeMap, BTreeSet};
use tor_protocol::{CellView, Position};

/// How far straight up [`ceiling_above`] looks, in cells.
const CEILING_SEARCH: i32 = 16;

/// Distinct borrowed labels of disclosed solid cells, classified by adjacency.
///
/// A material can occur in more than one set. Sets sort lexicographically and
/// borrow from the input cells (or static labels returned by [`roles_by`]); they
/// do not preserve cell counts or positions. Empty sets mean no matching role
/// was established from the supplied cells, not that the world has no surface.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Roles<'a> {
    /// Labels with a disclosed open neighbor immediately above.
    pub floors: BTreeSet<&'a str>,
    /// Labels with a disclosed open neighbor immediately below.
    pub ceilings: BTreeSet<&'a str>,
    /// Labels with a disclosed open neighbor in a horizontal cardinal direction.
    pub walls: BTreeSet<&'a str>,
}

/// Borrow the supplied material label, or a static fallback for an empty label.
/// This does not check that the cell is solid or sanitize nonempty text.
/// Whitespace-only material is returned unchanged.
///
/// ```
/// # use tor_protocol::{CellView, Position};
/// # fn cell(x: i32, y: i32, z: i32, wall: bool, material: &str) -> CellView {
/// #     CellView {
/// #         position: Position { x, y, z }, wall, material: material.into(),
/// #         key: format!("{x},{y},{z}"), lit: true, asset: None, door: None,
/// #         trap: None, object_presentation: None, stairs_up: false,
/// #         stairs_down: false, place_hint: false,
/// #     }
/// # }
/// use tor_client_common::surfaces::material;
/// let unspecified = cell(0, 0, -1, true, "");
/// assert_eq!(material(&unspecified), "unremarkable material");
/// let stone = cell(0, 0, -1, true, "stone");
/// assert_eq!(material(&stone), "stone");
/// ```
pub fn material(cell: &CellView) -> &str {
    if cell.material.is_empty() {
        "unremarkable material"
    } else {
        &cell.material
    }
}

/// Cells by relative position; the first occurrence wins, as in rendering.
fn index(cells: &[CellView]) -> BTreeMap<(i32, i32, i32), &CellView> {
    let mut map = BTreeMap::new();
    for cell in cells {
        let p = cell.position;
        map.entry((p.x, p.y, p.z)).or_insert(cell);
    }
    map
}

fn open(map: &BTreeMap<(i32, i32, i32), &CellView>, key: (i32, i32, i32)) -> bool {
    map.get(&key).is_some_and(|cell| !cell.wall)
}

/// Classify disclosed solid cells using [`material`] as their label.
///
/// Only adjacent disclosed open cells establish a role. Diagonal neighbors do
/// not establish walls. The first cell at a duplicate position wins, including
/// its open/solid state. See [`roles_by`] for coordinate requirements.
///
/// ```
/// # use tor_protocol::{CellView, Position};
/// # fn cell(x: i32, y: i32, z: i32, wall: bool, material: &str) -> CellView {
/// #     CellView {
/// #         position: Position { x, y, z }, wall, material: material.into(),
/// #         key: format!("{x},{y},{z}"), lit: true, asset: None, door: None,
/// #         trap: None, object_presentation: None, stairs_up: false,
/// #         stairs_down: false, place_hint: false,
/// #     }
/// # }
/// use tor_client_common::surfaces::roles;
/// let cells = vec![
///     cell(0, 0, -1, true, "stone"),
///     cell(0, 0, 0, false, ""),
///     cell(0, 0, 1, false, ""),
///     cell(0, 0, 2, true, "wood"),
///     cell(1, 0, 0, true, "marble"),
/// ];
/// let surfaces = roles(&cells);
/// assert_eq!(surfaces.floors.into_iter().collect::<Vec<_>>(), ["stone"]);
/// assert_eq!(surfaces.ceilings.into_iter().collect::<Vec<_>>(), ["wood"]);
/// assert_eq!(surfaces.walls.into_iter().collect::<Vec<_>>(), ["marble"]);
/// // A solid cell alone does not disclose which side faces open space.
/// assert_eq!(roles(&cells[..1]), Default::default());
/// ```
pub fn roles(cells: &[CellView]) -> Roles<'_> {
    roles_by(cells, material)
}

/// Classify cells with caller-provided labels, for example palette asset names.
///
/// `name` is called only for solid cells with an established role, possibly
/// more than once when one cell has several roles. Return a label borrowed from
/// the cell or a static label. Prefer a deterministic, side-effect-free callback.
/// Duplicate positions use the first cell, as in [`roles`].
///
/// # Panics
///
/// In overflow-checked builds, neighboring-coordinate arithmetic can panic if a
/// solid cell is at an `i32` boundary. Supply observer-relative coordinates with
/// room for a one-cell offset on every axis; this is not an arbitrary-coordinate
/// validation API.
///
/// ```
/// # use tor_protocol::{CellView, Position};
/// # fn cell(x: i32, y: i32, z: i32, wall: bool, material: &str) -> CellView {
/// #     CellView {
/// #         position: Position { x, y, z }, wall, material: material.into(),
/// #         key: format!("{x},{y},{z}"), lit: true, asset: None, door: None,
/// #         trap: None, object_presentation: None, stairs_up: false,
/// #         stairs_down: false, place_hint: false,
/// #     }
/// # }
/// use tor_client_common::surfaces::roles_by;
/// let cells = vec![
///     cell(0, 0, 0, true, "stone"),
///     cell(0, 0, 1, false, ""),
///     cell(1, 0, 0, false, ""),
/// ];
/// let surfaces = roles_by(&cells, |_| "rough rock");
/// assert!(surfaces.floors.contains("rough rock"));
/// assert!(surfaces.walls.contains("rough rock"));
/// assert!(surfaces.ceilings.is_empty());
/// ```
pub fn roles_by<'a>(cells: &'a [CellView], name: impl Fn(&'a CellView) -> &'a str) -> Roles<'a> {
    let map = index(cells);
    let mut roles = Roles::default();
    for (&(x, y, z), cell) in &map {
        if !cell.wall {
            continue;
        }
        if open(&map, (x, y, z + 1)) {
            roles.floors.insert(name(cell));
        }
        if open(&map, (x, y, z - 1)) {
            roles.ceilings.insert(name(cell));
        }
        if [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .into_iter()
            .any(|(dx, dy)| open(&map, (x + dx, y + dy, z)))
        {
            roles.walls.insert(name(cell));
        }
    }
    roles
}

/// Return the first supplied solid cell exactly one cell below `at`.
///
/// The position is in the same observer-relative frame as `cells`. `None` means
/// no supplied solid cell matches; it does not prove there is no floor. Unlike
/// [`roles`], this scans for a matching solid cell rather than deduplicating all
/// positions first. Neither `at` nor intervening open cells need be supplied.
///
/// # Panics
///
/// In overflow-checked builds, subtracting one from `at.z == i32::MIN` panics
/// when a solid cell is examined. Supply a position with room for that offset.
///
/// ```
/// # use tor_protocol::{CellView, Position};
/// # fn cell(x: i32, y: i32, z: i32, wall: bool, material: &str) -> CellView {
/// #     CellView {
/// #         position: Position { x, y, z }, wall, material: material.into(),
/// #         key: format!("{x},{y},{z}"), lit: true, asset: None, door: None,
/// #         trap: None, object_presentation: None, stairs_up: false,
/// #         stairs_down: false, place_hint: false,
/// #     }
/// # }
/// use tor_client_common::surfaces::floor_below;
/// let cells = vec![cell(0, 0, -1, true, "stone")];
/// let at = Position { x: 0, y: 0, z: 0 };
/// assert_eq!(floor_below(&cells, at).map(|c| c.material.as_str()), Some("stone"));
/// assert!(floor_below(&cells, Position { x: 1, ..at }).is_none());
/// ```
pub fn floor_below(cells: &[CellView], at: Position) -> Option<&CellView> {
    cells
        .iter()
        .find(|c| c.wall && c.position == Position { z: at.z - 1, ..at })
}

/// Find the nearest disclosed solid cell one to sixteen cells above `at`.
///
/// Returns the borrowed cell and its positive vertical distance in cells. Every
/// intervening position must be supplied and open; the first missing position
/// returns `None`, as does reaching the search limit without a solid cell.
/// `at` itself need not be supplied. Duplicate positions use the first cell.
/// This bounded absence of evidence is not proof that no ceiling exists.
///
/// # Panics
///
/// In overflow-checked builds, adding a searched distance to `at.z` panics if it
/// exceeds `i32::MAX`. Supply a position with room for the full 16-cell search.
///
/// ```
/// # use tor_protocol::{CellView, Position};
/// # fn cell(x: i32, y: i32, z: i32, wall: bool, material: &str) -> CellView {
/// #     CellView {
/// #         position: Position { x, y, z }, wall, material: material.into(),
/// #         key: format!("{x},{y},{z}"), lit: true, asset: None, door: None,
/// #         trap: None, object_presentation: None, stairs_up: false,
/// #         stairs_down: false, place_hint: false,
/// #     }
/// # }
/// use tor_client_common::surfaces::ceiling_above;
/// let mut cells = vec![
///     cell(0, 0, 1, false, ""),
///     cell(0, 0, 2, true, "wood"),
/// ];
/// let at = Position { x: 0, y: 0, z: 0 };
/// let (ceiling, distance) = ceiling_above(&cells, at).unwrap();
/// assert_eq!((ceiling.material.as_str(), distance), ("wood", 2));
/// cells.remove(0); // Do not infer an open cell across an undisclosed gap.
/// assert!(ceiling_above(&cells, at).is_none());
/// // The upper bound is inclusive, but a solid cell beyond it is not examined.
/// let mut tall: Vec<_> = (1..=16).map(|z| cell(0, 0, z, false, "")).collect();
/// tall.push(cell(0, 0, 17, true, "stone"));
/// assert!(ceiling_above(&tall, at).is_none());
/// tall[15] = cell(0, 0, 16, true, "stone");
/// assert_eq!(ceiling_above(&tall, at).map(|(_, distance)| distance), Some(16));
/// ```
pub fn ceiling_above(cells: &[CellView], at: Position) -> Option<(&CellView, u32)> {
    let map = index(cells);
    for distance in 1..=CEILING_SEARCH {
        let cell = map.get(&(at.x, at.y, at.z + distance))?;
        if cell.wall {
            return Some((cell, distance as u32));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(x: i32, y: i32, z: i32, wall: bool, material: &str) -> CellView {
        CellView {
            object_presentation: None,
            lit: true,
            asset: None,
            trap: None,
            door: None,
            material: material.into(),
            key: format!("{x},{y},{z}"),
            stairs_up: false,
            stairs_down: false,
            position: Position { x, y, z },
            wall,
            place_hint: false,
        }
    }

    /// A column: stone floor, two open cells, a wooden ceiling; and a marble
    /// wall beside the lower open cell.
    fn room() -> Vec<CellView> {
        vec![
            cell(0, 0, -1, true, "stone"),
            cell(0, 0, 0, false, ""),
            cell(0, 0, 1, false, ""),
            cell(0, 0, 2, true, "wood"),
            cell(1, 0, 0, true, "marble"),
        ]
    }

    #[test]
    fn solid_cells_take_roles_from_their_open_neighbours() {
        let cells = room();
        let roles = roles(&cells);
        assert_eq!(roles.floors, BTreeSet::from(["stone"]));
        assert_eq!(roles.ceilings, BTreeSet::from(["wood"]));
        assert_eq!(roles.walls, BTreeSet::from(["marble"]));
        let origin = Position { x: 0, y: 0, z: 0 };
        assert_eq!(
            floor_below(&cells, origin).map(|c| c.material.as_str()),
            Some("stone")
        );
        let (ceiling, distance) = ceiling_above(&cells, origin).unwrap();
        assert_eq!((ceiling.material.as_str(), distance), ("wood", 2));
    }

    #[test]
    fn an_unseen_gap_hides_the_ceiling_and_a_missing_floor_is_none() {
        let mut cells = room();
        cells.retain(|c| c.position.z != 1);
        let origin = Position { x: 0, y: 0, z: 0 };
        assert!(ceiling_above(&cells, origin).is_none());
        cells.retain(|c| c.position.z != -1);
        assert!(floor_below(&cells, origin).is_none());
        assert!(roles(&cells).floors.is_empty());
    }

    #[test]
    fn roles_can_name_cells_by_something_other_than_their_material() {
        let cells = room();
        let roles = roles_by(
            &cells,
            |c| if c.material == "marble" { "glass" } else { "x" },
        );
        assert_eq!(roles.walls, BTreeSet::from(["glass"]));
        assert_eq!(roles.floors, BTreeSet::from(["x"]));
    }
}
