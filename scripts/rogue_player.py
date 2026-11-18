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
    Direction,
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
    Unequip,
    UseAbility,
    UseItem,
    Wait,
    describe_action,
)


@dataclass
class Adventurer:
    retrieval_depth: int
    objective: str
    depth: int = 1
    maps: dict[int, FloorMap] = field(default_factory=dict)
    last_shot_tick: int | None = None
    searches: Counter[str] = field(default_factory=Counter)
    wall_searches: Counter[str] = field(default_factory=Counter)
    ascent_ticks_by_floor: dict[int, int] = field(default_factory=dict)
    descent_ticks_by_floor: dict[int, int] = field(default_factory=dict)
    entered_floor_at: int | None = None
    food_value: dict[str, int] = field(default_factory=dict)
    pending_meal: tuple[str, int, int] | None = None
    # A floor gets one bounded detour. A confirmed increase in supplies ends it early.
    food_scouts: dict[int, tuple[int, int]] = field(default_factory=dict)
    pending_stair: StairAttempt | None = None
    cancelled_stair_origin: str | None = None
    wait_after_cancel: bool = False

    last_unseen_attack: tuple[str, int] | None = None
    previous_health: int | None = None
    observed_loss: Counter[str] = field(default_factory=Counter)
    observed_loss_by_name: Counter[str] = field(default_factory=Counter)
    tried_unknown_scroll: bool = False
    sampled_potions: set[str] = field(default_factory=set)
    guarded_supplies: set[str] = field(default_factory=set)
    previous_losses: RestorableLosses | None = None
    draining_creatures: set[str] = field(default_factory=set)
    danger_until: dict[str, int] = field(default_factory=dict)
    recovering_from_pursuit: bool = False
    last_retreat_origin: str | None = None

    def observe(self, turn: Turn, combat: CombatReport | None = None) -> None:
        here = turn.cell
        for ground in turn.ground_items:
            if (ground.cell and ground.item.category in {"potion", "scroll"} and
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
                self.cancelled_stair_origin = None
                if attempt.direction is Direction.DOWN:
                    previous = self.maps[self.depth]
                    upstairs = {key for key, cell in previous.cells.items() if cell.stairs_up}
                    distance = previous.distance(attempt.origin, upstairs)
                    if distance is not None:
                        self.ascent_ticks_by_floor[self.depth] = (distance + 1) * (turn.tick - attempt.tick)
                    self.descent_ticks_by_floor[self.depth] = turn.tick - self.entered_floor_at
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
        self.maps.setdefault(self.depth, FloorMap()).observe(turn)

    def admitted(self, turn: Turn, action: Action, receipt: Result | None = None,
                 *, reason: str | None = None) -> None:
        if reason == "cancel_stalled_stair" and self.pending_stair:
            self.cancelled_stair_origin = self.pending_stair.origin
            self.wait_after_cancel = True
        elif reason == "wait_after_cancel":
            self.wait_after_cancel = False
        if isinstance(action, Move) and turn.cell:
            self.last_retreat_origin = turn.cell.key if reason == "retreat_to_stairs" else None
            if reason in {"retreat", "break_contact"} and turn.health and turn.health.current * 2 < turn.health.maximum:
                self.recovering_from_pursuit = True
        if isinstance(action, UseItem) and action.use is ItemUse.DRINK and action.item.identity is None:
            self.sampled_potions.add(action.item.appearance or action.item.name)
        if turn.cell and isinstance(action, UseAbility) and action.ability.identity == "rogue.search":
            self.searches[turn.cell.key] += 1
            self.wall_searches.update(
                c.key for c in self.maps[self.depth].adjacent(turn.cell.key) if c.wall
            )
        if isinstance(action, UseItem) and action.use is ItemUse.READ and action.item.identity is None:
            self.tried_unknown_scroll = True
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

    def carrying_objective(self, turn: Turn) -> bool:
        return any(item.identity == self.objective for item in turn.inventory)

    @staticmethod
    def trapped(turn: Turn) -> bool:
        return turn.has_condition("rogue.bear_trap")

    def combat(self, turn: Turn) -> tuple[Action, str] | None:
        health = turn.health
        injured = health is not None and health.current * 2 < health.maximum
        adjacent = [foe for foe in turn.hostiles if turn.in_contact(foe)]
        surrounded = len(adjacent) >= 2
        exposure = sum(max(self.observed_loss[foe.target], self.observed_loss_by_name[foe.name]) for foe in adjacent)
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

        if escape := self.retreat_to_stairs(turn):
            return escape

        if (
            turn.hostiles
            and (
                injured
                or self.recovering_from_pursuit
                or surrounded
                or two_hit_threat
                or any(c.name in self.draining_creatures for c in turn.hostiles)
            )
            and not self.trapped(turn)
            and not turn.holds
        ):
            if decision := self.evade(turn):
                return decision

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
            if decision := self.ready_weapon(turn, bow=False):
                return decision

        injury_priority: dict[Injury | None, int] = {
            Injury.NEAR_DEATH: 0,
            Injury.BADLY_WOUNDED: 1,
            Injury.WOUNDED: 2,
        }
        for foe in sorted(
            turn.hostiles, key=lambda foe: (injury_priority.get(foe.injury, 3), foe.distance)
        ):
            if attack := turn.attack(foe):
                return attack, "melee"

        if self.last_unseen_attack and not turn.hostiles:
            origin, tick = self.last_unseen_attack
            if tick <= turn.tick < tick + 400:
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

    def retreat_to_stairs(self, turn: Turn) -> tuple[Action, str] | None:
        if not turn.hostiles or self.depth <= 1 or self.trapped(turn) or turn.holds:
            return None
        if turn.has_condition("rogue.levitation"):
            return None
        dangerous = any(
            foe.name.casefold() in {"griffin", "dragon", "jabberwock", "wraith"}
            or foe.name in self.draining_creatures
            for foe in turn.hostiles
        )
        if not (dangerous or self.recovering_from_pursuit):
            return None
        if turn.cell and turn.cell.stairs_up:
            return Move(Direction.UP), "retreat_upstairs"
        if (dangerous and turn.cell and turn.cell.stairs_down and self.depth < self.retrieval_depth
                and not self.carrying_objective(turn)):
            return Move(Direction.DOWN), "retreat_downstairs"
        avoid = {self.last_retreat_origin} if self.last_retreat_origin is not None else set()
        for foe in turn.hostiles:
            for offset in foe.cells:
                if cell := turn.cell_at(offset):
                    avoid.add(cell.key)
        nearby = [cell.offset for cell in turn.visible_cells if cell.stairs_up and cell.offset[2] == 0]
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
                return Move(max(approaches)[2]), "retreat_to_stairs"
        upstairs = {key for key, cell in self.maps[self.depth].cells.items() if cell.stairs_up}
        if step := self.maps[self.depth].toward(turn, upstairs, avoid=avoid):
            return step, "retreat_to_stairs"
        return None

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
        stairs = {
            key for key, cell in memory.cells.items() if (cell.stairs_up if returning else cell.stairs_down)
        }
        if not self.trapped(turn):
            if turn.cell.key in self.danger_until:
                if action := memory.toward(turn, memory.visited - self.danger_until.keys()):
                    return action, "reposition_to_recover"
            action = memory.toward(turn, stairs)
            if isinstance(action, Move) and (cell := turn.cell_at(action.direction.offset)):
                if (cell.key in memory.visited or cell.key in stairs) and cell.key not in self.danger_until:
                    return action, "recover_en_route"
        waiting = missing * clock.period_ticks - clock.elapsed_ticks
        reserve = turn.nutrition
        return_trip = sum(self.ascent_ticks_by_floor.values()) if stairs else 0
        if reserve is None or reserve.ticks_until_empty > waiting + return_trip:
            return Wait(), "recover"
        return None

    def prepare(self, turn: Turn) -> tuple[Action, str] | None:
        if turn.restorable_losses and turn.restorable_losses.strength:
            for item in turn.inventory:
                if item.identity == "item.potion.restore-strength" and ItemUse.DRINK in item.uses:
                    return UseItem(item, ItemUse.DRINK), "restore_strength"
        for choice in turn.equip_choices():
            if choice.slot.occupant is None and choice.item.identity in {
                "item.ring.ring-searching",
                "item.ring.ring-slow-digestion",
                "item.ring.ring-regeneration",
                "item.ring.ring-protection",
            }:
                if choice.item.identity == "item.ring.ring-slow-digestion":
                    return choice, "conserve_food"
                return choice, "upgrade_equipment"
        if action := self.recovery_action(turn):
            return action
        for item in turn.inventory:
            if ItemUse.DRINK in item.uses and item.identity in {
                "item.potion.gain-strength",
                "item.potion.raise-level",
            }:
                return UseItem(item, ItemUse.DRINK), "permanent_growth"
            if (
                ItemUse.READ in item.uses
                and item.identity
                and item.identity.startswith("item.scroll.identify-")
            ):
                category = item.identity.removeprefix("item.scroll.identify-")
                if any(other.identity is None and other.category == category for other in turn.inventory):
                    return UseItem(item, ItemUse.READ), "identify_gear"
        health = turn.health
        if health and health.current * 5 >= health.maximum * 4 and not self.carrying_objective(turn):
            for item in turn.inventory:
                if (item.identity is None and ItemUse.DRINK in item.uses
                        and (item.appearance or item.name) not in self.sampled_potions):
                    return UseItem(item, ItemUse.DRINK), "test_unknown_potion"
        return self.ready_weapon(turn, bow=self.has_arrows(turn))

    @staticmethod
    def has_arrows(turn: Turn) -> bool:
        return any(item.identity == "item.weapon.arrow" for item in turn.inventory)

    @staticmethod
    def ready_weapon(turn: Turn, *, bow: bool) -> tuple[Action, str] | None:
        wanted = "item.weapon.shortbow" if bow else "item.weapon.mace"
        weapon = next(
            (
                choice
                for choice in turn.equip_choices()
                if choice.item.identity == wanted and choice.slot.kind == "weapon"
            ),
            None,
        )
        if weapon:
            if weapon.slot.occupant:
                return Unequip(weapon.slot), "change_weapon"
            return weapon, "equip_bow" if bow else "equip_melee"

        return None

    def search(self, turn: Turn, memory: FloorMap, returning: bool, *,
               allow_unseen: bool = True) -> tuple[Action, str] | None:
        ability = next((a for a in turn.abilities if a.identity == "rogue.search" and a.ready), None)
        if ability is None or turn.cell is None:
            return None
        sites = []
        for site, distance in memory.reachable(turn.cell.key):
            attempts = self.searches[site.key]
            if attempts >= 4:
                continue
            neighbors = memory.adjacent(site.key)
            walls = [c for c in neighbors if c.wall]
            untested = [c for c in walls if attempts < 3 and self.wall_searches[c.key] < 3]
            if not untested and (not allow_unseen or len(neighbors) == 4):
                continue
            near_exit = returning and any(
                n.stairs_up for wall in untested for n in memory.adjacent(wall.key)
            )
            # One search checks several boundaries. Favor corridor ends, especially near a lost exit.
            value = sum(3 - self.wall_searches[c.key] for c in untested) + 3 * (len(walls) >= 3)
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
        if prompt := turn.identification:
            unknown = next((item for item in prompt.items if item.identity is None), None)
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
        carried: Counter[str | None] = Counter()
        stock: Counter[str] = Counter()
        for item in turn.inventory:
            carried[item.identity] += item.quantity
            stock[item.category] += item.quantity
        for ground in sorted(ground_items, key=lambda row: row.item.identity != self.objective):
            item = ground.item
            wanted = item.identity == self.objective or item.category == "food"
            if item.identity in {"item.potion.healing", "item.potion.extra-healing"}:
                wanted = carried[item.identity] < 2
            elif not returning and item.category in {"potion", "scroll"} and stock[item.category] < 4:
                if ground.reachable:
                    wanted = not turn.hostiles or bool(turn.health and turn.health.current * 5 >= turn.health.maximum * 4
                                                      and all(turn.distance_to(foe) > 1 for foe in turn.hostiles))
                elif not turn.hostiles and ground.cell and ground.cell.key not in self.guarded_supplies:
                    supplies.add(ground.cell.key)
            if item.identity == "item.weapon.arrow" and carried["item.weapon.shortbow"]:
                wanted = (ground.reachable and carried[item.identity] < 32) or (
                    not turn.hostiles and carried[item.identity] < 8
                )
            if not returning:
                wanted |= item.identity == "item.weapon.dart" and carried[item.identity] < 12
                wanted |= item.identity == "item.weapon.spear" and not carried[item.identity]
            if wanted:
                if ground.reachable:
                    return Pickup(item), "take"
                if cell := ground.cell:
                    item_goals.add(cell.key)
        if not turn.hostiles and (decision := self.prepare(turn)):
            return decision
        if self.trapped(turn):
            return Wait(), "wait_for_restraint"
        if action := memory.toward(turn, item_goals):
            return action, "seek_item"
        if supplies and (action := memory.toward(turn, supplies, max_steps=3)):
            return action, "seek_supplies"

        if not hovering and not turn.hostiles and (decision := self.provision(turn, memory, returning)):
            return decision

        direction = Direction.UP if returning else Direction.DOWN
        if returning or self.depth < self.retrieval_depth:
            stairs = {
                cell.key
                for cell in memory.cells.values()
                if (cell.stairs_up if returning else cell.stairs_down)
            }
            if turn.cell.key in stairs:
                if hovering:
                    return Wait(), "wait_for_ground_contact"
                return Move(direction), "ascend" if returning else "descend"
            if action := memory.toward(turn, stairs):
                return action, "seek_stairs"

        if action := memory.toward(turn, memory.frontier):
            return action, "explore"
        unvisited = {
            key for key, cell in memory.cells.items() if not cell.wall and key not in memory.visited
        }
        if action := memory.toward(turn, unvisited):
            return action, "explore"
        fallback = None
        if returning and any(cell.stairs_up for cell in memory.cells.values()):
            for item in turn.inventory:
                if item.identity == "item.scroll.teleportation" and ItemUse.READ in item.uses:
                    fallback = UseItem(item, ItemUse.READ), "find_return_route"
                    break
        if fallback is None:
            for item in turn.inventory:
                if item.identity is None and ItemUse.READ in item.uses:
                    fallback = UseItem(item, ItemUse.READ), "test_unknown_magic"
                    break
        return self.search(turn, memory, returning, allow_unseen=fallback is None) or fallback or (
            None, "no_disclosed_route"
        )


def play(game: Session, player: Adventurer, budget: int, emit: Callable[[dict], None],
         *, record_decisions: bool = False) -> dict:
    """Observe the outcome after the last budgeted submission, including victory."""
    count = 0
    deepest = 1
    acquired = False
    detail: dict[str, object] = {}
    for decision in game.decisions(include_stalls=True):
        turn, combat = decision.turn, decision.combat
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
                    "known_down_stairs": sum(cell.stairs_down for cell in memory.cells.values()),
                    "searches_admitted": sum(player.searches[key] for key in memory.cells),
                }
            break
        result = turn.submit(action)
        count += 1
        receipt = {"intention": result.intention}
        if result.activity:
            receipt.update(type="admitted", **asdict(result.activity))
        emit(
            {
                **({"recording": turn.recording()} if record_decisions else {}),
                "floor": player.depth,
                "tick": turn.tick,
                **({"stalled": True} if decision.stalled else {}),
                "reason": reason,
                "action": describe_action(action),
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
            player.admitted(turn, action, result, reason=reason)
        elif result.status is not Outcome.STALE:
            previous = result
            reason = result.reason or result.status.value
            break
    else:
        raise RuntimeError("The decision stream ended before a terminal picture")
    detail.update(
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
