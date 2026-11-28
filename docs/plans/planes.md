# Coexisting planes

**Status: accepted and implemented.** This plan records the design
decisions for coexisting planes (the Ethereal Plane first; later the Plane of
Shadow, Shadesmar and others) and the generator identity change they depend on.
The maintainer authorized implementing the plan, including the format and
protocol version changes it needs and the compatibility break in
[milestone 4](#generator-identity-blocks) (removing `base_region`). All four
milestones are implemented; the [planes reference](../reference/planes.md) and
[generation reference](../reference/generation.md#identity-blocks) own their
behavior. Implementation choices that differ from the text below: cross-plane
reach is declared on the plane per species (`cross_plane_reach`) rather than as
a creature ability; the same place counts as contact across planes; natives
live in authored plane regions, because derived regions hold no actors; the
ghost's glimpse uses `cross_plane_struck`; a family's entity identities follow
from its region block (each region's 256-wide range) rather than a separately
requested entity block; family shadows are stored as overlap ranges and copy
no group joins; and the assigned blocks are recorded in each save's package
copy rather than a separate table.

## Model

- A **plane** is a set of regions with shared properties: descriptions, ambient
  light, gravity defaults, movement modes and channels.
- An **overlap** asserts that a volume of one region and a volume of another
  region are *the same place* on different planes. It is a topology relation in
  `crates/world`, beside joins, not a portal and not a teleport effect.
- A **channel** declares what crosses an overlap. Every channel is off unless
  declared.
- An entity is on exactly one plane at a time. Its correspondence on another
  plane is the image of its reference point.

Overlaps reuse the existing [geometry](../architecture/geometry.md) machinery:
the 24 proper rotations, transform validation and straddling-body agreement.

### Overlap transforms

An overlap is a similarity map: a rigid transform (translation plus one of the
24 rotations) and an integer uniform scale `k >= 1`.

- The correspondence is continuous. Every point of one volume corresponds to
  exactly one point of the other. A coarse cell and a `k x k x k` block of fine
  cells are two subdivisions of one place; neither is "the" partner.
- Positions already have 65,536 fixed-point units per cell, so the image of an
  actor's position is exact in the expanding direction and deterministically
  rounded in the contracting direction. Cell centres map to centres; for even
  `k`, ties break toward the minimum corner in the overlap's own frame, so the
  result is rotation-independent.
- Integer `k` keeps cell boundaries aligned, and uniform scale commutes with
  rotation. Rational ratios and per-axis scale are possible later.
- Bodies do not scale. A shift places the unscaled body at its reference
  point's image.
- The format carries `scale` from the first milestone. The compiler rejects
  `k > 1` until scaled overlaps are implemented as their own increment.

Overlaps may cover partial volumes. One region may carry many overlaps, and
overlaps may be many-to-one. There is **no compile-time agreement check**
between overlaps and joins: correctness does not depend on it, and divergent
layouts (distorted correspondences, rooms that are adjacent only on the other
plane) are a goal. A body straddling a seam can shift only if every cell's
mapping agrees on one rigid placement, as for any straddling body; otherwise
the shift is rejected. Cells outside every overlap have no counterpart, and a
shift from them is rejected.

Shifts follow **declared overlaps only**. Overlaps never compose implicitly: an
Ethereal-to-Shadow shift needs its own declared overlap.

### Authored and derived planes

A scenario chooses per plane:

- **Authored** regions, written like any region, with authored overlaps.
- **Derived** regions, built from a material region when first needed: the
  same bounds, empty terrain, copied joins and an identity overlap, plus sparse
  authored overrides. Once built, a derived region is an ordinary independent
  region; later wizard edits to its source do not propagate. This is the default
  derivation.
- **Virtual** derivation, where bounds and joins are computed from the source on
  every query and only overrides and occupants are stored, is an available
  option, not the default.

## Residency

An **overlap group** is the connected component of regions linked by overlaps.
A group is loaded and activated as one unit: if any member is active, all are.
This keeps the "no observer sees frozen time" rule true when the same place
exists twice. The compiler rejects a group larger than **16 regions**, as a
package capacity limit. Derived regions use the existing support for
never-built regions, so they are built on first need.

## Shifting

`plane_shift` is an effect payload granted by abilities, items and spells:

```toml
plane_shift = { to = "ethereal", on_blocked = "reject", return = "action", duration = 3000 }
```

- `on_blocked` defaults to `reject`: the shift fails and nothing moves. Other
  resolutions are declared per effect; see [Luau](#luau-boundary).
- A shift applies a status recording the actor's **home plane**. Returns always
  target the home plane, so nested shifts do not stack.
- `return` is `"action"` (default: an ordinary timed return action),
  `"expiry_only"`, or an explicit `"none"`. The compiler rejects no voluntary
  return together with no duration, unless the effect says `return = "none"`.
- With no duration, a shift never ends on its own.
- Native creatures have no status; their home is their plane.
- Carried and equipped items shift with their carrier, and a failed shift moves
  nothing. Loose items stay on their plane, keep their own motion, and cannot be
  seen or taken from another plane. Items with no carrier shift only when an
  effect targets them.

## The Ethereal Plane

These are the first plane's settings, not universal plane rules.

- **Gravity:** zero.
- **Light:** a plane-wide ambient level with authored overrides, set so natives
  are visible at short range.
- **Movement mode:** the plane grants *ether-gliding*, a movement mode distinct
  from flight. It moves the actor without support in any direction. Each move
  requires a Willpower + Discipline check against DC 15; on failure the action
  time is spent and the actor does not move. Creatures that also fly prefer
  flight, which needs no check, and keep ether-gliding if their flight is lost.
  A move uses the cheapest mode that works. Natives get authored flight.
- **Perception channel:** one-way, Ethereal to material, mode B (below), fixed
  light level, range **12** Manhattan cells. Ordinary sight is 16 cells and 6,017
  candidate offsets; 12 is 2,625.
- **Interaction:** none by default. Cross-plane strikes are a creature ability
  (see [the ghost](#the-ethereal-ghost)).
- **No physical coupling:** material walls, floors, traps and doors do not
  affect Ethereal movement.

### Movement modes

Movement modes are a general mechanism: an actor has a set of modes (walk, fly,
plane-granted modes), and a plane declares which modes it grants and with which
checks. The plane rule does not create or remove flight, so flight needs no
source tracking.

## Perception channels

Mode B, the only mode in the first version: the observer also runs an ordinary
perception pass on the other plane from the image of its eye cell, filtered by
the channel's range cap and fixed light level. Walls on the other plane block
it, as they would block a viewer standing there, and an eye inside solid
material sees nothing on that side. Channel mode, light rule and range are
declared per channel, so later planes can choose others (see-through, source
lighting).

Through a scaled overlap the range counts the observed plane's cells, and the
protocol reports the scale so clients can draw the echo layer as an inset
rather than overlaying it cell for cell.

A channel can also be **scoped to one target** and opened by a status. This is
how the ghost's reveal works, and later covers see-invisible and true-seeing
effects without a separate disclosure mechanism.

## Disclosure and plane knowledge

This needs a protocol version bump.

- Each occurrence carries an opaque per-observer **plane key**, derived from the
  private saved salt like cell keys. A key is sent only when it differs from the
  observer's current plane, so single-plane games send what they send today.
- Each observation states the observer's current plane key.
- Each plane has a **mysterious** and a **revealed** description. Knowing a
  plane is a remembered per-observer fact, persisted and replayed like
  [item knowledge](../reference/item-knowledge.md); spectators receive the
  mysterious description. Knowledge is descriptive only in the first version.
- Knowledge comes from three sources: the shifting effect's `reveals` flag, a
  Lore check on arrival against a plane-authored DC (one check per arrival,
  tracked on the home-plane status), and `reveal_plane` effects.
- Cell keys differ per plane, so material keys seen through the channel align
  client memory across a shift; a shift is not a discontinuity.
- Text and ASCII draw other-plane occurrences as a separate layer that never
  changes the current plane's map.

## The Ethereal ghost

An authored native that uses general mechanisms:

- Native authored flight; home plane Ethereal; ordinary AI that stays on its
  own plane.
- `cross_plane_reach`: its body cells are mapped through its overlap and the
  shared melee contact query runs on the target's plane, with visibility from
  its channel sight. It strikes only. Pushing across planes needs a separate
  ability. Scaled overlaps never permit cross-plane contact until scaled contact
  is designed (the mapped body volume would exceed the contact query's bound).
- Its attack ability declares its own reveal: on a hit, the victim makes a
  Perception (Awareness) check against DC 15. Success opens a channel scoped to
  the ghost for that victim until the ghost's next action. A miss reveals
  nothing.
- While revealed, the ghost can be struck through the same mapped contact by a
  weapon for which the derived item property `is_magical` holds: a nonzero
  `hit` or `damage` enchantment, or an authored `magical` flag, judged on the
  item's actual state. A non-magical strike is allowed, spends its time and
  passes through. A successful strike is a learning event that can reveal an
  unidentified weapon is enchanted.

## Luau boundary

[Luau scripts](../reference/scripting.md) handle plane-specific reactions so the
engine stays general. Handlers react at settlement points after actions and as
time advances; they never decide mid-action.

| Engine (declared data) | Luau (hooks, queries, effects) |
| --- | --- |
| Overlaps, channels, light, gravity | Hooks: `plane_shifted`, `plane_shift_blocked`, `cross_plane_struck` |
| Movement modes and their per-step checks | Queries: `ctx:plane(actor)`, `ctx:knows_plane(actor, plane)`, `ctx:check(actor, skill, dc)`, `ctx:nearest_fit(actor, plane, cell, radius)` |
| Perception passes | Effects: `ctx:shift(actor, plane, cell)`, `ctx:reveal_plane(actor, plane)`, `ctx:open_channel(observer, target, { ticks?, until_action_of? })` |
| Shift validation and the reject default | Lore check on arrival, `reveals`, shunts and other blocked-shift outcomes |
| The voluntary return action | Expiry-only and timed returns (timer plus `ctx:shift`) |
| Cross-plane contact and `is_magical` | The ghost's reveal (on `cross_plane_struck`, which fires on a hit) |

A synchronous resolver call on `HookHost`, letting a script decide how a shift
resolves before the action completes, may be added if a feature needs it. It
must stay off per-step paths.

## Example scenario: the haunted chapel

A small package in which the player meets ghosts attacking from the Ethereal
Plane. It exercises every first-version mechanism and is the intended
acceptance scenario for milestone 3, as `scenarios/haunted-chapel`.

**This is a sketch, not valid current input.** Plane fields, `cross_plane_reach`,
`plane_shift`, the `magical` flag and the new hooks do not exist yet. Existing
syntax (species modifiers, melee strikes, regions, Luau hooks) follows the
current [package format](../reference/scenario-packages.md); plane values such
as light levels are illustrative.

### Layout

| Region | Plane | Contents |
| --- | --- | --- |
| 1 Lychgate | Material | Start and exit. No Ethereal counterpart, so the player cannot shift here |
| 2 Chapel | Material | The nave (west part) touches the Ethereal; the vestry (east part) does not. A cold-iron dagger lies in the vestry |
| 3 Crypt | Material | The reliquary (objective), a tarnished dagger (unidentified, `hit = 1`) and a censer of veil incense |
| 1002 Grey nave | Ethereal | Derived from the nave only (a partial overlap). Two ghosts |
| 1003 Grey crypt | Ethereal | Derived from the whole crypt. One ghost |

Both derived regions are built from their material regions and get author-declared
IDs, as milestone 1 requires. The nave and crypt form two overlap groups of two
regions each, well under the 16-region limit.

### Manifest additions

```toml
# scenario.toml (sketch)
[planes.ethereal]
mysterious = "a grey, sourceless mist in which walls are only suggestions"
revealed = "the Border Ethereal, which lies against the waking world"
lore_dc = 14
gravity = [0, 0, 0]
ambient_light = "dim"            # illustrative: visible at short range
movement_modes = [{ mode = "ether_glide", check = { skill = "discipline", dc = 15 } }]
channels = [{ to = "material", perception = "mirror", range = 12, light = "dim" }]
derive = [
    { from = 2, id = 1002, volume = { min = [0, 0, 0], max = [5, 4, 1] } },  # nave only
    { from = 3, id = 1003 },                                               # whole crypt
]

[creatures.species.ghost]
kind = "undead"
home_plane = "ethereal"
attributes = { strength = 0, speed = 0, intellect = 0, willpower = 2, awareness = 2, presence = 1 }
displacement = { size = "medium", anchored = false }
cross_plane_reach = { strikes_only = true }
modifiers = [
    { scope = "always", op = { kind = "locomotion", value = { kind = "flying", enabled = true } } },
    { scope = "always", op = { kind = "add", stat = { kind = "health" }, value = 6 } },
]
# The chilling touch: a natural strike against Spiritual defense.
melee = { skill = "light_weaponry", bonus = 0, wind_up = 60, recovery = 100, strikes = [{ offset = 0, check = { actor = "user", skill = { kind = "natural_attack" }, against = { kind = "defense", defense = "spiritual" } }, on_success = [{ kind = "damage", damage = { kind = "components", components = [{ category = "spirit", amount = { kind = "rolled", dice = { count = 1, sides = 4, bonus = 1 } } }] } }], on_failure = [], descriptors = [], riders = [] }] }

[archetypes.cold_iron_dagger]
identity = "item.weapon.dagger"
name = "cold-iron dagger"
magical = true                   # harms the Ethereal without numeric enchantment

[archetypes.veil_incense]
identity = "item.misc.veil-incense"
name = "veil incense"
# Burning it shifts the user; returning is an ordinary action.
on_use = [{ kind = "plane_shift", to = "ethereal", on_blocked = "reject", return = "action", duration = 2000, reveals = false }]

[scripting]
scripts = ["scripts/haunting.luau"]

[[scripting.hooks]]
on = "region_first_visit"
region = 2
handler = "haunting.cold_air"
once = true

[[scripting.hooks]]
on = "cross_plane_struck"        # proposed hook: a cross-plane strike hit
handler = "haunting.struck"

[[scripting.hooks]]
on = "plane_shifted"             # proposed hook
handler = "haunting.arrived"
```

### Script

```lua
-- scripts/haunting.luau (sketch)
local haunting = {}

function haunting.cold_air(ctx, facts)
    ctx:message("The chapel is colder than the night outside.",
        { style = "narration", to = facts[1].actor })
end

-- A ghost's touch: the victim may glimpse it until the ghost acts again.
function haunting.struck(ctx, facts)
    for _, fact in ipairs(facts) do
        if ctx:check(fact.target, "perception", 15) then
            ctx:open_channel(fact.target, fact.actor, { until_action_of = fact.actor })
            ctx:message("For a heartbeat you see it: a pale shape in the air beside you.",
                { style = "narration", to = fact.target })
        else
            ctx:message("Something unseen and cold passes through you.",
                { style = "narration", to = fact.target })
        end
    end
end

-- One Lore check per arrival on a plane the actor does not know.
function haunting.arrived(ctx, facts)
    for _, fact in ipairs(facts) do
        if not ctx:knows_plane(fact.actor, fact.plane)
            and ctx:check(fact.actor, "lore", 14) then
            ctx:reveal_plane(fact.actor, fact.plane)
        end
    end
end

return haunting
```

The reveal lives in the scenario's script, not the species, so another package
can give the same ghost a different reveal or none.

### What the player experiences

1. **The nave.** The player walks in with an ordinary sword. Ghosts on the
   Ethereal see them through their channel and strike across the planes with
   their chilling touch. Each hit gives a Perception check (DC 15): on success
   the player glimpses the ghost until its next action and may strike back; on
   failure the hit comes from nowhere. A sword swing at a revealed ghost spends
   its time and passes through.
2. **The vestry.** It lies outside the overlap, so the ghosts cannot reach the
   player there: a safe room. The cold-iron dagger here is `magical` and harms
   revealed ghosts.
3. **The crypt.** The tarnished dagger is unidentified. Striking a revealed
   ghost with it does damage, which teaches the player it is enchanted.
4. **The veil incense.** Burning it shifts the player onto the Ethereal Plane.
   They see the mysterious description ("a grey, sourceless mist") and make a
   Lore check (DC 14) that may reveal its name. On the Ethereal Plane the ghosts
   are ordinary visible creatures and any weapon works, but every move needs a
   Willpower + Discipline check (DC 15) while the ghosts fly freely. The
   material crypt appears as a dim echo up to 12 cells away. Returning is an
   ordinary action; it fails, and leaves the player Ethereal, if they are
   standing where the material crypt is solid rock.
5. **The way out.** Carry the reliquary back to the lychgate to win, by the
   usual outcome script.

### Acceptance witnesses

Headless process tests drive most of this scenario:

- Cross-plane strikes in the nave, not in the vestry (partial overlap).
- Reveal success and failure, the reveal window ending at the ghost's next
  action, and non-magical strikes passing through.
- `magical` and numeric-enchantment weapons both harming revealed ghosts, and
  the enchantment learning event.
- Shifting with the incense, failed and successful ether-gliding checks
  (including during automatic travel), the Lore check on arrival, returning by
  action, and a return rejected inside rock.
- Save, rewind and retry while the player is Ethereal and while a ghost is
  revealed.
- Co-activation: the ghosts in the grey nave act whenever the player is in the
  nave.

Text and ASCII process tests cover what those clients present: the echo layer,
the glimpse narration and the plane descriptions.

## Milestones

1. **Plane core on authored regions.** Planes, overlaps in `crates/world`
   (rigid, `scale` field reserved), authored and derived regions, overlap
   groups, `plane_shift` and home-plane status, return policies, movement modes
   with ether-gliding, ambient light, item rules, the first Luau hooks, queries
   and effects, plane work counters, and the no-regression gate. Authored
   regions have explicit IDs; authors declare their derived regions' IDs.
2. **Perception and protocol.** Mode B channels with range caps, target-scoped
   channels, plane keys sent only when needed, descriptions and knowledge, and
   the text and ASCII layers.
3. **Ethereal ghost.** `cross_plane_reach`, the scripted reveal, `is_magical`,
   strikes without pushes, and enchantment learning, accepted through the
   [haunted chapel](#example-scenario-the-haunted-chapel) scenario.
4. **Generator identity blocks.** See
   below. This brings derived planes to generated families, including Rogue.

### Generator identity blocks

Each generator defines how it assigns IDs inside blocks the engine gives it:

- A generator requests a **region block size** and an **entity block size**,
  covering every plane it owns. It numbers its regions and their derived
  regions, and maps regions to entity sub-ranges, inside its blocks. A
  hyperbolic generator, for example, can rank canonical tile addresses within
  its block.
- A region's owner assigns its derived regions' IDs.
- The contract for every scheme: deterministic (from package, seed and logical
  address, never build order), injective, and confined to its blocks. The
  engine guarantees disjointness between generators.
- Blocks are assigned in sorted order of namespace-qualified generator names,
  above the authored ID space. The table is recorded in the compiled package
  index, bound by the validation certificate, and copied into each save. The
  save's table is authoritative and **append-only**: a new generator gets a new
  block after existing ones, and a generator that grows gets an additional
  extent rather than a moved block.
- `base_region` is removed. Authored references to generated regions use
  logical addresses (`family/1/start`) rather than numbers. Existing packages,
  fixtures and certificates are migrated. This replaces the current rule that
  generated actors and items take 256 identities fixed by a region ID of at most
  65,536 ([generation](../reference/generation.md)).

## Verification and performance

- Every milestone keeps existing workloads free of regressions, checked with
  `scripts/perf_compare.py`. Plane-specific branches sit behind a scenario
  capability switch.
- Work counters (shifts, channel passes, co-activated regions), modelled on the
  scripting `hook_*` counters, record nothing in a single-plane game and give a
  handle for optimization. Add plane cases to the
  [performance workloads](../development/performance.md), including empty
  co-activated regions and a dense native population before milestone 3.
- Most behavioral coverage uses the [headless client](../reference/headless.md):
  shifts, fit failures, ether-gliding checks (including during automatic
  travel), the reveal window, return policies and save, rewind and retry during
  a stay on another plane. Text and ASCII process tests cover what those
  clients present: the other-plane layer and plane descriptions.
- Follow the [testing policy](../development/testing.md) for contract ownership
  and witnesses.

## Deferred

- Scaled overlaps (format reserved), then scaled cross-plane contact.
- AI that shifts or follows targets across planes.
- See-through channel mode and source-lit channels.
- Physical coupling (for example, material floors supporting Ethereal bodies).
- Mechanical effects of plane knowledge.
- A read-only report of authored overlaps that disagree with joins.
- Shadesmar and other divergent planes.
- A hyperbolic generator, which needs bounded canonical numbering.
- Material traps and doors affecting Ethereal travellers.
- A synchronous script resolver, if a feature needs one.
