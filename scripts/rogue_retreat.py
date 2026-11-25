"""Choose retreat steps from visible terrain and pursuer positions."""

from collections.abc import Callable, Set

from tor_client_bot import Action, Creature, Direction, FloorMap, Move, Turn


def route_around_danger(turn: Turn, memory: FloorMap, goals: set[str], danger: Set[str]) -> Action | None:
    """Prefer a route outside recent danger, allowing a dangerous route when it is the only option."""
    if goals and danger:
        if step := memory.toward(turn, goals, avoid=danger):
            return step
    return memory.toward(turn, goals)


def evade_unseen_attacker(turn: Turn, attack_origin: str) -> Move | None:
    origin = next((cell for cell in turn.visible_cells if cell.key == attack_origin), None)
    best_rank = None
    step = None
    for direction in turn.walkable_directions:
        cell = turn.cell_at(direction.offset)
        if cell is None or cell.key == attack_origin:
            continue
        distance = max(abs(a - b) for a, b in zip(cell.offset, origin.offset)) if origin else 0
        rank = cell.trap is None, distance
        if best_rank is None or rank > best_rank:
            best_rank = rank
            step = Move(direction)
    return step


def best_onward_escape(
    turn: Turn,
    first_steps: list[Direction],
    safety: Callable[[tuple[int, int, int]], tuple[int, int]],
) -> Direction | None:
    """Choose a first step whose continuation improves on the starting square."""
    origin = (0, 0, 0)
    starting_safety = safety(origin)
    best_rank = None
    best_direction = None
    for direction in first_steps:
        cell = turn.cell_at(direction.offset)
        first_safety = safety(direction.offset)
        for second in turn.walkable_from(cell):
            destination = tuple(a + b for a, b in zip(direction.offset, second.offset))
            if destination == origin:
                continue
            onward_safety = safety(destination)
            if onward_safety < first_safety or onward_safety <= starting_safety:
                continue
            rank = onward_safety, cell.trap is None, turn.cell_at(destination).trap is None
            if best_rank is None or rank > best_rank:
                best_direction, best_rank = direction, rank
    return best_direction


def _flanked_by_walls(turn: Turn, x: int, y: int, direction: Direction) -> bool:
    """Are both sides of this cardinal passage known walls?"""
    dx, dy, _ = direction.offset
    left = turn.cell_at((x - dy, y + dx, 0))
    right = turn.cell_at((x + dy, y - dx, 0))
    return left is not None and left.wall and right is not None and right.wall


def _closing_squares(turn: Turn, foe: Creature, occupied: set[tuple[int, int, int]]) -> set[Direction]:
    """Open squares next to the player that this foe can reach in one step."""
    columns = {(x, y) for x, y, _ in foe.cells}
    if len(columns) != 1:
        return set()
    fx, fy = next(iter(columns))
    approaches = set()
    for direction in turn.walkable_directions:
        x, y, _ = direction.offset
        if max(abs(fx - x), abs(fy - y)) != 1 or not turn.in_contact(foe, from_offset=direction.offset):
            continue
        # Check the closing step at every visible height, including both diagonal corners.
        crossed = {(x, y)}
        if fx != x and fy != y:
            crossed.update(((fx, y), (x, fy)))
        sweep = ((cx, cy, z) for _, _, z in foe.cells for cx, cy in crossed)
        if all(at not in occupied and (cell := turn.cell_at(at)) and not cell.wall
               and (cell.door is None or cell.door.open) for at in sweep):
            approaches.add(direction)
    return approaches


def converging_pursuers(turn: Turn, distances: list[tuple[Creature, int]]) -> bool:
    """Can two foes enter melee through different open squares in one horizontal step?"""
    close = [foe for foe, distance in distances if distance == 2]
    if len(close) < 2 or any(x or y for x, y, _ in turn.body):
        return False
    occupied = {cell for creature in turn.creatures for cell in creature.cells}
    earlier_squares = set()
    for foe in close:
        squares = _closing_squares(turn, foe, occupied)
        if squares and earlier_squares and len(earlier_squares | squares) > 1:
            return True
        earlier_squares.update(squares)
    return False


def take_corridor(turn: Turn, pursuers: list[tuple[Creature, int]]) -> Move | None:
    """Enter a narrow passage with a clear retreat and the pursuers on the far side."""
    for direction in turn.walkable_directions:
        dx, dy, _ = direction.offset
        if dx and dy:
            continue
        if any(turn.distance_to(foe, from_offset=direction.offset) <= distance for foe, distance in pursuers):
            continue
        entrance = turn.cell_at(direction.offset)
        if entrance is None or entrance.trap or direction not in turn.walkable_from(entrance):
            continue
        retreat = turn.cell_at((2 * dx, 2 * dy, 0))
        if retreat is None or retreat.trap:
            continue
        if _flanked_by_walls(turn, dx, dy, direction) and not _flanked_by_walls(turn, 0, 0, direction):
            return Move(direction)
    return None


def _stair_distance(offset: tuple[int, int, int], goal: tuple[int, int, int]) -> tuple[int, int]:
    """Count diagonal walking steps, breaking ties by total axis distance."""
    dx, dy = abs(goal[0] - offset[0]), abs(goal[1] - offset[1])
    return max(dx, dy), dx + dy


def _preferred_visible_stairs(turn: Turn, exits: set[str]) -> list[tuple[int, int, int]]:
    """Prefer untrapped stairs, then the nearest. Keep ties for the pursuit decision."""
    exits_by_rank = {}
    for cell in turn.visible_cells:
        if cell.key in exits and cell.offset[2] == 0:
            rank = cell.trap is not None, _stair_distance((0, 0, 0), cell.offset)
            exits_by_rank.setdefault(rank, []).append(cell.offset)
    return exits_by_rank[min(exits_by_rank)] if exits_by_rank else []


def approach_visible_exit(turn: Turn, memory: FloorMap, exits: set[str], avoid: set[str]) -> Move | None:
    """Approach the best visible stairs while keeping clear of pursuers."""
    destinations = _preferred_visible_stairs(turn, exits)
    if not destinations:
        return None
    current_distance = _stair_distance((0, 0, 0), destinations[0])

    approaches = []
    for direction in turn.walkable_directions:
        offset = direction.offset
        candidate = turn.cell_at(offset)
        if candidate is None or candidate.key in avoid:
            continue
        if candidate.key not in exits and memory.is_dead_end(candidate.key):
            continue
        remaining = min(_stair_distance(offset, goal) for goal in destinations)
        if remaining < current_distance:
            clearance = min(turn.distance_to(foe, from_offset=offset) for foe in turn.hostiles)
            # With equal clearance, avoid a known trap before favoring a shorter approach.
            steps, axis_distance = remaining
            # Preserve the direction-name tie break for otherwise equivalent moves.
            rank = clearance, candidate.trap is None, -steps, -axis_distance, direction.value
            approaches.append((rank, direction))
    if not approaches:
        return None
    _, direction = max(approaches)
    return Move(direction)
