"""Rogue equipment decisions using disclosed player information."""

from collections import Counter, defaultdict
from collections.abc import Collection

from tor_client_bot import Action, Item, ItemUse, Turn, Unequip, UseItem, Zap


MELEE_WEAPONS = {"item.weapon.mace", "item.weapon.dagger", "item.weapon.long-sword",
                 "item.weapon.two-handed-sword", "item.weapon.spear"}
IDENTIFY_CATEGORIES = {
    "item.scroll.identify-armor": {"armor"},
    "item.scroll.identify-weapon": {"weapon"},
    "item.scroll.identify-potion": {"potion"},
    "item.scroll.identify-scroll": {"scroll"},
    "item.scroll.identify-wand": {"ring", "wand"},
}
# Public Rogue catalog values. The instance can be cursed or corroded.
BASE_PROTECTION = {
    "item.armor.leather-armor": 1, "item.armor.ring-mail": 2, "item.armor.studded-leather": 2,
    "item.armor.scale-mail": 3, "item.armor.chain-mail": 4, "item.armor.splint-mail": 5,
    "item.armor.banded-mail": 5, "item.armor.plate-mail": 6,
}


def needs_identification(item: Item) -> bool:
    return item.identity is None or (
        (item.category == "armor" or item.identity in MELEE_WEAPONS) and not item.equipment_identified
    )


def identification_categories(scroll: Item) -> Collection[str]:
    if ItemUse.READ not in scroll.uses:
        return ()
    return scroll.identifies or IDENTIFY_CATEGORIES.get(scroll.identity, ())


def can_identify_carried_item(scroll: Item, turn: Turn) -> bool:
    categories = identification_categories(scroll)
    return bool(categories) and any(
        item.category in categories and needs_identification(item) for item in turn.inventory
    )


def _armor_protection(item: Item, *, account_for_curse: bool = False) -> int | None:
    if item.physical_reduction is not None:
        return item.physical_reduction
    base = BASE_PROTECTION.get(item.identity or "")
    if base is not None and account_for_curse and item.known_cursed is not False:
        return base - 3  # Worst published penalty at birth.
    return base


def choose_armor(turn: Turn) -> tuple[Action, str] | None:
    scrolls = {i.identity: i for i in turn.inventory if ItemUse.READ in i.uses}
    cure = scrolls.get("item.scroll.remove-curse")
    can_identify_armor = any("armor" in identification_categories(item) for item in turn.inventory)
    best = None
    best_rank = None
    for choice in turn.equip_choices():
        slot, item = choice.slot, choice.item
        if slot.kind != "body_armor" or item.known_cursed or (slot.removal_blocked and not cure):
            continue
        worn = slot.occupant
        estimated = item.physical_reduction is None or (worn is not None and worn.physical_reduction is None)
        if estimated and can_identify_armor and (needs_identification(item) or (worn and needs_identification(worn))):
            continue
        current = _armor_protection(worn) if worn else 0
        rating = _armor_protection(item, account_for_curse=cure is None)
        if rating is None or current is None or rating <= current:
            continue
        rank = rating - current, item.physical_reduction is not None
        if best_rank is not None and rank <= best_rank:
            continue
        best_rank = rank
        if slot.removal_blocked and cure is not None:
            best = UseItem(cure, ItemUse.READ), "remove_armor_curse"
        else:
            best = (Unequip(slot) if worn else choice), "try_armor" if estimated else "upgrade_armor"
    return best


def reveal_unseen_attacker(turn: Turn) -> Action | None:
    """Use a free sight ring before spending a potion."""
    if turn.has_condition("rogue.blindness") or turn.has_condition("rogue.see_invisible"):
        return None
    sight_ring = "item.ring.ring-see-invisible"
    if any(slot.occupant and slot.occupant.identity == sight_ring for slot in turn.equipment):
        return None
    for choice in turn.equip_choices():
        if choice.item.identity == sight_ring and not choice.item.known_cursed and choice.slot.occupant is None:
            return choice
    for item in turn.inventory:
        if item.identity == "item.potion.see-invisible" and ItemUse.DRINK in item.uses:
            return UseItem(item, ItemUse.DRINK)
    return None


def _ring_ranks(*preferred: str) -> dict[str, int]:
    order = (*preferred, "sustain-strength", "see-invisible", "maintain-armor", "stealth")
    return {f"item.ring.ring-{name}": len(order) - index for index, name in enumerate(order)}


_NORMAL_RING_RANKS = _ring_ranks("slow-digestion", "protection", "searching", "regeneration")
_RECOVERY_RING_RANKS = _ring_ranks("regeneration", "slow-digestion", "protection", "searching")


def ring_upgrades(turn: Turn) -> dict[str, dict[int, int]]:
    """Positive gains by recognized ring identity and replaceable finger slot."""
    worn = Counter(slot.occupant.identity for slot in turn.equipment if slot.kind == "ring" and slot.occupant)
    injured = turn.health is not None and turn.health.current < turn.health.maximum
    ranks = _NORMAL_RING_RANKS
    if injured and any(ItemUse.EAT in item.uses for item in turn.inventory):
        ranks = _RECOVERY_RING_RANKS
    upgrades: dict[str, dict[int, int]] = {}
    for slot in turn.equipment:
        if slot.kind != "ring" or slot.removal_blocked:
            continue
        old = slot.occupant.identity if slot.occupant else None
        cost = ranks.get(old, 0) if old and worn[old] == 1 else 0
        for identity, rank in ranks.items():
            if identity not in worn and rank > cost:
                upgrades.setdefault(identity, {})[slot.index] = rank - cost
    return upgrades


def choose_ring(turn: Turn) -> tuple[Action, str] | None:
    choices = [choice for choice in turn.equip_choices() if choice.slot.kind == "ring"
               and choice.item.identity is not None and not choice.item.known_cursed]
    if not choices:
        return None
    upgrades = ring_upgrades(turn)
    best_gain = 0
    best = None
    for choice in choices:
        incumbent = choice.slot.occupant
        value = upgrades.get(choice.item.identity or "", {}).get(choice.slot.index, 0)
        if value > best_gain or (value == best_gain and value > 0 and incumbent is None):
            best, best_gain = choice, value
    if best is None:
        return None
    if best.slot.occupant:
        return Unequip(best.slot), "change_ring"
    return best, "conserve_food" if best.item.identity == "item.ring.ring-slow-digestion" else "upgrade_equipment"


def improve_worn_gear(turn: Turn) -> tuple[Action, str] | None:
    scrolls = {i.identity: i for i in turn.inventory if ItemUse.READ in i.uses}
    armor = next((s.occupant for s in turn.equipment if s.kind == "body_armor" and s.occupant), None)
    if armor:
        if armor.known_protected is False and armor.identity != "item.armor.leather-armor":
            if protect := scrolls.get("item.scroll.protect-armor"):
                return UseItem(protect, ItemUse.READ), "protect_armor"
        if enchant := scrolls.get("item.scroll.enchant-armor"):
            return UseItem(enchant, ItemUse.READ), "enchant_armor"
    primary = next((s.occupant for s in turn.equipment if s.kind == "weapon"), None)
    if primary and primary.identity in MELEE_WEAPONS and (scroll := scrolls.get("item.scroll.enchant-weapon")):
        if upgrade := ready_weapon(turn, bow=False):
            return upgrade
        return UseItem(scroll, ItemUse.READ), "enchant_weapon"
    return None


def melee_score(item: Item) -> float:
    """Rank weapons by accuracy-adjusted damage per tick across wind-up and recovery."""
    if item.category != "weapon" and item.identity != "item.weapon.mace":
        return -1
    if item.identity == "item.weapon.shortbow":
        return -1
    attack = item.weapon_attack
    if attack is None:
        return 0 if item.identity == "item.weapon.mace" else -1
    accuracy = max(0.1, 1 + attack.accuracy_bonus / 20)
    return accuracy * attack.mean_damage / (attack.wind_up + attack.recovery)


def ready_weapon(turn: Turn, *, bow: bool) -> tuple[Action, str] | None:
    slot = next((s for s in turn.equipment if s.kind == "weapon"), None)
    if slot is None or (slot.removal_blocked and turn.hostiles):
        return None
    worn = slot.occupant
    choices = [c for c in turn.equip_choices() if c.slot is slot and not c.item.known_cursed]
    if bow and worn and worn.identity == "item.weapon.shortbow":
        return None
    choice = None
    if bow:
        choice = next((c for c in choices if c.item.identity == "item.weapon.shortbow"), None)
    if choice is None:
        choice = max(choices, key=lambda candidate: melee_score(candidate.item), default=None)
        worn_score = melee_score(worn) if worn else -1
        if choice is None or melee_score(choice.item) <= worn_score:
            return None
    if slot.removal_blocked:
        for item in turn.inventory:
            if item.identity == "item.scroll.remove-curse" and ItemUse.READ in item.uses:
                return UseItem(item, ItemUse.READ), "remove_weapon_curse"
        return None
    if worn:
        return Unequip(slot), "change_weapon"
    return choice, "equip_bow" if choice.item.identity == "item.weapon.shortbow" else "equip_melee"


class EquipmentHistory:
    """Count distinct player-visible items reaching each stage of a run."""

    def __init__(self) -> None:
        self.stages: dict[str, dict[str, set[str]]] = defaultdict(lambda: defaultdict(set))
        self.last_devices: list[dict] = []
        self.last_inventory_tick: int | None = None

    def record(self, item: Item, stage: str) -> None:
        if item.category in {"armor", "weapon", "wand", "ring", "potion", "scroll", "food"}:
            self.stages[item.category][stage].add(item.target)

    def admitted(self, action: Action) -> None:
        if isinstance(action, Zap):
            self.record(action.device.item, "activation_admitted")
        elif isinstance(action, UseItem):
            self.record(action.item, "use_admitted")

    def observe(self, turn: Turn) -> None:
        if not turn.terminal:
            self.last_inventory_tick = turn.tick
            offered_charges = {device.item.target: device.charges for device in turn.devices}
            self.last_devices = [
                {"identity": item.identity, "name": item.name, "activation_offered": item.target in offered_charges,
                 "charges": offered_charges.get(item.target)}
                for item in turn.inventory if item.category == "wand" or item.target in offered_charges
            ]
        for ground in turn.ground_items:
            self.record(ground.item, "seen_on_ground")
        for item in turn.inventory:
            self.record(item, "carried")
            if item.identity is not None:
                self.record(item, "recognized")
            if item.physical_reduction is not None:
                self.record(item, "protection_known")
            if item.weapon_attack is not None:
                self.record(item, "attack_known")
        for slot in turn.equipment:
            if slot.occupant:
                self.record(slot.occupant, "worn")

    def counts(self) -> dict[str, dict[str, int]]:
        return {category: {stage: len(items) for stage, items in stages.items()}
                for category, stages in self.stages.items()}

    def report(self) -> dict[str, object]:
        """Report whole-run stage counts alongside the last nonterminal inventory."""
        return {
            "equipment_opportunities": self.counts(),
            "last_inventory_tick": self.last_inventory_tick,
            "last_carried_devices": self.last_devices,
        }
