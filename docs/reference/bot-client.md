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
Each cohort runs the archived Python sources in its output
directory and records their SHA-256 hashes in the summary.

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
    if prompt := turn.identification:
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

## Reading the world

- `turn.cell` is the visible cell at the actor's origin, or `None` when it is
  outside the eye scene. Visible cell offsets belong to the current view.
  `FloorMap` remembers links and disclosed traps by stable cell key across
  turns. Its `toward` method prefers a trap-free route when one is known.
  Pass `prefer_safe=False` to prioritize the shortest known route.
  `memory.adjacent(key)` gives known cardinal neighbors. `memory.reachable(key)`
  yields remembered cells and distances in breadth-first order. `memory.items`
  records what the actor last saw at each cell and refreshes on sight.
  Use `turn.walkable_from(cell)` to evaluate a possible next step within the visible scene.
- `turn.creatures` groups visible body cells by creature. Each creature has
  a name, distance to its nearest visible cell, and any disclosed affiliation
  and injury. `turn.hostiles` contains creatures explicitly disclosed as hostile.
- `turn.inventory` combines carried items with their offered uses. For example,
  an unidentified flask can offer `ItemUse.DRINK` while its identity is
  `None`. A policy decides whether to take that risk.
- `turn.step(direction)` examines visible terrain, doors and bodies. It returns
  a proposed move, an action to open a reachable door, or `None`. Reconsider
  movement after opening a door.
- `turn.projectiles()` queries usable throwables and compatible launcher/ammunition
  pairs, including when there are no targets in sight.
- `turn.ranged_attacks()` finds target cells on visible creatures and pairs
  them with the server's projectile choices. Each shot identifies its target,
  projectile, launcher when applicable, and aim offset. The server resolves
  reach, obstructions and hits.

The `turn.health` and `turn.recovery` properties describe current health
and its recovery clock. `turn.restorable_losses` exposes lasting losses
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
fixture.write_text(turn.recording(), encoding="utf-8")
recorded = Turn.replay(fixture.read_text(encoding="utf-8"))
assert choose(recorded) == choose(turn)
```

A recording includes the attached player's disclosed inventory and combat
facts. Chart and ammunition queries need a live connection. The replay
supports local decisions and cannot send commands.

The bundled retrieval policy and its decision loop live in
[`rogue_player.py`](../../scripts/rogue_player.py). The
[findings log](../development/rogue-o-matic-findings.md) records observations
worth investigating on the server.
