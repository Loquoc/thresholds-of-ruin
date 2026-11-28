# Coexisting planes

A package can declare planes besides the material one, such as an Ethereal
Plane that is the same place as the material world but a different world to
move through. The [planes plan](../plans/planes.md) records the design and the
deferred work; this page describes what is implemented.

## Model

- Every region is on one plane. Undeclared regions are on the material plane.
- An **overlap** asserts that a box of one region and a volume of a region on
  another plane are the same place. It maps positions through a translation and
  one of the 24 proper cube rotations. Overlaps are structural: they are kept
  whatever the residency of the regions they name.
- A cell has at most one counterpart on each other plane. Overlaps whose volumes
  would give a cell two counterparts on one plane are rejected, as are overlaps
  between two regions on the same plane.
- Overlaps never compose. A shift follows exactly one declared overlap.
- Overlaps need not agree with joins. Divergent layouts are allowed.
- An overlap has a `scale`, reserved for scaled overlaps; only `1` is accepted.

The world API is [`crates/world/src/planes.rs`](../../crates/world/src/planes.rs).

## Declaring planes

Planes are declared in the manifest. Region files are unchanged.

```toml
[planes.ethereal]
mysterious = "a grey, sourceless mist in which walls are only suggestions"
revealed = "the Border Ethereal"
lore_dc = 14
gravity = [0, 0, 0]
lit = true
movement_modes = [{ mode = "ether_glide", check = { skill = "discipline", dc = 15 } }]
channels = [{ to = "material", range = 12 }]
regions = [40]
overlaps = [{ from = 2, min = [0, 0, 0], max = [5, 4, 1], to = 40, origin = [0, 0, 0], rotation = 0 }]
derive = [{ from = 1, id = 1001 }, { from = 3, id = 1003, volume = { min = [0, 0, 0], max = [3, 2, 1] } }]
```

| Field | Meaning |
| --- | --- |
| `mysterious`, `revealed` | Descriptions for observers that do not and do know the plane, 1–240 bytes. |
| `lore_dc` | A Lore difficulty for scripts to check on arrival, 0–100. The engine does not roll it. |
| `channels` | Perception channels to other planes (below). |
| `cross_plane_reach` | Species whose members on this plane strike across its overlaps (below). |
| `gravity`, `lit` | Gravity and lighting of derived regions on this plane. Authored regions keep their own. |
| `movement_modes` | At most one; the only mode is `ether_glide` (below). |
| `regions` | Authored regions on this plane. |
| `overlaps` | Authored overlaps: a box `min`..`max` (inclusive) in region `from`, landing with `min` at `origin` in region `to`, rotated by `rotation`. |
| `derive` | Regions built from an authored material region (below). |
| `derive_families` | Procedural families whose every region is derived onto this plane (below). |

Plane names are lowercase ASCII letters, digits, `_` and `-`, at most 64 bytes;
`material` is reserved. A package declares at most 32 planes. Plane identities
are assigned in name order.

### Derived regions

A derived region is built from its source region when first needed. It has the
source's interior size and anchors, empty terrain with no shell, the plane's
gravity and light, and copies of the source's portals whose destinations are
also derived on the same plane. It overlaps its source over `volume`, or the
whole interior when `volume` is omitted, with the identity transform. Once built
it is an ordinary, independent region: later edits to its source do not reach
it.

A derived region's ID must not collide with an authored region or a procedural
family's range, and a derived region cannot be derived from another. In this
version the source must be an authored region.

### Derived generated families

`derive_families = ["dungeon"]` derives every region of a procedural family
onto the plane. The family's [identity block](generation.md#identity-blocks)
reserves one shadow span after its own regions for each plane that derives it,
in plane-name order. The shadow of the family's `i`-th region is the `i`-th
region of the span: an empty open room of the same size, on the plane, with the
plane's gravity and light, overlapping its source over the whole room. The
world stores each span as one overlap range, so an open family's unbounded
address space needs no per-region overlap list. Joins that a generation group
derives from its grid are not copied, so a family shadow is one room: an
ethereal traveller there can shift back but not glide into the next room.

### Overlap groups

Regions connected through overlaps form an **overlap group**. A group is active
whenever any member is active, and loaded whenever any member is loaded, so the
same place never runs on two clocks. Validation rejects groups of more than 16
regions.

## Shifting

A shift moves an actor's whole body, unscaled, to the counterpart of its
reference cell, with its frame composed with the overlap's rotation. Carried and
equipped items follow their carrier. A shift with no counterpart, to the plane
the actor is already on, or whose body would not fit at the destination, is
**rejected**: nothing moves, and scripts hear `plane_shift_blocked`.

An actor that leaves its home plane gets a **stay** recording the home plane,
how it may return and when it expires. Returning to the home plane ends the
stay. A second shift onward keeps the original home.

| Return policy | Meaning |
| --- | --- |
| `action` (default) | The `return_plane` action, an ordinary timed action, is available. |
| `expiry_only` | Only expiry returns the actor. |
| `none` | No return is offered; the stay lasts until another effect ends it. |

A stay with a duration ends at the actor's first action boundary at or after it
expires. A blocked forced return keeps the stay, and the next boundary tries
again. A voluntary return into a place the body does not fit is refused without
spending time.

Shifts come from three sources:

- The `plane_shift` effect, usable wherever content lists effects, for example
  a consumable:
  `consumable = { effects = [{ kind = "plane_shift", plane = "ethereal", returns = "action", duration = 2000 }] }`.
  It is applied after the rest of its batch resolves.
- Scripts, with `ctx:shift` (see [scripting](scripting.md)).
- The `return_plane` action, for an actor whose stay offers it.

## Movement modes

A plane's `ether_glide` mode lets anyone on it who does not fly move without
support in any direction. Each ordinary move first rolls the mode's check, a
skill check against its difficulty through the same path as hazard checks
(Discipline uses Willpower). A failed check spends the move's time and leaves
the actor in place, like a confused stay. An actor that flies by itself uses its
flight, which needs no check, and keeps the plane's mode if it loses flight.
Bodies without creature statistics pass the check.

## Perception channels

A channel is one-way. An observer on the declaring plane also runs an ordinary
eye pass on plane `to`, from the counterpart of its eye cell and in its own
frame composed with the overlap's rotation, within `range` Manhattan cells
(1–16). The other plane's lighting is ignored within that range. Walls there
block the pass as they would block a viewer standing there, and an eye whose
counterpart is solid sees nothing on that side. An observer with no counterpart
for its eye sees nothing through the channel. `perception = "mirror"` and
`light = "fixed"` are the only values and the defaults.

Echoes are separate from the observer's own sight. They are not part of its
visible cells, memory, awareness or AI decisions, and a creature disguised as an
object is not shown in them.

## Striking across planes

Members of a species listed in a plane's `cross_plane_reach` perceive what that
plane's channels show them as creatures standing at their counterpart cells:
they become ordinary visible actors, awareness subjects and targets for that
observer, so its AI hunts and strikes them. Melee contact runs from the
striker's counterpart placement on the target's plane; across planes the same
place also counts as contact, since nothing stops a body from standing where
its victim's counterpart stands. Cross-plane contact grants strikes only:
displacement such as a push never crosses planes.

A **glimpse** is a channel scoped to one target. While `observer` glimpses
`target`, it perceives and may strike `target` as above. A glimpse is opened
by scripts (`ctx:open_channel`) and closes when `target` next acts. Glimpses
are saved with the game.

A strike across planes by a creature that does not reach across by its nature
is **intangible** unless its weapon is magical: it lands and spends its time,
but its damage is skipped. A weapon is magical when its item is authored with
`magical = true` or has a nonzero hit or damage enchantment, judged on the
item's actual state whether or not the wielder knows it. Natural attacks are
never magical.

Whenever a strike from one plane hits a creature on another, scripts hear
`cross_plane_struck`, with the striker as `actor` and the victim as `target`.
A typical reveal opens a glimpse on a Perception check:

```lua
function haunting.struck(ctx, facts)
    for _, fact in ipairs(facts) do
        if ctx:check(fact.target, "perception", 15) then
            ctx:open_channel(fact.target, fact.actor)
        end
    end
end
```

Derived regions hold no authored actors; a plane's natives live in authored
regions on it. The [haunted chapel](../../scenarios/haunted-chapel) is the
example: an authored Ethereal nave overlapping the material nave holds a ghost
that hunts and strikes the pilgrim across the planes, and a vestry outside the
overlap is safe.

## Plane knowledge

An observer either knows a plane or does not; the material plane is always
known. Knowledge is saved with the game and only grows. It is learned when a
shift with `reveals = true` arrives, or through `ctx:reveal_plane`. A typical
arrival check is a script on `plane_shifted`:

```lua
function veil.shifted(ctx, facts)
    local fact = facts[1]
    if fact.plane ~= "material" and not ctx:knows_plane(fact.actor, fact.plane)
        and ctx:check(fact.actor, "lore", 14) then
        ctx:reveal_plane(fact.actor, fact.plane)
    end
end
```

Knowledge only changes which description the observer is told.

## Disclosure

In a game with planes, every observation carries a `plane` view (see
[protocol](protocol.md#coexisting-planes)): the observer's current plane and an
echo for each channel. Each plane is named by an opaque per-observer key and the
description the observer knows; the material plane is described as "the
material world". Echo cells carry ordinary cell keys, so memory of a plane seen
through a channel lines up with the same cells when the observer stands there.
Games without planes send no plane view.

- The text client names a plane other than the material one, and lists the
  figures glimpsed through each channel with their bearing.
- The ASCII map draws echo walls (`#`) and figures (the first letter of their
  name) in muted colors, only in columns its own map leaves empty.

## Performance

A game without planes keeps no plane state and saves exactly as before. Three
work counters record plane activity, and a single-plane game records none of
them: `plane_shifts` (attempted shifts, including blocked ones),
`plane_glide_checks` and `plane_coactivated_regions` (regions added to a
residency set by an overlap group).

Channel passes run only for observers on a plane with channels, once per
channel per observation.

## Verification

- World: [`crates/world/src/planes.rs`](../../crates/world/src/planes.rs) tests
  mapping in both directions under all 24 rotations, rejection and the
  one-counterpart rule.
- Simulation: [`crates/simulation/src/planes_tests.rs`](../../crates/simulation/src/planes_tests.rs)
  covers shifting, rejection, returns, expiry, retried forced returns, the
  movement mode's checks, co-activation, channel views, solid-eye blindness,
  plane knowledge and checkpoint round trips.
- Engine: [`crates/server/tests/it/planes.rs`](../../crates/server/tests/it/planes.rs)
  plays [`scenarios/tests/planes`](../../scenarios/tests/planes) through a
  scripted shift, the plane view and its echo, ether-gliding, the return
  action, a consumable `plane_shift` and replay, and checks derived-region
  validation.
- The engine test also plays the haunted chapel: the ghost strikes across the
  planes, the arrival and glimpse scripts run, and a glimpsed ghost becomes a
  target. Simulation tests cover cross-plane awareness rows, co-located
  contact, glimpse lifetime and saves, intangible strikes and magical items.
- [`scenarios/tests/planes-generated`](../../scenarios/tests/planes-generated)
  starts a run on the shadow of a generated room and returns home.
- Clients: `crates/client-ascii/tests/it/planes.rs` and the text client's
  `adventure` tests cover the echo layer and plane sentences.
