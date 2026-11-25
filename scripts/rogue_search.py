"""Map geometry and observations used when searching for concealed passages."""

from collections import Counter
from dataclasses import dataclass, field
from typing import NamedTuple

from tor_client_bot import ActivityPhase, Cell, FloorMap, RememberedCell, Result, Turn


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


@dataclass
class SearchHistory:
    """Account for admitted searches and the walls covered by completed actions."""

    admissions: Counter[str] = field(default_factory=Counter)
    unsuccessful_searches: Counter[str] = field(default_factory=Counter)
    completed_end_searches: Counter[str] = field(default_factory=Counter)
    pending_search: PendingSearch | None = None
    wall_searches: Counter[str] = field(default_factory=Counter)

    def observe(self, turn: Turn) -> None:
        search = self.pending_search
        if search is None:
            return
        phase = turn.outcome_of(search.receipt)
        if phase is ActivityPhase.RESOLVED:
            self.wall_searches.update(search.walls)
            if search.at_dead_end:
                self.completed_end_searches[search.origin] += 1
        elif phase in (ActivityPhase.FAILED, ActivityPhase.CANCELLED):
            self.unsuccessful_searches[search.origin] += 1
        else:
            return
        self.pending_search = None

    def admit(self, turn: Turn, origin: Cell, receipt: Result | None, memory: FloorMap | None) -> None:
        self.admissions[origin.key] += 1
        walls = nearby_search_walls(turn)
        if receipt is None or receipt.activity is None:
            self.wall_searches.update(walls)
            return
        at_dead_end = memory is not None and is_searchable_dead_end(origin, memory)
        self.pending_search = PendingSearch(receipt, walls, origin.key, at_dead_end)

    def attempts_at(self, key: str) -> int:
        """Count admitted searches toward the site limit unless the game reports failure or cancellation."""
        return self.admissions[key] - self.unsuccessful_searches[key]

    def remaining_dead_end_searches(self, site: Cell | RememberedCell, memory: FloorMap) -> int | None:
        """Return the unused completed-search allowance, or None for an ordinary search site."""
        if not is_searchable_dead_end(site, memory):
            return None
        return max(0, DEAD_END_SEARCHES - self.completed_end_searches[site.key])
