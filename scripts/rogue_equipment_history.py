"""Equipment observations and admitted item actions for Rogue run reports."""

from collections import defaultdict

from tor_client_bot import Action, Equip, Item, Pickup, Shot, Turn, UseItem, Zap


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
        if isinstance(action, Pickup):
            self.record(action.item, "pickup_admitted")
        elif isinstance(action, Equip):
            self.record(action.item, "equip_admitted")
        elif isinstance(action, Zap):
            self.record(action.device.item, "activation_admitted")
        elif isinstance(action, UseItem):
            self.record(action.item, "use_admitted")
        elif isinstance(action, Shot):
            self.record(action.projectile, "projectile_admitted")
            if action.launcher is not None:
                self.record(action.launcher, "launcher_admitted")

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
