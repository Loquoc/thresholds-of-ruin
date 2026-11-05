# Code-health register

This living register tracks source-supported defects, robustness/performance
questions and maintainability work found during systematic review. It complements
the [Rust API inventory](rust-api-coverage.md), existing
[documentation review](documentation-review.md) and
[test-effectiveness ledger](test-effectiveness.md). It is not a miscellaneous TODO
list, a claim of a full audit or permission to delete a path that looks old.

## Review and evidence rules

Review by subsystem and call path across all crates (including optional 3D),
scripts, scenarios, CI and format adapters. Include cleanliness, malformed input,
arithmetic/capacity limits, failure/cancellation, retries/backpressure, recovery,
retained memory, repeated work, ownership, schema coupling, test oracles and
operational diagnostics. File length or duplicate-looking types alone prove no
defect.

Each issue has a stable ID, category/status, impact/confidence, inspected baseline,
source/trigger, evidence and consequence, smallest next step, and acceptance
witnesses. Separate **confirmed defect**, **confirmed documentation mismatch**,
**source-supported candidate**, **measurement question** and **maintainability
opportunity**. Preserve meaningful negative findings and close stale entries.
Do not label a source-only candidate a reproduced runtime failure.

Initial findings below were reconciled against `a442ea4b` on 2026-11-03. A later
change must recheck the cited source before acting. Update the entry with its
resolution and evidence; avoid reopening the same root cause under a new ID.

## Active review queue

### CH-002: Commands retained outside the bounded mailbox

- **Category/status:** robustness; source-supported candidate, not reproduced.
- **Impact/confidence:** potentially high memory/latency impact under a stalled
  save and sustained authenticated commands; runtime consequence unmeasured.
- **Evidence:** [runner.rs](../../crates/server/src/runner.rs), `holds` and
  `deliver`, retain commands in a `held` deque while saving is overdue; `run`
  drains the bounded mailbox repeatedly and later drains held commands before
  ordinary polling. Mailbox capacity
  alone is therefore not evidence of a bound on retained command work.
- **Next step:** reproduce with a stalled writer and sustained commands; measure
  retained count/bytes, query/shutdown responsiveness, ordering and disconnect
  cleanup. Decide a bounded admission/backpressure policy from that evidence.
- **Acceptance:** independent tests for the selected bound and failure response,
  preserved request order and retry semantics, and demonstrated recovery without
  starving other host work. Do not silently drop or reorder accepted requests.

### CH-003: Retained presentation and history cost

- **Category/status:** performance; measurement question, not a claimed leak.
- **Impact/confidence:** retention/startup may grow with history and attachment
  count; the magnitude and practical threshold remain unmeasured.
- **Evidence:** [presentation_history.rs](../../crates/server/src/presentation_history.rs)
  retains bounded per-attachment presentation capabilities whose boundaries own
  shared Game roots; [history_index.rs](../../crates/server/src/history_index.rs)
  maintains history indexes. Old complete display views are already released.
- **Next step:** measure unique retained roots/bytes, RSS slope and startup cost
  across history length, clients and rewind-window expiry. Do not multiply a
  full Game size by capability count or equate checkpoint bounds with all memory.
- **Acceptance:** same-machine comparable measurements using the
  [performance workflow](performance.md), unchanged replay/disclosure behavior and
  an explanation of retained versus reclaimable ownership.


### CH-016: Equipment ranking scores cached; timing evidence remains open

- **Category/status:** performance/maintainability; repeated ranking projection
  corrected by [PR 256](https://github.com/KCall360/thresholds-of-ruin/pull/256),
  merged on 2026-11-05 at `49b9d7808ebaa909f73043f70f73370d75442431`.
  Work-count evidence is established; practical speedup remains unmeasured.
- **Correction:** [equipment ranking](../../crates/simulation/src/ai_items.rs)
  calculates each eligible choice's optional score once, retaining order,
  unavailable-score handling, first nondominated selection and existing
  Equip/Unequip operands. Only scores are retained, not candidate Games.
  Eligibility and production loadout projection remain unchanged. Dominance
  comparisons can still be quadratic; this is not a linear-time claim for the
  entire decision or proof that repeated eligibility baseline work is resolved.
- **Work-count evidence:** the [focused hosted witness](https://github.com/KCall360/thresholds-of-ruin/actions/runs/41949313623)
  passed three ranking tests, including actual 1/4/16/64-choice projection and
  Game-clone counts, unchanged-Game assertions and ordinary AI actions. A faithful
  repeated-projection mutation produced two intended singleton failures (2 versus
  1 and `[2]` versus `[1]`); restoring the optimized source passed all three again.
  Larger old-case counts remain source-derived because the mutation stopped at
  singleton failures. They are not measured baseline runs.
- **Behavior qualification:** focused affected selections passed for equipment
  (13), ordinary AI idle (21), duplicate sockets (1), remembered curses (1) and
  retained ranged equipment (5). [Final integrated full CI](https://github.com/KCall360/thresholds-of-ruin/actions/runs/41952664131)
  passed for head `05e4a1411b97378a6470109a3740790cf10cb918`, including both
  platform aggregates and Full PR qualification. These are historical receipts,
  not fresh runtime checks performed by this register update. Two earlier local
  test compilations were killed before execution and provide no runtime evidence.
- **Remaining measurement gate:** compare deterministic small and larger
  inventories under the [performance workflow](performance.md) before claiming
  a practical speedup. Report projection/clone/fold counts separately from elapsed
  time and storage bytes; shared-storage cloning does not copy the entire world.
  Keep eligibility follow-ups separate from the qualified ranking change and
  preserve loadout derivation, recovery, protection and equipment behavior.

### CH-017: Portable-light fuel filtering scans unrelated items

- **Category/status:** performance; source-supported iteration candidate, not a
  reproduced slowdown. Reviewed at `3c24d49fd` on 2026-11-04. Practical cost and
  priority are unmeasured; no illumination correctness defect is established.
- **Evidence:** [next_fuel_expiry and burn_light_fuel](../../crates/simulation/src/portable_light.rs)
  iterate the complete [ItemStore map](../../crates/simulation/src/item_store.rs)
  before filtering fuel eligibility. [Physics integration](../../crates/simulation/src/physics.rs)
  checks expiry, then calls `burn_light_fuel(1)` per actual physical iteration;
  the no-gravity/no-motion path instead burns one whole interval.
- **Scale:** with N stored item rows and T executed integration iterations, fuel
  eligibility alone visits N*T rows, even with no burning lights, plus the
  separate expiry scan. Early exits mean T is not necessarily requested elapsed
  ticks. These are cheap eligibility visits, not N*T optical calculations or
  deep clones, and do not prove fuel work dominates simulation time.
- **Next step:** count fuel candidates and compare release timings with increasing
  unrelated items, zero/one/many burning lights and moving/stationary paths.
  Control actual executed ticks and activity geography before choosing an
  optimization. If justified, consider a derived fuel-bearing/light candidate
  index maintained through item mutations, retaining live eligibility checks.
  Indexing only currently burning items additionally requires proof for every
  actor/equipment/location/activity transition; offscreen is not permission to
  skip fuel. No serialized cache or second lifetime authority is needed.
- **Acceptance:** define the promised candidate complexity, then establish its
  first-red count witness. Preserve [portable-light tests](../../crates/simulation/src/abilities/portable_light_tests.rs)
  for ignition preparation, exact wait/extinguish fuel, seven-tick expiry,
  interruption during burnout, and drop/pickup/stow without refill. An index
  also needs insertion/removal/extinguish/death/activity maintenance witnesses.
  Existing assertions were inspected, not executed. Optical topology, residency
  continuation and checkpoint handling remain outside this review; no speedup
  is claimed.

### CH-018: 3D loot labels repeat scene picking and discarded name preparation

- **Category/status:** performance; source-supported work candidate, not a
  reproduced functional bug or measured slowdown. Reviewed at `af1ff6046` on
  2026-11-05; medium priority pending representative counters/profile.
- **Evidence:** [label updates](../../crates/client-3d/src/loot_labels.rs) build
  summaries and call `Models::select` for each successfully projected group before
  comparing the resulting labels with the prior frame. The equality check avoids
  UI reconstruction, not preceding preparation, picking or layout work.
  [Picking](../../crates/client-3d/src/picking.rs) builds a fresh known-terrain map
  and scans it in `floor_cell`, then traverses every queried shaped mesh per call.
  Visible eligible meshes additionally walk ancestry and may receive a ray test;
  this does not claim every mesh is intersected. For G valid label rays, the path
  makes G terrain maps and G complete mesh-query traversals per active frame.
  Terrain includes aligned remembered cells, not only the loot cell. These are
  structural counts, not measured frame times.
- **Related preparation:** disclosed-item membership uses linear visible-cell
  scans. Count-only summaries still format and sort names before replacing text
  with stack counts or discarding small piles. This belongs to the same candidate,
  not a second issue merely to increase the finding count.
- **Next step:** first instrument map constructions, terrain entries, mesh visits
  and name formatting in representative ready scenes with multiple loot cells,
  names on/off and varying terrain/mesh counts. Consider one prepared terrain map
  and disclosed-cell lookup per update, plus a count-only grouping path. Preserve
  existing no-rebuild behavior; observation revision alone cannot cache final
  occlusion because camera, viewport and animated transforms can change it.
- **Acceptance:** establish the current repeated-work witness before optimization,
  then require at most one prepared terrain map per update with identical labels
  and occlusion. Cover stationary/moving cameras, animated foreground obstruction,
  cutaway/height, focus/modal transitions, empty/small piles and disappearing items.
  Benchmark representative scenes before claiming a frame-rate improvement.
- **Review limits:** existing label assertions cover quantities, bounded names,
  layout, count thresholds, noninteractive nodes and offscreen anchors; they were
  inspected, not run, and do not establish a work bound. No fresh runtime, GPU or
  allocation profile is claimed.

## Retained or resolved findings

### CH-011: Compact headless frames build discarded derived state

- **Category/status:** performance/maintainability; redundant construction
  corrected. This is a work-avoidance result, not a measured speedup or a
  retained-memory fix.
- **Correction:** [headless `emit` and `DiagnosticOutput::project`](../../crates/client-headless/src/main.rs)
  serialize the raw envelope first and use the existing compact eligibility
  decision before invoking the derived-field builder. Successful non-ready
  `map_page`, `palette` and `waiting` frames skip advancement formatting and
  observation, history, palette and other derived JSON construction. Full frames
  invoke the builder once. Accumulated diagnostic memory already skipped this
  path and is not counted as newly avoided work.
- **Construction evidence:** the regression
  `compact_background_skips_derived_construction_only_for_eligible_frames`
  exercises both flags, all three output kinds, local errors, background and
  complete message families, and absent raw messages. Its first-red run against
  the equivalent eager construction failed at one builder invocation on an
  eligible `map_page`, where zero was required. The fix moves that invocation
  and observation-base serialization after the compact branch.
- **Preserved contracts:** the existing
  `compact_background_keeps_raw_messages_and_marks_omitted_derived_state` and
  `compact_background_preserves_ready_observations_and_every_error_in_full`
  witnesses retain raw message, exact envelope, map hydration and full-frame
  assertions. Default and `--omit-memory` witnesses remain unchanged in scope,
  including preserved client memory. The actual-process owner remains
  [HeadlessProcesses.test_compact_background_retains_raw_pages_and_complete_gameplay_and_errors](../../scripts/test_headless_process.py).
  Ready, observation/lifecycle, snapshot and error frames remain complete;
  no protocol, save format, dependency or gameplay authority changes.
- **Limits:** optimized before/after latency and allocation magnitudes are not
  measured. Smaller output and fewer builder invocations must not be reported as
  a quantified CPU, allocation or memory-retention improvement.

### CH-001: Public core boundary contracts

- **Category/status:** documentation; resolved by [PR 225](https://github.com/KCall360/thresholds-of-ruin/pull/225),
  merged at `3aac0cbb21b031d109df2f19736a3a4bb987092b`. The specific gaps
  below remain corrected at `6fab91097`; broader member coverage is a separate
  API-inventory program, not an unresolved instance of this finding.
- **Impact/confidence:** high confidence; callers can misunderstand rotations,
  checkpoint preconditions, provisional host results or plan targets.
- **Evidence:** [World::connect](../../crates/world/src/topology.rs) accepted all
  24 cube rotations while its comment described planar quarter turns;
  [checkpoint](../../crates/simulation/src/checkpoint.rs) public entry points
  omitted settlement/panic and root-binding order; [Plan::target](../../crates/simulation/src/abilities/plan.rs)
  described its non-targeted bookkeeping case only inside the implementation.
- **Correction:** source-adjacent contracts and executable public-API examples;
  no runtime changes or removed paths. Remaining member coverage stays in the
  [API inventory](rust-api-coverage.md).
- **Qualification:** the original change records ten passing doctests, scoped
  library checks and Clippy, warning-free public/private rustdoc, documentation
  checks and rendered-page inspection. This reconciliation rechecked the cited
  source; it does not claim fresh runtime or whole-API qualification.

### CH-004: Action-transition phase readability

- **Category/status:** maintainability; resolved by documenting the existing phase
  and caller contracts, without a behavior-changing extraction. The review at
  `6fab91097` established no independently owned duplicate implementation whose
  removal would justify changing the transaction boundary.
- **Resolution:** the [action-transition phase map](../architecture/simulation-cycle.md#action-transition-phase-contract)
  records inputs, outputs, both callers and the ordering obligations in
  [Engine::resolve_action_transition](../../crates/server/src/engine.rs).
  Source-adjacent phase comments point to the same contract. Execution, residency,
  capability-only waits, event receipts, projection, navigation and membership
  retain their current sequence and failure propagation.
- **Preserved witnesses:** `frozen_presentation_survives_actions_without_durable_display_counters`
  in [engine tests](../../crates/server/src/engine.rs) distinguishes stationary
  waits from observer-affecting moves and checks frozen pictures and replay;
  [performance workloads](../../crates/server/tests/it/performance_workloads.rs)
  retain the independent mixed-trace disclosure/replay witness. The phase map
  joins those owners to the existing persistence and intention contracts.
- **Limits:** only documentation changes; no assertion or implementation was
  removed, and no fresh runtime/performance result is claimed. Any later extraction
  still requires the register's equivalence and unique-witness proof.

### CH-006: Rogue Search comment describes checked recognition

- **Category/status:** documentation; corrected. The Search comment in
  [Rogue exploration rules](../../scenarios/rogue-exploration/rules.toml) now
  describes actor-private recognition with a shared roll, eligible adjacent
  touch searches and no discovery through read-only queries. Success is not
  guaranteed. No executable TOML value or gameplay rule changed.
- **Certificate evidence:** the existing `tor-scenario-50` validator reproduced
  the baseline certificate byte-for-byte, then validated the corrected package.
  Only authored rules and source-closure content hashes changed; effective
  rules, model, index, dependency hashes and validator identity are unchanged.
  Repository LF-source certificate checks pass. No version bump is necessary
  for this comment-only identity change.
- **Limits:** this is certificate generation and source/semantic review, not a
  fresh gameplay regression run. Checked discovery's positive, failed-roll and
  nondisclosure witnesses remain unchanged and must pass final-head hosted CI.

### CH-010: Performance-trace parsing is not full fixture validation

- **Category/status:** maintainability; resolved as an explicit retained input
  contract, not a gameplay or benchmark defect. Reconciled at `6fab91097`.
- **Evidence:** [Trace and Step](../../crates/test-support/src/performance.rs)
  deserialize the workload subset of the embedded fixture. `version` is a plain
  integer; known successful outcome labels are not paired with action kinds.
  `verify_effect` checks the action kind after handling `blocked` specially.
  [The existing outcome-label test](../../crates/test-support/tests/it/performance.rs)
  deliberately accepts all four known labels on a wait step. The public contract
  now states these limits, so callers cannot mistake parsing for full validation.
- **Consequence/confidence:** high confidence in the source contract; a custom
  trace can carry misleading metadata while its action-specific assertions pass.
  No malformed embedded trace or measured false benchmark result was found.
- **Disposition:** current drivers call `Trace::load` on the compiled-in fixture;
  no custom-trace ingestion route was found in the server examples, integration
  owners or Python drivers. The public parser deliberately reads only workload
  fields, and its successful labels are reporting categories. Keep that documented
  API and its existing test rather than silently strengthening deserialization.
  A future custom-ingestion feature must define its version and pairing policy at
  that boundary; it is not an outstanding fix to the supported embedded workload.
- **Acceptance:** preserve rejection of unknown labels and independent
  blocked/movement/door/readiness oracle tests; add explicit version/pairing
  rejection witnesses only for an approved strengthened input contract.

### CH-014: Event-time sightings now measure absence

- **Category/status:** behavior/maintainability; resolved by
  [PR 262](https://github.com/KCall360/thresholds-of-ruin/pull/262), merged at
  `739ee7fdf704a33979d7711bcabc9807203328da`. The event-time
  [message composer](../../crates/client-ascii/src/messages.rs) records disclosed
  `LostSight` ticks silently, so a return is measured from the latest departure,
  not from the previous arrival. The threshold remains 1,000 ticks inclusive.
- **Evidence:** two first-red integration methods in
  [turns and map tests](../../crates/client-ascii/tests/it/turns_and_map.rs)
  reproduced redundant notice text after long visibility and one tick of absence.
  `event_time_sightings_measure_absence_from_latest_departure` covers initial
  notice, short/exact/long absences, repeated cycles, actor identity, same-tick
  ordering, reset and silent departure. The model-level
  `model_outcome_sightings_use_departure_time_without_refreshing_the_picture`
  applies genuine `OutcomeFacts` through `App::update` while the visual picture
  remains stale. The legacy observation-based witness is retained.
- **Impact/limits:** the correction removes redundant player-facing narration;
  it changes no gameplay authority, disclosed information or wire schema.
  Both regression methods and the full 182-method ASCII integration owner pass
  locally. Native process, Windows and hosted CI qualification are separate;
  no resource or performance improvement is claimed. See the
  [test-effectiveness record](test-effectiveness.md#event-time-sighting-absence-5-november-2026).

### CH-013: Linux storage probe preserves empty column boundaries

- **Disposition:** reproduced with controlled full-probe fixtures and corrected
  by [PR 255](https://github.com/KCall360/thresholds-of-ruin/pull/255), merged
  on 2026-11-04 at `af1ff60469f0342d81cbb87f7ee86df1371df08d`.
  The original whitespace parser assigned the first model word to an empty
  transport column. The intended first-red witness failed for both single-word
  and multiword models; this was not a captured failure on a physical device.
- **Correction:** [probe_linux_machine](../../scripts/perf_ledger.py) requests
  explicit `lsblk` JSON fields and preserves missing transport separately from
  device-model text. Non-string transport/model fields become unknown; missing
  or invalid device output and command failures retain explicit unknown-data
  fallbacks. The machine-record schema, fingerprint algorithm and Windows probe
  remain unchanged.
- **Coverage:** [MachineProbe tests](../../scripts/test_perf_ledger.py) assert
  exact storage fields and frozen fingerprint values for present/absent transport,
  single/multiword and absent models, partition-parent selection, rotational
  encodings, NVMe and quoted model text, command failures, malformed JSON and
  unexpected field/container types. The fixture also covers Windows path
  rendering without requiring a real Linux storage device.
- **Qualification:** the PR records an intended first-red failure and a passing
  27-test focused owner after correcting a Windows-path fixture assumption.
  After integrating current main, 30 ledger, 31 comparison and 20 documentation
  tests passed locally (81 total). [Hosted run 41930266853](https://github.com/KCall360/thresholds-of-ruin/actions/runs/41930266853)
  passed for final head `6a123303186e5f6b587d8a4d715305d3e5a0565e`, including
  all full Rust/Python lanes, both required platform aggregates and Full PR
  qualification. These are scoped historical receipts, not fresh checks performed
  by this register update or evidence of integration with every real storage device.
- **Compatibility/limits:** Linux JSON probing requires util-linux 2.27 or newer.
  Unsupported versions and unusable device output can produce unknown metadata.
  Corrected or fallback metadata can change future fingerprints; historical
  records remain intact and timing series with different fingerprints must not
  be combined. See the [performance workflow](performance.md). This corrects
  descriptive machine metadata, not a proven timing-comparison error, gameplay
  defect or measured performance improvement.

### CH-015: Workflow progress reconciliation is order-independent

- **Disposition:** reproduced and corrected on 2026-11-04. Workflow metadata is
  selected by parsed update time within the highest attempt. Equal-time report
  fields are reconciled with explicit conflict rejection; unknown highest-attempt
  chronology fails rather than choosing the first input.
- **Evidence:** two snapshots of run 42, attempt 2, previously retained either an
  incomplete 20-second workflow span or completed 120-second span depending on
  input order. Both had the same correctly reconciled 60 runner-seconds.
- **Coverage:** [accounting tests](../../scripts/test_ci_cost_report.py) require
  identical parsed JSON and human text from reversed repeated CLI inputs, latest
  metadata, higher-attempt precedence, timezone-equivalent ties, missing-field
  reconciliation, input purity and explicit ambiguity rejection without overwriting
  reports. Existing job deduplication, interval accounting and limitations remain.
- **Limits:** the local 27-method accounting suite passes. This fixes offline
  reporting metadata, not execution, billing or CI qualification; no affected
  historical baseline is claimed. See the [accounting contract](ci-cost-baseline.md)
  for the stricter handling of invalid attempts and unknown chronology.

### CH-012: Comparison workload ledger identities corrected

- **Disposition:** reproduced and corrected on 2026-11-04. The comparison
  registry now uses `ordered_strikes` and `active_mobs` as ledger names, with
  version 1 unchanged. Benchmark wire labels and ledger validation stay intact;
  historical measurements are not rewritten.
- **Evidence:** actual [ledger CLI](../../scripts/perf_ledger.py) exports of both
  prior registry identities failed at workload-name validation before writing.
  Otherwise-identical valid-name fixtures passed, isolating the defect from
  counts, timing statistics, source identity and raw-data checksum handling.
- **Coverage:** [ledger tests](../../scripts/test_perf_ledger.py) now exercise
  both registry-derived CLI append/check round trips with independent expected
  identities and preserved values, duplicate rejection without writing, and
  every registry name against the ledger schema. Existing streaming conversion
  and invalid-workload rejection remain covered.
- **Limits:** this is an export correctness fix, not a benchmark execution,
  gameplay or speedup claim. Previously generated comparison files carrying the
  invalid identities still fail strict validation; no historical importer was
  added. Local qualification covers the affected Python tooling, not a full
  Windows/Linux application or release run.


### CH-007: Sight implementations are not established dead code

- **Disposition:** retain pending a specific equivalence/removal proof.
- [eye_scene_reference](../../crates/world/src/sight3d.rs) is an exact oracle for
  accelerated sight. [shadowcasting](../../crates/world/src/shadowcasting.rs) and
  [volume_scene](../../crates/world/src/volume_scene.rs) have distinct tests and
  [fov_bench](../../crates/world/examples/fov_bench.rs) comparison callers.
- No runtime-call grep establishes that these test/diagnostic roles are obsolete.
  Consolidation must preserve their independent assertions and measured purpose.

### CH-008: Similar creature caches have distinct meanings

- **Disposition:** retain; no redundancy established.
- [CreatureState](../../crates/simulation/src/creatures/state.rs) holds permanent,
  baseline and effective derived values. [PermanentCreature](../../crates/simulation/src/creatures/derive.rs)
  documents why permanent inputs cannot be replaced by a condition-free effective
  sheet. Plan target bookkeeping likewise remains used by checkpoint validation
  and execution. A clearer name/comment is not evidence that storage can disappear.

### CH-009: Checkpoint-capacity failure classification already exists

- **Disposition:** resolved before this program; do not reopen an older review's
  request as an outstanding defect.
- [SaveFailure::CheckpointCapacity](../../crates/server/src/storage.rs) already
  distinguishes the retained-checkpoint bound. Recheck current tests and behavior
  if a new failure contradicts that classification; do not infer fresh runtime
  verification from this source reconciliation.

### CH-005: Server/storage wording and operational identities

- **Category/status:** documentation; resolved by [PR 227](https://github.com/KCall360/thresholds-of-ruin/pull/227),
  merged as `6b344410`; reconciled against `a7d0f10e1` on 2026-11-04.
- **Resolution/evidence:** [Engine::retry](../../crates/server/src/engine.rs)
  distinguishes an accepted receipt from a durable journal prefix.
  [Storage](../../crates/server/src/storage.rs) distinguishes background writes
  from caller-thread lazy reads, and `inspect_save` separates physical-frame
  identity from logical save format and strict gameplay replay.
  [Checkpoint preparation](../../crates/server/src/checkpoint.rs) and
  [persistence](../architecture/persistence.md) describe synchronous
  chart-history capacity encoding before admission, separately from periodic
  worker encoding. No runtime implementation or format changed.
- **Qualification:** PR 227 records server/protocol library checks and Clippy,
  two runnable doctests, warning-free public/private rustdoc, documentation
  checks and rendered-page inspection. This reconciliation rechecked the current
  source wording; it did not rerun those checks or establish runtime/performance
  qualification. Broader server/protocol member coverage remains partial in the
  [API inventory](rust-api-coverage.md).

## Bounded review coverage

### Nutrition and health recovery

Source review at `4c63a9367` on 2026-11-04 found no new actionable defect in
arithmetic, fraction preservation or active-time settlement under existing valid
gameplay inputs. This is a bounded negative finding, not a correctness
certification or a new CH issue. No behavioral tests or benchmarks were rerun.

- [Nutrition time projection](../../crates/simulation/src/nutrition.rs) uses at
  most one analytical positive/empty-reserve split and retains fractional work;
  large elapsed time alone does not establish per-tick digestion cost.
- [Health recovery](../../crates/simulation/src/health_recovery.rs) widens elapsed
  addition, caps restoration by injury, clears saturated/dead fractional clocks
  and does not advance stopped clocks. Rephasing floors the earned fraction.
- [Combined settlement](../../crates/simulation/src/creatures/state.rs) chooses
  expiry/nutrition boundaries and bisects earlier combined regeneration crossings.
  The inner search takes at most 64 probes over a u64 interval; this does not
  bound the whole settlement to 64 probes or establish measured runtime.
- [Regeneration accounting](../../crates/simulation/src/nutrition/regeneration.rs)
  intentionally charges gross actual restoration using widened checked products.
  Net before/after pool differences would lose gains before capacity changes.

Existing source witnesses include independent tick-oracle nutrition comparisons,
empty-boundary/deadline cases, maximum-tick settlement, split-interval recovery,
saturation/death and modifier/nutrition-toggle fraction checks in the linked
nutrition and recovery modules. Preserve those independent expected values rather
than deriving test oracles from production projection helpers. This pass excludes
complete condition/recovery interactions, balance, stochastic trigger distribution,
freeze/thaw scheduling and external input validation; those remain unqualified.

### Preparation-owned action costs

Source review at `e4ebbba5d` on 2026-11-04 found no new actionable issue in live
CostBundle arithmetic, CostLedger start/resume/finish/cancel, capacity reduction
or their creature wrappers. No new CH issue, fresh behavioral pass or complete
action-execution certification follows from this bounded negative result.

- [CostBundle and CostLedger](../../crates/simulation/src/costs.rs) use checked
  phase arithmetic and domain bounds. Explicit zero entries represent real
  preparations; removing them would change resumable/finishable/cancelable holds.
  Start validation is read-only and preflights all pools before spending;
  matching holds resume without another charge and conflicting costs are rejected.
- Available balances exclude unpaid resolution reservations. Finish checks all
  pools before payment and removes the hold afterward; repeat finish is Unknown.
  Cancel releases unpaid reservations without refunding paid start costs. These
  ledger semantics do not establish request-retry idempotence, owned by receipts.
- Capacity reduction cancels whole bundles in resource order, then newest
  intention ID, recomputing later pools after releases. It is not global
  newest-first ordering. [Creature wrappers](../../crates/simulation/src/creatures/state.rs)
  reject new cost validation/start for dead creatures before delegation.
- Reserved-total scans are bounded by [64 holds](../../crates/simulation/src/limits.rs)
  and [three resource pools](../../crates/simulation/src/resources.rs). This is
  not evidence of unbounded work or measured justification for cached totals.

The existing [cost tests](../../crates/simulation/src/costs_tests.rs) were inspected,
not executed: independent insufficient-pool/whole-ledger comparisons, explicit
start/resume/finish balances, cross-intention funding, cancellation, zero holds,
capacity reduction without repeated cancellation and full-capacity resume.
Preserve those concrete assertions. Whole scheduler atomicity, replay receipts,
every ability adapter, physical supplies, live ruleset replacement and checkpoint
validation remain outside this review; no performance measurements were made.

### Dice allocation and item-owned armor

Source review at `0de890ef7` on 2026-11-04 found no new actionable issue in two
bounded areas: deterministic dice/edge allocation and live armor-condition
transformations. No CH issue, fresh test pass or subsystem certification is
claimed; no benchmark measured these paths.

- [Dice](../../crates/simulation/src/dice.rs) clamp extra check candidates to
  three and extra pool candidates to the original count. The 64-die pool bound
  limits accepted die results to 128 even with extreme edge; rejection sampling
  means this is not a bound of 128 underlying generator calls. Pool combination
  checks overflow, and signed bonuses apply once after retention before the
  final zero floor. Shared edge consumes only its allocation, preserving the
  remainder for subsequent rolled pools; fixed damage allocates no dice.
- Both raw and retained dice have [damage trace consumers](../../crates/simulation/src/damage.rs).
  Their storage is not established redundant or a measured bottleneck.
  Existing [dice tests](../../crates/simulation/tests/it/dice.rs) inspect exact
  vectors, state continuation, canceled/extreme edge and signed totals; unit
  distribution tests use independent integer counts. These assertions were
  inspected, not rerun. Statistical RNG quality, full damage protection,
  action-wide replay and optimized timings remain outside this pass.
- [Armor conditions](../../crates/simulation/src/armor.rs) separate physical
  plating from magical ward. Corrosion consumes only remaining physical armor;
  protection prevents new corrosion without repairing history, and uncurse
  preserves penalty/rust. Enchantment checks the resulting contribution before
  returning a changed instance. Equipment views flatten physical Impact/Keen
  modifiers and the five-category ward, excluding Psychic, then clear the raw
  armor profile to prevent another physical fold. That clearing is intentional.
- Existing armor tests assert exact category vectors, retained ward/history and
  unchanged instances after failed bounds. The [corrosion integration witnesses](../../crates/simulation/src/abilities/corrosion_tests.rs)
  cover once-per-sequence rust and unchanged permanent-compilation counts. The
  rollback witness establishes mutation inside the candidate before later
  failure, then checks original Game/RNG equality; the successful counterpart
  checks payment and changed item state. These were not freshly executed and
  do not certify every adapter, nested effect, save format, UI or lifecycle
  transaction, nor establish a new performance improvement.

### Condition lifetimes and homogeneous transfers

Source review at `f72ef1784` on 2026-11-04 found no new actionable defect in
live condition lifetimes or homogeneous item transfer preparation/application.
These bounded negative findings create no CH issue and establish neither a
fresh behavioral test pass nor whole-subsystem correctness.

- [Conditions](../../crates/simulation/src/conditions.rs) check extension overflow
  before any field changes and lifetime generation allocation before eviction or
  age compaction. Renewals preserve lifetime identity; generation and compactable
  age are not redundant. Unbounded/mixed-duration admission requires Keep
  stacking, so Option ordering alone does not establish a reachable Longest/Stack
  bug. Expiry carries excess elapsed time into protection rather than granting a
  fresh full interval after both phases have elapsed.
- Reserved expired tokens intentionally retain zero-duration tombstones owned
  by the exact paid reservation; removing them or making them reusable would
  change that contract. Existing condition tests inspect full-state equality on
  overflow, unbounded expiry, protected/tombstone removal, stale lifetime tokens
  and prevention of reserved-token reuse/refresh/eviction. They were not rerun;
  all stacking combinations, ability consumers and scheduler/cancellation adapters
  remain outside this review.
- [Item stack keys](../../crates/simulation/src/items.rs) include definition,
  complete condition and charges, excluding corpses. Location and reservations
  are separate transfer constraints. Held destinations remain separate; free
  source quantity and exact source/destination/allocator witnesses are checked.
  Quantity overflow and new identity allocation are preflighted before source
  removal. Conditioned merges intentionally obtain fresh identities, not proven
  unnecessary allocator work.
- Omitted fuel/relocation progress alone does not prove unsafe stacking: held
  lights disallow stacks, and [stackable equipment](../../crates/simulation/src/interactions.rs)
  excludes periodic relocation. Existing [stack tests](../../crates/simulation/src/item_stack_tests.rs)
  inspect per-unit keys, exact split state, incompatible/wrong-location merge
  atomicity, held destinations and stale-plan refusal with full Game equality.
  These assertions were not executed in this pass. Large-inventory performance,
  all indexes and take/drop/equip/death/projectile adapters remain unqualified;
  no checkpoint validation or benchmark was performed.

### Cooldowns and expertise ownership

Source review reconciled at `24f0827c2` on 2026-11-04 found no new actionable
issue in actor-active cooldown storage/expiry or permanent expertise acquisition.
No new CH issue, fresh behavioral pass or complete scheduler/advancement
certification follows; no performance measurement was made.

- [Cooldown starts](../../crates/simulation/src/cooldowns.rs) validate duration,
  catalog identity, readiness and capacity before shared-storage mutation. Failed
  or duplicate starts do not renew intervals. Zero advancement exits before
  copy-on-write; positive advancement subtracts saturatingly and removes expired
  entries without a per-tick loop. Scans are linear in retained catalog-bounded
  entries, not a measured reason for another timer index. `is_ready` describes
  absence of cooldown, not ownership or permission to perform an ability.
- [Creature integration](../../crates/simulation/src/creatures/state.rs) includes
  cooldown deadlines independently of recovery. Existing [cooldown tests](../../crates/simulation/tests/it/action_cooldowns.rs)
  assert stopped recovery still expires at 100 ticks, death at tick 10 leaves
  90 of 100 cooldown ticks, and exact retained-entry/copy-on-write counts through
  zero, repeated and maximum-duration advancement. Unit snapshots preserve
  failed-start/zero-work assertions. These were inspected, not rerun; freezing,
  checkpoints and end-to-end scheduler timing remain outside this pass.
- [Expertise ownership](../../crates/simulation/src/expertise.rs) separates owned
  grants from active effects. Dormant grants remain owned and block repurchase;
  active effects deduplicate by expertise ID while independent origins survive
  removal of one source. Paid identity uses owning HD and point pool rather than
  mutable purchase-list position. [Acquisition](../../crates/simulation/src/creatures/advancement.rs)
  commits a build only after a successful preview. The private ownership insert
  helper's duplicate error discards a local compilation result, not an exposed
  partially updated ownership map.
- Existing [expertise tests](../../crates/simulation/src/expertise_tests.rs) inspect
  dormant duplicate rejection, independent grants, stable paid identity, exact
  shared-budget/refund balances and candidate ownership during grant replacement.
  No fresh execution is claimed; source provenance is not safely reducible to
  a boolean set. Full advancement pricing, all intellect choices, runtime
  weapon-profile capture and all action-preparation paths remain unqualified.

### Periodic search, hallucination and shared equipment capacity

Source review reconciled at `3c24d49fd` on 2026-11-04 found no new correctness
issue in these bounded paths. No fresh behavioral tests or benchmarks ran.

- [Search cadence](../../crates/simulation/src/search_clock.rs) preserves a
  floor-rounded fraction, rejects over-boundary/pending advancement before
  mutation and settles indexed pending owners rather than all loaded actors.
  Suppressed intervals earn no later catch-up. Existing [search tests](../../crates/simulation/src/discovery_time_tests.rs)
  inspect exact RNG at 99/100 ticks, duplicate-source equivalence, sleep/blindness
  suppression, real equip transitions and earned search before same-boundary
  nutrition sleep. Full discovery results, failed scan settlement and scheduler
  atomicity were not audited.
- [Hallucination cadence](../../crates/simulation/src/hallucination_clock.rs)
  advances multiple phases arithmetically, checking overflow before publication.
  Condition lifetime remains externally owned; expiry wins ties, an extension
  preserves its generation/deadline, and reapplication resets generation. Pure
  query projection neither mutates the actor nor samples rules RNG. Existing
  [lifecycle witnesses](../../crates/simulation/src/hallucination_clock_tests.rs)
  inspect paid completion, non-owner-driven authoritative phase changes,
  extension/reapplication and expiry. Palette/disclosure, all condition
  interactions, persistence authenticity and rendering performance remain outside
  this pass. Different search and visual cadence contracts are not redundant.
- [Equipment capacity](../../crates/simulation/src/equipment_capacity.rs)
  distinguishes structurally possible minimum demand from current demand. Minimum
  above capacity rejects; current excess returns a deficit without deleting gear.
  Socket-bounded checked sums reject unknown pools/invalid demand, with no implicit
  humanoid grip capacity. Existing [capacity tests](../../crates/simulation/src/equipment_capacity_tests.rs)
  inspect independent pools and exact deficits; [profile tests](../../crates/simulation/src/weapon_profile_tests.rs)
  retain gear through expertise loss/restoration, while [torch tests](../../crates/simulation/src/abilities/portable_light_tests.rs)
  reject a third grip atomically. Pure aggregation tests do not replace these
  integration witnesses. Anatomy edits, acquisition history, frozen paid plans
  and all projectile adapters remain unqualified.

### Map hydration, arena stops and exact action gates

Source review reconciled against main `3c24d49fd` on 2026-11-04 found no new
actionable defect in four bounded paths below. Existing assertions were read,
not executed; no fresh runtime pass, benchmark or full-system certification is
claimed.

- [Map hydration](../../crates/client-common/src/map_hydration.rs) binds pages to
  stream/actor/branch/base/window and validates request/ordinal/counts before
  staging. Partial pages do not publish memory. Matching aliases preserve valid
  occurrences; conflicting appearance cannot rewrite already committed regions.
  Cumulative page/cell/byte budgets reserve reply headroom before continuation.
  Existing [map tests](../../crates/client-common/tests/it/map_memory.rs) inspect
  25-page atomic publication with live readiness, mismatch refusal, cross-region
  alias conflicts and conservative continuation stops. [ClientState](../../crates/client-common/src/state.rs)
  separately recomputes viewport completeness after installation, so aggregate
  progress is not a viewport guarantee. Coordinates/topology, wire parsing,
  socket cancellation, rendering and reconnect behavior remain outside this pass.
- [Arena policy](../../crates/simulation/src/arena.rs) bounds limits and deadlines,
  counts committed execution rather than admission, and applies controls on a
  candidate. Stop precedence is elimination, action cap, then tick cap; elimination
  is rechecked after due creature settlement before preparation/queue cleanup.
  Existing [arena tests](../../crates/simulation/tests/it/arena_run.rs) inspect exact
  action/tick caps, one-step pause budgets, invalid controls and discarded step
  permission. Paid start charges remain spent when stopped preparations are
  canceled. These do not certify all callers' error atomicity, scheduler paths
  or save/restore behavior.
- [Readied touch ambush](../../crates/simulation/src/readied_ambush.rs) is a
  one-lifetime opportunity, not reusable free preparation. Acquisition and touch
  check eligibility/costs; body/placement/build/derived/condition changes retire
  credit. Invalidated touch can preserve provocation without immediate reveal or
  a strike. Existing in-module witnesses inspect one release with ordinary
  recovery, unchanged bump locations/items, no rearming after displacement and
  later ordinary preparation when incapacity ends. Preserve source-state identity
  checks; not all eligibility combinations, rollback, AI engagement or restoration
  paths were traced.
- [Weapon preview](../../crates/client-common/src/weapon_preview.rs) pins full
  action/context/request identity and rechecks control and current invocation at
  confirmation. It is not a reservation. Existing focused tests inspect changed
  echo/ability/weapon/aim/supplies, stream/revision, inventory/equipment and phase
  labels. [ASCII](../../crates/client-ascii/src/model.rs) consumes a ready gate
  before confirmation; [text](../../crates/client-text/src/engine/mod.rs) additionally
  matches the intended action. These source paths do not prove absence of every
  double-submit. Server preview calculation, all network orders, 3D, projectile
  pagination and actual-process frontend testing remain unqualified.

### Escape, theft, quantities and equipment birth

Source review reconciled against main `3c24d49fd` on 2026-11-04 found no new
issue in these four bounded gameplay paths. Existing tests were inspected, not
executed; no runtime pass, benchmark or whole-system certification is claimed.

- [Escape](../../crates/simulation/src/escape.rs) captures exact relationship,
  policy, generic opposed check, token claims and timing. Completion rechecks
  current relationship availability before RNG; stale work is abandoned without
  retargeting a replacement hold. Success removes only the exact hold before
  obtaining a fresh completion outcome. Existing witnesses distinguish successful
  release with captured recovery, failed check retaining links while paying
  recovery, same-holder replacement abandonment without RNG/debt, and endpoint
  token retirement. All scheduler callers, mirror mutations, protection expiry
  and rollback paths were not traced; internal staged writes alone do not prove
  public partial commit.
- [Theft](../../crates/simulation/src/theft.rs) filters living distinct actors,
  carried free units and unequipped items. Currency caps each fragment by remaining
  request/stock rather than summing a purse, using deterministic owner buckets
  and an existing lazy recipient merge index. Magical selection samples entries,
  not quantity-weighted units, and does not grant identity knowledge. Existing
  tests inspect finite stock, maximum requests, exact claims/equipment exclusion,
  unknown magical units and checked-rider authoring. Deferred receipts describe
  actual transfers. Multi-fragment rollback, distribution performance, relocation
  and loaded-region continuation remain outside this pass.
- [Quantity expressions](../../crates/simulation/src/quantity.rs) check depth
  arithmetic and positive range validity. Inclusive width cannot overflow for
  valid positive-only bounds; this does not generalize to ranges including zero.
  Fixed values draw no RNG. Rejection sampling permits at most 64 attempts and
  publishes candidate RNG only after success, without biased fallback. Existing
  tests inspect exact depth arithmetic, full-width deterministic progression and
  injected 64-rejection exhaustion with unchanged caller state. They do not prove
  RNG statistical quality or atomic failure handling by every caller.
- [Equipment birth](../../crates/simulation/src/equipment_generation.rs) validates
  all possible outcomes before drawing, including rare extreme modifiers.
  Authored/guaranteed conditions avoid RNG; signed potency derives curse from its
  sign, weapon birth changes hit rather than damage enchantment, and armor curse
  persists even when its physical penalty clamps to zero. Candidate RNG publishes
  only after success. Existing witnesses inspect invalid-policy nonmutation,
  reproducible disjoint outcome shapes, unchanged equipment and independent
  condition fields. Finite fixture samples are not exact frequency proofs;
  all-catalog validation, population scheduling and placement rollback were not
  audited.

### Locomotion, control, detection and cosmetic selection

Source review reconciled against main `3c24d49fd` on 2026-11-04 found no new
issue in these bounded paths. Existing assertions were inspected, not executed;
no runtime pass, statistical audit or performance measurement is claimed.

- [Locomotion composition](../../crates/simulation/src/locomotion.rs) combines
  grants, strongest exact rational chances and overriding suppression without
  source-order dependence. Hovering includes levitation; voluntary vertical
  control requires flight. [Action commitment](../../crates/simulation/src/actions.rs)
  samples confusion inside a candidate after command validation, retaining
  requested cost even for sampled stay/obstruction. Blocked draws are not rerolled.
  Existing [confusion witnesses](../../crates/simulation/src/locomotion/confusion_tests.rs)
  pin destinations/RNG, diagonal costs, pre-draw rejection and rollback after
  sampling with a deterministic retry. All geometry, movement modes, scheduler
  callers and RNG quality remain outside this pass.
- [Control-scroll state](../../crates/simulation/src/control_scrolls.rs) binds
  timed control to condition identity/generation rather than another expiry clock.
  Confusion is one replaceable charge: misses retain it, successful physical
  primaries consume it before extra resistance. Committed unavailable hostile
  attempts can release Hold while retaining confusion. Nearby target IDs are
  deduplicated before checks; approach receipts rearm on departure, not continued
  presence. Existing [control tests](../../crates/simulation/src/control_scroll_tests.rs)
  inspect old-generation rejection, unrelated-condition preservation, charge
  replacement and exact approach transitions. These distinct lifetimes are not
  established redundant; full creation/spatial reconciliation was not audited.
- [Detection completion](../../crates/simulation/src/detection_lifecycle.rs)
  captures checked base-turn duration while ordinary conditions own expiry.
  Refresh replaces remaining lifetime rather than extending it. Final matching
  chart receipts govern identification independently of gameplay mutation.
  Existing [detection tests](../../crates/simulation/src/detection_lifecycle_tests.rs)
  vary receipt status and mutation independently, preserve full lifetime without
  identifying an unavailable chart, and separate haste/slow preparation from
  captured duration. Census, loading, topology and checkpoint handling remain
  unqualified; no host-envelope claim follows from these checks.
- [Immutable cosmetic catalogs](../../crates/simulation/src/appearance_catalog.rs)
  canonicalize rows with sort/dedup and intentional neutral fallbacks. Shared
  handles avoid rebuilding the catalog for selection. [Deterministic selection](../../crates/simulation/src/appearance_state.rs)
  uses fixed operands and wrapping arithmetic, not gameplay RNG. The public
  4096-choice bound differs intentionally from internal admitted-palette selection.
  Existing tests retain literal unsigned-domain vectors, purity, reordered-source
  equivalence and larger canonical palettes. Do not infer exact frequencies,
  all-renderer behavior, allocation performance, disclosure or host-source-budget
  correctness from this bounded review.

### Capture uploads, projectile menus and shared route search

Source review reconciled against main `99dadca9a` on 2026-11-04 established no
new issue in these bounded paths. Existing assertions were inspected, not run;
this is neither a fresh runtime pass nor a performance or correctness certification.

- [Capture upload](../../crates/client-common/src/captures.rs) retains the picture
  anchor while checking the current attachment/branch. Construction bounds bytes
  and validates text UTF-8, without claiming PNG structure validation. Sequential
  chunks require the exact upload identity and next offset. Pre-commit failure
  permits bounded staging cleanup; after Commit, only a correlated rejection
  triggers cleanup. Uncertain transport, timeout or malformed outcomes must not
  become automatic abort/retry. The first correlated reply is retained through
  synchronization while ordinary messages reach the presentation callback.
  Existing [wire assertions](../../crates/client-common/tests/it/capture_upload.rs)
  preserve the original chunk/commit rejection, check the staging identity and
  distinguish explicit fresh capture after rejection from uncertain outcomes
  without a retry. Full storage durability, server format validation and socket
  cancellation remain outside this pass.
- [Projectile source/ammunition menus](../../crates/client-common/src/projectiles.rs)
  bind context, ability, inventory, equipment disclosure, request and complete
  query. Freshness checks cover reply acceptance, navigation, selection and final
  directional action. Supplied choices and the ammunition phase's launcher are
  rechecked against disclosed identities/sockets, not independently derived
  ammunition compatibility or numeric rules. Previous pages retain offsets;
  an empty filtered page can still have a continuation. Launcher selection resets
  pagination and binds its exact identity. Existing
  [menu tests](../../crates/client-common/src/projectile_tests.rs) inspect context
  and query mismatches, duplicate replies, empty-page navigation, launcher socket
  changes, exact final delivery and ten stale-state mutations. Snapshot clones
  alone do not establish an optimization opportunity. All-frontend interactions,
  server enumeration and menu-memory costs were not audited or measured.
- [Shared remembered-route search](../../crates/simulation/src/travel.rs) reuses
  settled prefixes for several destinations within one read-only decision. Keys
  retain location/orientation, ordered ties are deterministic and superseded queue
  entries are skipped. Expansion consults remembered cells/edges and actor movement
  capabilities/cost, including composed diagonal frames, rather than hidden terrain.
  Expanding the popped destination before returning permits continuation to later
  targets; removing that work needs an equivalent deferred-expansion mechanism.
  The existing `shared_search_matches_fresh_routes_across_target_orders_and_frames`
  differential witness compares shared and fresh routes through directed cycles,
  equal-cost choices, portal frames, unreachable cells and a known wall, across
  five actor frames, three target orders and repeated targets. This is source
  evidence for reuse semantics, not a timing result. Refresh/alignment, exhaustive
  geometry, route execution, memory costs and topology/checkpoint validation
  remain outside the review.

### Earned growth, strike pages, relocation clocks and finite supplies

Source review reconciled against main `99dadca9a` on 2026-11-04 established no
new actionable issue in these bounded paths. Existing regression assertions are
source evidence only: no fresh runtime pass, exhaustive correctness result or
performance measurement is claimed.

- [Earned-growth accounting](../../crates/simulation/src/earned_growth.rs) separates
  earned/applied high-water marks and exact spent-plus-held balances. Lost ordinals
  cost the current applied-tier price; forward projection uses binary search and
  [checked threshold sums](../../crates/simulation/src/rules/growth.rs), with an
  explicit representational-endpoint error. This scalar projection differs from
  [actual-row admission](../../crates/simulation/src/creatures/state.rs), which
  retains XP for dead actors, unavailable sources or structural limits and commits
  candidate spending only after successful rebuild. Existing growth assertions
  cover bounded iterative-oracle equivalence, full-width arithmetic, overflow
  preservation, drain/regrowth and banked XP beyond actual-row admission. All XP
  attribution, full build derivation and persistence remain outside this review.
- [Strike-page navigation](../../crates/client-common/src/inspection_pages.rs)
  binds context, target/source identity, request, cursor and inspection handle;
  pending queries prevent navigation and acceptance errors clear the navigator.
  Candidate compact structure is checked before publication. Eviction precedes
  formatting to retain at most two rendered pages, while bounded cursor/shape
  history survives for continuation and revisits. Existing module assertions
  cover stale/mismatched replies, skipped entries, aggregate effect/descriptor
  limits, maximum-source traversal and rider structure after rendered eviction.
  Keeping only two text pages is not an equivalent replacement for compact
  structure checks. Server authorization, protocol parsing, all UI interactions
  and the material cost of bounded aggregate rescans were not established here.
- [Periodic relocation clocks](../../crates/simulation/src/periodic_relocation.rs)
  retain independent per-item phases and trials. Advancement rejects jumping past
  the earliest deadline or proceeding with pending trials. Boundary removal keeps
  earned trials; fractional ownership transfers rather than duplicating elapsed
  time. Settlement takes the earned batch before reentrant relocation and checks
  liveness between ordered trials. Existing clock and
  [gameplay assertions](../../crates/simulation/src/periodic_relocation_tests.rs)
  cover independent phases, rejected catch-up, ordered simultaneous trials, frozen
  partial intervals, exact-boundary removal and pending-owner index isolation.
  Destination geometry, distributions, persistence and whole-scheduler costs
  remain outside scope; checkpoint implementation was not investigated.
- [Finite supplies](../../crates/simulation/src/supplies.rs) separate physical
  preview quantities from committed availability after holds. Replacement can
  discount only its retiring lineage; reservation binds lineage and cost ordinal.
  Release checks all references before removing holds, and consumption validates
  the complete preparation. Reservation still relies on its outer transaction;
  the helper alone is not certified atomic under arbitrary partial failure.
  Settled payment prevents repeated consumption without requiring a deleted source
  for a paid tail. Existing [lifecycle assertions](../../crates/simulation/tests/it/supply_lifecycle.rs)
  distinguish AI preview from held availability, resume paid work, preserve state
  on missing second supply and retain exact preparation on rejected replacement.
  Full action orchestration, every item kind, detached-state persistence and stack
  split/merge mutation were not audited in this pass.

### Defensive AI, resource pools, adventure objectives and sequence debt

Source review reconciled against main `99dadca9a` on 2026-11-04 established no
new actionable issue in these bounded paths. Primary sources and referenced
support/test files matched the reviewed versions. Assertions were inspected, not
executed; no fresh runtime pass, benchmark or exhaustive correctness claim follows.

- [Defensive AI](../../crates/simulation/src/ai_defense.rs) admits a narrow
  single-strike, unchecked, self-only condition shape, using
  [effect classification](../../crates/simulation/src/ai_effect_safety.rs) to reject
  unsupported payloads. Active conditions, reserved families, immunity, effective
  improvement and resource thresholds constrain fresh plans; priority ties use
  stable ability IDs. Exact suspended pre-prefix work is considered separately
  before fresh admission. The incoming-condition baseline is already outside the
  candidate loop. Existing [defense assertions](../../crates/simulation/src/ai_defense_tests.rs)
  cover rejected shapes, resource gates, frozen-plan resumption, declaration-order
  independence and bounded work counters across 257 candidates. Those counters
  are source assertions, not measurements from this review. Other AI priorities,
  all condition semantics and tactical strength remain outside scope.
- [Resource recovery](../../crates/simulation/src/resources.rs) combines ticks and
  remainder in wide arithmetic, caps gains before narrowing and discards excess
  fraction when full. Increased capacity does not refill; failed spending does
  not mutate. Full pools have no recovery event, and projection is nonmutating.
  Positive bounded periods come from rules validation. Existing
  [pool tests](../../crates/simulation/tests/it/resources.rs) cover partial
  deadlines, distinct pool schedules, partition equivalence, capacity changes,
  saturation, failed-spend preservation and full-width elapsed time. The loop is
  over three resources, not ticks. Whole-scheduler/frozen-region integration and
  proportional rephasing on ruleset changes were not established by this review.
- [Adventure objective policy](../../crates/simulation/src/run_policy.rs) requires
  a living starting character at the exact anchor, carrying the exact objective
  item when required. Missing or consumed items cannot satisfy it; a recorded
  victor is preserved. Continued play can retain victory without termination,
  while selected-character death independently ends an adventure. Existing
  [combat witnesses](../../crates/simulation/tests/it/combat.rs) distinguish
  noncharacters, continued victory and selected death; an
  [interaction witness](../../crates/simulation/tests/it/interactions.rs) consumes
  the objective item without granting victory. Complete policy-hook coverage,
  objective placement and persistence validation were not audited.
- [Sequence debt](../../crates/simulation/src/sequence.rs) subtracts elapsed work
  only while active. Checked completion debt includes the remaining next strike,
  authored offset tail and recovery; abandonment owes it only after a committed
  prefix. Paid prefixes retain no resource hold. Execution preserves exact gaps
  and starts final recovery after the last strike. Existing
  [strike assertions](../../crates/simulation/src/abilities/strike_tests.rs)
  preserve frozen plans, distinguish active/suspended cancellation deadlines and
  resume unfinished work without paying the committed resource prefix again.
  Preserve these operands and debts during simplification. This pass does not
  certify all rider/damage/hold lifetimes, offset boundaries or execution timing.

### Remembered placement, advancement confirmation and ranged acquisition

Source review reconciled against main `99dadca9a` on 2026-11-04 established no
new actionable defect in these bounded paths. Primary and referenced support/test
files matched the reviewed versions. Existing assertions were inspected, not run;
no fresh runtime pass, heap measurement or performance improvement is claimed.

- [Remembered-map placement](../../crates/client-common/src/map_memory.rs) derives
  translations from unique common cell identities, discarding old placement on
  conflicting or absent unambiguous anchors. Wide intermediate arithmetic and
  checked coordinate conversion avoid narrowing assumptions. Two independent
  anchor vectors are needed to certify old coverage; provisional placement alone
  does not certify a retained rectangle. Changed appearances replace byte
  accounting. Eviction favors unplaced records and clears certified coverage when
  placed data exceeds budget; adjacent-face requests avoid treating uncovered
  corners as covered. Existing [map assertions](../../crates/client-common/tests/it/map_memory.rs)
  cover conflicting/ambiguous anchors, elevation, extreme translation, rejected
  update preservation and sparse/collinear/rotated anchors. This extends placement
  and eviction coverage without certifying all hydration, protocol/frame validation,
  global topology or live-observation memory peaks.
- [Advancement confirmation](../../crates/client-common/src/advancement.rs) retains
  at most four complete pages, bounded offset history and one exact confirmation,
  with a pinned overview. Replies bind query, basis, revision, request and context
  before mutation, including section totals and encoded-envelope size. Navigation
  uses returned offsets. Identity/control loss discards private state; changed
  context stales confirmation rather than silently rebuilding it. Commit uses the
  retained proposal/basis and current admission; respec resume/cancel names the
  disclosed intention and permitted controls. Existing
  [reducer assertions](../../crates/client-common/src/advancement/tests.rs) cover
  cursor progress, stale/foreign replies, confirmation refusal, bounded traversal
  of 100 removal pages and exact respec operands. Server accounting, all parser
  grammar and UI end-to-end behavior remain outside this review.
- [Ranged acquisition](../../crates/simulation/src/ai_ranged_items.rs) orders hostile
  launch abilities by priority/ID, prefers equipped launchers, rejects inventories
  above 128 entries and inspects at most 16 launcher candidates. Complete ability
  plus weapon costs are checked against loadout maxima, including projected gear.
  Matching ammunition preserves a preferred usable launcher; depletion releases
  that preference. Saturating carried totals target a six-shot reserve. Ground
  quantities are per-candidate desires, not a simultaneous batch: the caller picks
  one action and caps it to that stack. Repeated deficits across candidate stacks
  do not alone prove overcollection, and bounded nested scans are not the earlier
  unbounded gear-ranking candidate. No focused acquisition test body was located
  in the bounded search; this does not establish that none exists. Before changing
  policy, locate or add witnesses for 128/129 inventory entries, 16/17 launchers,
  split-stack reserves, depleted preferred launchers, secondary-socket costs and
  repeated single-stack takes. These are verification scenarios, not observed
  failures. Numerical projectile compilation and whole tactical priority remain
  outside scope.

### Awareness clocks, projectile requests and skill-check arithmetic

Source review reconciled against main `99dadca9a` on 2026-11-04 established no
new actionable defect in these bounded paths. Primary files and referenced
support/test files matched the reviewed versions. Existing assertions were read,
not executed; no fresh runtime pass, profiling result or exhaustive audit follows.

- [Awareness clocks](../../crates/simulation/src/awareness.rs) reconcile expired or
  disabled alert memory without repeatedly extending its expiry. Expiry is
  exclusive; location alerts retain a snapshot, not the source's live position.
  Aggravation rephases fractional progress in wide arithmetic and rejects excess
  advancement or already-pending trials before clock mutation. Settlement takes
  pending work before running it and uses the pending-owner index. Checked
  arithmetic governs expiry and frozen-time shifts. Existing module assertions
  cover alert/source separation, exclusive expiry, retained 499/500 phase,
  single-consumption trials and shifted expiry boundaries. Sensory contests,
  hearing geometry, target disclosure, RNG distribution and surrounding candidate
  rollback were not audited in this clock-focused pass.
- [AI projectile requests](../../crates/simulation/src/ai_projectiles.rs) select
  exact item/socket/delivery operands rather than deferring weapon selection to
  execution. Fresh requests check cooldown, identified inventory and known curses
  on equipped throws; enumeration considers the first 128 rows and at most 16
  launchers with 128 ammunition rows each. These are NPC heuristics, not public
  menu completeness guarantees. Known profiles supply range, expertise and
  secondary-socket costs; shared AI helpers check full resource bundles and
  disclosed, aligned target directions. Paused work uses frozen projectile
  operands. No dedicated request-test body was inspected, so regression evidence
  here is weaker. Before changes, locate/add witnesses for cursed throws, range
  endpoints, diagonal alignment, multi-resource secondary costs, 128/129 rows,
  16/17 launchers and frozen-plan reuse. These are proposed scenarios, not failures
  or measured optimization opportunities; whole tactical priority is not qualified.
- [Attribute and skill checks](../../crates/simulation/src/attributes.rs) widen
  adjustment arithmetic before flooring/cap checks and reject ranks above five.
  Spellcasting uses its selected mana binding while Breath remains Willpower.
  Totals widen modifiers/thresholds to signed 64-bit values and use inclusive
  success, without natural-roll overrides. Diagnostics retain attribute/rank,
  edge allocation, raw/retained dice and before/after RNG state. Existing
  [attribute assertions](../../crates/simulation/tests/it/attributes.rs) cover
  adjustment preservation, caps, fixed and selected bindings, independent ranks,
  inclusive thresholds and extreme signed modifiers with natural three/eighteen.
  Dice sampling and full build-fold validation were not repeated. Similar cap
  checks at distinct semantic entry points alone do not warrant a cleanup issue.

### 3D picking correctness and camera publication

Source review reconciled against main `af1ff6046` on 2026-11-05 found no new
correctness issue in these bounded paths. Assertions were read, not executed.

- [Picking](../../crates/client-3d/src/picking.rs) checks below-cell coordinates,
  excludes unknown floor and uses coordinate tie-breaking for floor distances.
  Decorative and culling-only bounds are excluded; targets must still resolve in
  the disclosed view. Local-space intersections preserve rotation/nonuniform scale
  and return world distance, rejecting singular/nonfinite determinants. Existing
  assertions cover elevated floor, unknown space, culling-only bounds and transformed
  distance. GPU/UI behavior, exhaustive overlapping-mesh ties and extreme-coordinate
  integration remain outside scope. CH-018 concerns repeated picking, not a proven
  intersection defect.
- [Camera publication](../../crates/client-3d/src/camera.rs) shifts framing eye and
  focus together and excludes portrait cameras from exploration queries. Cave
  parameters are compared through immutable access before requesting mutable asset
  access, avoiding unconditional material modification. Existing
  [camera assertions](../../crates/client-3d/src/camera/tests.rs) cover inventory
  framing, zero material modifications across 64 unchanged updates and changes
  after focus/camera motion. No exhaustive input audit, long-duration precision
  test or measured GPU bind-group cost is claimed.

## Removal and consolidation proof

Before removing a path, field or data structure:

1. Search all references, including tests, examples, benchmarks, optional clients,
   scripts, source inclusion, generated adapters, serde names and saved artifacts.
2. Classify its role: supported API, runtime branch, oracle, diagnostic, format
   field or genuinely unreachable private code. An unused public item is not
   automatically safe to remove.
3. Establish equivalent error behavior, ordering, identity allocation, RNG draws,
   clocks, disclosure and persistence. Follow [compatibility policy](../../CONTRIBUTING.md#formats-and-compatibility)
   for any format/API decision; do not add historical compatibility as a shortcut.
4. Map every unique test witness to the retained owner. Never remove/weaken an
   assertion to make a cleanup pass. Behavior fixes require an intended first-red
   regression under [testing policy](testing.md).
5. Keep each removal batch coherent and separately reviewable from broad comment
   expansion. Record the exact before/after evidence, failures and unrun checks.
6. Claim performance improvement only after comparable measurement; smaller code
   or fewer fields alone does not establish lower cost.

The register may grow as evidence arrives, but repeated findings should reconcile
existing entries. A documentation pass is complete only for its stated coverage;
it does not imply that every logged design or performance question is resolved.
