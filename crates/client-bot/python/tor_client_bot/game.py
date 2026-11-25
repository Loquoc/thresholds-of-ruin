"""Player decisions and command submission over a common-client attachment."""

from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
from functools import cached_property
from itertools import product
import json
import logging
import time
from typing import TYPE_CHECKING, Iterator

from .models import (
    HORIZONTAL,
    Ability,
    Action,
    Activity,
    ActivityChoice,
    ActivityPhase,
    Cell,
    Chart,
    ChartComponent,
    ChartKind,
    ChartMark,
    CombatReport,
    Condition,
    Creature,
    Device,
    DeviceActivation,
    Direction,
    Door,
    Equip,
    EquipmentSlot,
    Escape,
    GroundItem,
    Health,
    Hold,
    Identification,
    Identify,
    Injury,
    Item,
    ItemUse,
    Move,
    Nutrition,
    OpenDoor,
    Outcome,
    Pickup,
    ProjectileOption,
    ProjectileRange,
    Recovery,
    RestorableLosses,
    Result,
    ServerNotice,
    Shot,
    StairAttempt,
    Trap,
    Unequip,
    UseAbility,
    UseItem,
    Zap,
    WeaponAttack,
    _direct_command,
)

from .training import Training

if TYPE_CHECKING:
    from . import _native


def _armor_reduction(equipment: dict | None) -> int | None:
    """Common unconditional reduction against both impact and keen damage."""
    if equipment is None:
        return None
    amounts = {"impact": 0, "keen": 0}
    for modifier in equipment["modifiers"]:
        if modifier["id"] not in {"reduction", "signed_reduction"}:
            continue
        fields = {field["id"]: field["value"]["value"] for field in modifier["fields"]}
        category = fields.get("category", {}).get("id")
        if fields.get("scope") == "always" and category in amounts:
            amounts[category] += int(fields["amount"])
    return min(amounts.values())


def _weapon_attack(equipment: dict | None) -> WeaponAttack | None:
    if not equipment or not (attack := equipment.get("attack")):
        return None
    damage = float(attack["damage_enchantment"])
    for component in attack["damage"]["components"]:
        amount = component["amount"]
        if amount["type"] == "fixed":
            damage += amount["value"]
        else:
            damage += amount["count"] * (amount["sides"] + 1) / 2 + amount["bonus"]
    return WeaponAttack(attack["skill"]["id"], attack["bonus"], damage, attack["wind_up"], attack["recovery"])


def _item(row: dict, uses: tuple[ItemUse, ...] = (), *, offer: dict | None = None,
          known_use: dict | None = None) -> Item:
    offer = offer or {}
    known_use = known_use or {}
    return Item(
        row["id"],
        row["name"],
        row["identity"] if row["identified"] else None,
        int(row["quantity"]),
        uses,
        row.get("class", "misc"),
        row.get("appearance"),
        _armor_reduction(offer.get("known_equipment")) if row.get("class") == "armor" else None,
        offer.get("known_cursed"),
        _weapon_attack(offer.get("known_equipment")),
        offer.get("known_equipment") is not None,
        offer.get("known_protected"),
        tuple(known_use.get("identifies", ())),
        ProjectileRange(**known_use["throw_range"]) if known_use.get("throw_range") else None,
        ProjectileRange(**known_use["launch_range"]) if known_use.get("launch_range") else None,
    )


class Turn:
    """One player's observation, including the attachment used to submit a choice."""

    def __init__(
        self,
        data: dict,
        *,
        connection: _native.Session | None = None,
        snapshot: _native.Observation | None = None,
    ):
        self._last_submission: Result | None = None
        self._session = connection
        self._snapshot = snapshot
        self._picture = data
        self._chart_pages: dict[str, dict | None] = data.get("queried_charts", {})
        self._projectile_choices: dict[str, dict | None] = data.get("queried_projectiles", {})
        self._view = data["state"]["observation"]
        self._readiness = data["readiness"]
        self._combat_updates = data.get("combat_updates", ())
        self._stream = data.get("stream")
        self._can_resume = data.get("can_resume", False)
        self._can_cancel = data.get("can_cancel", False)

    def recording(self) -> str:
        """Save the disclosed picture for an offline decision test; contains no credentials."""
        picture = self._picture.copy()
        if self._chart_pages:
            picture["queried_charts"] = self._chart_pages
        if self._projectile_choices:
            picture["queried_projectiles"] = self._projectile_choices
        return json.dumps(picture, separators=(",", ":"))

    @classmethod
    def replay(cls, recording: str) -> Turn:
        """Inspect a recorded picture without connecting to a server."""
        return cls(json.loads(recording))

    def _require_connection(self) -> tuple[_native.Session, _native.Observation]:
        if self._session is None or self._snapshot is None:
            raise RuntimeError("This operation needs a live player connection")
        return self._session, self._snapshot

    @cached_property
    def activities(self) -> tuple[Activity, ...]:
        """Actions still queued, underway or interrupted."""
        return tuple(Activity.from_row(row) for row in self._picture.get("intentions", ()))

    @cached_property
    def server_notice(self) -> ServerNotice | None:
        row = self._picture.get("server_notice")
        return ServerNotice(**row) if row else None

    @cached_property
    def finished_activities(self) -> tuple[Activity, ...]:
        """Recent terminal actions retained by the common client."""
        failures = self._picture.get("intention_failures", {})
        return tuple(Activity.from_row(row, failures.get(row["intention"])) for row in self._picture.get("finished_intentions", ()))

    def outcome_of(self, result: Result) -> ActivityPhase | None:
        """Latest observed phase for this receipt, or None outside the retained history."""
        original = result.activity
        if original is None:
            return None
        for group in (self.activities, self.finished_activities):
            for activity in reversed(group):
                if (
                    activity.actor == original.actor
                    and activity.branch == original.branch
                    and activity.intention == original.intention
                ):
                    return activity.phase
        return None

    @property
    def terminal(self) -> bool:
        return self._view.get("combat", {}).get("terminal", False)

    @property
    def can_act(self) -> bool:
        return (
            self._view["ready"]
            and self._readiness["admission"]
            and not self.terminal
            and self._view.get("combat", {}).get("preparation_remaining") is None
        )

    @property
    def falling(self) -> bool:
        """Whether the observed body has downward velocity."""
        motion = self._view.get("motion")
        return bool(motion and int(motion["velocity"][2]) < 0)

    @property
    def can_resume(self) -> bool:
        return self._can_resume

    @property
    def can_cancel(self) -> bool:
        return self._can_cancel

    @property
    def needs_identification(self) -> bool:
        """Whether the server is accepting an answer to the disclosed prompt."""
        return bool(self._readiness.get("admission") and self._view.get("interactions", {}).get("identification"))

    @cached_property
    def health(self) -> Health | None:
        combat = self._view.get("combat")
        if combat is None:
            return None
        return Health(int(combat["hp"]), int(combat["max_hp"]))

    @property
    def hit_dice(self) -> int | None:
        dice = self._view.get("combat", {}).get("own_stats", {}).get("hit_dice")
        return None if dice is None else int(dice["total"])

    @cached_property
    def visible_cells(self) -> tuple[Cell, ...]:
        rows = self._view["visible_cells"]
        doors: dict[str, Door] = {}
        for row in rows:
            if offer := row.get("door"):
                key = offer["id"]
                if key not in doors or (offer["reachable"] and not doors[key].reachable):
                    doors[key] = Door(key, offer["open"], offer["reachable"])
        cells = []
        for row in rows:
            pos = row["position"]
            offer = row.get("door")
            door = doors[offer["id"]] if offer else None
            trap = row.get("trap")
            cells.append(
                Cell(
                    row["key"],
                    (pos["x"], pos["y"], pos["z"]),
                    row["wall"],
                    row["stairs_up"],
                    row["stairs_down"],
                    door,
                    Trap(trap["id"], trap["family"], trap["reachable"]) if trap else None,
                    lit=row.get("lit"),
                )
            )
        return tuple(cells)

    @cached_property
    def cell(self) -> Cell | None:
        """The visible cell at the actor's origin, when disclosed."""
        return self._local_map.get((0, 0, 0))

    def cell_at(self, offset: tuple[int, int, int]) -> Cell | None:
        """Visible cell at an offset from the actor, or None outside its view."""
        return self._local_map.get(offset)

    @cached_property
    def _local_map(self) -> dict[tuple[int, int, int], Cell]:
        return {cell.offset: cell for cell in self.visible_cells}

    @cached_property
    def body(self) -> tuple[tuple[int, int, int], ...]:
        """Occupied cells of the observer that appear in this picture."""
        return tuple(
            (row["position"]["x"], row["position"]["y"], row["position"]["z"])
            for row in self._view["visible_actors"]
            if row["id"] == self.self_target
        )

    def distance_to(self, creature: Creature, *, from_offset: tuple[int, int, int] = (0, 0, 0)) -> int:
        """Minimum Chebyshev distance between visible bodies after translating the observer."""
        dx, dy, dz = from_offset
        return min(
            max(abs(x + dx - tx), abs(y + dy - ty), abs(z + dz - tz))
            for x, y, z in self.body or ((0, 0, 0),)
            for tx, ty, tz in creature.cells
        )

    def in_contact(self, creature: Creature, *, from_offset: tuple[int, int, int] = (0, 0, 0)) -> bool:
        """Whether the observer's translated body touches the creature through disclosed open terrain."""
        dx, dy, dz = from_offset
        for x, y, z in self.body:
            source = x + dx, y + dy, z + dz
            for target in creature.cells:
                if max(abs(a - b) for a, b in zip(source, target)) != 1:
                    continue
                corners = product({source[0], target[0]}, {source[1], target[1]}, {source[2], target[2]})
                if all(
                    (cell := self._local_map.get(at))
                    and not cell.wall
                    and (not cell.door or cell.door.open)
                    for at in corners
                    if at not in (source, target)
                ):
                    return True
        return False

    @cached_property
    def _occupied(self) -> frozenset[tuple[int, int, int]]:
        return frozenset(at for creature in self.creatures for at in creature.cells)

    def stair_attempt(self, move: Move, receipt: Result | None = None) -> StairAttempt:
        """Keep the origin and visible landing without retaining the live picture."""
        landing = self.stair_landing(move.direction)
        if self.cell is None:
            raise ValueError("The stair origin is outside this observation")
        return StairAttempt(move.direction, self.cell.key, landing.key if landing else None, self.tick, receipt)

    def stair_arrival(self, attempt: StairAttempt) -> bool | None:
        """True for witnessed arrival, False for a refused or failed move, otherwise unknown."""
        if attempt.receipt is not None:
            if attempt.receipt.status in (Outcome.REJECTED, Outcome.UNAVAILABLE, Outcome.STALE):
                return False
            phase = self.outcome_of(attempt.receipt)
            if phase in (ActivityPhase.CANCELLED, ActivityPhase.FAILED):
                return False
            if attempt.receipt.status is not Outcome.CONFIRMED or phase in (
                ActivityPhase.QUEUED, ActivityPhase.PAUSED, ActivityPhase.SUSPENDED, ActivityPhase.STARTED
            ):
                return None
        here = self.cell
        if here is None or here.key == attempt.origin or self.tick < attempt.tick:
            return None
        if attempt.landing is not None:
            if any(
                cell.key == attempt.landing and cell.offset[2] == 0 and max(map(abs, cell.offset)) <= 1
                for cell in self.visible_cells
            ):
                return True
        elif (here.stairs_up if attempt.direction is Direction.DOWN else here.stairs_down):
            return True
        return None

    def stair_landing(self, direction: Direction) -> Cell | None:
        """Recognize a visible opposite stair along the actor's vertical axis."""
        if direction not in (Direction.UP, Direction.DOWN):
            raise ValueError("Choose up or down for a stair")
        descending = direction is Direction.DOWN
        candidates = (
            cell
            for cell in self.visible_cells
            if cell.offset[:2] == (0, 0)
            and (
                cell.offset[2] < 0 and cell.stairs_up
                if descending
                else cell.offset[2] > 0 and cell.stairs_down
            )
        )
        return max(candidates, key=lambda cell: abs(cell.offset[2]), default=None)

    def step(self, direction: Direction) -> Move | OpenDoor | None:
        """A candidate horizontal step, including opening a reachable door first.

        Uses the visible terrain and bodies. Only the server can resolve movement.
        """
        return self._step_from((0, 0, 0), direction)

    def walkable_from(self, cell: Cell) -> tuple[Direction, ...]:
        """Possible horizontal moves if the actor stood here in the current picture."""
        if self.cell_at(cell.offset) is not cell:
            raise ValueError("Choose a cell from this observation")
        if cell.wall or cell.offset in self._occupied or (cell.door and not cell.door.open):
            return ()
        return tuple(d for d in HORIZONTAL if isinstance(self._step_from(cell.offset, d), Move))

    def _step_from(self, origin: tuple[int, int, int], direction: Direction) -> Move | OpenDoor | None:
        if direction not in HORIZONTAL:
            raise ValueError("step expects a horizontal direction")
        dx, dy = HORIZONTAL[direction]
        ox, oy, oz = origin

        footprint = self.body
        if (0, 0, 0) not in footprint:
            footprint = ((0, 0, 0), *footprint)

        def translated_cells(x: int, y: int) -> Iterator[Cell | None]:
            for bx, by, bz in footprint:
                at = (ox + bx + x, oy + by + y, oz + bz)
                cell = self._local_map.get(at)
                if at in self._occupied or (cell is not None and cell.wall):
                    yield None
                elif cell is not None:
                    yield cell

        if dx and dy:
            for x, y in ((dx, 0), (0, dy)):
                if (ox + x, oy + y, oz) not in self._local_map or any(
                    cell is None or (cell.door and not cell.door.open) for cell in translated_cells(x, y)
                ):
                    return None
        if (ox + dx, oy + dy, oz) not in self._local_map:
            return None
        closed = None
        for cell in translated_cells(dx, dy):
            if cell is None:
                return None
            if cell.door and not cell.door.open:
                if not cell.door.reachable:
                    return None
                closed = cell.door
        return OpenDoor(closed) if closed else Move(direction)

    @cached_property
    def walkable_directions(self) -> tuple[Direction, ...]:
        """Horizontal directions offering an immediate move in this picture."""
        return tuple(direction for direction in HORIZONTAL if isinstance(self.step(direction), Move))

    @cached_property
    def creatures(self) -> tuple[Creature, ...]:
        """Visible creatures other than the observer, with disclosed combat facts."""
        combat = {row["actor"]: row for row in self._view.get("combat", {}).get("actors", ())}
        bodies: dict[str, list[tuple[int, int, int]]] = {}
        names: dict[str, str] = {}
        for actor in self._view["visible_actors"]:
            target = actor["id"]
            if target == self._view["self_target"]:
                continue
            pos = actor["position"]
            bodies.setdefault(target, []).append((pos["x"], pos["y"], pos["z"]))
            names[target] = actor["name"]
        creatures = []
        for target, cells in bodies.items():
            facts = combat.get(target, {})
            creatures.append(
                Creature(
                    target,
                    names[target],
                    tuple(cells),
                    facts.get("hostile"),
                    facts.get("allied"),
                    Injury(facts["injury"]) if facts.get("injury") else None,
                )
            )
        return tuple(creatures)

    @cached_property
    def hostiles(self) -> tuple[Creature, ...]:
        return tuple(creature for creature in self.creatures if creature.hostile)

    @cached_property
    def _creature_cells(self) -> dict[int, tuple[Cell, ...]]:
        return {id(creature): tuple(self._local_map[at] for at in creature.cells if at in self._local_map)
                for creature in self.creatures}

    def cells_of(self, creature: Creature) -> tuple[Cell, ...]:
        """Disclosed terrain cells occupied by this creature in the current picture."""
        return self._creature_cells.get(id(creature), ())

    @cached_property
    def _inventory_offers(self) -> dict[str, dict]:
        return {row["item"]: row for row in self._view.get("interactions", {}).get("inventory", ())}

    @cached_property
    def inventory(self) -> tuple[Item, ...]:
        items = []
        known_uses = {use["item"]: use for use in self._view.get("known_item_uses", ())}
        for row in self._view["inventory"]:
            offer = self._inventory_offers.get(row["id"], {})
            uses = tuple(
                use
                for use, field in (
                    (ItemUse.EAT, "edible"),
                    (ItemUse.DRINK, "drinkable"),
                    (ItemUse.READ, "readable"),
                )
                if offer.get(field)
            )
            items.append(_item(row, uses, offer=offer, known_use=known_uses.get(row["id"])))
        return tuple(items)

    @cached_property
    def devices(self) -> tuple[Device, ...]:
        """Currently offered devices, with aim modes and charges as disclosed to this player."""
        devices = []
        for item in self.inventory:
            offer = self._inventory_offers.get(item.target, {})
            if offer.get("zappable") and (mode := offer.get("device_activation")):
                charges = offer.get("charges")
                devices.append(Device(item, DeviceActivation(mode), int(charges) if charges is not None else None))
        return tuple(devices)

    def aim_device(self, device: Device, creature: Creature, *, clear_horizontal_ray: bool = False,
                   reflected_ray_range: int | None = None) -> Zap | None:
        """Aim at the nearest suitable visible body cell. The server resolves the actual ray.

        Set clear_horizontal_ray to require a disclosed cardinal or diagonal lane with
        no intervening body, wall or closed door. A diagonal needs one open side.
        For a reflecting ray, pass its range in cells. The lane must clear both sides
        of every diagonal until a reflection could no longer reach the shooter's body.
        """
        if reflected_ray_range is not None and reflected_ray_range < 1:
            raise ValueError("A reflecting ray needs a positive range")
        if device.activation is DeviceActivation.SELF_TARGET or not any(device is d for d in self.devices):
            return None
        check_lane = clear_horizontal_ray or reflected_ray_range is not None
        for offset in self._aim_offsets(creature, reflected_ray_range):
            if not check_lane or self._clear_horizontal_ray(offset, reflected_ray_range):
                return Zap(device, self._local_map[offset])
        return None

    def _clear_horizontal_ray(self, offset: tuple[int, int, int], reflected_range: int | None) -> bool:
        x, y, z = offset
        if z or (x and y and abs(x) != abs(y)):
            return False
        distance = max(abs(x), abs(y))
        dx, dy = x // distance, y // distance

        def open_terrain(x: int, y: int) -> bool:
            cell = self.cell_at((x, y, 0))
            return cell is not None and not cell.wall and (cell.door is None or cell.door.open)

        body = set()
        if reflected_range is not None:
            if not self.body:
                return False
            # The launch origin may be below the visible body. Include its anchor and all observed body columns.
            body = {(bx, by) for bx, by, _ in self.body} | {(0, 0)}
        for step in range(1, (reflected_range or distance) + 1):
            step_x, step_y = step * dx, step * dy
            if not open_terrain(step_x, step_y) or (step < distance and (step_x, step_y, 0) in self._occupied):
                return False
            if dx and dy:
                sides = open_terrain(step_x - dx, step_y), open_terrain(step_x, step_y - dy)
                if not (all(sides) if reflected_range is not None else any(sides)):
                    return False
            if (step_x, step_y) in body and (step_x - dx, step_y - dy) not in body:
                return False
            if step >= distance:
                if reflected_range is None:
                    return True
                # Even a reversal can close at most one grid step per remaining ray visit.
                separation = min(max(abs(step_x - bx), abs(step_y - by)) for bx, by in body)
                if separation > reflected_range - step:
                    return True
        return False

    @cached_property
    def equipment(self) -> tuple[EquipmentSlot, ...]:
        interactions = self._view.get("interactions", {})
        items = {item.target: item for item in self.inventory}
        worn = {
            offer["equipped_slot"]: items[offer["item"]]
            for offer in self._inventory_offers.values()
            if offer.get("equipped_slot") is not None
        }
        blocked = set(interactions.get("curse_blocked", ()))
        return tuple(
            EquipmentSlot(index, row["id"], row["name"], worn.get(index),
                          index in worn and (worn[index].known_cursed is True or worn[index].target in blocked))
            for index, row in enumerate(interactions.get("slots", ()))
        )

    def equip_choices(self) -> Iterator[Equip]:
        """Pair carried gear with compatible equipment slots, including their current occupants."""
        for item in self.inventory:
            offer = self._inventory_offers.get(item.target, {})
            category = offer.get("slot")
            if category is None or offer.get("equipped_slot") is not None:
                continue
            for slot in self.equipment:
                if slot.kind == category["id"]:
                    yield Equip(item, slot)

    @property
    def tick(self) -> int:
        return int(self._view["tick"])

    @cached_property
    def restorable_losses(self) -> RestorableLosses | None:
        stats = self._view.get("combat", {}).get("own_stats", {})
        losses = stats.get("restorable_losses")
        if losses is None:
            return None
        return RestorableLosses(
            **losses["attributes"],
            hit_dice=int(losses["hit_dice"]),
            maximum_health=losses["maximum_health"],
        )

    @cached_property
    def conditions(self) -> tuple[Condition, ...]:
        stats = self._view.get("combat", {}).get("own_stats", {})
        return tuple(
            Condition(
                row["condition"]["id"],
                row["condition"]["name"],
                row["phase"] == "active",
                int(row["remaining_ticks"]) if row["remaining_ticks"] is not None else None,
            )
            for row in stats.get("conditions", ())
        )

    def has_condition(self, identity: str) -> bool:
        """Whether this picture shows an active, unexpired instance of the condition."""
        return any(c.identity == identity and c.active and c.remaining_ticks != 0 for c in self.conditions)

    @cached_property
    def nutrition(self) -> Nutrition | None:
        """The disclosed food clock of an actor that needs food."""
        stats = self._view.get("combat", {}).get("own_stats", {})
        food = stats.get("nutrition", {})
        if food.get("kind") != "required":
            return None
        return Nutrition(
            food["hunger"], food["reserve"], food["maximum"], int(food["unit_ticks"]), int(food["elapsed"])
        )

    @cached_property
    def holds(self) -> tuple[Hold, ...]:
        rows = self._view.get("linked_holds", ())
        if not rows:
            return ()
        visible = {creature.target: creature for creature in self.creatures}
        return tuple(Hold(row["target"], visible.get(row["holder"])) for row in rows)

    @property
    def hunger(self) -> str | None:
        stats = self._view.get("combat", {}).get("own_stats", {})
        return stats.get("nutrition", {}).get("hunger")

    @property
    def food_reserve(self) -> int | None:
        stats = self._view.get("combat", {}).get("own_stats", {})
        return stats.get("nutrition", {}).get("reserve")

    @property
    def starvation_threshold(self) -> int | None:
        """Reserve at which this actor starves, if disclosed for its food clock."""
        food = self._view.get("combat", {}).get("own_stats", {}).get("nutrition", {})
        return food.get("starve_at") if food.get("kind") == "required" else None

    @cached_property
    def recovery(self) -> Recovery | None:
        stats = self._view.get("combat", {}).get("own_stats", {})
        clock = stats.get("health_recovery")
        if clock is None:
            return None
        return Recovery(
            int(clock["period_ticks"]),
            int(clock["elapsed_ticks"]),
            clock["stopped"],
            clock["reset_percent"],
        )

    @cached_property
    def recent_combat(self) -> CombatReport:
        """The current picture's combat events, including any overlap with earlier pictures."""
        return CombatReport(
            self._view.get("self_target"), tuple(self._view.get("combat", {}).get("events", ()))
        )

    @property
    def self_target(self) -> str:
        return self._view["self_target"]

    @cached_property
    def abilities(self) -> tuple[Ability, ...]:
        stats = self._view.get("combat", {}).get("own_stats", {})
        resources = {row["resource"]["id"]: row["available"] for row in stats.get("resources", ())}
        stock: Counter[str] = Counter()
        for item in self._view.get("inventory", ()):
            if item.get("identified") and (supply := item.get("known_supply")):
                stock[supply["id"]] += int(item["quantity"])
        abilities = []
        for row in stats.get("abilities", ()):
            cost = row["cost"]
            affordable = (
                cost is None
                or resources.get(cost["resource"]["id"], 0) >= cost["start"] + cost["resolution"]
            )
            reach = row["reach"]
            abilities.append(
                Ability(
                    row["id"],
                    row["name"],
                    row["basic_attack"],
                    row["target_kind"],
                    reach["kind"],
                    reach.get("range"),
                    affordable and int(row["cooldown_remaining"]) == 0 and all(
                        stock[cost["supply"]] >= int(cost["quantity"]) for cost in row.get("supply_costs", ())
                    ),
                )
            )
        return tuple(abilities)

    @cached_property
    def basic_attack(self) -> Ability | None:
        return next((ability for ability in self.abilities if ability.basic_attack and ability.ready), None)

    def attack(self, creature: Creature, *, technique: str | None = None) -> UseAbility | None:
        """Propose a basic attack or a ready actor-targeted melee technique by its identity."""
        ability = self.basic_attack
        if technique is not None:
            ability = next((a for a in self.abilities if a.identity == technique and a.ready
                            and a.target_kind == "actor" and a.reach == "melee"), None)
        if ability is None or not any(creature is other for other in self.creatures):
            return None
        if ability.reach == "melee" and not self.in_contact(creature):
            return None
        if ability.range is not None and creature.distance > ability.range:
            return None
        return UseAbility(ability, creature)

    @property
    def available_charts(self) -> tuple[ChartKind, ...]:
        return tuple(
            ChartKind(row["kind"]) for row in self._view.get("charts", ()) if row["chart"] is not None
        )

    def chart(self, kind: ChartKind) -> Chart | None:
        """Read the common client's assembled chart. Positions use each component's frame."""
        if kind.value not in self._chart_pages:
            session, snapshot = self._require_connection()
            self._chart_pages[kind.value] = session.chart(snapshot, kind.value)
        data = self._chart_pages[kind.value]
        if data is None:
            return None
        marks = []
        for row in data["rows"]:
            mark = row["mark"]
            fact = mark.get("fact", mark)
            pos = row["position"]
            normal = fact.get("normal")
            marks.append(
                ChartMark(
                    int(row["component"]),
                    (int(pos[0]), int(pos[1]), int(pos[2])) if pos else None,
                    fact["kind"],
                    tuple(normal) if normal else None,
                    mark.get("name"),
                    mark.get("asset"),
                )
            )
        components = tuple(
            ChartComponent(int(row["component"]), row["alignment"]) for row in data["components"]
        )
        return Chart(kind, data["mode"], int(data["acquired_at"]), components, tuple(marks))

    def _projectile_pages(self, ability: Ability, phase: dict) -> Iterator[dict]:
        offset = 0
        while True:
            key = json.dumps([ability.identity, phase, offset], sort_keys=True, separators=(",", ":"))
            if key not in self._projectile_choices:
                session, snapshot = self._require_connection()
                self._projectile_choices[key] = session.projectile_choices(snapshot, ability.identity, phase, offset)
            page = self._projectile_choices[key]
            if page is None:
                return
            yield from page["choices"]
            successor = page["next_offset"]
            if successor is None:
                return
            if successor <= offset:
                raise ValueError("Projectile choices did not advance")
            offset = successor

    def _aim_offsets(self, creature: Creature, max_distance: int | None = None) -> list[tuple[int, int, int]]:
        visible = (
            cell.offset for cell in self.cells_of(creature)
            if cell.offset != (0, 0, 0) and (max_distance is None or max(map(abs, cell.offset)) <= max_distance)
        )
        return sorted(visible, key=lambda cell: sum(n * n for n in cell))

    def ranged_attacks(self, *, max_distance: int | None = None) -> Iterator[Shot]:
        """Aim at disclosed occupied cells, including visible creatures above or below.

        Each candidate uses the chosen delivery's exact item and socket. The server
        traces the projectile and resolves collisions. Distance is a policy limit.
        """
        targets = [(offset, creature) for creature in self.creatures for offset in self._aim_offsets(creature, max_distance)]
        if not targets:
            return
        for option in self.projectiles():
            for offset, target in targets:
                if option.range is not None and not option.range.reaches(offset):
                    continue
                yield Shot(
                    option.ability,
                    target,
                    offset,
                    option.projectile,
                    option.launcher,
                    option.socket,
                    option.equipped_throw,
                    option.range,
                )

    def projectiles(self) -> Iterator[ProjectileOption]:
        """Query compatible projectile choices even when there are no visible targets."""
        carried = {item.target: item for item in self.inventory}
        for ability in self.abilities:
            if not ability.ready or ability.target_kind != "directional_projectile":
                continue
            for choice in self._projectile_pages(ability, {"kind": "sources"}):
                if choice["kind"] == "launcher":
                    launcher = choice["launcher"]
                    phase = {"kind": "ammunition", "launcher": launcher}
                    for ammo in self._projectile_pages(ability, phase):
                        yield ProjectileOption(
                            ability, carried[ammo["item"]], carried[launcher["item"]], launcher["socket"]
                        )
                elif choice["kind"] == "throw":
                    source = choice["source"]
                    yield ProjectileOption(
                        ability,
                        carried[source["item"]],
                        None,
                        source["socket"],
                        source["kind"] == "equipped",
                    )

    @cached_property
    def ground_items(self) -> tuple[GroundItem, ...]:
        items = []
        for row in self._view.get("ground_items", ()):
            pos = row["position"]
            offset = (pos["x"], pos["y"], pos["z"])
            items.append(GroundItem(_item(row["item"]), offset, row["reachable"], self.cell_at(offset)))
        return tuple(items)

    @cached_property
    def identification(self) -> Identification | None:
        prompt = self._view.get("interactions", {}).get("identification")
        if prompt is None:
            return None
        offered = set(prompt["items"])
        return Identification(
            prompt["choice"], tuple(item for item in self.inventory if item.target in offered)
        )

    @property
    def dead(self) -> bool:
        return self._view.get("combat", {}).get("dead", False)

    @property
    def victorious(self) -> bool:
        combat = self._view.get("combat", {})
        return self.terminal and combat["victory"] and not combat["dead"]

    @cached_property
    def _live_objects(self) -> set[int]:
        # Item targets and creature targets belong to the observation that offered them.
        return {id(obj) for group in (self.inventory, self.creatures, self.abilities) for obj in group}

    def _owns_choice(self, action: Action) -> bool:
        if isinstance(action, Zap):
            return any(action.device is device for device in self.devices) and (
                not isinstance(action.target, Cell) or self.cell_at(action.target.offset) is action.target
            )
        if isinstance(action, Escape):
            return any(action.hold is hold for hold in self.holds)
        if isinstance(action, Unequip):
            return action.slot.occupant is not None and any(action.slot is slot for slot in self.equipment)
        if isinstance(action, Equip):
            return any(action.item is item for item in self.inventory) and any(
                action.slot is slot for slot in self.equipment
            )
        if isinstance(action, UseItem):
            return any(action.item is item for item in self.inventory)
        if isinstance(action, Pickup):
            return any(action.item is ground.item and ground.reachable for ground in self.ground_items)
        if isinstance(action, OpenDoor):
            return any(action.door is cell.door for cell in self.visible_cells)
        if isinstance(action, Identify):
            prompt = self.identification
            return bool(
                prompt
                and (action.prompt is None or action.prompt is prompt)
                and action.choice == prompt.choice
                and (action.item is None or any(action.item is item for item in prompt.items))
            )
        if isinstance(action, (UseAbility, Shot)):
            operands: list[Ability | Creature | Item] = [action.ability]
            if action.target:
                operands.append(action.target)
            if isinstance(action, Shot):
                operands.append(action.projectile)
                if action.launcher:
                    operands.append(action.launcher)
            return all(id(operand) in self._live_objects for operand in operands)
        return True

    def submit(self, action: Action) -> Result:
        result = self._submit(action)
        self._last_submission = result
        return result

    def _submit(self, action: Action) -> Result:
        session, snapshot = self._require_connection()
        if not self._owns_choice(action):
            return Result(Outcome.STALE, None, "Choose an action from this observation")
        if isinstance(action, ActivityChoice):
            change = session.resume if action is ActivityChoice.RESUME else session.cancel
            return Result.from_reply(change(snapshot))
        command: dict[str, object]
        if isinstance(action, Shot):
            if action.launcher:
                delivery = {
                    "kind": "launch",
                    "launcher": {"item": action.launcher.target, "socket": action.socket},
                    "ammunition": action.projectile.target,
                }
            else:
                delivery = {
                    "kind": "throw",
                    "source": {
                        "kind": "equipped" if action.equipped_throw else "carried",
                        "socket": action.socket,
                        "item": action.projectile.target,
                    },
                }
            command = {
                "type": "use_ability",
                "id": action.ability.identity,
                "invocation": {
                    "aim": {"kind": "cell_projectile", "offset": list(action.offset), "delivery": delivery},
                    "weapon": {"kind": "default"},
                },
                "supplies": {"kind": "carried"},
            }
        elif isinstance(action, UseAbility):
            target = action.target.target if action.target else self.self_target
            reply = session.use_ability(snapshot, action.ability.identity, target)
            return Result.from_reply(reply)
        elif isinstance(action, Unequip):
            assert action.slot.occupant is not None  # Validated by _owns_choice.
            command = {"type": "unequip", "item": action.slot.occupant.target}
        elif isinstance(action, Identify):
            command = {
                "type": "identify",
                "choice": action.choice,
                "item": action.item.target if action.item else None,
            }
        else:
            command = _direct_command(action)
        return Result.from_reply(session.act(command, snapshot))


class CombatJournal:
    """Read newly disclosed combat events once per stream, in arrival order."""

    def __init__(self) -> None:
        self._stream = None
        self._sequence = -1

    def read(self, turn: Turn) -> CombatReport:
        if turn._stream != self._stream:
            self._sequence = -1
            self._stream = turn._stream
        events = []
        for batch in turn._combat_updates:
            sequence = int(batch["cursor"]["sequence"])
            if sequence > self._sequence:
                events.extend(batch["events"])
                self._sequence = sequence
        return CombatReport(turn._view.get("self_target"), tuple(events))


@dataclass(frozen=True, slots=True)
class Decision:
    """Current picture, new combat events and the preceding decision's last receipt."""

    turn: Turn
    combat: CombatReport
    previous: Result | None
    stalled: bool = False

    @property
    def previous_phase(self) -> ActivityPhase | None:
        return self.turn.outcome_of(self.previous) if self.previous else None


class ReadinessTimeout(TimeoutError):
    """The game clock stopped advancing before the next decision was ready."""

    def __init__(self, turn: Turn):
        self.turn = turn
        message = f"Player readiness stalled at tick {turn.tick}"
        if turn.server_notice:
            message += f" ({turn.server_notice.code}): {turn.server_notice.message}"
        super().__init__(message)


class Session:
    """An ordinary player connection to a running game."""

    def __init__(self, connection: _native.Session):
        self._connection = connection

    @cached_property
    def training(self) -> Training:
        """Browse the attached actor's point pools and preview purchases."""
        return Training(self._connection)

    @classmethod
    def connect(cls, address: str, token: str, actor: int) -> Session:
        from . import _native

        return cls(_native.Session.connect(address, token, actor))

    def observe(self) -> Turn:
        snapshot = self._connection.observe(diagnostics=False)
        return Turn(snapshot.data(), connection=self._connection, snapshot=snapshot)

    def wait_for_decision(self, *, timeout: float = 20.0) -> Turn:
        """Observe until the actor needs input, the game ends, or progress stalls."""
        deadline = time.monotonic() + timeout
        last_tick = None
        while True:
            turn = self.observe()
            if turn.terminal or turn.can_act or turn.can_resume or turn.needs_identification:
                return turn
            now = time.monotonic()
            if turn.tick != last_tick:
                last_tick = turn.tick
                deadline = now + timeout
            if now >= deadline:
                raise ReadinessTimeout(turn)
            time.sleep(0.05)

    def decisions(self, *, timeout: float = 20.0, include_stalls: bool = False) -> Iterator[Decision]:
        """Yield decisions and the terminal picture. Opt in to inspecting a stalled turn before continuing."""
        journal = CombatJournal()
        previous = None
        while True:
            try:
                turn = self.wait_for_decision(timeout=timeout)
                stalled = False
            except ReadinessTimeout as error:
                if not include_stalls:
                    raise
                turn = error.turn
                stalled = True
            yield Decision(turn, journal.read(turn), previous, stalled)
            previous = turn._last_submission
            if turn.terminal:
                return

    def turns(self, *, timeout: float = 20.0) -> Iterator[Turn]:
        """Yield decisions until the game ends."""
        while True:
            turn = self.wait_for_decision(timeout=timeout)
            if turn.terminal:
                return
            yield turn

    def close(self) -> None:
        self._connection.close()

    def __enter__(self) -> Session:
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        try:
            self.close()
        except Exception as close_error:
            if exc is None:
                raise
            logging.getLogger(__name__).error("Session close also failed: %s", close_error)
