# Rogue adaptation: remaining work

The goal is Rogue's generated-dungeon, survival and descent/retrieval/ascent
experience using **shared TOR mechanics**. The [pinned Rogue reference](../research/rogue-5.4.4/README.md)
is evidence for content and behavior intent, not a second combat/timing engine or
an executable-parity commitment. [Current status](../status.md) owns scope;
[shared requirements](requirements.md) own reusable systems.

## Adaptation decisions

- Descent has no preset final floor, as in the pinned reference. Compact lazy
  descriptors use an explicit operational depth limit for safe execution; that
  limit is not a Rogue victory condition or a seed-selected dungeon length.
- Floors persist on revisits; no regeneration or ascent-specific loot suppression.
- Nine regions form a 3×3 floor, stone-filled with broad portal joins; fine-grained
  declarative stages carve rooms/corridors. Region boundaries need not be walls.
- Generate a complete floor group within the preparation/loading horizon before
  traversal requires it; distinguish speculative preparation, publication and
  active residency. Use independent stage streams and pinned versions.
- Named paired stairs connect corresponding endpoints. Backtracking is generally
  allowed; gates require separate scenario design. Falling through a portal is
  physics, not a stair action. Invalid arrivals consume no movement time.
  Stair locations and the initial entry vary deterministically with the seed.
  Returning to a paired stair deliberately differs from Rogue's separately
  selected arrival cell after regenerating each floor.
- Reproduce nine room slots, connecting corridors, omissions, darkness, secrets
  and mazes as their shared stages become supported. Required routes may contain
  discoverable secrets. Ordinary room entrances have no usable doors.
- Preserve Rogue's 26 species, item catalog and distinctive capabilities as
  standard TOR content, with scenario-restricted generation pools. Species are
  constructed from types, subtypes, HD and eligibility-gated talent trees and
  tuned in the arena for comparable behavior, not copied XP or combat equations;
  see the [progression design](progression-design.md#rogue-roster-as-constructed-creatures).
  Do not label fixed-damage placeholders complete species.
- TOR owns timing, progression, capacity, AI knowledge, physics and spatial sight.
  Exact Rogue combat numbers, action cycles, RNG, regenerated floors, room-wide
  revelation, capacity and scoring are not required parity targets.
- Use shared nutrition/healing, anatomy equipment, NetHack-style character
  knowledge, reusable conditions and ranged actions. Per-item unusual rules,
  including scare-monster scrolls, need individual decisions.
- Hallucinated identities persist per observer with occasional timed shifts.
  Population advances on active floors, including revisits, without frozen-time
  catch-up. Gold/item pursuit requires actor knowledge.
- There is exactly one Amulet of Yendor identity. On every actual player entry
  to depth 26 or deeper, including revisits, create it once if absent or relocate
  that same item to a random valid supported/reachable ground cell on the entered
  floor unless the player carries it. Existing piles, the entrant's cell and
  reselecting its current cell are valid outcomes. Queries, speculative preparation,
  NPC entry and frozen time never trigger placement. Depth 26 is an eligibility
  threshold, not the final floor. Intentional surface traversal wins only while
  carrying it; refusal without it costs no time. Internal backtracking never
  requires the Amulet.
  The objective and intentional surface escape are implemented; expanded
  presentation/scoring remain open.

## Implemented foundations and canonical owners

The generated persistent-floor foundation, complete 26-species selector, Warrior/
Mage starting kits and ordinary advancement, nutrition, discovery/traps, control
items, current loot and unique retrieval/ascent objective are implemented.
Mechanism availability does not establish a calibrated survival curve.

| Concern | Current owner |
| --- | --- |
| Launch, floors and objective | [Rogue play](../play/rogue.md) |
| Encounters, supplies, senses and survival | [Rogue mechanics](../play/rogue-mechanics.md) |
| Combat/conditions | [Combat](../reference/combat.md), [creature rules](../reference/creature-rules.md) |
| Builder and owned progression | [Progression](../reference/progression.md), [Polymorph](../reference/polymorph.md) |
| Physical/equipped/magical items | [Items](../reference/items.md), [equipment](../reference/equipment.md), [magic](../reference/magic-items.md) |
| Missing-consumables design decisions | [Design record](rogue-missing-consumables.md), [current effects](../reference/consumable-effects.md) |
| Population, stock and generated layouts | [Packages](../reference/scenario-packages.md#live-rogue-content-policy), [generation](../reference/generation.md) |
| Observation, discovery and tactics | [Perception](../architecture/perception.md), [AI](../reference/ai.md) |

## Remaining acceptance and design

- Calibrate coupled food, Health/resource recovery, XP, loot and depth-selected
  encounters against a locked reference build, following the [tuning plan](rogue-tuning.md).
- Establish useful ordinary earned builds and pressured descent/retrieval/ascent
  across both presets. A mechanism fixture, controlled opponent or one surviving
  prefix is not proof of autonomous full recovery or balanced starter survival.
- Preserve bounded comparator identities. Later content activation does not
  silently widen the frozen eighteen-foe earned-route study.
- Consider capacity/containers, richer item/presentation/scoring and runtime
  generation extensions only as separate approved increments. Do not relabel
  implemented Hallucination/discovery/items as absent because older plans did.
- Keep source-free persistence, client/native behavior, upper operational guards
  and measured scaling separate from content existence and numerical balance.

### Monster birth inventory and capable item use

Roadmap scope accepted on 2026-11-02; implementation and publication are separate
checkpoints. Keep this work separate from the item-visibility/identity change.
Match the pinned Rogue 5.4.4 distribution of monster-carried items, while using
ordinary TOR inventory and capability-gated AI rather than death-only rewards.

**Starting gap (before this milestone).** Successful supply rolls produced
one ration, not the ordinary random item distribution. Centaur, yeti, troll,
vampire, griffin, medusa, xeroc, jabberwock and dragon have birth-stock policies;
nymph and orc lacked their reference policies. Leprechauns already carried
finite Gold at birth. Existing AI supports selected care/equipment/loot behavior,
not a demonstrated ability to use every otherwise usable generated item.
Birth ownership must not itself grant knowledge, anatomy, skills or tactics.

**Implementation under qualification.** The source pool now has all 79 ordinary
subtypes using exact category-times-subtype weights, and the missing carry
policies are present. Generic equip-one and conservative known-item care use
ordinary actions. Daggers generate 2–5 units and arrows
use the shared weapon curse/enchantment recipe; all other floor-loot semantics
are retained as distributions, not as an identical seeded sequence: normal
condition/charge generation and item allocation remain shared. Quantity ranges
remain authored on loot entries, not item defaults.
The [package contract](../reference/scenario-packages.md) documents carrier food
history and its deliberate separation from unchanged ground-food safety.
Completion still requires the acceptance below; source tests alone are not
runtime or full-CI qualification.

**Reference distribution.** Use the [pinned monster and item tables](../research/rogue-5.4.4/content.md),
not a new uniform selection or the provisional ration pool:

| Species | Chance of one ordinary item-generation result |
| --- | ---: |
| Nymph, dragon | 100% |
| Jabberwock | 70% |
| Troll | 50% |
| Medusa | 40% |
| Xeroc, yeti | 30% |
| Griffin, vampire | 20% |
| Centaur, orc | 15% |
| All other species | 0% ordinary-item chance |

Conditional category weights are potion 26%, scroll 36%, food 16%, weapon 7%,
armor 7%, ring 4%, wand/staff 4%. Preserve the reference conditional subtype,
stack quantity, charges, enchantment and curse distributions through shared item
generation; one generation result can be a stack, not necessarily one unit.
The source item generator adds no depth gating to categories or subtypes. Do not
filter or reroll birth items for usefulness, combat value or positive effects;
nymph birth stock is not restricted to magical items. Capability gates subsequent
use, not the generation distribution.
Gold and the unique Amulet are not ordinary categories. The source food-drought
override can force food instead of the category roll and resets its generation
counter; this is not the receiving monster's hunger or a guaranteed extra item.
Record the mapping to TOR's existing food-drought owner before implementation,
including whether monster-born food contributes to that counter. In the source,
normal room monsters are created before the new-level counter increment, while
floor items and treasure-room monsters follow it; test this ordering explicitly.

Source anchors: [`new_thing`](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/things.c#L217-L299),
[`give_pack`](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/monsters.c#L218-L222),
and [treasure-room births](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/new_level.c#L218-L230).

**Eligibility and explicit adaptations.** Trace `give_pack` at normal room and
treasure-room births, including treasure-room effective depth, and its
`level >= max_level` guard; later wandering births do not call it. Preserve that
initial-versus-wandering distinction. The [accepted persistent-floor adaptation](#adaptation-decisions)
already rejects regenerated floors and ascent-specific loot suppression: do not
silently import a new deepest-floor suppression rule. Document this difference
from the source guard, and never reroll inventory on revisits, save/reopen or
speculative preparation. Cover other creation paths explicitly rather than
assuming every spawn inherits an initial-population rule.

Keep the approved leprechaun exception: finite Gold belongs to its inventory at
birth instead of being created by a special death reward. Preserve the existing
birth-floor quantity rule and do not also grant ordinary random stock or duplicate
Gold on death. Later theft/pickup changes the same ordinary inventory; consumed,
spent, stolen or dropped units cannot reappear when the carrier dies.

**Capable AI use.** Let mobs equip and use their birth items through the same
capability-checked actions as other acquired items: weapon/anatomy and equipment
slots, skills and targeting, nutrition and safe consumption, scroll/device use,
charges, reservations, timing and interruption all remain authoritative. Extend
missing AI choices only where the actor can legally use the item, using disclosed
knowledge rather than hidden identity, magic or curse facts. Ineligible items
remain carried; they do not confer abilities or force use. This is an intentional
TOR adaptation, not a claim that Rogue 5.4.4 monsters use every carried item.

**Acceptance before completion.** Add deterministic boundary tests for all 26
carry policies, conditional category/subtype and quantity tables, food-drought
behavior and zero/guaranteed chances; retain independent expected values rather
than sampling a distribution as sole proof. Exercise initial, treasure-room and
wandering births, effective-depth/deepest-depth boundaries and the documented
persistent-floor adaptation. Use real-process witnesses that inspect ownership
before death, equip/use a capable mob's item, reject an incapable/unknown-item
choice, exhaust finite stock, and recover only the remaining identical items on
death. Include ordinary/player controls, later pickup/theft, source-free reopen,
revisit/no-restock, rollback and owner/spectator knowledge boundaries. Requalify
changed gameplay and report effects on survival tuning separately; preserve
historical fixtures and do not retune unrelated populations to hide regressions.

### Deferred deep-stat capacity

The current 256 actual-HD admission and detailed-inspection guards are accepted
for now and do not block functional Rogue. Exact counts, owners, aggregate Health,
threat and XP use checked 64-bit domains; ordinary summaries use compiled
per-source counts, while saved and detailed records retain every actual HD.
Weighted finite/open population intervals and wide scalar fields do not remove
the row guards or establish support beyond them.

Exact paged HD storage, archive and inspection work and eventual guard removal
are deferred to a separate approved increment. That work must preserve every
actual HD record, individual Health seed and earned choice through saves,
inspection, rollback and replay. A counted-HD suffix, averaged Health or silent
clamp cannot substitute for it. Continue to report current capacity refusal
atomically rather than inventing a terminal floor.

## Historical sequence

Completed stage diaries, detailed checkpoint counts and retained fixture budgets
remain in the [reviewed pre-reorganization plan](https://github.com/KCall360/thresholds-of-ruin/blob/dacbc215e12aaa13ef3cf89900836a296127f074/docs/plans/rogue.md).
Useful design decisions remain above and exact current contracts have one owner.
