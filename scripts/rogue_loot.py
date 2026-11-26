"""Choose reachable Rogue loot and route to more distant supplies."""

from collections import Counter
from collections.abc import Callable, Set
from dataclasses import dataclass, field
from functools import cached_property

from tor_client_bot import Action, FloorMap, GroundItem, Item, RememberedItem, Turn

from rogue_combat import HEALING_POTIONS, recovery_deficit
from rogue_gear import can_wield_shortbow, melee_score, primary_weapon_slot, remove_curse_scroll, ring_upgrades
from rogue_retreat import route_around_danger


DETOUR_STOCK = {"potion": 4, "scroll": 4, "wand": 2}
LOOT_CLASSES = {"amulet", "armor", "coin", "food", "potion", "ring", "scroll", "tool", "wand", "weapon"}
HEALING_RESERVE_PER_KIND = 2


def useful_loot(item: Item) -> bool:
    return item.category in LOOT_CLASSES or item.identity == "item.misc.clean-bandage"


def guarded_supply_cells(turn: Turn) -> set[str]:
    """Locate useful ground items within one square of any part of a visible hostile."""
    if not turn.hostiles:
        return set()
    piles = {ground.cell.key: ground.offset for ground in turn.ground_items
             if ground.cell is not None and useful_loot(ground.item)}
    guards = {offset for foe in turn.hostiles for offset in foe.cells}
    return {
        key for key, (x, y, z) in piles.items()
        if any(abs(x - gx) <= 1 and abs(y - gy) <= 1 and abs(z - gz) <= 1 for gx, gy, gz in guards)
    }


def usable_melee_weapon(item: Item) -> bool:
    return not item.known_cursed and melee_score(item) >= 0


class CarriedLoot:
    """Stack quantities and usable equipment relevant to pickup and route choices."""

    def __init__(self, turn: Turn):
        self._turn = turn
        self.by_identity: Counter[str | None] = Counter()
        self.by_category: Counter[str] = Counter()
        self.usable_gear: set[str] = set()
        for item in turn.inventory:
            self.by_identity[item.identity] += item.quantity
            self.by_category[item.category] += item.quantity
            if item.category in {"armor", "ring"} and item.identity is not None and not item.known_cursed:
                self.usable_gear.add(item.identity)

    @cached_property
    def _needs_melee_weapon(self) -> bool:
        if any(usable_melee_weapon(item) for item in self._turn.inventory):
            return False
        slot = primary_weapon_slot(self._turn)
        if slot is None or not slot.removal_blocked:
            return True
        # A locked melee weapon still works. Switching from a locked bow requires a quiet turn and a cure.
        return (slot.occupant is not None and melee_score(slot.occupant) < 0
                and not self._turn.hostiles and remove_curse_scroll(self._turn) is not None)

    def wants_first_melee_weapon(self, item: Item) -> bool:
        return usable_melee_weapon(item) and self._needs_melee_weapon

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
        if self.wants_first_melee_weapon(item):
            return True
        if item.category == "armor":
            return item.identity not in self.usable_gear and self._wants_spare_armor
        if item.category == "ring":
            return not item.known_cursed and item.identity in self._wanted_rings
        return self.by_category[item.category] < DETOUR_STOCK.get(item.category, 0)

    def healing_reserve_gain(self, item: Item) -> int:
        """Number of missing reserve doses this stack can supply."""
        if item.identity not in HEALING_POTIONS:
            return 0
        missing = HEALING_RESERVE_PER_KIND - self.by_identity[item.identity]
        return max(0, min(item.quantity, missing))

    def needs_weapon_detour(self, item: Item, *, returning: bool, seek_arrows: bool) -> bool:
        identity = item.identity
        count = self.by_identity[identity]
        if returning:
            return seek_arrows and identity == "item.weapon.arrow" and count < 8
        return ((identity == "item.weapon.dart" and count < 12)
                or (identity == "item.weapon.spear" and count == 0))


def preferred_pickup(turn: Turn, ground_items: tuple[GroundItem, ...], *, objective: str,
                     returning: bool) -> Item | None:
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

    if not useful:
        return None
    if turn.hostiles:
        health = turn.health
        if health is None or recovery_deficit(health) or any(turn.distance_to(foe) <= 1 for foe in turn.hostiles):
            return None
    if len(useful) == 1:
        return useful[0]
    carried = CarriedLoot(turn)
    return max(useful, key=lambda item: (carried.healing_reserve_gain(item),
                                         carried.wants_first_melee_weapon(item)))


@dataclass
class LootGoals:
    essential: set[str] = field(default_factory=set)
    weapons: set[str] = field(default_factory=set)
    nearby: set[str] = field(default_factory=set)
    supplies: set[str] = field(default_factory=set)
    recover_arrows: bool = False

    @property
    def wanted(self) -> set[str]:
        """All destinations worth a long detour when a clear route exists."""
        return self.essential | self.weapons


def remembered_item_destinations(turn: Turn, memory: FloorMap, matches: Callable[[RememberedItem], bool]) -> set[str]:
    """Pickup handles the current square. Only other squares can be route destinations."""
    goals = memory.cells_with_item(matches)
    if turn.cell is not None:
        goals.discard(turn.cell.key)
    return goals


def loot_goals(turn: Turn, ground_items: tuple[GroundItem, ...], *, returning: bool,
               objective: str, guarded: Set[str]) -> LootGoals:
    goals = LootGoals()
    if turn.has_condition("rogue.levitation"):
        return goals
    seek_arrows = not turn.hostiles and can_wield_shortbow(turn)
    goals.recover_arrows = seek_arrows and not returning

    distant_items = [ground for ground in ground_items if not ground.reachable and ground.cell is not None
                     and ground.cell != turn.cell]
    if not distant_items:
        return goals
    carried = CarriedLoot(turn)
    for ground in distant_items:
        item = ground.item
        key = ground.cell.key
        if (item.category == "food" or (not returning and item.identity == objective)
                or carried.healing_reserve_gain(item) > 0):
            goals.essential.add(key)
        elif key not in guarded and carried.needs_weapon_detour(item, returning=returning, seek_arrows=seek_arrows):
            goals.weapons.add(key)
        if returning or turn.hostiles or key in guarded or item.identity in HEALING_POTIONS:
            continue
        if useful_loot(item) and not turn.cell.stairs_down:
            goals.nearby.add(key)
        if carried.needs_supply(item):
            goals.supplies.add(key)
    return goals


def route_to_loot(turn: Turn, memory: FloorMap, goals: LootGoals, danger: Set[str], *,
                  guarded: Set[str]) -> tuple[Action, str] | None:
    avoid = danger | guarded
    if goals.weapons and (action := memory.toward_avoiding_traps(turn, goals.wanted, avoid=avoid)):
        return action, "seek_item"
    if goals.essential and (action := route_around_danger(turn, memory, goals.essential, avoid)):
        return action, "seek_item"
    if goals.recover_arrows:
        arrows = remembered_item_destinations(turn, memory, lambda item: item.identity == "item.weapon.arrow") - guarded
        if arrows and (action := memory.toward_avoiding_traps(turn, arrows, avoid=avoid, max_steps=6)):
            return action, "recover_arrows"
    if goals.nearby and (action := memory.toward_avoiding_traps(turn, goals.nearby, avoid=avoid, max_steps=2)):
        return action, "seek_supplies"
    if goals.supplies and (action := memory.toward_avoiding_traps(turn, goals.supplies, avoid=avoid, max_steps=3)):
        return action, "seek_supplies"
    return None
