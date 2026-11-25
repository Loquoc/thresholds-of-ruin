"""Rogue combat decisions and evidence used to interpret encounters."""

from __future__ import annotations

from collections.abc import Iterable
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from tor_client_bot import Creature, Health, IncomingAbility, OutgoingAbility, Shot, Turn


SELF_DAMAGE_ABILITIES = {"rogue.wand_fire", "rogue.wand_cold", "rogue.wand_lightning", "rogue.breath_weapon"}


def recovery_deficit(health: Health) -> int:
    """Hit points needed to reach four fifths of maximum health, rounded up."""
    target = (4 * health.maximum + 4) // 5
    return max(0, target - health.current)


def includes_self_damage(outgoing: tuple[OutgoingAbility, ...], player: str | None) -> bool:
    """Whether the player's own ability could account for the observed health loss."""
    from tor_client_bot import AbilityOutcome

    return player is not None and any(
        effect.target == player and effect.ability in SELF_DAMAGE_ABILITIES and effect.outcome is AbilityOutcome.APPLIED
        for effect in outgoing
    )


def witnessed_unseen_attack(incoming: tuple[IncomingAbility, ...], hostiles: tuple[Creature, ...],
                            *, injured: bool) -> bool:
    """An unseen miss or hostile caster is direct evidence. Injury alone needs an unambiguous source."""
    from tor_client_bot import AbilityOutcome

    unseen_applied = False
    other_applied = False
    for event in incoming:
        if event.unseen:
            if event.outcome is AbilityOutcome.MISS or any(foe.target == event.caster for foe in hostiles):
                return True
            if event.outcome is AbilityOutcome.APPLIED:
                unseen_applied = True
        elif event.outcome is AbilityOutcome.APPLIED:
            other_applied = True
    return injured and unseen_applied and not other_applied


def sole_applied_attacker(incoming: tuple[IncomingAbility, ...]) -> str | None:
    """Attribute an injury only when every applied hit came from one known source."""
    from tor_client_bot import AbilityOutcome

    sources = {event.caster for event in incoming if event.outcome is AbilityOutcome.APPLIED}
    return sources.pop() if len(sources) == 1 else None


def preceding_attack(events_before_death, actor):
    """Read newest first. Only the dying actor's interruptions can follow the attack."""
    for event in events_before_death:
        kind = event["type"]
        if kind == "interrupted" and event.get("actor") == actor:
            continue
        if (kind not in {"ability", "unseen_ability"} or event.get("target") != actor
                or event.get("outcome") != "applied"):
            return None
        self_inflicted = event.get("caster") == actor
        if kind == "unseen_ability" and not self_inflicted:
            return "unseen_ability"
        ability = (event.get("ability") or {}).get("id")
        if self_inflicted and ability not in SELF_DAMAGE_ABILITIES:
            return None
        return ability or "unidentified_ability"
    return None


def combat_death_cause(history, player):
    """Use the player's death event and its immediately preceding combat evidence."""
    if player is None:
        # Older reports omit the player target. A single victim makes it unambiguous.
        victims = {event["actor"] for event in history if event["type"] == "died"}
        if len(victims) != 1:
            return None
        player = victims.pop()

    events = reversed(history)
    for event in events:
        if event["type"] == "died" and event["actor"] == player:
            cause = event.get("cause")
            if cause == "starvation":
                return cause
            return preceding_attack(events, player) or cause
    return None


def horizontal_push_direction(turn: Turn, foe: Creature) -> tuple[int, int] | None:
    """Return the push direction when every shared cell face points the same way."""
    directions = set()
    for sx, sy, sz in turn.body:
        for x, y, z in foe.cells:
            dx, dy, dz = x - sx, y - sy, z - sz
            if abs(dx) + abs(dy) + abs(dz) == 1:
                directions.add((dx, dy, dz))
    if len(directions) != 1:
        return None
    (dx, dy, dz), = directions
    return (dx, dy) if dz == 0 else None


def has_push_space(turn: Turn, foe: Creature) -> bool:
    """Can the foe's whole body land one cell beyond its current position?"""
    direction = horizontal_push_direction(turn, foe)
    if direction is None:
        return False
    dx, dy = direction
    landing = {(x + dx, y + dy, z) for x, y, z in foe.cells}
    for position in landing:
        cell = turn.cell_at(position)
        if cell is None or cell.wall or (cell.door and not cell.door.open):
            return False
    if not landing.isdisjoint(turn.body):
        return False
    return all(landing.isdisjoint(creature.cells) for creature in turn.creatures if creature.target != foe.target)


def choose_projectile(shots: Iterable[Shot]) -> Shot | None:
    """Prefer launched shots, then short range, preserving offer order on ties."""
    best_shot = None
    best_rank = None
    for shot in shots:
        if not shot.target.hostile:
            continue
        distance = max(map(abs, shot.offset))
        launched = shot.launcher is not None
        if not launched and (shot.equipped_throw or distance > 4):
            continue
        in_normal_range = shot.range is None or distance <= shot.range.short_cells
        if launched and in_normal_range:
            return shot
        rank = launched, in_normal_range
        if best_rank is None or rank > best_rank:
            best_shot, best_rank = shot, rank
    return best_shot
