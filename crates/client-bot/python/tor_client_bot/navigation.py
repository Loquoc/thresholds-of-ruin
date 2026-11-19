"""Remember observed horizontal links by stable cell identity across turns."""

from __future__ import annotations

from collections import deque
from collections.abc import Iterator, Set
from dataclasses import dataclass

from .game import Turn
from .models import HORIZONTAL, Direction, Move, OpenDoor


@dataclass(frozen=True, slots=True)
class RememberedCell:
    """Terrain last observed at a stable key. It carries no live action targets."""

    key: str
    wall: bool
    stairs_up: bool
    stairs_down: bool
    trapped: bool


@dataclass(frozen=True, slots=True)
class RememberedItem:
    """Last-seen item information without a live interaction target."""

    name: str
    identity: str | None
    category: str
    quantity: int


CARDINAL = (Direction.WEST, Direction.EAST, Direction.NORTH, Direction.SOUTH)


class FloorMap:
    """Memory for one floor. The policy decides when it has changed floors."""

    def __init__(self) -> None:
        self.cells: dict[str, RememberedCell] = {}
        self.visited: set[str] = set()
        self.items: dict[str, tuple[RememberedItem, ...]] = {}
        self._neighbors: dict[str, dict[Direction, str]] = {}
        self._traps: set[str] = set()
        self._links: dict[str, dict[Direction, str]] = {}

    def observe(self, turn: Turn) -> None:
        if turn.cell:
            self.visited.add(turn.cell.key)
        plane = {cell.offset[:2]: cell for cell in turn.visible_cells if cell.offset[2] == 0}
        sightings: dict[str, list[RememberedItem]] = {}
        for ground in turn.ground_items:
            cell = ground.cell
            if cell and cell.offset[2] == 0:
                item = ground.item
                sightings.setdefault(cell.key, []).append(
                    RememberedItem(item.name, item.identity, item.category, item.quantity)
                )
        for (x, y), cell in plane.items():
            if cell.key in sightings:
                self.items[cell.key] = tuple(sightings[cell.key])
            else:
                self.items.pop(cell.key, None)
            if cell.trap:
                self._traps.add(cell.key)
            else:
                self._traps.discard(cell.key)
            self.cells[cell.key] = RememberedCell(
                cell.key, cell.wall, cell.stairs_up, cell.stairs_down, cell.trap is not None
            )
            links = self._links.setdefault(cell.key, {})
            neighbors = self._neighbors.setdefault(cell.key, {})
            for direction in (
                *CARDINAL,
                Direction.NORTH_EAST,
                Direction.SOUTH_EAST,
                Direction.SOUTH_WEST,
                Direction.NORTH_WEST,
            ):
                dx, dy = HORIZONTAL[direction]
                neighbor = plane.get((x + dx, y + dy))
                if neighbor is None:
                    continue
                if direction in CARDINAL:
                    neighbors[direction] = neighbor.key
                if dx and dy:
                    corners = (plane.get((x + dx, y)), plane.get((x, y + dy)))
                    if any(c is None or c.wall or (c.door and not c.door.open) for c in corners):
                        links.pop(direction, None)
                        continue
                if cell.wall or neighbor.wall:
                    links.pop(direction, None)
                else:
                    links[direction] = neighbor.key

    @property
    def frontier(self) -> set[str]:
        """Unvisited floor cells bordering terrain the player has yet to observe."""
        return {
            key
            for key, sides in self._neighbors.items()
            if len(sides) < 4 and not self.cells[key].wall and key not in self.visited
        }

    def adjacent(self, key: str) -> tuple[RememberedCell, ...]:
        """Known cardinal neighbors, including walls."""
        return tuple(self.cells[n] for n in self._neighbors.get(key, {}).values())

    def is_dead_end(self, key: str) -> bool:
        """Whether a mapped floor cell has one passage and three known cardinal walls."""
        cell = self.cells.get(key)
        sides = self._neighbors.get(key)
        if cell is None or cell.wall or sides is None or len(sides) != 4:
            return False
        return sum(self.cells[neighbor].wall for neighbor in sides.values()) == 3

    def reachable(self, start: str) -> Iterator[tuple[RememberedCell, int]]:
        """Traverse known floor cells in order of distance, ignoring transient occupants."""
        if start not in self.cells or self.cells[start].wall:
            return
        seen = {start}
        queue = deque([(start, 0)])
        while queue:
            key, distance = queue.popleft()
            yield self.cells[key], distance
            for neighbor in self._links.get(key, {}).values():
                if neighbor not in seen:
                    seen.add(neighbor)
                    queue.append((neighbor, distance + 1))

    def distance(self, start: str, goals: set[str]) -> int | None:
        """Fewest known horizontal steps to a goal, ignoring transient occupants."""
        return next((distance for cell, distance in self.reachable(start) if cell.key in goals), None)

    def toward_unexplored(self, turn: Turn, *, avoid: Set[str] = frozenset()) -> Move | OpenDoor | None:
        """Head for a frontier, then any unvisited floor. Excluded cells stay off the route."""
        if step := self.toward(turn, self.frontier, avoid=avoid):
            return step
        unvisited = {key for key, cell in self.cells.items() if not cell.wall and key not in self.visited}
        return self.toward(turn, unvisited, avoid=avoid)

    def toward(self, turn: Turn, goals: set[str], *, prefer_safe: bool = True,
               avoid: Set[str] = frozenset(), max_steps: int | None = None) -> Move | OpenDoor | None:
        """Choose a first step on a known route within the optional horizontal step limit."""
        if prefer_safe and self._traps:
            step = self._route(turn, goals, self._traps | avoid, max_steps)
            if step is not None:
                return step
        return self._route(turn, goals, avoid, max_steps)

    def _route(self, turn: Turn, goals: set[str], excluded: Set[str], max_steps: int | None) -> Move | OpenDoor | None:
        here = turn.cell
        if here is None or here.key in goals or not goals or (max_steps is not None and max_steps < 1):
            return None
        seen = {here.key, *excluded}
        queue: deque[tuple[str, Move | OpenDoor, int]] = deque()
        for direction, neighbor in self._links.get(here.key, {}).items():
            first = turn.step(direction)
            if first is not None and neighbor not in seen:
                queue.append((neighbor, first, 1))
                seen.add(neighbor)
        while queue:
            key, first, distance = queue.popleft()
            if key in goals:
                return first
            if max_steps is not None and distance >= max_steps:
                continue
            for neighbor in self._links.get(key, {}).values():
                if neighbor not in seen:
                    seen.add(neighbor)
                    queue.append((neighbor, first, distance + 1))
        return None
