"""Rogue-O-Matic example. Run with the tor_client_bot extension installed."""

import argparse
from collections import Counter, deque
from dataclasses import dataclass, field
from functools import cache
from heapq import heappop, heappush
from itertools import count, product
import json
import os
from statistics import median_low
import time

from rogue_earned_route import CHARACTERS, ControllerError, first_purchase, starting_body_hd
from rogue_test_support import (
    DisclosedFloorGraph, disclosed_body_cells,
    disclosed_step_action, ground_sight_candidate, observe_walkthrough_boundaries,
)


DEFAULT_OBJECTIVE_IDENTITY = "item.amulet.amulet-of-yendor"
DEFAULT_RETRIEVAL_DEPTH = 26
ELEMENTAL_RAY_RANGE = 6
# The bundled weapons' base damage. Instance bonuses are assessed after pickup.
SWORD_BASE_DAMAGE = {"item.weapon.mace": 5, "item.weapon.long-sword": 7.5,
                     "item.weapon.two-handed-sword": 10}
DIAGONALS = ((1, -1, "north_east"), (1, 1, "south_east"),
             (-1, 1, "south_west"), (-1, -1, "north_west"))
MONSTERS_THAT_CANNOT_OPEN_DOORS = {"dragon", "griffin", "jabberwock"}
DANGEROUS_MONSTERS = MONSTERS_THAT_CANNOT_OPEN_DOORS | {"wraith", "vampire"}
FOES_WITH_CANCELLABLE_ATTACKS = {"dragon", "vampire", "wraith", "medusa", "ice monster", "rattlesnake", "aquator"}


def position(row):
    p = row["position"]
    return p["x"], p["y"], p["z"]


def melee_candidate(body, target, visible):
    for source in body:
        if source == target or any(abs(a - b) > 1 for a, b in zip(source, target)):
            continue
        # Diagonal contact needs every axis ordering to clear the disclosed terrain.
        corners = product(*({a, b} for a, b in zip(source, target)))
        if all((cell := visible.get(at)) and not cell["wall"]
               and (not cell.get("door") or cell["door"]["open"])
               for at in corners if at not in {source, target}):
            return True
    return False


def aim_key(view, origin, identity, target):
    occupied = tuple(sorted(position(actor) for actor in view["visible_actors"]
                            if actor["id"] == target))
    return origin, identity, target, occupied


def ability(view, identity):
    stats = view["combat"]["own_stats"]
    for offered in stats["abilities"]:
        if offered["id"] != identity or int(offered["cooldown_remaining"]):
            continue
        if cost := offered["cost"]:
            if not any(resource["resource"]["id"] == cost["resource"]["id"]
                       and resource["available"] >= cost["start"] + cost["resolution"]
                       for resource in stats["resources"]):
                continue
        return offered
    return None


def use(identity, target):
    return {"type": "use_ability", "id": identity,
            "invocation": {"aim": {"kind": "actor_target", "target": target},
                           "weapon": {"kind": "default"}},
            "supplies": {"kind": "carried"}}


IDENTIFY_SCROLLS = {
    "item.scroll.identify-weapon": {"weapon"},
    "item.scroll.identify-armor": {"armor"},
    "item.scroll.identify-potion": {"potion"},
    "item.scroll.identify-scroll": {"scroll"},
    "item.scroll.identify-wand": {"ring", "wand"},
}


def items_needing_identification(view):
    unknown_equipment = {offer["item"] for offer in view.get("interactions", {}).get("inventory", [])
                         if offer.get("slot") and offer.get("known_equipment") is None}
    return {item["id"] for item in view["inventory"]
            if not item.get("identified")
            or (item.get("class") in {"weapon", "armor"} and item["id"] in unknown_equipment)}


def identification_scroll(view):
    inventory = view["inventory"]
    candidates = items_needing_identification(view)
    unknown = {item["class"] for item in inventory if item["id"] in candidates and "class" in item}
    readable = {offer["item"] for offer in view.get("interactions", {}).get("inventory", [])
                if offer.get("readable")}
    for item in inventory:
        if (item.get("identified") and item["id"] in readable
                and unknown & IDENTIFY_SCROLLS.get(item.get("identity"), set())):
            return {"type": "read", "item": item["id"]}
    return None


def is_retrieval_item(item, identity):
    return item.get("identity") == identity


def known_items(view, affordance):
    allowed = {offer["item"] for offer in view.get("interactions", {}).get("inventory", [])
               if offer.get(affordance)}
    return {identity: item["id"] for item in view["inventory"]
            if item.get("identified") and item["id"] in allowed and (identity := item.get("identity"))}


def healing_potion(view):
    potions = known_items(view, "drinkable")
    return potions.get("item.potion.extra-healing") or potions.get("item.potion.healing")


def first_aid(view):
    if ability(view, "bind_wounds") and any(
            (item.get("known_supply") or {}).get("id") == "bandage" for item in view["inventory"]):
        return use("bind_wounds", view["self_target"]), "first_aid"
    return {"type": "wait"}, "recover"


def is_food(item):
    return item.get("class") == "food" or (item.get("known_supply") or {}).get("id") == "ration"


def food_reserve_ticks(nutrition):
    return nutrition["reserve"] * int(nutrition["unit_ticks"]) - int(nutrition["elapsed"])


def recovery_fits_food_budget(combat, has_food, return_ticks=0):
    stats = combat["own_stats"]
    recovery = stats.get("health_recovery")
    if not recovery:
        return True
    if recovery["stopped"]:
        return False
    nutrition = stats["nutrition"]
    if has_food or nutrition.get("kind") != "required":
        return True
    goal = (int(combat["max_hp"]) * 4 + 4) // 5
    missing = goal - int(combat["hp"])
    ticks_to_recover = missing * int(recovery["period_ticks"]) - int(recovery["elapsed_ticks"])
    return ticks_to_recover + return_ticks < food_reserve_ticks(nutrition)


def first_hostile_ray(view, visible, hostile, max_distance, *, min_distance=1, diagonal=False, bounce_range=0):
    actors = {position(actor): actor["id"] for actor in view["visible_actors"]
              if actor["id"] != view["self_target"]}

    def open_cell(at):
        cell = visible.get(at)
        return cell and not cell["wall"] and (not cell.get("door") or cell["door"]["open"])

    def open_step(dx, dy, distance):
        x, y = dx * distance, dy * distance
        return open_cell((x, y, 0)) and (
            not (dx and dy) or (open_cell((x - dx, y, 0)) and open_cell((x, y - dy, 0))))

    directions = tuple(DisclosedFloorGraph.directions)
    if diagonal:
        directions += DIAGONALS
    body = disclosed_body_cells(view) if bounce_range else ()
    for dx, dy, direction in directions:
        if bounce_range:
            # A reversal spends no visits. Reaching our furthest body cell on
            # the return would cost twice the outbound distance minus its offset.
            forward = max((x * dx + y * dy for x, y, z in body if z == 0
                           and x * dy == y * dx), default=0)
            forward //= dx * dx + dy * dy
            clear_steps = (bounce_range + forward) // 2 + 1
            if not all(open_step(dx, dy, step) for step in range(1, clear_steps + 1)):
                continue
        for distance in range(1, max_distance + 1):
            pos = (dx * distance, dy * distance, 0)
            if not open_step(dx, dy, distance):
                break
            if pos in actors:
                if actors[pos] in hostile and distance >= min_distance:
                    return direction, actors[pos]
                break
    return None


def directional_devices(view, effects, *, tried_unmeasured=None):
    charged = {offer["item"] for offer in view.get("interactions", {}).get("inventory", [])
               if offer.get("zappable") and (offer.get("charges")
                                             or (offer.get("charges") is None and tried_unmeasured is not None
                                                 and offer["item"] not in tried_unmeasured))
               and offer["device_activation"] in {"direction", "direction_or_self"}}
    yield from (item for item in view["inventory"] if item["id"] in charged
                and item.get("identified") and item["name"].partition(" of ")[2] in effects)


def offensive_device(view, visible, hostile, tried_unmeasured, *, min_distance=1):
    for item in directional_devices(view, {"magic missile", "fire", "cold", "lightning"},
                                    tried_unmeasured=tried_unmeasured):
        missile = item["name"].endswith(" of magic missile")
        ray = first_hostile_ray(view, visible, hostile, 8 if missile else ELEMENTAL_RAY_RANGE,
                                min_distance=min_distance, diagonal=True,
                                bounce_range=0 if missile else ELEMENTAL_RAY_RANGE)
        if ray:
            direction, _ = ray
            return {"type": "zap", "item": item["id"], "aim": {"kind": "direction", "direction": direction}}
    return None


def projectile_options(query, ability_id, phase):
    offset = 0
    while True:
        page = query(ability_id, phase, offset)
        if page is None:
            return
        yield from page["choices"]
        successor = page["next_offset"]
        if successor is None or successor <= offset:
            return
        offset = successor


def projectile_action(view, visible, hostile, query, *, min_distance=1):
    sources = {"kind": "sources"}
    for identity, range_limit in (("rogue.launch", 8), ("rogue.throw", 4)):
        if not ability(view, identity):
            continue
        ray = first_hostile_ray(view, visible, hostile, range_limit, diagonal=True,
                                min_distance=min_distance)
        if not ray:
            continue
        direction, _ = ray
        for choice in projectile_options(query, identity, sources):
            if choice["kind"] == "launcher":
                launcher = choice["launcher"]
                ammunition = next(projectile_options(
                    query, identity, {"kind": "ammunition", "launcher": launcher}), None)
                if ammunition is None:
                    continue
                delivery = {"kind": "launch", "launcher": launcher, "ammunition": ammunition["item"]}
            elif choice["kind"] == "throw" and choice["source"]["kind"] == "carried":
                delivery = {"kind": "throw", "source": choice["source"]}
            else:
                continue
            action = {"type": "use_ability", "id": identity,
                      "invocation": {"aim": {"kind": "directional_projectile",
                                             "direction": direction, "delivery": delivery},
                                     "weapon": {"kind": "default"}},
                      "supplies": {"kind": "carried"}}
            return action, delivery["kind"] + "_projectile"
    return None


def equipment_score(offer):
    known = offer.get("known_equipment")
    if known is None:
        return None
    if attack := known["attack"]:
        average = attack["damage_enchantment"]
        for component in attack["damage"]["components"]:
            amount = component["amount"]
            average += (amount["value"] if amount["type"] == "fixed"
                        else amount["count"] * (amount["sides"] + 1) / 2 + amount["bonus"])
        accuracy = max(0.1, 1 + attack["bonus"] / 20)
        return max(0, average) * accuracy / (attack["wind_up"] + attack["recovery"])
    return sum(int(field["value"]["value"])
               for modifier in known["modifiers"] if modifier["id"] == "reduction"
               for field in modifier["fields"] if field["id"] == "amount")


def equipment_upgrade(view, *, conserve_food=False):
    interactions = view.get("interactions", {})
    offers = interactions.get("inventory", [])
    worn = {offer["equipped_slot"]: offer for offer in offers
            if offer.get("equipped_slot") is not None}
    cursed = set(interactions.get("curse_blocked", []))
    recognized = {item["id"]: item.get("identity") for item in view["inventory"] if item.get("identified")}
    ring_scores = {
        "item.ring.ring-searching": 6, "item.ring.ring-slow-digestion": 5, "item.ring.ring-regeneration": 5,
        "item.ring.ring-protection": 4, "item.ring.ring-see-invisible": 4, "item.ring.ring-increase-damage": 3,
        "item.ring.ring-add-strength": 3, "item.ring.ring-maintain-armor": 2, "item.ring.ring-sustain-strength": 2,
        "item.ring.ring-dexterity": 2, "item.ring.ring-stealth": 2, "item.ring.ring-adornment": 0,
    }
    if conserve_food:
        ring_scores = {"item.ring.ring-slow-digestion": 1}
    worn_identities = {recognized.get(offer["item"]) for offer in worn.values()}
    for index, slot in enumerate(interactions.get("slots", [])):
        incumbent = worn.get(index)
        if incumbent and (incumbent.get("known_cursed") or incumbent["item"] in cursed):
            continue
        ring_slot = slot["id"] == "ring"
        if conserve_food and (incumbent or not ring_slot):
            continue
        threshold = 0
        if incumbent:
            threshold = (ring_scores.get(recognized.get(incumbent["item"])) if ring_slot
                         else equipment_score(incumbent))
        if threshold is None:
            continue
        upgrade = None
        for offer in offers:
            if (offer.get("equipped_slot") is not None or offer.get("known_cursed")
                    or (offer.get("slot") or {}).get("id") != slot["id"]):
                continue
            if ring_slot:
                identity = recognized.get(offer["item"])
                if identity in worn_identities:
                    continue
                score = ring_scores.get(identity)
            else:
                score = equipment_score(offer)
            if score is not None and score > threshold:
                upgrade, threshold = offer["item"], score
        if upgrade:
            if incumbent:
                return {"type": "unequip", "item": incumbent["item"]}
            return {"type": "equip", "item": upgrade, "slot": index}
    return None


def prepare_equipment(view):
    if upgrade := equipment_upgrade(view):
        return upgrade, "upgrade_equipment"
    offers = view.get("interactions", {}).get("inventory", [])
    worn_armor = (offer for offer in offers if offer.get("equipped_slot") is not None
                  and (offer.get("slot") or {}).get("id") == "body_armor")
    armor = min(worn_armor, key=lambda offer: offer["equipped_slot"], default=None)
    scrolls = known_items(view, "readable")
    if armor:
        if armor.get("known_protected") is False and (scroll := scrolls.get("item.scroll.protect-armor")):
            return {"type": "read", "item": scroll}, "protect_armor"
        if scroll := scrolls.get("item.scroll.enchant-armor"):
            return {"type": "read", "item": scroll}, "enchant_armor"
    if scroll := scrolls.get("item.scroll.enchant-weapon"):
        weapons = {item["id"] for item in view["inventory"] if item.get("class") == "weapon"}
        if any(offer["item"] in weapons and offer.get("equipped_slot") is not None
               and (offer.get("slot") or {}).get("id") == "weapon" for offer in offers):
            return {"type": "read", "item": scroll}, "enchant_weapon"
    return None


class MappingChart:
    """Match chart wall faces to current observations before using secret-door hints."""

    def __init__(self, chart):
        aligned = {row["component"] for row in chart["components"] if row["alignment"] == "aligned"}
        self.faces = {}
        self.secrets = {}
        for row in chart["rows"]:
            component = row["component"]
            if component not in aligned or row["position"] is None:
                continue
            at = tuple(map(int, row["position"]))
            fact = row["mark"]["fact"]
            if fact["kind"] == "wall_face" and fact["normal"][2] == 0:
                self.faces.setdefault(component, {}).setdefault(tuple(fact["normal"]), set()).add(at)
            elif fact["kind"] == "secret_door":
                self.secrets.setdefault(component, set()).add(at)

    def search_walls(self, visible):
        floor = {at for at, cell in visible.items() if at[2] == 0 and not cell["wall"]}
        faces = {}
        for at, cell in visible.items():
            x, y, z = at
            if z == 0 and cell["wall"]:
                for dx, dy, _ in DisclosedFloorGraph.directions:
                    if (x + dx, y + dy, z) in floor:
                        faces[at, (dx, dy, 0)] = cell["key"]
        if len(set(faces.values())) < 3:
            return set()

        def rotate(at, turns):
            x, y, z = at
            return ((x, y, z), (-y, x, z), (-x, -y, z), (y, -x, z))[turns]

        matches = []
        for turns in range(4):
            rotated = [(rotate(at, turns), rotate(normal, turns), key)
                       for (at, normal), key in faces.items()]
            for component, chart_faces in self.faces.items():
                anchor, normal, _ = min(rotated, key=lambda row: len(chart_faces.get(row[1], ())))
                for origin in chart_faces.get(normal, ()):
                    dx, dy, dz = (b - a for a, b in zip(anchor, origin))
                    if all((x + dx, y + dy, z + dz) in chart_faces.get(facing, ())
                           for (x, y, z), facing, _ in rotated):
                        if matches:
                            return set()
                        secrets = self.secrets.get(component, set())
                        matches.append({key for (x, y, z), _, key in rotated
                                        if (x + dx, y + dy, z + dz) in secrets})
        return matches[0] if matches else set()


@dataclass
class Floor:
    graph: DisclosedFloorGraph = field(default_factory=DisclosedFloorGraph)
    boundaries: dict[str, dict[str, str]] = field(default_factory=dict)
    search_attempts: Counter = field(default_factory=Counter)
    joining_wall_hints: dict[str, set[str]] = field(default_factory=dict)
    charted_entrances: set[str] = field(default_factory=set)
    mapping_checked: set[str] = field(default_factory=set)
    mapping_chart: MappingChart | None = None
    mapping_acquired: str | None = None
    mapping_read: bool = False
    tactile_attempts: Counter = field(default_factory=Counter)
    visited: set[str] = field(default_factory=set)
    rejected_edges: set[tuple[str, str]] = field(default_factory=set)
    movement_waited: set[str] = field(default_factory=set)
    rejected_aims: set[tuple] = field(default_factory=set)
    stairs_up: set[str] = field(default_factory=set)
    amulet_cells: set[str] = field(default_factory=set)
    food_cells: set[str] = field(default_factory=set)
    food_scout_end: int | None = None
    food_scout_servings: int = 0
    traps: set[str] = field(default_factory=set)
    recovery_threat_cells: set[str] = field(default_factory=set)
    recent_danger: dict[str, int] = field(default_factory=dict)
    retreat_doors: set[str] = field(default_factory=set)
    used_light_devices: set[str] = field(default_factory=set)
    avoidance_steps: Counter = field(default_factory=Counter)
    neighbors: dict[str, set[str]] = field(default_factory=dict)
    diagonal_routes: dict[str, tuple[tuple[str, str, tuple[str, str]], ...]] = field(default_factory=dict)

    def observe_neighbors(self, visible):
        for (x, y, z), cell in visible.items():
            if z == 0 and not cell["wall"]:
                directions = self.neighbors.setdefault(cell["key"], set())
                for dx, dy, _ in self.graph.directions:
                    wall = visible.get((x + dx, y + dy, z))
                    far_side = visible.get((x + 2 * dx, y + 2 * dy, z))
                    if wall and wall["wall"] and far_side and not far_side["wall"]:
                        self.joining_wall_hints.setdefault(wall["key"], set()).add(far_side["key"])
                directions.update(direction for dx, dy, direction in self.graph.directions
                                  if (x + dx, y + dy, z) in visible)

    def stair_component(self):
        """Cells connected to a remembered up stair by disclosed cardinal links."""
        seen = self.stairs_up & self.graph.edges.keys()
        queue = deque(seen)
        while queue:
            for neighbor in self.graph.edges[queue.popleft()].values():
                if neighbor not in seen:
                    seen.add(neighbor)
                    queue.append(neighbor)
        return seen

    def corridor_end(self, key):
        return (len(self.boundaries.get(key, {})) == len(self.graph.directions) - 1
                and len(self.graph.edges.get(key, {})) == 1)

    def frontier(self):
        return {key for key, directions in self.neighbors.items()
                if len(directions) < len(self.graph.directions) and key not in self.visited}

    def escape_continuation(self, start, other_exits, blocked):
        """Count safe steps away from the current cell and look for a loop."""
        seen = {start}
        queue = deque([(start, 1)])
        distance = 1
        reconnects = False
        while queue:
            here, distance = queue.popleft()
            reconnects |= here in other_exits
            if distance == 6:
                continue
            for direction, neighbor in self.graph.edges.get(here, {}).items():
                if neighbor in seen or neighbor in blocked or (here, direction) in self.rejected_edges:
                    continue
                seen.add(neighbor)
                queue.append((neighbor, distance + 1))
        return reconnects, distance

    def first_step(self, start, targets, blocked, hazard_cost=None, **options):
        route = self.find_route(start, targets, blocked, hazard_cost, **options)
        return route[0] if route is not None else None

    def find_route(self, start, targets, blocked, hazard_cost=None, *,
                   initial_diagonals=(), target_value=None, use_remembered_diagonals=False,
                   first_directions=None):
        """Return the first direction and move count of the lowest-cost route."""
        if not targets:
            return None
        hazard_cost = hazard_cost or {}
        # Prefer more useful destinations at the same route cost.
        target_value = target_value or {}
        order = count()
        costs = {start: 0}
        frontier = [(0, 0, next(order), start, None, 0)]
        while frontier:
            total, _, _, here, first_step, moves = heappop(frontier)
            if total != costs[here]:
                continue
            if here in targets:
                return first_step, moves
            neighbors = self.graph.edges.get(here, {})
            steps = [(direction, neighbors.get(direction), 10) for _, _, direction in self.graph.directions]
            if here == start:
                steps.extend((direction, neighbor, 14) for direction, neighbor in initial_diagonals)
            elif use_remembered_diagonals:
                steps.extend((direction, neighbor, 14)
                             for direction, neighbor, sides in self.diagonal_routes.get(here, ())
                             if not blocked.intersection(sides))
            for direction, neighbor, duration in steps:
                if here == start and first_directions is not None and direction not in first_directions:
                    continue
                if neighbor is None or neighbor in blocked or (here, direction) in self.rejected_edges:
                    continue
                cost = total + duration + (80 if neighbor in self.traps else 0) + 10 * hazard_cost.get(neighbor, 0)
                if neighbor in costs and cost >= costs[neighbor]:
                    continue
                costs[neighbor] = cost
                heappush(frontier, (cost, -target_value.get(neighbor, 0), next(order),
                                    neighbor, first_step or direction, moves + 1))
        return None


class Policy:
    """Choose from the player's current view and previously observed floor links."""

    def __init__(self, *, retrieval_depth=DEFAULT_RETRIEVAL_DEPTH,
                 objective_identity=DEFAULT_OBJECTIVE_IDENTITY):
        self.retrieval_depth = retrieval_depth
        self.objective_identity = objective_identity
        self.floor = 1
        self.floors: dict[int, Floor] = {}
        self.pending_stair = None
        self.return_travel_ticks = {}
        self.descent_travel_ticks = {}
        self.floor_entered_tick = None
        self.food_value = {}
        self.pending_meal = None
        self.recovering = False
        self.tested_magic = set()
        self.emergency_magic_used = False
        self.tried_unmeasured_offensive_devices = set()
        self.life_drainers = set()
        self.incoming_attackers = set()
        self.unseen_danger = {}
        self.unseen_pursuit_until = 0
        self.last_unseen_escape = None
        self.hold_targets = set()
        self.tried_hold_targets = set()
        self.largest_hits = {}
        self.multi_strike_foes = set()
        self.foe_names = {}
        self.previous_hp = None
        self.last_combat_sequence = None
        self.previous_losses = None
        self.lasting_injury_foes = set()
        self.stair_failures = Counter()
        self.stair_backoff = {}
        self.pursuit_steps = Counter()
        self.pursuit_target = None
        self.controlled_foes = set()
        self.control_target = None
        self.mapping_floors = {}

    def accepted(self, action, view, *, reason=None):
        if action["type"] == "eat":
            item = next((item for item in view["inventory"] if item["id"] == action["item"]), None)
            nutrition = view["combat"]["own_stats"]["nutrition"]
            if item and item.get("identity") and nutrition.get("kind") == "required":
                self.pending_meal = (item["identity"], item["id"], int(item.get("quantity", 1)),
                                     nutrition["reserve"])
        if reason == "retreat_upstairs":
            self.recovering = True
        if reason == "close_retreat_door":
            self.floors[self.floor].retreat_doors.add(action["door"])
        if reason == "map_floor":
            self.floors[self.floor].mapping_read = True
        if reason == "light_room":
            self.floors[self.floor].used_light_devices.add(action["item"])
        if reason == "ranged_device" and any(
                offer["item"] == action["item"] and offer.get("charges") is None
                for offer in view.get("interactions", {}).get("inventory", [])):
            self.tried_unmeasured_offensive_devices.add(action["item"])
        if reason == "approach_foe":
            self.pursuit_steps[self.pursuit_target] += 1
        if reason == "hold_threat":
            self.tried_hold_targets.update(self.hold_targets)
        if reason == "control_threat":
            self.controlled_foes.add(self.control_target)
        if reason == "evade_unseen_attack":
            self.last_unseen_escape = next(cell["key"] for cell in view["visible_cells"]
                                           if position(cell) == (0, 0, 0))
        if reason in {"avoid_foe", "break_contact"}:
            here = next(cell["key"] for cell in view["visible_cells"] if position(cell) == (0, 0, 0))
            self.floors[self.floor].avoidance_steps[here] += 1
        if reason == "settle_movement":
            here = next(cell["key"] for cell in view["visible_cells"] if position(cell) == (0, 0, 0))
            memory = self.floors[self.floor]
            memory.movement_waited.add(here)
            memory.rejected_edges.difference_update((here, direction) for direction in memory.graph.edges[here])
        if action["type"] == "wait" and self.stair_backoff:
            here = next(cell["key"] for cell in view["visible_cells"] if position(cell) == (0, 0, 0))
            anchor = self.floor, here
            if self.stair_backoff.get(anchor, 0) > 0:
                self.stair_backoff[anchor] -= 1
        if reason == "test_unknown_magic" and any(actor["hostile"] for actor in view["combat"]["actors"]):
            self.emergency_magic_used = True
        if action["type"] in {"read", "drink", "zap"}:
            item = next((item for item in view["inventory"] if item["id"] == action["item"]), None)
            if item and not item["identified"]:
                self.tested_magic.add((item["class"], item["appearance"]))
        if action["type"] == "move" and action["direction"] in {"up", "down"}:
            here = next(cell for cell in view["visible_cells"] if position(cell) == (0, 0, 0))
            direction = action["direction"]
            sign = -1 if direction == "down" else 1
            opposite = "stairs_up" if direction == "down" else "stairs_down"
            landing = max((cell for cell in view["visible_cells"] if cell[opposite]
                           and (offset := position(cell))[:2] == (0, 0) and offset[2] * sign > 0),
                          key=lambda cell: abs(position(cell)[2]), default=None)
            self.pending_stair = (direction, here["key"], landing["key"] if landing else None,
                                  int(view["tick"]) if "tick" in view else None)
        if action["type"] == "use_ability" and action["id"] == "rogue.search":
            here = next(cell for cell in view["visible_cells"] if position(cell) == (0, 0, 0))
            memory = self.floors[self.floor]
            memory.search_attempts.update(set(memory.boundaries.get(here["key"], {}).values()))
            memory.tactile_attempts[here["key"]] += 1

    def rejected(self, action, view, outcome):
        if outcome.get("code") != "invalid_action":
            return False
        cell = next(cell for cell in view["visible_cells"] if position(cell) == (0, 0, 0))
        here = cell["key"]
        memory = self.floors[self.floor]
        if action["type"] == "use_ability":
            aim = action["invocation"]["aim"]
            if aim["kind"] == "actor_target" and aim["target"] != view["self_target"]:
                key = aim_key(view, here, action["id"], aim["target"])
                if key not in memory.rejected_aims:
                    memory.rejected_aims.add(key)
                    return True
            return False
        if action["type"] == "set_door" and not action["open"]:
            if action["door"] not in memory.retreat_doors:
                memory.retreat_doors.add(action["door"])
                return True
        if action["type"] != "move":
            return False
        direction = action["direction"]
        if direction in {"up", "down"} and cell["stairs_" + direction]:
            stair = self.floor, here, direction
            failures = self.stair_failures[stair]
            if failures >= 3:
                return False
            self.stair_failures[stair] += 1
            self.stair_backoff[self.floor, here] = 1 << failures
            return True
        edge = here, direction
        known_direction = (direction in memory.graph.edges.get(here, {})
                           or any(direction == name for _, _, name in DIAGONALS))
        if not known_direction or edge in memory.rejected_edges:
            return False
        memory.rejected_edges.add(edge)
        return True

    def untried_consumable(self, view, category, affordance):
        usable = {offer["item"] for offer in view.get("interactions", {}).get("inventory", [])
                  if offer.get(affordance)}
        return next((item["id"] for item in view["inventory"]
                     if item["id"] in usable and not item["identified"] and item["class"] == category
                     and (category, item["appearance"]) not in self.tested_magic), None)

    def control_threat(self, view, visible, targets, *, min_distance=1):
        targets = targets - self.controlled_foes
        special_targets = {actor["id"] for actor in view["visible_actors"]
                           if actor.get("name", "").casefold() in FOES_WITH_CANCELLABLE_ATTACKS}
        special_targets.update(self.life_drainers)
        for effect, eligible in (("teleport away", targets), ("slow monster", targets),
                                 ("cancellation", targets & special_targets)):
            if not eligible:
                continue
            device = next(directional_devices(view, {effect}, tried_unmeasured=frozenset()), None)
            if device and (ray := first_hostile_ray(view, visible, eligible, 6,
                                                    min_distance=min_distance, diagonal=True)):
                direction, self.control_target = ray
                return {"type": "zap", "item": device["id"],
                        "aim": {"kind": "direction", "direction": direction}}, "control_threat"
        return None

    def try_unknown_device(self, view, visible, hostile):
        combat = view["combat"]
        if int(combat["hp"]) * 5 < int(combat["max_hp"]) * 4:
            return None
        inventory = view["inventory"]
        if any(is_retrieval_item(item, self.objective_identity) for item in inventory):
            return None
        if any(actor["id"] in hostile and sum(map(abs, position(actor))) < 3
               for actor in view["visible_actors"]):
            return None
        offers = {offer["item"] for offer in view.get("interactions", {}).get("inventory", [])
                  if offer.get("zappable") and offer.get("charges") != 0
                  and offer["device_activation"] == "direction"}
        device = next((item for item in inventory if item["id"] in offers
                       and not item["identified"] and item["class"] == "wand"
                       and ("wand", item["appearance"]) not in self.tested_magic), None)
        if device and (ray := first_hostile_ray(view, visible, hostile, 4, min_distance=3,
                                                bounce_range=ELEMENTAL_RAY_RANGE)):
            return {"type": "zap", "item": device["id"], "aim": {
                "kind": "direction", "direction": ray[0]}}
        return None

    def observe_combat(self, view, combat_updates=None):
        combat = view["combat"]
        events = combat.get("events", [])
        if combat_updates is not None:
            events = []
            for update in combat_updates:
                sequence = int(update["cursor"]["sequence"])
                if self.last_combat_sequence is not None and sequence <= self.last_combat_sequence:
                    continue
                events.extend(update["events"])
                self.last_combat_sequence = sequence
        hp = int(combat["hp"])
        self.foe_names.update((actor["id"], name) for actor in view["visible_actors"]
                              if (name := actor.get("name")))
        player = view["self_target"]
        incoming = [event for event in events
                    if event["type"] in {"ability", "unseen_ability"}
                    and event["target"] == player and event.get("caster") != player]
        attackers_in_view = {row["actor"] for row in combat["actors"] if row["hostile"]}
        if any(event["type"] == "unseen_ability"
               and (event["outcome"] == "miss" or event.get("caster") in attackers_in_view
                    or (event["outcome"] == "applied" and self.previous_hp is not None
                        and hp < self.previous_hp))
               for event in incoming):
            here = next((cell for cell in view["visible_cells"] if position(cell) == (0, 0, 0)), None)
            if here:
                tick = int(view.get("tick", 0))
                self.unseen_danger[here["key"]] = tick + 1200
                self.unseen_pursuit_until = tick + 400
        attacks = [event for event in incoming if event.get("caster")]
        self.incoming_attackers.update(event["caster"] for event in attacks)
        self.multi_strike_foes.update(key for event in attacks if (event.get("strike") or 0) > 0
                                     for key in self.threat_keys(event["caster"]))
        previous = self.previous_hp
        self.previous_hp = hp
        disclosed = combat["own_stats"].get("restorable_losses", {})
        losses = {**disclosed.get("attributes", {}),
                  "maximum_health": disclosed.get("maximum_health", 0),
                  "hit_dice": int(disclosed.get("hit_dice", 0))}
        previous_losses = self.previous_losses
        self.previous_losses = losses

        attackers = {event["caster"] for event in attacks if event["outcome"] == "applied"}
        if len(attackers) != 1:
            return events
        attacker, = attackers
        keys = tuple(self.threat_keys(attacker))
        if previous is not None and hp < previous:
            for key in keys:
                self.largest_hits[key] = max(previous - hp, self.largest_hits.get(key, 0))
        if (previous_losses is not None
                and any(amount > previous_losses.get(stat, 0) for stat, amount in losses.items())
                and any(row["actor"] == attacker and row["hostile"] for row in combat["actors"])):
            self.lasting_injury_foes.update(keys)
        return events

    def threat_keys(self, actor, name=None):
        yield ("actor", actor)
        if name := name or self.foe_names.get(actor):
            yield ("name", name)

    def damage_margin(self, actor, name=None):
        # Allow for another attack before recovery or retreat completes.
        return max((3 if key in self.multi_strike_foes else 2) * self.largest_hits.get(key, 0)
                   for key in self.threat_keys(actor, name))

    def unsafe_health_against(self, hp, actor, name=None):
        return hp <= self.damage_margin(actor, name)

    def observe_meal(self, carried_food, nutrition):
        if self.pending_meal is None:
            return
        identity, item_id, quantity, reserve_before = self.pending_meal
        self.pending_meal = None
        remaining = int(carried_food.get(item_id, {}).get("quantity", 0))
        if remaining < quantity and nutrition.get("kind") == "required":
            gain = nutrition["reserve"] - max(0, reserve_before)
            if gain > 0:
                self.food_value[identity] = max(gain, self.food_value.get(identity, 0))

    def estimate_trip_ticks(self, return_ticks):
        """Include the mapped ascent and estimate unseen legs from recent floors."""
        if not self.descent_travel_ticks:
            return return_ticks
        recent = sorted(self.descent_travel_ticks)[-5:]
        typical_floor = median_low([
            self.descent_travel_ticks[floor] + self.return_travel_ticks.get(floor, 0)
            for floor in recent
        ])
        return return_ticks + (self.retrieval_depth - self.floor) * typical_floor

    def may_need_escape(self, view):
        combat = view["combat"]
        hp = int(combat["hp"])
        hostile = {actor["actor"] for actor in combat["actors"] if actor["hostile"]}
        if not hostile:
            return False
        names = {actor["id"]: actor.get("name") for actor in view["visible_actors"]}
        margins = {actor: self.damage_margin(actor, names.get(actor)) for actor in hostile}
        if hp * 4 <= int(combat["max_hp"]) or max(margins.values()) >= hp:
            return True
        visible = {position(cell): cell for cell in view["visible_cells"]}
        body = disclosed_body_cells(view)
        contacts = {actor["id"] for actor in view["visible_actors"] if actor["id"] in hostile
                    and melee_candidate(body, position(actor), visible)}
        return sum(margins[actor] for actor in contacts) >= hp

    def observe_stair(self, here, visible_cells=(), tick=None):
        if self.pending_stair is not None:
            direction, origin, landing, admitted_tick = self.pending_stair
            self.pending_stair = None
            opposite = "stairs_up" if direction == "down" else "stairs_down"
            beside_landing = any(cell["key"] == landing and position(cell)[2] == 0
                                 for cell in visible_cells)
            if here["key"] != origin and (here[opposite] or beside_landing):
                self.stair_failures.pop((self.floor, origin, direction), None)
                self.stair_backoff.pop((self.floor, origin), None)
                if direction == "down" and admitted_tick is not None and tick is not None and int(tick) > admitted_tick:
                    if self.floor_entered_tick is not None and int(tick) > self.floor_entered_tick:
                        self.descent_travel_ticks.setdefault(self.floor, int(tick) - self.floor_entered_tick)
                    memory = self.floors.get(self.floor)
                    if memory is not None:
                        diagonals = ((name, target) for name, target, _ in memory.diagonal_routes.get(origin, ()))
                        route = memory.find_route(origin, memory.stairs_up, set(), initial_diagonals=diagonals,
                                                  use_remembered_diagonals=True)
                        if route is not None:
                            _, moves = route
                            self.return_travel_ticks[self.floor] = (moves + 1) * (int(tick) - admitted_tick)
                self.floor += 1 if direction == "down" else -1
                self.floor_entered_tick = int(tick) if tick is not None else None
                self.unseen_pursuit_until = 0
                self.last_unseen_escape = None

    def choose(self, state, *, projectile_choices=None, chart=None):
        view = state["observation"]
        combat = view["combat"]
        if combat["terminal"]:
            if combat["victory"]:
                return None, "victory"
            return None, "death" if combat["dead"] else "terminal"
        if prompt := view.get("interactions", {}).get("identification"):
            offered = set(prompt["items"])
            candidates = items_needing_identification(view)
            target = next((item["id"] for item in view["inventory"]
                           if item["id"] in offered and item["id"] in candidates), None)
            return {"type": "identify", "choice": prompt["choice"], "item": target}, "identify"
        if not view["ready"] or combat["preparation_remaining"] is not None:
            return None, "busy"

        self.observe_combat(view)
        visible = {position(cell): cell for cell in view["visible_cells"]}
        here = visible[(0, 0, 0)]
        key = here["key"]
        self.observe_stair(here, view["visible_cells"], view.get("tick"))
        if self.floor_entered_tick is None and "tick" in view:
            self.floor_entered_tick = int(view["tick"])
        mapping = next((summary for summary in view.get("charts", []) if summary["kind"] == "mapping"), {})
        acquired = mapping.get("acquired_at")
        if acquired is not None:
            self.mapping_floors.setdefault(acquired, self.floor)
        motion = view.get("motion") or {}
        if int(motion.get("velocity", (0, 0, 0))[2]) < 0:
            return {"type": "wait"}, "settle_fall"

        memory = self.floors.setdefault(self.floor, Floor())
        memory.graph.observe(state)
        memory.observe_neighbors(visible)
        observe_walkthrough_boundaries(view, memory.boundaries)
        if key not in memory.visited:
            memory.avoidance_steps.clear()
        memory.visited.add(key)
        memory.amulet_cells.difference_update(cell["key"] for cell in visible.values())
        memory.amulet_cells.update(visible[pos]["key"] for item in view["ground_items"]
                                  if is_retrieval_item(item["item"], self.objective_identity)
                                  and (pos := position(item)) in visible)
        ground_food = [row for row in view["ground_items"] if is_food(row["item"])]
        memory.food_cells.difference_update(cell["key"] for cell in visible.values())
        memory.food_cells.update(visible[pos]["key"] for row in ground_food if (pos := position(row)) in visible)
        memory.traps.difference_update(cell["key"] for cell in visible.values())
        memory.traps.update(cell["key"] for cell in visible.values() if cell.get("trap"))
        memory.stairs_up.update(cell["key"] for cell in visible.values() if cell["stairs_up"])

        body = disclosed_body_cells(view)
        occupied = {position(actor) for actor in view["visible_actors"]
                    if actor["id"] != view["self_target"]}
        obstacles = occupied | {pos for pos, cell in visible.items() if cell["wall"]}
        blocked = {cell["key"] for pos, cell in visible.items()
                   if pos[2] == 0 and any(
                       tuple(a + b for a, b in zip(pos, offset)) in obstacles for offset in body)}

        holds = view.get("linked_holds", [])
        movement_restricted = bool(holds) or any(
            row["condition"]["id"] == "rogue.bear_trap" and row["phase"] == "active"
            and int(row.get("remaining_ticks") or 0) > 0
            for row in combat["own_stats"].get("conditions", []))
        stair_hazards = {}

        @cache
        def diagonal_steps():
            steps = []
            for dx, dy, direction in DIAGONALS:
                corner = [visible.get(at) for at in ((dx, 0, 0), (0, dy, 0), (dx, dy, 0))]
                if ((key, direction) not in memory.rejected_edges
                        and all(tile and not tile["wall"] and tile["key"] not in blocked
                                and (not tile.get("door") or tile["door"]["open"]) for tile in corner)):
                    steps.append((dx, dy, direction, corner[-1]["key"]))
            return tuple(steps)

        memory.diagonal_routes[key] = tuple(
            (direction, target, (visible[(dx, 0, 0)]["key"], visible[(0, dy, 0)]["key"]))
            for dx, dy, direction, target in diagonal_steps()
        )

        def move_toward(targets, reason, hazard_cost=None, *, allow_diagonal=False, target_value=None,
                        use_remembered_diagonals=False):
            if movement_restricted:
                return None
            diagonals = {direction: target for _, _, direction, target in diagonal_steps()} if allow_diagonal else {}
            direction = memory.first_step(
                key, targets, blocked, hazard_cost, initial_diagonals=diagonals.items(),
                target_value=target_value, use_remembered_diagonals=use_remembered_diagonals,
            )
            if direction in diagonals:
                return {"type": "move", "direction": direction}, reason
            if direction:
                return disclosed_step_action(view, direction, wait_for_restraint=True), reason
            return None

        def stair_step(ascending):
            if movement_restricted or (protect_recovery and not ascending):
                return None
            direction, reason = ("up", "ascend") if ascending else ("down", "descend")
            if here["stairs_" + direction]:
                return {"type": "move", "direction": direction}, reason
            if ascending:
                return move_toward(memory.stairs_up, "return_to_stairs", stair_hazards,
                                   allow_diagonal=not others, use_remembered_diagonals=not others)
            return move_toward(memory.graph.stairs_down, "find_descent", stair_hazards, allow_diagonal=not others)

        inventory = view["inventory"]
        carrying = any(is_retrieval_item(item, self.objective_identity) for item in inventory)
        if int(combat["hp"]) * 2 < int(combat["max_hp"]) and (potion := healing_potion(view)):
            return {"type": "drink", "item": potion}, "heal"

        tick = int(view.get("tick", 0))
        hostile = {actor["actor"] for actor in combat["actors"] if actor["hostile"]}
        if not hostile:
            self.emergency_magic_used = False
        below_landing = 0
        if self.recovering and here["stairs_down"]:
            # The stair view can keep showing the pursuer on the other floor.
            below_landing = max((z for (x, y, z), cell in visible.items()
                                 if (x, y) == (0, 0) and z < 0 and cell["stairs_up"]), default=0)
        others = sorted((actor for actor in view["visible_actors"]
                         if actor["id"] in hostile
                         and (not below_landing or 2 * position(actor)[2] > below_landing)),
                        key=lambda actor: (sum(map(abs, position(actor))), position(actor)))
        protect_recovery = (others and self.recovering
                            and int(combat["hp"]) * 5 < int(combat["max_hp"]) * 4)
        for event in combat.get("events", []):
            if (event["type"] == "ability" and event.get("caster") in hostile
                    and event["target"] == view["self_target"]
                    and event["ability"]["id"] in {"rogue.energy_drain", "rogue.drain_vitality"}):
                self.life_drainers.add(event["caster"])
        recognized_threats = {actor["id"] for actor in others
                              if actor.get("name", "").casefold() in DANGEROUS_MONSTERS}
        unsafe_foes = {actor["id"] for actor in others
                       if self.unsafe_health_against(int(combat["hp"]), actor["id"])}
        lasting_threats = {actor["id"] for actor in others
                           if any(key in self.lasting_injury_foes for key in self.threat_keys(actor["id"]))}
        dangerous = recognized_threats | self.life_drainers | unsafe_foes | lasting_threats
        dangerous_foe = any(actor["id"] in dangerous and sum(map(abs, position(actor))) <= 4
                           for actor in others)

        if holds:
            injured = int(combat["hp"]) * 2 < int(combat["max_hp"])
            if injured or len(holds) > 1:
                if scroll := known_items(view, "readable").get("item.scroll.teleportation"):
                    return {"type": "read", "item": scroll}, "emergency_teleport"
            if not others or injured:
                return {"type": "escape", "hold": holds[0]["target"]}, "escape_hold"
        stair_delayed = bool(self.stair_backoff.get((self.floor, key), 0))
        self.unseen_danger = {cell: until for cell, until in self.unseen_danger.items() if until > tick}
        unseen_pursuit = tick < self.unseen_pursuit_until
        if not others and (unseen_pursuit or key in self.unseen_danger):
            if key in self.unseen_danger and here.get("lit") and int(combat["hp"]) * 2 >= int(combat["max_hp"]):
                sight_active = any(row["condition"]["id"] == "rogue.see_invisible" and row["phase"] == "active"
                                   for row in combat["own_stats"].get("conditions", []))
                if not sight_active:
                    sight = known_items(view, "drinkable").get("item.potion.see-invisible")
                    if sight:
                        return {"type": "drink", "item": sight}, "reveal_attacker"
            danger = {cell for cell, until in memory.recent_danger.items() if until > tick}
            danger.update(self.unseen_danger)
            if unseen_pursuit and self.last_unseen_escape is not None:
                danger.add(self.last_unseen_escape)
            safe = memory.graph.edges.keys() - danger - memory.traps - {key}
            costs = dict.fromkeys(danger, 8)
            if step := move_toward(safe & memory.visited, "evade_unseen_attack", costs, allow_diagonal=True):
                return step
            if step := move_toward(safe, "evade_unseen_attack", costs, allow_diagonal=True):
                return step
        if others:
            if not carrying:
                for row in view["ground_items"]:
                    item = row["item"]
                    if row["reachable"] and is_retrieval_item(item, self.objective_identity):
                        return {"type": "take", "item": item["id"], "quantity": item["quantity"]}, "loot"
            if not stair_delayed and (carrying or 12 <= self.floor < self.retrieval_depth):
                direction = "up" if carrying else "down"
                if here["stairs_" + direction]:
                    if step := stair_step(carrying):
                        return step

            critical_health = int(combat["hp"]) * 4 <= int(combat["max_hp"])
            foe_bodies = {}
            for foe in others:
                foe_bodies.setdefault(foe["id"], []).append(position(foe))
            foe_cells = [target for cells in foe_bodies.values() for target in cells]

            @cache
            def contacts(x, y):
                displaced_body = [(x + dx, y + dy, z) for dx, dy, z in body]
                return {actor for actor, cells in foe_bodies.items()
                        if any(melee_candidate(displaced_body, target, visible) for target in cells)}

            def contact_count(x, y):
                return len(contacts(x, y))

            @cache
            def threat_distance(x, y):
                distance = min(abs(x - tx) + abs(y - ty) for tx, ty, _ in foe_cells)
                return 1 if distance > 1 and contact_count(x, y) else distance

            if carrying or 12 <= self.floor < self.retrieval_depth:
                stair_hazards.update((cell["key"], 8) for (x, y, z), cell in visible.items()
                                     if z == 0 and threat_distance(x, y) <= 1)

            if dangerous_foe and "tick" in view:
                hostile_positions = {pos[:2] for actor in dangerous for pos in foe_bodies.get(actor, ())}
                memory.recent_danger.update((cell["key"], tick + 1200) for (x, y, z), cell in visible.items()
                                            if z == 0 and ((x, y) in hostile_positions or contacts(x, y) & dangerous))

            current_contacts = contact_count(0, 0)
            if (not stair_delayed and (carrying or 12 <= self.floor < self.retrieval_depth)
                    and int(combat["hp"]) * 2 >= int(combat["max_hp"])):
                if onward := stair_step(carrying):
                    direction = onward[0].get("direction")
                    offset = next(((dx, dy) for dx, dy, name in memory.graph.directions
                                   if name == direction), None)
                    if offset is not None and threat_distance(*offset) >= max(2, threat_distance(0, 0)):
                        return onward
            if current_contacts > 1:
                hp, maximum = int(combat["hp"]), int(combat["max_hp"])
                margin = sum(self.damage_margin(actor) for actor in contacts(0, 0))
                if hp < maximum and (hp * 4 <= maximum * 3 or margin >= hp):
                    if potion := healing_potion(view):
                        return {"type": "drink", "item": potion}, "heal"
            if dangerous_foe or critical_health:
                memory.recovery_threat_cells.update(visible[pos]["key"] for pos in foe_cells if pos in visible)
            if stair_delayed or dangerous_foe or critical_health or protect_recovery or current_contacts > 1:
                if not movement_restricted and self.floor > 1 and here["stairs_up"] and not stair_delayed:
                    return {"type": "move", "direction": "up"}, "retreat_upstairs"
                if (dangerous_foe and not current_contacts and not movement_restricted
                        and all(foe.get("name", "").casefold() in MONSTERS_THAT_CANNOT_OPEN_DOORS for foe in others)):
                    for dx, dy, _ in memory.graph.directions:
                        door = visible.get((dx, dy, 0), {}).get("door")
                        if (door and door["open"] and door["reachable"] and door["id"] not in memory.retreat_doors
                                and not any((x, y) == (dx, dy) for x, y, _ in occupied)
                                and all(x * dx + y * dy > 1 for x, y, _ in foe_cells)
                                and all(visible.get((dx + side * dy, dy - side * dx, 0), {}).get("wall")
                                        for side in (-1, 1))):
                            return {"type": "set_door", "door": door["id"], "open": False}, "close_retreat_door"
                current_distance = threat_distance(0, 0)
                exits = [(x, y, direction) for x, y, direction in memory.graph.directions
                         if not movement_restricted and (cell := visible.get((x, y, 0))) and not cell["wall"]
                         and cell["key"] not in blocked and (key, direction) not in memory.rejected_edges]
                retreat = [(x, y, direction) for x, y, direction in exits
                           if threat_distance(x, y) >= max(2, current_distance)]
                escape_steps = retreat + [
                    (x, y, direction) for x, y, direction, _ in diagonal_steps()
                    if not movement_restricted and threat_distance(x, y) >= max(2, current_distance)
                ]
                contact_cells = {cell["key"] for (x, y, z), cell in visible.items()
                                 if z == 0 and threat_distance(x, y) <= 1}
                unsafe_cells = blocked | contact_cells
                if escape_steps and self.floor > 1 and not stair_delayed:
                    best_gap = max(threat_distance(x, y) for x, y, _ in escape_steps) if current_contacts else 0
                    first_directions = {direction for x, y, direction in escape_steps
                                        if threat_distance(x, y) >= best_gap
                                        and (not (x and y) or threat_distance(x, y) > current_distance)}
                    diagonals = {direction: target for _, _, direction, target in diagonal_steps()
                                 if direction in first_directions}
                    if toward_stairs := memory.first_step(key, memory.stairs_up, unsafe_cells,
                                                           initial_diagonals=diagonals.items(),
                                                           first_directions=first_directions):
                        if toward_stairs in diagonals:
                            return {"type": "move", "direction": toward_stairs}, "retreat_to_stairs"
                        return disclosed_step_action(view, toward_stairs), "retreat_to_stairs"
                haste = None
                if dangerous_foe and not critical_health:
                    conditions = {row["condition"]["id"] for row in combat["own_stats"].get("conditions", [])}
                    if not conditions & {"rogue.hasted", "rogue.haste_exhaustion"}:
                        haste = known_items(view, "drinkable").get("item.potion.haste")
                if dangerous_foe and not current_contacts:
                    if control := self.control_threat(view, visible, dangerous, min_distance=3):
                        return control
                    if not critical_health and current_distance >= 3:
                        zap = offensive_device(view, visible, dangerous, self.tried_unmeasured_offensive_devices,
                                               min_distance=3)
                        if zap:
                            return zap, "ranged_device"
                        if projectile_choices:
                            if shot := projectile_action(view, visible, dangerous - unsafe_foes,
                                                         projectile_choices, min_distance=3):
                                return shot
                    if haste and current_distance >= 3 and retreat:
                        return {"type": "drink", "item": haste}, "combat_haste"
                self.hold_targets = {foe["id"] for foe in others
                                     if sum(map(abs, position(foe))) <= 2} - self.tried_hold_targets
                scrolls = known_items(view, "readable")
                hold_scroll = scrolls.get("item.scroll.hold-monster") if self.hold_targets else None
                if (hold_scroll and not current_contacts
                        and self.hold_targets & dangerous & self.incoming_attackers):
                    return {"type": "read", "item": hold_scroll}, "hold_threat"
                improving = [(x, y, direction) for x, y, direction in escape_steps
                             if threat_distance(x, y) > current_distance]
                if improving:
                    safe_exits = {visible[(x, y, 0)]["key"] for x, y, _ in escape_steps}
                    no_continuation = unsafe_cells | memory.traps | {key}
                    no_continuation.update(cell["key"] for cell in visible.values()
                                           if (door := cell.get("door")) and not door["open"])

                    def retreat_rank(exit_cell):
                        x, y, direction = exit_cell
                        destination = visible[(x, y, 0)]["key"]
                        continuation = memory.escape_continuation(
                            destination, safe_exits - {destination}, no_continuation)
                        return threat_distance(x, y), destination not in memory.traps, continuation, direction

                    _, _, direction = max(improving, key=retreat_rank)
                    if any(direction == diagonal for _, _, diagonal, _ in diagonal_steps()):
                        return {"type": "move", "direction": direction}, "retreat"
                    return disclosed_step_action(view, direction), "retreat"
                if current_contacts > 1:
                    if not movement_restricted:
                        for dx, dy, direction, _ in diagonal_steps():
                            if not contact_count(dx, dy):
                                return {"type": "move", "direction": direction}, "retreat"
                    fewer_contacts = [(-contact_count(x, y), threat_distance(x, y), direction)
                                      for x, y, direction in exits if contact_count(x, y) < current_contacts]
                    if fewer_contacts:
                        return disclosed_step_action(view, max(fewer_contacts)[2]), "retreat"
                if hold_scroll:
                    return {"type": "read", "item": hold_scroll}, "hold_threat"
                if scroll := scrolls.get("item.scroll.teleportation"):
                    return {"type": "read", "item": scroll}, "emergency_teleport"
                near_death = {row["actor"] for row in combat["actors"] if row.get("injury") == "near_death"}
                tough_contacts = contacts(0, 0) - near_death

                def displace_contact():
                    if ability(view, "dislocating_strike"):
                        for dx, dy, _ in memory.graph.directions:
                            target = next((foe for foe in tough_contacts if (dx, dy, 0) in foe_bodies[foe]), None)
                            if not target or aim_key(view, key, "dislocating_strike", target) in memory.rejected_aims:
                                continue
                            body_cells = foe_bodies[target]
                            landing = [(x + dx, y + dy, z) for x, y, z in body_cells]
                            if (all((x, y) == (dx, dy) for x, y, _ in body_cells)
                                    and all(pos not in obstacles and (tile := visible.get(pos))
                                            and (not tile.get("door") or tile["door"]["open"]) for pos in landing)):
                                return use("dislocating_strike", target), "melee"
                    return None

                displacement = displace_contact()
                if current_contacts > 1 and displacement:
                    return displacement
                if control := self.control_threat(view, visible, hostile):
                    return control
                if zap := offensive_device(view, visible, hostile, self.tried_unmeasured_offensive_devices):
                    return zap, "ranged_device"
                if haste:
                    return {"type": "drink", "item": haste}, "combat_haste"
                if displacement:
                    return displacement
                current_foes = contacts(0, 0)
                dangerous_contacts = current_foes & dangerous
                if (current_foes and (critical_health or dangerous_contacts)
                        and not memory.avoidance_steps[key]):
                    def open_retreat_cell(x, y):
                        cell = visible.get((x, y, 0))
                        return (cell and not cell["wall"] and cell["key"] not in blocked
                                and cell["key"] not in memory.traps
                                and (not cell.get("door") or cell["door"]["open"]))

                    for x, y, direction in exits:
                        if not open_retreat_cell(x, y) or contacts(x, y) - current_foes:
                            continue
                        middle = visible[(x, y, 0)]["key"]
                        for dx, dy, onward in memory.graph.directions:
                            if ((middle, onward) not in memory.rejected_edges
                                    and threat_distance(x + dx, y + dy) >= 2
                                    and open_retreat_cell(x + dx, y + dy)):
                                return {"type": "move", "direction": direction}, "break_contact"

                hp = int(combat["hp"])
                if hp < int(combat["max_hp"]) and any(
                        self.largest_hits.get(key, 0) >= hp
                        for actor in current_foes for key in self.threat_keys(actor)):
                    if potion := healing_potion(view):
                        return {"type": "drink", "item": potion}, "heal"

                if (not self.emergency_magic_used and tough_contacts
                        and (critical_health or tough_contacts & unsafe_foes)):
                    if potion := self.untried_consumable(view, "potion", "drinkable"):
                        return {"type": "drink", "item": potion}, "test_unknown_magic"
                    if scroll := self.untried_consumable(view, "scroll", "readable"):
                        return {"type": "read", "item": scroll}, "test_unknown_magic"

            spark = ability(view, "spark_lance_cold")
            injury_rank = {"near_death": 0, "badly_wounded": 1, "wounded": 2}
            target_rank = {row["actor"]: injury_rank.get(row.get("injury"), 3) for row in combat["actors"]}
            for actor in sorted(others, key=lambda foe: target_rank[foe["id"]]):
                if (spark and ground_sight_candidate(actor, spark["reach"]["range"])
                        and aim_key(view, key, spark["id"], actor["id"]) not in memory.rejected_aims):
                    return use(spark["id"], actor["id"]), "ranged"
                if melee_candidate(body, position(actor), visible):
                    for identity in ("rattling_blow", "basic_melee"):
                        if (ability(view, identity)
                                and aim_key(view, key, identity, actor["id"]) not in memory.rejected_aims):
                            return use(identity, actor["id"]), "melee"

            if zap := offensive_device(view, visible, hostile, self.tried_unmeasured_offensive_devices):
                return zap, "ranged_device"

            if projectile_choices:
                if shot := projectile_action(view, visible, hostile, projectile_choices):
                    return shot

            if not stair_delayed and (carrying or 12 <= self.floor < self.retrieval_depth):
                if onward := stair_step(carrying):
                    direction = onward[0].get("direction")
                    offset = next(((dx, dy) for dx, dy, name in memory.graph.directions
                                   if name == direction), None)
                    if direction in {"up", "down"}:
                        return onward
                    if offset is not None and threat_distance(*offset) >= threat_distance(0, 0):
                        return onward

            if experiment := self.try_unknown_device(view, visible, hostile):
                return experiment, "test_unknown_device"

            if holds:
                return {"type": "escape", "hold": holds[0]["target"]}, "escape_hold"
            if movement_restricted:
                return {"type": "wait"}, "wait_for_restraint"
            if critical_health:
                return {"type": "wait"}, "hold_position"

            if self.floor >= 12 and not stair_delayed and memory.avoidance_steps[key] < 2:
                safe_distance = max(2, threat_distance(0, 0))
                exposed = {cell["key"] for (x, y, z), cell in visible.items()
                           if z == 0 and threat_distance(x, y) < 2}
                exposed.update(cell["key"] for dx, dy, _ in memory.graph.directions
                               if (cell := visible.get((dx, dy, 0))) and threat_distance(dx, dy) < safe_distance)
                unexplored = memory.frontier() | (memory.graph.edges.keys() - memory.visited)
                direction = memory.first_step(key, unexplored, blocked | exposed)
                if direction:
                    return disclosed_step_action(view, direction, wait_for_restraint=True), "avoid_foe"

            if dangerous_foe or protect_recovery:
                return {"type": "wait"}, "hold_position"

            if not carrying:
                for foe in others:
                    target = foe["id"]
                    if target in dangerous or self.pursuit_steps[target] >= 12:
                        continue
                    contact = {cell["key"] for pos, cell in visible.items()
                               if pos[2] == 0 and cell["key"] not in blocked and pos != (0, 0, 0)
                               and any(sum(abs(a + b - c) for a, b, c in zip(pos, offset, position(foe))) == 1
                                       for offset in body)}
                    if step := move_toward(contact, "approach_foe"):
                        self.pursuit_target = target
                        return step

        hp, max_hp = int(combat["hp"]), int(combat["max_hp"])
        nutrition = combat["own_stats"]["nutrition"]
        hunger = nutrition.get("hunger")
        ration_needed = hunger in {"hungry", "weak", "empty"}
        return_ticks = sum(ticks for floor, ticks in self.return_travel_ticks.items()
                           if floor < self.floor)
        short_for_ascent = (return_ticks > 0 and nutrition.get("kind") == "required"
                            and food_reserve_ticks(nutrition) < return_ticks)
        carried_food = {item["id"]: item for item in inventory if is_food(item)}
        food_servings = sum(int(item.get("quantity", 1)) for item in carried_food.values())
        if memory.food_scout_end is not None and tick < memory.food_scout_end:
            if food_servings > memory.food_scout_servings:
                memory.food_scout_end = tick
            else:
                memory.food_scout_servings = food_servings
        edible_items = [carried_food[row["item"]] for row in view.get("interactions", {}).get("inventory", [])
                        if row.get("edible") and row["item"] in carried_food]
        edible = edible_items[0]["id"] if edible_items else None
        self.observe_meal(carried_food, nutrition)
        if not others:
            if ration_needed and edible:
                return {"type": "eat", "item": edible}, "eat"
            if hunger is not None and not memory.amulet_cells:
                if food := next((row["item"] for row in ground_food if row["reachable"]), None):
                    return {"type": "take", "item": food["id"], "quantity": food["quantity"]}, "loot"
                visible_food = {visible[pos]["key"] for row in ground_food if (pos := position(row)) in visible}
                food_danger = {cell: 8 for cell, until in memory.recent_danger.items() if until > tick}
                if step := move_toward(visible_food, "approach_loot", food_danger, allow_diagonal=True):
                    return step
                if (ration_needed or short_for_ascent) and not edible:
                    if step := move_toward(memory.food_cells, "forage", food_danger, allow_diagonal=True):
                        return step
                if (not carrying and self.floor < self.retrieval_depth and nutrition.get("kind") == "required"
                        and not stair_delayed and hp * 5 >= max_hp * 4
                        and (here["stairs_down"] or memory.food_scout_end is not None)
                        and all(item.get("identity") in self.food_value for item in edible_items)):
                    trip_ticks = self.estimate_trip_ticks(return_ticks)
                    supplies = sum(self.food_value[item["identity"]] * int(item.get("quantity", 1))
                                   for item in edible_items)
                    available = food_reserve_ticks(nutrition) + supplies * int(nutrition["unit_ticks"])
                    scout_end = memory.food_scout_end
                    if scout_end is None and here["stairs_down"] and available < trip_ticks:
                        scout_end = tick + food_reserve_ticks(nutrition) // 4
                    if scout_end is not None and tick < scout_end and available < trip_ticks:
                        if step := move_toward(memory.frontier(), "scout_for_food", food_danger,
                                               allow_diagonal=True):
                            memory.food_scout_end = scout_end
                            memory.food_scout_servings = food_servings
                            return step
            interactions = view.get("interactions", {})
            if interactions.get("curse_blocked") or any(
                    offer.get("equipped_slot") is not None and offer.get("known_cursed")
                    for offer in interactions.get("inventory", [])):
                if scroll := known_items(view, "readable").get("item.scroll.remove-curse"):
                    return {"type": "read", "item": scroll}, "remove_curse"
            potions = known_items(view, "drinkable")
            losses = combat["own_stats"].get("restorable_losses", {}).get("attributes", {})
            if losses.get("strength", 0) > 0 and (potion := potions.get("item.potion.restore-strength")):
                return {"type": "drink", "item": potion}, "restore_strength"
            for identity in ("item.potion.raise-level", "item.potion.gain-strength"):
                if growth := potions.get(identity):
                    return {"type": "drink", "item": growth}, "permanent_growth"
            if hp * 5 >= max_hp * 4:
                memory.recovery_threat_cells.clear()
            if not ration_needed and hp * 5 < max_hp * 4:
                care = first_aid(view)
                if care[1] == "recover" and combat["own_stats"]["nutrition"].get("kind") == "required":
                    if ring := equipment_upgrade(view, conserve_food=True):
                        return ring, "conserve_food"
                if (care[1] == "recover" and hp * 2 < max_hp and self.floor > 1
                        and not movement_restricted and not stair_delayed and memory.recovery_threat_cells):
                    if here["stairs_up"]:
                        return {"type": "move", "direction": "up"}, "retreat_upstairs"
                    if direction := memory.first_step(key, memory.stairs_up, blocked | memory.recovery_threat_cells):
                        return disclosed_step_action(view, direction), "retreat_to_stairs"
                recovery_return_ticks = return_ticks if hp * 2 >= max_hp else 0
                if (care[1] == "recover" and not stair_delayed
                        and (carrying or self.floor < self.retrieval_depth)
                        and not recovery_fits_food_budget(combat, edible is not None, recovery_return_ticks)):
                    allow_unvisited = carrying or not recovery_fits_food_budget(combat, edible is not None)
                    if allow_unvisited or here["stairs_down"]:
                        if step := stair_step(carrying):
                            return step
                    if (allow_unvisited and hp * 2 >= max_hp
                            and not movement_restricted and not memory.amulet_cells):
                        unexplored = memory.graph.edges.keys() - memory.visited
                        danger = {cell for cell, until in memory.recent_danger.items() if until > tick}
                        avoid = blocked | danger | memory.recovery_threat_cells | memory.traps | self.unseen_danger.keys()
                        if direction := memory.first_step(key, unexplored, avoid):
                            return disclosed_step_action(view, direction), "recover_en_route"
                if care[1] == "recover" and memory.recent_danger.get(key, 0) > tick:
                    exposed = {cell for cell, until in memory.recent_danger.items() if until > tick}
                    safe = memory.visited - exposed - memory.recovery_threat_cells - memory.traps
                    if step := move_toward(safe, "reposition_to_recover", dict.fromkeys(exposed, 8)):
                        return step
                if (care[1] == "recover" and hp * 2 >= max_hp and not memory.amulet_cells
                        and not movement_restricted and not stair_delayed
                        and (carrying or self.floor < self.retrieval_depth)):
                    stairs = memory.stairs_up if carrying else memory.graph.stairs_down
                    if stairs and key not in stairs:
                        unvisited = memory.graph.edges.keys() - memory.visited - stairs
                        danger = {cell for cell, until in memory.recent_danger.items() if until > tick}
                        closed = {cell["key"] for cell in visible.values()
                                  if (door := cell.get("door")) and not door["open"]}
                        avoid = (blocked | unvisited | danger | memory.recovery_threat_cells
                                 | memory.traps | self.unseen_danger.keys() | closed)
                        if direction := memory.first_step(key, stairs, avoid):
                            return {"type": "move", "direction": direction}, "recover_en_route"
                return care
        safe_departure = (not others and not ration_needed
                          and int(combat["hp"]) * 5 >= int(combat["max_hp"]) * 4
                          and not stair_delayed)
        if safe_departure and (carrying or self.floor < self.retrieval_depth):
            if step := stair_step(carrying):
                if preparation := prepare_equipment(view):
                    return preparation
                return step
        stock = Counter(item.get("class") for item in inventory)
        needs_darts = (any(offer["id"] == "rogue.throw" for offer in combat["own_stats"]["abilities"])
                       and not any(item.get("identity") == "item.weapon.dart" for item in inventory))
        needs_spear = not any(item.get("identity") == "item.weapon.spear" for item in inventory)
        best_sword_damage = max((SWORD_BASE_DAMAGE.get(item.get("identity"), 0)
                                 for item in inventory if item.get("identified")), default=0)
        limits = {"potion": 4, "scroll": 4, "weapon": 3, "armor": 2, "ring": 2, "wand": 2}
        loot = sorted(view["ground_items"], key=lambda row: (
            not is_retrieval_item(row["item"], self.objective_identity), sum(map(abs, position(row))), position(row),
            row["item"]["name"]))
        for row in loot:
            item = row["item"]
            supply = (item.get("known_supply") or {}).get("id")
            item_class = item.get("class")
            supplies = supply in {"ration", "bandage"}
            objective = is_retrieval_item(item, self.objective_identity) and not carrying
            identity = item.get("identity")
            useful_weapon = item_class == "weapon" and (
                (needs_spear and identity == "item.weapon.spear")
                or (needs_darts and identity == "item.weapon.dart")
                or (item.get("identified") and identity in {"item.weapon.long-sword", "item.weapon.two-handed-sword"}
                    and SWORD_BASE_DAMAGE[identity] > best_sword_damage))
            gear = not carrying and not others and (useful_weapon or stock[item_class] < limits.get(item_class, 0))
            if not (objective or supplies or gear):
                continue
            if row["reachable"]:
                return {"type": "take", "item": item["id"], "quantity": item["quantity"]}, "loot"
            cell = visible.get(position(row))
            if cell and (step := move_toward({cell["key"]}, "approach_loot")):
                return step

        if not carrying:
            if step := move_toward(memory.amulet_cells - {key}, "remembered_amulet"):
                return step

        if ration_needed and edible:
            return {"type": "eat", "item": edible}, "eat"

        if hp * 2 < max_hp:
            self.recovering = True
        elif hp * 5 >= max_hp * 4:
            self.recovering = False
        if not others and self.recovering:
            if hp * 2 < max_hp:
                care = first_aid(view)
                if care[1] == "first_aid" or not ration_needed:
                    return care
            if not ration_needed:
                return {"type": "wait"}, "recover"

        if not others:
            if scroll := identification_scroll(view):
                return scroll, "identify_gear"
            if not carrying and hp * 5 >= max_hp * 4:
                if item := self.untried_consumable(view, "potion", "drinkable"):
                    return {"type": "drink", "item": item}, "test_unknown_magic"
            if preparation := prepare_equipment(view):
                return preparation

        if movement_restricted:
            return {"type": "wait"}, "wait_for_restraint"
        if stair_delayed:
            return {"type": "wait"}, "stair_backoff"

        if carrying or self.floor < self.retrieval_depth:
            if step := stair_step(carrying):
                return step

        if not others and here.get("lit") is False:
            lights = {item["id"] for item in inventory if item.get("identified")
                      and item.get("identity") == "item.wand.wand-light"} - memory.used_light_devices
            for offer in view.get("interactions", {}).get("inventory", []):
                if (offer["item"] in lights and offer.get("zappable")
                        and offer.get("charges") != 0 and offer["device_activation"] == "self_target"):
                    return {"type": "zap", "item": offer["item"], "aim": {"kind": "self_target"}}, "light_room"

        search = ability(view, "rogue.search")
        if (search and not others and memory.corridor_end(key)
                and any(memory.search_attempts[wall] == 0 for wall in memory.boundaries[key].values())):
            return use("rogue.search", view["self_target"]), "search"

        memory.recent_danger = {cell: until for cell, until in memory.recent_danger.items() if until > tick}
        danger_cost = dict.fromkeys(memory.recent_danger.keys() | self.unseen_danger.keys(), 8)
        if step := move_toward(memory.frontier(), "explore", danger_cost, allow_diagonal=True):
            return step
        if step := move_toward(memory.graph.edges.keys() - memory.visited, "explore", danger_cost, allow_diagonal=True):
            return step

        exits = {direction for direction, destination in memory.graph.edges.get(key, {}).items()
                 if destination not in blocked}
        if (not others and exits and key not in memory.movement_waited
                and all((key, direction) in memory.rejected_edges for direction in exits)):
            return {"type": "wait"}, "settle_movement"

        unknown_scroll = None
        if not others and not carrying and hp * 5 >= max_hp * 4:
            unknown_scroll = self.untried_consumable(view, "scroll", "readable")

        if search:
            if chart and not others and mapping.get("supported"):
                if acquired is not None and self.mapping_floors[acquired] == self.floor:
                    if memory.mapping_acquired != acquired and (sample := chart("mapping")) is not None:
                        memory.mapping_chart = MappingChart(sample)
                        memory.mapping_acquired = acquired
                        memory.mapping_checked.clear()
                    if memory.mapping_chart and key not in memory.mapping_checked:
                        memory.mapping_checked.add(key)
                        memory.charted_entrances.update(memory.mapping_chart.search_walls(visible))
                elif not memory.mapping_read:
                    if scroll := known_items(view, "readable").get("item.scroll.magic-mapping"):
                        return {"type": "read", "item": scroll}, "map_floor"
            mapped, reconnect, likely_entrances, corridor_ends, unsearched, revisits = (
                Counter() for _ in range(6))
            stair_component = memory.stair_component() if carrying else set()
            for anchor, edges in memory.boundaries.items():
                for wall in set(edges.values()):
                    attempts = memory.search_attempts[wall]
                    if wall in memory.charted_entrances and attempts < 8:
                        mapped[anchor] += 1
                    elif (attempts < 8 and anchor not in stair_component
                          and stair_component.intersection(memory.joining_wall_hints.get(wall, ()))):
                        reconnect[anchor] += 1
                    elif wall in memory.joining_wall_hints and attempts < 3:
                        likely_entrances[anchor] += 1
                    elif attempts < 2 and memory.corridor_end(anchor):
                        corridor_ends[anchor] += 1
                    elif attempts == 0:
                        unsearched[anchor] += 1
                    elif attempts < 8:
                        revisits[anchor] += 1
            for anchor, directions in memory.neighbors.items():
                # Search may pick up a tactile cue from an unseen adjacent tile.
                # The tile still needs a disclosure before it can join the route graph.
                attempts = memory.tactile_attempts[anchor]
                if len(directions) < len(memory.graph.directions) and attempts < 4:
                    (unsearched if attempts == 0 else revisits)[anchor] += 1
            search_ranks = [mapped, reconnect, likely_entrances, corridor_ends]
            if not unknown_scroll:
                search_ranks.extend((unsearched, revisits))
            for targets in search_ranks:
                if key in targets:
                    return use("rogue.search", view["self_target"]), "search"
                if (targets is unsearched and not others and memory.tactile_attempts[key] == 1
                        and any(memory.search_attempts[wall] == 1
                                for wall in memory.boundaries.get(key, {}).values())):
                    adjacent = {destination for direction, destination in memory.graph.edges.get(key, {}).items()
                                if destination not in blocked and (key, direction) not in memory.rejected_edges}
                    adjacent.update(destination for _, _, _, destination in diagonal_steps())
                    if not adjacent.intersection(unsearched):
                        return use("rogue.search", view["self_target"]), "search"
                if step := move_toward(targets, "search", allow_diagonal=not others, target_value=targets):
                    return step
            if unknown_scroll and key in unsearched:
                return use("rogue.search", view["self_target"]), "search"
        # A scroll may break the route, but a sweep of weak leads also costs food.
        if unknown_scroll:
            return {"type": "read", "item": unknown_scroll}, "test_unknown_magic"

        if carrying and not others and memory.stairs_up:
            if scroll := known_items(view, "readable").get("item.scroll.teleportation"):
                return {"type": "read", "item": scroll}, "find_return_route"
        if not carrying and self.floor >= self.retrieval_depth:
            if step := stair_step(False):
                return step
        return None, "no_disclosed_route"


def earned_training(advancement, view):
    """Use a server-backed preview to spend one point on the current build."""
    stats = view["combat"]["own_stats"]
    has_spear = any(item.get("identified") and item.get("identity") == "item.weapon.spear" for item in view["inventory"])
    known = {entry["expertise"]["id"] for entry in stats.get("expertises", [])
             if entry["status"] == "active"}
    overview = advancement("overview")
    if overview["status"] != "confirmed":
        return None
    entries = (overview.get("page") or {}).get("entries", [])
    if entries and entries[0]["value"].get("locked"):
        return None
    starting = entries[0]["value"].get("starting_skills", []) if entries else []
    attack_skill = ("spellcasting" if any(skill["id"] == "spellcasting" for skill in starting)
                    else "heavy_weaponry")
    talents = {entry["id"] for entry in stats.get("active_talents", [])}
    can_learn_push = (int(stats.get("hit_dice", {}).get("total", 0)) >= 6
                      and "rattling_blow" in talents and "dislocating_strike" not in talents)
    page_result = advancement("balances")
    while page_result["status"] == "confirmed":
        page = page_result["page"]
        for entry in reversed(page["entries"]):
            row = entry["value"]
            if not row["unspent"]:
                continue
            candidates = []
            if has_spear and "rogue.spear" not in known and "expertise" in row["allowed"]:
                candidates.append("expertise rogue.spear")
            if can_learn_push and "talent" in row["allowed"]:
                candidates.append("talent dislocating_strike")
            if "skill_rank" in row["allowed"]:
                candidates.append("skill " + attack_skill)
            prefix = f'purchase {int(row["owner"]) + 1} {row["pool"]["id"]}'
            for selection in candidates:
                proposal = advancement(f"{prefix} {selection}")
                if proposal["status"] == "confirmed":
                    return advancement("confirm")
                if proposal["status"] != "rejected":
                    raise RuntimeError(f"Advancement preview was interrupted: {proposal}")
        if page.get("next") is None:
            return None
        page_result = advancement("next")
    return None


def visible_hostiles(view):
    threats = {row["actor"]: row.get("injury") for row in view["combat"]["actors"] if row["hostile"]}
    visible = {}
    for actor in view["visible_actors"]:
        identity = actor["id"]
        if identity in threats:
            entry = visible.setdefault(identity, {"actor": identity, "name": actor.get("name"),
                                                  "injury": threats[identity], "cells": []})
            entry["cells"].append(position(actor))
    return list(visible.values())


def run(session, policy, budget, report=None):
    """Play a bounded run and emit a JSON decision log."""
    if report is None:
        report = {}
    report.update(actions=0, max_depth=policy.floor, amulet_acquired=False,
                  estimated_ascent_ticks_by_floor=policy.return_travel_ticks,
                  estimated_ascent_ticks=sum(policy.return_travel_ticks.values()))
    trained_hd = 2
    pending_attack_hp = None
    pending_attack = None
    canceled_attack = None
    pending_intention = None
    pending_choice = None
    pending_projectile_tick = None
    blocked_projectile_tick = None
    pending_move = None
    canceled_move = None
    last_stalled_tick = None
    last_refused_observation = None
    for attempt in range(budget + 1):
        events_since_previous_action = []
        deadline = time.monotonic() + 20
        last_tick = report.get("tick")
        recovery_action = None
        while True:
            observation = session.observe()
            data = observation.data()
            view = data["state"]["observation"]
            combat = view["combat"]
            readiness = data["readiness"]
            combat_updates = data.get("combat_updates")
            observed_events = policy.observe_combat(view, combat_updates)
            if combat_updates is not None:
                events_since_previous_action.extend(observed_events)
            here = next((cell for cell in view["visible_cells"] if position(cell) == (0, 0, 0)), None)
            settled = (view["ready"] and readiness["admission"]
                       and combat["preparation_remaining"] is None)
            if here and settled and not combat["terminal"]:
                previous_floor = policy.floor
                policy.observe_stair(here, view["visible_cells"], view.get("tick"))
                if policy.floor > previous_floor:
                    report["estimated_ascent_ticks"] = sum(policy.return_travel_ticks.values())
            report["max_depth"] = max(report["max_depth"], policy.floor)
            report["amulet_acquired"] |= any(is_retrieval_item(item, policy.objective_identity) for item in view["inventory"])
            report["tick"] = int(view["tick"]) if "tick" in view else None
            finished_intention = None
            if settled and pending_intention is not None:
                finished_intention = next((row["phase"] for row in data.get("finished_intentions", [])
                                           if row["intention"] == pending_intention), None)
            finished_intention_id = pending_intention if finished_intention else None
            if finished_intention_id:
                report.update(previous_intention_id=finished_intention_id,
                              previous_intention_phase=finished_intention)
            if settled and pending_projectile_tick is not None:
                if (finished_intention == "failed"
                        or (finished_intention is None and report["tick"] == pending_projectile_tick)):
                    blocked_projectile_tick = report["tick"]
                pending_projectile_tick = None
            prompt = view.get("interactions", {}).get("identification")
            choice_waiting = prompt and prompt["choice"] == pending_choice
            if combat["terminal"] or (not choice_waiting and (prompt or settled or readiness.get("resume"))):
                break
            now = time.monotonic()
            tick = report["tick"]
            if tick is not None and (last_tick is None or tick > last_tick):
                last_tick = tick
                deadline = now + 20
            if now >= deadline:
                intentions = data.get("intentions", [])
                if attempt < budget and tick is not None and tick != last_stalled_tick:
                    if (readiness["admission"] and not intentions and not view["ready"]
                            and combat["preparation_remaining"] is None):
                        recovery_action = "wait"
                    elif (pending_intention and readiness.get("cancel") == [pending_intention]
                          and any(row["intention"] == pending_intention and row["phase"] == "queued"
                                  for row in intentions) and hasattr(session, "cancel")):
                        recovery_action = "cancel"
                if recovery_action:
                    last_stalled_tick = tick
                    break
                report["last_readiness"] = readiness
                report["last_intentions"] = intentions
                report["last_actor_ready"] = view["ready"]
                report["last_preparation_remaining"] = combat["preparation_remaining"]
                return "readiness_timeout"
            time.sleep(0.05)
        if combat["terminal"]:
            report["events_since_previous_action"] = events_since_previous_action
            if combat["dead"]:
                report["last_combat_events"] = combat.get("events", [])
                report["self_target"] = view["self_target"]
                report["last_nutrition"] = combat["own_stats"]["nutrition"]
            return policy.choose(data["state"])[1]
        if attempt == budget:
            return "action_budget"
        resuming = not prompt and bool(readiness.get("resume")) and (
            not readiness["admission"] or combat["preparation_remaining"] is not None)
        if recovery_action == "wait":
            action, reason = {"type": "wait"}, "advance_stalled_recovery"
            outcome = session.act(action, observation)
        elif recovery_action == "cancel":
            action, reason = {"type": "cancel_intention"}, "cancel_stalled_action"
            outcome = session.cancel(observation)
            if outcome["status"] == "confirmed":
                pending_intention = None
                pending_attack_hp = None
                pending_attack = None
                pending_choice = None
                pending_projectile_tick = None
                canceled_move = pending_move
                pending_move = None
                policy.pending_stair = None
        elif resuming:
            cancel = getattr(session, "cancel", None)
            if (cancel and pending_attack_hp is not None and int(combat["hp"]) < pending_attack_hp
                    and readiness.get("cancel") and policy.may_need_escape(view)):
                action, reason = {"type": "cancel_intention"}, "cancel_dangerous_attack"
                outcome = cancel(observation)
                if outcome["status"] == "confirmed":
                    canceled_attack = pending_attack
                    pending_attack = None
                    pending_attack_hp = None
            else:
                action, reason = {"type": "resume_intention"}, "resume"
                outcome = session.resume(observation)
        else:
            outcome = None
            advance = getattr(session, "advancement", None)
            hit_dice = int(combat["own_stats"].get("hit_dice", {}).get("total", 0))
            if (advance and not prompt and hit_dice > trained_hd
                    and not any(actor["hostile"] for actor in combat["actors"])):
                outcome = earned_training(advance, view)
                if outcome is None:
                    trained_hd = hit_dice
            if outcome is not None:
                action, reason = {"type": "purchase"}, "earned_training"
            else:
                offer = getattr(session, "projectile_choices", None)
                # A settled, same-tick receipt can mean the admitted projectile failed.
                if blocked_projectile_tick is not None and blocked_projectile_tick == report["tick"]:
                    offer = None
                query = (lambda identity, phase, offset: offer(observation, identity, phase, offset)) if offer else None
                chart_method = getattr(session, "chart", None)
                chart_query = (lambda kind: chart_method(observation, kind)) if chart_method else None
                action, reason = policy.choose(data["state"], projectile_choices=query, chart=chart_query)
                if action is None:
                    if reason == "no_disclosed_route":
                        memory = policy.floors[policy.floor]
                        report["navigation"] = {
                            "floor": policy.floor, "cell": here["key"],
                            "visited_cells": len(memory.visited),
                            "known_up_stairs": len(memory.stairs_up & memory.graph.edges.keys()),
                            "known_down_stairs": len(memory.graph.stairs_down),
                            "frontier_cells": len(memory.frontier()),
                            "rejected_edges": len(memory.rejected_edges),
                            "search_sites": len(memory.tactile_attempts),
                            "searches_admitted": memory.tactile_attempts.total(),
                        }
                    return reason
                if (action == canceled_move and report["tick"] == last_stalled_tick
                        and not any(actor["hostile"] for actor in combat["actors"])):
                    action, reason = {"type": "wait"}, "wait_before_retrying_move"
                outcome = session.act(action, observation)
        report["actions"] += 1
        print(json.dumps({"cursor": data["cursor"], "floor": policy.floor, "cell": here["key"] if here else None,
                          "tick": report["tick"], "hp": combat["hp"], "max_hp": combat["max_hp"],
                          "hunger": combat["own_stats"]["nutrition"].get("hunger"),
                          "nutrition_reserve": combat["own_stats"]["nutrition"].get("reserve"),
                          "health_recovery": combat["own_stats"].get("health_recovery"),
                          "hostiles": visible_hostiles(view), "events": combat.get("events", []), "reason": reason,
                          "events_since_previous_action": events_since_previous_action,
                          "previous_intention_id": finished_intention_id,
                          "previous_intention_phase": finished_intention,
                          "amulet_acquired": report["amulet_acquired"],
                          "action": action, "result": outcome}), flush=True)
        if outcome["status"] == "confirmed":
            if not resuming and recovery_action != "cancel":
                policy.accepted(action, view, reason=reason)
                receipt = outcome.get("receipt") or {}
                pending_intention = receipt.get("intention")
                if receipt.get("phase") == "queued" and reason in {"throw_projectile", "launch_projectile"}:
                    pending_projectile_tick = report["tick"]
                pending_move = action if action["type"] == "move" else None
                canceled_move = None
                pending_choice = action["choice"] if action["type"] == "identify" else None
                attack = reason in {"melee", "ranged", "ranged_device", "throw_projectile",
                                    "launch_projectile", "test_unknown_device", "control_threat"}
                pending_attack = action if attack else None
                # Replanning can select the same attack when no safer action is available.
                pending_attack_hp = int(combat["hp"]) if attack and action != canceled_attack else None
                canceled_attack = None
        elif outcome["status"] == "rejected" and policy.rejected(action, view, outcome):
            continue
        elif outcome["status"] != "stale_observation":
            refused_observation = (data["cursor"], readiness.get("revision"))
            if (outcome["status"] == "rejected" and outcome.get("code") in {"actor_busy", "stale_context"}
                    and refused_observation != last_refused_observation):
                last_refused_observation = refused_observation
                continue
            return outcome["status"]


def buy_starting_talent(session, character):
    """Spend the bundled preset's first point through its owner preview."""
    if character not in CHARACTERS:
        return
    view = session.observe().data()["state"]["observation"]
    stats = view["combat"]["own_stats"]
    ability_id = CHARACTERS[character][2]
    if any(offer["id"] == ability_id for offer in stats["abilities"]):
        return
    overview = session.advancement("overview")
    if overview["status"] != "confirmed":
        raise RuntimeError(f"Could not read starting advancement: {overview}")
    entries = overview["page"]["entries"]
    if not entries or entries[0]["value"].get("locked") or int(entries[0]["value"]["points"]["unspent"]) == 0:
        return
    try:
        body_hd = starting_body_hd(stats["hit_dice"], character)
    except ControllerError:
        # An actor number alone does not identify a compatible preset.
        return
    for operation in (first_purchase(character, body_hd=body_hd), "confirm"):
        result = session.advancement(operation)
        if result["status"] != "confirmed":
            raise RuntimeError(f"Could not confirm starting talent: {result}")


def main():
    from tor_client_bot import Session

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--connect", default="127.0.0.1:4000")
    parser.add_argument("--actor", type=int, default=1)
    parser.add_argument("--budget", type=int, default=20_000)
    parser.add_argument("--retrieval-depth", type=int, default=DEFAULT_RETRIEVAL_DEPTH)
    parser.add_argument("--objective-identity", default=DEFAULT_OBJECTIVE_IDENTITY)
    args = parser.parse_args()
    if args.retrieval_depth < 1:
        parser.error("Retrieval depth must be positive")
    session = Session.connect(args.connect, os.environ["TOR_SERVER_TOKEN"], args.actor)
    try:
        buy_starting_talent(session, args.actor)
        report = {}
        policy = Policy(retrieval_depth=args.retrieval_depth, objective_identity=args.objective_identity)
        result = run(session, policy, args.budget, report)
        print(json.dumps({"terminal": result, **report}), flush=True)
        return 0 if result == "victory" else 1
    finally:
        session.close()


if __name__ == "__main__":
    raise SystemExit(main())
