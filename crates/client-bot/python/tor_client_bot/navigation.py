"""Remember observed horizontal links by stable cell identity across turns."""

from __future__ import annotations

from collections import deque
from collections.abc import Callable, Iterator, Set
from dataclasses import dataclass

from .game import Turn
from .models import HORIZONTAL, Cell, Direction, Move, OpenDoor


@dataclass(frozen=True, slots=True)
class RememberedCell:
    """Terrain last observed at a stable key. It carries no live action targets."""

    key: str
    wall: bool
    stairs_up: bool
    stairs_down: bool
    trapped: bool
    closed_door: bool = False


@dataclass(frozen=True, slots=True)
class RememberedItem:
    """Last-seen item information without a live interaction target."""

    name: str
    identity: str | None
    category: str
    quantity: int


CARDINAL = (Direction.WEST, Direction.EAST, Direction.NORTH, Direction.SOUTH)


@dataclass(frozen=True, slots=True)
class _Link:
    destination: str
    corners: tuple[str, ...] = ()

    def matches_partial_diagonal(self, destination: str, horizontal: Cell | None, vertical: Cell | None) -> bool:
        """A missing corner provides no new evidence about a previously observed diagonal."""
        return (self.destination == destination
                and (horizontal is None or horizontal.key == self.corners[0])
                and (vertical is None or vertical.key == self.corners[1]))


class FloorMap:
    """Memory for one floor. The policy decides when it has changed floors."""

    def __init__(self) -> None:
        self.cells: dict[str, RememberedCell] = {}
        self.visited: set[str] = set()
        self.items: dict[str, tuple[RememberedItem, ...]] = {}
        self._neighbors: dict[str, dict[Direction, str]] = {}
        self._traps: set[str] = set()
        self._links: dict[str, dict[Direction, _Link]] = {}

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
                cell.key, cell.wall, cell.stairs_up, cell.stairs_down, cell.trap is not None,
                cell.door is not None and not cell.door.open,
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
                    # Remember the edge even if a wall currently conceals a door.
                    links[direction] = _Link(neighbor.key)
                    continue
                horizontal = plane.get((x + dx, y))
                vertical = plane.get((x, y + dy))
                if horizontal is not None and vertical is not None:
                    links[direction] = _Link(neighbor.key, (horizontal.key, vertical.key))
                elif direction in links and not links[direction].matches_partial_diagonal(neighbor.key, horizontal, vertical):
                    del links[direction]

    @property
    def frontier(self) -> set[str]:
        """Unvisited floor cells bordering terrain the player has yet to observe."""
        return {
            key
            for key, sides in self._neighbors.items()
            if len(sides) < 4 and not self.cells[key].wall and key not in self.visited
        }

    def cells_with_item(self, matches: Callable[[RememberedItem], bool]) -> set[str]:
        """Keys of cells last seen containing at least one matching item."""
        return {key for key, items in self.items.items() if any(matches(item) for item in items)}

    def stairs(self, direction: Direction) -> set[str]:
        """Cell keys last seen with stairs leading in the given vertical direction."""
        if direction is Direction.UP:
            return {key for key, cell in self.cells.items() if cell.stairs_up}
        if direction is Direction.DOWN:
            return {key for key, cell in self.cells.items() if cell.stairs_down}
        raise ValueError("Stairs lead up or down")

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

    def _passages_from(self, key: str) -> Iterator[tuple[Direction, str]]:
        """Use current terrain for both the destination and any diagonal corners."""
        for direction, link in self._links.get(key, {}).items():
            if self.cells[link.destination].wall:
                continue
            if link.corners and any(self.cells[k].wall or self.cells[k].closed_door for k in link.corners):
                continue
            yield direction, link.destination

    def reachable(self, start: str, *, avoid: Set[str] = frozenset()) -> Iterator[tuple[RememberedCell, int]]:
        """Traverse known floor from start by distance, without entering any excluded cell."""
        if start not in self.cells or self.cells[start].wall:
            return
        seen = {start, *avoid}
        queue = deque([(start, 0)])
        while queue:
            key, distance = queue.popleft()
            yield self.cells[key], distance
            for _, neighbor in self._passages_from(key):
                if neighbor not in seen:
                    seen.add(neighbor)
                    queue.append((neighbor, distance + 1))

    def distance(self, start: str, goals: set[str], *, avoid: Set[str] = frozenset()) -> int | None:
        """Fewest known horizontal steps to a goal without entering an excluded cell."""
        return next((distance for cell, distance in self.reachable(start, avoid=avoid) if cell.key in goals), None)

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
        for direction, neighbor in self._passages_from(here.key):
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
            for _, neighbor in self._passages_from(key):
                if neighbor not in seen:
                    seen.add(neighbor)
                    queue.append((neighbor, first, distance + 1))
        return None
