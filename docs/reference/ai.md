# Autonomous decisions and knowledge-limited tactics

This reference describes current decision ordering, not an optimal strategy or
balance target. [Behavior authoring](scenario-packages.md#behavior-defaults-and-ai-overrides)
owns species/profile fields; [perception](../architecture/perception.md) and
[provocation](creature-rules.md#creature-behavior-and-provocation) own sensory and
engagement authority. An actor is the deciding creature, not a global player.

Implementation entry points: [choice](../../crates/simulation/src/ai.rs),
[execution](../../crates/simulation/src/actions.rs),
[item tactics](../../crates/simulation/src/ai_items.rs),
[projectile choice](../../crates/simulation/src/ai_projectiles.rs),
[self-defense](../../crates/simulation/src/ai_defense.rs),
[known-item care](../../crates/simulation/src/ai_item_use.rs).

## Admission, choice and commit are different stages

1. The host admits an AI intention through ordinary simulation ownership. This
   neither chooses an action nor advances the actor's RNG or simulation time.
2. For a due actor, execution calls `choose_executable_ai`. The private decision
   contains one action, resulting AI/behavior/engagement/random state and, where
   applicable, a captured ability plan.
3. `choose_ai` reads the actor-specific decision snapshot and copied controller
   state. Repeated pure previews do not change authoritative visits, engagement,
   action queues or random state. A preview reserves nothing.
4. The execution gate handles narrow contention/numeric fallbacks, prepares the
   ordinary action and binds the original admitted intention identity. It does
   not secretly choose a different hidden source after a selected item fails.
5. Action effects and proposed controller state commit through the same private
   candidate. Failure does not publish speculative visits, engagement or RNG.
   Replay validates the recorded action through the ordinary execution gate.

The [simulation cycle](../architecture/simulation-cycle.md) owns scheduling;
[combat](combat.md) owns paid execution. NPCs do not have a second damage engine,
free equipment path or a reaction selected after inspecting a private result.

## Inputs and retained state

| Input/state | Meaning and limit |
| --- | --- |
| Species disposition and memory | Passive/Mean acquisition and actor-active engagement expiry; an AI profile does not replace these |
| Resolved profile | Independent idle, doors, care, equipment, useful loot, Gold and flee permissions; birth stock does not grant tactics |
| Disclosed decision snapshot | Current visible actors/cells, owned inventory, permitted contacts/interactions; not hidden positions or defenses |
| Personal navigation | Learned directed links, orientation and terrain, not unseen current topology |
| Engagement | Target/reference kind, last seen location and actor-active expiry |
| Anonymous alert | A location to investigate/flee, never a target identity or attack handle |
| Retained preparation | Frozen work, claims and committed cursor; resuming does not recapture it |
| Visit history | Saved per-location exploration decision counts; pure choice does not increment authoritative counts |
| Exploration destination | Optional actor-owned idle interest, not a stored route; combat/care/acquisition suspend it, successful erratic movement releases it |
| Behavior RNG | Saved actor-owned state used for authored random choices, not a wall clock |

Frozen actors accrue no active time and gain no catch-up turns on thaw. An attack
needs the current disclosed reference to match the engagement, not merely the same
private actor ID. Arrival at a remembered location can support investigation
without authorizing an attack against an undisclosed occupant.

A location-only alert completes when a committed actor transition starts or ends
at its sampled point. Arrival, authorized relocation, or a committed transition
at an already reached point retires that receipt; subsequent exploration is not
pulled back by the same cue. This is physical visit completion, not a search
contest or proof that the area is safe. Seeing the point, failing to reach it,
or observing the source elsewhere does not complete it. A later sensory event
can install a fresh cue. Pure previews and unpublished/rejected transitions do
not consume alerts. Existing actor-active expiry and frozen-clock shifting still
bound unreached cues. Completion uses the existing saved alert field, not a new
goal schema; rules 66 distinguishes the changed autonomous behavior.


## State classification

After the early intact-guise ambush branch, the decision is labeled Search, Attack
or Flee for the remaining priority branches:

| Frightened? | Licensed threat location? | State |
| --- | --- | --- |
| Yes | Yes | Flee |
| No | Yes | Attack |
| Either | No | Search |

Frightened means a forced-flee objective or
`current Health × 100 <= maximum Health × flee_percent`, using wide arithmetic.
Threat location prefers forced flee, then engagement; retained-work handling can
use an eligible visible candidate, followed by an anonymous awareness alert.

Hostile candidates must be currently disclosed, pass awareness eligibility and
perceived-hostility checks. Ranking uses remembered route length, then actor ID;
no route ranks after reachable candidates. During fresh acquisition, stationary
actors first prefer currently disclosed legal melee contact, because unavailable
pursuit routes cannot distinguish a nearby foe from a distant one. Moving actors
and retained-work fallback threats keep the original ordering. This does not
replace an existing engagement or bypass disposition, provocation or awareness. The [known-space routing algorithm](../architecture/perception.md#known-space-pathfinding-and-its-limits)
owns edge costs, deterministic search and path reconstruction. Mean actors with positive memory can
acquire eligible opponents directly, with **no separate wake lottery**. Passive
actors need ordinary provocation, except readied touch ambush. Continual Stealth
still gates awareness. Engagement refresh needs a matching current reference;
reaching last-seen position without seeing the target clears the engagement.

A fresh, actually started readied-touch attack can establish bounded engagement
with its currently disclosed faction-hostile toucher. This applies to both the
initial-ready strike and a later ordinary preparation after invalidation. It
preserves an existing live engagement and requires positive memory. Friendly or
undisclosed contact retains its immediate-contact semantics but gains no new
ongoing aggression or hidden tracking. Recognition and adjacency still produce
Wait; resumed work does not refresh continuation memory. Normal preparation,
recovery, costs and subsequent AI priorities remain authoritative.

## Exact priority order

The first applicable returning branch wins. Later choices are not evaluated just
because they might appear tactically better to an observer with hidden knowledge.

| Order | Branch | Selection / stopping rule |
| --- | --- | --- |
| 1 | Intact readied-touch guise, no retained work | With voluntary input and an actual toucher, try the qualifying touch plan; otherwise Wait. Adjacency/private recognition is not a touch attack |
| 2 | Reconcile engagement and state | Retained work does not acquire a fresh opponent or refresh its engagement |
| 3 | Resume available paused work outside Flee | Return the retained plan before new care, gear or attacks |
| 4 | Healing care, no retained work, Health at/below 50% | Known pure-healing carried item first, otherwise eligible harmless healing plan; no unknown-potion experiments |
| 5 | Additional known-item care | No retained work; supported safe effects with a recorded useful outcome, using an ordinary Drink/Read plan |
| 6 | Food care | Search, no visible hostile exposure, required nutrition not Nourished, no retained work and known carried food |
| 7 | Erratic movement | No retained work and movement allowed: copied actor RNG selects only among disclosed legal translations |
| 8 | Flee | Select a legal disclosed retreat using the current heuristic. Without a committed prefix, a qualifying step ability may implement it; otherwise ordinary Move owns abandonment/debt |
| 9 | Resume committed paused prefix when no retreat chosen | Do not replace it with new gear/healing/attacks or repeat its first strike |
| 10 | Proactive self-defense | Requires a current sensed engaged target and a narrowly recognized useful self-condition plan |
| 11 | Engaged attack | Current matching visible target; supported ranged positioning before hostile ability choice outside Flee, basic attacks only when cornered in Flee |
| 12 | Remembered pursuit | Outside Flee, first legal route step toward last seen; unusable pursuit clears engagement and returns to Search |
| 13 | Anonymous-alert investigation | Outside Flee and without engagement, route to licensed alert location; do not invent a target attack |
| 14 | Safe Search items | Equipment improvement, then useful nearby loot, then known Gold; each has policy and visible-hostile gates |
| 15 | Doors | Search plus door permission and voluntary input: first disclosed reachable closed door |
| 16 | Idle exploration | Search + Explore + voluntary input: increment proposed current-cell visits, continue a short remembered destination or pause once on arrival, otherwise select a disclosed destination; fall back to the established least-visited neighbor policy |
| 17 | Fallback | Wait through ordinary action execution |

Stable location ordering defines the candidate list, not a preferred compass direction.
A unique first-tier destination or local neighbor needs no tie draw. Within the
exploration stage, continuing a destination and its arrival Wait draw none.
Unsuccessful destination selection leaves the local fallback its original RNG
stream; independent earlier stages retain their own draw contracts.
This removes shared coordinate bias without introducing a generic goal registry or planner.

An idle Wait profile forbids roaming and detours, not retaliation or every useful
underfoot action. Tool permissions remain independent. One decision does not
search future combat outcomes or learn hidden immunity from rejected previews.

## Ability and defensive selection

Eligible attacks are considered by descending authored AI priority, then stable
ability identity. Choice checks the actor's own resources, cooldowns, source and
disclosed geometry; it does not optimize against secret opponent defenses.
Directional/ray candidates use disclosed alignment rather than a hidden trace to
find concealed bodies. Known finite devices are fallback sources only when their
identity and a positive charge fact make them usable; query or birth possession
does not identify them. Hostile device fallback accepts bounded Sight and
UntilObstruction first-body delivery. The latter uses a disclosed bearing without
inventing a distance cap; physical execution still owns obstruction and first-body
stopping. This does not change granted creature rays, infer cancellation utility
from private conditions, or establish that generated creatures acquire or identify
wands. Exact production Magic Missile admission is covered with synthetic actors
and ordinary Identify, not a natural NPC inventory claim.

Fresh ray, device, launcher and equipped-throw choices decline a bearing when
a disclosed nonhostile body cell or a disclosed wall lies on its nominal prefix
before the target in a fully disclosed ordinary local chart. This includes non-anchor body cells and
arbitrary cell bearings. After a definite veto only, they examine at most 16 unique
alternative disclosed target body-cell locations in canonical order. An alternate
must positively prove a complete clear prefix within any authored range; absence of
a veto is not proof of safety. The original bearing remains unchanged when it
has no definite veto. The shared decision-local chart reads disclosed occurrences,
not private target shape or future collisions, and introduces no random draw.

The chooser skips chart construction when every disclosed nonhostile body row
and wall lies outside the inclusive same-region box from the actor's origin to
the primary target cell. A canonical monotone prefix cannot meet such a row. This is only a
fast negative bound, not proof that a bearing is clear; every body row remains
eligible, and folded/aliased primary behavior and alternative checks are unchanged.

The conservative guard does not predict hidden bodies, post-target hits or
reflections. A known wall is a nominal straight-path veto, not a prediction of
whether a bounce could eventually hit. The entire prefix must still be disclosed:
a missing cell beyond a visible wall retains the original bearing. Conflicting
wall/clear rows, target-wall contradictions and ambiguous folded charts likewise
retain prior primary behavior and cannot justify an alternate. A safe endpoint beyond the cap may remain undiscovered.
Already prepared work retains its frozen aim and existing continuation policy.

Healing care recognizes a narrow safe plan: positive possible healing, with only
healing effects in the relevant branches, not any ability whose name resembles
healing. Finite stock and supply selection remain ordinary [item rules](items.md).
A harmful alternative branch cannot become a free self-care experiment.

Proactive defense likewise recognizes supported checkless self-condition plans,
not arbitrary beneficial-looking effects. It rejects unrelated side effects,
unsupported/reserved condition families and replacement/eviction cases that would
make the action unsafe. The current threat and effect eligibility are rechecked;
this is not a general utility search or optimal buff planner. The exact predicate
is owned by [ai_defense.rs](../../crates/simulation/src/ai_defense.rs).

## Conservative ranged positioning

Outside Flee, an engaged actor with a currently disclosed target may make one
legal step out of adjacent melee before attacking. This is capability-driven,
not keyed to a monster name: ordinary planning must admit an affordable,
currently usable actor-targeted sight-range ability whose authored preference
is outside melee. Unfunded or melee-only actors keep the ordinary attack policy.

The local guard supports upright column bodies and ordinary frame-preserving
movement. Destination body cells, the clear corridor and required solid footing
must already be disclosed in the appropriate local occurrence; a location alias
through a portal is not proof of support. It rejects unsupported orientation or
gravity, unknown support, and positions adjacent to any disclosed hostile body
cell, and preserves the admitted ability's effective range. Unsupported cases
fall back to the ordinary policy rather than reading hidden topology. The actor
replans next turn; this is not general kiting, a route planner or guaranteed safety.

## Limited two-step retreat

A strictly farther legal retreat retains priority. If none exists, a single-cell
actor with a local remembered chart may take an equal-distance first step when a
remembered second step leads farther from the licensed threat. This fallback
shares a budget of 512 newly expanded route nodes. Exhaustion is unknown, not
proof of escape; ordinary direct-retreat searches retain their existing behavior.

Both steps must preserve the local frame and ordinary displacement. Larger bodies,
folded or rotated transitions and unknown links are not certified by this fallback.
It chooses only the first action and replans afterward; stale memory or changed
occupancy can invalidate the opportunity. This is not a general planner,
multi-threat danger assessment, or guarantee of safety.

## Item decisions and bounded search

Known equipment is compared using disclosed usable alternatives and ordinary
capacity/removal rules. Each ordinary eligibility scan lazily shares one
actor-known baseline score, including an unavailable baseline, while independently
projecting every candidate and checking its equip admission. This scratch state
is bound to the immutable actor/world and is never serialized. Ranged acceptance
and final nondominated ranking remain separate policies.

Useful inventory pickup reserves finite need: appropriate
gear, one healing reserve and supplies for one payable safe healing use. Food
reserve depends on required nutrition. Gold interest is separately authored; it
does not grant theft or teach hidden magical effects.

Useful disclosed items and authored gold interest have no fixed gameplay
distance cutoff. Pursuit still requires a known reachable route, a safe goal and
first step, and the existing higher-priority needs; it is not a hidden-floor loot
oracle. Route search is finite over remembered topology, not a fixed CPU budget.
Projectile-source search scans at most 128
inventory rows and considers bounded launcher choices; no unbounded inventory
scan is implied by a generic projectile ability. Selection is deterministic at
its stable tie-breaks. See [item tactics](../../crates/simulation/src/ai_items.rs),
[projectiles](../../crates/simulation/src/ai_projectiles.rs) and
[finite supply selection](../../crates/simulation/src/supplies.rs).

These bounds are selection policies/capacity limits, not guarantees that every
eligible item deep in inventory will be found or that a chosen request will still
be valid at execution. Exact item ownership, holds and contact remain authoritative.

### Known medicine reserve

Collection recognizes a known restorative reserve separately from deciding to
consume it. Alongside legacy pure healing, a positive per-hit-die healing potion
may include removal of authored harmful conditions. Unknown identities, mixed
harm, unsupported effects, beneficial-condition removal and cure-only items do
not establish this healing reserve. The same classification governs ground
pickup and whether carried medicine already fills the one-dose reserve.

An uninjured care-capable actor may collect one known dose for later without
drinking it. Existing consumption thresholds and useful-effect checks remain
unchanged; collection grants no knowledge and adds no care or loot permission.

### Known consumable and scroll care

The independent `care` permission permits a bounded scan of at most 128 carried
inventory rows after the existing pure-healing priority and before food care.
It does not grant `equipment`, `loot`, anatomy, skills or interaction capabilities.
Items assigned at birth use the same decision path as items acquired later. Birth
possession never automatically identifies a type, reveals instance curses or
protection, or teaches device charges. Knowledge belonging to another observer
does not license the owner's choice.

The actor must know the item type and have a disclosed usable Drink/Read
interaction. Selection classifies its known authored effects, never its display
name or Rogue catalog identity. Every effect must have a supported safe shape;
one harmful or unsupported effect rejects the whole candidate. At least one
supported effect must also be useful now:

| Effect shape | Usefulness and safety gate |
| --- | --- |
| Positive Heal / HealPerHitDie | Health at or below 50%; a safe cure in the same payload may independently make it useful |
| RemoveCondition | The exact active condition has only recognized harmful mechanics, such as blindness, slow timing, action restrictions or hallucination; mixed beneficial modifiers are not discarded |
| RestoreAttribute / RestoreMaximumHealth | A positive amount and a matching actor-owned recorded deficit; a low stat alone is not a deficit |
| Timed safe condition / positive actor-turn duration | A currently visible hostile, an actual improvement over existing capabilities, no same-family or within-payload duplicate, enough condition capacity, no immunity-blocked descriptors or unsafe side effects |
| RemoveCurse on all equipment | An equipped item's remembered curse fact is true; hidden actual curse state is not a decision input |
| ProtectArmor on a selected armor socket | The selected eligible armor and its profile are understood, and its remembered protection is false; slot order is the ordinary first/last order |

Condition-shape and modifier-improvement analysis is shared with ability defense
in [ai_effect_safety](../../crates/simulation/src/ai_effect_safety.rs). Previewing
does not roll effect amounts, consume a unit, mutate knowledge or reserve resources.
An eligible candidate still goes through the ordinary planner and physical-source
checks. Ordinary work owns preparation, reservations, timing, interruption,
checkpoint restoration and eventual finite consumption. A remembered useful fact
may become stale; selection does not inspect hidden current facts to avoid that
ordinary outcome.

The currently supported live Rogue effect shapes include healing, extra healing,
restore strength, remove curse and protect armor. This is **not universal item
use**. Existing equipment, food and known-positive-charge hostile-device choices
remain separate policies. Unknown items are retained; identification prompts,
control/summoning scrolls, enchantment optimization, sensing/mapping/light utility,
relocation, polymorph, growth/hit-die restoration, risky haste doses and levitation
remain unsupported choices. See-invisible potions also remain unsupported because
their combined sensory effect is outside this conservative classifier, even when
another effect in that payload could cure blindness. A legally executable item
need not have a safe, useful autonomous decision policy yet.

## Execution-only refusal and recovery

The execution gate intentionally handles a small set of expected races:

- An action blocked by a current condition restriction on that action can become Wait.
- A selected projectile request that exceeds its numeric preparation budget can
  become Wait rather than inventing a different attack.
- An exact selected private item that is no longer available can become Wait;
  hidden alternatives are not substituted.
- Unrelated invariant/validation errors propagate and reject the candidate.
  “AI should keep moving” is not permission to swallow all errors.

A successful fallback still passes ordinary scheduling and publication. Partial
paid work retains its committed-prefix and recovery obligations; failed candidate
execution does not leak new engagement or RNG. Human and autonomous controllers
share the same capability, source and target gates.

<a id="current-retreat-limitation"></a>
## Retreat distance and limits

When a current route is known, flee compares **hop counts of stable
minimum-duration remembered routes** from the current position and each legal,
currently disclosed candidate. It does not mix accumulated movement time, local
coordinates or an invented `current + 1` into those scores. Candidate searches
start in the actual post-step location and body orientation. A remembered
**directed** graph with no return path from a known candidate to the otherwise
reachable threat gives that candidate unbounded distance. A candidate not yet
present in the remembered graph is unknown, not proof of an escape.

Current visibility does not require a retained navigation map. Without a current
route, a conservative local fallback compares Manhattan distances on both sides
of the comparison, but only when actor and threat share a region and the existing
physical disclosure chart within that region is unwrapped: every such occurrence
has its ordinary body-relative coordinates and unchanged frame. Candidate steps must likewise be
ordinary same-region displacements. Local mode rejects folded or rotated
occurrences within the actor’s region and does not use portal-crossing candidates
or cross-region coordinate comparisons. It neither inspects hidden links nor
teaches navigation during a decision. Disclosed occurrences in other
regions do not forbid ordinary local steps; a disclosed fold back into the
actor’s region does. It is a local heuristic, not a proof about undisclosed paths. A partial map never causes per-candidate metric
switching.

Only a strictly greater score qualifies; existing direction order resolves ties.
If neither metric is available, retained commitment, visible self-defense/basic
attack and eventual Wait retain their ordinary priority. First-step collision
and disclosure checks still apply; no reverse link or hidden topology is invented.

A separate frontier per candidate adds search work (at most ten candidate
origins); this is a correctness change, not a measured optimization or an optimal
escape planner.
The [regressions](../../crates/simulation/src/ai_flee_tests.rs) cover split/unsplit
corridors, rotated and one-way portals, blocked retreat, missing remembered
connections, no-map local retreat beside visible other-region portals, folded-chart
rejection, pure previews and checkpointed autonomous execution. The
[review record](../development/documentation-review.md#recommended-engineering-work)
retains the original source-only finding and its subsequent confirmation.

## Worked control-flow examples

- A paused preparation outside Flee resumes at row 3, even if carrying a healing
  potion. That priority preserves frozen work rather than recapturing an attack.
- An injured unprepared actor can heal before the flee branch. Flee is a state
  label, not a promise that movement always precedes self-care or erratic choice.
- A lost visible target can leave a last-seen pursuit location. Reaching that cell
  without redisclosure clears engagement; the actor cannot attack an imagined
  hidden occupant by private identity.
- A guised Xeroc remains waiting when another actor privately recognizes it.
  The readied touch branch requires actual touch, not recognition or adjacency.
- Gold under an actor can be useful while idle roaming is disabled, if the
  independent Gold policy permits it. A distant Gold detour still requires the
  appropriate movement policy and remembered route.

## Verification map

Source/test witnesses are static evidence here, not newly executed results.

| Contract | Owner |
| --- | --- |
| Pure choice and stable priorities | [ai.rs](../../crates/simulation/src/ai.rs), [decision tests](../../crates/simulation/src/ai.rs) |
| Retained attack matching, explicit selection modes and fresh fallback | [attack selection](../../crates/simulation/src/ai_attack_selection.rs), [direct probe tests](../../crates/simulation/src/ai_attack_selection_tests.rs) |
| Retreat metric/frame boundaries and distinct ranged spacing | [tactical movement](../../crates/simulation/src/ai_tactical_movement.rs), [frame tests](../../crates/simulation/src/ai_tactical_movement_tests.rs), [flee contracts](../../crates/simulation/src/ai_flee_tests.rs) |
| Idle visits, routes and permissions | [idle tests](../../crates/simulation/src/ai_idle_tests.rs), [hobgoblin host tests](../../crates/server/src/hobgoblin_roaming_tests.rs) |
| Retained exploration interests and save/replay | [exploration tests](../../crates/simulation/src/ai_exploration_tests.rs), [checkpoint tests](../../crates/simulation/src/checkpoint.rs), [streaming tests](../../crates/simulation/src/streaming.rs), [ordinary process tests](../../scripts/test_ai_interactions_process.py) |
| Awareness, normal contact and Stealth | [awareness](../../crates/simulation/src/awareness.rs) |
| Safe item/source selection | [ai_items](../../crates/simulation/src/ai_items.rs), [ai_projectiles](../../crates/simulation/src/ai_projectiles.rs) |
| Defense predicate | [ai_defense](../../crates/simulation/src/ai_defense.rs), [shared condition safety](../../crates/simulation/src/ai_effect_safety.rs) |
| Known-item care, remembered facts and paid replay | [item-care tests](../../crates/simulation/src/ai_item_use_tests.rs); includes live Rogue effect/condition subsets, not a whole-scenario playthrough |
| Candidate execution/fallback | [actions](../../crates/simulation/src/actions.rs), [intention ownership](../../crates/simulation/src/intention.rs) |

### Disclosed bystanders and gold pursuit

Existing directly reachable gold choices keep their prior ordering. If none is
usable and a disclosed creature blocks the first remembered step, a gold seeker
may choose a disclosed legal first step with a clear remembered suffix instead
of falling back immediately to exploration. Alternatives use movement-time cost,
exclude the origin and disclosed occupied creature cells, and share a 512-node
search budget across gold targets. Incomplete comparisons decline the new
alternative. This is not hidden-occupancy pathfinding or a guarantee of future
clearance; each ordinary move rechecks current conditions. Authored Wait, exposure
and condition restrictions remain. Greedy acquisition and subsequent Search reuse
one positive or negative gold decision. Other loot policies are unchanged.

### Recovering a disclosed ranged kit

Existing useful-loot choices retain priority and ordering. Only when that pass
finds no action may an exploring actor with both equipment and loot permission
recover a known launcher/payload pair around a disclosed creature blocking its
first remembered step. Pair eligibility and the chosen pickup or movement share
one completed approach cache. This prevents a split pair from alternately drawing
an actor home and becoming ineligible again there.

New detours use legal disclosed first steps and remembered suffixes avoiding the
origin and disclosed occupied creature cells. Destinations share a 512-node
expansion budget; an incomplete comparison discards all newly inferred detours.
The existing known-family pairing, finite ammunition reserve, anatomy, exposure
and action restrictions still apply. This does not add interests to creatures
without the relevant permissions, change care or ordinary equipment priorities,
or promise that future steps remain clear.

### Extension boundaries and decision-local state

The coordinator in `ai.rs` remains an explicit first-applicable priority pipeline,
not a global utility contest or dynamically registered behavior tree. Its order
is the table above. Fresh-care and safe-acquisition stages
live in [decision stages](../../crates/simulation/src/ai_decision_stages.rs).
[Exploration](../../crates/simulation/src/ai_exploration.rs) owns the bounded idle
interest and the established local fallback.
[Encounter stages](../../crates/simulation/src/ai_encounter.rs) own engagement
refresh and combat proposals. The coordinator visibly places retained-work
resumption, care, erratic movement, retreat and committed-prefix fallback before
engagement proposals, and anonymous investigation afterward. Encounter facts
retain sensed-target identity separately from remembered threats and alerts;
refresh updates only the copied drafts before care, in the established order.

The decision-local context is deliberately small and existing: one immutable
`DecisionSnapshot` (observation, sight and scene), actor-owned remembered route
search, a named positive-or-negative `GoldChoiceCache`, and one copied AI/behavior draft.
Read-only predicates may use the acting body's capabilities and present legality;
they must not inspect an opponent's private inventory or undisclosed future
occupancy. New stage helpers receive only the context they need, not a new
unrestricted knowledge store. The structural stage boundaries do not add another rules representation. The
exploration destination is explicitly serialized actor memory, not a global goal
registry or unrestricted blackboard.

A stage proposes an ordinary action or a frozen evaluated plan. It does not
execute, consume resources, draw world randomness or commit memory. The winning
proposal attaches the existing draft to `AiDecision`; ordinary execution owns
validation and commitment. Exploration may update its draft destination, visits and copied
actor-owned RNG. None becomes authoritative until ordinary commit. A stage that declines must preserve any
intentional draft effects already specified by its contract. Do not re-evaluate
an earlier predicate merely because a later stage needs its result: cached
negative gold decisions are as important as positive ones.

Hostile technique selection has its own [owner](../../crates/simulation/src/ai_attack_selection.rs),
separate from encounter state and tactical movement. Its named modes distinguish
ordinary engagement, cornered basic-only fallback, and a reposition eligibility
probe. The last may inspect an outside-melee capability while adjacent; it does
not execute that capability or choose movement. Granted candidates retain authored
priority and stable identity order, with known finite devices as the final allowed
fallback. Do not add combinations of mode flags implicitly.

Retained attack probing distinguishes a selected frozen plan, ordinary mismatch
that permits fresh selection, and an unsupported retained shape that stops the
selector. Mode eligibility is checked before aim matching. A matching but no-longer
available plan permits fresh selection. Retained bearing matching uses its legacy
disclosed geometry, not the fresh wall/ally alternate search: sharing that fresh
selector could silently recapture already-paid aim. The probe borrows the existing
plan; only the selected result is cloned. It adds no resource admission, cache,
random draw or serialized state.

[Tactical movement](../../crates/simulation/src/ai_tactical_movement.rs) owns retreat
and ranged-spacing proposals, not encounter priority or commitment. Retreat's
metric explicitly selects remembered route hops or canonical local coordinates
for the entire decision; equal scalar values do not make those units comparable.
Its borrowed local frame names coordinate evidence only, not legal occupancy,
support or execution. Frame construction performs no eager chart scan. Route
searches, the shared plateau budget and its destination cache stay in the policy.
A missing direct return path may prove unbounded graph distance; a failed or
exhausted bounded plateau query cannot prove escape. Preserve this distinction.
Ranged spacing retains its separate funded-technique, footprint, gravity,
footing, disclosure and range guards rather than sharing a generic move score.

Direct loot classification has a small decision-local `DirectLootNeeds` owner in
[ai_items](../../crates/simulation/src/ai_items.rs). It answers quantity only;
exposure checks, Wait semantics, approach construction, action legality and final
distance/item ranking remain visible in the chooser. Ranged pair quantities are
still computed first by their existing reachability-aware scan, so this is not
fully navigation-independent desire. One lazy known-gear baseline is reused
across ground rows. Preserve ranged-first quantity clamping, the original
gear/healing/food/supply evaluation order, one-unit precedence over supply needs,
and saturating carried-supply aggregation. This boundary adds no new cache,
interest, item interpretation or acquisition policy.

To add an interest or tactic, first choose its explicit priority and eligibility
policy, then implement its bounded decision in the relevant stage. Keep desire,
quantity and known-item interpretation separate from navigation and execution.
Reserve recognition and immediate consumption remain different predicates. Add
policy-level precedence, knowledge, budget and pure-choice tests plus ordinary
execution/replay coverage. New behavior needs its own rules-identity review;
structural extraction alone does not authorize changed priorities or RNG draws.

[Known equipment evaluation](../../crates/simulation/src/ai_equipment.rs) owns
remembered estimates, private loadout projection and score comparison. Acquisition
policy stays in `ai_items.rs` and ranged-item selection. Ordinary and ranged
comparisons share matching-socket order, known replacement eligibility and the
reset → carried-unit proposal → ordinary Equip admission → derived-fold mechanism.
They do not share acceptance: ordinary upgrades require strict dominance; ranged
policy may accept equality and its explicitly preferred attack trade-off, with
its own resource check. Candidate rejection remains distinct from a missing item
that aborts the ranged comparison. Each candidate revalidates admission; only the
immutable ordinary baseline score is cached within its actor-bound scan.

### Shared approach mechanism, separate acquisition policies

Gold and ranged-kit recovery share the knowledge-only
[approach planner](../../crates/simulation/src/ai_approach.rs). It enumerates legal
first steps in the existing horizontal/up/down order and compares remembered
suffixes by checked movement cost, then direction rank. It distinguishes an
exhausted comparison from a completed search with no safe approach. Exhaustion
invalidates the caller's entire fallback, including earlier successful targets.

Target ordering and budgets remain policy-owned: gold visits sorted item IDs,
including co-located items, and ranks `(cost, item, direction rank)`. Ranged kits
visit sorted unique destinations, reuse their completed approaches for pairing,
and rank pickups by `(hops, item)`. The planner deliberately does not deduplicate
or cache targets, unify direct-route handling, or reorder the AI coordinator.
This extraction does not change rules identity or actor behavior.

The gold idle tests retain exact-budget, repeated-destination, ordering,
disclosure/frame, direct-route and occupied-suffix witnesses. Ranged
[weapon-selection tests](../../crates/simulation/src/abilities/weapon_selection_tests.rs)
retain destination-cache, incomplete-budget, knowledge, exposure, policy and
occupied-goal witnesses. The ordinary
[AI process tests](../../scripts/test_ai_interactions_process.py) retain paid
multi-actor collection and restart coverage. These tests remain policy-level
contracts rather than depending on the helper's internal representation.


For example, an additional known equipment interest belongs in safe acquisition:
its item policy decides usefulness and finite quantity from actor-owned knowledge;
its approach mechanism determines only how a disclosed eligible target can be
reached. It must retain the stage's equipment/loot/gold ordering unless a behavior
change is explicitly intended, return a proposal rather than move the actor,
and decline a bounded comparison completely on exhaustion. Tests should include
an unknown item, a disclosed bystander, two equal-ranked targets, and an earlier
care or committed-work choice that must still win. This is an extension recipe,
not an implemented new interest.

### Short exploration interests

Actors already permitted to Explore can retain one short destination instead of
reranking adjacent cells after every step. This adds continuity, not a guard post,
patrol assignment or new creature lore. Wait profiles remain stationary unless an
independent higher-priority behavior applies; switching to Wait clears the goal.

New targets come only from currently disclosed non-wall cells, excluding the
actor and disclosed creature body cells. Sort and deduplicate by visit count and
location, then sample one candidate from each visit tier before another from the
same tier, with at most 16 attempts total. Endpoint visit count remains primary.
Among equal endpoint tiers, prefer the lower exact mean of actor-owned visits on
route-interior cells, excluding the endpoint; integer cross-products compare the
means without floating-point rounding. Ties retain the first sampled choice.
A valid higher-tier destination remains provisional while lower-visit tiers remain
within the allowance. Higher tiers cannot improve it; equal tiers can improve its
interior score. Only a minimum-tier candidate with zero interior visits ends the
comparison early. Each sampled tied tier has a lazy cyclic actor-RNG start; a
unique first-tier success draws nothing. Accept a remembered route of 3–12 steps.
The score compares ordinary shortest routes, not longer alternative paths to one
goal. Proposal draws commit only if a destination is selected;
failed selection preserves local fallback RNG. This bounded preference is not
global least-visited reachability optimization. In particular, unreachable
zero-visit air seen by a walking creature cannot monopolize every attempt and
exclude useful visited floor cells. No absolute-height or walking-only target
filter overrides portals, stairs or flight.

Continuation recomputes remembered navigation, stores no path, and rechecks the
actual next move against disclosed location/frame and present body legality.
Known occupied suffixes are rejected. No future undisclosed obstacle or occupant
is inspected. Selection and continuation permit at most 512 additional route-node
expansions in the decision's shared search. After that allowance is spent, later
candidates may still use routes already settled by the same search. This does not
cap work already done by
earlier priorities or route reconstruction. An impossible, blocked, overlong or
budget-exhausted interest is abandoned, then local exploration runs without
immediately choosing another destination. Exhaustion is a policy cost limit,
not evidence that the goal is unreachable.

At the remembered destination, the idle stage clears its draft and proposes one
ordinary Wait. This is a pause, not an active search action or a promise of safety.
Higher-priority combat, care, anonymous investigation and acquisition retain
their order and suspend the interest. Successful erratic movement is an explicit
exception: it releases the draft destination rather than resuming the old idle
interest afterward. A declined erratic draw leaves that destination intact.
Direction choice and RNG draws remain the erratic stage's existing contract;
release becomes authoritative only when its ordinary move commits. Current-cell
visits count eligible exploration decisions, including arrival pauses, not just
entries into a cell. A later departure can count another decision at that cell.
The destination survives checkpoints and frozen/detached regions; forgotten
navigation affects the next choice rather than invalidating a save. Structural
validation checks that its location is in the world's known bounds, not whether
it is currently traversable or unoccupied.

Retained continuation is unchanged: it does not abandon a valid destination
merely because an adjacent cell has fewer visits. Route-interior scoring applies
only when choosing a new goal. Following remembered routes can still revisit
cells, including unavoidable bottlenecks. These policies do not establish maximum
cell coverage, a complete purpose system or an all-roster balance/performance
guarantee. Historical checkpoint measurements and follow-up comparisons must
remain separate evidence; neither policy description claims measured improvement.
