# Rogue-O-Matic capability and acceptance checks

The [original Rog-O-Matic paper](https://www.cs.princeton.edu/~appel/papers/rogomatic.pdf)
describes its knowledge sources, tactics, and performance. The
[Rog-O-Matic XIV archive](https://github.com/neozeed/Rog-O-Matic) is the historical
behavior reference. The campaign results and capability table below describe the Python policy
before the bot API refactor. The rewritten example uses the
[public Python client](../reference/bot-client.md). Its full-depth fixture
checks retrieval and return against an independent player observation. The
historical seed results below belong to the revisions named in the text.

The [Python bot API implementation plan](../plans/python-bot-api.md) pairs each proposed API improvement with a Rogue-O-Matic verification case and defines the proposed campaign reliability target.

### Verified in the current production player

On development seed 51 the player saw the downstairs, walked to within one
step of it, then retreated across floor 2 when an emu and a hobgoblin came
into contact. At `82f497b2` it stepped onto the stairs and descended. A live
two-opponent encounter also confirmed arrival on the next floor. The change
took seed 51 from floor 2 to floor 4 and seed 52 from floor 2 to floor 3.

At `c2796572`, a paid Search counts toward the player's search budget for
visible walls on the same horizontal plane within three cells, including
diagonals. On seed 51, floor 2 took 145 resolved searches, down from 614 at
`82f497b2`. Its last food reserve on that floor rose from 267 to 933. The bot
died on floor 8. Both policies died on floor 3 on seed 52. Separate live
corridor cases verified six completed searches at a sealed end and opening
a door immediately after an earlier successful Search.

On development seed 49 the bot saw arrows twelve cells west and gold nearby
to the east. Moving west hid the arrows, at which point it went east for
the gold. In the first 800 actions, 213 immediate reversals alternated
between those two loot decisions with no new terrain. At `003da17f` the
outbound player uses its six-step arrow recovery budget for both visible and
remembered arrows. In the same 800-action window the loot reversals fell to
zero and total immediate reversals from 223 to 3. In a live corridor fixture
the player saw both items, walked east and collected the gold. The longer
seed-49 run stopped on floor 3 at action 987 with an unknown outcome and a
save-backlog error on close.

At `4f40aca5` a player carrying the Amulet can recover a disconnected return
route. If there is no known walkable path to the upstairs, it searches a wall
that borders both its reachable area and a remembered passage leading to the
stair. The policy tries a search site up to three times before exploring
elsewhere. In a live fixture with a recorded prior view, the player moved
east toward that wall and completed Search. A separate policy case left a
southern passage unexplored and confirmed the player took it after exhausting
the eastern search budget. Whether the wall hides a door is unknown until
the player discovers one.

At `4626f592` a live encounter placed two hobgoblins within three cells of a
player carrying two recognized haste potions. The player drank one before
contact. The following observation showed Hasted and one dose in inventory.
The player kept that dose while Hasted. The effect on full-campaign survival
still needs measurement.

The production CLI now collects reachable armor and melee weapons, identifies
individual pieces and compares their disclosed properties. Live encounters
checked the final worn armor and primary weapon using a second ordinary
connection. The bot replaced ring mail with clean plate, kept the mail when
the plate was cursed, upgraded a mace to a long sword, and replaced adornment
with searching while retaining protection. Paired decisions on the same map
checked a short outbound armor detour and immediate ascent with the Amulet.

After an unseen attack the player can equip a recognized see-invisible ring
in a free slot. In a live encounter it started with both the ring and a
recognized see-invisible potion. The phantom became visible, the ring was
worn and the potion remained in the pack. A separate run with only the
potion verified that the player drank it and saw the phantom.

The player uses the same ring ranking when deciding whether to leave its
route for a recognized ring and when choosing which finger to put it on.
In an outbound encounter it approached a slow-digestion ring three cells
away, picked it up and wore it. Before the change it chose to explore.
Paired policy checks kept a worn slow-digestion ring ahead of a searching
ring on the floor and skipped a duplicate searching ring. The existing
three-step detour budget applies on the outbound trip.

The bot picks up useful loot within reach on both legs of the trip, including
coins, tools, ammunition without a launcher, spare weapons and clean bandages.
Stock limits apply to longer detours. While exploring outbound it also takes
safe pickups within two moves. Combat and hungry-player decisions run first.

A mapped hallway end with three cardinal walls and one known exit gets six
resolved searches. An opening changes that assessment immediately. The player
can interrupt the search for combat or retrieval, and a failed search leaves
the remaining count unchanged. In a live corridor fixture it collected gold,
resolved six searches from the same cell and then resumed exploration.
In a second live corridor a concealed door initially appeared as a wall.
Search revealed the door before the sixth attempt. The next decision opened
it, and the subsequent player observation showed that same door open.

Two live combat encounters verified pre-contact choices. The bot entered
a narrow passage before two opponents surrounded it, and the opposing
player observed Slowed after a dangerous approach. A paired decision with a
nearby exit used the exit and conserved the charge. In a discretionary-growth
fixture the production CLI gained a hit die, previewed purchases, and learned
Rattling Blow. Bundled Rogue allocates its own growth automatically.

In a separate live encounter the bot had a known Magic Missile wand and a
dangerous opponent two cells away. The original decision was to explore.
After the change the bot aimed along the clear lane, the activation resolved,
and the opposing player's Health fell. Both Magic Missile and Slow use the
public aiming helper to check intervening bodies and terrain. The bot still
gives an available escape stair priority.

Starting on floor 3, a healthy outbound player can try an unread scroll at a
quiet down stair when it carries an unidentified device, ring or potion.
It remembers appearances already tried and skips guarded stairs. A live
test started with a concealed scroll and device, followed the paid selection,
and independently observed the resulting staff identity and charge count.

The stair-retreat test now also covers an exposed approach. When recorded
incoming damage could kill the player in two hits, the bot can step out of
contact before approaching the stair. The repeat production run on seed 47
matched the earlier experiment at floor 15 and 6,269 submissions. The bot
died in melee carrying an unidentified zinc wand whose activation was offered.

Two additional live encounters checked decisions before movement. In a dark
chamber the bot activated its known Light device and subsequently observed
both an illuminated origin and more visible terrain. On a retreat it followed
a remembered route to the stairs, opened a closed door while staying in the
same cell, and saw the door open after the intention resolved. Opening a door
uses the player's current position when estimating exposure to the pursuer.

When an uninjured enemy is approaching and its previously observed damage
could exhaust the player's current Health, the bot can aim a known Polymorph
device. It checks the line of fire and can spend one more measured charge
if it observes that the target resisted the first cast and no other hostile
is in contact. A live test supplied that threat history for a griffin, observed the
charged action resolve, and used the griffin's ordinary player connection to
confirm its new body had fewer hit dice. The scenario can choose different
replacement bodies. The test establishes the decision and this particular
outcome; other encounters still need campaign evidence.

The current Python player uses recognized Fire, Cold and Lightning devices.
It checks the six-step ray budget against the disclosed terrain, diagonal
corners and its own body before firing. Each device damaged a real opponent
without injuring the shooter on an open lane. With a wall four steps ahead,
the player kept its lightning wand even though it could aim directly at the
creature two steps away. Firing that declined shot in the fixture showed
the lightning returning to the caster and costing it Health. Unknown charge
counts still permit one trial per
device. Disclosed empty devices stay in the pack.

For travel, the bot asks its floor map for previously seen stair cells
leading in the desired direction. It applies its remembered danger period
to choices for ordinary travel, emergency retreat and walking while healing.
A live injured player waited in shelter until the stair became eligible again,
then advanced along its visited route.

Terminal reports count distinct items seen, carried, recognized, assessed
and worn, plus admitted item uses and activations. The cohort records observed
supply-detour ticks, reserve decreases and immediate movement reversals
without newly charted terrain. These counts use player observations.

On the rebuilt server identified below, the development cohort with gear
selection, training and pre-contact tactics produced the following results.
The earlier control used the same seeds, server and campaign budgets. Each
entry gives the deepest floor and number of submitted actions. All six runs
ended in death before retrieval.

| Production policy | Seed 42 | Seed 47 | Seed 49 |
| --- | --- | --- | --- |
| Earlier weapon control | 11 / 5,926 | 13 / 3,260 | 6 / 2,965 |
| Gear selection, training and tactics | 8 / 4,322 | 13 / 3,261 | 18 / 9,185 |

The differing outcomes across seeds rule out claiming a general survival
improvement from these runs. A later paired guarded-stair change took seed
42 from floor 8 to 9. The [findings log](rogue-o-matic-findings.md) records
the observed encounters and the separate experimental policies.

### Remaining decisions to verify on bundled Rogue

| Decision | Historical precedent and proposed check |
| --- | --- |
| When is it worth spending a turn to learn what an item does? | XIV's [scroll routine](https://github.com/neozeed/Rog-O-Matic/blob/main/tactics.c#L205-L309) prepared equipment and an identification target before trying an unknown scroll. Give the Python bot an unknown scroll and an unidentified device at a safe outbound stair, then repeat with a hostile in reach and with the Amulet. Record the effect, what became identifiable, and whether the bot subsequently uses that knowledge. |
| What makes a loadout better for this encounter? | XIV [ranked armor, weapons and rings](https://github.com/neozeed/Rog-O-Matic/blob/main/arms.c) and [changed what it wore](https://github.com/neozeed/Rog-O-Matic/blob/main/tactics.c#L16-L71). Current weapon scoring estimates damage per unit time. Compare a fast and a slow weapon at different player skill ranks. For rings, verify both the benefit and the nutrition cost of regeneration with and without spare food. |
| Can the bot reach the escape it chose? | XIV [reasoned about escape routes](https://github.com/neozeed/Rog-O-Matic/blob/main/survival.c). Recreate a wounded player beside a centaur with a stair ahead and a sidestep into safety. Check contact after each resolved move and whether the bot survives to reach the exit. Repeat with the sidestep blocked. |
| Should the player rest here, move while recovering, or leave? | XIV's [stair routine](https://github.com/neozeed/Rog-O-Matic/blob/main/tactics.c#L466-L495) accounted for food. Compare equal injuries with different food reserves on an already mapped, quiet return route. Measure Health recovered, food spent and distance home, then introduce a pursuer. |
| Is another supply detour worth its cost? | Compare picking up a ration on the route, leaving the route for an unknown ring, and revisiting food on the way home. Count the paid moves, change in reserve and any eventual use of the item. Compare descent and ascent from identical positions. |
| Can the player create a ranged attack opportunity? | XIV's [archery search](https://github.com/neozeed/Rog-O-Matic/blob/main/explore.c#L1006-L1106) selected a firing position. Start the bot one move away from a clear shot, then repeat with a blocked escape behind the firing square. Verify the shot, ammunition spent and enemy distance when the player can act again. |
| Where does the escape route lead after the next move? | XIV [marked doors on loops](https://github.com/neozeed/Rog-O-Matic/blob/main/survival.c#L28-L40) for use in retreat. Offer two corridors with equal immediate clearance, one a known dead end. Verify the turn choice, then obstruct the loop and check whether the bot changes course. |

Use isolated encounters to establish the behavior, then paired full campaigns
to measure its cost and benefit. A verified equipment upgrade can still occur
in a losing run.

### What the historical comparison suggests doing next

1. **Account for the cost of finding the exit.** On development seed 49 the
   quiet-scroll control submitted 1,337 searches from 454 positions and made
   824 moves toward search positions. It died on floor 17. Compare searching
   fresh walls first with giving nearby illuminated searches partial credit.
   Record first-descent ticks, remaining nutrition, exit discovery and eventual
   return on several seeds. Repeated searches can reveal a door, so a lower
   search count alone is insufficient. Giving partial credit for nearby
   illuminated searches reduced submissions to 1,031 on seed 49. The bot died
   on floor 15 and fainted 69 times, compared with 13 in the control. The
   experimental rule remains outside the production player.
2. **Make equipment choices respond to the encounter.** XIV's `handlearmor`
   sometimes kept its best suit in the pack to protect it from rust, and
   `ringclass` raised the priority of see-invisible when the player was being
   stalked. The Python player can upgrade and protect armor and can change
   rings. Give it the same inventory twice, changing only the observed threat.
   Check whether the new loadout helps before the cost of changing it exposes
   the player. Use bundled Rogue's effects and timing for this comparison.
3. **Exercise what the player learns.** In scenarios with discretionary
   growth, the bot can buy Rattling Blow or Dislocating Strike. Its melee
   decision now requests Rattling Blow when available. A live encounter with
   a hobgoblin blocking the exit confirmed that a trained player used the
   technique and an untrained player used basic melee. Both attacks resolved.
   Replayable API tests cover insufficient Stamina, supplies, reach and
   cooldown. At `bc60ae9d` an outnumbered player with Dislocating Strike can
   push a target when it sees a clear landing directly behind it. In a pinned
   two-hobgoblin encounter, seed 43 hit the opponent on the downstairs. The
   next observations showed the opponent beyond the exit and the player on
   the stairs, choosing descent. On seed 42 the opponent kept its cell after the strike.
   The seed-43 diagnostic completed the gameplay assertions before the client
   timed out on close. Bundled Rogue assigns its Warrior's advancement through
   an authored skill schedule. The technique requires a build that offers it.
   Using a nearby exit under pursuit also works without the technique.
4. **Plan where combat will happen.** XIV's `archmonster` searched for a
   firing position and `markcycles` marked doors on escape loops. The Python
   player already takes some corridor positions and checks clear device rays.
   Test a one-step setup that yields a shot and leaves a retreat, then close
   that retreat. Check where both creatures stand when control returns.

For supplies, distinguish three opportunities in campaign traces: picking
up an item already in reach, making a short detour, and returning later.
The current wand stock threshold applies to detours. A healthy player can
still pick up another reachable wand. Rations are already pickup targets,
but combat decisions run first and remembered rations can later become forage
targets. Measure how often a later trip could have been avoided with a safe
earlier pickup, including the paid pickup turn and the cost of the detour.

See the [findings and follow-ups](rogue-o-matic-findings.md) for issues uncovered
during implementation and campaign testing.

The implementation baseline used merged main `496de9a8`, its server and
installed wheel, actor 1, fresh bundled saves, a 20,000-action limit and a
900-second timeout per seed. Seed 42 died on floor 8 after 1,512 submissions
(Venomous Bite). Seeds 47 and 49 died from Rattling Blow after 343 and 238
submissions on floors 1 and 2 respectively. None acquired the Amulet. The
enemy-free full-depth fixture independently confirmed victory with the Amulet.
Seed 42 alternated east and west under the same stair-retreat rationale for
its last ten moves. The trace establishes the repetition; it does not record
enough spatial evidence to explain the route choice by itself.

Reproducing seed 42 with its decision memory showed the bot on a down stair
on floor 8 with two adjacent rattlesnakes, one already associated with lasting
injury. It stepped east to seek a distant up stair, then reversed. The
updated policy used the down stair when replayed from that picture. A fresh
campaign still died on floor 8, so the decision fix alone did not improve
maximum depth.

Seed 49's retained picture showed an equipped shortbow, no arrows, a carried
mace and two adjacent enemies. Handling the weapon change while enemies were
visible took seed 49 to floor 4 (1,596 submissions) and seed 47 to floor 7
(2,088). Seed 47 exhausted its disclosed routes after 462 searches on that
floor. Seed 42 still died on floor 8. All three runs used the baseline inputs
and budgets with the changed policy and Python API. Qualification remains open.

The rebuilt server (SHA-256 prefix `864f9811b114`) reproduced all three baseline
deaths at the same depths and action counts. The following development runs
used that binary, the same bundled scenario and the same budgets. Each entry
gives terminal status, deepest floor and submitted actions.

| Policy change | Seed 42 | Seed 47 | Seed 49 |
| --- | --- | --- | --- |
| Baseline control | Death, 8, 1,512 | Death, 1, 343 | Death, 2, 238 |
| Avoid reversing during pursuit and collect supplies | Death, 17, 4,748 | Death, 3, 1,263 | Stair unavailable, 1, 36 |
| Prefer a ready melee attack to throwing a dart in contact | Unscheduled | Death, 17, 7,537 | Unscheduled |
| Also wait for levitation to expire at the stair | Interrupted | Death, 17, 7,537 | Readiness timeout, 11, 5,310 |

On seed 49 the bot waited until ground contact returned and then descended.
Its final player observation did not establish a death. The seed-47 death on
floor 17 followed two applied melee strikes. None of these runs acquired the
Amulet. Seeds 71001–71020 remain reserved for the 16-of-20 qualification.

A fresh diagnostic on seed 47 reproduced the floor-17 death at tick 780440.
With a medusa adjacent, recovery from 34 to 35 of 69 Health made the bot
turn and attack. Two player attacks later its Health was 13. Continuing a
wounded retreat until ordinary recovery Health changed the route earlier in
the game. That candidate died on floor 10 after 2,544 submissions. It carried
a food ration while retreating from a centaur at a reserve of −7. The next
observation recorded Fainted, multiple attacks and death. The policy now
considers a carried meal during a pursuit when weak or empty. A live test
confirms meal admission and a reserve increase with an adjacent hostile.

Eating the ration extended seed 47 from 2,544 to 5,162 submissions; both
runs died on floor 10. The bot spent over a thousand decisions retreating
on that floor. Seeking the remembered upstairs during pursuit brought it
to floor 15 (death in melee). A separate seed-42 supply experiment
exposed a pickup loop on floor 8: walking south revealed a reachable
smoky potion and an ice monster two steps away. The bot walked north,
lost sight of the monster and sought the potion again. On replay of
the recorded approach a healthy bot took the potion, and a regression
checks that it remembers which nearby scroll the monster guards.

The current acceptance target is the bundled `rogue` scenario from current main.
The capability table combines live fixtures, earlier `rogue-exploration`
runs, and bundled `rogue` results. The last column lists the remaining
campaign checks.

In an 800-action check of the public-API rewrite on the `517a7812` server,
bundled seeds 42 and 47 reached floors 8 and 7 respectively. Both stopped at
the action budget. Before the client combined door reachability across
visible cells, both had spent the same budget on floor 1 repeatedly walking
east and west. The bounded check exercises navigation, combat and condition
disclosure. Bundled victory remains unverified for the rewrite.

After updating the binding and policy to main at `5192b143`, another
800-action run on seed 42 reached floor 8. Seed 47 died from injury on
floor 1 after 343 actions. Its final events show Rattling Blow applying
immediately before death. These single runs establish that the installed
wheel can play against this server. They do not measure a win rate.

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
   transitions. Seeds 42, 47 and 49 are repeatable development cases. Seeds
   45 and 46 were used in earlier evaluations. The current qualification
   requires 16 independently verified wins on fresh seeds 71001 through 71020.

## Reproduce campaigns

With the binding installed in Python 3.11 or newer, pass the built server and a new output
directory to the cohort runner. For example, run the development cohort together.
Use a separate output directory and a frozen candidate for final qualification.

```sh
python scripts/rogue_o_matic_cohort.py --server path/to/tor-server --output path/to/development-output 42 47 49
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
