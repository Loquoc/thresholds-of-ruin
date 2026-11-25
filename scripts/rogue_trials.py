"""Track experiments with items whose effects the player has yet to identify."""

from dataclasses import dataclass, field

from rogue_activity import receipt_history_expired
from tor_client_bot import ActivityPhase, Device, DeviceActivation, Item, ItemUse, Result, Turn


def trial_appearance(item: Item) -> str:
    """Recognize a sampled item type even when a different instance is offered."""
    return item.appearance or item.name


def stack_was_reduced(turn: Turn, original: Item) -> bool:
    """A missing or smaller stack is evidence of consumption after an interrupted trial."""
    remaining = next((item.quantity for item in turn.inventory if item.target == original.target), 0)
    return remaining < original.quantity


@dataclass(frozen=True)
class PendingScrollTrial:
    floor: int
    submitted_at: int
    receipt: Result
    scroll: Item


@dataclass
class PotionTrials:
    sampled: set[str] = field(default_factory=set)
    pending: tuple[Item, Result] | None = None
    submitted_at: int | None = None

    def choose(self, turn: Turn) -> Item | None:
        if self.pending is not None or (self.submitted_at is not None and turn.tick <= self.submitted_at):
            return None
        return next((item for item in turn.inventory
                     if item.identity is None and ItemUse.DRINK in item.uses
                     and trial_appearance(item) not in self.sampled), None)

    def admitted(self, turn: Turn, potion: Item, receipt: Result | None) -> None:
        appearance = trial_appearance(potion)
        if receipt and receipt.activity and appearance not in self.sampled:
            self.pending = potion, receipt
            self.submitted_at = turn.tick
        else:
            self.sampled.add(appearance)

    def observe(self, turn: Turn) -> None:
        if self.pending is None:
            return
        potion, receipt = self.pending
        phase = turn.outcome_of(receipt)
        if phase in (ActivityPhase.FAILED, ActivityPhase.CANCELLED):
            if stack_was_reduced(turn, potion):
                self.sampled.add(trial_appearance(potion))
        elif phase is ActivityPhase.RESOLVED or receipt_history_expired(turn, phase, self.submitted_at):
            self.sampled.add(trial_appearance(potion))
        else:
            return
        self.pending = None


@dataclass
class ScrollTrials:
    """Track sampled appearances across floors and interrupted reads on each floor."""

    sampled: set[str] = field(default_factory=set)
    stalled: set[tuple[int, str]] = field(default_factory=set)
    pending: PendingScrollTrial | None = None
    tried_in_encounter: bool = False

    def choose(self, turn: Turn, floor: int, *, at_exit: bool) -> Item | None:
        if self.pending is not None:
            return None
        for item in turn.inventory:
            if item.identity is not None or ItemUse.READ not in item.uses:
                continue
            if (floor, item.target) in self.stalled:
                continue
            if at_exit and trial_appearance(item) in self.sampled:
                continue
            return item
        return None

    def admitted(self, turn: Turn, floor: int, scroll: Item, receipt: Result | None) -> None:
        if receipt and receipt.activity:
            self.pending = PendingScrollTrial(floor, turn.tick, receipt, scroll)
        else:
            self.tried_in_encounter = True
            self.sampled.add(trial_appearance(scroll))

    def observe(self, turn: Turn) -> None:
        trial = self.pending
        if trial is None:
            return
        phase = turn.outcome_of(trial.receipt)
        if phase in (ActivityPhase.FAILED, ActivityPhase.CANCELLED):
            self.interrupted(turn)
            return
        if phase is ActivityPhase.RESOLVED or receipt_history_expired(turn, phase, trial.submitted_at):
            self.sampled.add(trial_appearance(trial.scroll))
            self.tried_in_encounter = True
            self.pending = None

    def interrupted(self, turn: Turn) -> None:
        trial = self.pending
        if trial is None:
            return
        self.stalled.add((trial.floor, trial.scroll.target))
        if stack_was_reduced(turn, trial.scroll):
            self.sampled.add(trial_appearance(trial.scroll))
        self.pending = None


@dataclass
class WandTrials:
    """Track unknown charges per device and experiments per appearance."""

    tried_without_charge_count: set[str] = field(default_factory=set)
    sampled: set[str] = field(default_factory=set)

    def can_spend_charge(self, device: Device) -> bool:
        if device.charges is None:
            return device.item.target not in self.tried_without_charge_count
        return device.charges > 0

    def can_experiment(self, device: Device) -> bool:
        item = device.item
        return (item.category == "wand" and item.identity is None
                and device.activation is DeviceActivation.DIRECTION
                and self.can_spend_charge(device) and trial_appearance(item) not in self.sampled)

    def admitted(self, device: Device, *, experiment: bool) -> None:
        if device.charges is None:
            self.tried_without_charge_count.add(device.item.target)
        if experiment:
            self.sampled.add(trial_appearance(device.item))
