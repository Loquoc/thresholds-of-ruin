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

### CH-005: Server/storage wording and operational identities

- **Category/status:** documentation; source review identifies corrections,
  owned by the server/protocol batch.
- **Evidence/next step:** inspect [Engine retry](../../crates/server/src/engine.rs)
  for accepted-versus-durable receipt language; [storage](../../crates/server/src/storage.rs)
  for worker-I/O claims versus synchronous lazy reads; [checkpoint preparation](../../crates/server/src/checkpoint.rs)
  and [persistence](../architecture/persistence.md) for chart-history capacity
  encoding; [save inspection](../../crates/server/src/storage.rs) for physical-frame
  versus save-format terminology. Verify each exact current symbol before editing.
- **Acceptance:** match the actual thread/phase and current identity owner,
  preserve format versions and persistence behavior, and validate affected docs
  and executable examples. Do not call journal admission a disk flush.

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
