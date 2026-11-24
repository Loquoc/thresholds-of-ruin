"""Rogue retrieval strategy using the ordinary Python player interface."""

from __future__ import annotations

from collections import Counter
from collections.abc import Iterable, Iterator
from dataclasses import dataclass, field
from enum import StrEnum
from statistics import median_low
from typing import NamedTuple

from tor_client_bot import (
    AbilityOutcome,
    Action,
    ActivityChoice,
    ActivityPhase,
    CombatReport,
    Cell,
    Device,
    DeviceActivation,
    Direction,
    Creature,
    Escape,
    FloorMap,
    GroundItem,
    Injury,
    Item,
    ItemUse,
    Move,
    Outcome,
    Pickup,
    PointPool,
    RestorableLosses,
    Result,
    StairAttempt,
    Shot,
    Turn,
    Training,
    TrainingKind,
    TrainingOption,
    TrainingRefused,
    UseAbility,
    UseItem,
    Wait,
    Zap,
)

from rogue_food import FoodScout, PendingMeal, carried_quantity, choose_meal, edible_items, estimate_carried_food
from rogue_gear import (
    IDENTIFY_CATEGORIES, choose_armor, choose_ring, improve_worn_gear,
    needs_identification, ready_weapon, ring_upgrades,
)


DETOUR_STOCK = {"potion": 4, "scroll": 4, "wand": 2}
LOOT_CLASSES = {"amulet", "armor", "coin", "food", "potion", "ring", "scroll", "tool", "wand", "weapon"}
MELEE_INJURY_PRIORITY = {
    Injury.NEAR_DEATH: 0,
    Injury.BADLY_WOUNDED: 1,
    Injury.WOUNDED: 2,
}
DEAD_END_SEARCHES = 6
ELEMENTAL_WANDS = {"item.wand.wand-fire", "item.wand.wand-cold", "item.wand.wand-lightning"}
ELEMENTAL_RAY_RANGE = 6


class MemoryReason(StrEnum):
    """Trace reasons that also govern memory updates on admission."""

    CANCEL_STALLED_SCROLL = "cancel_stalled_scroll"
    CANCEL_STALLED_STAIR = "cancel_stalled_stair"
    WAIT_AFTER_CANCEL = "wait_after_cancel"
    TEST_UNKNOWN_DEVICE = "test_unknown_device"
    LIGHT_ROOM = "light_room"
    SLOW_APPROACH = "slow_approach"
    POLYMORPH_THREAT = "polymorph_threat"
    SEEK_STAIRS = "seek_stairs"
    EXPLORE = "explore"
    TAKE_CORRIDOR = "take_corridor"
    RETREAT_TO_STAIRS = "retreat_to_stairs"
    RETREAT = "retreat"
    BREAK_CONTACT = "break_contact"
    TEST_UNKNOWN_MAGIC = "test_unknown_magic"
    STUDY_SCROLL_AT_EXIT = "study_scroll_at_exit"
    RETREAT_UPSTAIRS = "retreat_upstairs"
    RETREAT_DOWNSTAIRS = "retreat_downstairs"


CONTROL_DEVICES = (
    ("item.wand.wand-slow", MemoryReason.SLOW_APPROACH),
    ("item.wand.wand-polymorph", MemoryReason.POLYMORPH_THREAT),
    ("item.wand.wand-magic-missile", "magic_missile"),
)


def training_choices(training: Training, skill: str) -> Iterator[tuple[PointPool, TrainingOption]]:
    priorities = (
        (TrainingKind.TALENT, ("rattling_blow", "dislocating_strike")),
        (TrainingKind.SKILL, (skill,)),
    )
    pools = training.pools()
    for kind, identities in priorities:
        eligible = [pool for pool in pools if pool.available and kind in pool.allowed]
        if not eligible:
            continue
        options = training.options(kind)
        for identity in identities:
            option = next((item for item in options if item.identity == identity and not item.forms), None)
            if option is not None:
                for pool in eligible:
                    yield pool, option


def trial_appearance(item: Item) -> str:
    """Recognize a sampled item type even when a different instance is offered."""
    return item.appearance or item.name


class SearchRank(NamedTuple):
    """Higher fields win, in declaration order. Exact ties keep map traversal order."""

    near_return_stairs: bool
    has_searchable_walls: bool
    value_after_travel: int
    negative_distance: int


@dataclass
class LootGoals:
    pickup: Item | None = None
    wanted: set[str] = field(default_factory=set)
    nearby: set[str] = field(default_factory=set)
    supplies: set[str] = field(default_factory=set)
    recover_arrows: bool = False


@dataclass
class Adventurer:
    retrieval_depth: int
    objective: str
    depth: int = 1
    maps: dict[int, FloorMap] = field(default_factory=dict)
    last_exploration_origin: str | None = None
    corridor_stand: tuple[str, int] | None = None
    last_shot_tick: int | None = None
    searches: Counter[str] = field(default_factory=Counter)
    completed_end_searches: Counter[str] = field(default_factory=Counter)
    pending_end_search: tuple[str, Result] | None = None
    wall_searches: Counter[str] = field(default_factory=Counter)
    ascent_ticks_by_floor: dict[int, int] = field(default_factory=dict)
    descent_ticks_by_floor: dict[int, int] = field(default_factory=dict)
    entered_floor_at: int | None = None
    food_value: dict[str, int] = field(default_factory=dict)
    pending_meal: PendingMeal | None = None
    # A missing floor can start a detour. None records a finished detour.
    food_scouts: dict[int, FoodScout | None] = field(default_factory=dict)
    pending_stair: StairAttempt | None = None
    stair_pursuers: tuple[str, ...] = ()
    last_guarded_landing: tuple[str, str] | None = None
    unsafe_stairs: dict[tuple[str, Direction], int] = field(default_factory=dict)
    cancelled_stair_origin: str | None = None
    wait_after_cancel: bool = False

    last_unseen_attack: tuple[str, int] | None = None
    previous_health: int | None = None
    observed_loss: Counter[str] = field(default_factory=Counter)
    observed_loss_by_name: Counter[str] = field(default_factory=Counter)
    tried_unknown_scroll: bool = False
    sampled_potions: set[str] = field(default_factory=set)
    pending_scroll_trial: tuple[Result, str] | None = None
    stalled_scrolls: set[tuple[int, str]] = field(default_factory=set)
    sampled_scrolls: set[str] = field(default_factory=set)
    sampled_wands: set[str] = field(default_factory=set)
    guarded_supplies: set[str] = field(default_factory=set)
    tried_unmeasured_devices: set[str] = field(default_factory=set)
    attempted_slow: set[str] = field(default_factory=set)
    attempted_polymorph: set[str] = field(default_factory=set)
    resisted_polymorph: set[str] = field(default_factory=set)
    attempted_room_light: set[int] = field(default_factory=set)
    previous_losses: RestorableLosses | None = None
    draining_creatures: set[str] = field(default_factory=set)
    danger_until: dict[str, int] = field(default_factory=dict)
    recovering_from_pursuit: bool = False
    recovery_landing: str | None = None
    last_retreat_origin: str | None = None
    training_checked_at: int | None = None
    growth_locked: bool = False

    def ready_to_train(self, turn: Turn, *, stalled: bool) -> bool:
        if self.growth_locked or turn.hit_dice is None or turn.hit_dice == self.training_checked_at:
            return False
        return (not stalled and turn.can_act and not turn.hostiles and not turn.identification
                and not turn.can_resume and self.pending_stair is None)

    def train(self, turn: Turn, training: Training) -> tuple[dict, Result] | None:
        """Try one safe purchase, then let the next observation show the new build."""
        build = training.overview()
        if build.locked or not build.unspent:
            self.growth_locked = build.locked
            self.training_checked_at = turn.hit_dice
            return None
        skill = "spellcasting" if "spellcasting" in build.starting_skills else "heavy_weaponry"
        for pool, option in training_choices(training, skill):
            try:
                preview = training.preview(pool, option)
            except TrainingRefused as error:
                if error.result.status is Outcome.REJECTED and not error.result.stale:
                    continue
                raise
            if not preview.would_die:
                purchase = {"type": "purchase", "kind": option.kind.value, "id": option.identity}
                return purchase, training.confirm(preview)
        self.training_checked_at = turn.hit_dice
        return None

    def observe(self, turn: Turn, combat: CombatReport | None = None) -> None:
        here = turn.cell
        if self.corridor_stand and here and turn.can_act:
            entrance, until = self.corridor_stand
            if here.key != entrance or turn.tick >= until:
                self.corridor_stand = None
        if self.pending_end_search:
            origin, receipt = self.pending_end_search
            phase = turn.outcome_of(receipt)
            if phase is ActivityPhase.RESOLVED:
                self.completed_end_searches[origin] += 1
            if phase in (ActivityPhase.RESOLVED, ActivityPhase.FAILED, ActivityPhase.CANCELLED):
                self.pending_end_search = None
        for ground in turn.ground_items:
            if (ground.cell and (ground.item.category in DETOUR_STOCK or ground.item.category in {"armor", "ring"}) and
                    any(turn.distance_to(foe, from_offset=ground.offset) <= 1 for foe in turn.hostiles)):
                self.guarded_supplies.add(ground.cell.key)
        if self.entered_floor_at is None:
            self.entered_floor_at = turn.tick
        if self.pending_meal and turn.can_act:
            meal = self.pending_meal
            gain = meal.observed_gain(turn)
            if gain is not None:
                self.food_value[meal.identity] = min(gain, self.food_value.get(meal.identity, gain))
            self.pending_meal = None
        self.observe_combat(turn, turn.recent_combat if combat is None else combat)
        if self.pending_stair:
            attempt = self.pending_stair
            if here is None or not turn.can_act:
                return
            arrived = turn.stair_arrival(attempt)
            if arrived is None:
                return
            if arrived:
                self.record_stair_arrival(turn, attempt, attempt.landing or here.key)
            self.pending_stair = None
            self.stair_pursuers = ()
        scout = self.food_scouts.get(self.depth)
        if scout is not None and (
            turn.tick >= scout.deadline or sum(item.quantity for item in edible_items(turn)) > scout.starting_meals
        ):
            self.finish_food_scout()
        self.maps.setdefault(self.depth, FloorMap()).observe(turn)
        if here and here.lit:
            self.attempted_room_light.discard(self.depth)

    def observe_combat(self, turn: Turn, report: CombatReport) -> None:
        here = turn.cell
        health = turn.health
        if health and health.current * 5 >= health.maximum * 4:
            self.recovering_from_pursuit = False
            self.recovery_landing = None
        loss = self.previous_health - health.current if health and self.previous_health is not None else 0
        self.previous_health = health.current if health else None
        for effect in report.outgoing:
            target = effect.target
            if (target is not None and target in self.attempted_polymorph and target not in self.resisted_polymorph
                    and effect.ability == "rogue.wand_polymorph" and effect.outcome is AbilityOutcome.UNAFFECTED):
                self.attempted_polymorph.remove(target)
                self.resisted_polymorph.add(target)
        incoming = report.incoming
        self.record_injury(turn, report, loss)
        if self.danger_until:
            self.danger_until = {
                key: expiry for key, expiry in self.danger_until.items() if expiry > turn.tick
            }
        if here and (turn.hostiles or (loss > 0 and incoming)):
            self.danger_until[here.key] = turn.tick + 1200
        if not turn.hostiles:
            self.tried_unknown_scroll = False
            self.last_retreat_origin = None
        if here and any(
            event.unseen
            and (
                event.outcome is AbilityOutcome.MISS
                or (event.outcome is AbilityOutcome.APPLIED and loss > 0)
            )
            for event in incoming
        ):
            self.last_unseen_attack = here.key, turn.tick

    def record_injury(self, turn: Turn, report: CombatReport, health_loss: int) -> None:
        losses = turn.restorable_losses
        previous = self.previous_losses
        self.previous_losses = losses
        lasting_injury = losses is not None and previous is not None and losses.increased_since(previous)
        if health_loss <= 0 and not lasting_injury:
            return

        # Several attackers can contribute to the same observed change in health.
        sources = {event.caster for event in report.incoming if event.outcome is AbilityOutcome.APPLIED}
        if len(sources) != 1:
            return
        source, = sources
        if source is None:
            return
        if health_loss > 0:
            self.observed_loss[source] = max(health_loss, self.observed_loss[source])
        for foe in turn.hostiles:
            if foe.target != source:
                continue
            if health_loss > 0:
                self.observed_loss_by_name[foe.name] = max(health_loss, self.observed_loss_by_name[foe.name])
            if lasting_injury:
                self.draining_creatures.add(foe.name)

    def record_descent_timing(self, attempt: StairAttempt, arrival_tick: int) -> None:
        """Update the estimated return cost and retain the first outbound visit's duration."""
        floor = self.depth
        memory = self.maps[floor]
        steps_to_upstairs = memory.distance(attempt.origin, memory.stairs(Direction.UP))
        if steps_to_upstairs is not None:
            ticks_per_step = arrival_tick - attempt.tick
            self.ascent_ticks_by_floor[floor] = (steps_to_upstairs + 1) * ticks_per_step
        if self.entered_floor_at is not None:
            self.descent_ticks_by_floor.setdefault(floor, arrival_tick - self.entered_floor_at)

    def record_stair_arrival(self, turn: Turn, attempt: StairAttempt, landing: str) -> None:
        if self.stair_pursuers:
            if attempt.direction is Direction.UP and len(self.stair_pursuers) == 1:
                self.last_guarded_landing = attempt.origin, self.stair_pursuers[0]
            reverse = Direction.UP if attempt.direction is Direction.DOWN else Direction.DOWN
            self.unsafe_stairs[landing, reverse] = self.danger_until.get(
                attempt.origin, turn.tick)
        self.finish_food_scout()
        self.cancelled_stair_origin = None
        if attempt.direction is Direction.DOWN:
            self.record_descent_timing(attempt, turn.tick)
            self.depth += 1
        else:
            self.depth -= 1
            health = turn.health
            self.recovering_from_pursuit = bool(
                health
                and health.current * 5 < health.maximum * 4
                and not self.carrying_objective(turn)
            )
        self.recovery_landing = (
            landing if attempt.direction is Direction.UP and self.recovering_from_pursuit else None
        )
        self.entered_floor_at = turn.tick

    def admitted(self, turn: Turn, action: Action, receipt: Result | None = None,
                 *, reason: str | None = None) -> None:
        if reason == MemoryReason.CANCEL_STALLED_SCROLL and self.pending_scroll_trial:
            self.stalled_scrolls.add((self.depth, self.pending_scroll_trial[1]))
        self.pending_scroll_trial = None
        if reason == MemoryReason.CANCEL_STALLED_STAIR and self.pending_stair:
            self.cancelled_stair_origin = self.pending_stair.origin
            self.wait_after_cancel = True
        elif reason == MemoryReason.WAIT_AFTER_CANCEL:
            self.wait_after_cancel = False
        if isinstance(action, Zap):
            self.remember_zap(turn, action, reason)
        if isinstance(action, Move):
            self.remember_move(turn, action, receipt, reason)
        if isinstance(action, UseItem) and action.use is ItemUse.DRINK and action.item.identity is None:
            self.sampled_potions.add(trial_appearance(action.item))
        if turn.cell and isinstance(action, UseAbility) and action.ability.identity == "rogue.search":
            self.searches[turn.cell.key] += 1
            if receipt and self.maps[self.depth].is_dead_end(turn.cell.key):
                self.pending_end_search = turn.cell.key, receipt
            # Search covers visible walls. Credit the near and middle distance bands
            # on this floor, leaving distant or unseen walls for a closer look.
            self.wall_searches.update({
                cell.key for cell in turn.visible_cells
                if cell.wall and cell.offset[2] == 0 and max(map(abs, cell.offset)) <= 3
            })
        if isinstance(action, UseItem) and action.use is ItemUse.READ and action.item.identity is None:
            if receipt and reason in {MemoryReason.TEST_UNKNOWN_MAGIC, MemoryReason.STUDY_SCROLL_AT_EXIT}:
                self.pending_scroll_trial = receipt, action.item.target
            self.tried_unknown_scroll = True
            self.sampled_scrolls.add(trial_appearance(action.item))
        if (
            isinstance(action, UseItem)
            and action.use is ItemUse.EAT
            and action.item.identity
            and turn.nutrition
        ):
            identity = action.item.identity
            self.pending_meal = PendingMeal(identity, carried_quantity(turn, identity), turn.nutrition.reserve)
        if isinstance(action, Shot):
            self.last_shot_tick = turn.tick

    def remember_move(self, turn: Turn, action: Move, receipt: Result | None, reason: str | None) -> None:
        if turn.cell is None:
            return
        if reason == MemoryReason.SEEK_STAIRS:
            self.finish_food_scout()
        self.last_exploration_origin = turn.cell.key if reason == MemoryReason.EXPLORE else None
        self.corridor_stand = None
        if reason == MemoryReason.TAKE_CORRIDOR and (entrance := turn.cell_at(action.direction.offset)):
            # Allow pursuers to reach the choke even when the doorway breaks sight.
            self.corridor_stand = entrance.key, turn.tick + 600
        self.last_retreat_origin = turn.cell.key if reason == MemoryReason.RETREAT_TO_STAIRS else None
        if (reason in {MemoryReason.RETREAT, MemoryReason.BREAK_CONTACT} and turn.health
                and turn.health.current * 2 < turn.health.maximum):
            self.recovering_from_pursuit = True
        if action.direction in (Direction.UP, Direction.DOWN):
            self.pending_stair = turn.stair_attempt(action, receipt)
            self.stair_pursuers = (tuple(foe.target for foe in turn.hostiles)
                                   if reason in {MemoryReason.RETREAT_UPSTAIRS, MemoryReason.RETREAT_DOWNSTAIRS} else ())

    def remember_zap(self, turn: Turn, action: Zap, reason: str | None) -> None:
        if action.device.charges is None:
            self.tried_unmeasured_devices.add(action.device.item.target)
        if reason == MemoryReason.TEST_UNKNOWN_DEVICE:
            self.sampled_wands.add(trial_appearance(action.device.item))
        elif reason == MemoryReason.LIGHT_ROOM:
            self.attempted_room_light.add(self.depth)
        elif isinstance(action.target, Cell):
            targets = (foe.target for foe in turn.hostiles if action.target in turn.cells_of(foe))
            if reason == MemoryReason.SLOW_APPROACH:
                self.attempted_slow.update(targets)
            elif reason == MemoryReason.POLYMORPH_THREAT:
                self.attempted_polymorph.update(targets)

    def carrying_objective(self, turn: Turn) -> bool:
        return any(item.identity == self.objective for item in turn.inventory)

    @staticmethod
    def trapped(turn: Turn) -> bool:
        return turn.has_condition("rogue.bear_trap")

    def damage_estimate(self, foe: Creature) -> int:
        return max(self.observed_loss[foe.target], self.observed_loss_by_name[foe.name])

    def nearby_drainer(self, turn: Turn, foe: Creature) -> bool:
        # Allow one closing step before a creature can strike in contact.
        return foe.name in self.draining_creatures and turn.distance_to(foe) <= 2

    def can_spend_charge(self, device: Device) -> bool:
        if device.charges is None:
            return device.item.target not in self.tried_unmeasured_devices
        return device.charges > 0

    def combat(self, turn: Turn) -> tuple[Action, str] | None:
        health = turn.health
        injured = health is not None and health.current * 2 < health.maximum
        adjacent = [foe for foe in turn.hostiles if turn.in_contact(foe)]
        surrounded = len(adjacent) >= 2
        exposure = sum(self.damage_estimate(foe) for foe in adjacent)
        lethal = health is not None and exposure >= health.current
        two_hit_threat = health is not None and exposure * 2 >= health.current
        if injured or lethal or (surrounded and health and health.current * 4 < health.maximum * 3):
            for item in turn.inventory:
                if ItemUse.DRINK in item.uses and item.identity in {
                    "item.potion.healing",
                    "item.potion.extra-healing",
                }:
                    return UseItem(item, ItemUse.DRINK), "heal"

        if turn.hostiles and not lethal and turn.hunger in {"weak", "empty"}:
            if meal := choose_meal(turn, self.food_value):
                return UseItem(meal, ItemUse.EAT), "eat"

        if lethal:
            for item in turn.inventory:
                if item.identity == "item.scroll.teleportation" and ItemUse.READ in item.uses:
                    return UseItem(item, ItemUse.READ), "escape_pursuit"

        if adjacent and (two_hit_threat or injured or surrounded):
            if zap := self.aim_combat_wand(turn, adjacent, {"item.wand.wand-teleport-away"}):
                return zap, "displace_threat"

        if adjacent and health and (decision := self.counter_threat(turn, health.current, in_melee=True)):
            return decision
        if decision := self.fight_stair_guard(turn, adjacent):
            return decision
        distances = [(foe, turn.distance_to(foe)) for foe in turn.hostiles]
        close_pursuit = bool(adjacent) and sum(distance <= 2 for _, distance in distances) >= 2
        if escape := self.retreat_to_stairs(turn, pressed=injured or close_pursuit or two_hit_threat):
            return escape
        pursuers = [(foe, distance) for foe, distance in distances if distance <= 3]
        approaching_group = not adjacent and len(pursuers) >= 2
        if approaching_group and not self.trapped(turn):
            if step := self.take_corridor(turn, pursuers):
                return step, MemoryReason.TAKE_CORRIDOR
        if turn.hostiles and not adjacent and health and (decision := self.counter_threat(turn, health.current)):
            return decision

        if (approaching_group and health and not injured and not turn.has_condition("rogue.hasted")
                and not turn.has_condition("rogue.haste_exhaustion")):
            for item in turn.inventory:
                if item.identity == "item.potion.haste" and ItemUse.DRINK in item.uses:
                    return UseItem(item, ItemUse.DRINK), "combat_haste"

        if (
            turn.hostiles
            and (
                injured
                or self.recovering_from_pursuit
                or surrounded
                or two_hit_threat
                or any(self.nearby_drainer(turn, foe) for foe in turn.hostiles)
            )
            and not self.trapped(turn)
            and not turn.holds
        ):
            if decision := self.evade(turn):
                return decision

        if turn.hostiles:
            if zap := self.aim_combat_wand(turn, turn.hostiles, ELEMENTAL_WANDS, reflected_ray_range=ELEMENTAL_RAY_RANGE):
                return zap, "elemental_ray"

        if turn.hostiles and (trial := self.try_unknown_wand(turn)):
            return trial, MemoryReason.TEST_UNKNOWN_DEVICE

        if decision := self.ranged_combat(turn, adjacent):
            return decision

        if lethal and not self.tried_unknown_scroll:
            if trial := self.scroll_trial(turn):
                return trial

        if turn.hostiles and not self.has_arrows(turn):
            if decision := ready_weapon(turn, bow=False):
                return decision

        if attack := self.melee_attack(turn, make_space=surrounded or two_hit_threat):
            return attack, "melee"

        if decision := self.respond_to_unseen_attack(turn):
            return decision

        if turn.holds:
            return Escape(turn.holds[0]), "escape_hold"

        return None

    def fight_stair_guard(self, turn: Turn, adjacent: list[Creature]) -> tuple[Action, str] | None:
        """Fight a lone drainer when using this stair already failed to shake it."""
        if len(turn.hostiles) != 1 or not adjacent or turn.cell is None or not turn.cell.stairs_up:
            return None
        guard = adjacent[0]
        if self.last_guarded_landing != (turn.cell.key, guard.target) or guard.name not in self.draining_creatures:
            return None
        health = turn.health
        if health is None or health.current * 4 < health.maximum * 3 or self.carrying_objective(turn):
            return None
        if not 0 < self.damage_estimate(guard) * 4 < health.current:
            return None
        attack = turn.attack(guard, technique="rattling_blow") or turn.attack(guard)
        if attack is None:
            return None
        return ready_weapon(turn, bow=False) or (attack, "melee")

    def ranged_combat(self, turn: Turn, adjacent: list[Creature]) -> tuple[Action, str] | None:
        if not turn.hostiles:
            return None
        if self.has_arrows(turn):
            nearest = min(turn.distance_to(foe) for foe in turn.hostiles)
            if nearest >= 5 or nearest <= 2:
                if weapon := ready_weapon(turn, bow=nearest >= 5):
                    return weapon

        if turn.tick == self.last_shot_tick:
            return None
        can_strike = any(turn.attack(foe) is not None for foe in adjacent)
        first_throw = None
        for shot in turn.ranged_attacks(max_distance=8):
            if not shot.target.hostile:
                continue
            if shot.launcher:
                return shot, "launch_projectile"
            if first_throw is None and not can_strike and not shot.equipped_throw and shot.target.distance <= 4:
                first_throw = shot
        if first_throw is not None:
            return first_throw, "throw_projectile"
        return None

    def melee_attack(self, turn: Turn, *, make_space: bool) -> UseAbility | None:
        can_descend = self.depth < self.retrieval_depth and not self.carrying_objective(turn)

        def attack_order(foe: Creature) -> tuple[int, bool, int, int, int]:
            opens_exit = any(
                (cell.stairs_up and self.depth > 1) or
                (cell.stairs_down and can_descend)
                for cell in turn.cells_of(foe)
            )
            # Break tactical ties by threat, favoring direct evidence when foes share a species estimate.
            return (MELEE_INJURY_PRIORITY.get(foe.injury, 3), not opens_exit, turn.distance_to(foe),
                    -self.damage_estimate(foe), -self.observed_loss[foe.target])

        for foe in sorted(turn.hostiles, key=attack_order):
            if make_space and foe.injury is not Injury.NEAR_DEATH:
                push = turn.attack(foe, technique="dislocating_strike")
                if push and self.has_push_space(turn, foe):
                    return push
            attack = turn.attack(foe, technique="rattling_blow") or turn.attack(foe)
            if attack:
                return attack
        return None

    def respond_to_unseen_attack(self, turn: Turn) -> tuple[Action, str] | None:
        if turn.hostiles or self.last_unseen_attack is None:
            return None
        origin, attack_tick = self.last_unseen_attack
        if not attack_tick <= turn.tick < attack_tick + 400:
            return None

        sight_ring = "item.ring.ring-see-invisible"
        wearing_sight = any(slot.occupant and slot.occupant.identity == sight_ring for slot in turn.equipment)
        if (not wearing_sight and not turn.has_condition("rogue.see_invisible")
                and not turn.has_condition("rogue.blindness")):
            for choice in turn.equip_choices():
                if choice.item.identity == sight_ring and not choice.item.known_cursed and choice.slot.occupant is None:
                    return choice, "reveal_attacker"
            for item in turn.inventory:
                if ItemUse.DRINK in item.uses and item.identity == "item.potion.see-invisible":
                    return UseItem(item, ItemUse.DRINK), "reveal_attacker"

        if turn.holds or self.trapped(turn):
            return None
        for direction in turn.walkable_directions:
            destination = turn.cell_at(direction.offset)
            if destination is not None and destination.key != origin:
                return Move(direction), "evade_unseen_attack"
        return None

    @staticmethod
    def take_corridor(turn: Turn, pursuers: list[tuple[Creature, int]]) -> Move | None:
        """Enter a narrow passage with a clear retreat and the pursuers on the far side."""
        for direction in turn.walkable_directions:
            dx, dy, _ = direction.offset
            if dx and dy:
                continue
            if any(turn.distance_to(foe, from_offset=direction.offset) <= distance for foe, distance in pursuers):
                continue
            entrance = turn.cell_at(direction.offset)
            if entrance is None or entrance.trap or direction not in turn.walkable_from(entrance):
                continue
            flanks = [turn.cell_at((dx + sign * dy, dy + sign * dx, 0)) for sign in (-1, 1)]
            if not all(c is not None and c.wall for c in flanks):
                continue
            previous_flanks = [turn.cell_at((sign * dy, sign * dx, 0)) for sign in (-1, 1)]
            if not all(c is not None and c.wall for c in previous_flanks):
                return Move(direction)
        return None

    @staticmethod
    def has_push_space(turn: Turn, foe: Creature) -> bool:
        """Look for a clear landing directly behind a single column of body cells."""
        dx, dy, _ = foe.cells[0]
        if abs(dx) + abs(dy) != 1 or any((x, y) != (dx, dy) for x, y, _ in foe.cells):
            return False
        occupied = {offset for creature in turn.creatures for offset in creature.cells}
        for x, y, z in foe.cells:
            destination = (x + dx, y + dy, z)
            cell = turn.cell_at(destination)
            if cell is None or cell.wall or (cell.door and not cell.door.open) or destination in occupied:
                return False
        return True

    def aim_combat_wand(self, turn: Turn, foes: Iterable[Creature], identities: set[str],
                        *, reflected_ray_range: int | None = None) -> Zap | None:
        devices = [device for device in turn.devices
                   if device.item.identity in identities and self.can_spend_charge(device)]
        if not devices:
            return None
        targets = sorted(foes, key=self.damage_estimate, reverse=True)
        for device in devices:
            for foe in targets:
                if zap := turn.aim_device(device, foe, reflected_ray_range=reflected_ray_range):
                    return zap
        return None

    def try_unknown_wand(self, turn: Turn) -> Zap | None:
        health = turn.health
        if not health or health.current * 5 < health.maximum * 4 or self.carrying_objective(turn):
            return None
        if any(turn.distance_to(foe) < 3 for foe in turn.hostiles):
            return None
        for device in turn.devices:
            item = device.item
            if (item.category != "wand" or item.identity is not None or device.activation is not DeviceActivation.DIRECTION
                    or not self.can_spend_charge(device) or trial_appearance(item) in self.sampled_wands):
                continue
            for foe in turn.hostiles:
                if turn.distance_to(foe) <= 4:
                    if zap := turn.aim_device(device, foe, reflected_ray_range=ELEMENTAL_RAY_RANGE):
                        return zap
        return None

    def counter_threat(self, turn: Turn, health: int, *, in_melee: bool = False) -> tuple[Zap, str] | None:
        devices = []
        for identity, reason in CONTROL_DEVICES:
            if in_melee and reason != MemoryReason.POLYMORPH_THREAT:
                continue
            for device in turn.devices:
                if device.item.identity == identity and self.can_spend_charge(device):
                    devices.append((device, reason))
        if not devices:
            return None
        for foe in sorted(turn.hostiles, key=self.damage_estimate, reverse=True):
            damage = self.damage_estimate(foe)
            if turn.distance_to(foe) > 3:
                continue
            if 2 * damage < health and foe.name not in self.draining_creatures:
                continue
            for device, reason in devices:
                if reason == MemoryReason.SLOW_APPROACH and foe.target in self.attempted_slow:
                    continue
                if (reason == MemoryReason.POLYMORPH_THREAT
                        and not self.can_polymorph(turn, foe, device, lethal=damage >= health)):
                    continue
                if zap := turn.aim_device(device, foe, clear_horizontal_ray=True):
                    return zap, reason
        return None

    def can_polymorph(self, turn: Turn, foe: Creature, device: Device, lethal: bool) -> bool:
        if not lethal or foe.injury is not Injury.HEALTHY or foe.target in self.attempted_polymorph:
            return False
        if foe.target not in self.resisted_polymorph:
            return True
        # A witnessed resistance permits one retry with a measured charge and no other foe in contact.
        return device.charges is not None and not any(
            other.target != foe.target and turn.in_contact(other) for other in turn.hostiles
        )

    def safe_stairs(self, turn: Turn, direction: Direction) -> set[str]:
        return {key for key in self.maps[self.depth].stairs(direction)
                if self.unsafe_stairs.get((key, direction), 0) <= turn.tick
                and not (direction is Direction.DOWN and key == self.recovery_landing)}

    def retreat_to_stairs(self, turn: Turn, *, pressed: bool = False) -> tuple[Action, str] | None:
        if not turn.hostiles or self.trapped(turn) or turn.holds:
            return None
        if turn.has_condition("rogue.levitation"):
            return None
        returning = self.carrying_objective(turn)
        can_descend = self.depth < self.retrieval_depth and not returning
        downstairs = self.safe_stairs(turn, Direction.DOWN) if can_descend else set()
        onward = self.safe_stairs(turn, Direction.UP) if returning else downstairs
        if onward and (returning or pressed or self.recovering_from_pursuit):
            if turn.cell and turn.cell.key in onward:
                if returning:
                    return Move(Direction.UP), MemoryReason.RETREAT_UPSTAIRS
                return Move(Direction.DOWN), MemoryReason.RETREAT_DOWNSTAIRS
            contacts = sum(turn.in_contact(foe) for foe in turn.hostiles)
            for direction in turn.walkable_directions:
                cell = turn.cell_at(direction.offset)
                if cell is None or cell.key not in onward or cell.trap:
                    continue
                threats = [foe for foe in turn.hostiles if turn.in_contact(foe, from_offset=cell.offset)]
                exposure = sum(self.damage_estimate(foe) for foe in threats)
                if not threats or (turn.health and len(threats) <= contacts and 2 * exposure < turn.health.current):
                    return Move(direction), MemoryReason.RETREAT_TO_STAIRS
        if self.depth <= 1:
            return None
        dangerous = any(
            foe.name.casefold() in {"griffin", "dragon", "jabberwock", "wraith"}
            or self.nearby_drainer(turn, foe)
            for foe in turn.hostiles
        )
        if not (dangerous or self.recovering_from_pursuit):
            return None
        avoid = {self.last_retreat_origin} if self.last_retreat_origin is not None else set()
        exits = self.safe_stairs(turn, Direction.UP)
        if turn.cell and turn.cell.key in exits:
            return Move(Direction.UP), MemoryReason.RETREAT_UPSTAIRS
        if dangerous and downstairs:
            if turn.cell and turn.cell.key in downstairs:
                return Move(Direction.DOWN), MemoryReason.RETREAT_DOWNSTAIRS
            exits.update(downstairs)
        for foe in turn.hostiles:
            avoid.update(cell.key for cell in turn.cells_of(foe))
        memory = self.maps[self.depth]
        exits.difference_update(avoid)
        step = self.approach_visible_exit(turn, exits, avoid) or memory.toward(turn, exits, avoid=avoid)
        if step is None:
            return None
        # Opening a door spends the turn in the current cell.
        destination = step.direction.offset if isinstance(step, Move) else (0, 0, 0)
        exposure = sum(self.damage_estimate(foe) for foe in turn.hostiles
                       if turn.in_contact(foe, from_offset=destination))
        if turn.health and 2 * exposure >= turn.health.current:
            escape = self.evade(turn)
            if escape and not any(turn.in_contact(foe, from_offset=escape[0].direction.offset)
                                  for foe in turn.hostiles):
                return escape
        return step, MemoryReason.RETREAT_TO_STAIRS

    @staticmethod
    def approach_visible_exit(turn: Turn, exits: set[str], avoid: set[str]) -> Move | None:
        nearby = [cell.offset for cell in turn.visible_cells if cell.key in exits and cell.offset[2] == 0]
        if not nearby:
            return None
        destination = min(nearby, key=lambda offset: sum(map(abs, offset)))
        distance = sum(map(abs, destination))
        approaches = []
        for direction in turn.walkable_directions:
            offset = direction.offset
            candidate = turn.cell_at(offset)
            if candidate is None or candidate.key in avoid:
                continue
            remaining = sum(abs(a - b) for a, b in zip(offset, destination))
            if remaining < distance:
                clearance = min(turn.distance_to(foe, from_offset=offset) for foe in turn.hostiles)
                approaches.append((clearance, -remaining, direction))
        return Move(max(approaches)[2]) if approaches else None

    @staticmethod
    def evade(turn: Turn) -> tuple[Move, str] | None:
        def safety(offset):
            contacts = sum(turn.in_contact(foe, from_offset=offset) for foe in turn.hostiles)
            clearance = min(turn.distance_to(foe, from_offset=offset) for foe in turn.hostiles)
            return -contacts, clearance

        current = safety((0, 0, 0))
        choices = [(safety(d.offset), d) for d in turn.walkable_directions]
        if not choices:
            return None
        best, first = max(choices, key=lambda choice: choice[0])
        if best < current:
            return None
        candidates = [direction for score, direction in choices if score == best]
        if best > current and len(candidates) > 1:
            # Each candidate already improves safety. Prefer one that avoids a disclosed trap.
            clear = [direction for direction in candidates if turn.cell_at(direction.offset).trap is None]
            if clear:
                candidates = clear
                first = clear[0]
        if best > current and len(candidates) == 1:
            return Move(first), MemoryReason.RETREAT

        # Among equally safe first steps, prefer one with a safe onward move.
        best_onward = None
        for direction in candidates:
            cell = turn.cell_at(direction.offset)
            for second in turn.walkable_from(cell):
                offset = tuple(a + b for a, b in zip(direction.offset, second.offset))
                if offset == (0, 0, 0):
                    continue
                after = safety(offset)
                if after < best or after <= current:
                    continue
                # Compare trap exposure only when both routes offer the same protection from foes.
                rank = after, cell.trap is None, turn.cell_at(offset).trap is None
                if best_onward is None or rank > best_onward:
                    first, best_onward = direction, rank
        if best > current:
            return Move(first), MemoryReason.RETREAT
        if best_onward is not None:
            return Move(first), MemoryReason.BREAK_CONTACT
        return None

    def recovery_budget(self, turn: Turn, memory: FloorMap, direction: Direction) -> int | None:
        """Ticks available for rest after reserving travel, or None without a food clock."""
        nutrition = turn.nutrition
        if nutrition is None:
            return None
        available = nutrition.ticks_until_empty
        # Even a temporarily guarded stair will cost food to return through.
        if not memory.stairs(direction):
            return available
        travel = sum(ticks for floor, ticks in self.ascent_ticks_by_floor.items()
                     if direction is Direction.DOWN or floor <= self.depth)
        # Meals can pay for later travel. A rest must still fit the current food clock.
        packed = estimate_carried_food(edible_items(turn), self.food_value)
        travel_from_current_reserve = max(0, travel - packed.known_reserve * nutrition.ticks_per_unit)
        return available - travel_from_current_reserve

    def recovery_action(self, turn: Turn) -> tuple[Action, str] | None:
        if turn.cell is None:
            return None
        health, clock = turn.health, turn.recovery
        if health is None or clock is None or clock.stopped:
            return None
        missing = (health.maximum * 4 + 4) // 5 - health.current
        if missing <= 0:
            return None

        memory = self.maps[self.depth]
        returning = self.carrying_objective(turn)
        direction = Direction.UP if returning else Direction.DOWN
        stairs = self.safe_stairs(turn, direction)
        rest_budget = self.recovery_budget(turn, memory, direction)

        def can_afford_recovery(points: int) -> bool:
            ticks = clock.ticks_to_regain(points)
            return ticks is not None and (rest_budget is None or ticks < rest_budget)

        if not self.trapped(turn):
            if turn.cell.key in self.danger_until:
                if action := memory.toward(turn, memory.visited - self.danger_until.keys()):
                    return action, "reposition_to_recover"
            # Recover along the food route while scouting away from the stairs.
            food_route = None
            if not returning and self.depth in self.food_scouts:
                food_route = self.provision(turn, memory, returning)
            action = food_route[0] if food_route else memory.toward(turn, stairs)
            if isinstance(action, Move) and (cell := turn.cell_at(action.direction.offset)):
                if cell.key not in self.danger_until:
                    if food_route:
                        return food_route
                    if cell.key in memory.visited or cell.key in stairs:
                        return action, "recover_en_route"
                if (cell.key in self.danger_until and turn.cell.key not in self.danger_until
                        and can_afford_recovery(1)):
                    # Stay in shelter while a single point of recovery is still affordable.
                    return Wait(), "recover"
        if can_afford_recovery(missing):
            return Wait(), "recover"
        return None

    def prepare(self, turn: Turn) -> tuple[Action, str] | None:
        if turn.restorable_losses and turn.restorable_losses.strength:
            for item in turn.inventory:
                if item.identity == "item.potion.restore-strength" and ItemUse.DRINK in item.uses:
                    return UseItem(item, ItemUse.DRINK), "restore_strength"
        if upgrade := choose_armor(turn) or choose_ring(turn):
            return upgrade
        if action := self.recovery_action(turn):
            return action
        for item in turn.inventory:
            if ItemUse.DRINK in item.uses and item.identity in {
                "item.potion.gain-strength",
                "item.potion.raise-level",
            }:
                return UseItem(item, ItemUse.DRINK), "permanent_growth"
            if ItemUse.READ in item.uses and item.identity in IDENTIFY_CATEGORIES:
                categories = IDENTIFY_CATEGORIES[item.identity]
                if any(other.category in categories and needs_identification(other) for other in turn.inventory):
                    return UseItem(item, ItemUse.READ), "identify_gear"
        if improvement := improve_worn_gear(turn):
            return improvement
        health = turn.health
        can_experiment = bool(health and health.current * 5 >= health.maximum * 4 and not self.carrying_objective(turn))
        if can_experiment:
            for item in turn.inventory:
                if (item.identity is None and ItemUse.DRINK in item.uses
                        and trial_appearance(item) not in self.sampled_potions):
                    return UseItem(item, ItemUse.DRINK), "test_unknown_potion"
        if weapon := ready_weapon(turn, bow=False):
            return weapon
        if can_experiment:
            return self.study_scroll_at_exit(turn)
        return None

    def study_scroll_at_exit(self, turn: Turn) -> tuple[UseItem, MemoryReason] | None:
        """Try an unidentified scroll with a usable escape stair and something to identify."""
        here = turn.cell
        if not 3 <= self.depth < self.retrieval_depth or here is None or not here.stairs_down:
            return None
        if here.key in self.danger_until or self.unsafe_stairs.get((here.key, Direction.DOWN), 0) > turn.tick:
            return None
        if turn.holds or self.trapped(turn) or turn.has_condition("rogue.levitation"):
            return None
        if any(item.identity is None and item.category in {"wand", "ring", "potion"} for item in turn.inventory):
            return self.scroll_trial(turn, at_exit=True)
        return None

    @staticmethod
    def has_arrows(turn: Turn) -> bool:
        return any(item.identity == "item.weapon.arrow" for item in turn.inventory)

    @staticmethod
    def ready_search(turn: Turn) -> UseAbility | None:
        return next((UseAbility(ability) for ability in turn.abilities
                     if ability.identity == "rogue.search" and ability.ready), None)

    def search_dead_end(self, turn: Turn, memory: FloorMap) -> UseAbility | None:
        here = turn.cell
        if (here is None or here.stairs_up or here.stairs_down or turn.hostiles
                or self.completed_end_searches[here.key] >= DEAD_END_SEARCHES
                or not memory.is_dead_end(here.key)):
            return None
        return self.ready_search(turn)

    def search_return_connection(self, turn: Turn, memory: FloorMap, upstairs: set[str]) -> tuple[Action, str] | None:
        """Search where one opening could reconnect the route to a remembered staircase."""
        search = self.ready_search(turn)
        if turn.cell is None or search is None:
            return None
        sites: set[str] = set()
        for stair in upstairs:
            passages = {c.key for c, _ in memory.reachable(stair)}
            if turn.cell.key in passages:
                continue
            for key in passages:
                for wall in memory.adjacent(key):
                    if wall.wall:
                        sites.update(c.key for c in memory.adjacent(wall.key)
                                     if not c.wall and c.key not in passages and self.searches[c.key] < 3)
        if turn.cell.key in sites:
            return search, "search"
        if step := memory.toward(turn, sites):
            return step, "search"
        return None

    def search(self, turn: Turn, memory: FloorMap, returning: bool, *,
               allow_unseen: bool = True, wall_limit: int = 3) -> tuple[Action, str] | None:
        if returning and (action := self.search_dead_end(turn, memory)):
            return action, "search"
        search = self.ready_search(turn)
        if search is None or turn.cell is None:
            return None
        for key in self.ranked_search_sites(turn, memory, returning, allow_unseen, wall_limit):
            if key == turn.cell.key:
                return search, "search"
            if step := memory.toward(turn, {key}):
                return step, "search"
        return None

    def ranked_search_sites(self, turn: Turn, memory: FloorMap, returning: bool,
                           allow_unseen: bool, wall_limit: int) -> list[str]:
        sites = []
        for site, distance in memory.reachable(turn.cell.key):
            attempts = self.searches[site.key]
            neighbors = memory.adjacent(site.key)
            walls = [c for c in neighbors if c.wall]
            dead_end = memory.is_dead_end(site.key) and not (site.stairs_down or site.stairs_up)
            if dead_end:
                remaining = DEAD_END_SEARCHES - self.completed_end_searches[site.key]
                if remaining <= 0 or turn.hostiles:
                    continue
                untested = walls
                value = remaining * len(walls)
            else:
                if attempts > wall_limit:
                    continue
                untested = [c for c in walls if attempts < wall_limit and self.wall_searches[c.key] < wall_limit]
                value = sum(wall_limit - self.wall_searches[c.key] for c in untested)
            if not untested and (not allow_unseen or len(neighbors) == 4):
                continue
            near_exit = returning and any(
                n.stairs_up for wall in untested for n in memory.adjacent(wall.key)
            )
            # One search checks several boundaries. Favor corridor ends, especially near a lost exit.
            rank = SearchRank(
                near_return_stairs=near_exit,
                has_searchable_walls=bool(untested),
                value_after_travel=value - distance * 2,
                negative_distance=-distance,
            )
            sites.append((rank, site.key))
        # Equal ranks retain traversal order, independent of opaque cell keys.
        sites.sort(key=lambda site: site[0], reverse=True)
        return [key for _, key in sites]

    def estimated_journey_ticks(self, returning: bool) -> int:
        known_return = sum(ticks for floor, ticks in self.ascent_ticks_by_floor.items() if floor < self.depth)
        floors_ahead = self.retrieval_depth - self.depth
        if returning or floors_ahead <= 0 or not self.descent_ticks_by_floor:
            return known_return

        recent_floors = sorted(self.descent_ticks_by_floor)[-5:]
        round_trip = median_low(
            self.descent_ticks_by_floor[floor] + self.ascent_ticks_by_floor.get(floor, 0)
            for floor in recent_floors
        )
        return known_return + floors_ahead * round_trip

    def route_to_food(self, turn: Turn, memory: FloorMap) -> Action | None:
        food_cells = {key for key, items in memory.items.items() if any(item.category == "food" for item in items)}
        if food_cells and self.danger_until:
            if step := memory.toward(turn, food_cells, avoid=self.danger_until.keys()):
                return step
        return memory.toward(turn, food_cells)

    def finish_food_scout(self) -> None:
        """A completed scout stays closed on later visits to the same floor."""
        if self.depth in self.food_scouts:
            self.food_scouts[self.depth] = None

    def provision(self, turn: Turn, memory: FloorMap, returning: bool) -> tuple[Action, str] | None:
        nutrition = turn.nutrition
        if nutrition is None or turn.has_condition("rogue.levitation"):
            return None
        required = self.estimated_journey_ticks(returning)
        if nutrition.ticks_until_empty >= required:
            return None
        meals = list(edible_items(turn))
        packed = estimate_carried_food(meals, self.food_value)
        if packed.has_unmeasured_meals:
            return None
        available_ticks = nutrition.ticks_until_empty + packed.known_reserve * nutrition.ticks_per_unit
        if available_ticks >= required:
            return None
        if step := self.route_to_food(turn, memory):
            return step, "forage"
        if returning or self.depth >= self.retrieval_depth:
            return None
        if self.depth not in self.food_scouts:
            if turn.cell is None or not turn.cell.stairs_down:
                return None
            self.food_scouts[self.depth] = FoodScout(
                deadline=turn.tick + nutrition.ticks_until_empty // 4,
                starting_meals=sum(item.quantity for item in meals),
            )
        scout = self.food_scouts[self.depth]
        if scout is not None and turn.tick < scout.deadline:
            if step := memory.toward(turn, memory.frontier):
                return step, "scout_for_food"
        return None

    def scroll_trial(self, turn: Turn, *, at_exit: bool = False) -> tuple[UseItem, MemoryReason] | None:
        for item in turn.inventory:
            if item.identity is not None or ItemUse.READ not in item.uses:
                continue
            if (self.depth, item.target) in self.stalled_scrolls:
                continue
            if at_exit and trial_appearance(item) in self.sampled_scrolls:
                continue
            reason = MemoryReason.STUDY_SCROLL_AT_EXIT if at_exit else MemoryReason.TEST_UNKNOWN_MAGIC
            return UseItem(item, ItemUse.READ), reason
        return None

    def stalled(self, turn: Turn, previous: Result | None) -> tuple[Action | None, str]:
        if (previous is None or not turn.can_cancel or len(turn.activities) != 1
                or turn.outcome_of(previous) is not ActivityPhase.QUEUED):
            return None, "readiness_timeout"
        if self.pending_scroll_trial and self.pending_scroll_trial[0] == previous:
            return ActivityChoice.CANCEL, MemoryReason.CANCEL_STALLED_SCROLL
        stair = self.pending_stair
        if stair and stair.receipt == previous and stair.origin != self.cancelled_stair_origin:
            return ActivityChoice.CANCEL, MemoryReason.CANCEL_STALLED_STAIR
        return None, "readiness_timeout"

    def loot_goals(self, turn: Turn, ground_items: tuple[GroundItem, ...], returning: bool) -> LootGoals:
        goals = LootGoals()
        carried: Counter[str | None] = Counter()
        stock: Counter[str] = Counter()
        for item in turn.inventory:
            carried[item.identity] += item.quantity
            stock[item.category] += item.quantity
        spare_armor_to_identify = any(
            choice.slot.kind == "body_armor" and choice.item.physical_reduction is None
            for choice in turn.equip_choices()
        )
        safe_pickup = not turn.hostiles or bool(
            turn.health and turn.health.current * 5 >= turn.health.maximum * 4
            and all(turn.distance_to(foe) > 1 for foe in turn.hostiles)
        )
        goals.recover_arrows = (not returning and not turn.hostiles and not turn.has_condition("rogue.levitation")
                                and carried["item.weapon.shortbow"] > 0)
        for ground in sorted(ground_items, key=lambda row: row.item.identity != self.objective):
            item = ground.item
            useful = item.category in LOOT_CLASSES or item.identity == "item.misc.clean-bandage"
            if ground.reachable:
                if item.identity == self.objective or (safe_pickup and useful):
                    goals.pickup = item
                    break
                continue
            wanted = item.identity == self.objective or item.category == "food"
            if item.identity in {"item.potion.healing", "item.potion.extra-healing"}:
                wanted = carried[item.identity] < 2
            elif not returning and not turn.hostiles and ground.cell and ground.cell.key not in self.guarded_supplies:
                if useful and not turn.cell.stairs_down:
                    goals.nearby.add(ground.cell.key)
                seek_armor = item.category == "armor" and not spare_armor_to_identify and (
                    item.identity is None or not carried[item.identity]
                )
                seek_ring = (item.category == "ring" and item.identity is not None and not carried[item.identity]
                             and not item.known_cursed and item.identity in ring_upgrades(turn))
                if seek_armor or seek_ring or stock[item.category] < DETOUR_STOCK.get(item.category, 0):
                    goals.supplies.add(ground.cell.key)
            # Outbound arrows use the same recovery radius after leaving sight.
            if returning and item.identity == "item.weapon.arrow" and carried["item.weapon.shortbow"]:
                wanted = not turn.hostiles and carried[item.identity] < 8
            if not returning:
                wanted |= item.identity == "item.weapon.dart" and carried[item.identity] < 12
                wanted |= item.identity == "item.weapon.spear" and not carried[item.identity]
            if wanted and ground.cell:
                goals.wanted.add(ground.cell.key)
        return goals

    def route_to_loot(self, turn: Turn, memory: FloorMap, goals: LootGoals) -> tuple[Action, str] | None:
        if action := memory.toward(turn, goals.wanted):
            return action, "seek_item"
        if goals.recover_arrows:
            arrows = {key for key, items in memory.items.items()
                      if any(item.identity == "item.weapon.arrow" for item in items)}
            if arrows and (action := memory.toward(turn, arrows, avoid=self.danger_until.keys(), max_steps=6)):
                return action, "recover_arrows"
        if goals.nearby and (action := memory.toward(turn, goals.nearby, max_steps=2)):
            return action, "seek_supplies"
        if goals.supplies and (action := memory.toward(turn, goals.supplies, max_steps=3)):
            return action, "seek_supplies"
        return None

    def choose(self, turn: Turn) -> tuple[Action | None, str]:
        if turn.needs_identification and (prompt := turn.identification):
            unknown = next((item for item in prompt.items if needs_identification(item)), None)
            return prompt.select(unknown), "identify"
        if turn.can_resume:
            return ActivityChoice.RESUME, "resume"
        if not turn.can_act or turn.cell is None:
            return None, "busy"
        if self.pending_stair:
            return None, "uncertain_stair_landing"
        if turn.falling:
            return Wait(), "settle_fall"
        if self.wait_after_cancel:
            return Wait(), MemoryReason.WAIT_AFTER_CANCEL

        if decision := self.combat(turn):
            return decision

        if self.corridor_stand and turn.hunger not in {"weak", "empty"}:
            entrance, until = self.corridor_stand
            if turn.cell.key == entrance and turn.tick < until:
                return Wait(), "hold_corridor"

        memory = self.maps[self.depth]
        trapped = self.trapped(turn)
        hovering = turn.has_condition("rogue.levitation")
        ground_items = () if hovering else turn.ground_items
        if turn.hunger in {"hungry", "weak", "empty"}:
            if meal := choose_meal(turn, self.food_value):
                return UseItem(meal, ItemUse.EAT), "eat"
            for ground in ground_items:
                if ground.reachable and ground.item.category == "food":
                    return Pickup(ground.item), "take"
            if not hovering and not trapped and (step := self.route_to_food(turn, memory)):
                return step, "forage"

        returning = self.carrying_objective(turn)
        loot = self.loot_goals(turn, ground_items, returning)
        if loot.pickup is not None:
            return Pickup(loot.pickup), "take"
        if not returning and not hovering and not turn.hostiles and not trapped:
            objective_cells = {key for key, items in memory.items.items()
                               if any(item.identity == self.objective for item in items)}
            if objective_cells and (step := memory.toward(turn, objective_cells, avoid=self.danger_until.keys())):
                return step, "seek_objective"
        if not turn.hostiles and (decision := self.prepare(turn)):
            return decision
        if trapped:
            if not turn.holds and any(c.identity == "rogue.bear_trap" and c.active
                                      and c.remaining_ticks is None for c in turn.conditions):
                # While pinned, any Move is a paid attempt in place. Do not
                # turn it into Wait just because there is no walkable tile.
                return Move(Direction.NORTH), "escape_bear_trap"
            return Wait(), "wait_for_restraint"
        if not returning and not any(g.item.identity == self.objective for g in ground_items):
            if search := self.search_dead_end(turn, memory):
                return search, "search"
        if decision := self.route_to_loot(turn, memory, loot):
            return decision

        if not turn.hostiles and (decision := self.provision(turn, memory, returning)):
            return decision

        direction = Direction.UP if returning else Direction.DOWN
        if returning or self.depth < self.retrieval_depth:
            stairs = self.safe_stairs(turn, direction)
            if turn.cell.key in stairs:
                if hovering:
                    return Wait(), "wait_for_ground_contact"
                return Move(direction), "ascend" if returning else "descend"
            if action := memory.toward(turn, stairs):
                return action, MemoryReason.SEEK_STAIRS
            if returning and stairs and not turn.hostiles:
                if link := self.search_return_connection(turn, memory, stairs):
                    return link

        if (turn.cell.lit is False and not turn.hostiles and self.depth not in self.attempted_room_light
                and not turn.has_condition("rogue.blindness")):
            for device in turn.devices:
                if (device.item.identity == "item.wand.wand-light" and device.activation is DeviceActivation.SELF_TARGET
                        and device.charges != 0):
                    return Zap(device, None), MemoryReason.LIGHT_ROOM
        avoid = self.danger_until.keys()
        if action := memory.toward_unexplored(turn, avoid=avoid):
            if isinstance(action, Move) and (next_cell := turn.cell_at(action.direction.offset)):
                if next_cell.key == self.last_exploration_origin:
                    alternate = memory.toward(turn, memory.frontier, avoid=avoid | {next_cell.key}, max_steps=3)
                    if alternate:
                        return alternate, MemoryReason.EXPLORE
            return action, MemoryReason.EXPLORE
        if avoid:
            # Accept the risk when every remaining route crosses a recent threat.
            if action := memory.toward_unexplored(turn):
                return action, MemoryReason.EXPLORE
        fallback = None
        if returning and memory.stairs(Direction.UP):
            for item in turn.inventory:
                if item.identity == "item.scroll.teleportation" and ItemUse.READ in item.uses:
                    fallback = UseItem(item, ItemUse.READ), "find_return_route"
                    break
        if fallback is None:
            fallback = self.scroll_trial(turn)
        if decision := self.search(turn, memory, returning, allow_unseen=fallback is None) or fallback:
            return decision
        if (not returning and not turn.hostiles and memory.stairs(Direction.UP)
                and not memory.stairs(Direction.DOWN)):
            # A failed Search leaves concealed doors looking like ordinary walls.
            if decision := self.search(turn, memory, returning, allow_unseen=False, wall_limit=6):
                return decision
        return None, "no_disclosed_route"


def __getattr__(name: str):
    """Support callers that imported runner functions from the original policy module."""
    if name in {"play", "trace_result"}:
        import rogue_runner
        return getattr(rogue_runner, name)
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
