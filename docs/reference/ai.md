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
| Visit history | Saved per-location exploration counts; pure choice does not increment authoritative counts |
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
no route ranks after reachable candidates. The [known-space routing algorithm](../architecture/perception.md#known-space-pathfinding-and-its-limits)
owns edge costs, deterministic search and path reconstruction. Mean actors with positive memory can
acquire eligible opponents directly, with **no separate wake lottery**. Passive
actors need ordinary provocation, except readied touch ambush. Continual Stealth
still gates awareness. Engagement refresh needs a matching current reference;
reaching last-seen position without seeing the target clears the engagement.

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
| 16 | Idle exploration | Search + Explore + voluntary input: increment proposed current-cell visits, choose legal disclosed neighbor by fewest visits, breaking equal-best ties with the copied actor-owned RNG |
| 17 | Fallback | Wait through ordinary action execution |

Stable location ordering defines the candidate list, not a preferred compass direction.
Only a tie draws randomness; a unique best neighbor and an idle Wait do not.
This removes shared coordinate bias without introducing persistent goals or a planner.

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
does not identify them.

Fresh ray, device, launcher and equipped-throw choices decline a bearing when
a disclosed nonhostile body cell intercepts its prefix before the target in a
fully disclosed ordinary local chart. This includes non-anchor body cells and
arbitrary cell bearings. The conservative guard does not predict hidden bodies,
post-target hits or reflections; ambiguous folded charts retain prior behavior.
Already prepared work retains its existing continuation policy.

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
| Idle visits, routes and permissions | [idle tests](../../crates/simulation/src/ai_idle_tests.rs), [hobgoblin host tests](../../crates/server/src/hobgoblin_roaming_tests.rs) |
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
is the table above. Contiguous fresh-care, safe-acquisition and exploration stages
live in [decision stages](../../crates/simulation/src/ai_decision_stages.rs).
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
unrestricted knowledge store. Nothing adds serialized memory or another rules
representation.

A stage proposes an ordinary action or a frozen evaluated plan. It does not
execute, consume resources, draw world randomness or commit memory. The winning
proposal attaches the existing draft to `AiDecision`; ordinary execution owns
validation and commitment. Exploration may update draft visits and its copied
actor-owned RNG, in the existing order. A stage that declines must preserve any
intentional draft effects already specified by its contract. Do not re-evaluate
an earlier predicate merely because a later stage needs its result: cached
negative gold decisions are as important as positive ones.

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
