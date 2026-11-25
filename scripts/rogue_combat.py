"""Rogue combat decisions and evidence used to interpret encounters."""

from __future__ import annotations

from collections import Counter
from collections.abc import Iterable
from dataclasses import dataclass, field
from enum import StrEnum
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from tor_client_bot import Creature, Device, Health, IncomingAbility, OutgoingAbility, Shot, Turn, UseAbility


SELF_DAMAGE_ABILITIES = {"rogue.wand_fire", "rogue.wand_cold", "rogue.wand_lightning", "rogue.breath_weapon"}


def below_half_health(health: Health | None) -> bool:
    return health is not None and health.current * 2 < health.maximum


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


class PolymorphState(StrEnum):
    ATTEMPTED = "attempted"
    RESISTED = "resisted"
    RETRIED = "retried"


@dataclass
class PolymorphHistory:
    """Track the cast and single permitted retry separately for each opponent."""

    by_target: dict[str, PolymorphState] = field(default_factory=dict)

    def admitted(self, targets: Iterable[str]) -> None:
        for target in targets:
            if self.by_target.get(target) is PolymorphState.RESISTED:
                self.by_target[target] = PolymorphState.RETRIED
            else:
                self.by_target.setdefault(target, PolymorphState.ATTEMPTED)

    def observe(self, outgoing: tuple[OutgoingAbility, ...]) -> None:
        from tor_client_bot import AbilityOutcome

        for effect in outgoing:
            if effect.ability != "rogue.wand_polymorph" or effect.outcome is not AbilityOutcome.UNAFFECTED:
                continue
            target = effect.target
            if target is not None and self.by_target.get(target) is PolymorphState.ATTEMPTED:
                self.by_target[target] = PolymorphState.RESISTED

    def permits_cast(self, turn: Turn, foe: Creature, device: Device, lethal: bool) -> bool:
        from tor_client_bot import Injury

        if not lethal or foe.injury is not Injury.HEALTHY:
            return False
        state = self.by_target.get(foe.target)
        if state is None:
            return True
        if state is not PolymorphState.RESISTED:
            return False
        # A witnessed resistance permits one retry with a measured charge and no other foe in contact.
        return device.charges is not None and not any(
            other.target != foe.target and turn.in_contact(other) for other in turn.hostiles
        )

@dataclass
class MeleeExposure:
    """Visible foes in reach of a square and their combined estimated damage."""

    foes: list[Creature]
    damage: int

    def threatens(self, health: Health | None, *, rounds: int) -> bool:
        return health is not None and self.damage * rounds >= health.current

    def needs_healing(self, health: Health | None) -> bool:
        """Heal below half health, below three quarters when surrounded, or before a predicted lethal hit."""
        if health is None:
            return False
        surrounded = len(self.foes) >= 2
        return (below_half_health(health) or self.threatens(health, rounds=1)
                or (surrounded and health.current * 4 < health.maximum * 3))


@dataclass
class OpponentHistory:
    """Highest health loss attributed to each attacker and observed draining effects."""

    damage_by_target: Counter[str] = field(default_factory=Counter)
    damage_by_name: Counter[str] = field(default_factory=Counter)
    draining_names: set[str] = field(default_factory=set)

    def record_injury(self, hostiles: tuple[Creature, ...], incoming: tuple[IncomingAbility, ...],
                      health_loss: int, *, lasting_injury: bool) -> None:
        if health_loss <= 0 and not lasting_injury:
            return
        source = sole_applied_attacker(incoming)
        if source is None:
            return
        if health_loss > 0:
            self.damage_by_target[source] = max(health_loss, self.damage_by_target[source])
        for foe in hostiles:
            if foe.target == source:
                if health_loss > 0:
                    self.damage_by_name[foe.name] = max(health_loss, self.damage_by_name[foe.name])
                if lasting_injury:
                    self.draining_names.add(foe.name)
                return

    def damage_estimate(self, foe: Creature) -> int:
        return max(self.damage_by_target[foe.target], self.damage_by_name[foe.name])

    def melee_exposure(self, turn: Turn, offset: tuple[int, int, int] = (0, 0, 0)) -> MeleeExposure:
        foes = [foe for foe in turn.hostiles if turn.in_contact(foe, from_offset=offset)]
        return MeleeExposure(foes, sum(self.damage_estimate(foe) for foe in foes))

    def nearby_drainer(self, turn: Turn, foe: Creature) -> bool:
        # Allow one closing step before a creature can strike in contact.
        return foe.name in self.draining_names and turn.distance_to(foe) <= 2


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


def choose_melee_attack(turn: Turn, targets: Iterable[Creature], *, make_space: bool) -> UseAbility | None:
    """Try each target in priority order, using a push only when it can create space."""
    from tor_client_bot import Injury

    for foe in targets:
        if make_space and foe.injury is not Injury.NEAR_DEATH:
            push = turn.attack(foe, technique="dislocating_strike")
            if push and has_push_space(turn, foe):
                return push
        attack = turn.attack(foe, technique="rattling_blow") or turn.attack(foe)
        if attack:
            return attack
    return None


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
