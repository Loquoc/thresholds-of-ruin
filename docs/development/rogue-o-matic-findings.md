# Rogue-O-Matic findings and follow-ups

The production player picked up and identified plate mail in a live encounter.
An independent player connection confirmed which armor it wore afterward.
The clean plate replaced ring mail. A cursed plate with higher disclosed
reduction stayed in the pack. During the identification exchange the ordinary
server sometimes returned `rejected` with code `stale_context` after a prompt
had advanced. Bots can now use `Result.stale` to refresh and choose again for
both this server reply and a locally expired turn.

Bundled Rogue uses authored growth. Its scheduled skill ranks apply on earning
hit dice and the player has no purchase decision. Discretionary training tests
belong to scenarios that offer it, such as Rogue Exploration.

In the seed-47 development campaign, the player carried three recognized
armor instances and obtained exact protection for none. Recognition and
instance identification are separate. An armor trial can close that gap.
The client first observed an unmeasured plate, and after equipping it a
second connection observed six physical reduction with its curse still
undisclosed. The policy uses the public base armor values and worst published
birth penalty when it lacks an identification or remove-curse scroll.

On closer examination, the spare suits on seed 47 were leather and studded
leather. Keeping the starting ring mail was reasonable. A more consequential
choice came at 9 Health, after a centaur had dealt 16 damage. The bot moved
toward the stairs while remaining in contact. An eastward or northeastward
step would have taken it out of immediate reach. A route to a stair should
account for the hits the player can survive on the way.

Seed 49 reached floor 18 carrying three unidentified devices, three unknown
rings and five scrolls without an admitted scroll use. Counting pickups alone
hid an identification bottleneck. Test safe opportunities to read scrolls,
record what the player learns from each one, and compare the resulting
device and ring use on the same seed.

Seed 42 twice escaped an adjacent rattlesnake upstairs and promptly returned
to its floor. The bot now remembers the return direction for a short time
after observing arrival at the refuge. A two-floor live encounter verifies
that its next action resolves without taking it back to the guarded stair.
In the paired seed-42 runs that change advanced the deepest floor from 8 to
9 (4,322 and 4,193 submissions respectively). On the later run, the bot fled
a quagga on floor 9 and arrived back on floor 8 at 6 Health with five visible
foes. Venomous Bite ended both runs. Choosing between exits also requires
remembering what was last seen on the far side.

Two exploratory policies produced ordinary action traces but no verified
terminal result. A seed-47 retreat that preferred breaking dangerous contact
reached floor 15 (4,738 submissions). A seed-49 policy that sampled unknown
scrolls at safe stairs read three, reached floor 8, and stopped after 4,604
submissions. Both reported `unknown_outcome`. Neither counts as a death or a
win. An interrupted seed-47 control exposed a separate diagnostics problem:
the native observation refresh raised `UnknownOutcome`, then closing the
session raised a visual observation timeout. Preserve the original failure
when reporting cleanup errors. The traces do not establish a server cause.

The entries name the scenario and revision used where available. Earlier
experiments on `rogue-exploration` need fresh verification against bundled `rogue`.
Use this page to retain observations that may warrant separate work. Campaign
acceptance and reproduction commands are in the [capability and acceptance
checks](rogue-o-matic-parity.md).

## A resolved stair move can change the picture at the same tick

The occupied paired-stair process fixture on the `496de9a8` server returned
a queued receipt at tick zero. A subsequent observation, also at tick zero,
showed the matching receipt resolved, a different origin cell and the
previously observed stair endpoint one cardinal step away. The blocker remained
on the endpoint. The Python client now compares the scene and receipt without
requiring the simulation clock to advance. This case illustrates a client
assumption about time and does not establish a server defect.

## Seed 49 produced different search choices in fresh games

Two runs with the same arrow-recovery policy, baseline server and bundled seed
49 agreed through 1,228 submissions on floor, tick, action reason, movement
direction, admission and previous phase. At decision 1,229 both chose a search
approach at tick 120480. One moved south and one moved west. One exhausted
its routes on floor 4; the other reached floor 8. A policy regression
independently reproduced a cause of variation: exchanging the opaque keys of
two otherwise equivalent search sites reversed the preferred direction.
Search now breaks ties using map traversal order. Other sources of run-to-run
variation remain possible.

## Save backlog and an unknown movement outcome

The second seed-49 run above, under concurrent host build load, reported an
unknown movement outcome on floor 8 at tick 338078. Closing the ordinary
session then raised a save-backlog error: 15,539 entries were saved and this
save required 16,954. The runner timed out waiting ten seconds for server
shutdown. Treat this run as an infrastructure failure. The trace does not
establish whether the movement resolved or whether the backlog caused the
unknown receipt. A focused server/common-client follow-up should reproduce
backpressure, check which close errors reach the caller after an unknown
receipt, and verify durable completion and clean shutdown.

The cohort runner now retains the observed game result if server shutdown
times out. It records `server_cleanup: "timeout"` on the seed, continues the
remaining seeds and exits with an infrastructure error.

## An admitted throw can fail on the same tick

On bundled development seed 47, server SHA-256 prefix `864f9811b114`, the
ordinary player saw a hostile body cell at offset `(-3, 0, 1)` and an offered
throw source. At tick 8674 the server admitted Throw. The next ready picture
at tick 8674 showed the same intention in the failed phase. The retained
phase gives the outcome, but supplies no reason for execution failure.
A real-client test matches the admission to the failure by intention and
replays the subsequent decision. A server follow-up should establish which
execution check failed and whether its reason can be disclosed to the player.

## Queued descent with cancellation available

Three runs of the development seed-49 stair case reached floor 11. At tick
597000 a floor descent was admitted, and the clock then stopped. The last
ordinary player picture from the diagnostic run showed 55/57 Health,
nutrition reserve −759, active hallucination, a ready actor, closed admission
and one queued intention. The cancel offer named the admitted stair intention.
The server warned that saving was behind schedule. The run ended in a
readiness timeout. The role of the save backlog remains unresolved.

Reopening a copy of the save showed the same intention suspended at the same
tick. Cancellation was confirmed, a wait advanced the clock to 598400 and
a fresh descent was admitted. The next ready picture was at tick 598500.
Investigate why the live clock stopped while a ready actor had a queued move.

A fresh diagnostic followed the original policy up to the live stall, then
used the updated recovery policy. At tick 597000 it matched its admitted
descent to the queued activity, cancelled it and waited once. The next
descent at 598400 resolved on floor 12 at 598500. This diagnostic ended at
the crossing. The bot limits recovery to one cancellation at the same
stair origin until a crossing is established.

## A visible stair can be unusable while hovering

On development seed 49 the actor sampled an unidentified potion, reached
the down stair at tick 3492, and submitted a descent. The server returned
`invalid_action` and the message `Intention is unavailable`. The recorded
picture showed the active levitation condition with 1,958 ticks remaining.
The bundled rules suspend ground pickup and stair use while hovering. The
policy now waits at its intended stair and checks ground contact before
attempting a pickup or a vertical escape. On the same seed it waited twenty
turns and the server admitted descent at tick 5492. A small paired-stair
fixture verifies arrival on the lower floor after the sampled potion expires.

The terrain flag describes the stair correctly. A player-visible explanation
of the locomotion restriction on refusal would help clients distinguish it
from an obstructed landing. This is a response-design follow-up.

## One door can have different reachability on different body cells

In a bundled seed-42 campaign on `517a7812`, the observer saw one closed
secret entrance on two cells immediately west, at heights zero and one.
Both records had the same door ID. The lower record offered a reachable
approach from the player's cell. The upper record had no approaches and
reported unreachable. The observer's own visible body record was at height
one. Checking only that record made the bot walk back and forth beside a door
it could open. The Python client now presents one door and combines its
disclosed approaches for reachability. Per-cell approach information is valid
geometry. Clarifying its scope in the server field documentation would help
other clients avoid the same interpretation error.

## A free weapon socket can still be unusable

On the bundled Rogue scenario at `517a7812`, seeds 42 and 47 both reproduced
this at tick zero. Actor 1 had the mace in weapon socket 0, ring mail in socket
2, an empty weapon socket 10, and a shortbow in inventory. The inventory view
listed the shortbow's slot kind as `weapon`. Equipping it in socket 10
returned `Intention is unavailable`; the actor remained at tick zero.

The [equipment rules](../reference/equipment.md) give the shortbow a two-grip
requirement. The socket category and an unoccupied socket alone cannot
establish that the actor has sufficient grips. Consider returning the
player-known reason for an inadmissible equipment choice, or disclosing the
feasible socket choices directly. The ordinary common-client item selector
also chooses a free socket by matching category.

## Projectile reach during selection

On the API refactor baseline `517a7812`, the server offers an exact launcher
and compatible ammunition. It describes the ability's reach as `Projectile`.
The source and ammunition rows identify items and sockets, but neither row
gives the reach of the chosen delivery. A client can tell that an arrow fits
the bow and that a creature lies along a visible ray. It still has to submit
the shot or apply a scenario-specific distance estimate to decide whether the
creature is within range.

This is a response-design finding from the
[choice schema](../../crates/protocol/src/projectile_choices.rs) and
[ability reach schema](../../crates/protocol/src/creature.rs). Consider
disclosing the player-known effective range for the selected delivery,
including how vertical distance counts. A follow-up fixture should compare
two deliveries with different ranges and verify the boundary against actual
resolution while preserving any intentionally undisclosed modifiers.

On bundled seed 47 at `5192b143`, 14 of 18 throw submissions were followed
by a failed activity at the next decision. Three were resolved and one was
paused. Near the end of the run, the actor could submit melee at the same
tick as a failed adjacent throw. The turn trace establishes the admission
and subsequent phase. It does not establish why the throws failed. A
focused follow-up should compare the offered projectile and target with
the failure reason exposed to the player at resolution.

## Current main reconciliation

Main at `5b9299c4` includes `7d5e33ef`, which pools checkpoint records and
preserves journal durability when a capture exceeds the encoder limit. It
has focused regressions for committing and replaying after a deferred
checkpoint. The earlier workaround does not need to be carried forward.
The long-run replay cost remains relevant.

The earlier occupied-stair patch does not apply directly to the current
action and floor-activation code. A live reproduction on `5b9299c4` used
two ordinary player connections and a paired stair with an occupied anchor
and open adjacent tiles. The server returned `invalid_action` with the
message `Intention is unavailable`. Both actors stayed in their original
cells and the tick stayed at zero. The port keeps the paired endpoint as the admitted movement context and
samples an open destination at execution. A recorded historical failure
still replays under the occupied-endpoint rule. On the rebuilt server at `b28c8e90` plus the port, the same ordinary-client
fixture confirmed that the player arrived one cardinal tile from the
endpoint and the blocker stayed in place.

The first current `rogue` seed-42 run died to Venomous Bite on floor 8.
An intervening intention interruption originally hid the cause in the cohort
summary. The reported event itself identified the injury. The summary now
uses the player death event and checks the preceding ability across the
player's interruption.

In that run, the bot was clear of hostiles at 13 of 22 Health after the
first rattlesnake fight. The shallow-floor policy continued exploring and
ran into more enemies before recovering. Recovery now applies the same
health and food checks at every depth. A live replay of the exact post-fight
observation confirmed that the server accepted its recovery wait.

The next two seed-42 runs reached floor 8 and died to a hobgoblin's
Rattling Blow after 827 actions with 477 nutrition reserve. In the final
sequence the bot went from 9 to 2 Health, canceled its attack, tried five
retreat moves, and returned to melee without recovering. The later policy
also learned threats from increases in disclosed restorable losses when it
could attribute them to one hostile attack. A live two-client fixture checked
this using the strength loss from Venomous Bite. The two campaign traces
ended at the same tick, so the venom regression has no demonstrated effect
on this seed's hobgoblin encounter.

Seed 45 died fighting trolls on floor 13 after 7,753 actions with 701
nutrition reserve. More than 1,800 admitted search-related actions on
floor 5 preceded arrival on floor 6. The searches spread across many cells.
The bot also spent 799 actions recovering on floor 12. The raw trace
shows that long recovery sequences came from repeated injuries, with
uninterrupted healing advancing one Health per 900 ticks. An ordinary
client on a copy of that save confirmed the disclosed 900-tick clock
and saw it advance during a wait. The campaign trace now records the
clock so future runs can separate the recovery rate from interruptions.
The bot now moves out of a recently threatened cell before waiting when
its map offers a route to a previously visited cell outside the threat
area. A live injury fixture confirmed the move and the subsequent wait.
The seed-47 rerun never selected the new recovery move and ended at
the same tick. A seed-45 comparison is running.
Seed 46 died on floor 9 after 1,829 actions. On the rebuilt server,
seed 47 died to a hobgoblin on floor 6 after 868 actions. Floor 6
contained 162 confirmed retreat actions, and its last basic melee
attacks missed. The bot now considers a clear diagonal alongside
cardinal steps that increase distance from the nearest threat. A live
wraith fixture confirmed both the diagonal choice and actual movement.
In the seed-47 rerun, ten diagonal retreats were confirmed on floor 6.
The bot then resumed exploration at 24 Health and tick 96,572. The
earlier run died at tick 87,836. The rerun used the new recovery move
twice on floor 7 and died on floor 15 after 7,225 actions. The last
centaur hit dropped the player from 22 to 10 Health. The player
retreated and encountered a healthy troll, whose three-strike attack
killed it. Nutrition reserve was 463.

The two-step escape initially used species recognition, disclosure of
a lasting injury, or the critical-Health threshold. It now uses the
same observed-damage margin as the rest of the retreat policy. A live
fixture confirmed that knowledge of a hobgoblin's damage can trigger
the first escape step above the critical-Health threshold.

When an observed hit from an adjacent enemy could kill the player
and no escape route is available, the bot can now use a known
healing potion above its usual half-Health threshold. The live
fixture confirmed consumption and increased Health.

Known healing, haste, Hold Monster, and teleportation consumables
are matched by their canonical item identities. Their actions still
require the server's current Drink or Read offer. The live cornered
healing fixture renamed the potion and confirmed it still restored
Health.

The earlier seed-45 recovery run eventually died on floor 17. A
medusa remained at cardinal or diagonal contact through eight
retreat-to-stairs decisions, and the player's Health fell from 44 to
zero during that sequence. Nutrition reserve was 1,038. The pinned
seed-48 run reached floor 18 and died to a black unicorn after 5,706
actions. The stair route now yields to a step that creates a larger
disclosed gap while the enemy is still in reach. A live fixture with
paired stairs confirmed the choice and movement. The next pinned
seed-47 run died on floor 6 after 764 actions. It used several policy
changes made after the floor-15 run, so the result cannot isolate the
change to stair routing.

The corridor-search seed-45 run died on floor 5 while waiting to
recover. The visible hostile list was empty, but the same location
received repeated `unseen_ability` events and the terminal event
identified an applied unseen attack. Health fell from 8 to 5 to 2
while the bot kept waiting there. The policy now treats an unseen miss,
an unseen event from a disclosed hostile, or an applied unseen event
with Health loss as evidence that the current cell is exposed. It
prefers a known route away from that cell before waiting and carries
the risk forward briefly across observations. An ordinary two-player
fixture verified movement away from an invisible phantom. A pinned
seed-45 run took 18 evasion steps on floor 5 and still died to
an unseen attack. The final trace alternated between evasion and
rest, including a rest at one Health. A safe-looking destination
can still be in reach of an unseen pursuer.

With at least half Health in a lit cell, the policy can now use an
identified see-invisible potion if the server offers Drink and the
effect is inactive. Its choice uses the item's canonical identity.
An ordinary fixture verified that the potion revealed a phantom.
The same fixture also verifies the movement fallback when the
player has no such potion.

Following an unseen attack, the bot continues evading for up to 400
ticks after the last evidence while it has an available route. It
avoids immediately reversing the last evasion step. The two-client
fixture confirmed a second move with no new attack in the intervening
observation. The next pinned seed-45 run had taken 89 evasion steps
on floor 5 and was alive at 23 Health at tick 206,063. It eventually
died on the same floor at tick 238,174, after 112 evasion steps. The
preceding run died on that floor at tick 188,417.

Cohort summaries now classify a preceding applied
`unseen_ability` event targeting the player as the cause of death. They preserve the redaction of the attacker and ability. A live
ordinary-player fixture and two saved seed-45 death traces agree.
The fixture also saw `stale_context` when the phantom changed the
player's command context before a Wait reached admission. The bot
already retries that response after reading a fresh observation.

Health recovery proceeds during ordinary movement. The policy now
uses that time to approach known stairs when moderately injured and
the route's intermediate cells have been visited and are clear of
known hazards. It keeps the existing rest goal at the landing. A live
fixture showed elapsed recovery increasing while the bot walked and
confirmed its choice to wait on the stair. A pinned seed-48 run is
testing this change in bundled Rogue. The run died on floor 19
after 7,115 actions while cornered by two vampires. The terminal
observation attributed the injury to Drain Vitality. The run did
not select the recovery-while-moving action.

Against a dangerous enemy at range, the policy now considers a
projectile before falling back to retreat. It requires a clear
disclosed line, at least three grid steps to the target, and a
launcher or throwable source the server offers. Critical Health
or a learned lethal margin still favors escape. The live fixture
confirmed the launcher and ammunition query and the arrow leaving
the player's inventory. The seed-50 run reached floor 11 without
selecting a projectile. The bundled starting mace, shortbow, and
arrow stack already meet the generic weapon-stock limit. The policy
now accepts one dart stack beyond that limit when the actor knows
Throw. The live fixture verified pickup and a server-offered
carried throw source while the mace remained equipped. A pinned
seed-50 replay is testing the inventory change.

The historical Rog-O-Matic's secret-door search evaluates how many
eligible boundaries a search can cover from one cell. The Python policy
now uses boundary coverage to choose among sites of the same priority
and route cost. A live fixture confirmed travel to the site with two
eligible walls, followed by an admitted Search. The bundled seed-45
run used 1,965 admitted Searches and 731 search-route moves on floor 5
before descending. The earlier run used 275 admitted Searches and
80 search-route moves there. This run also included diagonal retreat,
so it does not isolate the search ranking change. The latest policy now gives a disclosed corridor end a second
check before sweeping ordinary room walls. A live fixture confirmed
the route and a completed Search. A fresh pinned seed-45 run is
measuring the campaign effect.

The Python example and independent victory check now recognize the
configured retrieval item by its disclosed canonical identity. A
custom fixture presented two identically named seals with distinct
identities. The bot acquired the configured seal using an ordinary
player session. The bundled scenario keeps the Amulet identity.

The example previously looked only for the `ration` supply tag in
carried food. An ordinary nutrition fixture disclosed an Eat offer for
an untagged meal and withheld the offer from another item in the food
class. The bot now eats the offered meal and applies the same
affordance when deciding whether it can afford to wait and recover.
The live fixture increased nutrition reserve from 2 to 8.
The policy also now records and collects ground items disclosed
as food even when they lack the Rogue ration tag. The ground
view does not disclose an Eat offer. The policy waits for the
server to offer Eat after pickup. A non-Rogue nutrition fixture
verified both actions.

Earlier cohorts launched each seed from the working tree. If the
policy changed between seeds, the cohort's initial hash could describe
only the earlier runs. New cohorts archive the Python policy and its
local imports under the output directory, run that copy for every
seed, and record the hash of each archived file.

The first seed-50 ranged-opening campaign ended with
`readiness_timeout` on floor 11 at tick 265,800. The server admitted
a down-stair intention, then admitted its cancellation and a new
down-stair intention. The replacement stayed queued with the actor
ready and no active preparation. SQLite contained both admissions
and the cancellation, with the replacement admission as the final
durable entry and no execution entry. The server also logged
save-behind warnings. The evidence establishes a durable queued
action that was not executed during the observation window. The
reason it stayed queued is open.

A second seed-50 run with the dart pickup policy timed out at the
same depth and tick. A copy of the first save reopened with the
same intention suspended and an ordinary Resume offer. The server
confirmed Resume and returned the intention to queued. Repeated
ordinary observations stayed at tick 265,800 for 34 seconds with
the actor ready and no preparation running. The cohort command now
returns a failing exit status for this timeout and keeps the trace
and summary.

A fresh ordinary headless connection also acknowledged Resume and
received `waiting on you` afterward with admission false and no
Resume offer. The accompanying state still showed the stair
intention queued. There was no server error in the next 12 seconds
of headless output. This rules out an execution error lost by the
Python binding during that capture. Scheduler selection and the
reason for the contradictory waiting state remain open.

A second ordinary-client experiment on a fresh copy canceled the
suspended stair, submitted Wait, and submitted Down once admission
returned. Wait advanced the clock to 265,854. The new Down
intention cleared and the player's cell changed at tick 266,000.
The bot now uses a one-turn stair backoff after canceling its own
stalled stair intention. In a separate run on a fresh copy, the bot
submitted Down and reproduced the stall at tick 265,800, canceled it
after the timeout, and waited. Wait advanced the clock to 265,900.
The retried Down moved the player to floor 12 at tick 266,000.
A fresh bundled seed-50 campaign then reproduced the timeout at the
same tick and passed into floor 12 using the same automatic sequence.
A pinned seed-49 run reproduced the same queue symptoms on an
ordinary eastward move on floor 6 during the return with the Amulet.
At tick 1,168,384 the bot canceled the move and resubmitted it.
The replacement was still queued with the actor ready, admission
closed, and no preparation running when the second timeout expired.
A manual replay on a copy of that save reproduced the eastward
stall. After cancellation, Wait advanced the clock from 1,168,384
to 1,168,800. A fresh eastward move changed the player cell at
1,170,000. Another pinned seed-49 run timed out in the same way on
the ascent from floor 7 at tick 1,156,344.

The bot now applies the same recovery to walking and stair movement.
After a confirmed cancellation, it waits once if it would retry
that same move at the stalled tick and no hostile is disclosed.
A new decision or a changed tick can proceed immediately. A bot
runner replay on a separate copy of the floor-6 save reproduced
Cancel, Wait, and East and reached the same cell at tick 1,170,000.
The player was hungry enough to be empty. A separate replay captured
no active conditions before the East admission or while it remained
queued, with the actor ready and no preparation running. Wait
advanced the clock and disclosed a Fainted event, with no condition
still active at the resulting ready boundary. This rules out an
already active, disclosed Fainted condition as the explanation for
that queued stall. A failure during candidate execution or
publication remains possible.

On base `b28c8e90`, the pinned seed-49 policy retrieved the Amulet on
floor 26 and returned to floor 2 in bundled `rogue`. It died of
starvation at tick 1,247,200 after 11,183 actions, with reserve at
the server's -852 threshold. Floor 4 was the largest bottleneck on
the return. The bot entered with 263 reserve and left with -495
after 459 attempts, including 216 exploration and 136 search
decisions. The archived policy's SHA-256 begins `d3c8ad9a07ef`.
A new pinned seed-49 run uses the current working policy.

The return-to-stairs planner now retains previously disclosed diagonal
steps for use later in a route. The first step still requires the
current view and its corner checks. A policy regression covers a
route that needs an earlier diagonal, and the latest full-depth
fixture still verified retrieval and surface escape. The original
floor-4 trace shows a gap in the route recorded on descent. On floor 4,
the player changed cells immediately after reading an unidentified
scroll, at ticks 80,036 to 80,136, with no intervening move or hostile.
The independently pinned diagonal policy has the same type of jump at
the same point on descent. A teleportation scroll fits the observation.
The trace does not include the consumed item's disclosed identity.
The bot now walks reachable unexplored routes before trying unknown
scrolls in safety, and can still test a scroll when exploration stalls
or in an emergency. A live regression with the concealed production
teleportation scroll verified the priority. A fresh seed-49 campaign
uses the revised policy. On floor 4 it walked more of the map but
still changed cells after reading an unknown scroll when useful
frontier travel ran out. The movement trace still has no continuous
walked route between its floor-4 entry and exit. The priority
change alone did not preserve a return route on this layout.

The next policy prioritizes both travel and the existing bounded
search sweeps over safely experimenting with unidentified scrolls.
A live corridor fixture with a concealed teleportation scroll
verified that the bot walks to a known search site and searches
there. A fresh seed-49 run tests whether the extra search time is
worth a continuous return route on the bundled layout. Before finding
the down stair, that campaign performed 517 Search invocations on
floor 4 and was down to 315 reserve. The policy now tries an available
unknown scroll ahead of low-priority repeat searches when hungry and
without carried food. It still checks mapped entrances, plausible
connections, corridor ends, and previously unchecked sites first.
A live corridor test verifies both priorities in one run.

The search-first campaign eventually left floor 4 with its entrance
and exit connected by ordinary moves. It read no scroll there.
It logged 1,846 floor-4 actions, including 517 Search invocations,
across two visits. Reserve was 710 on first arrival and 1,373 on
departure after finding food. The earlier policy logged 697 floor-4
actions and left with 1,304 reserve, with its walked route disconnected.
The search-first run later starved on floor 8. The food-aware variant
logged the same floor-4 search count and died in melee on floor 8
with negative reserve. Neither run reached the ascent.

The current policy checks mapped entrances, plausible connections,
corridor ends, and an unchecked boundary at the current site before
testing an unknown scroll. It then tries the scroll before starting
a broad sweep of weak leads. Once the scroll options are exhausted,
the broad search remains available. A paired seed-49 comparison is
underway. The live corridor test verifies that a promising site still
wins and a low-priority repeat search yields to the scroll.
The floor-4 descent used 682 actions, including three Search
invocations, and left 1,326 reserve. The third unidentified scroll
changed the player's cell. The walked route still does not connect
entry and exit, so the earlier route gap remains possible.

The bot now estimates return travel from each completed descent's
known upstairs route and its observed stair traversal time. The
estimate uses the same route planner as ascent and includes the move
count from remembered diagonal links. A regression covers a route
that needs a previously disclosed diagonal beyond the first move.
When carrying the Amulet with at least half Health and no edible food,
optional rest must leave enough reserve for that estimated travel
if the bot knows a stair route it can take. The estimate omits
unknown routes and uses stair timing as a proxy for walking.
The live full-depth fixture confirmed the recorded estimate and
independent victory verification. Bundled seed 49 is running with
the new policy.

The recovery policy can equip a known slow-digestion ring first
when the actor needs food and has an empty ring slot. The live
fixture confirmed both the equipment decision and lower nutrition
consumption over equal periods of resting.

While carrying the Amulet, the policy no longer spends moves closing
with enemies outside melee range when the upstairs route is unknown.
It can defend against close enemies and use ranged options. A live
fixture confirmed that it explored an available route away from the
enemy instead of walking toward it.

## Open follow-ups

| Finding | Evidence | Possible follow-up |
| --- | --- | --- |
| Oversized automatic checkpoints leave a growing replay tail | Two floor-23 saves produced checkpoints of 79,751,935 and 72,131,357 bytes against the 64 MiB limit. Journal commits now survive a skipped snapshot. | Profile which checkpoint sections grow during long campaigns and measure reopen time when several snapshots are skipped. |
| Movement refusals can be hard to interpret | A stationary-locomotion fixture returned `actor_busy` for a move and accepted a wait. In separate cases the player view already disclosed a live bear-trap condition or downward motion while nearby movement still looked possible. | Review how readiness and rejection codes communicate the specific reason a proposed move cannot execute. Keep trap, fall, and stationary cases distinct. |
| Long search sweeps can exhaust rations | Seed 45 survived the earlier lightning encounter and starved on floor 16. Reopening the terminal save confirmed reserve and the starvation threshold were both -852. The final stretch contained repeated Search actions. A fresh run with food planning passed floor 16 and died in combat on floor 18 while nourished. The policy now prioritizes visible known food before taking safe stairs even when already stocked and routes back to last-seen food on the current floor when hungry. | Compare search order and food planning on the same layout. Measure paid searches, travel steps, and reserve at floor transitions. |
| A stair retreat can immediately re-enter the same fight | In the food-planning seed-45 run the bot escaped a black unicorn by going upstairs at 22 HP, then went right back down and died. Both decisions are in the ordinary turn trace. | The policy now preserves recovery if the visible pursuer is on the far side of the projected stair gap while the bot is upstairs. Verify the change across a fresh campaign. |
| A retreat can end in a worse encounter | A seed-42 two-step retreat broke contact with a black unicorn before the route led into a troll. A later seed-42 run retreated west repeatedly before dying to two griffins on floor 23. The replay before the griffin fight showed 29 HP, an equipped two-handed sword, a depleted staff of slow monster, and unidentified instance stats on carried banded mail. | Reconstruct retreat choices from ordinary observations. The policy now compares up to six disclosed retreat steps and checks whether an exit reconnects to another safe exit. Compare this with the older route ranking on fixed seeds, and examine control devices and equipment preparation before deep floors. |

## Verified fixes in the earlier checkout

| Finding | Resolution and verification |
| --- | --- |
| An occupied paired stair landing could reject the traversal | The server randomly selects among the nearest unoccupied landing cells. Simulation tests cover selection and replay of historical outcomes. The paired-stair process test verifies the alternate placement survives reopening the save. |
| An oversized automatic checkpoint failed the journal batch | The save worker keeps the previous checkpoint and commits the journal. The regression verifies reopening, and a copy of an affected save reproduced the failure on the old server and advanced its durable sequence on the fixed server. [Storage regression](../../crates/server/src/storage.rs) |
| The policy could retry blocked movement instead of recovering | The bot checks disclosed bear traps and falls, and replans after movement refusals. Live fixtures cover waiting out a bear trap, settling after a fall, and taking a fresh action after `actor_busy`. |
| An elemental ray could reflect back into the bot | Seed 45 died at full health on floor 6 after its own lightning shot. The policy now checks the disclosed clearance before firing a bouncing ray or testing an unknown device. A fresh run passed the same encounter. [Policy tests](../../scripts/test_rogue_o_matic.py) |
| The weapon stock limit could make the bot pass up a stronger known sword | A live player with three maces walked past a long sword before the change. The policy now allows the pickup and evaluates actual equipment stats separately. |
| A cohort could credit the wrong death cause or trust a self-reported win | Death reports carry final nutrition details. The runner distinguishes starvation from self-targeted Search and verifies a reported win and carried Amulet over a fresh player connection. [Cohort tests](../../scripts/test_rogue_o_matic_cohort.py) |
| Python could see chart summaries but could not fetch chart rows | The binding now assembles validated chart pages through client-common. The live Mapping fixture retrieved 1,024 rows and verified the read-only query preserved the game tick. [Bot API](../reference/bot-client.md) |

A later seed-49 policy reached floor 24 and retreated to floor 23
with 11 of 66 HP. When a jabberwock appeared above, the bot immediately
took the same stairs down and then retreated back up. The same pattern
repeated with a griffin and black unicorn. The descent shortcut under
threat bypassed the existing recovery threshold. The policy now checks
that threshold before choosing a descent during recovery. A paired-stair
process regression sends the recovering bot upstairs, where a new
hostile is present, and verifies that its next move stays on the upper
floor. Without the safeguard, the same fixture chooses Down. On a
quiet landing, the food budget can still favor descending over resting.
A separate live fixture confirms this choice with a low reserve and
no carried food.
An earlier sequence of stair reversals occurred at full health and
remains a separate routing problem.

In the seed-42 acceptance run the recovering bot pursued a rattlesnake
on floor 8 at 17 of 38 HP and took a hit that reduced it to 10 HP.
It retreated toward the stairs and was later killed in combat with the
rattlesnake and quagga. Recovery now favors breaking contact before
pursuing, using the same escape choices even without a learned damage
estimate for the visible hostile. A separate live fixture checks the
decision after a stair retreat.

The next seed-42 acceptance run passed the floor-8 rattlesnake
encounter and died in melee on floor 9 at tick 430,743. It had
2 of 38 HP while retreating from two quaggas and an orc before
its last melee attempts. The run remained nourished.

A seed-49 campaign that read unknown scrolls before general searches
died in melee on floor 18. While pinned by a black unicorn and a
medusa, it canceled a damaged attack and immediately chose the same
attack. It repeated this cycle as Health fell from 50 to 13.
The bot now allows the identical immediate retry to resume if
interrupted again. A different attack remains eligible for
replanning. A runner regression reproduced the cancellation loop.

The fresh seed-42 run with attack retry handling also died in melee
on floor 9. It ended at tick 460,409 after retreating from a quagga
and a centaur. Held-out seed 45 starved on floor 14, and seed 46
died in melee on floor 2. None retrieved the Amulet.

Identification scrolls now match disclosed item identities when
choosing a readable scroll. A live fixture renamed the known
potion-identification scroll while keeping its identity and effect.
The old policy drank the unidentified potion. The revised policy
read the scroll and selected the potion from the server's prompt.

The bot also uses disclosed item identities to rank known rings,
avoid equivalent duplicates, and choose armor and weapon upgrade
scrolls. A live fixture with a renamed searching ring verified that
the bot equipped the ring in an ordinary empty slot. The ring's
display name has no effect on the ranking.

Bundled seed 47 exposed an admission/execution mismatch for
`rogue.throw` on floor 1 at tick 2,920. The ordinary bot repeatedly
received confirmed queued receipts for the same westward weapon
throw while the game clock stayed fixed. Read-only inspection of
the campaign journal found alternating `intention_admitted` and
`intention_failed` entries for the attempts. This differs from the
stair and walking stalls, where the queued move never retired.
The protocol supports a Failed intention phase. Client-common
removes terminal phases from its active list, and the Python
snapshot currently exposes that active list. The bot therefore
sees the receipt and the ensuing ready state without the terminal
phase needed to distinguish a failed throw from a resolved one.
The runner now defers projectile selection for the rest of the
current tick if a queued throw or launch returns directly to a
settled, same-tick observation. This uses only the bot's ordinary
observations. It allows projectiles again once time advances. The
bundled seed-47 process regression used to stay at tick 2,920 for
the entire 60-action budget. The revised bot attempted the throw
once at that tick and advanced past it. Why the offered throw
fails at execution is still open.

The bot client now retains a bounded window of validated terminal
intention updates in a separate Python observation field. The
seed-47 regression matches the failed throw to its admission receipt.
The runner can defer projectiles based on the reported Failed phase,
while still using the same-tick check when the phase is unavailable.

The bot worktree was brought forward from `b28c8e90` to
`e10b9a23`, using the bundled Rogue rules at that commit.
Its scenario manifest is unchanged. Cohort summaries now record
the hashes of the server binary, rules source, and validation
certificate so this change is visible in future comparisons.

On the earlier baseline, seed 48 stopped at `no_disclosed_route`
on floor 9 at tick 394,859, with 1,442 reserve at its last action.
It attempted Search 1,210 times across 193 cells on that floor.
Almost all of its last 300 decisions were Search or moves to search
sites.
This records that the policy ran out of routes. Whether a reachable
route was undisclosed, ruled out by the policy, or missed during
search remains open.

The food budget also applies while the stairs are unknown. If a
quiet actor has at least half Health and can't afford to rest,
the bot now uses a disclosed route to an unvisited cell, avoiding
known danger and traps. A live nutrition fixture verified that
moving advanced the recovery clock when there was no stair offer.

A live paired-stair fixture showed the retreat planner missing an
alternate route when the first move of its unconstrained shortest
path approached a griffin. The policy now searches for the shortest
stair route among safe first steps. The fixture verified that the
bot moved north toward the stair, away from the griffin, instead
of taking an unrelated retreat step.

The stair planner also considers a disclosed diagonal first step
when it increases separation from the visible threat. A paired
live fixture previously chose the first of two cardinal steps.
The bot now reaches the stair cell with one diagonal step, while
the cardinal-only escape case still takes its safe north route.

On `e10b9a23`, the safer cardinal retreat policy took acceptance
seed 42 to floor 21 before it died in melee at tick 668,473.
Several earlier melee intentions failed while enemies repositioned;
the bot selected a different action at each observed failure.

The live venom fixture exposed a gap between the ordered outcome
stream and the refreshed picture. The picture showed increased
restorable Strength loss but had an empty combat event list. The
native bot binding now exposes a bounded, cursor-ordered history
of recent semantic combat events. The policy consumes each batch
once and can learn which foe caused the injury from the retained
venomous bite. Effects that represent visual geometry are omitted
from the retained history.

The next seed-42 run died on floor 8 after using three unidentified
scrolls while cornered. It began the sequence at 21 Health and
finished it at 7. The policy now tries one blind consumable before
returning to the normal combat choices while a hostile remains
disclosed. Known healing and escape items keep their earlier
priority. A live fixture with two unidentified scrolls verified that
the second decision was a melee attack.

On `e10b9a23`, seed 49 retrieved the Amulet on floor 26 and
died in melee on floor 25 on the way out. It took the Amulet
at reserve -300 and ascended from the retrieval floor at -372.
This is a verified retrieval but not a victory.

The bot now forages for last-seen food before taking a stair
when the estimated ascent exceeds its reserve and it has no
edible supplies. A live paired-stair fixture verified that a
nourished bot left the stair to follow its remembered food route
when the estimated travel changed from affordable to unaffordable.

The same return budget is now used before the bot enters the
retrieval floor. If the forecast exceeds the actor's reserve,
there is no edible food, and a safe route to remembered food
exists, the bot can provision for escape before picking up the
objective. A paired live fixture verified both the ready descent
with sufficient reserve and the detour after the forecast rises.

The bundled weapon pickup and spear training decisions now use
disclosed item identities. The live pickup fixture renamed the
spear and verified that the bot added it to the full starter
loadout. The server still requires spear expertise before it
offers the spear as a throwable source.

The bot can now use a known teleportation scroll as a last resort
when carrying the Amulet, with a remembered up stair and no
available walking or search option. A live fixture verified the
decision and the resulting relocation. Whether this can reconnect
a particular bundled floor remains unverified.

A repeat of bundled seed 49 with ascent-only provisioning entered
floor 26 with 21 reserve, acquired the Amulet, and returned to
floor 25 with -373 reserve. It died at the same tick (1,107,692)
as the earlier melee death. This run reproduces the low-reserve
entry to the retrieval floor that the pre-retrieval policy targets.

The pinned pre-retrieval provisioning run also finished after
9,912 actions, with the same stair reserves and terminal tick.
On floor 25 it made no remembered-food detour despite entering
floor 26 with 21 reserve. The policy requires a route to known food
on the current floor, so this result leaves open whether starting
the food search on earlier floors would change the outcome.

The bot now compares its food reserve against the mapped portion
of the return trip on every floor. A live fixture three floors
before retrieval verified that the bot followed a remembered food
route when the estimate exceeded its reserve and took the stairs
when it had enough. The estimate only covers mapped return legs.
In the previous bundled seed-49 trace, the first floor where the
observed reserve fell below that floor's known return cost was
floor 9 (298 reserve against 322 needed). Floor 23 ended with
745 reserve against 857 needed, and floor 24 ended with 515
against 923. A shortfall can change the route when the bot has
no edible supplies and can reach remembered food.
The bundled seed-49 run with this change finished without a
remembered-food detour. Compared with the previous attempt, all
9,912 action rows had the same floors, ticks, reasons, reserves,
action types, directions, and ability IDs. It again acquired the
Amulet and died on floor 25 at tick 1,107,692.

The pre-retrieval seed-49 trace spent 122 reserve on recovery
waits on floor 25 before descending with 22 reserve. The bot now
includes its mapped return legs when deciding whether it can
afford to wait for recovery during descent while at least half
healthy. A live test verified that the bot took a safe stair
when the return estimate changed an affordable rest into a
shortfall. In bundled seed 49, the first change was on floor 23
at tick 851,230, at 33/66 Health and 872 reserve. The bot explored
east while recovering. The earlier policy waited there. The
recovery-budget variant subsequently encountered a dragon and a
wraith and died at tick 858,286, before retrieving the Amulet.
This distinguishes interrupting a rest to take a known stair
from interrupting it to explore undisclosed terrain.

During descent, if a rest fits the current reserve but would
cut into the mapped return budget, the bot may take a stair it
already stands on or recover along the previously visited route
to a stair. With no such route, it rests before exploring.
The live no-stair regression verified the rest resolved without
moving the player. The companion fixture still takes a stair
when the return budget makes a stationary rest too expensive.

The revised bundled seed-49 run rested at tick 851,230, where
the previous recovery-budget run left to explore. It survived
floor 23. Its first changed decision relative to the original
provisioning run was at tick 970,235, when it took the down stair
at 33/66 Health and 885 reserve. The original run rested there
and later left floor 23 with 745 reserve.

That run retrieved the Amulet, then died on floor 25 at tick
1,141,088. It entered floor 26 at -92 reserve and left it at
-545. The ordered events recorded two Fainted disclosures
during the final retreat. The first accompanied a 1,200-tick
gap between decisions and a drop from 76 to 41 Health. The
second preceded the medusa's fatal melee sequence. The server
reported injury at -823 reserve, above the -852 starvation
threshold. This confirms fainting during this run's melee
retreat. The cause of the player action gap in the earlier
griffin run remains unverified.

The finished seed-49 save shows two rations placed on floor 24.
The bot took and ate one while at -71 reserve. The journal has
no take or eat action for the other ration. Floors 25 and 26
placed neither rations nor fruit. A policy that only detours to
food it has already seen can therefore pass up available food
before entering a drought. The save was inspected after the run.
The bot received only ordinary player disclosures.
The same save shows an uncollected ration on floor 22.
The bot descended from that floor with 571 reserve against
about 800 for the mapped portion of the return route.
The baseline journal also shows that it carried a ration from
floor 20 through the floor-22 descent and ate it after retreating
to floor 21. The scout policy checks for the absence of carried
food, so this floor is a test of the forward food budget as
well as whether a frontier reaches the extra ration.

A healthy bot at a down stair can now scout an ordinary mapped
frontier when it lacks edible supplies and the known cost of
return exceeds its current food reserve. The scout gets a
one-time window on that floor of one quarter of the positive
reserve measured in ticks. A live fixture verified that this
detour brought a previously unseen ration into view and collected
it. A fresh bundled seed-49 run is testing the change.

The bounded-scout seed-49 run made 43 scout moves on floor 23
and 22 on floor 25. It entered floor 26 with 153 reserve,
retrieved the Amulet, and reached floor 24 on the return.
It fainted at empty reserve and died to a medusa at tick
1,087,472, with -351 reserve.

The next policy uses recent observed descent and return times
to estimate the cost of the remaining trip. It credits carried
servings using the reserve gain observed after eating the same
item identity. A carried ration can now be insufficient to
cancel a scout. The live fixture verified collection of a second
ration when the round-trip estimate exceeded the combined supply.

On bundled seed 49, the round-trip estimate first changed a
decision on floor 4. The scout found no food and the bot
descended at 1,184 reserve, down from 1,326 at the first scout
decision. On floor 5 the scout found a ration, confirmed by
the activation journal and the corresponding take action.
The floor-7 scout also found a ration. The bot ate twice on
floor 8, where it made more than 1,000 admitted Search actions.
The full run was still active at that point.

On floor 8, the earlier seed-49 policy opened a concealed door
immediately after searching from its adjacent cell, then took
the down stair. The round-trip variant searched from the same
cell at tick 419,906 and saw no opening. Its journal shows it
searched from other sites repeatedly without returning to the
successful adjacent cell through tick 668,597. This points to
search order after an unsuccessful attempt. The earlier route
does not establish that the door is the only way to the stairs.

The round-trip run eventually opened that door at tick 721,575
after 1,975 floor-8 Search actions had been admitted and resolved.
It reached floor 11, fainted at empty reserve, and died to Venomous
Bite. The policy now permits one retry at a disclosed local wall
after one Search when the nearest untried site takes more than
one move. A live fixture verified two real Search admissions at
the current site before traveling to the farther untried wall.
In the fresh bundled comparison, the first changed action came
on floor 8 at tick 402,992. The bot searched its current wall
instead of moving west to continue the sweep. Both runs had
54 Health and 1,183 food reserve at that decision.
The retry variant opened the concealed door at tick 475,968,
over 245,000 ticks earlier than the round-trip baseline. It
descended with 397 reserve after 205 confirmed Search actions
on floor 8. The baseline descended with 290 reserve after
1,975 resolved searches. Both runs were live campaigns with
different action sequences after the first retry.
The variant entered floor 11 with 904 reserve and survived
past the previous run's death there. It left floor 11 with
412 reserve and ate again on floor 12. The earlier
run entered floor 11 at -136 reserve.
The retry variant ultimately entered floor 16 at 54 reserve.
It fainted as its reserve became empty and died to a troll
at -5 reserve, before reaching the Amulet. The finished save
shows no generated food on floors 14 and 15 and one ration on
floor 16. The journal shows no pickup for that ration.

The same policy on bundled seed 42 repeated the earlier death
on floor 8 at tick 158,330 after 1,592 actions. It made no food
scouts. The ordered terminal trace identifies the final attack
as a quagga's melee strike.

The seed-49 save journal confirms that the last eastward move
before the sudden Health drop on floor 25 resolved at tick
1,105,642. The griffin then started seven melee attacks targeting
the player before the next player action at tick 1,106,700, when
the trace shows Health had fallen from 67 to 20. The bot had an
empty food reserve throughout that interval. Bundled rules allow
an empty actor to faint for 400 to 1,100 ticks. The journal shows
the repeated attacks and gap in player actions. The cause of the
gap remains unverified.

The next bundled seed-49 run recorded ordered condition events
between decisions, including Bear-trapped on floor 4 and Asleep
on floor 6. These disclosures are now available alongside the
action ticks when investigating later gaps in player readiness.

On the same pinned policy, held-out bundled seed 45 died on
floor 8 after 4,373 actions. The bot was nourished with 450
reserve, but was retreating from a rattlesnake and a hobgoblin
at 3 Health. The ordered terminal events attribute the death
to the hobgoblin's Rattling Blow at tick 455,655.

While carrying the Amulet, the bot ranks possible wall connections
by whether the disclosed far side joins the remembered upstairs
component. This gives a specific reason to search a wall before
repeat searches elsewhere. The live fixture verified that the bot
walked to the favored boundary and completed Search. The fixture
does not establish that a secret door exists in a particular wall
or in seed 49's disconnected return route.
