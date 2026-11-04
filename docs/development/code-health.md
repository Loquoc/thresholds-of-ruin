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

### CH-001: Public core boundary contracts

- **Category/status:** documentation; verified gaps, foundation correction.
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
- **Acceptance:** runnable doctests, warning-free public/private rustdoc, link
  checks and semantic review against owning implementation/tests. A clean docs
  build does not prove complete API coverage.

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

### CH-004: Action-transition phase readability

- **Category/status:** maintainability; opportunity, not a proven behavior defect.
- **Evidence:** [Engine::resolve_action_transition](../../crates/server/src/engine.rs)
  joins execution, observer projection, navigation, event capture and profiling.
- **Next step:** map each phase's inputs, outputs and ordering obligations while
  documenting its callers. Extract only a clearly owned responsibility when the
  phase contract and independent tests are understood; length alone is insufficient.
- **Acceptance:** preserved publication order, atomic failure, RNG/clocks and
  event-time disclosure, with existing unique witnesses retained. Independent
  wire/save/authoring adapters are not automatically duplication to remove.

### CH-006: Rogue Search comment no longer describes checked recognition

- **Category/status:** documentation; confirmed mismatch, correction pending.
- **Evidence:** [Rogue exploration rules](../../scenarios/rogue-exploration/rules.toml)
  describe Search as guaranteed physical-contact querying, while
  [discovery](../../crates/simulation/src/discovery.rs) owns checked private
  recognition. Read the complete nearby rules overrides before correcting prose.
- **Next step/acceptance:** change the comment, not the gameplay rule. Follow the
  package identity/certificate workflow because source-text changes may change
  pinned hashes even when executable values do not. Preserve checked discovery's
  positive, failed-roll and nondisclosure witnesses.

### CH-010: Performance-trace parsing is not full fixture validation

- **Category/status:** maintainability; source-supported schema limitation, not a
  reproduced gameplay or benchmark failure. Reviewed at `3aac0cbb2` on 2026-11-04.
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
- **Next step:** if custom trace ingestion becomes supported, decide its version
  policy and whether labels are semantic expectations or reporting categories
  before adding validation. Do not silently change existing fixture semantics.
- **Acceptance:** preserve rejection of unknown labels and independent
  blocked/movement/door/readiness oracle tests; add explicit version/pairing
  rejection witnesses only for an approved strengthened input contract.

### CH-011: Compact headless frames build discarded derived state

- **Category/status:** performance/maintainability; source-supported redundant
  work, benefit unmeasured. Reviewed at `a7d0f10e1`
  on 2026-11-04; no runtime failure or speedup is claimed.
- **Impact/confidence:** high confidence in unnecessary construction; practical
  CPU/allocation impact depends on observation, history, palette and background
  message volume. This is transient work, not evidence of retained-memory growth.
- **Evidence/trigger:** [headless `emit` and `DiagnosticOutput::project`](../../crates/client-headless/src/main.rs)
  first build the complete derived JSON object, including advancement display,
  observation state, history and palette. With `--compact-background`, successful
  non-ready `map_page`, `palette` and `waiting` frames then discard those fields
  and retain only the compact envelope. Thus smaller output does not establish
  avoided full-state serialization. Accumulated diagnostic memory already skips
  construction on this path; do not report it as part of the redundant work.
- **Next step:** select compact versus complete output before constructing the
  derived fields, sharing the exact eligibility predicate and compact envelope.
  Preserve raw-message serialization, map hydration, all errors and full ready,
  observation/lifecycle and snapshot frames. Keep ordinary output unchanged.
- **Acceptance:** retain the existing unit witnesses
  `compact_background_keeps_raw_messages_and_marks_omitted_derived_state` and
  `compact_background_preserves_ready_observations_and_every_error_in_full`, and
  the actual-process witness
  [HeadlessProcesses.test_compact_background_retains_raw_pages_and_complete_gameplay_and_errors](../../scripts/test_headless_process.py).
  Add a construction-boundary witness proving that omitted derived fields are
  not evaluated on compact frames, while complete frames still evaluate them.
  Compare optimized before/after time and allocation evidence for equal disclosed
  state and raw messages before claiming a performance improvement.
- **Review limits:** existing assertions were inspected, not freshly executed.
  No implementation, test, wire schema or dependency changed in this review.
  The headless lifecycle review also found explicit unknown-outcome diagnostics,
  shared pending-request completion and bounded stale-revision-only wizard
  retries; none is established redundant or safe to remove.

### CH-012: Supported comparison workloads do not fit ledger identities

- **Category/status:** robustness/maintainability; source-supported integration
  mismatch, not yet reproduced through the CLI. Reviewed at `d7b374664` on
  2026-11-04.
- **Impact/confidence:** high confidence that accepted strike and active-mob
  measurements cannot be appended through the comparison-to-ledger path. This
  affects recording results, not their measured execution or gameplay.
- **Evidence:** [comparison workloads and result assembly](../../scripts/perf_compare.py)
  use `ordered-strikes` and `active-mobs-v1` as ledger names with integer versions.
  [entry conversion and validation](../../scripts/perf_ledger.py) normalize only
  string-valued versions; the name constraint `[a-z][a-z0-9_]*` rejects both
  names before `append_entry` opens the ledger for writing.
- **Next step:** establish a first-red actual CLI export witness for both
  identities using valid independent comparison fixtures. Choose explicit ledger
  identities satisfying the existing schema, keeping benchmark wire labels
  separate. Do not broadly relax validation or rewrite historical measurements.
- **Acceptance:** positive ledger CLI append/check for both workloads with exact
  name/version and preserved counts, timings, source commit and raw-data checksum;
  a registry-wide ledger-name contract; and preserved streaming conversion,
  invalid-workload, duplicate and non-writing failure witnesses.
- **Review limits:** existing [ledger tests](../../scripts/test_perf_ledger.py)
  cover structured performance entries and streaming string-version conversion,
  but do not establish these two comparison-to-ledger round trips. No new runtime
  regression was executed for this entry. Nonconstant count ranges are deliberately
  excluded from headline entries and are not another lost-data defect.

### CH-013: Linux storage probe loses empty column boundaries

- **Category/status:** robustness; source-supported parsing candidate, not a
  reproduced machine-probe failure. Reviewed at `d7b374664` on 2026-11-04.
- **Impact/confidence:** high confidence in the string transformation; prevalence
  remains unmeasured. A device with an unreported transport and a reported model
  can have its first model word recorded as the bus, altering descriptive storage
  data included in the machine fingerprint. No timing-comparison error is proven.
- **Evidence:** [probe_linux_machine](../../scripts/perf_ledger.py) requests
  whitespace columns `ROTA,TRAN,MODEL`, then uses `partition(" ")` and
  `rest.strip().partition(" ")`. For an illustrative row `0      Example Disk`,
  stripping the empty middle column assigns `Example` to transport and `Disk`
  to model. This is source reasoning, not captured output from an affected host.
- **Next step:** preserve empty values with structured output or separate field
  reads, retaining explicit command-failure fallbacks. A changed interpretation
  can start a new fingerprint series; preserve historical entries rather than
  incorrectly joining measurement series.
- **Acceptance:** independent subprocess-output fixtures for present/missing
  transport, multiword/missing models, partition-parent lookup, command failures
  and empty output. Assert exact storage fields and fingerprint input, not merely
  acceptance by `validate_machine`.
- **Review limits:** existing [MachineProbe tests](../../scripts/test_perf_ledger.py)
  cover CPU/memory/OS text parsers and Windows extraction, not Linux findmnt/lsblk
  command composition or column decoding. No fresh probe execution is claimed.

### CH-014: Event-time sightings measure arrival interval, not absence

- **Category/status:** behavior/maintainability; high-confidence source-supported
  mismatch, not reproduced in a running client. Reviewed at `88e935cb8` on
  2026-11-04.
- **Evidence:** [Messages](../../crates/client-ascii/src/messages.rs) documents
  `SIGHTING_TICKS` as time out of sight and `sightings` as the last visible tick.
  The active [model path](../../crates/client-ascii/src/model.rs) calls
  `absorb_outcome_with_underfoot` with the event cursor tick. That method updates
  the timestamp only for `Topic::Noticed` and ignores `Topic::LostSight`.
  [OutcomeFacts](../../crates/protocol/src/outcome_facts.rs) emit these facts on
  entry to and exit from the disclosed actor set, not during continuous
  visibility; [narration](../../crates/client-common/src/narration.rs) preserves
  their corresponding topics.
- **Predicted trigger/consequence:** notice at tick 1, remain visible until
  departure at tick 2001, return at tick 2002. The retained tick is still 1, so
  the interval exceeds 1000 and another notice appears despite only one tick
  of absence. Separate message turns avoid duplicate compression obscuring the
  result. This predicts redundant player-facing narration, not changed gameplay
  authority, undisclosed information, a crash or measured resource cost.
- **Coverage gap:**
  [a_creature_is_announced_again_only_after_a_long_absence](../../crates/client-ascii/tests/it/turns_and_map.rs)
  exercises legacy `Messages::absorb`, which refreshes timestamps from the
  visible-actor list, rather than the event-time method used by the model.
  Existing outcome tests cover other narration behavior; no fresh runtime
  regression was executed in this review.
- **Next step:** establish the intended first-red event-time regression, then
  track disclosed departure time without displaying LostSight lines. Determine
  exact threshold semantics from the absence contract; preserve actor identity,
  event ordering, arrivals and timeline reset without importing stale views.
- **Acceptance:** first encounter announces; long visibility followed by short
  absence suppresses a repeat; threshold and longer absences announce; actors
  remain independent; repeated cycles use the latest departure; reset forgets
  prior sightings; LostSight remains silent. Include a model-level witness using
  genuine Outcome facts through normal application, not only hand-built lines.
  Retain the legacy test unless a separate complete removal proof justifies it.

### CH-015: Workflow progress reconciliation depends on input-file order

- **Category/status:** robustness/maintainability; high-confidence source-supported
  reporting mismatch, not a reproduced CLI failure. Reviewed at `af2683427` on
  2026-11-04.
- **Evidence:** [offline cost report](../../scripts/ci_cost_report.py) groups
  workflow records by repository/run ID and chooses `max` by `run_attempt` alone.
  Equal-attempt snapshots therefore retain the first input's workflow status,
  conclusion and update time, even when later evidence describes completion.
  Job-level reconciliation separately fills missing completion fields independent
  of input order; that existing protection is not absent.
- **Predicted witness:** two snapshots of run 42, attempt 2, created at 00:00:00,
  with one job starting at 00:00:10 and complete all-jobs scope. The earlier run
  is in progress at 00:00:20, with no job completion; the later run succeeds at
  00:02:00, with that same job completing at 00:01:10. Both orders retain 60
  runner-seconds. Older-first retains `workflow_not_completed`, null conclusion
  and a 20-second `github_terminal_span_seconds`; newer-first selects success
  and a 120-second span without the workflow-incomplete issue.
- **Impact boundary:** identical evidence can produce different lower-bound
  flags and workflow metadata. This witness does not change summed runner time,
  actual execution, GitHub state, billing or CI qualification, and does not prove
  any committed cost baseline used affected inputs.
- **Coverage gap:** [the accounting guide](ci-cost-baseline.md) permits overlapping
  inputs and progress snapshots. The existing
  [completed-snapshot test](../../scripts/test_ci_cost_report.py) covers both
  job-input orders, but its helper gives both workflow records completed status
  and identical update timestamps. No fresh regression was run for this entry.
- **Next step:** establish a first-red workflow-progress witness, then reconcile
  snapshots explicitly within the highest attempt using validated chronology.
  Define deterministic conflict/unknown handling; a lower completed attempt must
  not supersede a higher in-progress attempt. Preserve job interval accounting
  and completeness requirements.
- **Acceptance:** reversed inputs yield identical latest workflow metadata and
  the exact 60-second runner total; higher incomplete attempts remain incomplete;
  equivalent timestamps with different time zones agree; missing/conflicting
  metadata yields a documented deterministic limitation or error rather than
  false exactness. Test repeated reversed `--input` arguments through the CLI
  and compare parsed JSON and human-readable conclusions. Preserve job completion
  merging, conflict rejection, carried-forward deduplication, skipped/canceled
  jobs, page completeness, OS breakdown and optional-stage exclusion.

### CH-016: Equipment ranking reconstructs identical candidates repeatedly

- **Category/status:** performance/maintainability; source-supported repeated
  computation with quadratic worst-case ranking structure. Practical cost and
  improvement are unmeasured. Reviewed at `d961c7caa` on 2026-11-04; no gameplay
  failure or memory leak is claimed.
- **Evidence:** ordinary [AI Search decisions](../../crates/simulation/src/ai.rs)
  call `choose_gear` when equipment policy is enabled and no visible hostile
  exposes the actor. After ranged-specific paths, [choose_gear](../../crates/simulation/src/ai_items.rs)
  collects eligible item/socket choices, then finds the first candidate not
  dominated by another choice. Both outer candidates and inner competitors are
  rebuilt with `gear_projection` and `gear_score` against the unchanged Game.
- **Cost mechanism:** each successful projection clones a shared-storage Game,
  projects the loadout with the socket removed, checks equip allocation and
  projects the replacement. Both loadout folds restore original actor inputs,
  use remembered item conditions and execute production modifier/recovery logic.
  Shared storage means a Game clone does not imply copying the whole world;
  derivation and copy-on-write costs require measurement.
- **Predicted scale witness:** strictly improving eligible same-slot choices in
  increasing score order force repeated scans until later dominating choices
  are reached. Ranking projections can grow quadratically with choice count.
  A best first choice exits earlier; not every decision has quadratic actual
  cost. Eligibility also repeats baseline work in `gear_upgrade`, a separate
  follow-up after a narrow ranking optimization is qualified.
- **Next step:** calculate each eligible choice's optional score once for
  ranking, preserving list order, first nondominated selection, unavailable-score
  handling, socket and old-item action. Retain production loadout derivation;
  neither equipment-only arithmetic nor arbitrary inventory cutoffs preserve
  the existing contract automatically.
- **Acceptance:** exact independently expected actions and unchanged original
  Game for single/increasing/equal/incomparable choices, unavailable scores,
  occupied/multiple sockets and retained ranged equipment. Cover ordinary
  `choose_ai` and subsequent equip work. A work-count witness must show at most
  one ranking projection per eligible choice, separately accounting for setup
  and eligibility. Retain existing recovery/protection/damage/timing witnesses
  in [equipment tests](../../crates/simulation/src/ai_items.rs) and ordinary
  stack-equipment/remembered-condition witnesses in
  [AI idle tests](../../crates/simulation/src/ai_idle_tests.rs).
- **Measurement gate:** compare deterministic small and larger inventories with
  the [performance workflow](performance.md) before claiming a speedup. Report
  projection/clone/fold counts separately from timings and storage bytes. No
  fresh benchmark or behavioral regression was run in this review.

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

## Retained or resolved findings

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
