"""Track experiments with items whose effects the player has yet to identify."""

from dataclasses import dataclass, field

from tor_client_bot import ActivityPhase, Device, DeviceActivation, Item, ItemUse, Result, Turn


def trial_appearance(item: Item) -> str:
    """Recognize a sampled item type even when a different instance is offered."""
    return item.appearance or item.name


def stack_is_intact(turn: Turn, original: Item) -> bool:
    """Whether the player still carries the same item stack at its pre-use quantity."""
    return any(item.target == original.target and item.quantity == original.quantity for item in turn.inventory)


def resolved_or_history_expired(turn: Turn, phase: ActivityPhase | None, submitted_at: int | None) -> bool:
    """Release a trial on resolution or when a later ready turn has lost its history."""
    return phase is ActivityPhase.RESOLVED or (
        phase is None and submitted_at is not None and turn.tick > submitted_at and turn.can_act
    )


@dataclass(frozen=True)
class PendingScrollTrial:
    floor: int
    submitted_at: int
    receipt: Result
    scroll: Item
    newly_sampled_appearance: str | None


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
        self.sampled.add(appearance)

    def observe(self, turn: Turn) -> None:
        if self.pending is None:
            return
        potion, receipt = self.pending
        phase = turn.outcome_of(receipt)
        if phase in (ActivityPhase.FAILED, ActivityPhase.CANCELLED):
            if stack_is_intact(turn, potion):
                self.sampled.discard(trial_appearance(potion))
        elif not resolved_or_history_expired(turn, phase, self.submitted_at):
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
        appearance = trial_appearance(scroll)
        if receipt and receipt.activity:
            newly_sampled = appearance if appearance not in self.sampled else None
            self.pending = PendingScrollTrial(floor, turn.tick, receipt, scroll, newly_sampled)
        else:
            self.tried_in_encounter = True
        self.sampled.add(appearance)

    def observe(self, turn: Turn) -> None:
        trial = self.pending
        if trial is None:
            return
        phase = turn.outcome_of(trial.receipt)
        if phase in (ActivityPhase.FAILED, ActivityPhase.CANCELLED):
            self.interrupted(turn)
            return
        if resolved_or_history_expired(turn, phase, trial.submitted_at):
            self.tried_in_encounter = True
            self.pending = None

    def interrupted(self, turn: Turn) -> None:
        trial = self.pending
        if trial is None:
            return
        self.stalled.add((trial.floor, trial.scroll.target))
        if trial.newly_sampled_appearance is not None and stack_is_intact(turn, trial.scroll):
            self.sampled.discard(trial.newly_sampled_appearance)
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
