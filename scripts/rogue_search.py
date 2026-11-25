"""Map geometry and observations used when searching for concealed passages."""

from dataclasses import dataclass
from typing import NamedTuple

from tor_client_bot import Cell, FloorMap, RememberedCell, Result, Turn


ORDINARY_WALL_SEARCHES = 3
DEAD_END_SEARCHES = 6


def nearby_search_walls(turn: Turn) -> frozenset[str]:
    """Credit each disclosed wall on this floor once in the near and middle discovery bands."""
    return frozenset(cell.key for cell in turn.visible_cells
                     if cell.wall and cell.offset[2] == 0 and max(map(abs, cell.offset)) <= 3)


def is_searchable_dead_end(site: Cell | RememberedCell, memory: FloorMap) -> bool:
    return not (site.stairs_up or site.stairs_down) and memory.is_dead_end(site.key)


def squares_across_stair_walls(memory: FloorMap, origin: str, upstairs: set[str]) -> set[RememberedCell]:
    """Find floor squares one wall away from a disconnected upstairs passage."""
    candidates: set[RememberedCell] = set()
    remaining_stairs = upstairs.copy()
    while remaining_stairs:
        passage = {cell.key for cell, _ in memory.reachable(remaining_stairs.pop())}
        remaining_stairs.difference_update(passage)
        if origin in passage:
            continue
        walls = {cell.key for key in passage for cell in memory.adjacent(key) if cell.wall}
        for wall in walls:
            candidates.update(cell for cell in memory.adjacent(wall) if not cell.wall and cell.key not in passage)
    return candidates


class SearchRank(NamedTuple):
    """Higher fields win, in declaration order. Tied sites share a routing query."""

    near_return_stairs: bool
    has_searchable_walls: bool
    value_after_travel: int
    negative_distance: int


@dataclass(frozen=True)
class PendingSearch:
    receipt: Result
    walls: frozenset[str]
    origin: str
    at_dead_end: bool
