# Rogue-O-Matic capability and acceptance checks

The [original Rog-O-Matic paper](https://www.cs.princeton.edu/~appel/papers/rogomatic.pdf)
describes its knowledge sources, tactics, and performance. The
[Rog-O-Matic XIV archive](https://github.com/neozeed/Rog-O-Matic) is the historical
behavior reference. The Python example implements these decisions using TOR's
ordinary player disclosures and current scenario rules.

See the [findings and follow-ups](rogue-o-matic-findings.md) for issues uncovered
during implementation and campaign testing.

The current acceptance target is the bundled `rogue` scenario from current main.
The capability table combines live fixtures, earlier `rogue-exploration`
runs, and bundled `rogue` results. The last column lists the remaining
campaign checks.

On main at `5b9299c4`, the bot binding and server built together and
the ordinary live startup test passed on `rogue`, including 20
policy action attempts under authored growth. The first full seed-42 campaign reached
floor 8 and died to a rattlesnake's Venomous Bite after 420 action attempts.

On `b28c8e90`, a pinned bundled seed-49 run retrieved the Amulet on
floor 26 and returned to floor 2. Starvation ended the run before
the surface exit. The full-depth enemy-free fixture independently
verified victory with the carried Amulet.

On `e10b9a23`, the deepest bundled seed-42 run reached floor 21.
Bundled seed 49 acquired the Amulet and died in melee on floor 25
during the ascent. The repeat attempts reached the same terminal tick.
A bundled victory remains open.

| Capability | Current implementation and evidence | Remaining acceptance |
| --- | --- | --- |
| Terrain and connectivity | Floor graph remembers disclosed cell keys. Tests cover frontiers, stairs, routes weighted by trap presence, and repeat searches grouped by proximity after an initial sweep. Live tests verify a floor transition, opening a discovered hidden entrance, room illumination, and resuming movement after falling. At the search stage the policy can read a known Mapping scroll on a supported floor. It uses charted secret doors to prioritize already observed walls when the wall faces yield a unique alignment. | Prove stable descent and return across the complete bundled dungeon. |
| Object and inventory tracking | Policy distinguishes known and unknown items, chooses accessible loot, bounds optional pickups, and tests revisiting a last-seen Amulet location. | Exercise retained object knowledge over a full run. |
| Pursuit, melee, and retreat | Policy recognizes disclosed diagonal contacts, counts each hostile once regardless of body size, and prioritizes wounded foes when it stays to fight. It uses safer routes toward stairs or unexplored cells and learns from observed damage. Live tests verify diagonal attacks, exits from melee, multiple body cells belonging to one hostile, closing a doorway during retreat, and creating space with Dislocating Strike. | Prove sustained combat and recovery across deeper floors with ordinary advancements. |
| Missile fire | Policy uses disclosed ranged spells, known charged wands, and server-offered physical projectiles. Live Python clients verify spear impact and recovery, and the policy selects compatible ammunition and hits a target five tiles away along a diagonal with an equipped shortbow. | Exercise ranged choices across complete runs. |
| Weapon and armor decisions | Policy compares disclosed combat stats, identifies gear whose name is known but whose stats are missing, and clears disclosed curses on worn gear. Live tests verify equipping plate mail over leather armor and removing armor after lifting its curse. | Exercise weapon and armor choices against representative opponents across complete runs. |
| Magic and identification | Policy waits for an accepted identification answer to settle, tests unknown consumables under favorable conditions, and uses healing and teleportation to survive emergencies. Live fixtures verify wand identification, an empty identification prompt answered once, and teleportation out of two holds. | Exercise charged-device choice across complete runs. |
| Health and food | Policy waits for safe recovery, uses finite bandages and known rations, and restarts decisions after an interrupted intention. | Verify food and recovery remain viable across descent and ascent. |
| Amulet and victory | The current bot descended from floor 1 to floor 26, retrieved the Amulet, and returned to the surface on an enemy-free fixture derived from bundled `rogue`. The real server and a fresh ordinary player connection confirmed victory and the carried Amulet. | Repeat the complete run on the bundled scenario with its ordinary enemies and nutrition. |

The final run record supplies action count, deepest observed stair depth, Amulet
acquisition, terminal status, final tick, and available combat events. Keep the
acceptance seed and a separately fixed held-out cohort when evaluating a policy
revision. Report victories, depth, acquisition, actions, and attributable deaths.
The historical Rogue score measures a different ruleset. Use the TOR cohort
results as the baseline for comparing Python policies under identical scenario
inputs.

In bundled seed 43, the run with nearby repeat searches left floor 9 after
595 search-related moves and 843 paid searches. The earlier run used 1,313
search-related moves and 779 paid searches on that floor. These are two
calibration runs.

The first held-out run, seed 45, stopped at floor 16 after 11,878 actions.
The server rejected all cardinal moves from the same cell. On a copy of that
save, a confirmed wait displaced the actor and movement resumed. The policy now
waits and retries after all disclosed local exits reject movement. The resumed
save is a diagnostic run. Fresh bundled-scenario victory and final held-out
campaign results are still pending.

A later seed-45 run exposed a lethal reflected lightning shot. The bot now
checks disclosed terrain and corners far enough beyond the firing position to
rule out a reflection returning within the six-step elemental ray budget. It
applies the same check before testing an unidentified directional device.

A floor-19 replay also showed a disclosed two-step route out of vampire melee.
The policy now considers such a route during critical health or contact with
known dangerous foes. Both cells must be open, and the first step must retain
or reduce the set of melee contacts.


## Policy design

The original system kept a shared model of terrain, objects, inventory, and
combat state. Its ordered decision modules used that model, with weighted
shortest paths and corridor connectivity available to exploration and retreat.
Its combat rules estimated how soon a threat could kill the player before
choosing whether to fight, use a device, or retreat. Use the
[original paper, section 3](https://www.cs.princeton.edu/~appel/papers/rogomatic.pdf)
and the archived [strategy](https://github.com/neozeed/Rog-O-Matic/blob/main/strategy.c),
[tactics](https://github.com/neozeed/Rog-O-Matic/blob/main/tactics.c), and
[retreat](https://github.com/neozeed/Rog-O-Matic/blob/main/survival.c) sources
as references when revising those decisions.

The Python policy already has a floor graph and learned damage estimates.
A known cancellation device can suppress a dangerous special attack. The live vampire test verifies that Drain Vitality disappears while basic melee remains, adapting the original [cancellation rule](https://github.com/neozeed/Rog-O-Matic/blob/main/strategy.c#L779-L792). Healing and paused attack cancellation now compare the combined damage margin of distinct foes in melee reach, with a separate allowance for observed multi-strike attacks. Several action priorities still live inside one decision function. Move the
shared estimates and route comparisons into the decisions that need them, using
the existing ordinary player disclosures and action interfaces.

1. Combat and retreat should compare the time to take another damaging hit
   with the time to break contact. Derive escape candidates from the known
   route to stairs, a defensible doorway, or a loop. Re-evaluate when an
   observation changes the threat or blocks the route. Cover the two-griffin
   encounter and the black-unicorn retreat with the same decision rules.
2. Food, recovery, and exploration should share an estimate of how many
   paid turns the remaining food affords. Collect visible food during safe
   travel, use last-seen food when needed, and let those estimates govern how
   long to search or whether to take an available stair. The XIV
   [stair routine](https://github.com/neozeed/Rog-O-Matic/blob/main/tactics.c#L466-L495)
   conditions resting on the amount of food carried. The Python policy uses
   the disclosed recovery period and nutrition clock to estimate whether a
   rest without rations would reach its Health target before the reserve is
   empty. It takes an available safe stair when the rest exceeds that budget.
   The estimate uses elapsed time and the current clock rates. Regeneration
   and other effects can also consume nutrition.
   Before and after retrieval, the bot also compares its reserve with the
   cost of the mapped return legs. If the known cost exceeds the reserve
   and there are no edible supplies, the bot can detour to remembered food
   on the current floor.
   A separate pre-descent check estimates the remaining trip using the
   median of recent floor travel times and tracks how much reserve each
   observed food identity restored. When those supplies fall short, the
   bot can spend a bounded part of its reserve scouting a reachable
   frontier from a down stair. It ends that scout when a newly
   observed food pickup increases its carried supply.
3. Compare policy changes on fixed initial inputs. Use focused cases to test
   decision tradeoffs and complete campaigns to measure victories, Amulet
   acquisition, survival depth, paid searches, and nutrition at floor
   transitions. Keep seed 42 as the acceptance run and 45 and 46 as held-out
   runs.

## Reproduce campaigns

With the binding installed in Python 3.11 or newer, pass the built server and a new output
directory to the cohort runner. Keep the acceptance seed and held-out seeds in
separate directories.

```sh
python scripts/rogue_o_matic_cohort.py --server path/to/tor-server --output path/to/acceptance-output 42
python scripts/rogue_o_matic_cohort.py --server path/to/tor-server --output path/to/heldout-output 45 46
```

Each directory gets a summary and each seed retains its own turn trace, server
logs, and fresh save. The summary records the actor, the server path and binary
hash, and the scenario path and manifest hash. It also hashes the scenario rules
and validation certificate when present, along with the policy sources, at
startup. The summary is present before the first seed launches, so an
interrupted run still has its input hashes and planned seeds. Completed
seed results are appended to the summary as each seed finishes.
It records admitted searches and moves per floor,
plus the first, last, and lowest observed nutrition reserves. Floor transitions
include the reserves from the observations immediately before and after the
change. Bot terminal reports also include the ascent estimate by floor
and its sum. A floor appears once the bot confirms a descent and has a
remembered route back to its up stairs. The timing uses the stair
traversal as a proxy for each move. Admission counts come from
confirmed receipts and can include an
intention that was later interrupted. The summary counts resolved and failed
Search intentions separately, matched to their admission floor. An admission
with no recorded terminal phase contributes only to the admitted count. A fresh player connection verifies terminal victory and
the carried Amulet before the runner counts a win. A wall timeout or client
failure is reported separately
from an ordinary in-game terminal result.
Action rows retain whether the bot has observed the retrieval item in its
inventory. This lets an interrupted run retain retrieval evidence even
without a terminal row.
Action and terminal rows include `events_since_previous_action`.
This field records new events from the binding's bounded, ordered
combat history that the bot observed during its wait for the next
action. Action rows also retain the current observation's `events`.

## Test Python policy edits

Run the policy and cohort tests with `python -m unittest discover -s scripts -p 'test_rogue_o_matic*.py'`.
For live tests against existing builds, set `TOR_TEST_BINARIES` to the directory
with the server, headless client, and scenario validator. Set
`TOR_TEST_BOT_EXTENSION_DIR` to the directory with the built Python-enabled
`tor_client_bot.dll` on Windows or `libtor_client_bot.so` on Linux. Then run
`python -m unittest discover -s scripts -p test_rogue_bot_process.py`.
