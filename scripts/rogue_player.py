"""Rogue retrieval strategy using the ordinary Python player interface."""

from __future__ import annotations

from collections import Counter
from collections.abc import Iterable, Iterator
from dataclasses import dataclass, field
from enum import StrEnum
from functools import cache
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
    Health,
    IncomingAbility,
    Injury,
    Item,
    ItemUse,
    Move,
    OutgoingAbility,
    Pickup,
    RememberedCell,
    RestorableLosses,
    Result,
    StairAttempt,
    Shot,
    Turn,
    Training,
    UseAbility,
    UseItem,
    Wait,
    Zap,
)

from rogue_combat import includes_self_damage, sole_applied_attacker, witnessed_unseen_attack
from rogue_loot import (
    DETOUR_STOCK, HEALING_POTIONS, LootGoals, loot_goals, preferred_pickup, route_to_loot,
)
from rogue_training import purchase_training
from rogue_trials import PotionTrials, ScrollTrials, WandTrials
from rogue_retreat import (
    approach_visible_exit, best_onward_escape, converging_pursuers, evade_unseen_attacker, route_around_danger, take_corridor,
)
from rogue_food import (
    MEAL_HUNGER, URGENT_HUNGER, FoodScout, PendingMeal, choose_meal, edible_items, estimate_carried_food,
    estimate_round_trip_ticks,
)
from rogue_gear import (
    can_identify_carried_item, choose_armor, choose_ring, improve_worn_gear,
    identification_target, offered_item_use, ready_weapon, reveal_unseen_attacker,
)


MELEE_INJURY_PRIORITY = {
    Injury.NEAR_DEATH: 3,
    Injury.BADLY_WOUNDED: 2,
    Injury.WOUNDED: 1,
}
DEAD_END_SEARCHES = 6
RECENT_DANGER_TICKS = 1200
ELEMENTAL_WANDS = {"item.wand.wand-fire", "item.wand.wand-cold", "item.wand.wand-lightning"}
ELEMENTAL_RAY_RANGE = 6


class PolymorphState(StrEnum):
    ATTEMPTED = "attempted"
    RESISTED = "resisted"
    RETRIED = "retried"


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


def choose_healing_potion(turn: Turn, *, lethal: bool) -> Item | None:
    """Prefer the larger healing roll when nearby enemies could kill the player."""
    doses = (item for item in turn.inventory if item.identity in HEALING_POTIONS and ItemUse.DRINK in item.uses)
    if lethal:
        return max(doses, key=lambda item: item.identity == "item.potion.extra-healing", default=None)
    return next(doses, None)


def recovery_deficit(health: Health) -> int:
    """Hit points needed to reach four fifths of maximum health, rounded up."""
    target = (4 * health.maximum + 4) // 5
    return max(0, target - health.current)


def choose_projectile(shots: Iterable[Shot], *, can_strike: bool) -> Shot | None:
    """Prefer launched shots, then short range, preserving offer order on ties."""
    best_shot = None
    best_rank = None
    for shot in shots:
        if not shot.target.hostile:
            continue
        distance = max(map(abs, shot.offset))
        launched = shot.launcher is not None
        if not launched and (can_strike or shot.equipped_throw or distance > 4):
            continue
        in_normal_range = shot.range is None or distance <= shot.range.short_cells
        if launched and in_normal_range:
            return shot
        rank = launched, in_normal_range
        if best_rank is None or rank > best_rank:
            best_shot, best_rank = shot, rank
    return best_shot


@dataclass
class MeleeExposure:
    """Visible foes in reach of a square and their combined estimated damage."""

    foes: list[Creature]
    damage: int

    def threatens(self, health: Health | None, *, rounds: int) -> bool:
        return health is not None and self.damage * rounds >= health.current


class MeleeRank(NamedTuple):
    """Higher fields win in order. Equal ranks preserve the observed target order."""

    injury_priority: int
    opens_exit: bool
    negative_distance: int
    estimated_damage: int
    damage_from_this_foe: int


def is_searchable_dead_end(site: Cell | RememberedCell, memory: FloorMap) -> bool:
    return not (site.stairs_up or site.stairs_down) and memory.is_dead_end(site.key)


class SearchRank(NamedTuple):
    """Higher fields win, in declaration order. Tied sites share a routing query."""

    near_return_stairs: bool
    has_searchable_walls: bool
    value_after_travel: int
    negative_distance: int


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
    cancelled_stairs: set[tuple[str, Direction]] = field(default_factory=set)
    wait_after_cancel: bool = False

    last_unseen_attack: tuple[str, int] | None = None
    previous_health: int | None = None
    observed_loss: Counter[str] = field(default_factory=Counter)
    observed_loss_by_name: Counter[str] = field(default_factory=Counter)
    potion_trials: PotionTrials = field(default_factory=PotionTrials)
    scroll_trials: ScrollTrials = field(default_factory=ScrollTrials)
    wand_trials: WandTrials = field(default_factory=WandTrials)
    supplies_guarded_until: dict[str, int] = field(default_factory=dict)
    attempted_slow: set[str] = field(default_factory=set)
    polymorph_by_target: dict[str, PolymorphState] = field(default_factory=dict)
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
        if purchase := purchase_training(training, build.starting_skills):
            return purchase
        self.training_checked_at = turn.hit_dice
        return None

    def observe(self, turn: Turn, combat: CombatReport | None = None) -> None:
        here = turn.cell
        if self.corridor_stand and here and turn.can_act:
            entrance, until = self.corridor_stand
            if here.key != entrance or turn.tick >= until:
                self.corridor_stand = None
        self.observe_end_search(turn)
        self.potion_trials.observe(turn)
        self.scroll_trials.observe(turn)
        self.expire_danger_warnings(turn.tick)
        self.observe_guarded_supplies(turn)
        if self.entered_floor_at is None:
            self.entered_floor_at = turn.tick
        self.observe_meal(turn)
        self.observe_combat(turn, turn.recent_combat if combat is None else combat)
        self.observe_stair(turn)
        if self.pending_stair:
            return
        scout = self.food_scouts.get(self.depth)
        if scout is not None and scout.finished(turn):
            self.finish_food_scout()
        if self.depth not in self.maps:
            self.maps[self.depth] = FloorMap()
        self.maps[self.depth].observe(turn)
        if here and here.lit:
            self.attempted_room_light.discard(self.depth)

    def expire_danger_warnings(self, tick: int) -> None:
        for warnings in (self.danger_until, self.supplies_guarded_until, self.unsafe_stairs):
            if warnings:
                expired = [key for key, deadline in warnings.items() if deadline <= tick]
                for key in expired:
                    del warnings[key]

    def observe_guarded_supplies(self, turn: Turn) -> None:
        guard_cells = [cell for foe in turn.hostiles for cell in foe.cells]
        if not guard_cells:
            return
        for ground in turn.ground_items:
            category = ground.item.category
            if ground.cell is None or (category not in DETOUR_STOCK and category not in {"armor", "ring"}):
                continue
            x, y, z = ground.offset
            if any(abs(x - gx) <= 1 and abs(y - gy) <= 1 and abs(z - gz) <= 1 for gx, gy, gz in guard_cells):
                self.supplies_guarded_until[ground.cell.key] = turn.tick + RECENT_DANGER_TICKS

    def observe_combat(self, turn: Turn, report: CombatReport) -> None:
        here = turn.cell
        health = turn.health
        if health and recovery_deficit(health) == 0:
            self.recovering_from_pursuit = False
            self.recovery_landing = None
        loss = self.previous_health - health.current if health and self.previous_health is not None else 0
        self.previous_health = health.current if health else None
        losses = turn.restorable_losses
        previous = self.previous_losses
        self.previous_losses = losses
        lasting_injury = losses is not None and previous is not None and losses.increased_since(previous)
        injured_this_turn = loss > 0 or lasting_injury
        outgoing = report.outgoing
        self.observe_polymorph_resistance(outgoing)
        attributable_loss = 0 if includes_self_damage(outgoing, turn.self_target) else loss
        incoming = report.incoming
        self.record_injury(turn, incoming, attributable_loss, lasting_injury=lasting_injury)
        if here and (turn.hostiles or (injured_this_turn and incoming)):
            self.danger_until[here.key] = turn.tick + RECENT_DANGER_TICKS
        if not turn.hostiles:
            self.scroll_trials.tried_in_encounter = False
            self.last_retreat_origin = None
        if here and witnessed_unseen_attack(incoming, turn.hostiles, injured=injured_this_turn):
            self.last_unseen_attack = here.key, turn.tick

    def observe_polymorph_resistance(self, outgoing: tuple[OutgoingAbility, ...]) -> None:
        for effect in outgoing:
            if effect.ability != "rogue.wand_polymorph" or effect.outcome is not AbilityOutcome.UNAFFECTED:
                continue
            target = effect.target
            if target is not None and self.polymorph_by_target.get(target) is PolymorphState.ATTEMPTED:
                self.polymorph_by_target[target] = PolymorphState.RESISTED

    def record_injury(self, turn: Turn, incoming: tuple[IncomingAbility, ...], health_loss: int,
                      *, lasting_injury: bool) -> None:
        if health_loss <= 0 and not lasting_injury:
            return

        source = sole_applied_attacker(incoming)
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

    def observe_stair(self, turn: Turn) -> None:
        """Resolve a pending stair from a ready or terminal picture with landing evidence."""
        attempt = self.pending_stair
        if attempt is None or turn.cell is None or not (turn.can_act or turn.terminal):
            return
        arrived = turn.stair_arrival(attempt)
        if arrived is None:
            return
        if arrived:
            self.record_stair_arrival(turn, attempt, attempt.landing or turn.cell.key)
        self.pending_stair = None
        self.stair_pursuers = ()

    def record_stair_arrival(self, turn: Turn, attempt: StairAttempt, landing: str) -> None:
        if self.last_unseen_attack and turn.cell and self.last_unseen_attack[0] != turn.cell.key:
            self.last_unseen_attack = None
        if self.stair_pursuers:
            if attempt.direction is Direction.UP and len(self.stair_pursuers) == 1:
                self.last_guarded_landing = attempt.origin, self.stair_pursuers[0]
            reverse = Direction.UP if attempt.direction is Direction.DOWN else Direction.DOWN
            self.unsafe_stairs[landing, reverse] = self.danger_until.get(
                attempt.origin, turn.tick)
        self.finish_food_scout()
        self.cancelled_stairs.clear()
        if attempt.direction is Direction.DOWN:
            self.record_descent_timing(attempt, turn.tick)
            self.depth += 1
        else:
            self.depth -= 1
            health = turn.health
            self.recovering_from_pursuit = bool(
                health
                and recovery_deficit(health) > 0
                and not self.carrying_objective(turn)
            )
        self.recovery_landing = (
            landing if attempt.direction is Direction.UP and self.recovering_from_pursuit else None
        )
        self.entered_floor_at = turn.tick

    def admitted(self, turn: Turn, action: Action, receipt: Result | None = None,
                 *, reason: str | None = None) -> None:
        if reason == MemoryReason.CANCEL_STALLED_SCROLL:
            self.scroll_trials.interrupted(turn)
        if reason == MemoryReason.CANCEL_STALLED_STAIR and self.pending_stair:
            self.cancelled_stairs.add((self.pending_stair.origin, self.pending_stair.direction))
            self.wait_after_cancel = True
        elif reason == MemoryReason.WAIT_AFTER_CANCEL:
            self.wait_after_cancel = False
        if isinstance(action, Zap):
            self.remember_zap(turn, action, reason)
        if isinstance(action, Move):
            self.remember_move(turn, action, receipt, reason)
        if isinstance(action, UseItem):
            self.remember_item_use(turn, action, receipt, reason)
        if isinstance(action, UseAbility) and action.ability.identity == "rogue.search" and turn.cell:
            self.remember_search(turn, turn.cell, receipt)
        if isinstance(action, Shot):
            self.last_shot_tick = turn.tick

    def remember_item_use(self, turn: Turn, action: UseItem, receipt: Result | None, reason: str | None) -> None:
        item = action.item
        if action.use is ItemUse.DRINK and item.identity is None:
            self.potion_trials.admitted(turn, item, receipt)
        elif action.use is ItemUse.READ and item.identity is None:
            trial = reason in {MemoryReason.TEST_UNKNOWN_MAGIC, MemoryReason.STUDY_SCROLL_AT_EXIT}
            self.scroll_trials.admitted(turn, self.depth, item, receipt if trial else None)
        elif action.use is ItemUse.EAT:
            self.pending_meal = PendingMeal.before_eating(turn, item, receipt)

    def observe_meal(self, turn: Turn) -> None:
        """Keep the lowest measured gain for each food after the player can act again."""
        meal = self.pending_meal
        if meal is None:
            return
        phase = turn.outcome_of(meal.receipt) if meal.receipt else None
        if phase in (ActivityPhase.FAILED, ActivityPhase.CANCELLED):
            self.pending_meal = None
            return
        if not turn.can_act or phase not in (None, ActivityPhase.RESOLVED):
            return
        gain = meal.observed_gain(turn)
        if gain is not None:
            self.food_value[meal.identity] = min(gain, self.food_value.get(meal.identity, gain))
        self.pending_meal = None

    def observe_end_search(self, turn: Turn) -> None:
        """Only a completed search spends one of the attempts budgeted for a dead end."""
        if self.pending_end_search is None:
            return
        origin, receipt = self.pending_end_search
        phase = turn.outcome_of(receipt)
        if phase is ActivityPhase.RESOLVED:
            self.completed_end_searches[origin] += 1
        elif phase not in (ActivityPhase.FAILED, ActivityPhase.CANCELLED):
            return
        self.pending_end_search = None

    def remember_search(self, turn: Turn, origin: Cell, receipt: Result | None) -> None:
        self.searches[origin.key] += 1
        if receipt and is_searchable_dead_end(origin, self.maps[self.depth]):
            self.pending_end_search = origin.key, receipt
        # Credit each visible wall once in the near and middle search bands.
        self.wall_searches.update({
            cell.key for cell in turn.visible_cells
            if cell.wall and cell.offset[2] == 0 and max(map(abs, cell.offset)) <= 3
        })

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
        self.wand_trials.admitted(action.device, experiment=reason == MemoryReason.TEST_UNKNOWN_DEVICE)
        if reason == MemoryReason.LIGHT_ROOM:
            self.attempted_room_light.add(self.depth)
        elif isinstance(action.target, Cell):
            targets = (foe.target for foe in turn.hostiles if action.target in turn.cells_of(foe))
            if reason == MemoryReason.SLOW_APPROACH:
                self.attempted_slow.update(targets)
            elif reason == MemoryReason.POLYMORPH_THREAT:
                for target in targets:
                    if self.polymorph_by_target.get(target) is PolymorphState.RESISTED:
                        self.polymorph_by_target[target] = PolymorphState.RETRIED
                    else:
                        self.polymorph_by_target.setdefault(target, PolymorphState.ATTEMPTED)

    def carrying_objective(self, turn: Turn) -> bool:
        return any(item.identity == self.objective for item in turn.inventory)

    @staticmethod
    def trapped(turn: Turn) -> bool:
        return turn.has_condition("rogue.bear_trap")

    @staticmethod
    def respond_to_bear_trap(turn: Turn) -> tuple[Action, str]:
        """Choose how to spend a turn while pinned by an active bear trap."""
        if not turn.holds:
            for condition in turn.conditions:
                if condition.identity == "rogue.bear_trap" and condition.active and condition.remaining_ticks is None:
                    # Any direction attempts escape in place, even on an isolated tile.
                    return Move(Direction.NORTH), "escape_bear_trap"
        return Wait(), "wait_for_restraint"

    def damage_estimate(self, foe: Creature) -> int:
        return max(self.observed_loss[foe.target], self.observed_loss_by_name[foe.name])

    def melee_exposure(self, turn: Turn, offset: tuple[int, int, int] = (0, 0, 0)) -> MeleeExposure:
        foes = [foe for foe in turn.hostiles if turn.in_contact(foe, from_offset=offset)]
        return MeleeExposure(foes, sum(self.damage_estimate(foe) for foe in foes))

    def nearby_drainer(self, turn: Turn, foe: Creature) -> bool:
        # Allow one closing step before a creature can strike in contact.
        return foe.name in self.draining_creatures and turn.distance_to(foe) <= 2

    def survival_consumable(self, turn: Turn, *, needs_healing: bool, lethal: bool) -> tuple[UseItem, str] | None:
        """Heal first, then consider teleportation under lethal pressure or food under urgent hunger."""
        if needs_healing:
            if potion := choose_healing_potion(turn, lethal=lethal):
                return UseItem(potion, ItemUse.DRINK), "heal"

        if lethal:
            if teleport := offered_item_use(turn, "item.scroll.teleportation", ItemUse.READ):
                return teleport, "escape_pursuit"
        elif turn.hostiles and turn.hunger in URGENT_HUNGER:
            if meal := choose_meal(turn, self.food_value):
                return UseItem(meal, ItemUse.EAT), "eat"
        return None

    def combat(self, turn: Turn) -> tuple[Action, str] | None:
        health = turn.health
        injured = health is not None and health.current * 2 < health.maximum
        exposure = self.melee_exposure(turn)
        adjacent = exposure.foes
        surrounded = len(adjacent) >= 2
        lethal = exposure.threatens(health, rounds=1)
        two_hit_threat = exposure.threatens(health, rounds=2)
        needs_healing = injured or lethal or (surrounded and health is not None and health.current * 4 < health.maximum * 3)
        if decision := self.survival_consumable(turn, needs_healing=needs_healing, lethal=lethal):
            return decision

        if adjacent and (two_hit_threat or injured or surrounded):
            if zap := self.aim_combat_wand(turn, adjacent, {"item.wand.wand-teleport-away"}):
                return zap, "displace_threat"

        if adjacent and health and (decision := self.counter_threat(turn, health.current, in_melee=True)):
            return decision
        if decision := self.fight_stair_guard(turn, adjacent):
            return decision
        distances = [(foe, turn.distance_to(foe)) for foe in turn.hostiles]
        close_pursuit = bool(adjacent) and sum(distance <= 2 for _, distance in distances) >= 2
        pressed = injured or close_pursuit or two_hit_threat or (not adjacent and converging_pursuers(turn, distances))
        if escape := self.retreat_to_stairs(turn, pressed=pressed):
            return escape
        pursuers = [(foe, distance) for foe, distance in distances if distance <= 3]
        approaching_group = not adjacent and len(pursuers) >= 2
        if approaching_group and not self.trapped(turn):
            if step := take_corridor(turn, pursuers):
                return step, MemoryReason.TAKE_CORRIDOR
        if turn.hostiles and not adjacent and health and (decision := self.counter_threat(turn, health.current)):
            return decision

        if (approaching_group and health and not injured and not turn.has_condition("rogue.hasted")
                and not turn.has_condition("rogue.haste_exhaustion")):
            if haste := offered_item_use(turn, "item.potion.haste", ItemUse.DRINK):
                return haste, "combat_haste"

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

        if decision := self.ranged_combat(turn, adjacent):
            return decision

        if lethal and not self.scroll_trials.tried_in_encounter:
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
        if zap := self.aim_combat_wand(turn, turn.hostiles, ELEMENTAL_WANDS, reflected_ray_range=ELEMENTAL_RAY_RANGE):
            return zap, "elemental_ray"
        if trial := self.try_unknown_wand(turn):
            return trial, MemoryReason.TEST_UNKNOWN_DEVICE

        if self.has_arrows(turn):
            nearest = min(turn.distance_to(foe) for foe in turn.hostiles)
            if nearest >= 5 or nearest <= 2:
                if weapon := ready_weapon(turn, bow=nearest >= 5):
                    return weapon

        if turn.tick == self.last_shot_tick:
            return None
        can_strike = any(turn.attack(foe) is not None for foe in adjacent)
        shot = choose_projectile(turn.ranged_attacks(max_distance=8, hostile_only=True), can_strike=can_strike)
        if shot is None:
            return None
        return shot, "launch_projectile" if shot.launcher else "throw_projectile"

    def melee_attack(self, turn: Turn, *, make_space: bool) -> UseAbility | None:
        can_descend = self.depth < self.retrieval_depth and not self.carrying_objective(turn)

        def attack_order(foe: Creature) -> MeleeRank:
            opens_exit = any(
                (cell.stairs_up and self.depth > 1) or
                (cell.stairs_down and can_descend)
                for cell in turn.cells_of(foe)
            )
            return MeleeRank(
                injury_priority=MELEE_INJURY_PRIORITY.get(foe.injury, 0),
                opens_exit=opens_exit,
                negative_distance=-turn.distance_to(foe),
                estimated_damage=self.damage_estimate(foe),
                damage_from_this_foe=self.observed_loss[foe.target],
            )

        for foe in sorted(turn.hostiles, key=attack_order, reverse=True):
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

        if action := reveal_unseen_attacker(turn):
            return action, "reveal_attacker"

        if turn.holds or self.trapped(turn):
            return None
        if step := evade_unseen_attacker(turn, origin, self.maps.get(self.depth)):
            return step, "evade_unseen_attack"
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
                   if device.item.identity in identities and self.wand_trials.can_spend_charge(device)]
        if not devices:
            return None
        targets = sorted(foes, key=self.damage_estimate, reverse=True)
        for device in devices:
            for foe in targets:
                if zap := turn.aim_device(device, foe, clear_horizontal_ray=True, reflected_ray_range=reflected_ray_range):
                    return zap
        return None

    def try_unknown_wand(self, turn: Turn) -> Zap | None:
        health = turn.health
        if not health or recovery_deficit(health) > 0 or self.carrying_objective(turn):
            return None
        if any(turn.distance_to(foe) < 3 for foe in turn.hostiles):
            return None
        for device in turn.devices:
            if not self.wand_trials.can_experiment(device):
                continue
            for foe in turn.hostiles:
                if turn.distance_to(foe) <= 4:
                    if zap := turn.aim_device(device, foe, reflected_ray_range=ELEMENTAL_RAY_RANGE):
                        return zap
        return None

    def counter_threat(self, turn: Turn, health: int, *, in_melee: bool = False) -> tuple[Zap, str] | None:
        devices = self.control_devices(turn, in_melee=in_melee)
        if not devices:
            return None
        for foe, damage in self.control_targets(turn, health):
            for device, reason in devices:
                if reason == MemoryReason.SLOW_APPROACH and foe.target in self.attempted_slow:
                    continue
                if (reason == MemoryReason.POLYMORPH_THREAT
                        and not self.can_polymorph(turn, foe, device, lethal=damage >= health)):
                    continue
                if zap := turn.aim_device(device, foe, clear_horizontal_ray=True):
                    return zap, reason
        return None

    def control_devices(self, turn: Turn, *, in_melee: bool) -> list[tuple[Device, str]]:
        """List usable control wands in policy order, preserving inventory order within each kind."""
        devices = []
        for identity, reason in CONTROL_DEVICES:
            if in_melee and reason != MemoryReason.POLYMORPH_THREAT:
                continue
            for device in turn.devices:
                if device.item.identity == identity and self.wand_trials.can_spend_charge(device):
                    devices.append((device, reason))
        return devices

    def control_targets(self, turn: Turn, health: int) -> list[tuple[Creature, int]]:
        """Rank nearby drainers and potential two-hit threats by observed damage."""
        targets = []
        for foe in turn.hostiles:
            if turn.distance_to(foe) <= 3:
                damage = self.damage_estimate(foe)
                if 2 * damage >= health or foe.name in self.draining_creatures:
                    targets.append((foe, damage))
        return sorted(targets, key=lambda target: target[1], reverse=True)

    def can_polymorph(self, turn: Turn, foe: Creature, device: Device, lethal: bool) -> bool:
        if not lethal or foe.injury is not Injury.HEALTHY:
            return False
        state = self.polymorph_by_target.get(foe.target)
        if state is None:
            return True
        if state is not PolymorphState.RESISTED:
            return False
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
            if step := self.step_onto_retreat_stair(turn, onward):
                return step, MemoryReason.RETREAT_TO_STAIRS
        if self.depth <= 1:
            return None
        dangerous = any(
            foe.name.casefold() in {"griffin", "dragon", "jabberwock", "wraith"}
            or self.nearby_drainer(turn, foe)
            for foe in turn.hostiles
        )
        if not (dangerous or self.recovering_from_pursuit):
            return None
        exits = self.safe_stairs(turn, Direction.UP)
        if turn.cell and turn.cell.key in exits:
            return Move(Direction.UP), MemoryReason.RETREAT_UPSTAIRS
        if dangerous and downstairs:
            if turn.cell and turn.cell.key in downstairs:
                return Move(Direction.DOWN), MemoryReason.RETREAT_DOWNSTAIRS
            exits.update(downstairs)
        return self.route_to_retreat_stairs(turn, exits)

    def route_to_retreat_stairs(self, turn: Turn, exits: set[str]) -> tuple[Action, str] | None:
        """Choose a stair approach while accounting for pursuit and immediate melee risk."""
        avoid = {cell.key for foe in turn.hostiles for cell in turn.cells_of(foe)}
        if self.last_retreat_origin is not None:
            avoid.add(self.last_retreat_origin)
        exits = exits - avoid
        memory = self.maps[self.depth]
        step = approach_visible_exit(turn, memory, exits, avoid) or memory.toward(turn, exits, avoid=avoid)
        if step is None:
            return None
        # Opening a door spends the turn in the current cell.
        destination = step.direction.offset if isinstance(step, Move) else (0, 0, 0)
        if self.melee_exposure(turn, destination).threatens(turn.health, rounds=2):
            escape = self.evade(turn)
            if escape and not any(turn.in_contact(foe, from_offset=escape[0].direction.offset)
                                  for foe in turn.hostiles):
                return escape
        return step, MemoryReason.RETREAT_TO_STAIRS

    def step_onto_retreat_stair(self, turn: Turn, exits: set[str]) -> Move | None:
        contacts = sum(turn.in_contact(foe) for foe in turn.hostiles)
        candidates = []
        for direction in turn.walkable_directions:
            cell = turn.cell_at(direction.offset)
            if cell is None or cell.key not in exits or cell.trap:
                continue
            exposure = self.melee_exposure(turn, cell.offset)
            guards = len(exposure.foes)
            if guards and (not turn.health or guards > contacts or exposure.threatens(turn.health, rounds=2)):
                continue
            clearance = min((turn.distance_to(foe, from_offset=cell.offset) for foe in turn.hostiles), default=0)
            candidates.append(((exposure.damage, guards, -clearance), direction))
        if not candidates:
            return None
        _, direction = min(candidates, key=lambda candidate: candidate[0])
        return Move(direction)

    @staticmethod
    def evade(turn: Turn) -> tuple[Move, str] | None:
        if not turn.hostiles:
            return None

        @cache
        def safety(offset: tuple[int, int, int]) -> tuple[int, int]:
            contacts = sum(turn.in_contact(foe, from_offset=offset) for foe in turn.hostiles)
            clearance = min(turn.distance_to(foe, from_offset=offset) for foe in turn.hostiles)
            return -contacts, clearance

        current = safety((0, 0, 0))
        choices = [(safety(d.offset), d) for d in turn.walkable_directions]
        if not choices:
            return None
        best = max(score for score, _ in choices)
        if best < current:
            return None
        candidates = [direction for score, direction in choices if score == best]
        immediate_retreat = best > current
        if immediate_retreat:
            if len(candidates) > 1:
                clear = [direction for direction in candidates if turn.cell_at(direction.offset).trap is None]
                candidates = clear or candidates
            if len(candidates) == 1:
                return Move(candidates[0]), MemoryReason.RETREAT

        if onward := best_onward_escape(turn, candidates, safety):
            reason = MemoryReason.RETREAT if immediate_retreat else MemoryReason.BREAK_CONTACT
            return Move(onward), reason
        if immediate_retreat:
            return Move(candidates[0]), MemoryReason.RETREAT
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
        return available - packed.travel_from_current_reserve(nutrition, travel)

    def recovery_action(self, turn: Turn) -> tuple[Action, str] | None:
        if turn.cell is None:
            return None
        health, clock = turn.health, turn.recovery
        if health is None or clock is None or clock.stopped:
            return None
        missing = recovery_deficit(health)
        if missing == 0:
            return None

        memory = self.maps[self.depth]
        returning = self.carrying_objective(turn)
        direction = Direction.UP if returning else Direction.DOWN
        stairs = self.safe_stairs(turn, direction)
        points_to_rest = missing

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
                elif turn.cell.key not in self.danger_until:
                    # Stay in shelter while a single point of recovery is still affordable.
                    points_to_rest = 1

        ticks = clock.ticks_to_regain(points_to_rest)
        rest_budget = self.recovery_budget(turn, memory, direction)
        if ticks is not None and (rest_budget is None or ticks < rest_budget):
            return Wait(), "recover"
        return None

    def prepare(self, turn: Turn) -> tuple[Action, str] | None:
        if turn.restorable_losses and turn.restorable_losses.strength:
            if restore := offered_item_use(turn, "item.potion.restore-strength", ItemUse.DRINK):
                return restore, "restore_strength"
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
            if can_identify_carried_item(item, turn):
                return UseItem(item, ItemUse.READ), "identify_gear"
        if improvement := improve_worn_gear(turn):
            return improvement
        health = turn.health
        can_experiment = bool(health and recovery_deficit(health) == 0 and not self.carrying_objective(turn))
        if can_experiment and (potion := self.potion_trials.choose(turn)):
            return UseItem(potion, ItemUse.DRINK), "test_unknown_potion"
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
        if (here is None or turn.hostiles
                or self.completed_end_searches[here.key] >= DEAD_END_SEARCHES
                or not is_searchable_dead_end(here, memory)):
            return None
        return self.ready_search(turn)

    def follow_stairs(self, turn: Turn, memory: FloorMap, *, returning: bool) -> tuple[Action, str] | None:
        """Descend toward the retrieval floor or follow upstairs with the objective."""
        if turn.cell is None or (not returning and self.depth >= self.retrieval_depth):
            return None
        direction = Direction.UP if returning else Direction.DOWN
        stairs = self.safe_stairs(turn, direction)
        if not stairs:
            return None
        if turn.cell.key in stairs:
            if turn.has_condition("rogue.levitation"):
                return Wait(), "wait_for_ground_contact"
            return Move(direction), "ascend" if returning else "descend"
        if step := route_around_danger(turn, memory, stairs, self.danger_until.keys()):
            return step, MemoryReason.SEEK_STAIRS
        if returning and not turn.hostiles:
            return self.search_return_connection(turn, memory, stairs)
        return None

    def search_return_connection(self, turn: Turn, memory: FloorMap, upstairs: set[str]) -> tuple[Action, str] | None:
        """Search where one opening could reconnect the route to a remembered staircase."""
        search = self.ready_search(turn)
        if turn.cell is None or search is None:
            return None
        sites: set[str] = set()
        remaining_stairs = upstairs.copy()
        while remaining_stairs:
            passages = {cell.key for cell, _ in memory.reachable(remaining_stairs.pop())}
            remaining_stairs.difference_update(passages)
            if turn.cell.key in passages:
                continue
            boundary_walls = {cell.key for key in passages for cell in memory.adjacent(key) if cell.wall}
            for wall in boundary_walls:
                sites.update(cell.key for cell in memory.adjacent(wall)
                             if not cell.wall and cell.key not in passages and self.searches[cell.key] < 3)
        if turn.cell.key in sites:
            return search, "search"
        if step := route_around_danger(turn, memory, sites, self.danger_until.keys()):
            return step, "search"
        return None

    def search(self, turn: Turn, memory: FloorMap, returning: bool, *,
               allow_unseen: bool = True, wall_limit: int = 3) -> tuple[Action, str] | None:
        if returning and (action := self.search_dead_end(turn, memory)):
            return action, "search"
        search = self.ready_search(turn)
        if search is None or turn.cell is None:
            return None
        for sites in self.ranked_search_sites(turn, memory, returning, allow_unseen, wall_limit):
            if turn.cell.key in sites:
                return search, "search"
            if step := route_around_danger(turn, memory, sites, self.danger_until.keys()):
                return step, "search"
        return None

    def ranked_search_sites(self, turn: Turn, memory: FloorMap, returning: bool,
                           allow_unseen: bool, wall_limit: int) -> Iterator[set[str]]:
        sites_by_rank: dict[SearchRank, set[str]] = {}
        for site, distance in memory.reachable(turn.cell.key):
            rank = self.search_site_rank(turn, memory, site, distance, returning=returning,
                                         allow_unseen=allow_unseen, wall_limit=wall_limit)
            if rank is None:
                continue
            sites_by_rank.setdefault(rank, set()).add(site.key)
        # Route to tied destinations together so a clear path wins over a trapped one.
        for rank in sorted(sites_by_rank, reverse=True):
            yield sites_by_rank[rank]

    def search_site_rank(self, turn: Turn, memory: FloorMap, site: RememberedCell, distance: int, *,
                         returning: bool, allow_unseen: bool, wall_limit: int) -> SearchRank | None:
        attempts = self.searches[site.key]
        neighbors = memory.adjacent(site.key)
        walls = [c for c in neighbors if c.wall]
        dead_end = is_searchable_dead_end(site, memory)
        if dead_end:
            remaining = DEAD_END_SEARCHES - self.completed_end_searches[site.key]
            if remaining <= 0 or turn.hostiles:
                return None
            untested = walls
            value = remaining * len(walls)
        else:
            if attempts > wall_limit:
                return None
            untested = [c for c in walls if attempts < wall_limit and self.wall_searches[c.key] < wall_limit]
            value = sum(wall_limit - self.wall_searches[c.key] for c in untested)
        if not untested and (not allow_unseen or len(neighbors) == 4):
            return None
        near_exit = returning and any(
            n.stairs_up for wall in untested for n in memory.adjacent(wall.key)
        )
        # One search checks several boundaries. Favor corridor ends, especially near a lost exit.
        return SearchRank(
            near_return_stairs=near_exit,
            has_searchable_walls=bool(untested),
            value_after_travel=value - distance * 2,
            negative_distance=-distance,
        )

    def estimated_journey_ticks(self, returning: bool) -> int:
        """Budget the shallower return legs and the round trips still ahead."""
        known_return = sum(ticks for floor, ticks in self.ascent_ticks_by_floor.items() if floor < self.depth)
        if returning:
            return known_return
        return known_return + estimate_round_trip_ticks(
            self.depth, self.retrieval_depth, self.descent_ticks_by_floor, self.ascent_ticks_by_floor
        )

    def respond_to_hunger(self, turn: Turn, memory: FloorMap, ground_items: tuple[GroundItem, ...],
                          *, can_forage: bool) -> tuple[Action, str] | None:
        if turn.hunger not in MEAL_HUNGER:
            return None
        if meal := choose_meal(turn, self.food_value):
            return UseItem(meal, ItemUse.EAT), "eat"
        for ground in ground_items:
            if ground.reachable and ground.item.category == "food":
                return Pickup(ground.item), "take"
        if can_forage and (step := self.route_to_food(turn, memory)):
            return step, "forage"
        return None

    def route_to_food(self, turn: Turn, memory: FloorMap) -> Action | None:
        food_cells = memory.cells_with_item(lambda item: item.category == "food")
        return route_around_danger(turn, memory, food_cells, self.danger_until.keys())

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
        packed = estimate_carried_food(edible_items(turn), self.food_value)
        if packed.has_unmeasured_meals:
            return None
        if nutrition.ticks_until_empty >= packed.travel_from_current_reserve(nutrition, required):
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
                starting_meals=packed.meal_count,
            )
        scout = self.food_scouts[self.depth]
        if scout is not None and turn.tick < scout.deadline:
            if step := route_around_danger(turn, memory, memory.frontier, self.danger_until.keys()):
                return step, "scout_for_food"
        return None

    def scroll_trial(self, turn: Turn, *, at_exit: bool = False) -> tuple[UseItem, MemoryReason] | None:
        scroll = self.scroll_trials.choose(turn, self.depth, at_exit=at_exit)
        if scroll is None:
            return None
        reason = MemoryReason.STUDY_SCROLL_AT_EXIT if at_exit else MemoryReason.TEST_UNKNOWN_MAGIC
        return UseItem(scroll, ItemUse.READ), reason

    def route_scroll(self, turn: Turn, memory: FloorMap, returning: bool) -> tuple[UseItem, str] | None:
        """When cut off from remembered upstairs, prefer a known teleport to a trial."""
        if returning and memory.stairs(Direction.UP):
            if teleport := offered_item_use(turn, "item.scroll.teleportation", ItemUse.READ):
                return teleport, "find_return_route"
        return self.scroll_trial(turn)

    def stalled(self, turn: Turn, previous: Result | None) -> tuple[Action | None, str]:
        if (previous is None or not turn.can_cancel or len(turn.activities) != 1
                or turn.outcome_of(previous) is not ActivityPhase.QUEUED):
            return None, "readiness_timeout"
        if self.scroll_trials.pending and self.scroll_trials.pending.receipt == previous:
            return ActivityChoice.CANCEL, MemoryReason.CANCEL_STALLED_SCROLL
        stair = self.pending_stair
        if stair and stair.receipt == previous and (stair.origin, stair.direction) not in self.cancelled_stairs:
            return ActivityChoice.CANCEL, MemoryReason.CANCEL_STALLED_STAIR
        return None, "readiness_timeout"

    def choose_pickup(self, turn: Turn, ground_items: tuple[GroundItem, ...], *, returning: bool) -> Item | None:
        safe_pickup = not turn.hostiles or bool(
            turn.health and recovery_deficit(turn.health) == 0
            and all(turn.distance_to(foe) > 1 for foe in turn.hostiles)
        )
        return preferred_pickup(turn, ground_items, objective=self.objective, returning=returning,
                                safe_pickup=safe_pickup)

    def loot_goals(self, turn: Turn, ground_items: tuple[GroundItem, ...], returning: bool) -> LootGoals:
        return loot_goals(turn, ground_items, returning=returning, objective=self.objective,
                          guarded=self.supplies_guarded_until.keys())

    def light_dark_room(self, turn: Turn) -> Zap | None:
        if turn.cell is None or turn.cell.lit is not False or turn.hostiles:
            return None
        if self.depth in self.attempted_room_light or turn.has_condition("rogue.blindness"):
            return None
        for device in turn.devices:
            if (device.item.identity == "item.wand.wand-light" and device.activation is DeviceActivation.SELF_TARGET
                    and self.wand_trials.can_spend_charge(device)):
                return Zap(device, None)
        return None

    def explore_frontier(self, turn: Turn, memory: FloorMap) -> Action | None:
        avoid = self.danger_until.keys()
        step = memory.toward_unexplored(turn, avoid=avoid)
        if step is None and avoid:
            # Cross a recent threat only when it blocks every remaining route.
            return memory.toward_unexplored(turn)
        if isinstance(step, Move) and self.last_exploration_origin is not None:
            destination = turn.cell_at(step.direction.offset)
            if destination and destination.key == self.last_exploration_origin:
                detour = memory.toward(turn, memory.frontier, avoid=avoid | {destination.key}, max_steps=3)
                return detour or step
        return step

    def explore(self, turn: Turn, memory: FloorMap, *, returning: bool) -> tuple[Action | None, str]:
        """Find a route when remembered stairs and supplies offer no next step."""
        if light := self.light_dark_room(turn):
            return light, MemoryReason.LIGHT_ROOM
        if action := self.explore_frontier(turn, memory):
            return action, MemoryReason.EXPLORE
        scroll = self.route_scroll(turn, memory, returning)
        if decision := self.search(turn, memory, returning, allow_unseen=scroll is None):
            return decision
        if scroll is not None:
            return scroll
        if (not returning and not turn.hostiles and memory.stairs(Direction.UP)
                and not memory.stairs(Direction.DOWN)):
            # A failed Search leaves concealed doors looking like ordinary walls.
            if decision := self.search(turn, memory, returning, allow_unseen=False, wall_limit=6):
                return decision
        return None, "no_disclosed_route"

    def choose(self, turn: Turn) -> tuple[Action | None, str]:
        if turn.needs_identification and (prompt := turn.identification):
            return prompt.select(identification_target(turn, prompt.items)), "identify"
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

        return self.choose_ready_action(turn)

    def choose_ready_action(self, turn: Turn) -> tuple[Action | None, str]:
        """Choose a gameplay action once prompts, interruptions and forced waits are handled."""
        if decision := self.combat(turn):
            return decision

        if self.corridor_stand and turn.hunger not in URGENT_HUNGER:
            entrance, until = self.corridor_stand
            if turn.cell.key == entrance and turn.tick < until:
                return Wait(), "hold_corridor"

        memory = self.maps[self.depth]
        trapped = self.trapped(turn)
        hovering = turn.has_condition("rogue.levitation")
        ground_items = () if hovering else turn.ground_items
        if decision := self.respond_to_hunger(turn, memory, ground_items, can_forage=not hovering and not trapped):
            return decision

        returning = self.carrying_objective(turn)
        if item := self.choose_pickup(turn, ground_items, returning=returning):
            return Pickup(item), "take"
        if not returning and not hovering and not turn.hostiles and not trapped:
            objective_cells = memory.cells_with_item(lambda item: item.identity == self.objective)
            if objective_cells and (step := memory.toward(turn, objective_cells, avoid=self.danger_until.keys())):
                return step, "seek_objective"
        if not turn.hostiles and (decision := self.prepare(turn)):
            return decision
        if trapped:
            return self.respond_to_bear_trap(turn)
        if not returning and not any(g.item.identity == self.objective for g in ground_items):
            if search := self.search_dead_end(turn, memory):
                return search, "search"
        loot = self.loot_goals(turn, ground_items, returning)
        if decision := route_to_loot(turn, memory, loot, self.danger_until.keys()):
            return decision

        if not turn.hostiles and (decision := self.provision(turn, memory, returning)):
            return decision

        if decision := self.follow_stairs(turn, memory, returning=returning):
            return decision

        return self.explore(turn, memory, returning=returning)


def __getattr__(name: str):
    """Support callers that imported runner functions from the original policy module."""
    if name in {"play", "trace_result"}:
        import rogue_runner
        return getattr(rogue_runner, name)
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
