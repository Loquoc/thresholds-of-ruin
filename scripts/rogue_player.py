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
    CombatJournal,
    CombatReport,
    Direction,
    Equip,
    Escape,
    FloorMap,
    Identify,
    Injury,
    ItemUse,
    Move,
    OpenDoor,
    Outcome,
    Pickup,
    RestorableLosses,
    Session,
    Shot,
    Turn,
    Unequip,
    UseAbility,
    UseItem,
    Wait,
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
    pending_stair: tuple[Direction, str, str | None, int] | None = None

    last_unseen_attack: tuple[str, int] | None = None
    previous_health: int | None = None
    observed_loss: Counter[str] = field(default_factory=Counter)
    tried_unknown_scroll: bool = False
    previous_losses: RestorableLosses | None = None
    draining_creatures: set[str] = field(default_factory=set)
    danger_until: dict[str, int] = field(default_factory=dict)
    recovering_after_ascent: bool = False

    def observe(self, turn: Turn, combat: CombatReport | None = None) -> None:
        here = turn.cell
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
            self.recovering_after_ascent = False
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
                if lasting_injury:
                    self.draining_creatures.update(c.name for c in turn.hostiles if c.target == source)
        if self.danger_until:
            self.danger_until = {
                key: expiry for key, expiry in self.danger_until.items() if expiry > turn.tick
            }
        if here and (turn.hostiles or (loss > 0 and incoming)):
            self.danger_until[here.key] = turn.tick + 1200
        if not turn.hostiles:
            self.tried_unknown_scroll = False
        if here and any(
            event.unseen
            and (
                event.outcome is AbilityOutcome.MISS
                or (event.outcome is AbilityOutcome.APPLIED and loss > 0)
            )
            for event in incoming
        ):
            self.last_unseen_attack = here.key, turn.tick
        if self.pending_stair and here and turn.can_act:
            direction, origin, expected_landing, started = self.pending_stair
            landed = here.stairs_up if direction is Direction.DOWN else here.stairs_down
            adjacent_landing = any(
                cell.key == expected_landing and cell.offset[2] == 0 for cell in turn.visible_cells
            )
            if here.key != origin and (landed or adjacent_landing):
                if direction is Direction.DOWN:
                    previous = self.maps[self.depth]
                    upstairs = {key for key, cell in previous.cells.items() if cell.stairs_up}
                    distance = previous.distance(origin, upstairs)
                    if distance is not None:
                        self.ascent_ticks_by_floor[self.depth] = (distance + 1) * (turn.tick - started)
                    self.descent_ticks_by_floor[self.depth] = turn.tick - self.entered_floor_at
                    self.depth += 1
                else:
                    self.depth -= 1
                    self.recovering_after_ascent = bool(
                        health
                        and health.current * 5 < health.maximum * 4
                        and not self.carrying_objective(turn)
                    )
                self.entered_floor_at = turn.tick
            self.pending_stair = None
        self.maps.setdefault(self.depth, FloorMap()).observe(turn)

    def admitted(self, turn: Turn, action: Action) -> None:
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
            landing = turn.stair_landing(action.direction)
            self.pending_stair = (
                action.direction,
                turn.cell.key,
                landing.key if landing else None,
                turn.tick,
            )

    def carrying_objective(self, turn: Turn) -> bool:
        return any(item.identity == self.objective for item in turn.inventory)

    @staticmethod
    def trapped(turn: Turn) -> bool:
        return any(
            condition.active and condition.identity == "rogue.bear_trap" and condition.remaining_ticks != 0
            for condition in turn.conditions
        )

    def combat(self, turn: Turn) -> tuple[Action, str] | None:
        health = turn.health
        injured = health is not None and health.current * 2 < health.maximum
        adjacent = [foe for foe in turn.hostiles if turn.in_contact(foe)]
        surrounded = len(adjacent) >= 2
        exposure = sum(self.observed_loss[foe.target] for foe in adjacent)
        lethal = health is not None and exposure >= health.current
        two_hit_threat = health is not None and exposure * 2 >= health.current
        if injured or lethal or (surrounded and health and health.current * 4 < health.maximum * 3):
            for item in turn.inventory:
                if ItemUse.DRINK in item.uses and item.identity in {
                    "item.potion.healing",
                    "item.potion.extra-healing",
                }:
                    return UseItem(item, ItemUse.DRINK), "heal"

        if (
            self.depth > 1
            and not self.trapped(turn)
            and not turn.holds
            and any(
                foe.name.casefold() in {"griffin", "dragon", "jabberwock", "wraith"}
                or foe.name in self.draining_creatures
                for foe in turn.hostiles
            )
        ):
            if turn.cell and turn.cell.stairs_up:
                return Move(Direction.UP), "retreat_upstairs"
            nearby = [cell.offset for cell in turn.visible_cells if cell.stairs_up and cell.offset[2] == 0]
            if nearby:
                destination = min(nearby, key=lambda at: sum(map(abs, at)))
                distance = sum(map(abs, destination))
                approaches = []
                for direction in turn.walkable_directions:
                    offset = direction.offset
                    remaining = sum(abs(a - b) for a, b in zip(offset, destination))
                    if remaining < distance:
                        clearance = min(
                            max(abs(a - b) for a, b in zip(offset, at))
                            for foe in turn.hostiles
                            for at in foe.cells
                        )
                        approaches.append((clearance, -remaining, direction))
                if approaches:
                    return Move(max(approaches)[2]), "retreat_to_stairs"
            upstairs = {key for key, cell in self.maps[self.depth].cells.items() if cell.stairs_up}
            if step := self.maps[self.depth].toward(turn, upstairs):
                return step, "retreat_to_stairs"

        if (
            turn.hostiles
            and (
                injured
                or self.recovering_after_ascent
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
            for shot in turn.ranged_attacks(max_distance=8):
                if shot.target.hostile and (
                    shot.launcher or (not shot.equipped_throw and shot.target.distance <= 4)
                ):
                    return shot, "launch_projectile" if shot.launcher else "throw_projectile"

        if lethal and not self.tried_unknown_scroll:
            for item in turn.inventory:
                if item.identity is None and ItemUse.READ in item.uses:
                    return UseItem(item, ItemUse.READ), "test_unknown_magic"

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

    @staticmethod
    def evade(turn: Turn) -> tuple[Move, str] | None:
        body = turn.body or ((0, 0, 0),)
        threats = [cell for foe in turn.hostiles for cell in foe.cells]

        def clearance(offset):
            return min(
                max(abs(a + delta - b) for a, delta, b in zip(source, offset, target))
                for source in body
                for target in threats
            )

        current = clearance((0, 0, 0))
        choices = [(clearance(d.offset), d) for d in turn.walkable_directions]
        if not choices:
            return None
        best, direction = max(choices, key=lambda choice: choice[0])
        if best > current:
            return Move(direction), "retreat"
        routes = []
        for distance, direction in choices:
            if distance < current:
                continue
            cell = turn.cell_at(direction.offset)
            if cell is None:
                continue
            for second in turn.walkable_from(cell):
                offset = tuple(a + b for a, b in zip(direction.offset, second.offset))
                after = clearance(offset)
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
        # Free both grips before preparing the bow in safety.
        has_arrows = any(item.identity == "item.weapon.arrow" for item in turn.inventory)
        wanted = "item.weapon.shortbow" if has_arrows else "item.weapon.mace"
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
            return weapon, "equip_bow" if has_arrows else "equip_melee"

        return None

    def search(self, turn: Turn, memory: FloorMap, returning: bool) -> tuple[Action, str] | None:
        ability = next((a for a in turn.abilities if a.identity == "rogue.search" and a.ready), None)
        if ability is None or turn.cell is None:
            return None
        sites = []
        for site, distance in memory.reachable(turn.cell.key):
            if self.searches[site.key] >= 3:
                continue
            walls = [c for c in memory.adjacent(site.key) if c.wall]
            untested = [c for c in walls if self.wall_searches[c.key] < 3]
            if not untested:
                continue
            near_exit = returning and any(
                n.stairs_up for wall in untested for n in memory.adjacent(wall.key)
            )
            # One search checks several boundaries. Favor corridor ends, especially near a lost exit.
            value = sum(3 - self.wall_searches[c.key] for c in untested) + 3 * (len(walls) >= 3)
            sites.append((near_exit, value - distance * 2, -distance, site.key))
        for _, _, _, key in sorted(sites, reverse=True):
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

    def choose(self, turn: Turn) -> tuple[Action | None, str]:
        if prompt := turn.identification:
            unknown = next((item for item in prompt.items if item.identity is None), None)
            return prompt.select(unknown), "identify"
        if turn.can_resume:
            return ActivityChoice.RESUME, "resume"
        if not turn.can_act or turn.cell is None:
            return None, "busy"

        if decision := self.combat(turn):
            return decision

        memory = self.maps[self.depth]
        if turn.hunger in {"hungry", "weak", "empty"}:
            for item in turn.inventory:
                if ItemUse.EAT in item.uses:
                    return UseItem(item, ItemUse.EAT), "eat"
            for ground in turn.ground_items:
                if ground.reachable and ground.item.category == "food":
                    return Pickup(ground.item), "take"
            food = {
                key for key, items in memory.items.items() if any(item.category == "food" for item in items)
            }
            if not self.trapped(turn) and (step := memory.toward(turn, food)):
                return step, "forage"

        returning = self.carrying_objective(turn)
        item_goals = set()
        carried: Counter[str | None] = Counter()
        for item in turn.inventory:
            carried[item.identity] += item.quantity
        for ground in sorted(turn.ground_items, key=lambda row: row.item.identity != self.objective):
            item = ground.item
            wanted = item.identity == self.objective or item.category == "food"
            if not returning:
                wanted |= item.identity == "item.weapon.dart" and carried[item.identity] < 12
                wanted |= item.identity == "item.weapon.spear" and not carried[item.identity]
            if wanted:
                if ground.reachable:
                    return Pickup(item), "take"
                if cell := turn.cell_at(ground.offset):
                    item_goals.add(cell.key)
        if not turn.hostiles and (decision := self.prepare(turn)):
            return decision
        if self.trapped(turn):
            return Wait(), "wait_for_restraint"
        if action := memory.toward(turn, item_goals):
            return action, "seek_item"

        if not turn.hostiles and (decision := self.provision(turn, memory, returning)):
            return decision

        direction = Direction.UP if returning else Direction.DOWN
        if returning or self.depth < self.retrieval_depth:
            stairs = {
                cell.key
                for cell in memory.cells.values()
                if (cell.stairs_up if returning else cell.stairs_down)
            }
            if turn.cell.key in stairs:
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
        if decision := self.search(turn, memory, returning):
            return decision
        if returning and any(cell.stairs_up for cell in memory.cells.values()):
            for item in turn.inventory:
                if item.identity == "item.scroll.teleportation" and ItemUse.READ in item.uses:
                    return UseItem(item, ItemUse.READ), "find_return_route"
        for item in turn.inventory:
            if item.identity is None and ItemUse.READ in item.uses:
                return UseItem(item, ItemUse.READ), "test_unknown_magic"
        return None, "no_disclosed_route"


def describe(action: Action) -> dict:
    """Record the decision with stable labels for cohort reports."""
    if isinstance(action, ActivityChoice):
        return {"type": action.value}
    if isinstance(action, Move):
        return {"type": "move", "direction": action.direction.value}
    if isinstance(action, Wait):
        return {"type": "wait"}
    if isinstance(action, UseItem):
        return {"type": action.use.value, "item": action.item.target}
    if isinstance(action, Pickup):
        return {"type": "take", "item": action.item.target}
    if isinstance(action, Equip):
        return {"type": "equip", "item": action.item.target, "slot": action.slot.index}
    if isinstance(action, Unequip):
        return {"type": "unequip", "slot": action.slot.index}
    if isinstance(action, Escape):
        return {"type": "escape", "hold": action.hold.target}
    if isinstance(action, OpenDoor):
        return {"type": "set_door", "door": action.door.target, "open": True}
    if isinstance(action, Identify):
        return {"type": "identify", "item": action.item.target if action.item else None}
    if isinstance(action, Shot):
        return {
            "type": "use_ability",
            "id": action.ability.identity,
            "target": action.target.target,
            "offset": action.offset,
            "projectile": action.projectile.target,
            "launcher": action.launcher.target if action.launcher else None,
        }
    if isinstance(action, UseAbility):
        return {
            "type": "use_ability",
            "id": action.ability.identity,
            "target": action.target.target if action.target else None,
        }
    raise TypeError(f"Cannot trace action {action!r}")


def play(game: Session, player: Adventurer, budget: int, emit: Callable[[dict], None]) -> dict:
    """Observe the outcome after the last budgeted submission, including victory."""
    count = 0
    deepest = 1
    acquired = False
    journal = CombatJournal()
    previous = None
    detail: dict[str, object] = {}
    while True:
        try:
            turn = game.wait_for_decision()
        except TimeoutError:
            return {
                "terminal": "readiness_timeout",
                "actions": count,
                "max_depth": deepest,
                "amulet_acquired": acquired,
            }
        combat = journal.read(turn)
        events = combat.records()
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
        acquired |= player.carrying_objective(turn)
        if count >= budget:
            reason = "action_budget"
            break
        action, reason = player.choose(turn)
        if action is None:
            if reason == "no_disclosed_route":
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
        emit(
            {
                "floor": player.depth,
                "tick": turn.tick,
                "reason": reason,
                "action": describe(action),
                "result": {"status": result.status.value, "receipt": {"intention": result.intention}},
                "previous_intention_id": previous.intention if previous else None,
                "previous_intention_phase": turn.outcome_of(previous) if previous else None,
                "health_recovery": asdict(turn.recovery) if turn.recovery else None,
                "nutrition_reserve": turn.food_reserve,
                "events_since_previous_action": events,
                "amulet_acquired": acquired,
            }
        )
        previous = result
        if result.status is Outcome.CONFIRMED:
            player.admitted(turn, action)
        elif result.status is not Outcome.STALE:
            reason = result.reason or result.status.value
            break
    detail.update(
        estimated_ascent_ticks_by_floor=player.ascent_ticks_by_floor,
        estimated_ascent_ticks=sum(player.ascent_ticks_by_floor.values()),
    )
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
