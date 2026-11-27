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

CH-020 retains a focused local measurement question about dense ground-gear
eligibility. The other bounded findings have been resolved or retained with
explicit evidence and limits below. This is not a complete audit or a claim that
broader performance and scaling work is finished.

### CH-020: Dense ground-launcher eligibility cost needs representative profiling

- **Category/status:** performance; bounded measurement question, not a confirmed
  gameplay defect. Duplicate known-baseline scoring is now removed; remaining
  candidate cost and ordinary dungeon frequency remain open measurement questions.
- **Inspected baseline:** local comparison on 2026-11-15 between `fbbdfb1d`
  (pre-distance-cap-removal policy plus identical diagnostic scaffold) and
  `1b5bf52b` (local item-pursuit experiment). These are experimental revisions,
  not a claim about current main, hosted CI or a shipped release.
- **Source/trigger:** [ground-item selection](../../crates/simulation/src/ai_items.rs)
  calls `gear_destination` for items not selected by ranged acquisition.
  Socket eligibility uses `gear_upgrade`, `gear_projection` and known-loadout
  projections. The projection helpers clone private Games while preserving the
  remembered-information boundary. This is separate from CH-016's corrected
  repeated ranking-score calculation; it does not reopen that resolved defect.
- **Evidence:** the local
  [ranged acquisition workload](../../crates/simulation/examples/ranged_loot_bench.rs)
  and [comparison driver](../../scripts/perf_compare.py) ran three balanced release
  rounds, 60 valid samples per side/case and no failed runs. With 128 disclosed
  ground launchers having distinct families and no matching ammunition, complete
  warmed decision p50 at distances four/six was 9.200/9.387 ms on the baseline
  and 9.452/9.442 ms on the candidate. Both sides recorded 512 Game clones per
  decision. The source supports inspecting generic eligibility projections next;
  the total timing and clone counter do not attribute all cost to one function.
  The substantial dense-case cost is already present in the baseline. Differences
  of +2.7%/+0.6% here do not establish a distance-removal regression or causal
  speedup. Raw comparison and source manifests are retained locally, not published
  as an accepted performance-ledger result.
- **Limits/consequence:** this is an artificial, single-actor, warmed repeated
  choice in an open 16-by-9 room, with all stacks colocated and disclosed. It is
  not a representative dungeon-frequency sample, multi-actor latency measurement,
  cold-topology result or whole-game performance claim. Action and choice-purity
  assertions passed. Ranged pairing limits do not bound all generic gear or
  topology work. No gameplay-distance limit or speculative optimization follows.
- **Narrow optimization:** a local release comparison against `fe107efd` on
  2026-11-16 reused one lazy, actor-bound known-loadout baseline per ordinary
  eligibility scan. Each candidate still performs its own projection and equip
  admission; ranged acceptance and final ranking are unchanged. The unchanged
  workload's unmatched-launcher cases reduced Game clones from 4/64/512 to
  3/33/257 for 1/16/128 items. Four balanced pairs, 80 samples per case, measured
  dense complete-decision p50 reductions of about 12–19%; 16-item cases improved
  about 20–22%. Payload and Gold controls retained zero clones. These are local
  results, not an accepted performance-ledger artifact or a heap-byte claim.
  Scale assertions cover independent eligibility/ranking work and unchanged
  choices; fresh-loadout comparisons cover stronger/equal/weaker candidates in
  both orders and unchanged private state. A separate 100-AI stress workload
  disables equipment/loot, and ordinary-action controls contain no AI; their
  variable tails do not establish a whole-game speedup. Their deterministic
  work/activity and strict restart checks remain equal. Real-roster process
  traces also retain identical accepted actions, ticks, events and outcomes.
- **Early natural-exposure measurement (2026-11-17):** on local `699683be`,
  512 fresh tuned-Rogue floors used seeds 0–127 at depths 4, 10, 16 and 22.
  Only the ordinary starter's anchor changed; catalogs, population, knowledge,
  lighting and hostile behavior remained authored. AI work was drained before
  each of twelve submitted player Waits, with no final drain and a 2,000-tick
  ceiling. This is stationary-start owner exposure, not whole-floor density or
  earned-playthrough frequency. All 89 zero-decision floors, 17 death outcomes
  and 23 naturally selected treasure rooms remained in the denominator.
  The 8,425 decision boundaries involved 914 acting actor instances. At most two
  disclosed potential-equipment-class stacks (weapon/armor/ring/tool, not proven
  eligible candidates) and one disclosed shortbow were present in any sampled
  view. Only 28 floors exposed such equipment-class rows; one exposed a shortbow.
  Initial receipts contained 15 born ground shortbows across the full sample,
  illustrating why observed exposure must not be called whole-floor density.
  Projection work occurred in 96 previews from 14 actor instances on 11 floors,
  with at most three gear projections in one preview. Twenty-two positive
  previews had no disclosed ground items: counters include carried evaluation
  and ranking, and do not attribute all work to ground acquisition. Repeated
  decisions are not independent encounters. Owner observations were unchanged,
  repeated choices matched, and each preview matched its actual queued action;
  this does not prove complete private-state equality. The normalized 16-floor
  pilot prefix and all source hashes matched. Independent receipt/count review
  agreed. No wall-clock timing or speedup was measured, and this is not an
  accepted performance-ledger artifact. It does not establish a representative
  need to prioritize dense 128-launcher optimization or rule out later piles.
- **Smallest next step:** examine accumulated-playthrough exposure and small-case
  timing when a representative costly case is established. The early sparse
  sample does not close those questions. Measure complete choices and their
  constituent work without replacing independent equip admission or weakening
  the remembered-information boundary.
- **Acceptance witnesses for any later change:** compare identical workloads and
  source identities; retain useful, incompatible and secondary-slot candidates,
  known/unknown/cursed-item controls, candidate-order and exact-action equivalence,
  private-state/observer boundaries, purity and replay. Report complete-decision
  timings and work counts, including small cases and tails; do not infer heap
  savings or whole-game improvement from fewer clones alone.

## Retained or resolved findings

### CH-026: Whole-tree reference and suppression audits

- **Category/status:** maintainability; audited on branch
  `claude/code-docs-refactor-ovqedk`. No correction was needed.
- **Documentation references:** every backticked code-like name (snake_case
  with an underscore, or CamelCase) in the 97 Markdown files was matched against
  the repository's code and configuration. Of 1,024 distinct names, 40 have no
  match in code. Each is a Clippy lint name, a Rogue 5.4.4 source name in the
  research notes, a third-party crate name, a name a plan proposes, a wire code
  that serde renders in snake_case, a JSON wizard operation, a deliberately
  historical test name, or the bad half of a naming example. None is a stale
  reference to renamed code.
- **Markers and suppressions:** no `TODO`, `FIXME`, `XXX` or `HACK` remains in
  Rust or Python sources. All 15 `allow(dead_code)`/`allow(unused…)`
  attributes are in test support, benchmark examples that include shared test
  modules through `#[path]`, or 3D GPU uniform and platform-specific code.

### CH-025: Oversized dispatch and validation functions

- **Category/status:** maintainability; partly corrected on branch
  `claude/code-docs-refactor-ovqedk` (pending hosted CI and merge). No
  gameplay, wire or save change.
- **Source/trigger:** a line-count inventory of every production function in
  the main workspace and the 3D workspace. The largest mixed several
  independent concerns behind one `match` or one long sequence of checks.
- **Correction:** each split is a pure extraction. Arms and checks keep their
  order, early returns and messages. Among them:
  - `Service::apply_wizard` drops from 521 to 262 lines. Creature advancement,
    template toggling, room placement, item stock, creature spawning and rewind
    are now methods; rewind still returns before observer revalidation.
  - Intention-admission refusals are mapped by `intention_refusal`.
  - The text client's `Engine::play` drops from 343 to 114 lines.
    `direct_goal`, `resume_pending` and `read_sentence` hold the lone
    hold/magic shortcuts, answers to earlier questions and per-sentence
    interpretation.
  - `Connection::next` routes chart and map query replies through
    `route_query_reply`.
  - Earlier commits on the branch split `App::input`, `window_loop`,
    `Ruleset::validate`, `StateView::validate`, `AbilityPlan::valid`,
    scripting `invoke` and the others listed in the pull request.
- **Retained, with reasons:** eight main-workspace functions remain at 300 or
  more lines.
  - `advance_preparation` (now 408 lines) and `apply_outcome_effects` are
    ordered state machines; their checks and effects run in an order replay
    depends on. Only self-contained steps were taken out of the first:
    `isolate_equipped_unit` and `schedule_after_strike`.
  - `Ruleset::validate` (394) builds the interned catalogs, checks the
    constants, calls one validator per definition kind and derives the talent
    order, flag graph and indexes. Each stage needs the catalogs built before
    it.
  - `parse_wizard_with_origin` is one flat grammar table.
  - The creature-inspection and combat-diagnostics views are linear view
    builders.
  - `capture_selected` and `roll_planned_effect` were already narrowed and keep
    their remaining steps together.
  - The fourteen 3D functions of that size, mostly mesh and outfit
    composition, belong to the 3D geometry work, which has its own open fitting
    failures.
- **Evidence:** each change passed its crate's tests and the process tests of
  the affected binaries; see the pull request's verification section.

### CH-021: Client map memory rescanned every placement on each update

- **Category/status:** performance; confirmed per-update cost proportional to
  retained memory, corrected on branch `claude/code-docs-refactor-ovqedk`
  (pending hosted CI and merge). No correctness defect was found.
- **Source/trigger:** [map memory](../../crates/client-common/src/map_memory.rs)
  `trim` ended every `overlay` by checking each placed cell against the record
  map, even when nothing was evicted. A callgrind profile of the client
  workload attributed about 56% of all instructions to that scan.
- **Correction:** the scan now runs only after an eviction in `trim` or after a
  bulk snapshot or region install, the only paths that can leave a placement
  without its record. The invariant is documented on `MapMemory::cells`.
- **Evidence:** two new unit tests (an orphaned snapshot placement, and evicted
  placed records over the byte budget) each fail when their reconciliation is
  removed. A local balanced `perf_compare.py` run is recorded in the
  [client workload](performance.md#client-workloads): at 20,956 retained cells,
  apply p50 fell from 1.129 to 0.126 ms per update, with identical memory and
  chart counts. That is a synthetic stationary workload, not a whole-client or
  exploration measurement, and has no ledger entry yet.

### CH-023: Ruleset reference sets were rebuilt for every validation call

- **Category/status:** performance; confirmed repeated work on restore and
  derivation, corrected on branch `claude/code-docs-refactor-ovqedk` (pending
  hosted CI and merge). No gameplay or format change.
- **Source/trigger:** [`Ruleset`](../../crates/simulation/src/rules.rs)
  `validate_condition_modifiers`, `validate_effects` and `validate_strikes` each
  cloned every ability, tag, type, condition, descriptor and flag ID into fresh
  sets on every call. A callgrind profile of engine open attributed about 21%
  of its instructions to the first of these.
- **Correction:** `ReferenceSets` is derived once in `Ruleset::validate` and
  stored beside the indexes. It is a pure function of the validated document,
  so ruleset equality keeps its meaning. `References` looks equipment slots up
  through the existing index instead of a per-call map of borrowed definitions.
- **Evidence:** restart p50 fell 10–18% in the 1,000-history combat cases
  across two balanced local comparisons; see the
  [combat workload](performance.md#combat-workload-v1) for both runs and their
  unexplained client-apply shift.

### CH-024: Workspace-wide stricter-lint audit

- **Category/status:** maintainability and small-scale performance; applied on
  branch `claude/code-docs-refactor-ovqedk` (pending hosted CI and merge).
- **Method:** every non-3D workspace target was linted with sixteen extra
  Clippy lints beyond the CI set (among them `redundant_clone`,
  `needless_collect`, `format_collect`, `or_fun_call`, `set_contains_or_insert`
  and `manual_string_new`), producing 495 distinct findings.
- **Applied:** 151 of 152 redundant clones (production values now move where
  the original was dead) and the manual empty strings. Seventeen copies of
  per-byte `format!("{byte:02x}")` digest encoding, which allocated 64 strings
  per SHA-256, now use one `tor_protocol::lower_hex`, and the server's separate
  cell-key encoder delegates to it. Its exhaustive test keeps an independent
  `format!` oracle. Of the 21 eager `or`/`map_or` arguments, only one allocated
  (a status `String` in the ASCII weapon-preview path) and is now lazy; the rest
  build cheap pure values, and none consumes randomness.
- **Rejected, with reasons:** all production `needless_collect` findings. Each
  collect ends a borrow before the loop mutates its owner, or collects into a
  `BTreeSet` for deduplication and deterministic order. Two automatic fixes
  failed to compile for exactly that reason. All ten
  `set_contains_or_insert` findings are validation memos that insert a key
  only after its check succeeds; inserting first would leave a failed key
  marked valid. One `redundant_clone` suggestion in
  a test removed the copy that made a value mutable and was restored.
  `redundant_closure_for_method_calls` (160) and `trivially_copy_pass_by_ref`
  (19) are style-only and were left as written. These nursery lints are not
  added to CI because of such false positives.

### CH-022: Mirrored protocol/simulation sources had no drift guard

- **Category/status:** maintainability; confirmed byte-identical duplication,
  guarded on branch `claude/code-docs-refactor-ovqedk` (pending hosted CI and
  merge).
- **Source/trigger:** `item_identity.rs` and `item_knowledge.rs` exist in both
  `tor-protocol` and `tor-simulation`, which may not depend on each other.
  Nothing kept the copies identical.
- **Correction:** [`check_architecture.py`](../../scripts/check_architecture.py)
  `MIRRORED_SOURCES` now rejects any difference or missing copy, with first-red
  tests in `test_check_architecture.py`. Both files say they are mirrored.
- **Open question:** how to remove the duplicate is a dependency-policy decision
  for the maintainer, not part of this guard. The constraint is the
  [architecture overview](../architecture/overview.md)'s rule that protocol and
  simulation types stay independent, so a wire change cannot silently change
  simulation or save data. The 644 mirrored lines depend only on `serde` (and
  `serde_json` in their tests). Options:
  - **Keep both copies and the guard (current).** No dependency change, but
    every edit is made twice; the check catches a forgotten copy, not a wrong
    shared edit.
  - **One source file compiled into both crates.** `tor-simulation` would
    include the protocol file with `#[path = "../../protocol/src/…"]`. Each crate
    still gets its own distinct types, so the independence rule holds, and the
    server already includes test-support files this way. Cost: a file outside
    the crate directory (unpackageable, not an issue for unpublished crates),
    and the CI planner must count a change to those protocol files as a
    simulation change. The guard is then removed.
  - **A shared leaf crate (for example `tor-item-syntax`).** Both crates depend
    on it and re-export the types. This is the cleanest dependency graph, but
    protocol and simulation then share one type. A future encoding change to
    it would change wire and save formats together, which is the coupling the
    independence rule exists to prevent. It also adds a workspace member,
    architecture-map entries and planner ownership.
  - **Move them into `tor-world`.** Not recommended: `tor-protocol` would gain
    its first internal dependency, on a crate it otherwise has no reason to
    know.

  The recommended option is the single source file, if removing the copy is
  worth a planner change. It removes the duplicate without coupling the two
  formats.

### CH-019: Checkpoint representation and journal durability at the capacity bound

- **Category/status:** robustness and representation cost; resolved by
  [PR 272](https://github.com/KCall360/thresholds-of-ruin/pull/272), merged on
  2026-11-07 at `855b2dc512614e44cd0d5be889095375e67eda10` after
  [full exact-head CI](https://github.com/KCall360/thresholds-of-ruin/actions/runs/42312239517),
  both platform aggregates and Full PR qualification passed.
- **Resolution:** [checkpoint records](../../crates/simulation/src/checkpoint.rs)
  pool complete actor/AI values across retained roots, memoize unchanged capture
  sources, and validate bounded expansion before publication/restoration. Save
  format 68 is an explicitly approved compatibility break; older saves remain
  usable only with their matching older executable, not an importer.
  [Storage](../../crates/server/src/storage.rs) now commits legal journal work
  without the optional checkpoint only for classified capacity failure. Old
  checkpoint/region selection remains intact; other errors still block admission
  and require retry. This preserves CH-009's explicit failure distinction.
- **Behavior evidence:** the pre-integration repair passed 58 simulation checkpoint
  tests, 169 affected simulation unit tests, 87 server checkpoint tests, 63 focused
  integration tests and 19 actual-process checkpoint/background-save tests.
  The memoized checkpoint code passed all 58 checkpoint tests. After merging
  `a4a9a274`, all-target Clippy, 63 focused integration tests and 19 freshly built
  actual-process tests passed again. The first exhaustive run exposed remaining
  migration fixtures and nullable profiling-count handling; after correction,
  all 1,348 default-feature server tests, the exact failed streaming-benchmark
  process test and 67 report-validator tests passed locally. Nutrition and item-
  preparation fixture corrections preserve complete pooled values and COW isolation;
  the corrected simulation integration owner passed all 816 tests. A retained-frame
  native travel synchronization defect was fixed without relaxing mouse/tick
  assertions. Final hosted Windows/Linux debug/release and native lanes all passed.
  The qualification seal binds exact tree `a87f5ccacbe1e76847b4fdcec8388a3078edad93`
  to the tested main/PR parents. Earlier failed and cancelled cohorts do not supply
  this qualification. The [main validation](https://github.com/KCall360/thresholds-of-ruin/actions/runs/42319402785)
  then passed fresh builds, platform-codec tests and 2,017 selected process tests,
  plus both aggregates, under prior-exact-tree-full-plus-fresh-main assurance.
  This is fresh integration with retained exhaustive qualification, not a second
  exhaustive matrix.
- **Cost evidence:** four balanced release rounds compared `ec134e1c` with
  `c1e8dab42`. All 16 runs validated with matching work/durability counts.
  Checkpoint size and save/restart time improved; an earlier consistent interaction
  regression was no longer observed after capture memoization. Small interaction
  differences were not uniform across rounds. Raw evidence is retained locally
  pending permission to publish; this is not an accepted public-ledger entry or
  a measurement of a later integration revision.
- **Limits:** the private deployed save/scenario and native
  desktop were unavailable; synthetic workloads are not that reproduction. The
  fix cannot recover unrecorded in-memory progress from the original failure.
  This closure covers the implemented representation/durability contract and
  hosted qualification, not private-scenario reproduction or deployment. Strict
  decode/replay, rewind ownership and failure distinctions remain unchanged.

### CH-003: Retained presentation and history cost

- **Category/status:** performance; bounded ownership/measurement question
  answered by [PR 266](https://github.com/KCall360/thresholds-of-ruin/pull/266),
  merged on 2026-11-06 at `14676bd25693d30f21bb1fa06f189c4e02a28416` after
  [exhaustive CI](https://github.com/KCall360/thresholds-of-ruin/actions/runs/42153509080),
  both platform aggregates and Full PR qualification passed. No leak was established.
- **Ownership:** engine rewind retains at most 256 shared boundary roots; each
  attachment has at most 32 capabilities. The union bound is 256 + 32C for C
  attachments, not a bound on arbitrary-world heap memory. Complete chronology,
  accepted receipts and history indexes intentionally grow with history to
  preserve replay, retries and history. Old complete display views are released.
- **Evidence:** the isolated normal-release
  [diagnostic](https://github.com/KCall360/thresholds-of-ruin/actions/runs/42117066862)
  completed 24 separate-process samples: 0/100/1,000/10,000 starting records,
  one/eight modeled capability owners and three repeats per case. Every sample
  passed exact ownership/reclamation and checkpoint recovery assertions.
  After rewind expiry, one/eight owners retained 160/384 unique roots; releasing
  them left 128, with each expired root's reclamation proven by `Weak::upgrade`.
  The independent Service witness verifies eight actual attachments share one
  stationary Game root and release their own capabilities on reset/disconnect.
- **Measured costs:** warm checkpoint reopen medians grew from 10.230/12.376 ms
  at zero starting records to 142.919/146.083 ms at 10,000 for one/eight owners.
  The [performance guide](performance.md) records every case, p50/p95/max, RSS,
  bytes, machine and source identity. With n=3 per case, p95 equals maximum.
  These are head-only fixture measurements, not a before/after improvement.
- **Limits:** snapshot batch JSON is a state-size proxy, not allocated heap;
  inline root bytes exclude referenced allocations. Whole-process RSS includes
  allocator reuse and need not fall immediately when roots are reclaimed.
  Modeled owners do not measure all Service/socket state; warm reopen is not
  cold-storage startup. Ordinary CI artifacts expire after 14 days; no permanent
  raw release publication or accepted headline ledger entry is claimed.
  Broader history/world scaling and practical acceptance remain separate work.

### CH-016: Equipment ranking scores cached; timing evidence remains open

- **Category/status:** performance/maintainability; repeated ranking projection
  was corrected by [PR 256](https://github.com/KCall360/thresholds-of-ruin/pull/256)
  at `49b9d7808ebaa909f73043f70f73370d75442431`. The bounded timing question now
  has controlled release evidence in
  [PR 270](https://github.com/KCall360/thresholds-of-ruin/pull/270), merged on
  2026-11-06 at `9cdd30c50c8f74b8f4fd1708a5c144705f675327` after its
  [exhaustive CI](https://github.com/KCall360/thresholds-of-ruin/actions/runs/42114220443),
  both platform aggregates and Full PR qualification passed. General gameplay impact is not
  established; the original heading remains stable for incoming links.
- **Correction and behavior:** each eligible choice's optional score is computed
  once, preserving candidate order, unavailable-score handling, first
  nondominated selection and Equip/Unequip operands. Only scores are retained,
  not candidate Games. Eligibility and production loadout projection remain
  unchanged; dominance comparisons can still be quadratic. PR 256's final
  [full CI](https://github.com/KCall360/thresholds-of-ruin/actions/runs/41952664131)
  qualified its behavior; that is historical evidence, not a fresh run here.
- **Measurement:** three balanced interleaved rounds supplied 60 valid samples
  per side/case for 1/4/16/64 eligible carried armor choices, with no failed or
  incomplete runs. The controlled baseline restored repeated-score ranking with
  all other inputs equal, rather than replaying an entire historical tree.
  Complete-decision p50 fell from 0.070/0.447/3.399/47.365 ms to
  0.055/0.176/0.878/5.097 ms. The [performance guide](performance.md) preserves
  p95/max and source/toolchain/machine details. Four-choice maximum increased
  from 0.919 to 1.313 ms despite lower median and p95.
- **Work counts:** actual projections and dynamic folds per complete decision
  were 3→2, 21→8, 183→32 and 2271→128; Game clones were 4→3, 25→12, 199→48
  and 2335→192. These include eligibility/baseline work, not ranking alone.
  Every sample selected the independent expected action and left Game unchanged.
- **Limits:** instrumented synthetic timings do not establish inventory
  frequencies, allocation savings, linear total complexity or general gameplay
  speedup. Small-case variation is material. Permanent raw release publication
  needs separate maintainer approval and has not occurred; no accepted headline
  ledger entry is claimed. Eligibility improvements remain separate scope.

### CH-017: Portable-light fuel filtering scans unrelated items

- **Category/status:** performance; repeated eligibility work reproduced and
  bounded by [PR 267](https://github.com/KCall360/thresholds-of-ruin/pull/267),
  merged on 2026-11-06 at `c1e8dab42a560b4fc5e92449550e926ec59f44dd` after
  [exhaustive CI](https://github.com/KCall360/thresholds-of-ruin/actions/runs/42159495637),
  both platform aggregates and Full PR qualification passed. No illumination gameplay
  defect or practical slowdown magnitude was established.
- **Correction:** a derived ordered fuel-bearing candidate index in
  [ItemStore](../../crates/simulation/src/item_store.rs) replaces complete-map
  visits in expiry/burn passes. Restore rebuilds the index; insert, replacement,
  remove, extend and guarded edits maintain it, including unwinding. Retained
  clones share unchanged membership, including fuel-amount-only edits.
  Actor life, equipment, location and region eligibility remain live checks.
  The index is not serialized and is not a second lifetime authority.
- **Evidence:** the first-red hosted simulation owner reached its intended
  work-count failure (5/5 visits instead of 3/3) after positive fuel assertions.
  The retained scale witness checks 0/100/10,000 unrelated items, exact fuel
  outcomes, repeated passes, zero elapsed time and no fuel candidates. A separate
  ItemStore witness covers mutation, restore, ordering, unwind and sharing.
- **Preserved contracts:** existing portable-light owners retain paid ignition,
  expiry, frozen/thaw behavior, checkpoint restore, drop/pickup/stow, grip/death,
  movement and disclosure. Activity and eligibility are not cached; offscreen
  status does not excuse fuel work. No gameplay or format change is intended.
- **Limits:** the promise is candidate-bounded visits independent of unrelated
  rows. Actual integration iterations still depend on simulation activity.
  No measured latency, allocation, frame-rate or optical-work improvement is
  claimed; broader physics/residency optimization is outside this finding.

### CH-018: 3D loot labels repeat scene picking and discarded name preparation

- **Category/status:** performance; repeated preparation reproduced and reduced
  by [PR 268](https://github.com/KCall360/thresholds-of-ruin/pull/268), merged on
  2026-11-06 at `207c538c244d4a6ae4bf7ac16f8fcabb2cbf4e55` after
  [exhaustive core CI](https://github.com/KCall360/thresholds-of-ruin/actions/runs/42243409406),
  both aggregates, Full PR qualification and
  [all four native-3D lanes](https://github.com/KCall360/thresholds-of-ruin/actions/runs/42243409328)
  passed. This is not a gameplay bug
  or a measured frame-rate gain.
- **Correction:** [loot labels](../../crates/client-3d/src/loot_labels.rs) prepare
  disclosed terrain and eligible mesh references once per update, reusing that
  frame-local context for each label ray. Count-only mode counts non-corpse
  stacks without formatting discarded names. Empty candidates skip preparation;
  unchanged placement retains existing UI nodes.
- **Evidence:** the first-red native Linux debug owner positively established
  three visible piles, then observed 3 terrain preparations / 18 terrain visits /
  12 mesh visits / 15 discarded name formats instead of 1 / 6 / 4 / 0. These are
  work-count witnesses, not timing, GPU or allocation measurements.
- **Preserved contracts:** disclosed membership, corpse counting, name content,
  placement, focus/modal behavior and click-through remain covered. Added actual
  system witnesses vary the camera, animated foreground occlusion, hidden meshes,
  disclosure removal, focus/modal transitions and empty candidates without a new
  observation revision. Prepared references last only for that update; current
  visibility, hierarchy and animated transforms remain live inputs. Observation
  revision alone is not a cache key for occlusion.
- **Limits:** fewer preparations do not make label ray intersection constant-time
  or establish end-to-end rendering speedup. Core qualification and all four
  independent native 3D lanes remain separate merge obligations.

### CH-002: Save-deferred ingress is bounded and reclaimed

- **Category/status:** robustness; confirmed retention defect, corrected with
  bounded admission and stream-lifetime cleanup.
- **Evidence:** against `6fab91097`, the
  [first-red Linux debug run](https://github.com/KCall360/thresholds-of-ruin/actions/runs/42098728533/job/127495738606)
  retained **257 requests**, kept a disconnected stream's deferred request, and
  failed the real-WebSocket overload liveness witness. These are three behavioral
  witnesses of one deferred-ingress ownership defect, not an observed OOM or RSS
  measurement. A fourth test's positive receipt fixture failed before its intended
  assertion and is classified separately in the [test ledger](test-effectiveness.md).
- **Correction:** [runner](../../crates/server/src/runner.rs) holds at most 256
  commands/attachment commits in addition to the bounded mailbox. Overflow aborts
  the submitting stream and reclaims its deferred work. Surviving requests retain
  FIFO order; explicit and service-side disconnects release their work, and
  recovery rechecks saving before each held operation. Queries, connection mail
  and shutdown continue through the ordinary bounded mailbox passes.
- **Retry contract:** abort does not revoke previously accepted journal records.
  An exact retry can recover an existing receipt after reconnect; a fresh request
  still needs current context, and an uncommitted upload must be staged again.
  No fabricated non-commit error is sent for a possibly accepted retry. The global
  limit is not a per-client reservation or a claim of flood-source isolation.
- **Acceptance:** [focused witnesses](../../crates/server/src/runner_backpressure_tests.rs)
  cover sustained ingress, explicit/service-side reclamation, real transport,
  surviving order, accepted overflow retries, unprocessed-request non-commit,
  recovery and a second overdue interval. Existing
  [save-backpressure process](../../scripts/test_save_backpressure_process.py),
  [background-save](../../crates/server/tests/it/background_save.rs), attachment
  staging and transport/permission witnesses remain intact. No wire, save or rules
  identity changes, total-process memory bound or performance gain are claimed.

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
- **Historical certificate evidence:** the validator identified below reproduced
  the baseline certificate byte-for-byte, then validated the corrected package.
  Only authored rules and source-closure content hashes changed; effective
  rules, model, index, dependency hashes and validator identity are unchanged.
  Repository LF-source certificate checks pass. No version bump is necessary
  for this comment-only identity change.
- **Limits:** this is certificate generation and source/semantic review, not a
  fresh gameplay regression run. Checked discovery's positive, failed-roll and
  nondisclosure witnesses remain unchanged and must pass final-head hosted CI.

| Milestone | Status | Historical format change |
| --- | --- | --- |
| CH-006 Search comment correction | Completed historical certificate check | No format change; the recorded run used `tor-scenario-50`, not the current validator. |

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
  verification from this source reconciliation. The new representation/fallback
  policy work is tracked separately in [CH-019](#ch-019-checkpoint-representation-and-journal-durability-at-the-capacity-bound).

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
