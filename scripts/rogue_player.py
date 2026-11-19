"""Rogue retrieval strategy using the ordinary Python player interface."""

from __future__ import annotations

from collections import Counter
from dataclasses import asdict, dataclass, field
from statistics import median_low
from typing import Callable

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
    Injury,
    ItemUse,
    Move,
    Outcome,
    Pickup,
    RestorableLosses,
    Result,
    Session,
    StairAttempt,
    Shot,
    Turn,
    Training,
    TrainingKind,
    TrainingRefused,
    UseAbility,
    UseItem,
    Wait,
    Zap,
    describe_action,
)


from rogue_gear import (
    IDENTIFY_CATEGORIES, EquipmentHistory, choose_armor, choose_ring, improve_worn_gear,
    needs_identification, ready_weapon, ring_upgrades,
)


DETOUR_STOCK = {"potion": 4, "scroll": 4, "wand": 2}
LOOT_CLASSES = {"amulet", "armor", "coin", "food", "potion", "ring", "scroll", "tool", "wand", "weapon"}
DEAD_END_SEARCHES = 6
ELEMENTAL_WANDS = {"item.wand.wand-fire", "item.wand.wand-cold", "item.wand.wand-lightning"}
ELEMENTAL_RAY_RANGE = 6
CONTROL_DEVICES = (
    ("item.wand.wand-slow", "slow_approach"),
    ("item.wand.wand-polymorph", "polymorph_threat"),
    ("item.wand.wand-magic-missile", "magic_missile"),
)


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
    pending_meal: tuple[str, int, int] | None = None
    # A floor gets one bounded detour. A confirmed increase in supplies ends it early.
    food_scouts: dict[int, tuple[int, int]] = field(default_factory=dict)
    pending_stair: StairAttempt | None = None
    fled_via_stair: bool = False
    unsafe_stairs: dict[tuple[str, Direction], int] = field(default_factory=dict)
    cancelled_stair_origin: str | None = None
    wait_after_cancel: bool = False

    last_unseen_attack: tuple[str, int] | None = None
    previous_health: int | None = None
    observed_loss: Counter[str] = field(default_factory=Counter)
    observed_loss_by_name: Counter[str] = field(default_factory=Counter)
    tried_unknown_scroll: bool = False
    sampled_potions: set[str] = field(default_factory=set)
    sampled_scrolls: set[str] = field(default_factory=set)
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
    last_retreat_origin: str | None = None
    training_checked_at: int | None = None
    growth_locked: bool = False

    def train(self, turn: Turn, training: Training) -> tuple[dict, Result] | None:
        """Try one safe purchase, then let the next observation show the new build."""
        build = training.overview()
        if build.locked or not build.unspent:
            self.growth_locked = build.locked
            self.training_checked_at = turn.hit_dice
            return None
        skill = "spellcasting" if "spellcasting" in build.starting_skills else "heavy_weaponry"
        priorities = [(TrainingKind.TALENT, "rattling_blow"), (TrainingKind.TALENT, "dislocating_strike"),
                      (TrainingKind.SKILL, skill)]
        pools = training.pools()
        for kind, identity in priorities:
            eligible = [p for p in pools if p.available and kind in p.allowed]
            if not eligible:
                continue
            option = next((o for o in training.options(kind) if o.identity == identity and not o.forms), None)
            if option is None:
                continue
            for pool in eligible:
                try:
                    preview = training.preview(pool, option)
                except TrainingRefused as error:
                    if error.result.status is Outcome.REJECTED and not error.result.stale:
                        continue
                    raise
                if not preview.would_die:
                    return {"type": "purchase", "kind": kind.value, "id": identity}, training.confirm(preview)
        self.training_checked_at = turn.hit_dice
        return None

    def observe(self, turn: Turn, combat: CombatReport | None = None) -> None:
        here = turn.cell
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
            identity, count, reserve = self.pending_meal
            remaining = sum(item.quantity for item in turn.inventory if item.identity == identity)
            if remaining < count and turn.nutrition:
                gain = turn.nutrition.reserve - reserve
                if gain > 0:
                    self.food_value[identity] = min(gain, self.food_value.get(identity, gain))
            self.pending_meal = None
        health = turn.health
        if health and health.current * 5 >= health.maximum * 4:
            self.recovering_from_pursuit = False
        loss = self.previous_health - health.current if health and self.previous_health is not None else 0
        self.previous_health = health.current if health else None
        report = turn.recent_combat if combat is None else combat
        for effect in report.outgoing:
            target = effect.target
            if (target is not None and target in self.attempted_polymorph and target not in self.resisted_polymorph
                    and effect.ability == "rogue.wand_polymorph" and effect.outcome is AbilityOutcome.UNAFFECTED):
                self.attempted_polymorph.remove(target)
                self.resisted_polymorph.add(target)
        incoming = report.incoming
        losses = turn.restorable_losses
        lasting_injury = losses and self.previous_losses and losses.increased_since(self.previous_losses)
        self.previous_losses = losses
        if loss > 0 or lasting_injury:
            # Net HP loss is an estimate. Multiple possible sources make attribution ambiguous.
            sources = {event.caster for event in incoming if event.outcome is AbilityOutcome.APPLIED}
            if len(sources) == 1 and (source := next(iter(sources))) is not None:
                if loss > 0:
                    self.observed_loss[source] = max(loss, self.observed_loss[source])
                for foe in turn.hostiles:
                    if foe.target == source:
                        if loss > 0:
                            self.observed_loss_by_name[foe.name] = max(loss, self.observed_loss_by_name[foe.name])
                        if lasting_injury:
                            self.draining_creatures.add(foe.name)
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
        if self.pending_stair:
            attempt = self.pending_stair
            if here is None or not turn.can_act:
                return
            arrived = turn.stair_arrival(attempt)
            if arrived is None:
                return
            if arrived:
                if self.fled_via_stair:
                    reverse = Direction.UP if attempt.direction is Direction.DOWN else Direction.DOWN
                    self.unsafe_stairs[attempt.landing or here.key, reverse] = self.danger_until.get(
                        attempt.origin, turn.tick)
                self.cancelled_stair_origin = None
                if attempt.direction is Direction.DOWN:
                    previous = self.maps[self.depth]
                    upstairs = previous.stairs(Direction.UP)
                    distance = previous.distance(attempt.origin, upstairs)
                    if distance is not None:
                        self.ascent_ticks_by_floor[self.depth] = (distance + 1) * (turn.tick - attempt.tick)
                    # Forecast unfamiliar floors from the first outbound visit.
                    self.descent_ticks_by_floor.setdefault(self.depth, turn.tick - self.entered_floor_at)
                    self.depth += 1
                else:
                    self.depth -= 1
                    self.recovering_from_pursuit = bool(
                        health
                        and health.current * 5 < health.maximum * 4
                        and not self.carrying_objective(turn)
                    )
                self.entered_floor_at = turn.tick
            self.pending_stair = None
            self.fled_via_stair = False
        self.maps.setdefault(self.depth, FloorMap()).observe(turn)
        if here and here.lit:
            self.attempted_room_light.discard(self.depth)

    def admitted(self, turn: Turn, action: Action, receipt: Result | None = None,
                 *, reason: str | None = None) -> None:
        if reason == "cancel_stalled_stair" and self.pending_stair:
            self.cancelled_stair_origin = self.pending_stair.origin
            self.wait_after_cancel = True
        elif reason == "wait_after_cancel":
            self.wait_after_cancel = False
        if isinstance(action, Zap) and action.device.charges is None:
            self.tried_unmeasured_devices.add(action.device.item.target)
        if isinstance(action, Zap) and reason == "light_room":
            self.attempted_room_light.add(self.depth)
        if reason in {"slow_approach", "polymorph_threat"} and isinstance(action, Zap) and isinstance(action.target, Cell):
            attempted = self.attempted_slow if reason == "slow_approach" else self.attempted_polymorph
            attempted.update(foe.target for foe in turn.hostiles if action.target in turn.cells_of(foe))
        if isinstance(action, Move) and turn.cell:
            if reason == "seek_stairs" and self.depth in self.food_scouts:
                deadline, stock = self.food_scouts[self.depth]
                if turn.tick < deadline:
                    self.food_scouts[self.depth] = turn.tick, stock
            self.last_exploration_origin = turn.cell.key if reason == "explore" else None
            self.corridor_stand = None
            if reason == "take_corridor" and (entrance := turn.cell_at(action.direction.offset)):
                # Allow pursuers to reach the choke even when the doorway breaks sight.
                self.corridor_stand = entrance.key, turn.tick + 600
            self.last_retreat_origin = turn.cell.key if reason == "retreat_to_stairs" else None
            if reason in {"retreat", "break_contact"} and turn.health and turn.health.current * 2 < turn.health.maximum:
                self.recovering_from_pursuit = True
        if isinstance(action, UseItem) and action.use is ItemUse.DRINK and action.item.identity is None:
            self.sampled_potions.add(action.item.appearance or action.item.name)
        if turn.cell and isinstance(action, UseAbility) and action.ability.identity == "rogue.search":
            self.searches[turn.cell.key] += 1
            if receipt and self.maps[self.depth].is_dead_end(turn.cell.key):
                self.pending_end_search = turn.cell.key, receipt
            self.wall_searches.update(
                c.key for c in self.maps[self.depth].adjacent(turn.cell.key) if c.wall
            )
        if isinstance(action, UseItem) and action.use is ItemUse.READ and action.item.identity is None:
            self.tried_unknown_scroll = True
            self.sampled_scrolls.add(action.item.appearance or action.item.name)
        if (
            isinstance(action, UseItem)
            and action.use is ItemUse.EAT
            and action.item.identity
            and turn.nutrition
        ):
            count = sum(i.quantity for i in turn.inventory if i.identity == action.item.identity)
            self.pending_meal = action.item.identity, count, turn.nutrition.reserve
        if isinstance(action, Shot):
            self.last_shot_tick = turn.tick
        if turn.cell and isinstance(action, Move) and action.direction in (Direction.UP, Direction.DOWN):
            self.pending_stair = turn.stair_attempt(action, receipt)
            self.fled_via_stair = reason in {"retreat_upstairs", "retreat_downstairs"}

    def carrying_objective(self, turn: Turn) -> bool:
        return any(item.identity == self.objective for item in turn.inventory)

    @staticmethod
    def trapped(turn: Turn) -> bool:
        return turn.has_condition("rogue.bear_trap")

    def damage_estimate(self, foe: Creature) -> int:
        return max(self.observed_loss[foe.target], self.observed_loss_by_name[foe.name])

    def nearby_drainer(self, foe: Creature) -> bool:
        # Allow one closing step before a creature can strike in contact.
        return foe.distance <= 2 and foe.name in self.draining_creatures

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
            for item in turn.inventory:
                if ItemUse.EAT in item.uses:
                    return UseItem(item, ItemUse.EAT), "eat"

        if lethal:
            for item in turn.inventory:
                if item.identity == "item.scroll.teleportation" and ItemUse.READ in item.uses:
                    return UseItem(item, ItemUse.READ), "escape_pursuit"

        if adjacent and (two_hit_threat or injured or surrounded):
            foe = max(adjacent, key=self.damage_estimate)
            for device in turn.devices:
                if device.item.identity != "item.wand.wand-teleport-away" or not self.can_spend_charge(device):
                    continue
                if zap := turn.aim_device(device, foe):
                    return zap, "displace_threat"

        if adjacent and health and (decision := self.counter_threat(turn, health.current, in_melee=True)):
            return decision
        if escape := self.retreat_to_stairs(turn):
            return escape
        if not adjacent and sum(foe.distance <= 3 for foe in turn.hostiles) >= 2:
            if step := self.take_corridor(turn):
                return step, "take_corridor"
        if turn.hostiles and not adjacent and health and (decision := self.counter_threat(turn, health.current)):
            return decision

        if (
            turn.hostiles
            and (
                injured
                or self.recovering_from_pursuit
                or surrounded
                or two_hit_threat
                or any(self.nearby_drainer(foe) for foe in turn.hostiles)
            )
            and not self.trapped(turn)
            and not turn.holds
        ):
            if decision := self.evade(turn):
                return decision

        if turn.hostiles:
            for device in turn.devices:
                if device.item.identity not in ELEMENTAL_WANDS or not self.can_spend_charge(device):
                    continue
                for foe in sorted(turn.hostiles, key=self.damage_estimate, reverse=True):
                    if zap := turn.aim_device(device, foe, reflected_ray_range=ELEMENTAL_RAY_RANGE):
                        return zap, "elemental_ray"

        if turn.hostiles and self.has_arrows(turn):
            nearest = min(foe.distance for foe in turn.hostiles)
            if nearest >= 5 or nearest <= 2:
                if weapon := ready_weapon(turn, bow=nearest >= 5):
                    return weapon

        if turn.hostiles and turn.tick != self.last_shot_tick:
            can_strike = any(turn.attack(foe) is not None for foe in adjacent)
            for shot in turn.ranged_attacks(max_distance=8):
                if shot.target.hostile and (
                    shot.launcher or (not can_strike and not shot.equipped_throw and shot.target.distance <= 4)
                ):
                    return shot, "launch_projectile" if shot.launcher else "throw_projectile"

        if lethal and not self.tried_unknown_scroll:
            for item in turn.inventory:
                if item.identity is None and ItemUse.READ in item.uses:
                    return UseItem(item, ItemUse.READ), "test_unknown_magic"

        if turn.hostiles and not self.has_arrows(turn):
            if decision := ready_weapon(turn, bow=False):
                return decision

        injury_priority: dict[Injury | None, int] = {
            Injury.NEAR_DEATH: 0,
            Injury.BADLY_WOUNDED: 1,
            Injury.WOUNDED: 2,
        }
        def attack_order(foe: Creature):
            opens_exit = any(
                (cell.stairs_up and self.depth > 1) or
                (cell.stairs_down and self.depth < self.retrieval_depth and not self.carrying_objective(turn))
                for cell in turn.cells_of(foe)
            )
            return injury_priority.get(foe.injury, 3), not opens_exit, foe.distance

        for foe in sorted(turn.hostiles, key=attack_order):
            if attack := turn.attack(foe, technique="rattling_blow"):
                return attack, "melee"
            if attack := turn.attack(foe):
                return attack, "melee"

        if self.last_unseen_attack and not turn.hostiles:
            origin, tick = self.last_unseen_attack
            if tick <= turn.tick < tick + 400:
                sight_ring = "item.ring.ring-see-invisible"
                wearing_sight = any(slot.occupant and slot.occupant.identity == sight_ring for slot in turn.equipment)
                if (not wearing_sight and not turn.has_condition("rogue.see_invisible")
                        and not turn.has_condition("rogue.blindness")):
                    for choice in turn.equip_choices():
                        if (choice.item.identity == sight_ring and not choice.item.known_cursed
                                and choice.slot.occupant is None):
                            return choice, "reveal_attacker"
                    for item in turn.inventory:
                        if ItemUse.DRINK in item.uses and item.identity == "item.potion.see-invisible":
                            return UseItem(item, ItemUse.DRINK), "reveal_attacker"
                if not turn.holds and not self.trapped(turn):
                    for direction in turn.walkable_directions:
                        offset = direction.offset
                        if any(cell.offset == offset and cell.key != origin for cell in turn.visible_cells):
                            return Move(direction), "evade_unseen_attack"

        if turn.holds:
            return Escape(turn.holds[0]), "escape_hold"

        return None

    @staticmethod
    def take_corridor(turn: Turn) -> Move | None:
        """Enter a narrow passage with a clear retreat and the pursuers on the far side."""
        for direction in turn.walkable_directions:
            dx, dy, _ = direction.offset
            if dx and dy:
                continue
            if not all(turn.distance_to(foe, from_offset=direction.offset) > foe.distance
                       for foe in turn.hostiles if foe.distance <= 3):
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

    def counter_threat(self, turn: Turn, health: int, *, in_melee: bool = False) -> tuple[Zap, str] | None:
        devices = [(device, reason) for identity, reason in CONTROL_DEVICES
                   if not in_melee or reason == "polymorph_threat"
                   for device in turn.devices
                   if device.item.identity == identity and self.can_spend_charge(device)]
        if not devices:
            return None
        for foe in sorted(turn.hostiles, key=self.damage_estimate, reverse=True):
            damage = self.damage_estimate(foe)
            if (foe.distance > 3 or
                    (2 * damage < health and foe.name not in self.draining_creatures)):
                continue
            for device, reason in devices:
                if reason == "slow_approach" and foe.target in self.attempted_slow:
                    continue
                if reason == "polymorph_threat":
                    if foe.target in self.attempted_polymorph or foe.injury is not Injury.HEALTHY or damage < health:
                        continue
                    if foe.target in self.resisted_polymorph and (device.charges is None or any(
                        other.target != foe.target and turn.in_contact(other) for other in turn.hostiles
                    )):
                        continue
                if zap := turn.aim_device(device, foe, clear_horizontal_ray=True):
                    return zap, reason
        return None

    def safe_stairs(self, turn: Turn, direction: Direction) -> set[str]:
        return {key for key in self.maps[self.depth].stairs(direction)
                if self.unsafe_stairs.get((key, direction), 0) <= turn.tick}

    def retreat_to_stairs(self, turn: Turn) -> tuple[Action, str] | None:
        if not turn.hostiles or self.depth <= 1 or self.trapped(turn) or turn.holds:
            return None
        if turn.has_condition("rogue.levitation"):
            return None
        dangerous = any(
            foe.name.casefold() in {"griffin", "dragon", "jabberwock", "wraith"}
            or self.nearby_drainer(foe)
            for foe in turn.hostiles
        )
        if not (dangerous or self.recovering_from_pursuit):
            return None
        avoid = {self.last_retreat_origin} if self.last_retreat_origin is not None else set()
        exits = self.safe_stairs(turn, Direction.UP)
        if turn.cell and turn.cell.key in exits:
            return Move(Direction.UP), "retreat_upstairs"
        can_descend = dangerous and self.depth < self.retrieval_depth and not self.carrying_objective(turn)
        if can_descend:
            downstairs = self.safe_stairs(turn, Direction.DOWN)
            if turn.cell and turn.cell.key in downstairs:
                return Move(Direction.DOWN), "retreat_downstairs"
            exits.update(downstairs)
        for foe in turn.hostiles:
            avoid.update(cell.key for cell in turn.cells_of(foe))
        memory = self.maps[self.depth]
        exits.difference_update(avoid)
        nearby = [cell.offset for cell in turn.visible_cells if cell.key in exits and cell.offset[2] == 0]
        step = None
        if nearby:
            destination = min(nearby, key=lambda at: sum(map(abs, at)))
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
            if approaches:
                step = Move(max(approaches)[2])
        if step is None:
            step = memory.toward(turn, exits, avoid=avoid)
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
        return step, "retreat_to_stairs"

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
        best, direction = max(choices, key=lambda choice: choice[0])
        if best > current:
            return Move(direction), "retreat"
        routes = []
        for first_step, direction in choices:
            if first_step < current:
                continue
            cell = turn.cell_at(direction.offset)
            if cell is None:
                continue
            for second in turn.walkable_from(cell):
                offset = tuple(a + b for a, b in zip(direction.offset, second.offset))
                after = safety(offset)
                if after > current:
                    routes.append((after, direction))
        if routes:
            return Move(max(routes, key=lambda route: route[0])[1]), "break_contact"
        return None

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
        reserve = turn.nutrition
        rest_budget = reserve.ticks_until_empty if reserve else None
        # A guarded exit still counts toward the return budget.
        if rest_budget is not None and (stairs or memory.stairs(direction)):
            rest_budget -= sum(ticks for floor, ticks in self.ascent_ticks_by_floor.items()
                               if not returning or floor <= self.depth)

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
                if (cell.key in memory.visited or cell.key in stairs) and cell.key not in self.danger_until:
                    return food_route or (action, "recover_en_route")
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
                        and (item.appearance or item.name) not in self.sampled_potions):
                    return UseItem(item, ItemUse.DRINK), "test_unknown_potion"
        if weapon := ready_weapon(turn, bow=False):
            return weapon
        # An unread scroll might open a paid identification choice or move the player.
        # Try it while healthy, with something to identify and a usable exit.
        if (can_experiment and 3 <= self.depth < self.retrieval_depth and turn.cell and turn.cell.stairs_down
                and self.unsafe_stairs.get((turn.cell.key, Direction.DOWN), 0) <= turn.tick
                and turn.cell.key not in self.danger_until and not turn.holds and not self.trapped(turn)
                and not turn.has_condition("rogue.levitation")
                and any(i.identity is None and i.category in {"wand", "ring", "potion"} for i in turn.inventory)):
            for item in turn.inventory:
                if (item.identity is None and ItemUse.READ in item.uses
                        and (item.appearance or item.name) not in self.sampled_scrolls):
                    return UseItem(item, ItemUse.READ), "study_scroll_at_exit"
        return None

    @staticmethod
    def has_arrows(turn: Turn) -> bool:
        return any(item.identity == "item.weapon.arrow" for item in turn.inventory)

    def search_dead_end(self, turn: Turn, memory: FloorMap) -> UseAbility | None:
        here = turn.cell
        if (here is None or here.stairs_up or here.stairs_down or turn.hostiles
                or self.completed_end_searches[here.key] >= DEAD_END_SEARCHES
                or not memory.is_dead_end(here.key)):
            return None
        ability = next((a for a in turn.abilities if a.identity == "rogue.search" and a.ready), None)
        return UseAbility(ability) if ability else None

    def search(self, turn: Turn, memory: FloorMap, returning: bool, *,
               allow_unseen: bool = True, wall_limit: int = 3) -> tuple[Action, str] | None:
        if returning and (action := self.search_dead_end(turn, memory)):
            return action, "search"
        ability = next((a for a in turn.abilities if a.identity == "rogue.search" and a.ready), None)
        if ability is None or turn.cell is None:
            return None
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
                if attempts >= wall_limit + 1:
                    continue
                untested = [c for c in walls if attempts < wall_limit and self.wall_searches[c.key] < wall_limit]
                value = sum(wall_limit - self.wall_searches[c.key] for c in untested)
            if not untested and (not allow_unseen or len(neighbors) == 4):
                continue
            near_exit = returning and any(
                n.stairs_up for wall in untested for n in memory.adjacent(wall.key)
            )
            # One search checks several boundaries. Favor corridor ends, especially near a lost exit.
            rank = near_exit, bool(untested), value - distance * 2, -distance
            sites.append((rank, site.key))
        # Equal ranks keep the map traversal order. Cell keys identify places within the game.
        for _, key in sorted(sites, key=lambda site: site[0], reverse=True):
            if key == turn.cell.key:
                return UseAbility(ability), "search"
            if step := memory.toward(turn, {key}):
                return step, "search"
        return None

    def provision(self, turn: Turn, memory: FloorMap, returning: bool) -> tuple[Action, str] | None:
        nutrition = turn.nutrition
        if nutrition is None:
            return None
        required = sum(ticks for floor, ticks in self.ascent_ticks_by_floor.items() if floor < self.depth)
        if not returning and self.descent_ticks_by_floor:
            recent = sorted(self.descent_ticks_by_floor)[-5:]
            round_trip = median_low(
                [
                    self.descent_ticks_by_floor[floor] + self.ascent_ticks_by_floor.get(floor, 0)
                    for floor in recent
                ]
            )
            required += max(0, self.retrieval_depth - self.depth) * round_trip
        if nutrition.ticks_until_empty >= required:
            return None
        meals = [item for item in turn.inventory if ItemUse.EAT in item.uses]
        provisions = 0
        for item in meals:
            if item.identity is None or item.identity not in self.food_value:
                return None
            provisions += self.food_value[item.identity] * item.quantity
        if nutrition.ticks_until_empty + provisions * nutrition.ticks_per_unit >= required:
            return None
        food = {
            key for key, items in memory.items.items() if any(item.category == "food" for item in items)
        }
        if step := memory.toward(turn, food):
            return step, "forage"
        if returning or self.depth >= self.retrieval_depth:
            return None
        count = sum(item.quantity for item in meals)
        if self.depth not in self.food_scouts:
            if turn.cell is None or not turn.cell.stairs_down:
                return None
            self.food_scouts[self.depth] = turn.tick + nutrition.ticks_until_empty // 4, count
        deadline, starting_count = self.food_scouts[self.depth]
        if turn.tick < deadline and count <= starting_count:
            if step := memory.toward(turn, memory.frontier):
                return step, "scout_for_food"
        return None

    def stalled(self, turn: Turn, previous: Result | None) -> tuple[Action | None, str]:
        stair = self.pending_stair
        if (stair and previous and stair.receipt == previous and stair.origin != self.cancelled_stair_origin
                and turn.can_cancel and len(turn.activities) == 1 and turn.outcome_of(previous) is ActivityPhase.QUEUED):
            return ActivityChoice.CANCEL, "cancel_stalled_stair"
        return None, "readiness_timeout"

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
        if self.wait_after_cancel:
            return Wait(), "wait_after_cancel"

        if decision := self.combat(turn):
            return decision

        if self.corridor_stand and turn.hunger not in {"weak", "empty"}:
            entrance, until = self.corridor_stand
            if turn.cell.key == entrance and turn.tick < until:
                return Wait(), "hold_corridor"

        memory = self.maps[self.depth]
        hovering = turn.has_condition("rogue.levitation")
        ground_items = () if hovering else turn.ground_items
        if turn.hunger in {"hungry", "weak", "empty"}:
            for item in turn.inventory:
                if ItemUse.EAT in item.uses:
                    return UseItem(item, ItemUse.EAT), "eat"
            for ground in ground_items:
                if ground.reachable and ground.item.category == "food":
                    return Pickup(ground.item), "take"
            food = {
                key for key, items in memory.items.items() if any(item.category == "food" for item in items)
            }
            if not hovering and not self.trapped(turn) and (step := memory.toward(turn, food)):
                return step, "forage"

        returning = self.carrying_objective(turn)
        item_goals = set()
        supplies = set()
        nearby_loot = set()
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
        for ground in sorted(ground_items, key=lambda row: row.item.identity != self.objective):
            item = ground.item
            useful = item.category in LOOT_CLASSES or item.identity == "item.misc.clean-bandage"
            if ground.reachable:
                if item.identity == self.objective or (safe_pickup and useful):
                    return Pickup(item), "take"
                continue
            wanted = item.identity == self.objective or item.category == "food"
            if item.identity in {"item.potion.healing", "item.potion.extra-healing"}:
                wanted = carried[item.identity] < 2
            elif not returning and not turn.hostiles and ground.cell and ground.cell.key not in self.guarded_supplies:
                if useful and not turn.cell.stairs_down:
                    nearby_loot.add(ground.cell.key)
                seek_armor = item.category == "armor" and not spare_armor_to_identify and (
                    item.identity is None or not carried[item.identity]
                )
                seek_ring = (item.category == "ring" and item.identity is not None and not carried[item.identity]
                             and not item.known_cursed and item.identity in ring_upgrades(turn))
                if seek_armor or seek_ring or stock[item.category] < DETOUR_STOCK.get(item.category, 0):
                    supplies.add(ground.cell.key)
            if item.identity == "item.weapon.arrow" and carried["item.weapon.shortbow"]:
                wanted = not turn.hostiles and carried[item.identity] < 8
            if not returning:
                wanted |= item.identity == "item.weapon.dart" and carried[item.identity] < 12
                wanted |= item.identity == "item.weapon.spear" and not carried[item.identity]
            if wanted and ground.cell:
                item_goals.add(ground.cell.key)
        if not turn.hostiles and (decision := self.prepare(turn)):
            return decision
        if self.trapped(turn):
            return Wait(), "wait_for_restraint"
        if not returning and not any(g.item.identity == self.objective for g in ground_items):
            if search := self.search_dead_end(turn, memory):
                return search, "search"
        if action := memory.toward(turn, item_goals):
            return action, "seek_item"
        if not returning and not hovering and not turn.hostiles and carried["item.weapon.shortbow"]:
            arrows = {key for key, items in memory.items.items()
                      if any(item.identity == "item.weapon.arrow" for item in items)}
            if arrows and (action := memory.toward(turn, arrows, avoid=self.danger_until.keys(), max_steps=6)):
                return action, "recover_arrows"
        if nearby_loot and (action := memory.toward(turn, nearby_loot, max_steps=2)):
            return action, "seek_supplies"
        if supplies and (action := memory.toward(turn, supplies, max_steps=3)):
            return action, "seek_supplies"

        if not hovering and not turn.hostiles and (decision := self.provision(turn, memory, returning)):
            return decision

        direction = Direction.UP if returning else Direction.DOWN
        if returning or self.depth < self.retrieval_depth:
            stairs = self.safe_stairs(turn, direction)
            if turn.cell.key in stairs:
                if hovering:
                    return Wait(), "wait_for_ground_contact"
                return Move(direction), "ascend" if returning else "descend"
            if action := memory.toward(turn, stairs):
                return action, "seek_stairs"

        if (turn.cell.lit is False and not turn.hostiles and self.depth not in self.attempted_room_light
                and not turn.has_condition("rogue.blindness")):
            for device in turn.devices:
                if (device.item.identity == "item.wand.wand-light" and device.activation is DeviceActivation.SELF_TARGET
                        and device.charges != 0):
                    return Zap(device, None), "light_room"
        avoid = self.danger_until.keys()
        if action := memory.toward_unexplored(turn, avoid=avoid):
            if isinstance(action, Move) and (next_cell := turn.cell_at(action.direction.offset)):
                if next_cell.key == self.last_exploration_origin:
                    alternate = memory.toward(turn, memory.frontier, avoid=avoid | {next_cell.key}, max_steps=3)
                    if alternate:
                        return alternate, "explore"
            return action, "explore"
        if avoid:
            # Accept the risk when every remaining route crosses a recent threat.
            if action := memory.toward_unexplored(turn):
                return action, "explore"
        fallback = None
        if returning and memory.stairs(Direction.UP):
            for item in turn.inventory:
                if item.identity == "item.scroll.teleportation" and ItemUse.READ in item.uses:
                    fallback = UseItem(item, ItemUse.READ), "find_return_route"
                    break
        if fallback is None:
            for item in turn.inventory:
                if item.identity is None and ItemUse.READ in item.uses:
                    fallback = UseItem(item, ItemUse.READ), "test_unknown_magic"
                    break
        if decision := self.search(turn, memory, returning, allow_unseen=fallback is None) or fallback:
            return decision
        if (not returning and not turn.hostiles and memory.stairs(Direction.UP)
                and not memory.stairs(Direction.DOWN)):
            # A failed Search leaves concealed doors looking like ordinary walls.
            if decision := self.search(turn, memory, returning, allow_unseen=False, wall_limit=6):
                return decision
        return None, "no_disclosed_route"


def play(game: Session, player: Adventurer, budget: int, emit: Callable[[dict], None],
         *, record_decisions: bool = False) -> dict:
    """Observe the outcome after the last budgeted submission, including victory."""
    count = 0
    deepest = 1
    acquired = False
    detail: dict[str, object] = {}
    equipment = EquipmentHistory()
    for decision in game.decisions(include_stalls=True):
        turn, combat = decision.turn, decision.combat
        equipment.observe(turn)
        if player.training_checked_at is None:
            player.training_checked_at = turn.hit_dice
        previous = decision.previous
        events = combat.records()
        acquired |= player.carrying_objective(turn)
        if turn.terminal:
            reason = "terminal"
            if turn.victorious:
                reason = "victory"
            elif turn.dead:
                reason = "death"
            detail.update(
                last_combat_events=turn.recent_combat.records() or events,
                events_since_previous_action=events,
                self_target=turn.self_target,
            )
            break
        player.observe(turn, combat)
        deepest = max(deepest, player.depth)
        if count >= budget:
            reason = "action_budget"
            break
        purchase = None
        action = None
        if (not player.growth_locked and turn.hit_dice is not None and turn.hit_dice != player.training_checked_at
                and not decision.stalled and turn.can_act and not turn.hostiles and not turn.identification
                and not turn.can_resume and not player.pending_stair):
            try:
                purchase = player.train(turn, game.training)
            except TrainingRefused as error:
                purchase = {"type": "training_preview"}, error.result
        if purchase:
            command, result = purchase
            reason = "earned_training"
        else:
            action, reason = player.stalled(turn, previous) if decision.stalled else player.choose(turn)
            if action is None:
                if decision.stalled:
                    detail.update(health=asdict(turn.health) if turn.health else None,
                                  nutrition_reserve=turn.food_reserve,
                                  conditions=[asdict(condition) for condition in turn.conditions])
                elif reason == "no_disclosed_route":
                    memory = player.maps[player.depth]
                    detail["navigation"] = {
                        "floor": player.depth,
                        "visited_cells": len(memory.visited),
                        "known_down_stairs": len(memory.stairs(Direction.DOWN)),
                        "searches_admitted": sum(player.searches[key] for key in memory.cells),
                    }
                break
            result = turn.submit(action)
            command = describe_action(action)
        count += 1
        receipt = {"intention": result.intention}
        if result.activity:
            receipt.update(type="admitted", **asdict(result.activity))
        emit(
            {
                **({"recording": turn.recording()} if record_decisions else {}),
                "floor": player.depth,
                "tick": turn.tick,
                "cell": turn.cell.key if turn.cell else None,
                "known_cells": len(player.maps[player.depth].cells),
                "reachable_pickups": [
                    {"item": ground.item.target, "category": ground.item.category,
                     "identity": ground.item.identity, "quantity": ground.item.quantity,
                     "cell": ground.cell.key if ground.cell else None}
                    for ground in turn.ground_items if ground.reachable
                ],
                **({"stalled": True} if decision.stalled else {}),
                "reason": reason,
                "action": command,
                "result": {"status": result.status.value, "receipt": receipt,
                           "code": result.code, "message": result.reason},
                "previous_intention_id": previous.intention if previous else None,
                "previous_intention_phase": decision.previous_phase,
                "health_recovery": asdict(turn.recovery) if turn.recovery else None,
                "nutrition_reserve": turn.food_reserve,
                "events_since_previous_action": events,
                "amulet_acquired": acquired,
            }
        )
        if result.status is Outcome.CONFIRMED:
            if action is not None:
                player.admitted(turn, action, result, reason=reason)
                equipment.admitted(action)
        elif not result.stale:
            previous = result
            reason = result.reason or result.status.value
            break
    else:
        raise RuntimeError("The decision stream ended before a terminal picture")
    detail.update(
        equipment_opportunities=equipment.counts(),
        last_inventory_tick=equipment.last_inventory_tick,
        last_carried_devices=equipment.last_devices,
        estimated_ascent_ticks_by_floor=player.ascent_ticks_by_floor,
        estimated_ascent_ticks=sum(player.ascent_ticks_by_floor.values()),
    )
    if record_decisions:
        detail["recording"] = turn.recording()
    if previous:
        detail.update(
            previous_intention_id=previous.intention, previous_intention_phase=turn.outcome_of(previous)
        )
    return {
        "terminal": reason,
        "actions": count,
        "max_depth": deepest,
        "amulet_acquired": acquired,
        "tick": turn.tick,
        **detail,
    }
