# Rust API documentation coverage

This is the working inventory for the source-documentation program, not a claim
that every public item is documented. The [engineering reading paths](documentation-coverage.md)
cover concepts across the project; rustdoc owns API-local contracts. The
[code-health register](code-health.md) tracks defects, investigation candidates
and cleanup decisions discovered while following those contracts.

The initial inventory was reconciled against `a442ea4b` on 2026-11-03. Reconcile
changed exports and methods before each batch. A source file's `pub` count is not
an API coverage percentage: private modules expose re-exported types and inherent
methods, macros generate identities, and test modules contain their own items.

## Coverage states and completion rule

- **Inventoried:** source/API family identified; item-by-item review remains.
- **Partial:** specific contracts/examples improved, with remaining work named.
- **Complete:** every reachable public type, field, variant, function, trait and
  associated item reviewed; meaningful errors/panics/units/ownership documented;
  examples and links qualified. Name the reviewed source boundary and evidence.
- **Retained exception:** an individually explained omission, not a broad
  `doc(hidden)` or lint allowance used to inflate coverage.

No family is complete merely because its module introduction exists. Inspect
public methods implemented in private modules, root re-exports and generated
items. Standard trait implementations need explanation where their semantics
differ from a reader's expectation, such as copy-on-write allocation identity.
Do not expand obvious getters into repetitive tutorials.

For each completed family retain a concise member checklist: public path, source
owner, contract status, example/test witness and unresolved issue IDs. Keep
unfinished member work visible here or in the linked register. Only enable
missing-documentation denial once the actual surface is covered.

## Whole-project inventory

| Source family | Status | Remaining review |
| --- | --- | --- |
| [World](../../crates/world/src/lib.rs) | Partial | Complete exported topology, region lifecycle, sight/lighting, ray, serialization and ownership contracts |
| [Simulation](../../crates/simulation/src/lib.rs) | Partial | Complete the module/export inventory below and public methods implemented outside the crate root |
| [Protocol](../../crates/protocol/src/lib.rs) | Partial | Bounded decoding/assembly, envelope clocks, disclosed handles, direction, schema versus semantic validation |
| [Server](../../crates/server/src/lib.rs) | Partial | Authority/dedup, scheduler callbacks, publication/durability, storage, compilation and operational error boundaries |
| [Client common](../../crates/client-common/src/lib.rs) | Partial | Transport/state ownership, recovery, completion, page reducers, retained pictures and disclosed affordances |
| [Text](../../crates/client-text/src/lib.rs) | Partial | Parsing/resolution, clarification, execution chains, prompt readiness, narrative evidence and reset retention |
| [ASCII](../../crates/client-ascii/src/lib.rs) | Partial | Worker/reducer ownership, modal controls, coordinates/hit tests, rendering and capture completion |
| [Headless](../../crates/client-headless/src) | Inventoried | JSON-lines lifecycle, exit/failure handling and receipt versus completed gameplay |
| [Test support](../../crates/test-support/src/lib.rs) | Public contract pass | Performance workload members documented below; future schema policy tracked as CH-010 |
| [Optional 3D](../../crates/client-3d) | Partial | Separate model/application workspaces, live/review clocks, resource ownership, async identity, coordinates and native evidence |

The optional 3D application remains a separate workspace. Its own documentation
and doctests need explicit qualification; a root-workspace pass does not cover it.
Scripts, scenarios, CI and format adapters also belong to the code-health review,
even when they are not rustdoc surfaces.

## Simulation public module inventory

The crate root exposes 56 public modules at the initial baseline. Each row below
includes every named module in that family; its public nested modules and
re-exports remain part of the same review. All rows are **inventoried** unless
the foundation notes below identify a narrower completed contract.

| Family | Public modules | Contract focus |
| --- | --- | --- |
| Identity and arithmetic | [content_ids](../../crates/simulation/src/content_ids.rs), [limits](../../crates/simulation/src/limits.rs), [hd](../../crates/simulation/src/hd.rs), [attributes](../../crates/simulation/src/attributes.rs), [dice](../../crates/simulation/src/dice.rs), [quantity](../../crates/simulation/src/quantity.rs) | Typed namespaces, syntax versus existence, units, bounds, rounding, RNG ownership |
| Content and construction | [rules](../../crates/simulation/src/rules.rs), [creatures](../../crates/simulation/src/creatures/mod.rs), [expertise](../../crates/simulation/src/expertise.rs), [prepared_spawn](../../crates/simulation/src/prepared_spawn.rs) | Raw versus validated definitions, source ownership, immutable recipes, derived/live state |
| Advancement | [progression](../../crates/simulation/src/progression.rs), [earned_growth](../../crates/simulation/src/earned_growth.rs), [player_advancement](../../crates/simulation/src/player_advancement.rs), [player_respec](../../crates/simulation/src/player_respec.rs) | Body/personal ownership, preview freshness, retained loss, payment and restoration |
| Execution | [abilities](../../crates/simulation/src/abilities.rs), [attacks](../../crates/simulation/src/attacks.rs), [combat](../../crates/simulation/src/combat.rs), [damage](../../crates/simulation/src/damage.rs), [conditions](../../crates/simulation/src/conditions.rs), [cooldowns](../../crates/simulation/src/cooldowns.rs), [costs](../../crates/simulation/src/costs.rs), [action_duration](../../crates/simulation/src/action_duration.rs) | Frozen/live operands, preparation/recovery, payment, claims, atomicity and clocks |
| Living state | [health](../../crates/simulation/src/health.rs), [health_recovery](../../crates/simulation/src/health_recovery.rs), [nutrition](../../crates/simulation/src/nutrition.rs), [resources](../../crates/simulation/src/resources.rs) | Actor-active time, dead/frozen behavior, saturation and returned lifecycle consequences |
| Equipment | [equipment_capacity](../../crates/simulation/src/equipment_capacity.rs), [equipment_capture](../../crates/simulation/src/equipment_capture.rs), [weapon_profiles](../../crates/simulation/src/weapon_profiles.rs), [projectile_profiles](../../crates/simulation/src/projectile_profiles.rs), [supplies](../../crates/simulation/src/supplies.rs) | Slots/grips, exact selections, stacks, knowledge, finite spending and birth-only policy |
| Behavior and motion | [ai](../../crates/simulation/src/ai.rs), [behavior](../../crates/simulation/src/behavior.rs), [control_scrolls](../../crates/simulation/src/control_scrolls.rs), [linked_hold](../../crates/simulation/src/linked_hold.rs), [escape](../../crates/simulation/src/escape.rs), [theft](../../crates/simulation/src/theft.rs), [locomotion](../../crates/simulation/src/locomotion.rs) | Knowledge-limited choices, priority, revalidation, bodies and source/target ownership |
| Disclosure | [perception](../../crates/simulation/src/perception.rs), [appearance_catalog](../../crates/simulation/src/appearance_catalog.rs), [appearance_state](../../crates/simulation/src/appearance_state.rs), [discovery](../../crates/simulation/src/discovery.rs), [detection](../../crates/simulation/src/detection.rs), [public_plan](../../crates/simulation/src/public_plan.rs), [chart](../../crates/simulation/src/chart.rs), [chart_history](../../crates/simulation/src/chart_history.rs) | Observer-scoped facts, fresh state versus cached geometry, presentation versus authority |
| Residency and policy | [spatial_scope](../../crates/simulation/src/spatial_scope.rs), [scope_projection](../../crates/simulation/src/scope_projection.rs), [floor_activation](../../crates/simulation/src/floor_activation.rs), [unique_retrieval](../../crates/simulation/src/unique_retrieval.rs), [run_policy](../../crates/simulation/src/run_policy.rs), [arena](../../crates/simulation/src/arena.rs) | Canonical sources, load/activation, private settlement, receipts, independent random streams |
| Persistence and diagnostics | [checkpoint](../../crates/simulation/src/checkpoint.rs), [diagnostics](../../crates/simulation/src/diagnostics.rs), [resolution_diagnostics](../../crates/simulation/src/resolution_diagnostics.rs), [survival_diagnostics](../../crates/simulation/src/survival_diagnostics.rs) | Root binding, validation reuse, transient witnesses, observational counters and failure bounds |

Explicit nested public modules: `creatures::{composition, record}`,
`spatial_scope::{codec, mapping, registry}`, `chart::delivery` and
`diagnostics::spans`. Follow the `pub use` blocks in abilities, creatures, rules,
nutrition and floor_activation; their private definition files are not exclusions.

### Root exports and private implementation owners

Review the root declarations (`Game`, actions, outcomes, actor/item IDs and errors)
plus every public inherent method, including methods defined in:

- [actions](../../crates/simulation/src/actions.rs), [intention](../../crates/simulation/src/intention.rs), [lifecycle](../../crates/simulation/src/lifecycle.rs), [fixture](../../crates/simulation/src/fixture.rs) and [actor_creatures](../../crates/simulation/src/actor_creatures.rs).
- [item_identity](../../crates/simulation/src/item_identity.rs), [item_knowledge](../../crates/simulation/src/item_knowledge.rs), [items](../../crates/simulation/src/items.rs), [armor](../../crates/simulation/src/armor.rs), [equipment_generation](../../crates/simulation/src/equipment_generation.rs), [equipment_magic](../../crates/simulation/src/equipment_magic.rs), [magic_items](../../crates/simulation/src/magic_items.rs) and [interactions](../../crates/simulation/src/interactions.rs).
- [input_basis](../../crates/simulation/src/input_basis.rs), [human_operands](../../crates/simulation/src/human_operands.rs), [boundary_effects](../../crates/simulation/src/boundary_effects.rs), [event_history](../../crates/simulation/src/event_history.rs), [observation](../../crates/simulation/src/observation.rs) and [explored_map](../../crates/simulation/src/explored_map.rs).
- [physics](../../crates/simulation/src/physics.rs), [polymorph](../../crates/simulation/src/polymorph.rs), [traps](../../crates/simulation/src/traps.rs), [streaming](../../crates/simulation/src/streaming.rs), [travel](../../crates/simulation/src/travel.rs), [places](../../crates/simulation/src/places.rs), [portable_light](../../crates/simulation/src/portable_light.rs) and [periodic_relocation](../../crates/simulation/src/periodic_relocation.rs).

This owner list is a discovery map, not proof that every member is already
audited. Expand the member checklist when following call paths into other private
files. Include macro-generated IDs and public fields/variants, not only methods.

## World export inventory

| Owners | Public surface and review focus |
| --- | --- |
| [lib](../../crates/world/src/lib.rs), [topology](../../crates/world/src/topology.rs), [rotation](../../crates/world/src/rotation.rs) | Position/extent, region/location, directions/doors, World methods, directed portals, cube frames and error domains |
| [geometry](../../crates/world/src/geometry.rs), [ray](../../crates/world/src/ray.rs), [scene](../../crates/world/src/scene.rs) | Adjacency, RayStep, SightCell; topology versus movement permission and visible occurrences |
| [sight3d](../../crates/world/src/sight3d.rs), [sight3d_fast](../../crates/world/src/sight3d_fast.rs), [shadowcasting](../../crates/world/src/shadowcasting.rs), [volume_scene](../../crates/world/src/volume_scene.rs), [lighting](../../crates/world/src/lighting.rs), [reach](../../crates/world/src/reach.rs) | Physical/abstract links, eye/frame/radius, occlusion, illumination and exact reference versus accelerated/diagnostic paths |
| [concealed](../../crates/world/src/concealed.rs), [material](../../crates/world/src/material.rs), [region_slice](../../crates/world/src/region_slice.rs) | Features, terrain, loaded/known/detached regions and attachment validation |
| [shared](../../crates/world/src/shared.rs), [sight_cache](../../crates/world/src/sight_cache.rs), [world_checkpoint](../../crates/world/src/world_checkpoint.rs), [checkpoint_map](../../crates/world/src/checkpoint_map.rs) | Value equality/COW, process-local witnesses, deterministic codec and shared-pool validation |

## Foundation contracts and executable examples

The first batch adds actual runnable rustdoc examples, not a second tutorial
implementation or a claim of complete API coverage:

| Source owner | Contract/example | Remaining work |
| --- | --- | --- |
| World crate root | Directed connection, explicit reverse, closed-door adjacency versus walkability | Complete World member documentation |
| Rotation helpers | Planar and nonplanar transforms, composition order, inverse and invalid-input requirements | Cross-link other frame-bearing APIs |
| Shared | COW isolation, value serialization and weak-witness invalidation | Complete remaining methods and checkpoint wrappers |
| Simulation crate root | Validated content, admitted Wait, no admission-time advance and atomic rejection | Creature-backed paid-work, observation and host-publication workflows |
| Checkpoint | Related snapshots, codec round trip, complete-root bind and deterministic continuation | Detailed validation errors, decoded-pool re-encoding and negative root examples |
| Content IDs | Grammar, separate typed namespaces and textual encoding | Per-category catalog consumers |
| Quantity/action duration | Inclusive depth range, zero/missing-depth rejection, unchanged RNG on fixed/error sample, rounding/overflow | Other arithmetic/clock APIs |
| Plan target/host entry points | Bookkeeping target versus hit recipient; provisional candidate settlement | Full ability/intention/control-state member review |

Examples deliberately use public APIs and existing dev dependencies. The empty
catalog example is a minimal backend fixture, not a production authoring recipe.
Later batches should add creature construction/advancement, paid interruption and
resumption, condition clocks, disclosure, and residency/population workflows with
independently expected positive and negative assertions.

## Merged client and server/protocol contract batches

Reconciled against `a7d0f10e1` on 2026-11-04. The rows below record bounded
improvements from [PR 226](https://github.com/KCall360/thresholds-of-ruin/pull/226)
and [PR 227](https://github.com/KCall360/thresholds-of-ruin/pull/227); neither PR
updated this shared inventory. The remaining-review column above is the broader
family review boundary, not a claim that none of those contracts exists. No
family is marked complete by these batches.

| Source family / reviewed owners | Bounded contracts and witnesses | Remaining boundary |
| --- | --- | --- |
| Client common: crate root, connection, state, pending requests and captures | Server-before-local ownership, recovery, immutable pictures, request versus gameplay completion, capture receipt versus durability; runnable atomic stream-rejection example | Remaining public members, fields/variants and reducer/selection families require member-level review |
| Text: crate root, admission and engine | Queued admission, execution ownership and prompt/readiness boundaries; runnable admission example | Parser/resolution, clarification, narrative evidence and remaining engine members |
| ASCII: crate root, model and render | Worker/reducer ownership, modal controls, coordinate/hit-test and render/capture boundaries | Complete public/member inventory and executable examples; no native UI qualification |
| Optional 3D: renderer-free model root, advancement, appearance, commands, equipment, frame, interaction, inventory, map, observation and routing; application README | Frame translation/reset, bounded Unicode search and exact compass-offset examples; stale selections, map confirmation, inventory/equipment/advancement contracts | Separate application/model member coverage, async resource ownership and native/art evidence remain incomplete |
| Server: crate root, Engine, Service/session, runner, storage and checkpoint | Trusted host versus authenticated session, accepted versus durable receipts, save barriers/policy/status, mailbox cancellation and checkpoint capacity phases; runnable save-barrier example; CH-005 resolved | Remaining public members, scheduler/publication paths, compilation and operational failures |
| Protocol: crate root, codec and recovery | Bounded decoding, schema-versus-context validation, snapshot assembly and lifecycle reading paths; runnable assembler example | Remaining envelope/member contracts, disclosed-handle families, clocks and direction |

PR 226 records five passing runnable doctests and scoped library, Clippy and
public/private rustdoc checks for common/Text/ASCII and the separate renderer-free
model workspace. PR 227 records two passing doctests and scoped server/protocol
checks. Both record documentation checks and rendered-page inspection. These are
the respective PRs' evidence, not fresh execution in this tracking reconciliation
or proof of full runtime, native, release or cross-platform qualification.
Subsequent merged batches retain their own bounded evidence:

- [PR 229](https://github.com/KCall360/thresholds-of-ruin/pull/229) adds world
  residency and coordinate contracts; its [member checklist](world-residency-api.md)
  records the reviewed surface and remaining world work. World remains Partial.
- [PR 230](https://github.com/KCall360/thresholds-of-ruin/pull/230) improves
  simulation content IDs, dice, hit dice, quantities, action duration and rules
  contracts and examples. The broader simulation inventory remains Partial.
- [PR 231](https://github.com/KCall360/thresholds-of-ruin/pull/231) adds the
  test-support public contract pass and member checklist below; CH-010 remains
  open. That bounded status is preserved, not promoted to whole-project completion.

The source-only headless review records redundant compact-frame construction as
[CH-011](code-health.md#ch-011-compact-headless-frames-build-discarded-derived-state).
It adds no source contracts or executable examples, so Headless stays Inventoried.

## Qualification and maintenance

Run the affected doctests, public and private-item rustdoc with warnings denied,
documentation links and the checks required by [testing policy](testing.md).
Public-only rustdoc matters: private-item documentation can conceal broken
reader paths through hidden implementation modules. Source-comment changes are
not automatically the Markdown-only prose path. Do not label scoped checks as a
full runtime, native UI or cross-platform qualification.

At the end of a family, reconcile the exported/member inventory again, check
errors/panics and field units, inspect rendered documentation, and only then add
focused missing-documentation enforcement. Keep one authoritative home for rules
and preserve unique test oracles. Record newly found health concerns separately
from missing prose, and qualify cleanup through the register's removal proof.

## Test-support performance contracts

Reviewed against `3aac0cbb2` on 2026-11-04. This is the complete current public
surface of `tor-test-support`, not a qualification of all benchmark drivers.
The crate does not manage processes or measure elapsed time.

| Public path | Contract and witness |
| --- | --- |
| `performance::SPEC` | Embedded fixture scope, no runtime I/O; shared server fixture remains its source |
| `performance::Trace` and `version`, `seed`, `secondary`, `traversal` | Workload-only parsing, metadata versus validation, driver-owned secondary/traversal execution; CH-010 records schema limits |
| `Trace::load`, `Trace::steps` | Embedded parse panic, ordered minimum-region filtering without world validation; runnable filtering example |
| `performance::Step` and all four fields | Label categories, default threshold, supported outcomes versus action dispatch; runnable deserialization/default/typo-rejection example |
| `performance::TraceAction` and all variants/fields | Actor-relative move, desired door state and wait; no stored door identity |
| `Step::resolve` | Readiness and one distinct eligible disclosed door, repeated-occurrence deduplication; existing readiness/missing/ambiguous/duplicate-door tests |
| `Step::verify`, `Step::verify_effect` | Issued picture revision versus separately checked causal boundary, exact blocked equality and narrow action-specific effects; existing positive/negative effect tests |

The independent witnesses live in
[performance tests](../../crates/test-support/tests/it/performance.rs).
The [latency benchmark](../../crates/server/examples/latency_bench.rs) demonstrates
caller-owned causal-boundary checks before `verify_effect`; its existence is not
fresh performance-measurement evidence. No runtime behavior, fixture data or
oracle assertions changed in this documentation pass.


## Client presentation helpers

Reviewed against `3c24d49fd` on 2026-11-04. This is the bounded member inventory
for the existing `tor_client_common::surfaces` and `public_plan` exports, not
completion of the client-common family. No runtime behavior changed. Eight
runnable rustdoc examples use public APIs with explicit expected results; they
require compilation and execution through the normal Rust documentation gate.

| Public path | Contract and witness |
| --- | --- |
| `surfaces::Roles` and `floors`, `ceilings`, `walls` | Borrowed, sorted unique labels; open-neighbor roles, multiple roles and absence versus unknown space; classification examples |
| `surfaces::material` | Borrowed material versus static empty-label fallback, no validation or sanitization; explicit-label/fallback example |
| `surfaces::roles` | Disclosed cardinal adjacency, first duplicate position wins, no inferred unseen surface; room and lone-solid example |
| `surfaces::roles_by` | Caller labels, borrowed lifetime, callback repetition and coordinate arithmetic precondition; multi-role example |
| `surfaces::floor_below` | Exact adjacent position, first matching solid cell, no unknown-space inference and coordinate arithmetic precondition; floor/absent-column example |
| `surfaces::ceiling_above` | Complete disclosed column, nearest solid cell, inclusive 16-cell limit and coordinate arithmetic precondition; unseen-gap and 16/17-cell examples |
| `public_plan::line` | Historical observation tick/frame, unknown operands, unchanged remaining work, no validation or target resolution; unavailable-aim and directional-aim examples |
| `public_plan::changed_line` | Complete-record value comparison, silent disappearance, explicit no-public-plan observation, caller-owned prior state; absent/equal/later-observation example |

Existing surface unit tests remain independent witnesses for role classification,
unknown gaps and custom labels. These helpers do not confer interaction authority
or refresh historical facts. Remaining qualification includes rendered public and
private API review, warning-free rustdoc and executable examples; member review
alone does not establish that those checks ran.
