# Coexisting planes

A package can declare planes besides the material one, such as an Ethereal
Plane that is the same place as the material world but a different world to
move through. The [planes plan](../plans/planes.md) records the design and the
milestones still to come; this page describes what is implemented.

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
regions = [40]
overlaps = [{ from = 2, min = [0, 0, 0], max = [5, 4, 1], to = 40, origin = [0, 0, 0], rotation = 0 }]
derive = [{ from = 1, id = 1001 }, { from = 3, id = 1003, volume = { min = [0, 0, 0], max = [3, 2, 1] } }]
```

| Field | Meaning |
| --- | --- |
| `mysterious`, `revealed` | Descriptions for observers that do and do not know the plane, 1–240 bytes. Not yet disclosed to clients. |
| `lore_dc` | Lore difficulty to recognize the plane, 0–100. Not yet used. |
| `gravity`, `lit` | Gravity and lighting of derived regions on this plane. Authored regions keep their own. |
| `movement_modes` | At most one; the only mode is `ether_glide` (below). |
| `regions` | Authored regions on this plane. |
| `overlaps` | Authored overlaps: a box `min`..`max` (inclusive) in region `from`, landing with `min` at `origin` in region `to`, rotated by `rotation`. |
| `derive` | Regions built from an authored material region (below). |

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

## Performance

A game without planes keeps no plane state and saves exactly as before. Three
work counters record plane activity, and a single-plane game records none of
them: `plane_shifts` (attempted shifts, including blocked ones),
`plane_glide_checks` and `plane_coactivated_regions` (regions added to a
residency set by an overlap group).

## Verification

- World: [`crates/world/src/planes.rs`](../../crates/world/src/planes.rs) tests
  mapping in both directions under all 24 rotations, rejection and the
  one-counterpart rule.
- Simulation: [`crates/simulation/src/planes_tests.rs`](../../crates/simulation/src/planes_tests.rs)
  covers shifting, rejection, returns, expiry, retried forced returns, the
  movement mode's checks, co-activation and checkpoint round trips.
- Engine: [`crates/server/tests/it/planes.rs`](../../crates/server/tests/it/planes.rs)
  plays [`scenarios/tests/planes`](../../scenarios/tests/planes) through a
  scripted shift, ether-gliding, the return action, a consumable
  `plane_shift` and replay, and checks derived-region validation.
