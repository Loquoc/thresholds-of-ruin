"""Immutable player observations, action choices and submission results."""

from __future__ import annotations

from collections.abc import Mapping
from typing import Any
from dataclasses import dataclass, fields
from enum import Enum


class Direction(str, Enum):
    NORTH = "north"
    EAST = "east"
    SOUTH = "south"
    WEST = "west"
    NORTH_EAST = "north_east"
    SOUTH_EAST = "south_east"
    SOUTH_WEST = "south_west"
    NORTH_WEST = "north_west"
    UP = "up"
    DOWN = "down"

    @property
    def offset(self) -> tuple[int, int, int]:
        if self is Direction.UP:
            return (0, 0, 1)
        if self is Direction.DOWN:
            return (0, 0, -1)
        x, y = HORIZONTAL[self]
        return (x, y, 0)


# Horizontal neighbors in the observer's coordinate frame.
HORIZONTAL = {
    Direction.NORTH: (0, -1),
    Direction.EAST: (1, 0),
    Direction.SOUTH: (0, 1),
    Direction.WEST: (-1, 0),
    Direction.NORTH_EAST: (1, -1),
    Direction.SOUTH_EAST: (1, 1),
    Direction.SOUTH_WEST: (-1, 1),
    Direction.NORTH_WEST: (-1, -1),
}


@dataclass(frozen=True, slots=True)
class Move:
    direction: Direction


@dataclass(frozen=True, slots=True)
class Wait:
    """Spend a turn waiting."""


@dataclass(frozen=True, slots=True)
class Health:
    current: int
    maximum: int


@dataclass(frozen=True, slots=True)
class RestorableLosses:
    """Disclosed amounts the actor could recover through restoration."""

    strength: int
    speed: int
    intellect: int
    willpower: int
    awareness: int
    presence: int
    hit_dice: int
    maximum_health: int

    def increased_since(self, previous: RestorableLosses) -> bool:
        return any(getattr(self, f.name) > getattr(previous, f.name) for f in fields(self))


@dataclass(frozen=True, slots=True)
class Door:
    target: str
    open: bool
    reachable: bool


@dataclass(frozen=True, slots=True)
class OpenDoor:
    door: Door


@dataclass(frozen=True, slots=True)
class Trap:
    target: str
    family: str | None
    reachable: bool


@dataclass(frozen=True, slots=True)
class Cell:
    key: str
    offset: tuple[int, int, int]
    wall: bool
    stairs_up: bool
    stairs_down: bool
    door: Door | None
    trap: Trap | None = None


class Injury(str, Enum):
    HEALTHY = "healthy"
    WOUNDED = "wounded"
    BADLY_WOUNDED = "badly_wounded"
    NEAR_DEATH = "near_death"


@dataclass(frozen=True, slots=True)
class Creature:
    """One visible body, even when it occupies several disclosed cells."""

    target: str
    name: str
    cells: tuple[tuple[int, int, int], ...]
    hostile: bool | None
    allied: bool | None
    injury: Injury | None

    @property
    def distance(self) -> int:
        """Chebyshev distance from the observer's origin to the closest body cell."""
        return min(max(map(abs, cell)) for cell in self.cells)


class ItemUse(str, Enum):
    EAT = "eat"
    DRINK = "drink"
    READ = "read"


@dataclass(frozen=True, slots=True)
class Item:
    target: str
    name: str
    identity: str | None
    quantity: int
    uses: tuple[ItemUse, ...]
    category: str


@dataclass(frozen=True, slots=True)
class UseItem:
    item: Item
    use: ItemUse

    def __post_init__(self) -> None:
        if self.use not in self.item.uses:
            raise ValueError(f"{self.item.name} does not offer {self.use.value}")


@dataclass(frozen=True, slots=True)
class Pickup:
    item: Item


@dataclass(frozen=True, slots=True)
class EquipmentSlot:
    index: int
    kind: str
    name: str
    occupant: Item | None


@dataclass(frozen=True, slots=True)
class Equip:
    item: Item
    slot: EquipmentSlot


@dataclass(frozen=True, slots=True)
class Unequip:
    slot: EquipmentSlot


@dataclass(frozen=True, slots=True)
class GroundItem:
    item: Item
    offset: tuple[int, int, int]
    reachable: bool


@dataclass(frozen=True, slots=True)
class Identify:
    choice: str
    item: Item | None


@dataclass(frozen=True, slots=True)
class Identification:
    choice: str
    items: tuple[Item, ...]

    def select(self, item: Item | None) -> Identify:
        if item is not None and item not in self.items:
            raise ValueError("Choose an offered item")
        return Identify(self.choice, item)


@dataclass(frozen=True, slots=True)
class Ability:
    identity: str
    name: str
    basic_attack: bool
    target_kind: str
    reach: str
    range: int | None
    ready: bool


@dataclass(frozen=True, slots=True)
class UseAbility:
    ability: Ability
    target: Creature | None = None


@dataclass(frozen=True, slots=True)
class ProjectileOption:
    """A throwable item or a compatible projectile and equipped launcher."""

    ability: Ability
    projectile: Item
    launcher: Item | None
    socket: int
    equipped_throw: bool = False


@dataclass(frozen=True, slots=True)
class Shot:
    ability: Ability
    target: Creature
    offset: tuple[int, int, int]
    projectile: Item
    launcher: Item | None
    socket: int
    equipped_throw: bool = False


class ActivityChoice(Enum):
    RESUME = "resume"
    CANCEL = "cancel"


@dataclass(frozen=True, slots=True)
class Recovery:
    period_ticks: int
    elapsed_ticks: int
    stopped: bool
    reset_percent: int


@dataclass(frozen=True, slots=True)
class Condition:
    """A disclosed condition on the player, including its protection period."""

    identity: str
    name: str
    active: bool
    remaining_ticks: int | None


@dataclass(frozen=True, slots=True)
class Nutrition:
    hunger: str
    reserve: int
    maximum: int
    ticks_per_unit: int
    elapsed_ticks: int

    @property
    def ticks_until_empty(self) -> int:
        return max(0, self.reserve * self.ticks_per_unit - self.elapsed_ticks)


@dataclass(frozen=True, slots=True)
class Hold:
    target: str
    holder: Creature | None


@dataclass(frozen=True, slots=True)
class Escape:
    hold: Hold


class ChartKind(str, Enum):
    MAPPING = "mapping"
    MONSTERS = "monsters"
    FOOD = "food"
    MAGIC = "magic"


@dataclass(frozen=True, slots=True)
class ChartMark:
    component: int
    position: tuple[int, int, int] | None
    feature: str
    normal: tuple[int, int, int] | None
    name: str | None
    asset: str | None


@dataclass(frozen=True, slots=True)
class ChartComponent:
    number: int
    alignment: str


@dataclass(frozen=True, slots=True)
class Chart:
    kind: ChartKind
    mode: str
    acquired_at: int
    components: tuple[ChartComponent, ...]
    marks: tuple[ChartMark, ...]


class AbilityOutcome(str, Enum):
    APPLIED = "applied"
    UNAFFECTED = "unaffected"
    MISS = "miss"


@dataclass(frozen=True, slots=True)
class IncomingAbility:
    caster: str | None
    ability: str | None
    outcome: AbilityOutcome
    unseen: bool
    strike: int | None


class CombatReport:
    """Combat witnessed by the player over one observation interval."""

    def __init__(self, observer: str | None, events: tuple[dict, ...]):
        self._observer = observer
        self._events = events

    @property
    def incoming(self) -> tuple[IncomingAbility, ...]:
        """Other actors' ability uses directed at this observer, in observation order."""
        return tuple(
            IncomingAbility(
                event.get("caster"),
                (event.get("ability") or {}).get("id"),
                AbilityOutcome(event["outcome"]),
                event["type"] == "unseen_ability",
                event.get("strike"),
            )
            for event in self._events
            if event["type"] in {"ability", "unseen_ability", "anonymous_ability"}
            and self._observer is not None
            and event.get("target") == self._observer
            and event.get("caster") != self._observer
        )

    def records(self) -> list[dict]:
        """Original disclosed events for lossless trace output."""
        from copy import deepcopy

        return deepcopy(list(self._events))


class ActivityPhase(str, Enum):
    QUEUED = "queued"
    SUSPENDED = "suspended"
    PAUSED = "paused"
    STARTED = "started"
    RESOLVED = "resolved"
    FAILED = "failed"
    CANCELLED = "cancelled"


@dataclass(frozen=True, slots=True)
class Activity:
    """One observation of a gameplay action admitted by the server."""

    actor: int
    branch: str
    intention: str
    phase: ActivityPhase

    @classmethod
    def from_row(cls, row: dict) -> Activity:
        return cls(row["actor"], row["branch"], row["intention"], ActivityPhase(row["phase"]))


class Outcome(str, Enum):
    CONFIRMED = "confirmed"
    REJECTED = "rejected"
    STALE = "stale_observation"
    UNAVAILABLE = "intention_unavailable"
    UNKNOWN = "unknown_outcome"


@dataclass(frozen=True, slots=True)
class Result:
    status: Outcome
    intention: str | None
    reason: str | None
    activity: Activity | None = None
    code: str | None = None

    @classmethod
    def from_reply(cls, reply: Mapping[str, Any]) -> Result:
        receipt = reply.get("receipt", {})
        activity = Activity.from_row(receipt) if receipt.get("type") == "admitted" else None
        return cls(
            Outcome(reply["status"]),
            receipt.get("intention"),
            reply.get("message"),
            activity,
            reply.get("code"),
        )


Action = (
    Move
    | Escape
    | Equip
    | Unequip
    | OpenDoor
    | Wait
    | UseItem
    | Pickup
    | Identify
    | ActivityChoice
    | UseAbility
    | Shot
)
