"""Food observations and provisioning estimates used by the Rogue player."""

from collections.abc import Iterable, Iterator, Mapping
from dataclasses import dataclass, field
from statistics import median_low

from rogue_activity import receipt_history_expired
from tor_client_bot import ActivityPhase, Direction, FloorMap, Item, ItemUse, Nutrition, Result, StairAttempt, Turn


URGENT_HUNGER = frozenset({"weak", "empty"})
MEAL_HUNGER = URGENT_HUNGER | {"hungry"}


def edible_items(turn: Turn) -> Iterator[Item]:
    return (item for item in turn.inventory if ItemUse.EAT in item.uses)


def carried_meal_count(turn: Turn) -> int:
    return sum(item.quantity for item in edible_items(turn))


def choose_meal(turn: Turn, food_value: Mapping[str, int], *, candidates: Iterable[Item] | None = None) -> Item | None:
    """Prefer the greatest usable gain, then the meal that wastes less food."""
    nutrition = turn.nutrition

    def rank(item: Item) -> tuple[int, int]:
        if item.identity is None or item.identity not in food_value:
            return -1, 0
        gain = food_value[item.identity]
        usable = min(gain, nutrition.maximum - nutrition.reserve) if nutrition else gain
        return usable, -gain

    if candidates is None:
        candidates = edible_items(turn)
    return max(candidates, key=rank, default=None)


def carried_quantity(turn: Turn, identity: str) -> int:
    return sum(item.quantity for item in turn.inventory if item.identity == identity)


@dataclass
class PendingMeal:
    identity: str
    quantity_before: int
    reserve_before: int
    submitted_at: int
    receipt: Result | None = None
    resolved: bool = False

    def observed_gain(self, turn: Turn) -> int | None:
        nutrition = turn.nutrition
        if nutrition is None or nutrition.reserve >= nutrition.maximum:
            # At the cap, the reserve change only gives a lower bound on the meal's value.
            return None
        consumed = self.quantity_before - carried_quantity(turn, self.identity)
        if consumed != 1:
            return None
        gain = nutrition.reserve - self.reserve_before
        return gain if gain > 0 else None


@dataclass
class MealHistory:
    """Keep the lowest measured net reserve gain for each recognized meal."""

    gains: dict[str, int] = field(default_factory=dict)
    pending: PendingMeal | None = None

    def admitted(self, turn: Turn, item: Item, receipt: Result | None = None) -> None:
        nutrition = turn.nutrition
        if item.identity is None or nutrition is None:
            self.pending = None
            return
        self.pending = PendingMeal(item.identity, carried_quantity(turn, item.identity), nutrition.reserve,
                                   submitted_at=turn.tick, receipt=receipt)

    def observe(self, turn: Turn) -> None:
        meal = self.pending
        if meal is None:
            return
        if meal.receipt and meal.receipt.activity:
            if not meal.resolved:
                phase = turn.outcome_of(meal.receipt)
                history_expired = receipt_history_expired(turn, phase, meal.submitted_at)
                if phase in (ActivityPhase.FAILED, ActivityPhase.CANCELLED) or history_expired:
                    self.pending = None
                    return
                if phase is not ActivityPhase.RESOLVED:
                    return
                meal.resolved = True
            if turn.nutrition is None and not turn.can_act:
                return
        elif not turn.can_act:
            return
        self.pending = None
        gain = meal.observed_gain(turn)
        if gain is not None:
            self.gains[meal.identity] = min(gain, self.gains.get(meal.identity, gain))


@dataclass(frozen=True)
class FoodEstimate:
    known_reserve: int
    has_unmeasured_meals: bool

    def spare_ticks_after_travel(self, nutrition: Nutrition, travel_ticks: int) -> int:
        """Current food-clock ticks free for rest. A negative result is a journey shortfall."""
        packed_ticks = self.known_reserve * nutrition.ticks_per_unit
        travel_on_current_reserve = max(0, travel_ticks - packed_ticks)
        return nutrition.ticks_until_empty - travel_on_current_reserve


@dataclass(frozen=True)
class FoodScout:
    deadline: int
    meal_baseline: int

    def finished(self, turn: Turn) -> bool:
        if turn.tick >= self.deadline:
            return True
        return carried_meal_count(turn) > self.meal_baseline


@dataclass
class FoodScoutHistory:
    """Each floor gets one detour. A finished entry keeps it closed on revisits."""

    by_floor: dict[int, FoodScout | None] = field(default_factory=dict)

    def finish(self, floor: int) -> None:
        if floor in self.by_floor:
            self.by_floor[floor] = None

    def observe(self, floor: int, turn: Turn) -> None:
        scout = self.by_floor.get(floor)
        if scout is not None and scout.finished(turn):
            self.finish(floor)

    def before_food_pickup(self, floor: int, turn: Turn) -> None:
        scout = self.by_floor.get(floor)
        if scout is not None:
            # An acquired portion still counts when it replaces one eaten during the detour.
            carried = carried_meal_count(turn)
            if carried < scout.meal_baseline:
                self.by_floor[floor] = FoodScout(scout.deadline, meal_baseline=carried)

    @staticmethod
    def _starting_budget(turn: Turn) -> int:
        """Allow a quarter of the remaining food clock for a detour from the down stair."""
        if turn.cell is None or not turn.cell.stairs_down or turn.nutrition is None:
            return 0
        return turn.nutrition.ticks_until_empty // 4

    def can_explore(self, floor: int, turn: Turn) -> bool:
        if floor not in self.by_floor:
            return self._starting_budget(turn) > 0
        scout = self.by_floor[floor]
        return scout is not None and turn.tick < scout.deadline

    def start_if_new(self, floor: int, turn: Turn) -> None:
        if floor not in self.by_floor and (budget := self._starting_budget(turn)) > 0:
            self.by_floor[floor] = FoodScout(deadline=turn.tick + budget, meal_baseline=carried_meal_count(turn))


def estimate_carried_food(meals: Iterable[Item], food_value: Mapping[str, int]) -> FoodEstimate:
    known_reserve = 0
    has_unmeasured_meals = False
    for meal in meals:
        if meal.identity is None or meal.identity not in food_value:
            has_unmeasured_meals = True
        else:
            known_reserve += food_value[meal.identity] * meal.quantity
    return FoodEstimate(known_reserve, has_unmeasured_meals)


def estimate_round_trip_ticks(depth: int, retrieval_depth: int, descent: Mapping[int, int],
                              ascent: Mapping[int, int]) -> int:
    """Budget a round trip for each floor from here up to the retrieval floor."""
    if depth >= retrieval_depth or not descent:
        return 0
    round_trips = {floor: ticks + ascent.get(floor, 0) for floor, ticks in descent.items()}
    measured = [ticks for floor, ticks in round_trips.items() if depth <= floor < retrieval_depth]
    unmeasured = retrieval_depth - depth - len(measured)
    if not unmeasured:
        return sum(measured)
    # Estimate unseen floors from the five deepest descents observed so far.
    deepest = sorted(round_trips)[-5:]
    typical = median_low(round_trips[floor] for floor in deepest)
    return sum(measured) + unmeasured * typical


@dataclass
class FloorTravelTimes:
    """Observed first descents and estimated return times used to budget food."""

    descent: dict[int, int] = field(default_factory=dict)
    ascent: dict[int, int] = field(default_factory=dict)
    entered_at: int | None = None

    def record_descent(self, floor: int, memory: FloorMap, attempt: StairAttempt, arrival_tick: int) -> None:
        if self.entered_at is not None:
            self.descent.setdefault(floor, arrival_tick - self.entered_at)
        upstairs = memory.stairs(Direction.UP)
        if upstairs:
            steps = memory.distance(attempt.origin, upstairs)
            if steps is not None:
                ticks_per_step = arrival_tick - attempt.tick
                self.ascent[floor] = (steps + 1) * ticks_per_step

    def return_ticks(self, *, through_floor: int | None = None) -> int:
        return sum(ticks for floor, ticks in self.ascent.items() if through_floor is None or floor <= through_floor)

    def journey_ticks(self, depth: int, retrieval_depth: int, *, returning: bool) -> int:
        known_return = self.return_ticks(through_floor=depth - 1)
        if returning:
            return known_return
        return known_return + estimate_round_trip_ticks(depth, retrieval_depth, self.descent, self.ascent)
