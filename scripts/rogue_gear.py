"""Rogue equipment decisions using disclosed player information."""

from collections import Counter, defaultdict
from collections.abc import Collection, Iterable

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


def offered_item_use(turn: Turn, identity: str, use: ItemUse) -> UseItem | None:
    """Use the first recognized copy for which the player has this action."""
    for item in turn.inventory:
        if item.identity == identity and use in item.uses:
            return UseItem(item, use)
    return None


def needs_identification(item: Item, turn: Turn) -> bool:
    if item.identity is None:
        return True
    if item.equipment_identified or (item.category != "armor" and item.identity not in MELEE_WEAPONS):
        return False
    if item.known_cursed:
        return any(slot.occupant and slot.occupant.target == item.target for slot in turn.equipment)
    return True


def identification_target(turn: Turn, candidates: Iterable[Item]) -> Item | None:
    """Prefer a larger unknown consumable stack. Equal scores keep candidate order."""
    def revealed_units(item: Item) -> int:
        if item.identity is None and item.category in {"potion", "scroll", "food"}:
            return item.quantity
        return 1

    eligible = (item for item in candidates if needs_identification(item, turn))
    return max(eligible, key=revealed_units, default=None)


def identification_categories(scroll: Item) -> Collection[str]:
    if ItemUse.READ not in scroll.uses:
        return ()
    return scroll.identifies or IDENTIFY_CATEGORIES.get(scroll.identity, ())


def can_identify_carried_item(scroll: Item, turn: Turn) -> bool:
    categories = identification_categories(scroll)
    if not categories:
        return False
    return any(item.category in categories and needs_identification(item, turn) for item in turn.inventory)


def _armor_protection(item: Item, *, account_for_curse: bool = False) -> int | None:
    if item.physical_reduction is not None:
        return item.physical_reduction
    base = BASE_PROTECTION.get(item.identity or "")
    if base is not None and account_for_curse and item.known_cursed is not False:
        return base - 3  # Worst published penalty at birth.
    return base


def choose_armor(turn: Turn) -> tuple[Action, str] | None:
    cure = offered_item_use(turn, "item.scroll.remove-curse", ItemUse.READ)
    can_identify_armor = any("armor" in identification_categories(item) for item in turn.inventory)
    best = None
    best_rank = None
    for choice in turn.equip_choices():
        slot, item = choice.slot, choice.item
        if slot.kind != "body_armor" or item.known_cursed or (slot.removal_blocked and not cure):
            continue
        worn = slot.occupant
        estimated = item.physical_reduction is None or (worn is not None and worn.physical_reduction is None)
        if estimated and can_identify_armor and (needs_identification(item, turn) or (worn and needs_identification(worn, turn))):
            continue
        current = _armor_protection(worn) if worn else 0
        rating = _armor_protection(item, account_for_curse=cure is None)
        if rating is None or current is None or rating <= current:
            continue
        rank = rating - current, item.physical_reduction is not None
        if best_rank is None or rank > best_rank:
            best_rank = rank
            best = choice, estimated

    if best is None:
        return None
    choice, estimated = best
    if choice.slot.removal_blocked and cure is not None:
        return cure, "remove_armor_curse"
    action = Unequip(choice.slot) if choice.slot.occupant else choice
    return action, "try_armor" if estimated else "upgrade_armor"


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
    return offered_item_use(turn, "item.potion.see-invisible", ItemUse.DRINK)


def _ring_ranks(*preferred: str) -> dict[str, int]:
    order = (*preferred, "sustain-strength", "see-invisible", "maintain-armor", "stealth")
    return {f"item.ring.ring-{name}": len(order) - index for index, name in enumerate(order)}


_NORMAL_RING_RANKS = _ring_ranks("slow-digestion", "protection", "searching", "regeneration")
_RECOVERY_RING_RANKS = _ring_ranks("regeneration", "slow-digestion", "protection", "searching")


def ring_upgrades(turn: Turn) -> dict[str, dict[int, int]]:
    """Positive gains by recognized ring identity and replaceable finger slot."""
    fingers = [slot for slot in turn.equipment if slot.kind == "ring"]
    worn = Counter(slot.occupant.identity for slot in fingers if slot.occupant)
    injured = turn.health is not None and turn.health.current < turn.health.maximum
    ranks = _NORMAL_RING_RANKS
    if injured and any(ItemUse.EAT in item.uses for item in turn.inventory):
        ranks = _RECOVERY_RING_RANKS
    upgrades: dict[str, dict[int, int]] = {}
    unworn = {identity: rank for identity, rank in ranks.items() if identity not in worn}
    for slot in fingers:
        if slot.removal_blocked:
            continue
        old = slot.occupant.identity if slot.occupant else None
        # Removing a duplicate keeps the same effect on another finger, even if that finger is blocked.
        lost_rank = ranks.get(old, 0) if old and worn[old] == 1 else 0
        for identity, rank in unworn.items():
            if rank > lost_rank:
                upgrades.setdefault(identity, {})[slot.index] = rank - lost_rank
    return upgrades


def choose_ring(turn: Turn) -> tuple[Action, str] | None:
    choices = [choice for choice in turn.equip_choices() if choice.slot.kind == "ring"
               and choice.item.identity is not None and not choice.item.known_cursed]
    if not choices:
        return None
    upgrades = ring_upgrades(turn)
    # Prefer the largest gain, then a free slot. Keep offer order for equal choices.
    best_rank = (0, True)
    best = None
    for choice in choices:
        gain = upgrades.get(choice.item.identity, {}).get(choice.slot.index, 0)
        rank = gain, choice.slot.occupant is None
        if rank > best_rank:
            best, best_rank = choice, rank
    if best is None:
        return None
    if best.slot.occupant:
        return Unequip(best.slot), "change_ring"
    return best, "conserve_food" if best.item.identity == "item.ring.ring-slow-digestion" else "upgrade_equipment"


def improve_worn_gear(turn: Turn) -> tuple[Action, str] | None:
    armor = next((s.occupant for s in turn.equipment if s.kind == "body_armor" and s.occupant), None)
    if armor:
        if armor.known_protected is False and armor.identity != "item.armor.leather-armor":
            if protect := offered_item_use(turn, "item.scroll.protect-armor", ItemUse.READ):
                return protect, "protect_armor"
        if enchant := offered_item_use(turn, "item.scroll.enchant-armor", ItemUse.READ):
            return enchant, "enchant_armor"
    primary = next((s.occupant for s in turn.equipment if s.kind == "weapon"), None)
    if primary is None or primary.identity not in MELEE_WEAPONS:
        return None
    if scroll := offered_item_use(turn, "item.scroll.enchant-weapon", ItemUse.READ):
        if upgrade := ready_weapon(turn, bow=False):
            return upgrade
        return scroll, "enchant_weapon"
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

    def rank(item: Item | None) -> tuple[bool, float]:
        if item is None:
            return False, -1
        preferred_bow = bow and item.identity == "item.weapon.shortbow"
        return preferred_bow, melee_score(item)

    best_rank = rank(worn)
    upgrade = None
    for choice in turn.equip_choices():
        if choice.slot is not slot or choice.item.known_cursed:
            continue
        candidate_rank = rank(choice.item)
        if candidate_rank > best_rank:
            best_rank, upgrade = candidate_rank, choice
    if upgrade is None:
        return None
    if slot.removal_blocked:
        if cure := offered_item_use(turn, "item.scroll.remove-curse", ItemUse.READ):
            return cure, "remove_weapon_curse"
        return None
    if worn:
        return Unequip(slot), "change_weapon"
    return upgrade, "equip_bow" if upgrade.item.identity == "item.weapon.shortbow" else "equip_melee"


def carried_device_report(turn: Turn) -> list[dict]:
    """Describe each carried device using the same item target as the action trace."""
    charges = {device.item.target: device.charges for device in turn.devices}
    return [
        {"item": item.target, "identity": item.identity, "name": item.name,
         "activation_offered": item.target in charges, "charges": charges.get(item.target)}
        for item in turn.inventory if item.category == "wand" or item.target in charges
    ]


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
            self.last_devices = carried_device_report(turn)
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
