"""Choose reachable Rogue loot and route to more distant supplies."""

from collections import Counter
from collections.abc import Set
from dataclasses import dataclass, field
from functools import cached_property

from tor_client_bot import Action, FloorMap, GroundItem, Item, Turn

from rogue_combat import recovery_deficit
from rogue_gear import can_wield_shortbow, melee_score, remove_curse_scroll, ring_upgrades
from rogue_retreat import route_around_danger


DETOUR_STOCK = {"potion": 4, "scroll": 4, "wand": 2}
LOOT_CLASSES = {"amulet", "armor", "coin", "food", "potion", "ring", "scroll", "tool", "wand", "weapon"}
HEALING_RESERVE_PER_KIND = 2
HEALING_POTIONS = {"item.potion.healing", "item.potion.extra-healing"}


def useful_loot(item: Item) -> bool:
    return item.category in LOOT_CLASSES or item.identity == "item.misc.clean-bandage"


def usable_melee_weapon(item: Item) -> bool:
    return not item.known_cursed and melee_score(item) >= 0


class CarriedLoot:
    """Stack quantities and usable equipment relevant to pickup and route choices."""

    def __init__(self, turn: Turn):
        self._turn = turn
        self.by_identity: Counter[str | None] = Counter()
        self.by_category: Counter[str] = Counter()
        self.usable_gear: set[str] = set()
        weapon_slot = next((slot for slot in turn.equipment if slot.kind == "weapon"), None)
        weapon_locked = bool(weapon_slot and weapon_slot.removal_blocked)
        self.needs_melee_weapon = not weapon_locked
        for item in turn.inventory:
            self.by_identity[item.identity] += item.quantity
            self.by_category[item.category] += item.quantity
            if usable_melee_weapon(item):
                self.needs_melee_weapon = False
            if item.category in {"armor", "ring"} and item.identity is not None and not item.known_cursed:
                self.usable_gear.add(item.identity)

    @cached_property
    def _wanted_rings(self) -> set[str]:
        can_remove_curse = remove_curse_scroll(self._turn) is not None
        return ring_upgrades(self._turn, can_remove_curse=can_remove_curse).keys() - self.usable_gear

    @cached_property
    def _wants_spare_armor(self) -> bool:
        locked = any(slot.kind == "body_armor" and slot.removal_blocked for slot in self._turn.equipment)
        if locked and remove_curse_scroll(self._turn) is None:
            return False
        return not any(
            choice.slot.kind == "body_armor" and not choice.item.known_cursed
            and choice.item.physical_reduction is None
            for choice in self._turn.equip_choices()
        )

    def needs_supply(self, item: Item) -> bool:
        if self.needs_melee_weapon and usable_melee_weapon(item):
            return True
        if item.category == "armor":
            return item.identity not in self.usable_gear and self._wants_spare_armor
        if item.category == "ring":
            return not item.known_cursed and item.identity in self._wanted_rings
        return self.by_category[item.category] < DETOUR_STOCK.get(item.category, 0)


def healing_reserve_gain(item: Item, carried: Counter[str | None]) -> int:
    """Number of missing reserve doses this stack can supply."""
    if item.identity not in HEALING_POTIONS:
        return 0
    missing = HEALING_RESERVE_PER_KIND - carried[item.identity]
    return max(0, min(item.quantity, missing))


def preferred_pickup(turn: Turn, ground_items: tuple[GroundItem, ...], *, objective: str,
                     returning: bool, safe_pickup: bool | None = None) -> Item | None:
    """Prioritize the objective, missing healing doses, and a first melee weapon. Ties keep ground order."""
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
        if useful_loot(item):
            useful.append(item)

    if not useful or safe_pickup is False:
        return None
    if safe_pickup is None and turn.hostiles:
        health = turn.health
        if health is None or recovery_deficit(health) or any(turn.distance_to(foe) <= 1 for foe in turn.hostiles):
            return None
    if len(useful) == 1:
        return useful[0]
    carried = CarriedLoot(turn)
    return max(useful, key=lambda item: (healing_reserve_gain(item, carried.by_identity),
                                         carried.needs_melee_weapon and usable_melee_weapon(item)))


@dataclass
class LootGoals:
    wanted: set[str] = field(default_factory=set)
    nearby: set[str] = field(default_factory=set)
    supplies: set[str] = field(default_factory=set)
    recover_arrows: bool = False


def worth_long_detour(objective: str, item: Item, carried: Counter[str | None], *, returning: bool,
                      needs_return_arrows: bool, guarded: bool) -> bool:
    """Items worth routing to beyond the short supply-detour radius."""
    identity = item.identity
    if identity in HEALING_POTIONS:
        return healing_reserve_gain(item, carried) > 0
    if item.category == "food" or (not returning and identity == objective):
        return True
    if guarded:
        return False
    if returning:
        return identity == "item.weapon.arrow" and needs_return_arrows
    return ((identity == "item.weapon.dart" and carried[identity] < 12)
            or (identity == "item.weapon.spear" and carried[identity] == 0))


def loot_goals(turn: Turn, ground_items: tuple[GroundItem, ...], *, returning: bool,
               objective: str, guarded: Set[str]) -> LootGoals:
    goals = LootGoals()
    if turn.has_condition("rogue.levitation"):
        return goals
    carried = CarriedLoot(turn)

    seek_arrows = not turn.hostiles and can_wield_shortbow(turn)
    goals.recover_arrows = seek_arrows and not returning
    needs_return_arrows = seek_arrows and returning and carried.by_identity["item.weapon.arrow"] < 8
    for ground in ground_items:
        if ground.reachable or ground.cell is None:
            continue
        item = ground.item
        key = ground.cell.key
        if worth_long_detour(objective, item, carried.by_identity, returning=returning,
                             needs_return_arrows=needs_return_arrows, guarded=key in guarded):
            goals.wanted.add(key)
        if returning or turn.hostiles or key in guarded or item.identity in HEALING_POTIONS:
            continue
        if useful_loot(item) and not turn.cell.stairs_down:
            goals.nearby.add(key)
        if carried.needs_supply(item):
            goals.supplies.add(key)
    return goals


def route_to_loot(turn: Turn, memory: FloorMap, goals: LootGoals, danger: Set[str], *,
                  guarded: Set[str]) -> tuple[Action, str] | None:
    if action := route_around_danger(turn, memory, goals.wanted, danger):
        return action, "seek_item"
    if goals.recover_arrows:
        arrows = memory.cells_with_item(lambda item: item.identity == "item.weapon.arrow") - guarded
        if arrows and (action := memory.toward(turn, arrows, avoid=danger, max_steps=6)):
            return action, "recover_arrows"
    if goals.nearby and (action := memory.toward(turn, goals.nearby, avoid=danger, max_steps=2)):
        return action, "seek_supplies"
    if goals.supplies and (action := memory.toward(turn, goals.supplies, avoid=danger, max_steps=3)):
        return action, "seek_supplies"
    return None
