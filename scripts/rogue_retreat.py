"""Choose retreat steps from visible terrain and pursuer positions."""

from tor_client_bot import Creature, Move, Turn


def evade_unseen_attacker(turn: Turn, attack_origin: str) -> Move | None:
    trapped_step = None
    for direction in turn.walkable_directions:
        cell = turn.cell_at(direction.offset)
        if cell is None or cell.key == attack_origin:
            continue
        if cell.trap is None:
            return Move(direction)
        if trapped_step is None:
            trapped_step = Move(direction)
    return trapped_step


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
        flanks = [turn.cell_at((dx + sign * dy, dy + sign * dx, 0)) for sign in (-1, 1)]
        if not all(c is not None and c.wall for c in flanks):
            continue
        previous_flanks = [turn.cell_at((sign * dy, sign * dx, 0)) for sign in (-1, 1)]
        if not all(c is not None and c.wall for c in previous_flanks):
            return Move(direction)
    return None


def approach_visible_exit(turn: Turn, exits: set[str], avoid: set[str]) -> Move | None:
    nearby = [cell for cell in turn.visible_cells if cell.key in exits and cell.offset[2] == 0]
    if not nearby:
        return None
    def distance(offset, goal):
        # A diagonal covers both axes in one move. Prefer a straighter route on a tie.
        dx, dy = abs(goal[0] - offset[0]), abs(goal[1] - offset[1])
        return max(dx, dy), dx + dy

    origin = (0, 0, 0)
    destination = min(nearby, key=lambda cell: (cell.trap is not None, distance(origin, cell.offset)))
    current = distance(origin, destination.offset)
    approaches = []
    for direction in turn.walkable_directions:
        offset = direction.offset
        candidate = turn.cell_at(offset)
        if candidate is None or candidate.key in avoid:
            continue
        remaining = distance(offset, destination.offset)
        if remaining < current:
            clearance = min(turn.distance_to(foe, from_offset=offset) for foe in turn.hostiles)
            # With equal clearance, avoid a known trap before favoring a shorter approach.
            steps, axis_distance = remaining
            approaches.append((clearance, candidate.trap is None, -steps, -axis_distance, direction))
    if not approaches:
        return None
    *_, direction = max(approaches)
    return Move(direction)
