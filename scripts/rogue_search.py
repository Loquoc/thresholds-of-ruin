"""Map geometry and observations used when searching for concealed passages."""

from collections import Counter
from collections.abc import Iterator
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

    def return_connection_sites(self, memory: FloorMap, origin: str, upstairs: set[str]) -> set[str]:
        """Find unexhausted search squares across a wall from a disconnected stair passage."""
        sites: set[str] = set()
        for cell in squares_across_stair_walls(memory, origin, upstairs):
            remaining = self.remaining_dead_end_searches(cell, memory)
            if remaining is None:
                remaining = ORDINARY_WALL_SEARCHES - self.attempts_at(cell.key)
            if remaining > 0:
                sites.add(cell.key)
        if sites - {origin}:
            return {cell.key for cell, _ in memory.reachable(origin) if cell.key in sites}
        return sites

    def ranked_search_sites(self, turn: Turn, memory: FloorMap, *, returning: bool,
                           allow_unseen: bool, wall_limit: int) -> Iterator[set[str]]:
        sites_by_rank: dict[SearchRank, set[str]] = {}
        for site, distance in memory.reachable(turn.cell.key):
            rank = self.search_site_rank(turn, memory, site, distance, returning=returning,
                                         allow_unseen=allow_unseen, wall_limit=wall_limit)
            if rank is None:
                continue
            sites_by_rank.setdefault(rank, set()).add(site.key)
        # Route to tied destinations together so a clear path wins over a trapped one.
        for rank in sorted(sites_by_rank, reverse=True):
            yield sites_by_rank[rank]

    def search_site_rank(self, turn: Turn, memory: FloorMap, site: RememberedCell, distance: int, *,
                         returning: bool, allow_unseen: bool, wall_limit: int) -> SearchRank | None:
        neighbors = memory.adjacent(site.key)
        can_search_unseen = allow_unseen and len(neighbors) < 4
        wall_keys = {cell.key for cell in neighbors if cell.wall}
        remaining = self.remaining_dead_end_searches(site, memory)
        if remaining is not None:
            if remaining == 0 or turn.hostiles:
                return None
            untested = wall_keys
            value = remaining * len(untested)
        else:
            attempts = self.attempts_at(site.key)
            # A missing cardinal neighbor allows one final search after the wall budget runs out.
            site_limit = wall_limit + int(can_search_unseen)
            if attempts >= site_limit:
                return None
            untested = set()
            if attempts < wall_limit:
                untested = {key for key in wall_keys if self.wall_searches[key] < wall_limit}
            value = sum(wall_limit - self.wall_searches[key] for key in untested)
        if not untested and not can_search_unseen:
            return None
        near_exit = returning and any(
            neighbor.stairs_up for key in untested for neighbor in memory.adjacent(key)
        )
        # One search checks several boundaries. Favor corridor ends, especially near a lost exit.
        return SearchRank(
            near_return_stairs=near_exit,
            has_searchable_walls=bool(untested),
            value_after_travel=value - distance * 2,
            negative_distance=-distance,
        )
