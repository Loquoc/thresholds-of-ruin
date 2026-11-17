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

The bot respects the scenario's authored growth and skips owner purchases
when the server reports that advancement is locked. It prints one JSON row
per attempted action and ends with a terminal
reason, deepest reached floor, Amulet acquisition, action count, and final tick.
Each action row includes the tick, player health, and any visible hostiles with
their injury and relative position. Action and terminal rows include the ordered
combat events observed since the previous action in
`events_since_previous_action`. For deaths, the summary also includes combat
events in the final observation and the actor's final nutrition state.
A `no_disclosed_route` report includes the bot's last cell, remembered stairs,
frontier size, rejected moves, and admitted searches for that floor.
Its policy uses the ordinary player view, along with terrain and stairs
it has observed during play.

## Session API

```python
from tor_client_bot import Session

session = Session.connect(address, token, actor=1)
try:
    observation = session.observe()
    picture = observation.data()
    if picture["readiness"]["admission"]:
        result = session.act({"type": "wait"}, observation)
        next_picture = session.observe().data()
finally:
    session.close()
```

`Observation.data()` returns a detached copy of the disclosed state. Use the
original `Observation` to submit an action. The session checks its context
against the current attachment before sending. The server also applies ordinary
control, readiness, and targeting rules.

A confirmed receipt records admission and can precede completion of the in-game
intention. Read the next observation before sending a new command.
`intentions` lists active work. `finished_intentions` contains the latest 32
terminal updates observed on the current stream, in arrival order. Match a
terminal update's `intention` to the admission receipt to tell whether the work
resolved, failed, or was canceled. A stream reset clears the retained updates.
The `combat_updates` list retains up to 16 recent nonvisual combat
event batches from the current stream. Use each batch's cursor sequence
to avoid processing it twice. The latest picture can omit an event that
already arrived on the ordered stream.

When readiness offers a paused intention, pass the observation to
`session.resume(observation)` to continue that work. When readiness permits
cancellation, pass the observation to `session.cancel(observation)`. A stale observation needs a
fresh decision. An unknown result after a
transport failure requires reconnecting and reviewing the new observation before
any further action. The session never retransmits an action on its own.

After a fresh observation, `session.advancement("overview")` opens the controlled
actor's advancement page. Use the commands in the [advancement guide](../play/advancement.md)
to preview a proposal and confirm it through the same method.

See the [capability and acceptance checks](../development/rogue-o-matic-parity.md) for the original bot's tactics and the current verification scope.

Use `session.projectile_choices(observation, ability, phase, offset)` to read
server-owned throw sources, equipped launchers, and compatible ammunition.
Start with `{"kind": "sources"}` and offset `0`. To look up ammunition, pass
`{"kind": "ammunition", "launcher": launcher}` using the exact returned launcher.
Follow `next_offset` until the page has no successor. A stale observation returns
`None`. Submit the selected delivery and direction as an ordinary ability action.

Use `session.chart(observation, "mapping")` to retrieve a complete Mapping
chart. The same method accepts `"monsters"`, `"food"`, and `"magic"`. The
result contains the chart kind, mode, acquisition tick, components, and rows.
The session assembles and validates all pages without spending a turn. It
returns `None` if the chart is unavailable or the observation is stale.
Projected positions are in each chart component's frame. Use ordinary
observations for movement and targeting.
