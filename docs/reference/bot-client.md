# Python bot client

Build and install the local bot client in a Python 3.10 or newer virtual environment.

```sh
python -m pip install ./crates/client-bot
```

Start the bundled Rogue scenario with an ordinary player token. Use a fresh save
and select actor 1 in the bundled `rogue` scenario. The server launch steps are in the
[Rogue play guide](../play/rogue.md).

With the same player token in `TOR_SERVER_TOKEN`, run the example from the
repository root. For a custom retrieval scenario, pass `--retrieval-depth`
to set when the bot switches from seeking stairs to searching for the
objective. Pass `--objective-identity` to select the canonical item identity
the server discloses when the item is known. The default is
`item.amulet.amulet-of-yendor`. The cohort runner reads the depth from the
scenario manifest and accepts the same identity option for independent
victory verification.
The cohort runner requires Python 3.11 or newer for manifest parsing.
Each cohort runs the archived bot policy under `policy/` in its output
directory. The installed Python client modules and native extension are
copied beside it under `policy/tor_client_bot/`. The summary records
per-file SHA-256 hashes in `policy_sources_sha256` and `bot_bindings_sha256`
so two runs can be compared even when the installed client has changed.

```sh
python scripts/rogue_o_matic.py --connect 127.0.0.1:4000 --actor 1 --budget 20000
```

## First policy

A session attaches to one player. Each turn is a snapshot of what that player
can perceive, with the common client's attachment context retained for submission.
Creating a choice spends nothing. `turn.submit(choice)` sends one command.

```python
import os
from tor_client_bot import ActivityChoice, Direction, Move, Outcome, Session, Wait


def choose(turn):
    if turn.needs_identification and (prompt := turn.identification):
        return prompt.select(next(iter(prompt.items), None))
    if turn.can_resume:
        return ActivityChoice.RESUME
    if turn.cell and turn.cell.stairs_down:
        return Move(Direction.DOWN)
    return turn.step(Direction.EAST) or Wait()


with Session.connect("127.0.0.1:4000", os.environ["TOR_SERVER_TOKEN"], actor=1) as game:
    for turn in game.turns():
        result = turn.submit(choose(turn))
        if result.status is Outcome.STALE:
            continue
        if result.status is not Outcome.CONFIRMED:
            print(result.status.value, result.reason)
            break
```

`game.turns()` polls for player decisions and ends at a terminal observation.
Use `game.wait_for_decision()` when the policy also needs to inspect the final
turn. `game.observe()` returns a snapshot even while the actor is busy.
If the clock stops advancing while the client waits, `ReadinessTimeout.turn`
retains the last observation. For example, a bot can record
`error.turn.tick` and `error.turn.outcome_of(receipt)` before ending a
stalled run. The exception is a subclass of `TimeoutError`.
With `game.decisions(include_stalls=True)`, the iterator yields a
`decision.stalled` picture on timeout. Its `previous` and `previous_phase`
let the policy check whether its own action is still queued. Submitting a
permitted cancellation from that picture keeps the same decision stream.

For a campaign trace, iterate over `game.decisions()`. Each decision
includes the current `turn`, newly witnessed `combat` events, the preceding
decision's last submission receipt (`previous`), and its latest observed
`previous_phase`. The iterator also yields the terminal picture.
`describe_action(action)` gives a structured description suitable for logging.

### Following a stair crossing

Retain the submitted move's evidence until the next decision. The following
example assumes the actor is standing on a disclosed down stair.

```python
from tor_client_bot import Direction, Move

move = Move(Direction.DOWN)
receipt = turn.submit(move)
attempt = turn.stair_attempt(move, receipt)
after = game.wait_for_decision()
arrived = after.stair_arrival(attempt)
```

An arrival of `True` means the subsequent picture supports switching maps.
`False` reports a refusal or observed failure. `None` leaves arrival uncertain.
A policy can use `after.outcome_of(receipt)` to inspect the latest retained phase.
For a failed action, the matching entry in `after.finished_activities` can
also carry a `failure` such as `unfunded` or `cooling_down`. The server
discloses these reasons to the controller when execution fails. A missing
reason leaves the cause unknown.

If a submission returns `Outcome.UNKNOWN`, keep its `request_id` when
available. It identifies the outgoing request and can be matched to a durable
journal receipt. Admission and execution are still uncertain, so reconnect
and inspect the actor's current state before deciding whether to act again.

When a queued action stops progressing, inspect `turn.server_notice` for a
server-reported code and message. A readiness timeout includes the current
notice when available. Keep the action receipt as well, since a notice about
the session can accompany an action belonging to another actor.

When the origin disclosed a landing, its stable key ties the subsequent arrival
to the expected place, including an adjacent landing beside an occupied stair.
The origin and landing can have the same simulation tick. Match the move receipt
and check the actor's cell in the subsequent decision before switching maps.
A prompt can likewise remain visible while its answer is processing. Use
`game.wait_for_decision()` before choosing another answer. An accepted answer
closes admission before the next picture necessarily replaces the prompt.

## Reading the world

- `turn.cell` is the visible cell at the actor's origin, or `None` when it is
  outside the eye scene. Visible cell offsets belong to the current view.
  `FloorMap` remembers links and disclosed traps by stable cell key across
  turns. Its `toward` method prefers a trap-free route when one is known.
  To head for a remembered exit, use `memory.toward(turn, memory.stairs(Direction.UP))`.
  The keys come from the latest sighting of each cell and persist when it leaves view.
  Pass `prefer_safe=False` to prioritize the shortest known route.
  Pass `avoid={cell.key}` to exclude a cell from every candidate route.
  Use `memory.toward_unexplored(turn, avoid=dangerous_cells)` to explore.
  It prefers an unvisited cell bordering unknown terrain, then any reachable
  unvisited floor cell. It returns a move, a door-opening action, or `None`
  when the remembered map offers no such route outside the excluded cells.
  `memory.adjacent(key)` gives known cardinal neighbors. `memory.reachable(key)`
  yields remembered cells and distances in breadth-first order. `memory.items`
  records what the actor last saw at each cell and refreshes on sight.
  `memory.is_dead_end(key)` identifies a floor cell with three observed cardinal
  walls and one known passage. It returns false while a side is unknown and
  updates when a search reveals a previously concealed door.
  Use `turn.walkable_from(cell)` to evaluate a possible next step within the visible scene.
- `turn.creatures` groups visible body cells by creature. Each creature has
  a name, distance to its nearest visible cell, and any disclosed affiliation
  and injury. `turn.hostiles` contains creatures explicitly disclosed as hostile.
- `ground.cell` associates a ground item with its visible cell and is `None`
  when that cell is outside the picture. For example, a bot can route toward
  `ground.cell.key` until `ground.reachable` offers a pickup.
- `turn.distance_to(foe, from_offset=direction.offset)` returns the shortest
  Chebyshev distance between the visible bodies after translating the observer.
  Use `turn.walkable_directions` to choose candidate steps before scoring them.
  When `turn.falling` is true, Rogue-O-Matic waits until it lands before choosing a step.
  In Rogue, a Move while caught in a bear trap spends one turn attempting to
  break free in place. Even a blocked direction works for this attempt. Waiting
  does not release the trap. The actor can still choose available melee actions.
- `turn.inventory` combines carried items with their offered uses. For example,
  an unidentified flask can offer `ItemUse.DRINK` while its identity is
  `None`. A policy decides whether to take that risk.
  `item.appearance` preserves the disclosed appearance when a policy wants
  to remember an unidentified flask across item stacks.
  A recognized identification scroll exposes its eligible categories in
  `item.identifies` before it is read. The ring-or-wand scroll lists both even
  when the inventory contains only one of those categories.
- `turn.has_condition(identity)` checks for an active, unexpired condition.
  Rogue-O-Matic uses it to wait at a stair while levitation prevents descent.
- `turn.equip_choices()` pairs carried equipment with slots of the offered
  kind. A shortbow and an empty weapon socket can still need a second free
  grip. Inspect the submission result for the server's decision, including
  its disclosed rejection code and message.
- `turn.step(direction)` examines visible terrain, doors and bodies. It returns
  a proposed move, an action to open a reachable door, or `None`. Reconsider
  movement after opening a door. A multi-cell door can offer an approach at
  only one of its cells. When grouping cells by door ID, keep the approaches
  from every visible cell so an unreachable upper cell cannot hide a reachable
  lower one.
- `turn.projectiles()` queries usable throwables and compatible launcher/ammunition
  pairs, including when there are no targets in sight.
- `turn.attack(foe)` proposes the basic attack. Pass an identity such as
  `turn.attack(foe, technique="rattling_blow")` to request a particular melee
  technique. It returns `None` if the technique is unavailable, its target
  is outside contact, or the creature belongs to an earlier observation.
  Readiness accounts for cooldown, resource costs and identified carried supplies.
- `turn.ranged_attacks()` finds target cells on visible creatures and pairs
  them with the server's projectile choices. Each shot identifies its target,
  projectile, launcher when applicable, and aim offset. The server resolves
  obstructions and hits. When disclosed, `shot.range` gives short and maximum
  reach in cells for that delivery. An offset is in range when its largest
  absolute component, including height, is at most `long_cells`.
  `ranged_attacks(max_distance=n)` also applies the caller's distance limit.

### Comparing armor

`turn.equip_choices()` pairs each carried candidate with a compatible slot and
its current occupant. For armor, `item.physical_reduction` is the lesser of
the disclosed unconditional reductions against impact and keen damage. A suit
that reduces each by six reports six. An unidentified instance reports
`None` even when the player recognizes the kind of armor. Zero is a known
value. `item.known_cursed` also preserves undisclosed knowledge as `None`.
`slot.removal_blocked` records a known curse or a reported block on removal.
Compare the candidate with the occupant before spending an action to unequip.

`item.weapon_attack` gives the disclosed skill, accuracy bonus, mean damage
before defenses, wind-up ticks and recovery ticks. The bot can weigh sustained
damage against the cost of committing to a slow strike. `equipment_identified`
records whether instance statistics were disclosed even for gear without a
weapon attack. `Result.stale` covers a choice refused locally or returned by
the server with a stale input context.

### Activating a device

An inventory item and its offered device activation share the same live
item object. `turn.devices` gives the offered aim mode and the disclosed
remaining charges. A count of `None` means the player has no measurement.
For example, `turn.aim_device(device, foe)` proposes aiming at the nearest
visible cell of that creature. Use `Zap(device, cell)` to choose a specific
bearing. A direction also supplies
a bearing. Pass `None` explicitly for an offered self activation. A fresh
turn requires a fresh device and cell. Admission and a resolved phase can
both precede an observed dodge, so check the enemy's next position.

For an elemental device in bundled Rogue, use
`turn.aim_device(device, foe, reflected_ray_range=6)`. The six is the
scenario's ray range. The helper requires enough visible open terrain that
a reflection could no longer return to the shooter's observed body before
the ray runs out of steps. This includes checking both sides of diagonal
corners and the terrain beyond a nearby target. It returns `None` when
the disclosed scene cannot establish that clearance. Creatures and doors
can still move between choosing the shot and its release.

For a first-contact device such as Slow or Magic Missile, use
`turn.aim_device(device, foe, clear_horizontal_ray=True)` to request a
visible cardinal or diagonal lane. The helper tries each visible cell of
the creature, nearest first, and returns `None` when every lane is off-axis,
unknown or obstructed. Each diagonal step needs one visibly open side.
The check describes the current picture. Movement during preparation, the
device's reach, and effects beyond the target still depend on the server.

### Two spatial decisions

`cell.lit` reports the server's current illumination of that cell, including
when the observer can sense their own position in darkness. Older recordings
that lack the field return `None`. A bot can use this when deciding whether
to activate a known light device, then check the same cell after the action
resolves.

A collector can pursue the observed position of an item and use the pickup
offer when it becomes reachable. The route helper can return a door-opening
action as the next step.

```python
from tor_client_bot import Move, Pickup

def collect(turn, memory, ground):
    if ground.reachable:
        return Pickup(ground.item)
    if ground.cell is not None:
        return memory.toward(turn, {ground.cell.key}, max_steps=3)
    return None


def retreat(turn, foe):
    def clearance(direction):
        return turn.distance_to(foe, from_offset=direction.offset)

    step = max(turn.walkable_directions, key=clearance, default=None)
    if step is not None and clearance(step) > turn.distance_to(foe):
        return Move(step)
    return None
```

The collector allows up to three horizontal steps on the remembered map.
Opening a door may take an extra action. Omitting `max_steps` permits a
route of any known length. The retreat compares the nearest observed cells of both bodies. It leaves
the decision to stand and fight to the caller when every known step keeps
the foe as close or moves closer. Against several enemies, count
`turn.in_contact(foe, from_offset=step.offset)` for each candidate direction
to find a step that leaves fewer opponents in melee reach.

The `turn.health` and `turn.recovery` properties describe current health
and its recovery clock. For an injured player, `turn.recovery.ticks_to_regain(2)`
estimates how many active ticks two Health points would take at the current
rate, including progress already earned. A stopped clock gives `None` for a
positive amount. Fresh damage can change the estimate.
`turn.restorable_losses` exposes lasting losses
such as strength, hit dice, or maximum health. For instance, a policy
can save a restore-strength potion until `turn.restorable_losses.strength`
is positive. These properties return `None` when the player lacks the
corresponding disclosure.

## Following an action

A confirmed submission may complete immediately or start an activity. For an
admitted action, `result.activity.phase` gives the phase at admission. On a
later observation, `turn.outcome_of(result)` returns its latest disclosed
`ActivityPhase`, such as `PAUSED`, `RESOLVED` or `FAILED`. It returns `None`
if there is no matching activity in the current picture or retained history.
A resolved attack can still miss. A stale submission calls for a fresh
observation. If a disconnect leaves the outcome unknown, inspect the player
after reconnecting before deciding whether to repeat the action.

## Interpreting combat

Each entry in `turn.recent_combat.incoming` describes an ability directed at the
player by another or unidentified actor. It carries the observed outcome and any
disclosed caster, ability identity and strike number. For an unseen attacker,
an author can respond to a miss or a drop in health even when the caster and
ability are unknown. A policy can save a `CombatJournal()` and call
`journal.read(turn).incoming` each time it observes to consume each streamed
event once. `report.records()` preserves the disclosed events for a trace.

Use `report.outgoing` to see what happened to the targets of your own abilities,
including self-targeted abilities. Each entry gives the disclosed target handle,
ability ID, outcome, and strike index. Undisclosed fields are `None`. For example,
a wand action can resolve even when the observed outcome is `AbilityOutcome.UNAFFECTED`.
Use the journal when each effect should influence the bot's memory only once.

## Character advancement

`game.training.overview()` discloses whether the scenario locks advancement
and how many points remain. For an unlocked build, browse `training.pools()`
and `training.options(TrainingKind.TALENT)` on the same `training = game.training`
object. Each pool lists the kinds of purchase it can fund. An option lists
its required form choices when applicable. Call `training.preview(pool, option)`
with an optional `form=` selection, inspect the resulting health and
`would_die`, then pass that preview to `training.confirm(preview)`. Browsing
again invalidates the preview. A second confirmation returns `Outcome.STALE`.

## Testing a decision offline

A policy can read health, memory, visible creatures and items from a saved
picture. Save a recording from a live turn while investigating a decision.

```python
from pathlib import Path
from tor_client_bot import Turn

fixture = Path("at-the-stairs.json")
chosen = choose(turn)
fixture.write_text(turn.recording(), encoding="utf-8")
recorded = Turn.replay(fixture.read_text(encoding="utf-8"))
assert choose(recorded) == chosen
```

Record the turn after choosing an action to include the chart and projectile
pages consulted by the policy. Replaying that recording can make the same
queries offline. A new query that requires an unrecorded page needs a live
connection. Store the policy's starting memory separately for a reproducible
decision. To retain those inputs in a development campaign, pass
`--record-decisions` to the cohort runner. Each JSON trace row then carries
its turn recording. Rebuild the policy's memory from its starting configuration
by replaying observations, choices and recorded admission receipts in order.
Ordinary campaigns omit these larger payloads.

The bundled retrieval policy and its decision loop live in
[`rogue_player.py`](../../scripts/rogue_player.py). The
[findings log](../development/rogue-o-matic-findings.md) records observations
worth investigating on the server.
