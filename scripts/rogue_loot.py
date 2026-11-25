"""Choose reachable Rogue loot and route to more distant supplies."""

from collections import Counter
from collections.abc import Set
from dataclasses import dataclass, field
from functools import cache

from tor_client_bot import Action, FloorMap, GroundItem, Item, Turn

from rogue_food import carried_quantity
from rogue_gear import ring_upgrades
from rogue_retreat import route_around_danger


DETOUR_STOCK = {"potion": 4, "scroll": 4, "wand": 2}
LOOT_CLASSES = {"amulet", "armor", "coin", "food", "potion", "ring", "scroll", "tool", "wand", "weapon"}
HEALING_RESERVE_PER_KIND = 2
HEALING_POTIONS = {"item.potion.healing", "item.potion.extra-healing"}


def useful_loot(item: Item) -> bool:
    return item.category in LOOT_CLASSES or item.identity == "item.misc.clean-bandage"


def preferred_pickup(turn: Turn, ground_items: tuple[GroundItem, ...], *, objective: str,
                     returning: bool, safe_pickup: bool) -> Item | None:
    """Take the objective first, then needed healing, then other useful loot in ground order."""
    useful = []
    for ground in ground_items:
        if not ground.reachable:
            continue
        item = ground.item
        if item.identity == objective:
            if not returning:
                return item
            if item.category == "amulet":
                continue
        if safe_pickup and useful_loot(item):
            useful.append(item)

    def healing_reserve_gain(item: Item) -> int:
        if item.identity not in HEALING_POTIONS:
            return 0
        missing = HEALING_RESERVE_PER_KIND - carried_quantity(turn, item.identity)
        return max(0, min(item.quantity, missing))

    return max(useful, key=healing_reserve_gain, default=None)



@dataclass
class LootGoals:
    wanted: set[str] = field(default_factory=set)
    nearby: set[str] = field(default_factory=set)
    supplies: set[str] = field(default_factory=set)
    recover_arrows: bool = False


def worth_long_detour(objective: str, item: Item, carried: Counter[str | None], *, returning: bool,
                      under_threat: bool) -> bool:
    """Items worth routing to beyond the short supply-detour radius."""
    identity = item.identity
    if identity in HEALING_POTIONS:
        return carried[identity] < HEALING_RESERVE_PER_KIND
    if identity == "item.weapon.arrow" and returning and carried["item.weapon.shortbow"]:
        return not under_threat and carried[identity] < 8
    if (identity == objective and not returning) or item.category == "food":
        return True
    if returning:
        return False
    return ((identity == "item.weapon.dart" and carried[identity] < 12)
            or (identity == "item.weapon.spear" and carried[identity] == 0))


def loot_goals(turn: Turn, ground_items: tuple[GroundItem, ...], *, returning: bool,
               objective: str, guarded: Set[str]) -> LootGoals:
    goals = LootGoals()
    if turn.has_condition("rogue.levitation"):
        return goals
    carried: Counter[str | None] = Counter()
    stock: Counter[str] = Counter()
    usable_gear: set[str] = set()
    for item in turn.inventory:
        carried[item.identity] += item.quantity
        stock[item.category] += item.quantity
        if item.category in {"armor", "ring"} and item.identity is not None and not item.known_cursed:
            usable_gear.add(item.identity)

    @cache
    def wanted_rings() -> set[str]:
        return ring_upgrades(turn).keys() - usable_gear

    @cache
    def has_spare_armor_to_identify() -> bool:
        return any(
            choice.slot.kind == "body_armor" and not choice.item.known_cursed
            and choice.item.physical_reduction is None
            for choice in turn.equip_choices()
        )

    goals.recover_arrows = not returning and not turn.hostiles and carried["item.weapon.shortbow"] > 0
    for ground in ground_items:
        if ground.reachable or ground.cell is None:
            continue
        item = ground.item
        key = ground.cell.key
        if worth_long_detour(objective, item, carried, returning=returning, under_threat=bool(turn.hostiles)):
            goals.wanted.add(key)
        if returning or turn.hostiles or key in guarded or item.identity in HEALING_POTIONS:
            continue
        if useful_loot(item) and not turn.cell.stairs_down:
            goals.nearby.add(key)
        seek_armor = (item.category == "armor" and item.identity not in usable_gear
                      and not has_spare_armor_to_identify())
        seek_ring = item.category == "ring" and not item.known_cursed and item.identity in wanted_rings()
        if seek_armor or seek_ring or stock[item.category] < DETOUR_STOCK.get(item.category, 0):
            goals.supplies.add(key)
    return goals


def route_to_loot(turn: Turn, memory: FloorMap, goals: LootGoals, danger: Set[str]) -> tuple[Action, str] | None:
    if action := route_around_danger(turn, memory, goals.wanted, danger):
        return action, "seek_item"
    if goals.recover_arrows:
        arrows = memory.cells_with_item(lambda item: item.identity == "item.weapon.arrow")
        if arrows and (action := memory.toward(turn, arrows, avoid=danger, max_steps=6)):
            return action, "recover_arrows"
    if goals.nearby and (action := memory.toward(turn, goals.nearby, avoid=danger, max_steps=2)):
        return action, "seek_supplies"
    if goals.supplies and (action := memory.toward(turn, goals.supplies, avoid=danger, max_steps=3)):
        return action, "seek_supplies"
    return None
