# World residency API checklist

This is the bounded member checklist for the world residency documentation pass.
It supplements the [project-wide API inventory](rust-api-coverage.md); it does not
mark all of `tor-world` complete. API-local contracts remain in the source/rustdoc
owners below, rather than being repeated here as a second specification.

## Reviewed members

The pass covers the named public members, including fields and variants. Standard
derived traits are covered by their type's ownership/ordering/serialization notes
where those semantics matter. Internal invariants explicitly covered are region
key ordering, empty-owner copy-on-write preservation, and attachment validation
before mutation.

| Owner | Reviewed public members | Contract focus |
| --- | --- | --- |
| [Coordinate types](../../crates/world/src/lib.rs) | `Position::{x, y, z}`; `Extent::{dimensions, origin, new, contains, with_shell}` | Cell axes/units, half-open bounds, finite shell arithmetic, raw deserialization versus validation |
| [Region and link types](../../crates/world/src/topology.rs) | `RegionId` and tuple field `0`; `Location::{region, position}`; `NamedAnchor::{region, name}`; `Region::{id, name, bounds}`; `Passage::{from, direction, to}` | Frame ownership, known versus loaded identity, name syntax, payload/source ownership |
| [Directions](../../crates/world/src/topology.rs) | All 18 `Direction` variants; `HORIZONTAL`, `components`, `delta`, `from_delta`, `rotated` | Exact supported vectors, ordered components, no normalization, cube-rotation domain |
| [World errors](../../crates/world/src/topology.rs) | All five `WorldError` variants | Method-specific rejection categories, without inventing actor permission or persistence semantics |
| [Construction and loaded bounds](../../crates/world/src/topology.rs) | `World::{new, add_region, add_chamber, region, contains}` | Validated metadata, loaded-ID rejection versus incomplete absent-ID replacement, ordinary regions versus chamber shells, loaded storage versus walkability |
| [Known metadata and lifecycle](../../crates/world/src/region_slice.rs) | `World::{knows_region, knows, add_unbuilt_region, known_region, loaded_regions, detached_regions, publish_region_anchors, detach_region, attach_region}` | Absent includes unbuilt, retained metadata/anchors, payload ownership, errors and unchanged-on-rejection behavior |
| [Region slice](../../crates/world/src/region_slice.rs) | `RegionSlice::{id, region, linked_regions, door_ids, traps, trap_ids, valid}` | Backend-only payloads, deterministic iteration, local structural checks versus complete host validation |
| [Dependency and landing queries](../../crates/world/src/region_slice.rs), [topology](../../crates/world/src/topology.rs), [anchor lookup](../../crates/world/src/concealed.rs) | `World::{linked_regions, required_linked_regions, register_named_anchors, connect_named_stair, stair_candidates_in_region, stair_region, link_destination, named_anchor, exits}` | Structural versus coordinate-resolved dependencies, directed source ownership, stable anchor publication, iterator grouping |

The `World` type overview is also reviewed, particularly deserialization and
copy-on-write/cache semantics. Its other inherent methods are not implicitly
included. A helper implemented in a private module is still public API when it
is an inherent method of the exported `World` type.

## Executable witnesses

The examples use public APIs and assert independent expected outcomes:

- `Extent`: shell bounds and unchanged coordinate frame, exclusive upper edge,
  rejected nonpositive dimensions and shell arithmetic overflow.
- `Direction::from_delta`: exact unit-vector matching, rejected normalization and
  three-axis corners, ordered components and rotation into the expected axis.
- `World::connect_named_stair`: an unbuilt destination, unresolved versus resolved
  loading dependency, coordinate publication before terrain, successful attachment,
  and immutable-anchor rejection without changing the world.
- `RegionSlice`: incoming links retained while outgoing links travel with the
  slice, loaded versus known coordinates, serialized round trip, restored equality
  and closed-door behavior.
- `World::attach_region`: a deliberate loaded-door identity conflict rejected
  without mutation, retaining a clone for a successful alternative candidate.

Existing independent behavior witnesses remain in
[region bounds](../../crates/world/tests/it/region_bounds.rs),
[passages](../../crates/world/tests/it/passages.rs),
[rotations](../../crates/world/tests/it/rotations.rs),
[geometry snapshots](../../crates/world/tests/it/geometry_snapshot.rs), and the
owning source tests for named stairs and loading dependencies. Listing an owner
is not a claim that every test was freshly rerun or that an example replaces it.

## Remaining world work

Terrain/door/trap operations, ray/sight/lighting and physics/reach queries,
checkpoint pools, shared wrappers, cache witnesses and their remaining fields,
variants and validation paths still need their own member checklists. Preserve
reference sight implementations and unique negative/oracle tests during that
review. Similar-looking helpers are not established redundant code.

Run affected doctests, public/private rustdoc with warnings denied, formatting,
links and the risk-based checks required by [testing policy](testing.md). Review
newly discovered health candidates in the [code-health register](code-health.md)
with evidence before describing them as reproduced defects or changing behavior.
