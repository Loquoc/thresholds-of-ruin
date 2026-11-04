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
