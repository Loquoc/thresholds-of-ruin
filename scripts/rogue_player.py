"""Rogue retrieval strategy using the ordinary Python player interface."""

from __future__ import annotations

from collections.abc import Iterable
from dataclasses import dataclass, field
from enum import StrEnum
from typing import NamedTuple

from tor_client_bot import (
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
    Pickup,
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

from rogue_combat import (
    below_half_health, choose_melee_attack, choose_projectile, includes_self_damage, recovery_deficit,
    MeleeExposure, OpponentHistory, PolymorphHistory, witnessed_unseen_attack,
)
from rogue_loot import (
    HEALING_POTIONS, LootGoals, loot_goals, preferred_pickup, route_to_loot, useful_loot,
)
from rogue_training import purchase_training
from rogue_trials import PotionTrials, ScrollTrials, WandTrials
from rogue_retreat import (
    stair_retreat_step,
    choose_retreat_step, converging_pursuers, evade_unseen_attacker,
    recovery_stair_step, route_around_danger, route_to_recovery_shelter, take_corridor,
)
from rogue_search import ORDINARY_WALL_SEARCHES, SearchHistory
from rogue_food import (
    MEAL_HUNGER, URGENT_HUNGER, FoodScoutHistory, MealHistory, PendingMeal, choose_meal, edible_items, estimate_carried_food,
    FloorTravelTimes,
)
from rogue_gear import (
    can_wield_shortbow, choose_armor, choose_growth_or_identification, choose_ring, improve_worn_gear,
    identification_target, offered_item_use, ready_weapon, ready_weapon_for_range, reveal_unseen_attacker,
)


MELEE_INJURY_PRIORITY = {
    Injury.NEAR_DEATH: 3,
    Injury.BADLY_WOUNDED: 2,
    Injury.WOUNDED: 1,
}
RECENT_DANGER_TICKS = 1200
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


def choose_healing_potion(turn: Turn, *, two_hit_threat: bool) -> Item | None:
    """Reserve extra healing for hallucination or a threat that can kill within two rounds."""
    doses = (item for item in turn.inventory if item.identity in HEALING_POTIONS and ItemUse.DRINK in item.uses)
    if two_hit_threat or turn.has_condition("rogue.hallucinating"):
        preferred = "item.potion.extra-healing"
    else:
        preferred = "item.potion.healing"
    return max(doses, key=lambda item: item.identity == preferred, default=None)


class MeleeRank(NamedTuple):
    """Higher fields win in order. Equal ranks preserve the observed target order."""

    injury_priority: int
    opens_exit: bool
    negative_distance: int
    estimated_damage: int
    damage_from_this_foe: int


@dataclass(frozen=True)
class PendingStair:
    attempt: StairAttempt
    pursuers: tuple[str, ...]


@dataclass
class Adventurer:
    retrieval_depth: int
    objective: str
    depth: int = 1
    maps: dict[int, FloorMap] = field(default_factory=dict)
    last_exploration_origin: str | None = None
    corridor_stand: tuple[str, int] | None = None
    last_shot_tick: int | None = None
    search_history: SearchHistory = field(default_factory=SearchHistory)
    travel_times: FloorTravelTimes = field(default_factory=FloorTravelTimes)
    meals: MealHistory = field(default_factory=MealHistory)
    food_scouts: FoodScoutHistory = field(default_factory=FoodScoutHistory)
    pending_stair: PendingStair | None = None
    last_guarded_landing: tuple[str, str] | None = None
    unsafe_stairs: dict[tuple[str, Direction], int] = field(default_factory=dict)
    cancelled_stairs: set[tuple[str, Direction]] = field(default_factory=set)
    wait_after_cancel: bool = False

    last_unseen_attack: tuple[str, int] | None = None
    previous_health: int | None = None
    opponents: OpponentHistory = field(default_factory=OpponentHistory)
    potion_trials: PotionTrials = field(default_factory=PotionTrials)
    scroll_trials: ScrollTrials = field(default_factory=ScrollTrials)
    wand_trials: WandTrials = field(default_factory=WandTrials)
    supplies_guarded_until: dict[str, int] = field(default_factory=dict)
    attempted_slow: set[str] = field(default_factory=set)
    polymorph: PolymorphHistory = field(default_factory=PolymorphHistory)
    attempted_room_light: set[int] = field(default_factory=set)
    previous_losses: RestorableLosses | None = None
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

    def holding_corridor(self, cell: Cell, tick: int) -> bool:
        if self.corridor_stand is None:
            return False
        entrance, deadline = self.corridor_stand
        return cell.key == entrance and tick < deadline

    def observe(self, turn: Turn, combat: CombatReport | None = None) -> None:
        here = turn.cell
        if here and turn.can_act and not self.holding_corridor(here, turn.tick):
            self.corridor_stand = None
        self.search_history.observe(turn)
        self.potion_trials.observe(turn)
        self.scroll_trials.observe(turn)
        self.expire_danger_warnings(turn.tick)
        self.observe_guarded_supplies(turn)
        if self.travel_times.entered_at is None:
            self.travel_times.entered_at = turn.tick
        self.meals.observe(turn)
        self.observe_combat(turn, turn.recent_combat if combat is None else combat)
        self.observe_stair(turn)
        if self.pending_stair:
            return
        self.food_scouts.observe(self.depth, turn)
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
            if ground.cell is None or not useful_loot(ground.item):
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
        outgoing = report.outgoing
        self.polymorph.observe(outgoing)
        attributable_loss = 0 if includes_self_damage(outgoing, turn.self_target) else loss
        incoming = report.incoming
        self.opponents.record_injury(turn.hostiles, incoming, attributable_loss, lasting_injury=lasting_injury)
        injury_from_incoming = attributable_loss > 0 or lasting_injury
        if here and (turn.hostiles or (injury_from_incoming and incoming)):
            self.danger_until[here.key] = turn.tick + RECENT_DANGER_TICKS
        if not turn.hostiles:
            self.scroll_trials.tried_in_encounter = False
            self.last_retreat_origin = None
        if here and witnessed_unseen_attack(incoming, turn.hostiles, injured=injury_from_incoming):
            self.last_unseen_attack = here.key, turn.tick

    def observe_stair(self, turn: Turn) -> None:
        """Retire a failed attempt immediately. Confirm arrival on a ready or terminal picture."""
        pending = self.pending_stair
        if pending is None:
            return
        attempt = pending.attempt
        arrived = turn.stair_arrival(attempt)
        if arrived is None:
            return
        if arrived:
            if not (turn.can_act or turn.terminal):
                return
            self.record_stair_arrival(turn, pending)
        self.pending_stair = None

    def record_stair_arrival(self, turn: Turn, pending: PendingStair) -> None:
        attempt = pending.attempt
        landing = attempt.landing or turn.cell.key
        if self.last_unseen_attack and turn.cell and self.last_unseen_attack[0] != turn.cell.key:
            self.last_unseen_attack = None
        if pending.pursuers:
            if attempt.direction is Direction.UP and len(pending.pursuers) == 1:
                self.last_guarded_landing = attempt.origin, pending.pursuers[0]
            reverse = Direction.UP if attempt.direction is Direction.DOWN else Direction.DOWN
            self.unsafe_stairs[landing, reverse] = self.danger_until.get(
                attempt.origin, turn.tick)
        self.food_scouts.finish(self.depth)
        self.cancelled_stairs.clear()
        if attempt.direction is Direction.DOWN:
            self.travel_times.record_descent(self.depth, self.maps[self.depth], attempt, turn.tick)
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
        self.travel_times.entered_at = turn.tick

    def admitted(self, turn: Turn, action: Action, receipt: Result | None = None,
                 *, reason: str | None = None) -> None:
        """Record the policy decision and the admitted action, which can both change memory."""
        match reason:
            case MemoryReason.SEEK_STAIRS:
                self.food_scouts.finish(self.depth)
            case MemoryReason.CANCEL_STALLED_SCROLL:
                self.scroll_trials.interrupted(turn)
            case MemoryReason.CANCEL_STALLED_STAIR if self.pending_stair:
                attempt = self.pending_stair.attempt
                self.cancelled_stairs.add((attempt.origin, attempt.direction))
                self.wait_after_cancel = True
            case MemoryReason.WAIT_AFTER_CANCEL:
                self.wait_after_cancel = False

        match action:
            case Zap():
                self.remember_zap(turn, action, reason)
            case Move():
                self.remember_move(turn, action, receipt, reason)
            case UseItem():
                self.remember_item_use(turn, action, receipt, reason)
            case UseAbility() if action.ability.identity == "rogue.search" and turn.cell:
                self.search_history.admit(turn, turn.cell, receipt, self.maps.get(self.depth))
            case Shot():
                self.last_shot_tick = turn.tick

    def remember_item_use(self, turn: Turn, action: UseItem, receipt: Result | None, reason: str | None) -> None:
        item = action.item
        if action.use is ItemUse.DRINK and item.identity is None:
            self.potion_trials.admitted(turn, item, receipt)
        elif action.use is ItemUse.READ and item.identity is None:
            trial = reason in {MemoryReason.TEST_UNKNOWN_MAGIC, MemoryReason.STUDY_SCROLL_AT_EXIT}
            self.scroll_trials.admitted(turn, self.depth, item, receipt if trial else None)
        elif action.use is ItemUse.EAT:
            self.meals.pending = PendingMeal.before_eating(turn, item, receipt)

    def remember_move(self, turn: Turn, action: Move, receipt: Result | None, reason: str | None) -> None:
        if turn.cell is None:
            return
        self.last_exploration_origin = turn.cell.key if reason == MemoryReason.EXPLORE else None
        self.corridor_stand = None
        if reason == MemoryReason.TAKE_CORRIDOR and (entrance := turn.cell_at(action.direction.offset)):
            # Allow pursuers to reach the choke even when the doorway breaks sight.
            self.corridor_stand = entrance.key, turn.tick + 600
        self.last_retreat_origin = turn.cell.key if reason == MemoryReason.RETREAT_TO_STAIRS else None
        if (reason in {MemoryReason.RETREAT, MemoryReason.BREAK_CONTACT}
                and below_half_health(turn.health)):
            self.recovering_from_pursuit = True
        if action.direction in (Direction.UP, Direction.DOWN):
            pursuers = ()
            if reason in {MemoryReason.RETREAT_UPSTAIRS, MemoryReason.RETREAT_DOWNSTAIRS}:
                pursuers = tuple(foe.target for foe in turn.hostiles)
            self.pending_stair = PendingStair(turn.stair_attempt(action, receipt), pursuers)

    def remember_zap(self, turn: Turn, action: Zap, reason: str | None) -> None:
        self.wand_trials.admitted(action.device, experiment=reason == MemoryReason.TEST_UNKNOWN_DEVICE)
        if reason == MemoryReason.LIGHT_ROOM:
            self.attempted_room_light.add(self.depth)
        elif isinstance(action.target, Cell):
            targets = (foe.target for foe in turn.hostiles if action.target in turn.cells_of(foe))
            if reason == MemoryReason.SLOW_APPROACH:
                self.attempted_slow.update(targets)
            elif reason == MemoryReason.POLYMORPH_THREAT:
                self.polymorph.admitted(targets)

    def carrying_objective(self, turn: Turn) -> bool:
        return any(item.identity == self.objective for item in turn.inventory)

    @staticmethod
    def trapped(turn: Turn) -> bool:
        return turn.has_condition("rogue.bear_trap")

    def restrained(self, turn: Turn) -> bool:
        """A bear trap or a creature's hold prevents an ordinary retreat step."""
        return self.trapped(turn) or bool(turn.holds)

    @staticmethod
    def respond_to_bear_trap(turn: Turn) -> tuple[Action, str]:
        """Choose how to spend a turn while pinned by an active bear trap."""
        if not turn.holds:
            for condition in turn.conditions:
                if condition.identity == "rogue.bear_trap" and condition.active and condition.remaining_ticks is None:
                    # Any direction attempts escape in place, even on an isolated tile.
                    return Move(Direction.NORTH), "escape_bear_trap"
        return Wait(), "wait_for_restraint"

    def survival_consumable(self, turn: Turn, exposure: MeleeExposure) -> tuple[UseItem, str] | None:
        """Heal first, then consider teleportation under lethal pressure or food under urgent hunger."""
        health = turn.health
        if exposure.needs_healing(health):
            if potion := choose_healing_potion(turn, two_hit_threat=exposure.threatens(health, rounds=2)):
                return UseItem(potion, ItemUse.DRINK), "heal"

        if exposure.threatens(health, rounds=1):
            if teleport := offered_item_use(turn, "item.scroll.teleportation", ItemUse.READ):
                return teleport, "escape_pursuit"
        elif turn.hostiles and turn.hunger in URGENT_HUNGER:
            if meal := choose_meal(turn, self.meals.gains):
                return UseItem(meal, ItemUse.EAT), "eat"
        return None

    def combat(self, turn: Turn) -> tuple[Action, str] | None:
        exposure = self.opponents.melee_exposure(turn)
        if decision := self.survival_consumable(turn, exposure):
            return decision

        if turn.hostiles:
            if decision := self.fight_visible_hostiles(turn, exposure):
                return decision
        elif decision := self.respond_to_unseen_attack(turn):
            return decision

        if turn.holds:
            return Escape(turn.holds[0]), "escape_hold"
        return None

    def fight_visible_hostiles(self, turn: Turn, exposure: MeleeExposure) -> tuple[Action, str] | None:
        health = turn.health
        adjacent = exposure.foes
        surrounded = len(adjacent) >= 2
        injured = below_half_health(health)
        two_hit_threat = exposure.threatens(health, rounds=2)
        if adjacent and (two_hit_threat or injured or surrounded):
            if zap := self.aim_combat_wand(turn, adjacent, {"item.wand.wand-teleport-away"}):
                return zap, "displace_threat"

        if adjacent and health and (decision := self.counter_threat(turn, health.current, in_melee=True)):
            return decision
        if decision := self.fight_stair_guard(turn, adjacent):
            return decision
        if decision := self.respond_to_pursuit(turn, in_contact=bool(adjacent), injured=injured,
                                              two_hit_threat=two_hit_threat):
            return decision

        needs_distance = (injured or self.recovering_from_pursuit or surrounded or two_hit_threat
                          or any(self.opponents.nearby_drainer(turn, foe) for foe in turn.hostiles))
        if needs_distance and not self.restrained(turn):
            if decision := self.evade(turn, self.opponents):
                return decision

        if decision := self.ranged_combat(turn, adjacent):
            return decision

        if exposure.threatens(health, rounds=1) and not self.scroll_trials.tried_in_encounter:
            if trial := self.scroll_trial(turn):
                return trial

        if not (self.has_arrows(turn) and can_wield_shortbow(turn)):
            if decision := ready_weapon(turn, bow=False):
                return decision

        if attack := self.melee_attack(turn, make_space=surrounded or two_hit_threat):
            return attack, "melee"

        return None

    def respond_to_pursuit(self, turn: Turn, *, in_contact: bool, injured: bool,
                           two_hit_threat: bool) -> tuple[Action, str] | None:
        health = turn.health
        distances = [(foe, turn.distance_to(foe)) for foe in turn.hostiles]
        close_pursuit = in_contact and sum(distance <= 2 for _, distance in distances) >= 2
        pressed = injured or close_pursuit or two_hit_threat or (not in_contact and converging_pursuers(turn, distances))
        if escape := self.retreat_to_stairs(turn, pressed=pressed):
            return escape
        pursuers = [(foe, distance) for foe, distance in distances if distance <= 3]
        approaching_group = not in_contact and len(pursuers) >= 2
        if approaching_group and not self.restrained(turn):
            if step := take_corridor(turn, pursuers):
                return step, MemoryReason.TAKE_CORRIDOR
        if turn.hostiles and not in_contact and health and (decision := self.counter_threat(turn, health.current)):
            return decision

        if (approaching_group and health and not injured and not turn.has_condition("rogue.hasted")
                and not turn.has_condition("rogue.haste_exhaustion")):
            if haste := offered_item_use(turn, "item.potion.haste", ItemUse.DRINK):
                return haste, "combat_haste"
        return None

    def fight_stair_guard(self, turn: Turn, adjacent: list[Creature]) -> tuple[Action, str] | None:
        """Fight a lone drainer when using this stair already failed to shake it."""
        if len(turn.hostiles) != 1 or not adjacent or turn.cell is None or not turn.cell.stairs_up:
            return None
        guard = adjacent[0]
        if self.last_guarded_landing != (turn.cell.key, guard.target) or not self.opponents.is_drainer(guard):
            return None
        health = turn.health
        if health is None or health.current * 4 < health.maximum * 3 or self.carrying_objective(turn):
            return None
        if not 0 < self.opponents.damage_estimate(guard) * 4 < health.current:
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
            if weapon := ready_weapon_for_range(turn, nearest):
                return weapon

        if self.last_shot_tick is not None and turn.tick <= self.last_shot_tick:
            return None
        shot = choose_projectile(turn.ranged_attacks(max_distance=8, hostile_only=True))
        if shot is None:
            return None
        if shot.launcher is not None:
            return shot, "launch_projectile"
        if any(turn.attack(foe) is not None for foe in adjacent):
            return None
        return shot, "throw_projectile"

    def melee_attack(self, turn: Turn, *, make_space: bool) -> UseAbility | None:
        returning = self.carrying_objective(turn)
        can_ascend = returning or self.depth > 1
        can_descend = not returning and self.depth < self.retrieval_depth

        def attack_order(foe: Creature) -> MeleeRank:
            opens_exit = any(
                (cell.stairs_up and can_ascend and self.stair_is_safe(cell.key, Direction.UP, turn.tick))
                or (cell.stairs_down and can_descend and self.stair_is_safe(cell.key, Direction.DOWN, turn.tick))
                for cell in turn.cells_of(foe)
            )
            return MeleeRank(
                injury_priority=MELEE_INJURY_PRIORITY.get(foe.injury, 0),
                opens_exit=opens_exit,
                negative_distance=-turn.distance_to(foe),
                estimated_damage=self.opponents.damage_estimate(foe),
                damage_from_this_foe=self.opponents.damage_by_target[foe.target],
            )

        targets = sorted(turn.hostiles, key=attack_order, reverse=True)
        return choose_melee_attack(turn, targets, make_space=make_space)

    def respond_to_unseen_attack(self, turn: Turn) -> tuple[Action, str] | None:
        if turn.hostiles or self.last_unseen_attack is None:
            return None
        origin, attack_tick = self.last_unseen_attack
        if not attack_tick <= turn.tick < attack_tick + 400:
            return None

        if action := reveal_unseen_attacker(turn):
            return action, "reveal_attacker"

        if self.restrained(turn):
            return None
        if step := evade_unseen_attacker(turn, origin, self.maps.get(self.depth)):
            return step, "evade_unseen_attack"
        return None

    def aim_combat_wand(self, turn: Turn, foes: Iterable[Creature], identities: set[str],
                        *, reflected_ray_range: int | None = None) -> Zap | None:
        # These wands share a ray range, so the first usable offer can test every target.
        device = next((wand for wand in turn.devices
                       if wand.item.identity in identities and wand.activation is not DeviceActivation.SELF_TARGET
                       and self.wand_trials.can_spend_charge(wand)), None)
        if device is None:
            return None
        for foe in sorted(foes, key=self.opponents.damage_estimate, reverse=True):
            if zap := turn.aim_device(device, foe, clear_horizontal_ray=True, reflected_ray_range=reflected_ray_range):
                return zap
        return None

    def try_unknown_wand(self, turn: Turn) -> Zap | None:
        health = turn.health
        if not health or recovery_deficit(health) > 0 or self.carrying_objective(turn):
            return None
        # All eligible trial wands use the same directional aim and reflection range.
        device = next((wand for wand in turn.devices if self.wand_trials.can_experiment(wand)), None)
        if device is None:
            return None
        targets = []
        for foe in turn.hostiles:
            distance = turn.distance_to(foe)
            if distance < 3:
                return None
            if distance <= 4:
                targets.append(foe)
        for foe in targets:
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
                        and not self.polymorph.permits_cast(turn, foe, device, lethal=damage >= health)):
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
                damage = self.opponents.damage_estimate(foe)
                if 2 * damage >= health or self.opponents.is_drainer(foe):
                    targets.append((foe, damage))
        return sorted(targets, key=lambda target: target[1], reverse=True)

    def stair_is_safe(self, key: str, direction: Direction, tick: int) -> bool:
        return (self.unsafe_stairs.get((key, direction), 0) <= tick
                and not (direction is Direction.DOWN and key == self.recovery_landing))

    def safe_stairs(self, turn: Turn, direction: Direction) -> set[str]:
        return {key for key in self.maps[self.depth].stairs(direction)
                if self.stair_is_safe(key, direction, turn.tick)}

    def retreat_to_stairs(self, turn: Turn, *, pressed: bool = False) -> tuple[Action, str] | None:
        if not turn.hostiles or self.restrained(turn):
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
            or self.opponents.nearby_drainer(turn, foe)
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
        step = stair_retreat_step(turn, self.maps[self.depth], exits, self.last_retreat_origin)
        if step is None:
            return None
        # Opening a door spends the turn in the current cell.
        destination = step.direction.offset if isinstance(step, Move) else (0, 0, 0)
        if self.opponents.melee_exposure(turn, destination).threatens(turn.health, rounds=2):
            escape = self.evade(turn, self.opponents)
            if escape:
                offset = escape[0].direction.offset
                remaining = self.opponents.melee_exposure(turn, offset)
                if not remaining.foes:
                    return escape
                # Accept continued contact only when its cost is known and survivable.
                if (not remaining.threatens(turn.health, rounds=2) and not turn.cell_at(offset).trap
                        and all(self.opponents.damage_estimate(foe) > 0 and not self.opponents.is_drainer(foe)
                                for foe in remaining.foes)):
                    return escape
        return step, MemoryReason.RETREAT_TO_STAIRS

    def step_onto_retreat_stair(self, turn: Turn, exits: set[str]) -> Move | None:
        contacts = sum(turn.in_contact(foe) for foe in turn.hostiles)
        candidates = []
        for direction in turn.walkable_directions:
            cell = turn.cell_at(direction.offset)
            if cell is None or cell.key not in exits or cell.trap:
                continue
            exposure = self.opponents.melee_exposure(turn, cell.offset)
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
    def evade(turn: Turn, opponents: OpponentHistory | None = None) -> tuple[Move, str] | None:
        step = choose_retreat_step(turn, opponents.damage_estimate if opponents else None)
        if step is None:
            return None
        reason = MemoryReason.RETREAT if step.improves_immediately else MemoryReason.BREAK_CONTACT
        return Move(step.direction), reason

    def can_afford_wait(self, turn: Turn, memory: FloorMap, direction: Direction, ticks: int) -> bool:
        """Leave enough food for the wait and the estimated travel through remembered stairs."""
        nutrition = turn.nutrition
        if nutrition is None:
            return True
        if not memory.stairs(direction):
            return ticks < nutrition.ticks_until_empty
        travel = self.travel_times.return_ticks(through_floor=self.depth if direction is Direction.UP else None)
        packed = estimate_carried_food(edible_items(turn), self.meals.gains)
        return ticks < packed.spare_ticks_after_travel(nutrition, travel)

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

        def rest_if_affordable(points: int) -> tuple[Wait, str] | None:
            ticks = clock.ticks_to_regain(points)
            if ticks is not None and self.can_afford_wait(turn, memory, direction, ticks):
                return Wait(), "recover"
            return None

        if self.trapped(turn):
            response = self.respond_to_bear_trap(turn)
            if isinstance(response[0], Move):
                return response
            return rest_if_affordable(missing)

        danger = self.danger_until.keys()
        if turn.cell.key in danger:
            if action := route_to_recovery_shelter(turn, memory, danger):
                return action, "reposition_to_recover"
        # Recover along the food route while scouting away from the stairs.
        food_route = None
        if not returning and self.depth in self.food_scouts.by_floor:
            food_route = self.provision(turn, memory, returning)
        if food_route:
            action, reason = food_route
        else:
            stairs = self.safe_stairs(turn, direction)
            action = recovery_stair_step(turn, memory, stairs, danger)
            reason = "recover_en_route"
        if isinstance(action, Move) and (cell := turn.cell_at(action.direction.offset)):
            if cell.key not in danger:
                return action, reason
            if turn.cell.key not in danger:
                # Stay in shelter while a single point of recovery is still affordable.
                return rest_if_affordable(1)
        return rest_if_affordable(missing)

    def prepare(self, turn: Turn) -> tuple[Action, str] | None:
        if turn.restorable_losses and turn.restorable_losses.strength:
            if restore := offered_item_use(turn, "item.potion.restore-strength", ItemUse.DRINK):
                return restore, "restore_strength"
        if upgrade := choose_armor(turn) or choose_ring(turn):
            return upgrade
        if action := self.recovery_action(turn):
            return action
        if consumable := choose_growth_or_identification(turn):
            return consumable
        if improvement := improve_worn_gear(turn):
            return improvement
        if weapon := ready_weapon(turn, bow=False):
            return weapon

        health = turn.health
        if health is None or recovery_deficit(health) or self.carrying_objective(turn):
            return None
        if potion := self.potion_trials.choose(turn):
            return UseItem(potion, ItemUse.DRINK), "test_unknown_potion"
        return self.study_scroll_at_exit(turn)

    def study_scroll_at_exit(self, turn: Turn) -> tuple[UseItem, MemoryReason] | None:
        """Try an unidentified scroll with a usable escape stair and something to identify."""
        here = turn.cell
        if not 3 <= self.depth < self.retrieval_depth or here is None or not here.stairs_down:
            return None
        if here.key in self.danger_until or not self.stair_is_safe(here.key, Direction.DOWN, turn.tick):
            return None
        if self.restrained(turn) or turn.has_condition("rogue.levitation"):
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
        if here is None or turn.hostiles or not self.search_history.remaining_dead_end_searches(here, memory):
            return None
        return self.ready_search(turn)

    def follow_stairs(self, turn: Turn, memory: FloorMap, *, returning: bool) -> tuple[Action, str] | None:
        """Descend toward the retrieval floor or follow upstairs with the objective."""
        if turn.cell is None or (not returning and self.depth >= self.retrieval_depth):
            return None
        direction = Direction.UP if returning else Direction.DOWN
        stairs = self.safe_stairs(turn, direction)
        if not stairs:
            # A temporary warning on the only exit can expire while we wait on this side.
            if returning and turn.cell.stairs_up and not turn.hostiles and turn.cell.key not in self.danger_until:
                wait_ticks = self.unsafe_stairs.get((turn.cell.key, Direction.UP), 0) - turn.tick
                if wait_ticks > 0 and self.can_afford_wait(turn, memory, direction, wait_ticks):
                    return Wait(), "wait_for_safe_ascent"
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
        sites = self.search_history.return_connection_sites(memory, turn.cell.key, upstairs)
        return self.search_at_sites(turn, memory, sites, search)

    def search_at_sites(self, turn: Turn, memory: FloorMap, sites: set[str],
                        search: UseAbility) -> tuple[Action, str] | None:
        """Search here if eligible, otherwise approach an eligible site."""
        if turn.cell.key in sites:
            return search, "search"
        if step := route_around_danger(turn, memory, sites, self.danger_until.keys()):
            return step, "search"
        return None

    def search(self, turn: Turn, memory: FloorMap, returning: bool, *,
               allow_unseen: bool = True, wall_limit: int = ORDINARY_WALL_SEARCHES) -> tuple[Action, str] | None:
        if returning and (action := self.search_dead_end(turn, memory)):
            return action, "search"
        search = self.ready_search(turn)
        if search is None or turn.cell is None:
            return None
        ranked = self.search_history.ranked_search_sites(turn, memory, returning=returning,
                                                        allow_unseen=allow_unseen, wall_limit=wall_limit)
        for sites in ranked:
            if decision := self.search_at_sites(turn, memory, sites, search):
                return decision
        return None

    def estimated_journey_ticks(self, returning: bool) -> int:
        """Budget the shallower return legs and the round trips still ahead."""
        return self.travel_times.journey_ticks(self.depth, self.retrieval_depth, returning=returning)

    def respond_to_hunger(self, turn: Turn, memory: FloorMap, ground_items: tuple[GroundItem, ...],
                          *, can_forage: bool) -> tuple[Action, str] | None:
        if turn.hunger not in MEAL_HUNGER:
            return None
        if meal := choose_meal(turn, self.meals.gains):
            return UseItem(meal, ItemUse.EAT), "eat"
        reachable_food = (ground.item for ground in ground_items if ground.reachable and ground.item.category == "food")
        if meal := choose_meal(turn, self.meals.gains, candidates=reachable_food):
            return Pickup(meal), "take"
        if can_forage and (step := self.route_to_food(turn, memory)):
            return step, "forage"
        return None

    def route_to_food(self, turn: Turn, memory: FloorMap) -> Action | None:
        food_cells = memory.cells_with_item(lambda item: item.category == "food")
        avoid = self.danger_until.keys() | self.supplies_guarded_until.keys()
        return route_around_danger(turn, memory, food_cells, avoid)

    def provision(self, turn: Turn, memory: FloorMap, returning: bool) -> tuple[Action, str] | None:
        nutrition = turn.nutrition
        if nutrition is None or turn.has_condition("rogue.levitation"):
            return None
        required = self.estimated_journey_ticks(returning)
        if nutrition.ticks_until_empty >= required:
            return None
        packed = estimate_carried_food(edible_items(turn), self.meals.gains)
        if packed.has_unmeasured_meals:
            return None
        if packed.spare_ticks_after_travel(nutrition, required) >= 0:
            return None
        if step := self.route_to_food(turn, memory):
            return step, "forage"
        if returning or self.depth >= self.retrieval_depth:
            return None
        if not self.food_scouts.can_explore(self.depth, turn, ticks_until_empty=nutrition.ticks_until_empty,
                                           carried_meals=packed.meal_count):
            return None
        if step := route_around_danger(turn, memory, memory.frontier, self.danger_until.keys(), avoid_traps=True):
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
        stair = self.pending_stair.attempt if self.pending_stair else None
        if stair and stair.receipt == previous and (stair.origin, stair.direction) not in self.cancelled_stairs:
            return ActivityChoice.CANCEL, MemoryReason.CANCEL_STALLED_STAIR
        return None, "readiness_timeout"

    def choose_pickup(self, turn: Turn, ground_items: tuple[GroundItem, ...], *, returning: bool) -> Item | None:
        return preferred_pickup(turn, ground_items, objective=self.objective, returning=returning)

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
                # Only avoid doubling back when the whole detour is clear.
                blocked = avoid | {destination.key}
                detour = memory.toward_avoiding_traps(turn, memory.frontier, avoid=blocked, max_steps=3)
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

        trapped = self.trapped(turn)
        if not trapped and self.holding_corridor(turn.cell, turn.tick) and turn.hunger not in URGENT_HUNGER:
            return Wait(), "hold_corridor"

        memory = self.maps[self.depth]
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
        if decision := route_to_loot(turn, memory, loot, self.danger_until.keys(),
                                     guarded=self.supplies_guarded_until.keys()):
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
