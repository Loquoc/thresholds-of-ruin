"""Player decisions and command submission over a common-client attachment."""

from __future__ import annotations

from functools import cached_property
from itertools import product
import json
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
    Recovery,
    RestorableLosses,
    Result,
    Shot,
    Trap,
    Unequip,
    UseAbility,
    UseItem,
    Wait,
)

from .training import Training

if TYPE_CHECKING:
    from . import _native


def _item(row: dict, uses: tuple[ItemUse, ...] = ()) -> Item:
    return Item(
        row["id"],
        row["name"],
        row["identity"] if row["identified"] else None,
        int(row["quantity"]),
        uses,
        row.get("class", "misc"),
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
        self._session = connection
        self._snapshot = snapshot
        self._picture = data
        self._view = data["state"]["observation"]
        self._readiness = data["readiness"]
        self._combat_updates = data.get("combat_updates", ())
        self._stream = data.get("stream")
        self._can_resume = data.get("can_resume", False)
        self._can_cancel = data.get("can_cancel", False)

    def recording(self) -> str:
        """Save the disclosed picture for an offline decision test; contains no credentials."""
        return json.dumps(self._picture, separators=(",", ":"))

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
    def finished_activities(self) -> tuple[Activity, ...]:
        """Recent terminal actions retained by the common client."""
        return tuple(Activity.from_row(row) for row in self._picture.get("finished_intentions", ()))

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
    def can_resume(self) -> bool:
        return self._can_resume

    @property
    def can_cancel(self) -> bool:
        return self._can_cancel

    @property
    def needs_identification(self) -> bool:
        return bool(self._view.get("interactions", {}).get("identification"))

    @cached_property
    def health(self) -> Health | None:
        combat = self._view.get("combat")
        if combat is None:
            return None
        return Health(int(combat["hp"]), int(combat["max_hp"]))

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

    def in_contact(self, creature: Creature) -> bool:
        """Whether two observed body cells touch through disclosed open terrain."""
        for source in self.body:
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
    def inventory(self) -> tuple[Item, ...]:
        offers = {offer["item"]: offer for offer in self._view.get("interactions", {}).get("inventory", ())}
        items = []
        for row in self._view["inventory"]:
            offer = offers.get(row["id"], {})
            uses = tuple(
                use
                for use, field in (
                    (ItemUse.EAT, "edible"),
                    (ItemUse.DRINK, "drinkable"),
                    (ItemUse.READ, "readable"),
                )
                if offer.get(field)
            )
            items.append(_item(row, uses))
        return tuple(items)

    @cached_property
    def equipment(self) -> tuple[EquipmentSlot, ...]:
        interactions = self._view.get("interactions", {})
        items = {item.target: item for item in self.inventory}
        worn = {
            offer["equipped_slot"]: items[offer["item"]]
            for offer in interactions.get("inventory", ())
            if offer.get("equipped_slot") is not None
        }
        return tuple(
            EquipmentSlot(index, row["id"], row["name"], worn.get(index))
            for index, row in enumerate(interactions.get("slots", ()))
        )

    def equip_choices(self) -> Iterator[Equip]:
        """Pair carried gear with compatible equipment slots, including their current occupants."""
        interaction = self._view.get("interactions", {})
        offers = {row["item"]: row for row in interaction.get("inventory", ())}
        for item in self.inventory:
            offer = offers.get(item.target, {})
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
                    affordable and int(row["cooldown_remaining"]) == 0,
                )
            )
        return tuple(abilities)

    @cached_property
    def basic_attack(self) -> Ability | None:
        return next((ability for ability in self.abilities if ability.basic_attack and ability.ready), None)

    def attack(self, creature: Creature) -> UseAbility | None:
        """Propose the actor's basic attack against a visible creature in contact."""
        ability = self.basic_attack
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
        session, snapshot = self._require_connection()
        data = session.chart(snapshot, kind.value)
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
        session, snapshot = self._require_connection()
        offset = 0
        while True:
            page = session.projectile_choices(snapshot, ability.identity, phase, offset)
            if page is None:
                return
            yield from page["choices"]
            successor = page["next_offset"]
            if successor is None:
                return
            if successor <= offset:
                raise ValueError("Projectile choices did not advance")
            offset = successor

    def ranged_attacks(self, *, max_distance: int | None = None) -> Iterator[Shot]:
        """Aim at disclosed occupied cells, including visible creatures above or below.

        Each candidate uses the chosen delivery's exact item and socket. The server
        traces the projectile and resolves collisions. Distance is a policy limit.
        """
        targets = []
        for creature in self.creatures:
            visible = (
                cell
                for cell in creature.cells
                if cell != (0, 0, 0)
                and cell in self._local_map
                and (max_distance is None or max(map(abs, cell)) <= max_distance)
            )
            for offset in sorted(visible, key=lambda cell: sum(n * n for n in cell)):
                targets.append((offset, creature))
        if not targets:
            return
        for option in self.projectiles():
            for offset, target in targets:
                yield Shot(
                    option.ability,
                    target,
                    offset,
                    option.projectile,
                    option.launcher,
                    option.socket,
                    option.equipped_throw,
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
            items.append(GroundItem(_item(row["item"]), (pos["x"], pos["y"], pos["z"]), row["reachable"]))
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
        session, snapshot = self._require_connection()
        if not self._owns_choice(action):
            return Result(Outcome.STALE, None, "Choose an action from this observation")
        if isinstance(action, ActivityChoice):
            change = session.resume if action is ActivityChoice.RESUME else session.cancel
            return Result.from_reply(change(snapshot))
        command: dict[str, object]
        if isinstance(action, Move):
            command = {"type": "move", "direction": action.direction.value}
        elif isinstance(action, OpenDoor):
            command = {"type": "set_door", "door": action.door.target, "open": True}
        elif isinstance(action, Shot):
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
        elif isinstance(action, Escape):
            command = {"type": "escape", "hold": action.hold.target}
        elif isinstance(action, Unequip):
            assert action.slot.occupant is not None  # Validated by _owns_choice.
            command = {"type": "unequip", "item": action.slot.occupant.target}
        elif isinstance(action, Equip):
            command = {"type": "equip", "item": action.item.target, "slot": action.slot.index}
        elif isinstance(action, Pickup):
            command = {"type": "take", "item": action.item.target}
        elif isinstance(action, Identify):
            command = {
                "type": "identify",
                "choice": action.choice,
                "item": action.item.target if action.item else None,
            }
        elif isinstance(action, UseItem):
            command = {"type": action.use.value, "item": action.item.target}
        elif isinstance(action, Wait):
            command = {"type": "wait"}
        else:
            raise TypeError(f"Unsupported action {action!r}")
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
                raise TimeoutError("Player readiness stalled")
            time.sleep(0.05)

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
        self.close()
