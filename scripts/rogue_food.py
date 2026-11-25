"""Food observations and provisioning estimates used by the Rogue player."""

from collections.abc import Iterable, Iterator, Mapping
from dataclasses import dataclass
from statistics import median_low

from tor_client_bot import Item, ItemUse, Nutrition, Turn


URGENT_HUNGER = frozenset({"weak", "empty"})
MEAL_HUNGER = URGENT_HUNGER | {"hungry"}


def edible_items(turn: Turn) -> Iterator[Item]:
    return (item for item in turn.inventory if ItemUse.EAT in item.uses)


def choose_meal(turn: Turn, food_value: Mapping[str, int]) -> Item | None:
    """Prefer the greatest usable gain, then the meal that wastes less food."""
    nutrition = turn.nutrition

    def rank(item: Item) -> tuple[int, int]:
        if item.identity is None or item.identity not in food_value:
            return -1, 0
        gain = food_value[item.identity]
        usable = min(gain, nutrition.maximum - nutrition.reserve) if nutrition else gain
        return usable, -gain

    return max(edible_items(turn), key=rank, default=None)


def carried_quantity(turn: Turn, identity: str) -> int:
    return sum(item.quantity for item in turn.inventory if item.identity == identity)


@dataclass(frozen=True)
class PendingMeal:
    identity: str
    quantity_before: int
    reserve_before: int

    @classmethod
    def before_eating(cls, turn: Turn, item: Item) -> "PendingMeal | None":
        nutrition = turn.nutrition
        if item.identity is None or nutrition is None:
            return None
        return cls(item.identity, carried_quantity(turn, item.identity), nutrition.reserve)

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


@dataclass(frozen=True)
class FoodEstimate:
    known_reserve: int
    has_unmeasured_meals: bool
    meal_count: int

    def known_ticks(self, nutrition: Nutrition) -> int:
        return self.known_reserve * nutrition.ticks_per_unit


@dataclass(frozen=True)
class FoodScout:
    deadline: int
    starting_meals: int


def estimate_carried_food(meals: Iterable[Item], food_value: Mapping[str, int]) -> FoodEstimate:
    known_reserve = 0
    has_unmeasured_meals = False
    meal_count = 0
    for meal in meals:
        meal_count += meal.quantity
        if meal.identity is None or meal.identity not in food_value:
            has_unmeasured_meals = True
        else:
            known_reserve += food_value[meal.identity] * meal.quantity
    return FoodEstimate(known_reserve, has_unmeasured_meals, meal_count)


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
